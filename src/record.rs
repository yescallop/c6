use std::io::{self, prelude::*};

use base64::engine::DEFAULT_ENGINE;

use crate::{Board, BoardKind, Point, SetError, Stone};

fn write_var_u65(buf: &mut Vec<u8>, hi_64: u64, lo_1: u8) {
    let mut var_buf = [0; 10];
    let mut x = hi_64;
    let mut i = 0;

    let mut b = ((x & 0x3f) << 1) as u8 | lo_1;
    x >>= 6;

    while x != 0 {
        var_buf[i] = b | 0x80;
        b = (x & 0x7f) as u8;

        x >>= 7;
        i += 1;
    }
    var_buf[i] = b;
    buf.extend_from_slice(&var_buf[..=i]);
}

fn read_var_u65(buf: &mut &[u8]) -> Option<(u64, u8)> {
    if buf.is_empty() {
        return None;
    }

    let mut b = buf[0];
    let lo_1 = b & 1;

    let mut hi_64 = ((b & 0x7f) >> 1) as u64;
    let mut shifts = 6;
    let mut i = 1;

    while b & 0x80 != 0 {
        b = *buf.get(i)?;
        i += 1;

        hi_64 |= ((b & 0x7f) as u64) << shifts;

        if shifts + 7 > 64 {
            if b >= 1 << (64 - shifts) {
                return None;
            }
            break;
        }
        shifts += 7;
    }

    *buf = &buf[i..];
    Some((hi_64, lo_1))
}

const HEADER_LINE: &str = "-----BEGIN CONNECT6 RECORD-----";
const VERSION_LINE: &str = concat!(
    "Version: ",
    env!("CARGO_PKG_NAME"),
    " ",
    env!("CARGO_PKG_VERSION")
);
const TAIL_LINE: &str = "-----END CONNECT6 RECORD-----";

struct LineReader<R> {
    reader: R,
    buf: String,
}

impl<R: BufRead> LineReader<R> {
    fn new(reader: R) -> Self {
        Self {
            reader,
            buf: String::new(),
        }
    }

    fn read_line(&mut self) -> io::Result<Option<&str>> {
        self.buf.clear();
        if self.reader.read_line(&mut self.buf)? == 0 {
            return Ok(None);
        }

        if self.buf.ends_with('\n') {
            self.buf.pop();
            if self.buf.ends_with('\r') {
                self.buf.pop();
            }
        }
        Ok(Some(&self.buf[..]))
    }
}

// Stolen from OpenPGP spec:
// https://www.rfc-editor.org/rfc/rfc4880#section-6.1
fn crc24(bytes: &[u8]) -> u32 {
    const CRC24_INIT: u32 = 0xb704ce;
    const CRC24_POLY: u32 = 0x1864cfb;

    let mut crc = CRC24_INIT;
    for &b in bytes {
        crc ^= (b as u32) << 16;
        for _ in 0..8 {
            crc <<= 1;
            if crc & 0x1000000 != 0 {
                crc ^= CRC24_POLY;
            }
        }
    }
    crc & 0xffffff
}

fn parse_kind(mut s: &str) -> Option<BoardKind> {
    if s == "Infinite" {
        return Some(BoardKind::Infinite);
    }
    s = s.strip_prefix("Rect(")?.strip_suffix(')')?;
    let (x, y) = s.split_once('*')?;
    Some(BoardKind::Rect(x.parse().ok()?, y.parse().ok()?))
}

#[derive(Debug, thiserror::Error)]
pub enum LoadRecordError {
    #[error("io failure: {0}")]
    Io(#[from] io::Error),
    #[error("syntax error: {0}")]
    Syntax(&'static str),
    #[error("unable to decode base64: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("corrupted data: {0}")]
    Data(&'static str),
    #[error("unable to set on board: {0}")]
    Set(#[from] SetError),
}

impl Board {
    pub fn save_record<W: Write>(&self, mut writer: W) -> io::Result<()> {
        writeln!(writer, "{HEADER_LINE}")?;
        writeln!(writer, "{VERSION_LINE}")?;
        match self.kind {
            BoardKind::Infinite => {
                writeln!(writer, "Board: Infinite")?;
            }
            BoardKind::Rect(x, y) => {
                writeln!(writer, "Board: Rect({x}*{y})")?;
            }
        }
        writeln!(writer, "Count: {}", self.count())?;
        writeln!(writer)?;

        let mut buf = Vec::new();
        for &(point, stone) in &self.record {
            write_var_u65(&mut buf, point.index(), stone as u8);
        }

        let mut b64_buf = [0; 64];
        for chunk in buf.chunks(48) {
            let len = base64::encode_engine_slice(chunk, &mut b64_buf, &DEFAULT_ENGINE);
            writer.write_all(&b64_buf[..len])?;
            writeln!(writer)?;
        }

        // OpenPGP uses BE, so we use LE here, for a change.
        let crc = crc24(&buf).to_le_bytes();
        base64::encode_engine_slice(&crc[..3], &mut b64_buf[1..], &DEFAULT_ENGINE);
        b64_buf[0] = b'=';
        b64_buf[5] = b'\n';
        writer.write_all(&b64_buf[..6])?;

        writeln!(writer, "{TAIL_LINE}")
    }

    pub fn load_record<R: BufRead>(reader: R) -> Result<Board, LoadRecordError> {
        use LoadRecordError::*;

        let mut reader = LineReader::new(reader);

        if reader.read_line()? != Some(HEADER_LINE) {
            return Err(Syntax("expected header line"));
        }

        let mut kind = BoardKind::Infinite;
        let mut count = None;
        loop {
            let line = reader.read_line()?.ok_or(Syntax("unexpected EOF"))?;
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }

            let (key, value) = line
                .split_once(':')
                .ok_or(Syntax("expected colon in header"))?;
            let value = value.trim_start();
            match key {
                "Board" => {
                    kind = parse_kind(value).ok_or(Syntax("invalid header: Board"))?;
                }
                "Count" => match value.parse::<usize>() {
                    Ok(res) => count = Some(res),
                    Err(_) => return Err(Syntax("invalid header: Count")),
                },
                _ => {}
            }
        }

        let mut rec_buf = Vec::new();
        let mut line;
        loop {
            line = reader.read_line()?.ok_or(Syntax("unexpected EOF"))?;
            if line.starts_with('=') {
                break;
            }
            base64::decode_engine_vec(line, &mut rec_buf, &DEFAULT_ENGINE)?;
        }

        if !(line.starts_with('=') && line.len() == 5) {
            return Err(Syntax("expected checksum"));
        }
        let mut crc = [0; 4];
        base64::decode_engine_slice(&line.as_bytes()[1..5], &mut crc, &DEFAULT_ENGINE)?;
        if u32::from_le_bytes(crc) != crc24(&rec_buf) {
            return Err(Data("wrong checksum"));
        }

        let mut board = Board::new(kind);
        let mut rec_buf = &rec_buf[..];
        let mut actual_count = 0;
        while !rec_buf.is_empty() {
            let Some((point_i, stone_i)) = read_var_u65(&mut rec_buf) else {
                return Err(Data("malformed varint"));
            };

            let point = Point::from_index(point_i);
            let stone = match stone_i {
                0 => Stone::Black,
                _ => Stone::White,
            };

            board.set(point, stone)?;
            actual_count += 1;
        }

        if let Some(count) = count {
            if count != actual_count {
                return Err(Data("wrong count"));
            }
        }
        Ok(board)
    }
}
