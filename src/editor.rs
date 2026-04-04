use std::fs::File;
use std::io::{self, Read, Write, Stdout, SeekFrom, Seek};
use std::mem::swap;
use std::ops::Add;
use crossterm::{style::{
    Print,
    Color,
    Attribute,
    SetAttribute,
    SetBackgroundColor,
    ResetColor,
}, cursor::{
    MoveTo
}, terminal::{
    Clear,
    ClearType
}, QueueableCommand, terminal};

use crate::snapshot_controller::SnapshotController;

pub struct StatusLog {
    log: Vec<String>,
    lines: u16
}

impl StatusLog {
    pub fn new(lines: u16) -> StatusLog {
        StatusLog { log: Vec::new(), lines }
    }

    pub fn print(&mut self, message: &str) {

        if self.log.len() >= self.lines as usize - 1 {
            self.log.remove(0);
        }
        self.log.push(String::from(message));
    }
}

pub struct Editor {
    file: File,
    file_name: String,
    stdout: Stdout,

    data_buffer: String,
    select_window: Option<SelectWindow>,
    buffer_pos: usize,
    cursor_pos: (u16, u16),

    header: Option<String>,

    status_log: StatusLog,

    file_state_controller: SnapshotController,

    // number of rows scrolled from top
    row_offset: usize
}

#[derive(Clone, Copy)]
struct SelectWindow {
    origin: usize,
    pivot: usize,
}

impl SelectWindow {
    pub fn get_left(&self) -> usize {
        std::cmp::min(self.origin, self.pivot)
    }

    pub fn get_right(&self) -> usize {
        std::cmp::max(self.origin, self.pivot)
    }
}
impl Editor {

    pub fn new(mut file: File, file_name: String) -> Self {

        // Load file into buffer
        let mut data_buffer = String::new();
        
        file.read_to_string(&mut data_buffer)
            .expect("Failed to read data from {file_name}!");
        let state = data_buffer.clone();

        // Todo: cache buffer position and load it here
        let buffer_pos = 0;

        Editor {
            file,
            file_name,
            stdout: io::stdout(),
            data_buffer,
            select_window: None,
            buffer_pos,
            cursor_pos: (0, 0),
            header: None,
            status_log: StatusLog::new(5),
            file_state_controller: SnapshotController::new(state, (0, 0)),
            row_offset: 0
        }
    }

    pub fn insert_char(&mut self, c: char) {
        if self.select_window.is_some() {
            let window = self.select_window.unwrap();
            self.data_buffer.drain(window.get_left()..window.get_right());
            self.buffer_pos = window.get_left();
            self.data_buffer.insert(self.buffer_pos, c);
        }
        else {
            self.data_buffer.insert(self.buffer_pos, c);
        }
        self.buffer_pos = self.buffer_pos.saturating_add(1);
        self.buffer_pos = self.buffer_pos.clamp(0, self.data_buffer.len());

        self.update_cursor_position();
        self.save_snapshot();
    }

    pub fn delete_char(&mut self) {
        if self.select_window.is_none() {
            self.buffer_pos = self.buffer_pos.saturating_sub(1);
            self.data_buffer.remove(self.buffer_pos);
        }
        else {
            let window = self.select_window.unwrap();
            self.data_buffer.drain(window.get_left()..window.get_right());
            self.buffer_pos = window.get_left();

        }
        self.update_cursor_position();
        self.save_snapshot();
    }

    pub fn save_snapshot(&mut self) {
        self.file_state_controller.push_snapshot(self.data_buffer.clone(), self.cursor_pos());
    }

    pub fn undo(&mut self) {
        self.file_state_controller.move_pointer(-1);
        self.data_buffer = self.file_state_controller.get_current_snapshot().data;
        self.cursor_pos = self.file_state_controller.get_current_snapshot().cursor_pos;
        self.update_buffer_position();
    }

    pub fn redo(&mut self) {
        self.file_state_controller.move_pointer(1);
        self.data_buffer = self.file_state_controller.get_current_snapshot().data;
        self.cursor_pos = self.file_state_controller.get_current_snapshot().cursor_pos;
        self.update_buffer_position();
    }

    // Will clamp to the current row.
    // Return the amount of cols actually moved.
    pub fn move_cols(&mut self, num_cols: isize) -> usize {

        let original_pos = self.buffer_pos as isize;

        self.buffer_pos = self.buffer_pos.saturating_add_signed(num_cols);
        self.buffer_pos = self.buffer_pos.clamp(0, self.data_buffer.chars().count());
        self.update_cursor_position();

        (original_pos - self.buffer_pos as isize).abs() as usize
    }

    // Will clamp to the current col.
    // Return the amount of rows actually moved.
    pub fn move_rows(&mut self, num_rows: i16) -> usize {

        let (mut col, mut row) = self.cursor_pos;
        let original_pos = self.buffer_pos as isize;

        let target_row = row.saturating_add_signed(num_rows);
        let final_row = self.data_buffer.lines().count() as u16 - 1;

        row = target_row.clamp(0, final_row);
        
        // Clamp the cursor col at the length of the destination row
        if let Some(row_len) = self.get_row_length(row) {
            col = std::cmp::min(col, row_len as u16);
        }

        self.cursor_pos = (col, row);
        self.update_buffer_position();

        (original_pos - self.buffer_pos as isize).abs() as usize
    }

    pub fn move_next_line(&mut self) -> Result<(), ()>{
        let mut ptr = self.buffer_pos;
        loop {
            if let Some(c) = self.data_buffer.chars().nth(ptr) {
                if c == '\n' {
                    self.buffer_pos = ptr + 1;
                    self.update_cursor_position();
                    return Ok(());
                }
                else {
                    ptr += 1;
                }
            }
            else {
                // If at end of file, append a new line
                self.insert_char('\n');
                self.buffer_pos = ptr + 1;
                self.update_cursor_position();
                return Ok(());
            }
        }
    }

    pub fn drag_cols(&mut self, num_cols: isize) -> usize {
        let original_pos = self.buffer_pos;
        let cols_moved = self.move_cols(num_cols);

        let origin: usize;
        let pivot: usize;

        if self.select_window_active() {
            let window = self.select_window.unwrap();
            origin = window.origin;
            pivot = self.buffer_pos;
        }
        else {
            origin = original_pos;
            pivot = self.buffer_pos;
        }

        self.select_window = Some(SelectWindow { origin, pivot });

        cols_moved
    }

    pub fn drag_rows(&mut self, num_rows: i16) -> usize {
        let original_pos = self.buffer_pos;
        let rows_moved = self.move_rows(num_rows);

        let origin: usize;
        let pivot: usize;

        if self.select_window_active() {
            let window = self.select_window.unwrap();
            origin = window.origin;
            pivot = self.buffer_pos;
        }
        else {
            origin = original_pos;
            pivot = self.buffer_pos;
        }

        self.select_window = Some(SelectWindow { origin, pivot });

        rows_moved
    }

    pub fn save_file(&mut self) -> Result<(), Box<dyn std::error::Error>> {

        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(self.data_buffer.as_bytes())?;
        self.file.flush()?;

        Ok(())
    }

    // Drag to the end of the word under the cursor
    pub fn snap_drag_right(&mut self) {
        if let Some(next_space_pos) = self.get_next_char_pos(' ') {
            let distance = next_space_pos as isize - self.buffer_pos as isize;
            self.drag_cols(distance);
        }
        else {
            self.drag_cols(isize::MAX);
        }
    }

    pub fn snap_drag_left(&mut self) {
        if let Some(last_space_pos) = self.get_last_char_pos(' ') {
            let distance = last_space_pos as isize - self.buffer_pos as isize;
            self.drag_cols(distance);
        }
    }

    pub fn get_next_char_pos(&mut self, c: char) -> Option<usize> {
        if let Some(pos) = self.data_buffer[self.buffer_pos..]
            .chars()
            .position(|fc| fc == c) {
            return Some(pos + self.buffer_pos);
        }
        None
    }

    pub fn get_last_char_pos(&mut self, c: char) -> Option<usize> {
        self.data_buffer[..self.buffer_pos]
            .chars()
            .position(|fc| fc == c)
    }

    pub fn page_down(&mut self) {
        self.row_offset += terminal::size().unwrap().1 as usize;
        self.row_offset = self.row_offset.clamp(0, self.data_buffer.lines().count());
        self.update_buffer_position();
    }

    pub fn page_up(&mut self) {
        self.row_offset = self.row_offset.saturating_sub(terminal::size().unwrap().1 as usize);
        self.row_offset = self.row_offset.clamp(0, self.data_buffer.lines().count());
        self.update_buffer_position();
    }

    pub fn select_window_active(&mut self) -> bool {
        self.select_window.is_some()
    }

    pub fn close_select_window(&mut self) {
        self.select_window = None;
    }

    pub fn log(&mut self, message: String) {
        self.status_log.print(&message);
    }

    pub fn redraw(&mut self) {

        self.queue_clear_screen();
        self.queue_write_data_buffer();
        //self.queue_write_status();
        self.queue_write_status_log();

        self.stdout.flush()
            .expect("Failed to flush stdout!");

        // Restore cursor to its place
        crossterm::execute!(self.stdout, MoveTo(self.cursor_pos.0, self.cursor_pos.1))
            .expect("Failed to move to cursor!");
    }

    // Sync the cursor pos when the buffer pos is changed manually.
    fn update_cursor_position(&mut self) {

        let mut x = 0u16;
        let mut y = 0u16;

        let chars_before_screen = self.num_chars_before_screen();

        for ch in self.data_buffer
                .chars()
                .take(self.buffer_pos - chars_before_screen) {

            if ch == '\n' {
                x = 0;
                y += 1;
            } else {
                x += 1;
            }
        }
        self.cursor_pos = (x, y);
    }

    pub fn num_chars_before_screen(&mut self) -> usize {
        let mut chars = 0;
        for i in 0..self.row_offset {
            chars += self.data_buffer.lines().nth(i).unwrap().len();
        }
        chars
    }

    // Sync the buffer pos when the cursor pos is changed manually.
    fn update_buffer_position(&mut self) {

        let mut buffer_pos = 0usize;
        let (col, mut row) = self.cursor_pos;
        row += self.row_offset as u16;
        let mut lines = self.data_buffer.lines();

        for _ in 0..row {
            if let Some(line) = lines.next() {
                // Increment because lines() removes '\n'
                buffer_pos += line.len() + 1;
            }
            else {
                break;
            }
        }
        self.buffer_pos = buffer_pos + col as usize
    }

    fn get_current_row_number(&self) -> usize {
        let mut count = 0;
        let mut line_num = 0;

        while count < self.buffer_pos {

            match self.data_buffer[count..].find('\n') {
                Some(line_len) => {
                    // Increment to move past '\n'
                    count += line_len + 1;
                    if count <= self.buffer_pos {
                        line_num += 1;
                    }
                }
                None => break
            }
        }
        line_num
    }

    fn get_row_length(&self, row: u16) -> Option<usize> {
        return match self.data_buffer.lines().nth(row as usize) {
            Some(line) => Some(line.len()),
            None => None
        };
    }

    fn queue_clear_screen(&mut self) {
        crossterm::queue!(
            self.stdout,
            MoveTo(0, 0),
            Clear(ClearType::All),
            Clear(ClearType::Purge),
        )
            .expect("Failed to clear screen!");
    }

    fn queue_write_data_buffer(&mut self) {

        // Write visible lines from row_offset to the end of the terminal
        let available_rows = terminal::size()
            .expect("Failed to get terminal size!")
            .1 - self.status_log.lines;

        if self.select_window.is_none() {
            for i in self.row_offset..self.row_offset + available_rows as usize {
                if let Some(line) = self.data_buffer.lines().nth(i) {
                    crossterm::queue!(
                        self.stdout,
                        Print(format!("{}\n", line)),
                    ).expect("Failed to print line!");
                }
                else {
                    //self.log(String::from("Ran out of lines!"));
                    break;
                }
            }
            // crossterm::queue! (
            //     self.stdout,
            //     Print(&self.data_buffer),
            // )
            //     .expect("Failed to write data buffer!");
        }
        else {
            let window = self.select_window.unwrap();
            let l = window.get_left();
            let r = window.get_right();
            crossterm::queue! (
                self.stdout,
                Print(&self.data_buffer[..l]),
                SetAttribute(Attribute::Bold),
                SetBackgroundColor(Color::Cyan),
                Print(&self.data_buffer[l..r]),
                SetAttribute(Attribute::Reset),
                ResetColor,
                Print(&self.data_buffer[r..]),
            )
                .expect("Failed to write data buffer!");
        }
    }

    fn queue_write_status_log(&mut self) {

        let (_, h) = crossterm::terminal::size()
            .expect("Failed to get terminal size!");

        crossterm::queue! (
            self.stdout,
            MoveTo(0, h.saturating_sub(self.status_log.lines))
        ).unwrap();

        for mut message in &self.status_log.log {
            crossterm::queue! (
                self.stdout,
                SetBackgroundColor(Color::DarkGrey),
                Print(message.clone().add("\n")),
                SetBackgroundColor(Color::Reset),
            ).unwrap();
        }
    }

    pub fn cursor_pos(&self) -> (u16, u16) {
        self.cursor_pos
    }

    pub fn buffer_pos(&self) -> usize {
        self.buffer_pos
    }
}