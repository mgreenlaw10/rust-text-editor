mod editor;

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write, Seek, SeekFrom, Stdout};

use crossterm::{
    event::{
        self,
        Event,
        KeyCode,
        KeyModifiers,
        KeyEventKind
    },
    style::{
        self,
        Color
    },
    cursor,
    terminal,
    QueueableCommand
};

struct SelectWindow {
    begin: usize,
    end: usize,
}

struct State {
    opened_file: File,
    opened_file_name: String,
    char_buffer: String,
    buffer_pos: usize,
    buffer_pos_outdated: bool,
    stdout: Stdout,

    status: String,
    tab_width: usize,
    header: String,

    select_window: Option<SelectWindow>,
}

const HEADER_HEIGHT: u16 = 0;

fn main() {

    let mut args: Vec<String> =
        std::env::args()
            .skip(1)
            .collect();

    // If there are no args, run test args
    if args.is_empty() {
        args = vec! [
            String::from("test")
        ];
    }

    let file_name= match args.get_mut(0) {
        Some(str) => str,
        None => return file_name_not_present()
    };

    // Append .txt extension if there is no extension
    if !file_name.contains('.') {
        file_name.push_str(".txt");
    }

    println!("Opening file... {file_name}");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&file_name);

    match file {

        Ok(mut file) => {
            terminal::enable_raw_mode().unwrap();
            start(file, file_name.clone());
            terminal::disable_raw_mode().unwrap();
        }

        Err(error) => {
            println!("Error opening file {file_name}: {error}");
            std::process::exit(1);
        }
    }
}

fn get_header(file_name: String) -> String {
    format!(
      "=======================================================\n\
         Editing file: {file_name}                            \n\
       ======================================================="
    )
}

fn start(file: File, file_name: String) {

    let mut out = io::stdout();

    let mut state = State {
        opened_file: file,
        opened_file_name: file_name,
        stdout: out,
        char_buffer: String::with_capacity(1024),
        buffer_pos: 0,
        buffer_pos_outdated: false,
        status: String::new(),
        tab_width: 4,
        header: String::new(),
        select_window: None
    };
    state.header = get_header(state.opened_file_name);

    // Read file into buffer
    if state.opened_file.read_to_string(&mut state.char_buffer).is_err() {
        println!("Failed to read file!");
        return;
    }



    crossterm::queue! (
        state.stdout,

        cursor::MoveTo(0, 0),
        terminal::Clear(terminal::ClearType::All),
        terminal::Clear(terminal::ClearType::Purge),
        //style::Print(&state.header),

        //cursor::MoveTo(0, HEADER_HEIGHT),
        style::Print(&state.char_buffer),
    ).unwrap();

    state.stdout.flush().unwrap();

    state.buffer_pos = cursor_to_buffer_pos(get_effective_cursor_pos(), &state.char_buffer);

    loop {
        if let Event::Key(event) = event::read().unwrap() {

            if event.kind != KeyEventKind::Press {
                continue;
            }

            match event.code {

                // ctrl+s
                KeyCode::Char('s') if event.modifiers.contains(KeyModifiers::CONTROL) => {
                    save_file(&mut state.opened_file, &mut state.char_buffer);
                }

                KeyCode::Char(c) => {
                    state.char_buffer.insert(state.buffer_pos, c);
                    crossterm::queue!(state.stdout, cursor::MoveRight(1)).unwrap();
                    state.buffer_pos_outdated = true;
                }

                KeyCode::Tab => {
                    for _ in 0..state.tab_width {
                        state.char_buffer.insert(state.buffer_pos, ' ');
                    }

                    crossterm::queue!(state.stdout, cursor::MoveRight(state.tab_width as u16)).unwrap();
                    state.buffer_pos_outdated = true;
                }

                KeyCode::Backspace => {

                    let (col, row) = get_effective_cursor_pos();

                    if col == 0 {
                        // If at the beginning of the file, don't do anything
                        if row == 0 { break; }

                        let last_line = state.char_buffer
                            .lines()
                            .nth((row - 1) as usize)
                            .unwrap();

                        // If at the beginning of a line but there is a previous
                        // line to return to, move the cursor to the end of that line
                        crossterm::queue!(state.stdout, cursor::MoveTo(last_line.len() as u16, row - 1)).unwrap();
                        state.buffer_pos_outdated = true;
                    }
                    else {
                        // If not at the beginning of a line, move the cursor back one col
                        crossterm::queue!(state.stdout, cursor::MoveLeft(1)).unwrap();
                        state.buffer_pos_outdated = true;
                    }
                    state.char_buffer.remove(state.buffer_pos - 1);
                }

                KeyCode::Enter => {
                    state.char_buffer.insert(state.buffer_pos, '\n');
                    crossterm::queue!(state.stdout, cursor::MoveToNextLine(1)).unwrap();
                    state.buffer_pos_outdated = true;
                }

                KeyCode::Up => {
                    let (col, row) = get_effective_cursor_pos();

                    if row == 0 {
                        continue;
                    }

                    let last_line = state.char_buffer
                        .lines()
                        .nth((row - 1) as usize)
                        .unwrap();

                    // If the length of the last line < cursor col,
                    // jump to the end of the last line
                    if last_line.len() <= col as usize {
                        crossterm::queue!(
                            state.stdout,
                            cursor::MoveTo(last_line.len() as u16, row - 1)
                        ).unwrap();
                    }
                    else {
                        crossterm::queue!(state.stdout, cursor::MoveUp(1)).unwrap();
                    }
                    state.buffer_pos_outdated = true;
                }
                KeyCode::Down => {
                    let (col, row) = get_effective_cursor_pos();

                    if row >= (state.char_buffer.lines().count() - 1) as u16 {
                        continue;
                    }

                    let next_line = state.char_buffer
                        .lines()
                        .nth((row + 1) as usize)
                        .unwrap();

                    // If the length of the next line < cursor col,
                    // jump to the end of the next line
                    if next_line.len() <= col as usize {
                        crossterm::queue!(
                            state.stdout,
                            cursor::MoveTo(next_line.len() as u16, row + 1)
                        ).unwrap();
                    }
                    else {
                        crossterm::queue!(state.stdout, cursor::MoveDown(1)).unwrap();
                    }
                    state.buffer_pos_outdated = true;
                }
                KeyCode::Left => {
                    // If not at line beginning, move cursor left
                    if (get_effective_cursor_pos().0 > 0) {
                        crossterm::queue!(state.stdout, cursor::MoveLeft(1)).unwrap();
                        state.buffer_pos_outdated = true;
                    }
                    // Else, move to the previous line
                    else {
                        crossterm::queue!(state.stdout, cursor::MoveToPreviousLine(1)).unwrap();
                        state.buffer_pos_outdated = true;
                    }
                }
                KeyCode::Right => {
                    if let Some(c) = state.char_buffer.chars().nth(state.buffer_pos) {
                        // If not at line end, move cursor right
                        if (c != '\n') {
                            if state.select_window.is_none() {
                                state.select_window = Some(SelectWindow {
                                    begin: get_effective_cursor_pos().0 as usize,
                                    end: get_effective_cursor_pos().0 as usize
                                });
                            }
                            else {
                                state.select_window.as_mut().unwrap().end += 1;
                            }
                            crossterm::queue!(state.stdout, cursor::MoveRight(1)).unwrap();
                            state.buffer_pos_outdated = true;
                        }
                        // Else, move to the next line
                        else {
                            crossterm::queue!(state.stdout, cursor::MoveToNextLine(1)).unwrap();
                            state.buffer_pos_outdated = true;
                        }
                    }
                }

                KeyCode::Esc => {
                    // Exit program
                    crossterm::execute! (
                        state.stdout,
                        cursor::MoveTo(0, 0),
                        terminal::Clear(terminal::ClearType::All),
                        terminal::Clear(terminal::ClearType::Purge),
                    ).unwrap();
                    return;
                }

                _ => {}
            }
        }

        // Execute cursor moves
        state.stdout.flush().unwrap();

        // Update logical char pos
        if state.buffer_pos_outdated {
            // Sync buffer pos with updated cursor pos
            state.buffer_pos = cursor_to_buffer_pos(get_effective_cursor_pos(), &state.char_buffer);
            state.buffer_pos_outdated = false;
        }

        // DEBUG STATUS
        state.status = format! (
            "Buffer position: {}, Cursor position: {}, {}",
            state.buffer_pos,
            get_effective_cursor_pos().0,
            get_effective_cursor_pos().1
        );

        // Execute draws
        if let Err(e) = crossterm::queue! (
            state.stdout,

            cursor::SavePosition,

            cursor::MoveTo(0, 0),
            terminal::Clear(terminal::ClearType::All),
            terminal::Clear(terminal::ClearType::Purge),
            //style::Print(&state.header),

            //cursor::MoveTo(0, HEADER_HEIGHT),
            style::Print(&state.char_buffer),

            cursor::MoveTo(0, terminal::size().unwrap().1 - 1),
            style::Print(&state.status),

            cursor::RestorePosition
        ) { println!("{e}"); }

        state.stdout.flush().unwrap();
    }
}

fn get_effective_cursor_pos() -> (u16, u16) {
    let mut cursor_pos = cursor::position().unwrap();
    cursor_pos.1 = cursor_pos.1.saturating_sub(HEADER_HEIGHT);
    cursor_pos
}

fn queue_print_char_buffer(state: &State) {

    if let Some(select_window) = state.select_window.as_ref() {

        let (s_col, s_row) = buffer_to_cursor_pos(select_window.begin, &state.char_buffer);
        let (e_col, e_row) = buffer_to_cursor_pos(select_window.end - 1, &state.char_buffer);

        crossterm::queue! (
            io::stdout(),

            style::Print(&state.char_buffer[..select_window.begin]),

            cursor::MoveTo(s_col, s_row),
            style::SetBackgroundColor(Color::Cyan),
            style::Print(&state.char_buffer[select_window.begin..select_window.end - 1]),

            cursor::MoveTo(e_col, e_row),
            style::SetBackgroundColor(Color::Reset),
            style::Print(&state.char_buffer[select_window.end - 1..]),
        ).unwrap();
    }
    else {
        crossterm::queue! (
            io::stdout(),
            style::Print(&state.char_buffer),
        ).unwrap();
        write!(io::stdout(), "\r\n").unwrap();
    }
}

fn cursor_to_buffer_pos(cursor_pos: (u16, u16), buffer: &str) -> usize {
    let mut buffer_pos = 0usize;
    let mut lines = buffer.lines();

    for _ in 0..cursor_pos.1 {
        // Increment len by 1 because lines() removes \n
        buffer_pos += lines.next().unwrap().len() + 1;
    }
    buffer_pos + cursor_pos.0 as usize
}

fn buffer_to_cursor_pos(buffer_pos: usize, buffer: &str) -> (u16, u16) {
    let mut cursor_pos = (0u16, 0u16);
    let mut remaining_chars = buffer_pos;

    for line in buffer.lines() {
        let char_count = line.chars().count();

        if char_count > remaining_chars {
            // If there is less than a line remaining,
            // Set the col to the amount of remaining chars.
            cursor_pos.0 += remaining_chars as u16;
            break;
        }
        else {
            // If there is more than a line remaining,
            // consume the line and increment a row.
            remaining_chars -= line.chars().count();
            cursor_pos.1 += 1;
        }
    }

    cursor_pos
}

fn save_file(file: &mut File, buffer: &mut String) {
    file.set_len(0).unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();
    file.write_all(buffer.as_bytes()).unwrap();
    file.flush().unwrap();
}

fn file_name_not_present() {
    println!("Must contain a file name!");
    std::process::exit(1);
}
