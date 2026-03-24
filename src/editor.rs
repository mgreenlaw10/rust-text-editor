use std::fs::File;
use std::io::{self, Read, Write, Stdout, SeekFrom, Seek};

use crossterm::{
    style::{
        Print,
        Color
    },
    cursor::{
        MoveTo
    },
    terminal::{
        Clear,
        ClearType
    },
    QueueableCommand
};

pub struct Editor {
    file: File,
    file_name: String,
    stdout: Stdout,

    data_buffer: String,
    select_window: Option<SelectWindow>,
    buffer_pos: usize,
    cursor_pos: (u16, u16),

    status: Option<String>,
    header: Option<String>,
}

struct SelectWindow {
    begin: usize,
    end: usize,
}
impl Editor {

    pub fn new(mut file: File, file_name: String) -> Self {

        // Load file into buffer
        let mut data_buffer = String::new();
        file.read_to_string(&mut data_buffer)
            .expect("Failed to read data from {file_name}!");

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
            status: None,
            header: None,
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.data_buffer.insert(self.buffer_pos, c);
    }

    pub fn delete_char(&mut self) {
        self.data_buffer.remove(self.buffer_pos);
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

        let (col, row) = self.cursor_pos;
        let original_pos = self.buffer_pos as isize;
        let final_row = self.data_buffer.lines().count() as u16 - 1;

        let target_row = row.saturating_add_signed(num_rows);

        self.cursor_pos = (col, target_row.clamp(0, final_row));
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

    pub fn save_file(&mut self) -> Result<(), Box<dyn std::error::Error>> {

        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(self.data_buffer.as_bytes())?;
        self.file.flush()?;

        Ok(())
    }

    pub fn set_status(&mut self, status: String) {
        self.status = Some(status);
    }

    pub fn redraw(&mut self) {

        self.queue_clear_screen();
        self.queue_write_data_buffer();
        self.queue_write_status();

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

        for ch in self.data_buffer
                .chars()
                .take(self.buffer_pos) {

            if ch == '\n' {
                x = 0;
                y += 1;
            } else {
                x += 1;
            }
        }
        self.cursor_pos = (x, y);
    }

    // Sync the buffer pos when the cursor pos is changed manually.
    fn update_buffer_position(&mut self) -> usize {

        let mut buffer_pos = 0usize;
        let (col, row) = self.cursor_pos;
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
        buffer_pos + col as usize
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
        crossterm::queue! (
            self.stdout,
            Print(&self.data_buffer),
        )
            .expect("Failed to write data buffer!");
    }

    fn queue_write_status(&mut self) {
        // Skip if status doesn't exist
        if let Some(ref status) = self.status {

            let (_, h) = crossterm::terminal::size()
                .expect("Failed to get terminal size!");

            crossterm::queue! (
                self.stdout,
                MoveTo(0, h.saturating_sub(1)),
                Print(status),
            )
                .expect("Failed to write status!");
        }
    }

    pub fn cursor_pos(&self) -> (u16, u16) {
        self.cursor_pos
    }

    pub fn buffer_pos(&self) -> usize {
        self.buffer_pos
    }
}