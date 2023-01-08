use c6::{Board, Point, Stone};
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    env,
    error::Error,
    fs::File,
    io::{self, BufReader, BufWriter},
};
use tui::{
    backend::{Backend, CrosstermBackend},
    buffer::Buffer,
    layout::Rect,
    style::Style,
    widgets::Widget,
    Terminal,
};

fn main() -> Result<(), Box<dyn Error>> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        // EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let res = run_app(&mut terminal);

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        // DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>) -> Result<(), Box<dyn Error>> {
    let mut board = match env::args().nth(1) {
        Some(path) => Board::load_record(BufReader::new(File::open(path)?))?,
        None => Board::new_infinite(),
    };
    let mut term_center = Point::ORIGIN;
    let mut cursor = Point::ORIGIN;
    let (mut stone, mut swap) = board.infer_turn();

    loop {
        let cursor_msg = format!("Cursor: ({}, {})", cursor.x, cursor.y);
        let turn_msg = format!(
            "{}: {} to play",
            match stone {
                Stone::Black => "Black (●)",
                Stone::White => "White (○)",
            },
            if swap { 1 } else { 2 }
        );
        terminal.draw(|f| {
            f.render_widget(
                BoardView {
                    board: &board,
                    term_center: &mut term_center,
                    cursor,
                    messages: [&turn_msg, &cursor_msg],
                },
                f.size(),
            );
        })?;

        let prev_cursor = cursor;
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Char('s') => {
                    board.save_record(BufWriter::new(File::create("save.c6")?))?;
                }
                KeyCode::Char('c') => {
                    term_center = Point::ORIGIN;
                    cursor = Point::ORIGIN;
                }
                KeyCode::Char('p') => {
                    stone = stone.opposite();
                    swap = board.is_empty();
                }
                KeyCode::Char(' ') | KeyCode::Enter => {
                    if board.set(cursor, stone).is_ok() {
                        if swap {
                            stone = stone.opposite();
                        }
                        swap = !swap;
                    }
                }
                KeyCode::Char('[') => {
                    board.unset();
                    (stone, swap) = board.infer_turn();
                }
                KeyCode::Char(']') => {
                    board.reset();
                    (stone, swap) = board.infer_turn();
                }
                KeyCode::Home => {
                    board.jump(0);
                    (stone, swap) = board.infer_turn();
                }
                KeyCode::End => {
                    board.jump(board.total_count());
                    (stone, swap) = board.infer_turn();
                }
                KeyCode::Up => cursor.y -= 1,
                KeyCode::Left => cursor.x -= 1,
                KeyCode::Down => cursor.y += 1,
                KeyCode::Right => cursor.x += 1,
                _ => (),
            }
        }
        if !board.bounds().contains(cursor) {
            cursor = prev_cursor;
        }
    }
}

struct BoardView<'a> {
    board: &'a Board,
    term_center: &'a mut Point,
    cursor: Point,
    messages: [&'a str; 2],
}

impl<'a> Widget for BoardView<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let bounds = self.board.bounds();
        let view_width = area.width / 2 + area.width % 2 - 1;
        let view_height = area.height - 2;

        let mut x_min = self.term_center.x - (view_width / 2) as i32;
        let mut x_max = x_min + view_width as i32 - 1;
        let mut y_min = self.term_center.y - (view_height / 2) as i32;
        let mut y_max = y_min + view_height as i32 - 1;

        let dx = self.cursor.x - self.cursor.x.clamp(x_min, x_max);
        self.term_center.x += dx;
        x_min += dx;
        x_max += dx;

        let dy = self.cursor.y - self.cursor.y.clamp(y_min, y_max);
        self.term_center.y += dy;
        y_min += dy;
        y_max += dy;

        for y in 0..view_height {
            for x in 0..view_width {
                let point = Point::new(x_min + x as i32, y_min + y as i32);
                let slot = self.board.get(point);
                let ch = if bounds.contains(point) {
                    match slot {
                        Some(Stone::Black) => '●',
                        Some(Stone::White) => '○',
                        None => '·',
                    }
                } else {
                    ' '
                };
                buf.get_mut(area.x + x * 2 + 1, area.y + y).set_char(ch);
            }
        }

        let mut insert_cursor = |pos: Point, left_ch, right_ch| {
            if (x_min..=x_max).contains(&pos.x) && (y_min..=y_max).contains(&pos.y) {
                let cur_x = area.x + (pos.x - x_min) as u16 * 2;
                let cur_y = area.y + (pos.y - y_min) as u16;
                buf.get_mut(cur_x, cur_y).set_char(left_ch);
                buf.get_mut(cur_x + 2, cur_y).set_char(right_ch);
            }
        };

        let record = self.board.past_record();
        if let Some(&(_, last_stone)) = record.last() {
            for &(point, stone) in record.iter().rev() {
                if stone == last_stone {
                    insert_cursor(point, '`', '`');
                } else {
                    break;
                }
            }
        }

        insert_cursor(self.cursor, '(', ')');

        for (i, message) in self.messages.iter().enumerate() {
            let colon_pos = message.chars().position(|b| b == ':').unwrap();
            let message_x = view_width / 2 * 2 - colon_pos as u16 + 1;
            buf.set_string(message_x, view_height + i as u16, message, Style::default());
        }
    }
}
