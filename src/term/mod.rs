use std::io;
use std::io::Write;

#[cfg(unix)]
use std::os::unix::io::{AsRawFd, RawFd};
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, RawHandle};

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
        is_a_terminal(self)
    }

    /// Returns the terminal size or gets sensible defaults.
    #[inline(always)]
    pub fn size(&self) -> (u16, u16) {
        self.size_checked().unwrap_or((24, DEFAULT_WIDTH))
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

/// A fast way to check if the application has a user attended.
///
/// This means that stdout is connected to a terminal instead of a
/// file or redirected by other means.
pub fn user_attended() -> bool {
    Term::stdout().is_term()
}

#[cfg(unix)]
impl AsRawFd for Term {

    fn as_raw_fd(&self) -> RawFd {
        use libc;
        match self.target {
            TermTarget::Stdout => libc::STDOUT_FILENO,
            TermTarget::Stderr => libc::STDERR_FILENO,
        }
    }
}

#[cfg(windows)]
impl AsRawHandle for Term {

    fn as_raw_handle(&self) -> RawHandle {
        use winapi::{STD_OUTPUT_HANDLE, STD_ERROR_HANDLE};
        use kernel32::GetStdHandle;
        unsafe {
            GetStdHandle(match self.target {
                TermTarget::Stdout => STD_OUTPUT_HANDLE,
                TermTarget::Stderr => STD_ERROR_HANDLE,
            }) as RawHandle
        }
    }
}

#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use self::unix::*;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::*;
