mod editor;
mod terminal_controller;

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write, Seek, SeekFrom, Stdout};
use editor::Editor;

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
            let mut editor = Editor::new(file, file_name.clone());
            start_loop(&mut editor);
            terminal::disable_raw_mode().unwrap();
        }

        Err(error) => {
            println!("Error opening file {file_name}: {error}");
            std::process::exit(1);
        }
    }
}
fn start_loop(editor: &mut Editor) {

    editor.redraw();

    loop {
        if let Event::Key(event) = event::read().unwrap() {

            if event.kind != KeyEventKind::Press {
                continue;
            }

            match event.code {

                KeyCode::Char('s') if event.modifiers.contains(KeyModifiers::CONTROL) => {
                    match editor.save_file() {
                        Ok(_) => {
                            editor.log(String::from("File saved!"));
                        },
                        Err(error) => {
                            editor.log(format!("Error saving file: {error}"));
                        }
                    }

                }
                KeyCode::Char(c) => {
                    editor.insert_char(c);
                    editor.move_cols(1);
                    editor.close_select_window();
                }
                KeyCode::Backspace => {
                    if editor.move_cols(-1) > 0 {
                        editor.delete_char();
                    }
                    editor.close_select_window();
                }
                KeyCode::Enter => {
                    editor.insert_char('\n');
                    editor.move_cols(1);
                }
                KeyCode::Tab => {
                    //todo
                    // editor.insert_char('\t');
                    // editor.move_cols(1);
                }
                KeyCode::Up => {
                    editor.move_rows(-1);
                    log_positions(editor);
                }
                KeyCode::Down => {
                    editor.move_rows(1);
                    log_positions(editor);
                }
                KeyCode::Left => {
                    if event.modifiers.contains(KeyModifiers::SHIFT) {
                        editor.drag_cols(-1);
                    }
                    else if editor.move_cols(-1) == 0 {
                        editor.move_next_line().expect("");
                    }
                    else {
                        editor.close_select_window();
                    }
                    log_positions(editor);
                }
                KeyCode::Right => {
                    if event.modifiers.contains(KeyModifiers::SHIFT) {
                        editor.drag_cols(1);
                    }
                    else if editor.move_cols(1) == 0 {
                        editor.move_next_line().expect("");
                    }
                    else {
                        editor.close_select_window();
                    }
                    log_positions(editor);
                }
                KeyCode::Esc => {
                    // Exit program
                    crossterm::execute! (
                        io::stdout(),
                        cursor::MoveTo(0, 0),
                        terminal::Clear(terminal::ClearType::All),
                        terminal::Clear(terminal::ClearType::Purge),
                    ).unwrap();
                    return;
                }

                _ => {}
            }
            editor.redraw();
        }
    }
}

fn log_positions(editor: &mut Editor) {
    editor.log(format! (
        "Cursor position: ({}, {}) | Buffer position: {}",
        editor.cursor_pos().0,
        editor.cursor_pos().1,
        editor.buffer_pos()
    ));
}

fn file_name_not_present() {
    println!("Must contain a file name!");
    std::process::exit(1);
}
