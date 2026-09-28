//! PNG images of the bitmaps looked at, set cells black, two pixels a
//! cell, written to `target/gct_diagnostics/`: a PNG is a zlib stream,
//! so this writes one uncompressed, with no library.

use super::bitmaps::looked_at;
use bitmap::Bitmap;
use std::fs;
use std::path::PathBuf;

/// Where the images go, under the crate's root.
const FOLDER: &str = "target/gct_diagnostics";
/// Each cell is this many pixels square.
const PIXELS_A_CELL: usize = 2;
/// The bitmap's side, in cells.
const SIDE: usize = 256;
/// A set cell's grey level.
const BLACK: u8 = 0;
/// A clear cell's grey level.
const WHITE: u8 = 255;

/// The PNG format's fixed values: the file signature...
const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
/// ...8 bits a pixel...
const BIT_DEPTH: u8 = 8;
/// ...the greyscale colour type...
const GREYSCALE: u8 = 0;
/// ...deflate, the only compression method...
const DEFLATE: u8 = 0;
/// ...adaptive filtering, the only filter method...
const ADAPTIVE_FILTERING: u8 = 0;
/// ...no interlacing...
const NO_INTERLACE: u8 = 0;
/// ...and each row's filter type: none.
const NO_FILTER: u8 = 0;
/// CRC-32's reversed polynomial, as PNG uses it.
const CRC_POLYNOMIAL: u32 = 0xEDB8_8320;
/// A zlib header: deflate with a 32 KiB window, no dictionary, lowest
/// compression level -- stored blocks.
const ZLIB_HEADER: [u8; 2] = [0x78, 0x01];
/// Adler-32's modulus, the largest prime under 2^16.
const ADLER_MODULUS: u32 = 65521;
/// The most one stored deflate block holds.
const STORED_BLOCK: usize = u16::MAX as usize;

/// The CRC-32 PNG chunks carry.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ CRC_POLYNOMIAL } else { crc >> 1 };
        }
    }
    !crc
}

/// Adler-32, the checksum ending a zlib stream.
fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in bytes {
        a = (a + byte as u32) % ADLER_MODULUS;
        b = (b + a) % ADLER_MODULUS;
    }
    (b << 16) | a
}

/// Appends one PNG chunk to `out`: its length, `kind`, `data`, and the
/// CRC of the last two.
fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend(data);
    out.extend(&body);
    out.extend(crc32(&body).to_be_bytes());
}

/// A zlib stream of stored (uncompressed) blocks.
fn stored_zlib(data: &[u8]) -> Vec<u8> {
    let mut out = ZLIB_HEADER.to_vec();
    let blocks: Vec<&[u8]> = data.chunks(STORED_BLOCK).collect();
    for (at, block) in blocks.iter().enumerate() {
        out.push(u8::from(at + 1 == blocks.len()));
        out.extend((block.len() as u16).to_le_bytes());
        out.extend((!(block.len() as u16)).to_le_bytes());
        out.extend(*block);
    }
    out.extend(adler32(data).to_be_bytes());
    out
}

/// `bitmap` as an 8-bit greyscale PNG.
pub fn png(bitmap: &Bitmap) -> Vec<u8> {
    let side = SIDE * PIXELS_A_CELL;
    let mut rows = Vec::with_capacity(side * (side + 1));
    for y in 0..side {
        rows.push(NO_FILTER);
        for x in 0..side {
            let set = bitmap.get((x / PIXELS_A_CELL) as u8, (y / PIXELS_A_CELL) as u8);
            rows.push(if set { BLACK } else { WHITE });
        }
    }
    let mut out = SIGNATURE.to_vec();
    let mut header = (side as u32).to_be_bytes().to_vec();
    header.extend((side as u32).to_be_bytes());
    header.extend([BIT_DEPTH, GREYSCALE, DEFLATE, ADAPTIVE_FILTERING, NO_INTERLACE]);
    chunk(&mut out, b"IHDR", &header);
    chunk(&mut out, b"IDAT", &stored_zlib(&rows));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Writes a PNG of every bitmap looked at, and prints where.
#[test]
#[ignore]
fn render() {
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FOLDER);
    fs::create_dir_all(&folder).unwrap();
    for (name, bitmap) in looked_at() {
        let path = folder.join(format!("{name}.png"));
        fs::write(&path, png(&bitmap)).unwrap();
        println!("  {}", path.display());
    }
}
