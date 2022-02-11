use std::fmt::{Debug, Formatter};
use std::io::Write;
use std::sync::{Arc, Mutex};

use vt100::Parser;

use crate::TermLike;

/// A thin wrapper around [`vt100::Parser`].
///
/// This is just an [`Arc`] around its internal state, so it can be freely cloned.
#[derive(Debug, Clone)]
pub struct InMemoryTerm {
    state: Arc<Mutex<InMemoryTermState>>,
}

impl InMemoryTerm {
    pub fn new(rows: u16, cols: u16) -> InMemoryTerm {
        assert!(rows > 0, "rows must be > 0");
        assert!(cols > 0, "cols must be > 0");
        InMemoryTerm {
            state: Arc::new(Mutex::new(InMemoryTermState::new(rows, cols))),
        }
    }

    pub fn contents(&self) -> String {
        let state = self.state.lock().unwrap();

        // For some reason, the `Screen::contents` method doesn't include newlines in what it
        // returns, making it useless for our purposes. So we need to manually reconstruct the
        // contents by iterating over the rows in the terminal buffer.
        let mut rows = state
            .parser
            .screen()
            .rows(0, state.width)
            .collect::<Vec<_>>();

        // Reverse the rows and trim empty lines from the end
        rows = rows
            .into_iter()
            .rev()
            .skip_while(|line| line.is_empty())
            .collect();

        // Un-reverse the rows and join them up with newlines
        rows.reverse();
        rows.join("\n")
    }
}

impl TermLike for InMemoryTerm {
    fn width(&self) -> usize {
        self.state.lock().unwrap().width as usize
    }

    fn move_cursor_up(&self, n: usize) -> std::io::Result<()> {
        self.state
            .lock()
            .unwrap()
            .write_str(&*format!("\x1b[{}A", n))
    }

    fn move_cursor_down(&self, n: usize) -> std::io::Result<()> {
        self.state
            .lock()
            .unwrap()
            .write_str(&*format!("\x1b[{}B", n))
    }

    fn move_cursor_right(&self, n: usize) -> std::io::Result<()> {
        self.state
            .lock()
            .unwrap()
            .write_str(&*format!("\x1b[{}C", n))
    }

    fn move_cursor_left(&self, n: usize) -> std::io::Result<()> {
        self.state
            .lock()
            .unwrap()
            .write_str(&*format!("\x1b[{}D", n))
    }

    fn write_line(&self, s: &str) -> std::io::Result<()> {
        let mut state = self.state.lock().unwrap();
        state.write_str(s)?;
        if (s.len() < state.width as usize) && !s.ends_with('\n') {
            state.write_str("\n")
        } else {
            Ok(())
        }
    }

    fn write_str(&self, s: &str) -> std::io::Result<()> {
        self.state.lock().unwrap().write_str(s)
    }

    fn clear_line(&self) -> std::io::Result<()> {
        self.state.lock().unwrap().write_str("\r\x1b[2K")
    }

    fn flush(&self) -> std::io::Result<()> {
        self.state.lock().unwrap().parser.flush()
    }
}

struct InMemoryTermState {
    width: u16,
    parser: vt100::Parser,
}

impl InMemoryTermState {
    pub(crate) fn new(rows: u16, cols: u16) -> InMemoryTermState {
        InMemoryTermState {
            width: cols,
            parser: Parser::new(rows, cols, 0),
        }
    }

    pub(crate) fn write_str(&mut self, s: &str) -> std::io::Result<()> {
        self.parser.write_all(s.as_bytes())
    }
}

impl Debug for InMemoryTermState {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryTermState").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressFinish, ProgressStyle};

    #[test]
    fn basic_progress_bar() {
        let in_mem = InMemoryTerm::new(10, 80);
        let pb = ProgressBar::with_draw_target(
            10,
            ProgressDrawTarget::term_like(Box::new(in_mem.clone())),
        );

        assert_eq!(in_mem.contents(), String::new());

        pb.tick();
        assert_eq!(
            in_mem.contents(),
            "░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/10"
        );

        pb.finish();
        assert_eq!(
            in_mem.contents(),
            "██████████████████████████████████████████████████████████████████████████ 10/10"
        );
    }

    #[test]
    fn multi_progress() {
        let in_mem = InMemoryTerm::new(10, 80);
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::term_like(Box::new(
            in_mem.clone(),
        )));

        let pb1 = mp.add(
            ProgressBar::new(10)
                .with_style(ProgressStyle::default_bar().on_finish(ProgressFinish::AndLeave)),
        );
        let pb2 = mp.add(ProgressBar::new(5));
        let pb3 = mp.add(ProgressBar::new(100));

        assert_eq!(in_mem.contents(), String::new());

        pb1.tick();
        assert_eq!(
            in_mem.contents(),
            r#"░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/10"#
        );

        pb2.tick();

        assert_eq!(
            in_mem.contents(),
            r#"
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/10
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/5"#
                .trim_start()
        );

        pb3.tick();
        assert_eq!(
            in_mem.contents(),
            r#"
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/10
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/5
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/100"#
                .trim_start()
        );

        drop(pb1);
        drop(pb2);
        drop(pb3);

        assert_eq!(
            in_mem.contents(),
            r#"██████████████████████████████████████████████████████████████████████████ 10/10"#
        );
    }
}
