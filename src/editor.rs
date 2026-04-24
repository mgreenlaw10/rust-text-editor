use std::fs::File;
use std::io::{self, Read, Write, Stdout, SeekFrom, Seek};
use std::ops::Add;

use crossterm::{
    style::{
        Print,
        Color,
        Attribute,
        SetAttribute,
        SetBackgroundColor,
        ResetColor,
    },
    cursor::MoveTo,
    terminal::{
        Clear,
        ClearType,
    },
    terminal,
};

use crate::snapshot_controller::SnapshotController;

pub struct StatusLog {
    log: Vec<String>,
    lines: u16,
}

impl StatusLog {
    pub fn new(lines: u16) -> StatusLog {
        StatusLog {
            log: Vec::new(),
            lines,
        }
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

    // BYTE index into data_buffer.
    buffer_pos: usize,

    // Visible cursor position: (character column, visible row).
    cursor_pos: (u16, u16),

    header: Option<String>,

    status_log: StatusLog,

    file_state_controller: SnapshotController,

    // Number of rows scrolled from top.
    row_offset: usize,
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
        let mut data_buffer = String::new();

        file.read_to_string(&mut data_buffer)
            .expect("Failed to read data from {file_name}!");

        let state = data_buffer.clone();

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

            row_offset: 0,
        }
    }

    pub fn insert_char(&mut self, c: char) {
        if self.select_window.is_some() {
            let window = self.select_window.unwrap();
            let left = window.get_left();
            let right = window.get_right();

            self.data_buffer.drain(left..right);
            self.buffer_pos = left;
            self.select_window = None;
        }

        self.data_buffer.insert(self.buffer_pos, c);
        self.buffer_pos += c.len_utf8();

        self.clamp_buffer_pos_to_char_boundary();
        self.update_cursor_position();
        self.save_snapshot();
    }

    pub fn delete_char(&mut self) {
        if self.select_window.is_none() {
            if self.buffer_pos == 0 {
                return;
            }

            let prev = self.prev_char_boundary(self.buffer_pos);
            self.data_buffer.drain(prev..self.buffer_pos);
            self.buffer_pos = prev;
        } else {
            let window = self.select_window.unwrap();
            let left = window.get_left();
            let right = window.get_right();

            self.data_buffer.drain(left..right);
            self.buffer_pos = left;
            self.select_window = None;
        }

        self.clamp_buffer_pos_to_char_boundary();
        self.update_cursor_position();
        self.save_snapshot();
    }

    pub fn save_snapshot(&mut self) {
        self.file_state_controller
            .push_snapshot(self.data_buffer.clone(), self.cursor_pos());
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
        let original_col = self.cursor_pos.0;

        if num_cols > 0 {
            for _ in 0..num_cols {
                if !self.move_right_one_char() {
                    break;
                }
            }
        } else {
            for _ in 0..num_cols.unsigned_abs() {
                if !self.move_left_one_char() {
                    break;
                }
            }
        }

        self.update_cursor_position();

        original_col.abs_diff(self.cursor_pos.0) as usize
    }

    // Will clamp to the current col.
    // Return the amount of rows actually moved.
    pub fn move_rows(&mut self, num_rows: i16) -> usize {
        let (mut col, mut row) = self.cursor_pos;
        let original_row = row;

        let line_count = self.data_buffer.lines().count();
        if line_count == 0 {
            self.buffer_pos = 0;
            self.cursor_pos = (0, 0);
            return 0;
        }

        let max_absolute_row = line_count.saturating_sub(1) as u16;
        let absolute_row = row.saturating_add(self.row_offset as u16);

        let target_absolute_row = absolute_row.saturating_add_signed(num_rows);
        let final_absolute_row = target_absolute_row.clamp(0, max_absolute_row);

        row = final_absolute_row.saturating_sub(self.row_offset as u16);

        if let Some(row_len) = self.get_row_length(final_absolute_row) {
            col = std::cmp::min(col, row_len as u16);
        }

        self.cursor_pos = (col, row);
        self.update_buffer_position();

        original_row.abs_diff(self.cursor_pos.1) as usize
    }

    pub fn move_next_line(&mut self) -> Result<(), ()> {
        for (offset, c) in self.data_buffer[self.buffer_pos..].char_indices() {
            if c == '\n' {
                self.buffer_pos += offset + c.len_utf8();
                self.update_cursor_position();
                return Ok(());
            }
        }

        self.insert_char('\n');
        Ok(())
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
        } else {
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
        } else {
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

    pub fn snap_drag_right(&mut self) {
        if let Some(next_space_pos) = self.get_next_char_pos(' ') {
            let distance = self.count_chars_between(self.buffer_pos, next_space_pos) as isize;
            self.drag_cols(distance);
        } else {
            self.drag_cols(isize::MAX);
        }
    }

    pub fn snap_drag_left(&mut self) {
        if let Some(last_space_pos) = self.get_last_char_pos(' ') {
            let distance = -(self.count_chars_between(last_space_pos, self.buffer_pos) as isize);
            self.drag_cols(distance);
        }
    }

    pub fn get_next_char_pos(&mut self, c: char) -> Option<usize> {
        self.data_buffer[self.buffer_pos..]
            .char_indices()
            .find(|(_, fc)| *fc == c)
            .map(|(offset, _)| self.buffer_pos + offset)
    }

    pub fn get_last_char_pos(&mut self, c: char) -> Option<usize> {
        self.data_buffer[..self.buffer_pos]
            .char_indices()
            .rev()
            .find(|(_, fc)| *fc == c)
            .map(|(index, _)| index)
    }

    pub fn page_down(&mut self) {
        self.row_offset += terminal::size().unwrap().1 as usize;
        self.row_offset = self.row_offset.clamp(0, self.data_buffer.lines().count());
        self.update_buffer_position();
    }

    pub fn page_up(&mut self) {
        self.row_offset = self
            .row_offset
            .saturating_sub(terminal::size().unwrap().1 as usize);

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
        self.queue_write_status_log();

        self.stdout.flush()
            .expect("Failed to flush stdout!");

        crossterm::execute!(
            self.stdout,
            MoveTo(self.cursor_pos.0, self.cursor_pos.1)
        )
        .expect("Failed to move to cursor!");
    }

    // Sync the cursor pos when the buffer pos is changed manually.
    fn update_cursor_position(&mut self) {
        self.clamp_buffer_pos_to_char_boundary();

        let mut absolute_row = 0usize;
        let mut col = 0usize;

        for ch in self.data_buffer[..self.buffer_pos].chars() {
            if ch == '\n' {
                absolute_row += 1;
                col = 0;
            } else {
                col += 1;
            }
        }

        let visible_row = absolute_row.saturating_sub(self.row_offset);

        self.cursor_pos = (
            col.min(u16::MAX as usize) as u16,
            visible_row.min(u16::MAX as usize) as u16,
        );
    }

    // Sync the buffer pos when the cursor pos is changed manually.
    fn update_buffer_position(&mut self) {
        let (col, row) = self.cursor_pos;
        let absolute_row = row as usize + self.row_offset;

        let mut current_row = 0usize;
        let mut row_start_byte = 0usize;

        for line in self.data_buffer.split_inclusive('\n') {
            if current_row == absolute_row {
                let line_without_newline = line.strip_suffix('\n').unwrap_or(line);
                let target_byte_inside_line = Self::byte_index_for_char_col(
                    line_without_newline,
                    col as usize,
                );

                self.buffer_pos = row_start_byte + target_byte_inside_line;
                self.clamp_buffer_pos_to_char_boundary();
                return;
            }

            row_start_byte += line.len();
            current_row += 1;
        }

        self.buffer_pos = self.data_buffer.len();
        self.clamp_buffer_pos_to_char_boundary();
    }

    fn get_current_row_number(&self) -> usize {
        self.data_buffer[..self.buffer_pos]
            .chars()
            .filter(|c| *c == '\n')
            .count()
    }

    fn get_row_length(&self, row: u16) -> Option<usize> {
        self.data_buffer
            .lines()
            .nth(row as usize)
            .map(|line| line.chars().count())
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
        let available_rows = terminal::size()
            .expect("Failed to get terminal size!")
            .1
            .saturating_sub(self.status_log.lines);

        if self.select_window.is_none() {
            for i in self.row_offset..self.row_offset + available_rows as usize {
                if let Some(line) = self.data_buffer.lines().nth(i) {
                    crossterm::queue!(
                        self.stdout,
                        Print(format!("{}\n", line)),
                    )
                    .expect("Failed to print line!");
                } else {
                    break;
                }
            }
        } else {
            let window = self.select_window.unwrap();
            let l = window.get_left();
            let r = window.get_right();

            crossterm::queue!(
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

        crossterm::queue!(
            self.stdout,
            MoveTo(0, h.saturating_sub(self.status_log.lines))
        )
        .unwrap();

        for message in &self.status_log.log {
            crossterm::queue!(
                self.stdout,
                SetBackgroundColor(Color::DarkGrey),
                Print(message.clone().add("\n")),
                SetBackgroundColor(Color::Reset),
            )
            .unwrap();
        }
    }

    pub fn cursor_pos(&self) -> (u16, u16) {
        self.cursor_pos
    }

    pub fn buffer_pos(&self) -> usize {
        self.buffer_pos
    }

    fn move_right_one_char(&mut self) -> bool {
        if self.buffer_pos >= self.data_buffer.len() {
            return false;
        }

        let current_char = self.data_buffer[self.buffer_pos..].chars().next();

        if let Some(c) = current_char {
            self.buffer_pos += c.len_utf8();
            true
        } else {
            false
        }
    }

    fn move_left_one_char(&mut self) -> bool {
        if self.buffer_pos == 0 {
            return false;
        }

        self.buffer_pos = self.prev_char_boundary(self.buffer_pos);
        true
    }

    fn prev_char_boundary(&self, byte_pos: usize) -> usize {
        self.data_buffer[..byte_pos]
            .char_indices()
            .last()
            .map(|(index, _)| index)
            .unwrap_or(0)
    }

    fn clamp_buffer_pos_to_char_boundary(&mut self) {
        self.buffer_pos = self.buffer_pos.min(self.data_buffer.len());

        while self.buffer_pos > 0 && !self.data_buffer.is_char_boundary(self.buffer_pos) {
            self.buffer_pos -= 1;
        }
    }

    fn byte_index_for_char_col(s: &str, target_col: usize) -> usize {
        match s.char_indices().nth(target_col) {
            Some((byte_index, _)) => byte_index,
            None => s.len(),
        }
    }

    fn count_chars_between(&self, left: usize, right: usize) -> usize {
        if left >= right {
            return 0;
        }

        self.data_buffer[left..right].chars().count()
    }
}