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

        // Don't try to handle writing lines with additional newlines embedded in them - it's not
        // worth the extra code for something that indicatif doesn't even do. May revisit in future.
        debug_assert!(
            s.lines().count() <= 1,
            "calling write_line with embedded newlines is not allowed"
        );

        // vte100 needs the full \r\n sequence to jump to the next line and reset the cursor to
        // the beginning of the line. Be flexible and take either \n or \r\n
        state.write_str(s)?;
        state.write_str("\r\n")
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
    use crate::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressFinish};

    fn cursor_pos(in_mem: &InMemoryTerm) -> (u16, u16) {
        in_mem
            .state
            .lock()
            .unwrap()
            .parser
            .screen()
            .cursor_position()
    }

    #[test]
    fn line_wrapping() {
        let in_mem = InMemoryTerm::new(10, 5);
        assert_eq!(cursor_pos(&in_mem), (0, 0));

        in_mem.write_str("ABCDE").unwrap();
        assert_eq!(in_mem.contents(), "ABCDE");
        assert_eq!(cursor_pos(&in_mem), (0, 5));

        // Should wrap onto next line
        in_mem.write_str("FG").unwrap();
        assert_eq!(in_mem.contents(), "ABCDE\nFG");
        assert_eq!(cursor_pos(&in_mem), (1, 2));

        in_mem.write_str("HIJ").unwrap();
        assert_eq!(in_mem.contents(), "ABCDE\nFGHIJ");
        assert_eq!(cursor_pos(&in_mem), (1, 5));
    }

    #[test]
    fn write_line() {
        let in_mem = InMemoryTerm::new(10, 5);
        assert_eq!(cursor_pos(&in_mem), (0, 0));

        in_mem.write_line("A").unwrap();
        assert_eq!(in_mem.contents(), "A");
        assert_eq!(cursor_pos(&in_mem), (1, 0));

        in_mem.write_line("B").unwrap();
        assert_eq!(in_mem.contents(), "A\nB");
        assert_eq!(cursor_pos(&in_mem), (2, 0));

        in_mem.write_line("Longer than cols").unwrap();
        assert_eq!(in_mem.contents(), "A\nB\nLonge\nr tha\nn col\ns");
        assert_eq!(cursor_pos(&in_mem), (6, 0));
    }

    #[test]
    fn basic_functionality() {
        let in_mem = InMemoryTerm::new(10, 80);

        in_mem.write_line("This is a test line").unwrap();
        assert_eq!(in_mem.contents(), "This is a test line");

        in_mem.write_line("And another line!").unwrap();
        assert_eq!(in_mem.contents(), "This is a test line\nAnd another line!");

        in_mem.move_cursor_up(1).unwrap();
        in_mem.write_str("TEST").unwrap();

        assert_eq!(in_mem.contents(), "This is a test line\nTESTanother line!");
    }

    #[test]
    fn newlines() {
        let in_mem = InMemoryTerm::new(10, 10);
        in_mem.write_line("LINE ONE").unwrap();
        in_mem.write_line("LINE TWO").unwrap();
        in_mem.write_line("").unwrap();
        in_mem.write_line("LINE FOUR").unwrap();

        assert_eq!(in_mem.contents(), "LINE ONE\nLINE TWO\n\nLINE FOUR");
    }

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

        let pb1 = mp.add(ProgressBar::new(10).with_finish(ProgressFinish::AndLeave));
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
