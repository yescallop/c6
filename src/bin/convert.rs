use std::{
    error::Error,
    fs::{self, File},
    io::{BufRead, BufReader, BufWriter},
    path::Path,
};

use c6::*;

fn parse_point(s: &str) -> Point {
    let x = (s.as_bytes()[0] - b'A') as i32 - 9;
    let y = s[1..].parse::<i32>().unwrap() - 1 - 9;
    Point { x, y }
}

fn main() -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir("../connect6/records")? {
        let entry = entry?;
        let file = File::open(entry.path())?;

        let mut board = Board::new_square(19);
        board.set(Point::ORIGIN, Stone::Black)?;

        for line in BufReader::new(file).lines() {
            let line = line?;
            if !line.contains("moved") {
                continue;
            }
            let stone = if line.contains("Black") {
                Stone::Black
            } else {
                Stone::White
            };
            let s = line.split_once('(').unwrap().1;
            let s = s.strip_suffix(')').unwrap();
            let (a, b) = s.split_once(", ").unwrap();
            let (a, b) = (parse_point(a), parse_point(b));

            board.set(a, stone)?;
            board.set(b, stone)?;
        }

        let path = Path::new("records")
            .join(entry.file_name())
            .with_extension("c6");
        let bw = BufWriter::new(File::create(path)?);
        board.save_record(bw)?;
    }

    Ok(())
}
