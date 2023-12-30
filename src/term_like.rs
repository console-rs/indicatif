use std::cell::Cell;
use std::fmt::Debug;
use std::io;

use console::Term;

/// A trait for minimal terminal-like behavior.
///
/// Anything that implements this trait can be used a draw target via [`ProgressDrawTarget::term_like`].
///
/// [`ProgressDrawTarget::term_like`]: crate::ProgressDrawTarget::term_like
pub trait TermLike: Debug + Send + Sync {
    /// Return the terminal width
    fn width(&self) -> u16;
    /// Return the terminal height
    fn height(&self) -> u16 {
        // FIXME: remove this default impl in the next major version bump
        20 // sensible default
    }

    /// Move the cursor up by `n` lines
    fn move_cursor_up(&self, n: usize) -> io::Result<()>;
    /// Move the cursor down by `n` lines
    fn move_cursor_down(&self, n: usize) -> io::Result<()>;
    /// Move the cursor right by `n` chars
    fn move_cursor_right(&self, n: usize) -> io::Result<()>;
    /// Move the cursor left by `n` chars
    fn move_cursor_left(&self, n: usize) -> io::Result<()>;

    /// Write a string and add a newline.
    fn write_line(&self, s: &str) -> io::Result<()>;
    /// Write a string
    fn write_str(&self, s: &str) -> io::Result<()>;
    /// Clear the current line and reset the cursor to beginning of the line
    fn clear_line(&self) -> io::Result<()>;

    fn flush(&self) -> io::Result<()>;

    // Whether ANSI escape sequences are supported
    fn supports_ansi_codes(&self) -> bool;
}

impl TermLike for Term {
    fn width(&self) -> u16 {
        self.size().1
    }

    fn height(&self) -> u16 {
        self.size().0
    }

    fn move_cursor_up(&self, n: usize) -> io::Result<()> {
        self.move_cursor_up(n)
    }

    fn move_cursor_down(&self, n: usize) -> io::Result<()> {
        self.move_cursor_down(n)
    }

    fn move_cursor_right(&self, n: usize) -> io::Result<()> {
        self.move_cursor_right(n)
    }

    fn move_cursor_left(&self, n: usize) -> io::Result<()> {
        self.move_cursor_left(n)
    }

    fn write_line(&self, s: &str) -> io::Result<()> {
        self.write_line(s)
    }

    fn write_str(&self, s: &str) -> io::Result<()> {
        self.write_str(s)
    }

    fn clear_line(&self) -> io::Result<()> {
        self.clear_line()
    }

    fn flush(&self) -> io::Result<()> {
        self.flush()
    }

    fn supports_ansi_codes(&self) -> bool {
        self.features().colors_supported()
    }
}

pub(crate) struct SyncGuard<'a, T: TermLike + ?Sized> {
    term_like: Cell<Option<&'a T>>,
}

impl<'a, T: TermLike + ?Sized> SyncGuard<'a, T> {
    pub(crate) fn begin_sync(term_like: &'a T) -> io::Result<Self> {
        term_like.write_str("\x1b[?2026h")?;
        Ok(Self {
            term_like: Cell::new(Some(term_like)),
        })
    }

    pub(crate) fn finish_sync(self) -> io::Result<()> {
        self.finish_sync_inner()
    }

    fn finish_sync_inner(&self) -> io::Result<()> {
        if let Some(term_like) = self.term_like.take() {
            term_like.write_str("\x1b[?2026l")?;
        }
        Ok(())
    }
}

impl<T: TermLike + ?Sized> Drop for SyncGuard<'_, T> {
    fn drop(&mut self) {
        let _ = self.finish_sync_inner();
    }
}
