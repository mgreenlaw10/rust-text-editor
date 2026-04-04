// Terminal is split into 3 sections:
// Header
// FileView
// StatusLog

use crossterm::{self, terminal::{Clear, ClearType}, style::{Print}};
use std::io;

pub struct TerminalController {
    header: Header,
    file_view: FileView,
    status_log: StatusLog,
}

impl TerminalController {
    pub fn queue_draw_screen(&self) {
        self.header.queue_draw();
    }
}

pub struct Header {
    text: String,
    lines: u16,
}

pub struct FileView {
    text: String,
    lines: u16,
}

pub struct StatusLog {
    log: Vec<String>,
    lines: u16
}

impl Header {
    pub fn new(text: String) -> Header {
        Header { text: String::from("Header"), lines: 1 }
    }

    pub fn queue_draw(&self) {
        crossterm::queue!(
            io::stdout(),
            Print(&self.text)
        ).unwrap();
    }
}

impl FileView {
    pub fn new(text: String, lines: u16) -> FileView {
        FileView { text, lines, }
    }

    pub fn queue_draw(&self) {
        crossterm::queue! (
            io::stdout(),
            Print(&self.text)
        ).unwrap();
    }
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