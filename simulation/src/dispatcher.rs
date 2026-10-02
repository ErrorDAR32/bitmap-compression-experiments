//! The thread dispatcher: worker threads started once and kept, parked
//! between jobs, so a tick pays no thread's start -- and a job run on
//! all of them at once, the caller's thread doing the first part.
//!
//! A job borrows what the caller holds, for no longer than [`run`]
//! takes: `run` hands its workers the job, does part 0 itself, and does
//! not return -- not even when a part panics -- until every worker has
//! finished its part. That is what lets a borrowed job be handed to
//! threads that outlive it, and the one `unsafe` here rests on it.
//!
//! [`run`]: Dispatcher::run

use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

/// A job: run once a part, given the part's number.
type Job = dyn Fn(usize) + Sync;

/// A job, handed to the workers for no longer than [`Dispatcher::run`]
/// waits on them.
#[derive(Clone, Copy)]
struct JobPointer(*const Job);

// Safety: the job is `Sync`, so its parts may run on any thread; the
// pointer is only followed while `Dispatcher::run` waits.
unsafe impl Send for JobPointer {}

/// What the dispatcher and its workers share, behind the lock.
struct State {
    /// The job running, if any.
    job: Option<JobPointer>,
    /// Counts the jobs handed out: a worker runs each new one once.
    generation: u64,
    /// Workers still running their part of the job.
    running: usize,
    /// Whether a worker's part panicked.
    panicked: bool,
    /// Whether the workers are to stop.
    quit: bool,
}

/// The lock and the two waits on it.
struct Shared {
    /// The state.
    state: Mutex<State>,
    /// Signalled when a job is handed out, or the workers are to stop.
    job_handed_out: Condvar,
    /// Signalled when the last worker finishes its part.
    job_done: Condvar,
}

/// Worker threads, kept between jobs.
pub struct Dispatcher {
    /// What the dispatcher and the workers share.
    shared: Arc<Shared>,
    /// The workers: one fewer than the threads, the caller's being one.
    workers: Vec<JoinHandle<()>>,
}

impl Dispatcher {
    /// A dispatcher of `threads` threads: this one, and `threads - 1`
    /// workers started now and kept. At least one.
    pub fn new(threads: usize) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State { job: None, generation: 0, running: 0, panicked: false, quit: false }),
            job_handed_out: Condvar::new(),
            job_done: Condvar::new(),
        });
        let workers = (1..threads.max(1))
            .map(|part| {
                let shared = Arc::clone(&shared);
                std::thread::spawn(move || work(&shared, part))
            })
            .collect();
        Self { shared, workers }
    }

    /// How many threads run a job: the parts it is split into.
    pub fn threads(&self) -> usize {
        self.workers.len() + 1
    }

    /// Runs `job` once a part, part 0 on this thread and each other on a
    /// worker, and returns once every part has: a part's panic is raised
    /// here, after.
    pub fn run(&self, job: &(dyn Fn(usize) + Sync)) {
        if self.workers.is_empty() {
            job(0);
            return;
        }
        // Safety: the lifetime is erased only for the workers to hold the
        // job while this call waits for them: it returns after the last
        // has finished with it, so the job outlives every use.
        let erased: *const Job = unsafe { std::mem::transmute::<*const (dyn Fn(usize) + Sync + '_), *const Job>(job) };
        {
            let mut state = self.shared.state.lock().expect("the dispatcher's lock");
            state.job = Some(JobPointer(erased));
            state.generation += 1;
            state.running = self.workers.len();
            state.panicked = false;
        }
        self.shared.job_handed_out.notify_all();
        let own = catch_unwind(AssertUnwindSafe(|| job(0)));
        let mut state = self.shared.state.lock().expect("the dispatcher's lock");
        while state.running > 0 {
            state = self.shared.job_done.wait(state).expect("the dispatcher's lock");
        }
        state.job = None;
        let panicked = state.panicked;
        drop(state);
        if let Err(panic) = own {
            resume_unwind(panic);
        }
        assert!(!panicked, "a part of the job panicked on a worker");
    }
}

/// A worker's life: waits for each job, runs its part, says so.
fn work(shared: &Shared, part: usize) {
    let mut seen = 0;
    loop {
        let job = {
            let mut state = shared.state.lock().expect("the dispatcher's lock");
            while state.generation == seen && !state.quit {
                state = shared.job_handed_out.wait(state).expect("the dispatcher's lock");
            }
            if state.quit {
                return;
            }
            seen = state.generation;
            state.job.expect("a job handed out")
        };
        // Safety: `Dispatcher::run` holds the job until this part is done.
        let result = catch_unwind(AssertUnwindSafe(|| unsafe { (*job.0)(part) }));
        let mut state = shared.state.lock().expect("the dispatcher's lock");
        state.panicked |= result.is_err();
        state.running -= 1;
        if state.running == 0 {
            shared.job_done.notify_one();
        }
    }
}

impl Drop for Dispatcher {
    /// Stops the workers and waits for them.
    fn drop(&mut self) {
        self.shared.state.lock().expect("the dispatcher's lock").quit = true;
        self.shared.job_handed_out.notify_all();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}
