use std::io;
use std::io::Write;

use parking_lot::Mutex;

enum TermTarget {
    Stdout,
    Stderr,
}

pub struct Term {
    target: TermTarget,
    buffer: Mutex<Vec<u8>>,
}

impl Term {
    pub fn stdout() -> Term {
        Term {
            target: TermTarget::Stdout,
            buffer: Mutex::new(vec![]),
        }
    }

    pub fn write_line(&self, s: &str) -> io::Result<()> {
        let mut buffer = self.buffer.lock();
        buffer.extend_from_slice(s.as_bytes());
        buffer.push(b'\n');
        Ok(())
    }

    pub fn write_bytes(&self, bytes: &[u8]) -> io::Result<()> {
        self.buffer.lock().write_all(bytes)
    }

    pub fn flush(&self) -> io::Result<()> {
        let mut buffer = self.buffer.lock();
        if !buffer.is_empty() {
            match self.target {
                TermTarget::Stdout => {
                    io::stdout().write_all(&buffer[..])?;
                    io::stdout().flush()?;
                }
                TermTarget::Stderr => {
                    io::stderr().write_all(&buffer[..])?;
                    io::stderr().flush()?;
                }
            }
            buffer.clear();
        }
        Ok(())
    }

    pub fn size(&self) -> Option<(u16, u16)> {
        terminal_size()
    }

    pub fn move_cursor_up(&self, n: usize) -> io::Result<()> {
        move_cursor_up(self, n)
    }

    pub fn move_cursor_down(&self, n: usize) -> io::Result<()> {
        move_cursor_down(self, n)
    }

    pub fn clear_line(&self) -> io::Result<()> {
        clear_line(self)
    }

    pub fn show_cursor(&self, val: bool) -> io::Result<()> {
        if val {
            show_cursor(self)
        } else {
            hide_cursor(self)
        }
    }

    pub fn clear_last_lines(&self, n: usize) -> io::Result<()> {
        self.move_cursor_up(n)?;
        for _ in 0..n {
            self.clear_line()?;
            self.move_cursor_down(1)?;
        }
        self.move_cursor_up(n)?;
        Ok(())
    }
}

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use self::unix::*;
