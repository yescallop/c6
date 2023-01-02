use std::io::{self, prelude::*};

use base64::engine::DEFAULT_ENGINE;

use crate::{Board, BoardSpan, Point, SetError, Stone};

fn write_var_u65(vec: &mut Vec<u8>, hi_64: u64, lo_1: u8) {
    let mut buf = [0; 10];
    let mut x = hi_64;
    let mut i = 0;

    let mut b = ((x & 0x3f) << 1) as u8 | lo_1;
    x >>= 6;

    while x != 0 {
        buf[i] = b | 0x80;
        b = (x & 0x7f) as u8;

        x >>= 7;
        i += 1;
    }
    buf[i] = b;
    vec.extend_from_slice(&buf[..=i]);
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
const TAIL_LINE: &str = "-----END CONNECT6 RECORD-----";

fn read_line<'a, R: BufRead>(reader: &mut R, buf: &'a mut String) -> io::Result<&'a str> {
    buf.clear();
    reader.read_line(buf)?;
    if buf.ends_with('\n') {
        buf.pop();
        if buf.ends_with('\r') {
            buf.pop();
        }
    }
    Ok(&buf[..])
}

fn parse_span(mut s: &str) -> Option<BoardSpan> {
    if s == "Infinite" {
        return Some(BoardSpan::Infinite);
    }
    s = s.strip_prefix("Rect(")?.strip_suffix(')')?;
    let (x, y) = s.split_once('*')?;
    Some(BoardSpan::Rect(x.parse().ok()?, y.parse().ok()?))
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
        match self.span {
            BoardSpan::Infinite => {
                writeln!(writer, "Board: Infinite")?;
            }
            BoardSpan::Rect(x, y) => {
                writeln!(writer, "Board: Rect({x}*{y})")?;
            }
        }
        writeln!(writer, "Count: {}", self.count())?;
        writeln!(writer)?;

        let mut buf = Vec::new();
        for &(point, stone) in &self.record {
            write_var_u65(&mut buf, point.index(), stone as u8);
        }

        let mut out_buf = [0; 64];
        for chunk in buf.chunks(48) {
            let len = base64::encode_engine_slice(chunk, &mut out_buf, &DEFAULT_ENGINE);
            writer.write_all(&out_buf[..len])?;
            writeln!(writer)?;
        }

        writeln!(writer, "{TAIL_LINE}")
    }

    pub fn load_record<R: BufRead>(mut reader: R) -> Result<Board, LoadRecordError> {
        let mut buf = String::new();

        if read_line(&mut reader, &mut buf)? != HEADER_LINE {
            return Err(LoadRecordError::Syntax("expected header line"));
        }

        let mut span = BoardSpan::Infinite;
        let mut count = None;
        loop {
            let line = read_line(&mut reader, &mut buf)?;
            if line.is_empty() {
                break;
            }

            let (key, value) = line
                .split_once(':')
                .ok_or(LoadRecordError::Syntax("expected colon in header"))?;
            let value = value.trim();
            match key.trim() {
                "Board" => {
                    span = parse_span(value)
                        .ok_or(LoadRecordError::Syntax("invalid header: Board"))?;
                }
                "Count" => match value.parse::<usize>() {
                    Ok(res) => count = Some(res),
                    Err(_) => return Err(LoadRecordError::Syntax("invalid header: Count")),
                },
                _ => {}
            }
        }

        let mut rec_buf = Vec::new();
        loop {
            let line = read_line(&mut reader, &mut buf)?;
            if line.is_empty() || line.starts_with('-') {
                break;
            }

            base64::decode_engine_vec(line, &mut rec_buf, &DEFAULT_ENGINE)?;
        }

        let mut board = Board::new(span);
        let mut rec_buf = &rec_buf[..];
        let mut actual_count = 0;
        while !rec_buf.is_empty() {
            let Some((point_i, stone_i)) = read_var_u65(&mut rec_buf) else {
                return Err(LoadRecordError::Data("unexpected EOF"));
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
                return Err(LoadRecordError::Data("count mismatch"));
            }
        }

        Ok(board)
    }
}
