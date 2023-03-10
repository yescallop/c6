mod record;
pub use record::LoadRecordError;

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const ORIGIN: Point = Point::new(0, 0);

    pub const fn new(x: i32, y: i32) -> Point {
        Point { x, y }
    }

    pub fn index(self) -> u64 {
        let x = zigzag_encode(self.x);
        let y = zigzag_encode(self.y);
        interleave(x, y)
    }

    pub fn from_index(i: u64) -> Point {
        let (x, y) = deinterleave(i);
        Point::new(zigzag_decode(x), zigzag_decode(y))
    }
}

fn zigzag_encode(x: i32) -> u32 {
    ((x << 1) ^ (x >> 31)) as u32
}

fn zigzag_decode(x: u32) -> i32 {
    ((x >> 1) ^ (x & 1).wrapping_neg()) as i32
}

#[cfg(not(target_arch = "x86_64"))]
fn interleave(x: u32, y: u32) -> u64 {
    fn scatter_even(x: u32) -> u64 {
        let mut x = x as u64;
        x = (x | (x << 16)) & 0x0000ffff0000ffff;
        x = (x | (x << 8)) & 0x00ff00ff00ff00ff;
        x = (x | (x << 4)) & 0x0f0f0f0f0f0f0f0f;
        x = (x | (x << 2)) & 0x3333333333333333;
        x = (x | (x << 1)) & 0x5555555555555555;
        x
    }
    scatter_even(x) | (scatter_even(y) << 1)
}

#[cfg(target_arch = "x86_64")]
fn interleave(x: u32, y: u32) -> u64 {
    use std::arch::x86_64::_pdep_u64;
    unsafe {
        let even = _pdep_u64(x as u64, 0x5555555555555555);
        let odd = _pdep_u64(y as u64, 0xaaaaaaaaaaaaaaaa);
        even | odd
    }
}

#[cfg(not(target_arch = "x86_64"))]
fn deinterleave(x: u64) -> (u32, u32) {
    fn gather_even(mut x: u64) -> u32 {
        x &= 0x5555555555555555;
        x = (x | (x >> 1)) & 0x3333333333333333;
        x = (x | (x >> 2)) & 0x0f0f0f0f0f0f0f0f;
        x = (x | (x >> 4)) & 0x00ff00ff00ff00ff;
        x = (x | (x >> 8)) & 0x0000ffff0000ffff;
        (x | (x >> 16)) as u32
    }
    (gather_even(x), gather_even(x >> 1))
}

#[cfg(target_arch = "x86_64")]
fn deinterleave(i: u64) -> (u32, u32) {
    use std::arch::x86_64::_pext_u64;
    unsafe {
        let x = _pext_u64(i, 0x5555555555555555);
        let y = _pext_u64(i, 0xaaaaaaaaaaaaaaaa);
        (x as u32, y as u32)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stone {
    Black = 0,
    White = 1,
}

impl Stone {
    pub fn opposite(self) -> Stone {
        match self {
            Stone::Black => Stone::White,
            Stone::White => Stone::Black,
        }
    }
}

const CHUNK_SIZE_BITS: u32 = 4;
const CHUNK_SIZE: usize = 1 << CHUNK_SIZE_BITS;
const WORDS_PER_CHUNK: usize = CHUNK_SIZE * CHUNK_SIZE * 2 / 64;

const SLOT_INDEX_BITS: u32 = 5;
const WORD_INDEX_BITS: u32 = CHUNK_SIZE_BITS * 2 - SLOT_INDEX_BITS;

#[derive(Debug, Default)]
struct Chunk {
    words: [u64; WORDS_PER_CHUNK],
}

impl Chunk {
    fn get(&self, word_i: usize, slot_i: usize) -> Option<Stone> {
        let word = self.words[word_i];
        match (word >> (slot_i * 2)) & 3 {
            0 => None,
            1 => Some(Stone::Black),
            _ => Some(Stone::White),
        }
    }

    fn set(&mut self, word_i: usize, slot_i: usize, stone: Stone) -> bool {
        let word = self.words[word_i];
        if (word >> (slot_i * 2)) & 3 != 0 {
            return false;
        }
        self.words[word_i] = word | (1 << (slot_i * 2 + stone as usize));
        true
    }

    fn unset(&mut self, word_i: usize, slot_i: usize) -> Option<Stone> {
        let word = self.words[word_i];
        self.words[word_i] = word & !(3 << (slot_i * 2));
        match (word >> (slot_i * 2)) & 3 {
            0 => None,
            1 => Some(Stone::Black),
            _ => Some(Stone::White),
        }
    }
}

fn extract_lo_bits(i: &mut u64, bits: u32) -> u64 {
    let lo = *i & ((1 << bits) - 1);
    *i >>= bits;
    lo
}

impl Point {
    fn indexes(self) -> (u64, usize, usize) {
        let mut i = self.index();
        let slot_i = extract_lo_bits(&mut i, SLOT_INDEX_BITS);
        let word_i = extract_lo_bits(&mut i, WORD_INDEX_BITS);
        (i, word_i as usize, slot_i as usize)
    }
}

#[derive(Debug, Default)]
pub struct RawBoard {
    // Visualization:
    // 3 2 2 3
    // 1 0 0 1
    // 1 0 0 1
    // 3 2 2 3
    chunks: BTreeMap<u64, Chunk>,
}

impl RawBoard {
    pub const fn new() -> RawBoard {
        RawBoard {
            chunks: BTreeMap::new(),
        }
    }

    pub fn get(&self, point: Point) -> Option<Stone> {
        let (chunk_i, word_i, slot_i) = point.indexes();
        self.chunk(chunk_i)
            .and_then(|chunk| chunk.get(word_i, slot_i))
    }

    #[must_use]
    pub fn set(&mut self, point: Point, stone: Stone) -> bool {
        let (chunk_i, word_i, slot_i) = point.indexes();
        self.chunk_mut(chunk_i).set(word_i, slot_i, stone)
    }

    pub fn unset(&mut self, point: Point) -> Option<Stone> {
        let (chunk_i, word_i, slot_i) = point.indexes();
        self.chunk_mut(chunk_i).unset(word_i, slot_i)
    }

    fn chunk(&self, chunk_i: u64) -> Option<&Chunk> {
        self.chunks.get(&chunk_i)
    }

    fn chunk_mut(&mut self, chunk_i: u64) -> &mut Chunk {
        self.chunks.entry(chunk_i).or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Bounds {
    #[default]
    Infinite,
    Rect(u32, u32),
}

impl Bounds {
    pub fn contains(self, p: Point) -> bool {
        match self {
            Bounds::Infinite => true,
            Bounds::Rect(x, y) => zigzag_encode(p.x) < x && zigzag_encode(p.y) < y,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SetError {
    #[error("occupied")]
    Occupied,
    #[error("out of bounds")]
    OutOfBounds,
}

#[derive(Debug, Default)]
pub struct Board {
    board: RawBoard,
    bounds: Bounds,
    record: Vec<(Point, Stone)>,
    index: usize,
}

impl Board {
    pub const fn new(bounds: Bounds) -> Board {
        Board {
            board: RawBoard::new(),
            bounds,
            record: Vec::new(),
            index: 0,
        }
    }

    pub const fn new_infinite() -> Board {
        Board::new(Bounds::Infinite)
    }

    pub const fn new_square(size: u32) -> Board {
        Board::new(Bounds::Rect(size, size))
    }

    pub fn bounds(&self) -> Bounds {
        self.bounds
    }

    pub fn total_count(&self) -> usize {
        self.record.len()
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn is_empty(&self) -> bool {
        self.index == 0
    }

    pub fn get(&self, point: Point) -> Option<Stone> {
        self.board.get(point)
    }

    pub fn past_record(&self) -> &[(Point, Stone)] {
        &self.record[..self.index]
    }

    pub fn set(&mut self, point: Point, stone: Stone) -> Result<(), SetError> {
        if !self.bounds.contains(point) {
            Err(SetError::OutOfBounds)
        } else if !self.board.set(point, stone) {
            Err(SetError::Occupied)
        } else {
            self.record.truncate(self.index);
            self.record.push((point, stone));
            self.index += 1;
            Ok(())
        }
    }

    pub fn unset(&mut self) -> Option<(Point, Stone)> {
        if self.index == 0 {
            return None;
        }
        self.index -= 1;
        let last = self.record[self.index];

        self.board.unset(last.0);
        Some(last)
    }

    pub fn reset(&mut self) -> Option<(Point, Stone)> {
        if self.index >= self.record.len() {
            return None;
        }
        let next = self.record[self.index];
        self.index += 1;

        let _ = self.board.set(next.0, next.1);
        Some(next)
    }

    pub fn jump(&mut self, index: usize) {
        assert!(index <= self.record.len());
        if self.index < index {
            for i in self.index..index {
                let next = self.record[i];
                let _ = self.board.set(next.0, next.1);
            }
        } else {
            for i in (index..self.index).rev() {
                let last = self.record[i];
                self.board.unset(last.0);
            }
        }
        self.index = index;
    }

    pub fn infer_turn(&self) -> (Stone, bool) {
        if self.index == 0 {
            return (Stone::Black, true);
        }

        let last = self.record[self.index - 1].1;
        if self.index == 1 {
            return (Stone::White, last == Stone::White);
        }

        let last_prev = self.record[self.index - 2].1;
        if last == last_prev {
            (last.opposite(), false)
        } else {
            (last, true)
        }
    }
}

impl PartialEq for Board {
    fn eq(&self, other: &Board) -> bool {
        self.bounds == other.bounds && self.record[..self.index] == other.record[..other.index]
    }
}

impl Eq for Board {}
