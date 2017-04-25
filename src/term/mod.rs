use std::io;
use std::io::Write;

use parking_lot::Mutex;

enum TermTarget {
    Stdout,
    Stderr,
}

/// Abstraction around a terminal.
pub struct Term {
    target: TermTarget,
    buffer: Option<Mutex<Vec<u8>>>,
}

impl Term {
    /// Return a new unbuffered terminal
    #[inline(always)]
    pub fn stdout() -> Term {
        Term {
            target: TermTarget::Stdout,
            buffer: None,
        }
    }

    /// Return a new unbuffered terminal to stderr
    #[inline(always)]
    pub fn stderr() -> Term {
        Term {
            target: TermTarget::Stderr,
            buffer: None,
        }
    }

    /// Return a new buffered terminal
    pub fn buffered_stdout() -> Term {
        Term {
            target: TermTarget::Stdout,
            buffer: Some(Mutex::new(vec![])),
        }
    }

    /// Return a new buffered terminal to stderr
    pub fn buffered_stderr() -> Term {
        Term {
            target: TermTarget::Stderr,
            buffer: Some(Mutex::new(vec![])),
        }
    }

    #[doc(hidden)]
    pub fn write_str(&self, s: &str) -> io::Result<()> {
        match self.buffer {
            Some(ref buffer) => buffer.lock().write_all(s.as_bytes()),
            None => self.write_through(s.as_bytes())
        }
    }

    /// Writes a string to the terminal and adds a newline.
    pub fn write_line(&self, s: &str) -> io::Result<()> {
        match self.buffer {
            Some(ref mutex) => {
                let mut buffer = mutex.lock();
                buffer.extend_from_slice(s.as_bytes());
                buffer.push(b'\n');
                Ok(())
            }
            None => {
                self.write_through(format!("{}\n", s).as_bytes())
            }
        }
    }

    /// Flushes
    pub fn flush(&self) -> io::Result<()> {
        match self.buffer {
            Some(ref buffer) => {
                let mut buffer = buffer.lock();
                if !buffer.is_empty() {
                    self.write_through(&buffer[..])?;
                    buffer.clear();
                }
            }
            None => {}
        }
        Ok(())
    }

    /// Checks if the terminal is indeed a terminal.
    pub fn is_term(&self) -> bool {
        is_a_terminal()
    }

    /// Returns the terminal size or gets sensible defaults
    #[inline(always)]
    pub fn size(&self) -> (u16, u16) {
        self.size_checked().unwrap_or((24, 80))
    }

    /// Returns the terminal size in rows and columns.
    ///
    /// If the size cannot be reliably determined None is returned.
    #[inline(always)]
    pub fn size_checked(&self) -> Option<(u16, u16)> {
        terminal_size()
    }

    /// Moves the cursor up `n` lines
    pub fn move_cursor_up(&self, n: usize) -> io::Result<()> {
        move_cursor_up(self, n)
    }

    /// Moves the cursor down `n` lines
    pub fn move_cursor_down(&self, n: usize) -> io::Result<()> {
        move_cursor_down(self, n)
    }

    /// Shows or hides the cursor
    pub fn show_cursor(&self, val: bool) -> io::Result<()> {
        if val {
            show_cursor(self)
        } else {
            hide_cursor(self)
        }
    }

    /// Clears the current line.
    pub fn clear_line(&self) -> io::Result<()> {
        clear_line(self)
    }

    /// Clear the last `n` lines.
    pub fn clear_last_lines(&self, n: usize) -> io::Result<()> {
        self.move_cursor_up(n)?;
        for _ in 0..n {
            self.clear_line()?;
            self.move_cursor_down(1)?;
        }
        self.move_cursor_up(n)?;
        Ok(())
    }

    // helpers

    fn write_through(&self, bytes: &[u8]) -> io::Result<()> {
        match self.target {
            TermTarget::Stdout => {
                io::stdout().write_all(bytes)?;
                io::stdout().flush()?;
            }
            TermTarget::Stderr => {
                io::stderr().write_all(bytes)?;
                io::stderr().flush()?;
            }
        }
        Ok(())
    }
}

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use self::unix::*;
