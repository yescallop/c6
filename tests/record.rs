use std::error::Error;

use c6::*;

const RECORD_EXPECTED: &[u8] = include_bytes!("record.c6");

#[test]
fn test_record_save_load() -> Result<(), Box<dyn Error>> {
    let mut board = Board::new_square(19);

    for x in -9..=9 {
        for y in -9..=9 {
            board.set(Point::new(x, y), Stone::Black)?;
        }
    }

    let mut record = Vec::new();
    board.save_record(&mut record)?;

    let board_expected = Board::load_record(RECORD_EXPECTED)?;
    assert_eq!(board, board_expected);
    assert_eq!(record, RECORD_EXPECTED);
    Ok(())
}
