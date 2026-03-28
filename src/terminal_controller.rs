// Terminal is split into 3 sections:
// Header
// FileView
// StatusLog

use crossterm::{self, terminal::{Clear, ClearType}};
use std::io;

pub struct TerminalController {
    header: Header,
    file_view: FileView,
    status_log: StatusLog,
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

    pub fn queue_draw() {
        crossterm::queue!(io::stdout(), Clear(ClearType::All)).unwrap();
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