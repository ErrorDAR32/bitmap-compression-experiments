//! A bitmap as a PNG image, set cells black, two pixels a cell: a PNG is
//! a zlib stream, so this writes one uncompressed, with no library.

use crate::Bitmap;

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
    let (mut sum_of_bytes, mut sum_of_sums) = (1u32, 0u32);
    for &byte in bytes {
        sum_of_bytes = (sum_of_bytes + byte as u32) % ADLER_MODULUS;
        sum_of_sums = (sum_of_sums + sum_of_bytes) % ADLER_MODULUS;
    }
    (sum_of_sums << 16) | sum_of_bytes
}

/// Appends one PNG chunk to `bytes`: its length, `kind`, `data`, and the
/// CRC of the last two.
fn chunk(bytes: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    bytes.extend((data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend(data);
    bytes.extend(&body);
    bytes.extend(crc32(&body).to_be_bytes());
}

/// A zlib stream of stored (uncompressed) blocks.
fn stored_zlib(data: &[u8]) -> Vec<u8> {
    let mut bytes = ZLIB_HEADER.to_vec();
    let blocks: Vec<&[u8]> = data.chunks(STORED_BLOCK).collect();
    for (block_index, block) in blocks.iter().enumerate() {
        bytes.push(u8::from(block_index + 1 == blocks.len()));
        bytes.extend((block.len() as u16).to_le_bytes());
        bytes.extend((!(block.len() as u16)).to_le_bytes());
        bytes.extend(*block);
    }
    bytes.extend(adler32(data).to_be_bytes());
    bytes
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
    let mut bytes = SIGNATURE.to_vec();
    let mut header = (side as u32).to_be_bytes().to_vec();
    header.extend((side as u32).to_be_bytes());
    header.extend([BIT_DEPTH, GREYSCALE, DEFLATE, ADAPTIVE_FILTERING, NO_INTERLACE]);
    chunk(&mut bytes, b"IHDR", &header);
    chunk(&mut bytes, b"IDAT", &stored_zlib(&rows));
    chunk(&mut bytes, b"IEND", &[]);
    bytes
}
