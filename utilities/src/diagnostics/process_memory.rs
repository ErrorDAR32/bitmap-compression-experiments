//! The process's memory, as the system counts it: what it holds in
//! physical memory now, and the most it has held. Read from
//! `/proc/self/status`, so only on Linux; elsewhere nothing is known.
//!
//! [`MemoryTrack`] reads it again and again over a run, for the peak
//! and the average.

/// What the process holds in physical memory, in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Memory {
    /// Held now (`VmRSS`).
    pub resident: u64,
    /// The most held since the process started (`VmHWM`).
    pub peak: u64,
}

/// The process's memory now, if the system says.
pub fn process_memory() -> Option<Memory> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let field = |name: &str| {
        let line = status.lines().find(|line| line.starts_with(name))?;
        let kibibytes: u64 = line[name.len()..].trim().trim_end_matches("kB").trim().parse().ok()?;
        Some(kibibytes * 1024)
    };
    Some(Memory { resident: field("VmRSS:")?, peak: field("VmHWM:")? })
}

/// The process's memory read again and again over a run: how many
/// readings, their sum, and the system's peak at the last one.
#[derive(Clone, Copy, Debug, Default)]
pub struct MemoryTrack {
    /// Readings taken.
    readings: u64,
    /// Their resident bytes, added up.
    resident_sum: u64,
    /// The peak at the last reading.
    peak: u64,
}

impl MemoryTrack {
    /// Reads the process's memory now, if the system says.
    pub fn read(&mut self) {
        if let Some(memory) = process_memory() {
            self.readings += 1;
            self.resident_sum += memory.resident;
            self.peak = memory.peak;
        }
    }

    /// The average resident bytes over the readings, if any were taken.
    pub fn average(&self) -> Option<u64> {
        (self.readings > 0).then(|| self.resident_sum / self.readings)
    }

    /// The most the process has held, at the last reading, if any.
    pub fn peak(&self) -> Option<u64> {
        (self.readings > 0).then_some(self.peak)
    }
}

/// `bytes` in mebibytes, to one decimal: how a report shows memory.
pub fn mebibytes(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / (1u64 << 20) as f64)
}
