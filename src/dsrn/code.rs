//! The tile passes: the tree delta, the payload, and reading them back.
//!
//! A node is a square of the quadtree. A pass works one tile size, and
//! asks of each node whether the tiles of that size inside it are
//! homogeneous:
//!
//! - all of them are: **bind**, and emit one value per tile.
//! - none of them is: **defer**, to be asked again at the next size
//!   down.
//! - some are: **skip** the node and **subdivide** it, and ask the
//!   same question of its four children at the same size.
//!
//! A pass ends when every node left is deferred, and those carry into
//! the next. A node is never smaller than the tile size, because a
//! node of exactly that size holds one tile and so either binds or
//! defers.
//!
//! Two readings of the specification are written in here rather than
//! guessed at each call, because the decoder has to make the same two:
//!
//! - **Subdivided children are taken depth first**, before the node's
//!   siblings, which is what "classical depth first tree encoding"
//!   asks for. Deferred nodes keep the order they were met in.
//! - **A bound node does not subdivide**, so within one encoding no
//!   binding nests inside another. Nesting is what a later delta does
//!   to a region already bound, which is a thing this does not do yet.
//!
//! The 1x1 pass is not here either. What the tile passes leave is
//! emitted as raw cells, so the encoding is whole and checkable while
//! the copy codes are still to come.

use crate::dsrn::Pyramid;
use crate::{BitMatrix, WIDTH};

/// The labels a node is given, two bits each.
const SKIP: u8 = 0b00;
const BIND: u8 = 0b01;
const DEFER: u8 = 0b10;
const SUBDIVIDE: u8 = 0b11;

/// The largest tile, and the smallest the tile passes reach.
const TOP: usize = 8;
const BOTTOM: usize = 1;

/// A square of the quadtree: side `1 << level`, at `(x, y)` in units
/// of that side.
#[derive(Clone, Copy)]
struct Node {
    level: usize,
    x: usize,
    y: usize,
}

/// A stream of bits, written low end first.
#[derive(Default)]
pub struct Bits {
    words: Vec<u64>,
    len: usize,
}

impl Bits {
    /// How many bits have been written.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether nothing has been written.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn clear(&mut self) {
        self.words.clear();
        self.len = 0;
    }

    fn push(&mut self, value: u64, width: usize) {
        let (at, shift) = (self.len / 64, self.len % 64);
        if at >= self.words.len() {
            self.words.push(0);
        }
        self.words[at] |= value << shift;
        if shift + width > 64 {
            self.words.push(value >> (64 - shift));
        }
        self.len += width;
    }

    fn take(&self, at: usize, width: usize) -> u64 {
        let (word, shift) = (at / 64, at % 64);
        let mask = if width == 64 { u64::MAX } else { (1u64 << width) - 1 };
        let mut got = self.words[word] >> shift;
        if shift + width > 64 {
            got |= self.words[word + 1] << (64 - shift);
        }
        got & mask
    }
}

/// What an encode produces: the tree delta, and the payload in the
/// order the nodes were bound.
#[derive(Default)]
pub struct Encoded {
    pub tree: Bits,
    pub payload: Bits,
    /// The cells the tile passes did not reach, as raw bits. Stands in
    /// for the 1x1 pass.
    pub leftover: Bits,
}

impl Encoded {
    /// Every bit of the encoding.
    pub fn bits(&self) -> usize {
        self.tree.len() + self.payload.len() + self.leftover.len()
    }

    fn clear(&mut self) {
        self.tree.clear();
        self.payload.clear();
        self.leftover.clear();
    }
}

/// The room an encode works in, found once and reused.
#[derive(Default)]
pub struct Work {
    stack: Vec<Node>,
    deferred: Vec<Node>,
    next: Vec<Node>,
}

/// Encodes the bitmap. The pyramid must already hold it.
pub fn encode(pyramid: &Pyramid, bits: &BitMatrix, work: &mut Work, out: &mut Encoded) {
    out.clear();
    work.deferred.clear();
    work.deferred.push(Node { level: TOP, x: 0, y: 0 });

    for tile in (BOTTOM..=TOP).rev() {
        work.stack.clear();
        // Depth first, so the stack is filled back to front and the
        // nodes come off it in reading order.
        for &node in work.deferred.iter().rev() {
            work.stack.push(node);
        }
        work.next.clear();

        while let Some(node) = work.stack.pop() {
            let across = 1 << (node.level - tile);
            let (tx, ty) = (node.x * across, node.y * across);
            let (all, any) = pyramid.block(tile, tx, ty, across);

            if all {
                out.tree.push(BIND as u64, 2);
                for row in ty..ty + across {
                    for col in tx..tx + across {
                        out.payload.push(u64::from(pyramid.value(tile, col, row)), 1);
                    }
                }
            } else if !any {
                out.tree.push(DEFER as u64, 2);
                work.next.push(node);
            } else {
                out.tree.push(SKIP as u64, 2);
                out.tree.push(SUBDIVIDE as u64, 2);
                for corner in [(0, 0), (1, 0), (0, 1), (1, 1)].iter().rev() {
                    work.stack.push(Node {
                        level: node.level - 1,
                        x: node.x * 2 + corner.0,
                        y: node.y * 2 + corner.1,
                    });
                }
            }
        }

        std::mem::swap(&mut work.deferred, &mut work.next);
    }

    // Whatever is still deferred after the smallest tile pass is a
    // node no size of tile described, so its cells go out raw. This is
    // where the 1x1 pass belongs.
    for node in &work.deferred {
        let side = 1 << node.level;
        for y in node.y * side..(node.y + 1) * side {
            for x in node.x * side..(node.x + 1) * side {
                out.leftover.push(u64::from(bits.get(x as u8, y as u8)), 1);
            }
        }
    }
}

/// Reads an encoding back into a bitmap, making the same two readings
/// of the specification the encoder made.
pub fn decode(out: &Encoded, work: &mut Work, bits: &mut BitMatrix) {
    bits.words.fill(0);
    work.deferred.clear();
    work.deferred.push(Node { level: TOP, x: 0, y: 0 });
    let (mut tree_at, mut payload_at) = (0usize, 0usize);

    for tile in (BOTTOM..=TOP).rev() {
        work.stack.clear();
        for &node in work.deferred.iter().rev() {
            work.stack.push(node);
        }
        work.next.clear();

        while let Some(node) = work.stack.pop() {
            let label = out.tree.take(tree_at, 2) as u8;
            tree_at += 2;
            match label {
                BIND => {
                    let across = 1 << (node.level - tile);
                    let (tx, ty) = (node.x * across, node.y * across);
                    for row in ty..ty + across {
                        for col in tx..tx + across {
                            let held = out.payload.take(payload_at, 1) != 0;
                            payload_at += 1;
                            if held {
                                let side = 1 << tile;
                                bits.set_rect(
                                    (col * side) as i64,
                                    (row * side) as i64,
                                    (col * side + side - 1) as i64,
                                    (row * side + side - 1) as i64,
                                );
                            }
                        }
                    }
                }
                DEFER => work.next.push(node),
                SKIP => {
                    // Skip is only ever written with a subdivide after
                    // it, which the encoder guarantees.
                    debug_assert_eq!(out.tree.take(tree_at, 2) as u8, SUBDIVIDE);
                    tree_at += 2;
                    for corner in [(0, 0), (1, 0), (0, 1), (1, 1)].iter().rev() {
                        work.stack.push(Node {
                            level: node.level - 1,
                            x: node.x * 2 + corner.0,
                            y: node.y * 2 + corner.1,
                        });
                    }
                }
                _ => unreachable!("a node is labelled skip, bind or defer"),
            }
        }
        std::mem::swap(&mut work.deferred, &mut work.next);
    }

    let mut raw = 0usize;
    for node in &work.deferred {
        let side = 1 << node.level;
        for y in node.y * side..(node.y + 1) * side {
            for x in node.x * side..(node.x + 1) * side {
                if out.leftover.take(raw, 1) != 0 {
                    bits.set(x as u8, y as u8);
                }
                raw += 1;
            }
        }
    }
    let _ = WIDTH;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::samples;

    /// An encoding has to come back the bitmap that went in. Nothing
    /// else about it matters if this is ever false.
    #[test]
    fn an_encoding_round_trips() {
        let (mut pyramid, mut work) = (Pyramid::new(), Work::default());
        let (mut out, mut back) = (Encoded::default(), BitMatrix::new());

        let mut cases = vec![BitMatrix::new()];
        for shape in samples::SHAPES {
            cases.extend(shape.tested());
        }
        cases.extend(samples::grown(0, 1.0, 0.0, 1));

        for bits in &cases {
            pyramid.clear();
            pyramid.rebuild(bits);
            encode(&pyramid, bits, &mut work, &mut out);
            decode(&out, &mut work, &mut back);
            for y in 0..=u8::MAX {
                for x in 0..=u8::MAX {
                    assert_eq!(bits.get(x, y), back.get(x, y), "differs at ({x}, {y})");
                }
            }
        }
    }
}
