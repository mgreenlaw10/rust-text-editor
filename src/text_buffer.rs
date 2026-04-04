use crossterm::style::Print;
use std::fs::{File, OpenOptions};
use std::io;
use std::io::{Read, Seek, SeekFrom, Stdout, Write};
use std::process::Stdio;

pub struct TextBuffer {
    file: File,
    fptr: u64,
    chars: [u8; 1024],
    head: usize
}

impl TextBuffer {

    // Main constructor
    pub fn with_file(file_name: &str) -> Result<Self, io::Error> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&file_name)?;

        let mut buffer = [0; 1024];
        file.read(&mut buffer)?;

        Ok(Self {
            file,
            fptr: 0,
            chars: buffer,
            head: 0
        })
    }
    pub fn write_char(&mut self, c: u8) {
        self.chars[self.head] = c;
    }

    pub fn move_head(&mut self, dst: isize) {
        self.head = self.head.saturating_add_signed(dst);
    }

    pub fn flush(&mut self) -> io::Result<()> {
        // Only overwrite bytes [offset, offset + 1024)
        self.file.seek(SeekFrom::Start(self.fptr))?;
        self.file.write_all(self.chars.as_ref())?;
        self.file.flush()?;

        Ok(())
    }

    pub fn queue_draw(&mut self, stdout: &mut Stdout) {
        stdout.write_all(&self.chars)
            .expect("Failed to write to stdout!");
    }
}