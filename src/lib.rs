mod record;
pub use record::LoadRecordError;

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub fn new(x: i32, y: i32) -> Point {
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
pub enum BoardKind {
    #[default]
    Infinite,
    Rect(u32, u32),
}

impl BoardKind {
    pub fn check_bounds(self, p: Point) -> bool {
        match self {
            BoardKind::Infinite => true,
            BoardKind::Rect(x, y) => zigzag_encode(p.x) < x && zigzag_encode(p.y) < y,
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
    kind: BoardKind,
    record: Vec<(Point, Stone)>,
}

impl Board {
    pub const fn new(kind: BoardKind) -> Board {
        Board {
            board: RawBoard::new(),
            kind,
            record: Vec::new(),
        }
    }

    pub const fn new_infinite() -> Board {
        Board::new(BoardKind::Infinite)
    }

    pub const fn new_square(size: u32) -> Board {
        Board::new(BoardKind::Rect(size, size))
    }

    pub fn kind(&self) -> BoardKind {
        self.kind
    }

    pub fn count(&self) -> usize {
        self.record.len()
    }

    pub fn get(&self, point: Point) -> Option<Stone> {
        self.board.get(point)
    }

    pub fn set(&mut self, point: Point, stone: Stone) -> Result<(), SetError> {
        if !self.kind.check_bounds(point) {
            Err(SetError::OutOfBounds)
        } else if !self.board.set(point, stone) {
            Err(SetError::Occupied)
        } else {
            self.record.push((point, stone));
            Ok(())
        }
    }

    pub fn unset_last(&mut self) -> Option<(Point, Stone)> {
        let last = self.record.pop();
        if let Some((point, _)) = last {
            self.board.unset(point);
        }
        last
    }
}

impl PartialEq for Board {
    fn eq(&self, other: &Board) -> bool {
        self.kind == other.kind && self.record == other.record
    }
}

impl Eq for Board {}
