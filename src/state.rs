use std::borrow::Cow;
use std::io;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::style::{ProgressFinish, ProgressStyle};
use crate::utils::{duration_to_secs, secs_to_duration, Estimate};
use console::Term;

/// The state of a progress bar at a moment in time.
pub(crate) struct ProgressState {
    pub(crate) style: ProgressStyle,
    pub(crate) pos: u64,
    pub(crate) len: u64,
    pub(crate) tick: u64,
    pub(crate) started: Instant,
    pub(crate) draw_target: ProgressDrawTarget,
    pub(crate) message: Cow<'static, str>,
    pub(crate) prefix: Cow<'static, str>,
    pub(crate) draw_delta: u64,
    pub(crate) draw_rate: u64,
    pub(crate) draw_next: u64,
    pub(crate) status: Status,
    pub(crate) est: Estimate,
    pub(crate) tick_thread: Option<thread::JoinHandle<()>>,
    pub(crate) steady_tick: u64,
}

impl ProgressState {
    /// Returns the string that should be drawn for the
    /// current spinner string.
    pub fn current_tick_str(&self) -> &str {
        if self.is_finished() {
            self.style.get_final_tick_str()
        } else {
            self.style.get_tick_str(self.tick)
        }
    }

    /// Indicates that the progress bar finished.
    pub fn is_finished(&self) -> bool {
        match self.status {
            Status::InProgress => false,
            Status::DoneVisible => true,
            Status::DoneHidden => true,
        }
    }

    /// Returns `false` if the progress bar should no longer be
    /// drawn.
    pub fn should_render(&self) -> bool {
        !matches!(self.status, Status::DoneHidden)
    }

    /// Returns the completion as a floating-point number between 0 and 1
    pub fn fraction(&self) -> f32 {
        let pct = match (self.pos, self.len) {
            (_, 0) => 1.0,
            (0, _) => 0.0,
            (pos, len) => pos as f32 / len as f32,
        };
        pct.max(0.0).min(1.0)
    }

    /// Returns the position of the status bar as `(pos, len)` tuple.
    pub fn position(&self) -> (u64, u64) {
        (self.pos, self.len)
    }

    /// Returns the current message of the progress bar.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the current prefix of the progress bar.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// The entire draw width
    pub fn width(&self) -> usize {
        self.draw_target.width()
    }

    /// Return the current average time per step
    pub fn avg_time_per_step(&self) -> Duration {
        self.est.time_per_step()
    }

    /// The expected ETA
    pub fn eta(&self) -> Duration {
        if self.len == !0 || self.is_finished() {
            return Duration::new(0, 0);
        }
        let t = duration_to_secs(self.avg_time_per_step());
        // add 0.75 to leave 0.25 sec of 0s for the user
        secs_to_duration(t * self.len.saturating_sub(self.pos) as f64 + 0.75)
    }

    /// The expected total duration (that is, elapsed time + expected ETA)
    pub fn duration(&self) -> Duration {
        if self.len == !0 || self.is_finished() {
            return Duration::new(0, 0);
        }
        self.started.elapsed() + self.eta()
    }

    /// The number of steps per second
    pub fn per_sec(&self) -> u64 {
        let avg_time = self.avg_time_per_step().as_nanos();
        if avg_time == 0 {
            0
        } else {
            (1_000_000_000 / avg_time) as u64
        }
    }

    /// Call the provided `FnOnce` to update the state. Then redraw the
    /// progress bar if the state has changed.
    pub fn update_and_draw<F: FnOnce(&mut ProgressState)>(&mut self, f: F) {
        if self.update(f) {
            self.draw().ok();
        }
    }

    /// Call the provided `FnOnce` to update the state. Then unconditionally redraw the
    /// progress bar.
    pub fn update_and_force_draw<F: FnOnce(&mut ProgressState)>(&mut self, f: F) {
        self.update(|state| {
            state.draw_next = state.pos;
            f(state);
        });
        self.draw().ok();
    }

    /// Call the provided `FnOnce` to update the state. If a draw should be run, returns `true`.
    pub fn update<F: FnOnce(&mut ProgressState)>(&mut self, f: F) -> bool {
        let old_pos = self.pos;
        f(self);
        let new_pos = self.pos;
        if new_pos != old_pos {
            self.est.record_step(new_pos);
        }
        if new_pos >= self.draw_next {
            self.draw_next = new_pos.saturating_add(if self.draw_rate != 0 {
                self.per_sec() / self.draw_rate
            } else {
                self.draw_delta
            });
            true
        } else {
            false
        }
    }

    /// Finishes the progress bar and leaves the current message.
    pub fn finish(&mut self) {
        self.update_and_force_draw(|state| {
            state.pos = state.len;
            state.status = Status::DoneVisible;
        });
    }

    /// Finishes the progress bar at current position and leaves the current message.
    pub fn finish_at_current_pos(&mut self) {
        self.update_and_force_draw(|state| {
            state.status = Status::DoneVisible;
        });
    }

    /// Finishes the progress bar and sets a message.
    pub fn finish_with_message(&mut self, msg: impl Into<Cow<'static, str>>) {
        let msg = msg.into();
        self.update_and_force_draw(|state| {
            state.message = msg;
            state.pos = state.len;
            state.status = Status::DoneVisible;
        });
    }

    /// Finishes the progress bar and completely clears it.
    pub fn finish_and_clear(&mut self) {
        self.update_and_force_draw(|state| {
            state.pos = state.len;
            state.status = Status::DoneHidden;
        });
    }

    /// Finishes the progress bar and leaves the current message and progress.
    pub fn abandon(&mut self) {
        self.update_and_force_draw(|state| {
            state.status = Status::DoneVisible;
        });
    }

    /// Finishes the progress bar and sets a message, and leaves the current progress.
    pub fn abandon_with_message(&mut self, msg: impl Into<Cow<'static, str>>) {
        let msg = msg.into();
        self.update_and_force_draw(|state| {
            state.message = msg;
            state.status = Status::DoneVisible;
        });
    }

    /// Finishes the progress bar using the [`ProgressFinish`] behavior stored
    /// in the [`ProgressStyle`].
    pub fn finish_using_style(&mut self) {
        match self.style.get_on_finish() {
            ProgressFinish::AndLeave => self.finish(),
            ProgressFinish::AtCurrentPos => self.finish_at_current_pos(),
            ProgressFinish::WithMessage(msg) => {
                // Equivalent to `self.finish_with_message` but avoids borrow checker error
                self.message.clone_from(msg);
                self.finish();
            }
            ProgressFinish::AndClear => self.finish_and_clear(),
            ProgressFinish::Abandon => self.abandon(),
            ProgressFinish::AbandonWithMessage(msg) => {
                // Equivalent to `self.abandon_with_message` but avoids borrow checker error
                self.message.clone_from(msg);
                self.abandon();
            }
        }
    }

    pub(crate) fn draw(&mut self) -> io::Result<()> {
        // we can bail early if the draw target is hidden.
        if self.draw_target.is_hidden() {
            return Ok(());
        }

        let draw_state = ProgressDrawState {
            lines: if self.should_render() {
                self.style.format_state(&*self)
            } else {
                vec![]
            },
            orphan_lines: 0,
            finished: self.is_finished(),
            force_draw: false,
            move_cursor: false,
        };
        self.draw_target.apply_draw_state(draw_state)
    }
}

impl Drop for ProgressState {
    fn drop(&mut self) {
        // Progress bar is already finished.  Do not need to do anything.
        if self.is_finished() {
            return;
        }

        self.finish_using_style();
    }
}

#[derive(Debug)]
pub(crate) struct MultiProgressState {
    /// The collection of states corresponding to progress bars
    /// the state is None for bars that have not yet been drawn or have been removed
    pub(crate) draw_states: Vec<Option<ProgressDrawState>>,
    /// Set of removed bars, should have corresponding `None` elements in the `draw_states` vector
    pub(crate) free_set: Vec<usize>,
    /// Indices to the `draw_states` to maintain correct visual order
    pub(crate) ordering: Vec<usize>,
    /// Target for draw operation for MultiProgress
    pub(crate) draw_target: ProgressDrawTarget,
    /// Whether or not to just move cursor instead of clearing lines
    pub(crate) move_cursor: bool,
}

impl MultiProgressState {
    fn width(&self) -> usize {
        self.draw_target.width()
    }

    pub(crate) fn draw(&mut self, idx: usize, draw_state: ProgressDrawState) -> io::Result<()> {
        let force_draw = draw_state.finished || draw_state.force_draw;
        let mut orphan_lines = vec![];

        // Split orphan lines out of the draw state, if any
        let lines = if draw_state.orphan_lines > 0 {
            let split = draw_state.lines.split_at(draw_state.orphan_lines);
            orphan_lines.extend_from_slice(split.0);
            split.1.to_vec()
        } else {
            draw_state.lines
        };

        let draw_state = ProgressDrawState {
            lines,
            orphan_lines: 0,
            ..draw_state
        };

        self.draw_states[idx] = Some(draw_state);

        // the rest from here is only drawing, we can skip it.
        if self.draw_target.is_hidden() {
            return Ok(());
        }

        let mut lines = vec![];

        // Make orphaned lines appear at the top, so they can be properly
        // forgotten.
        let orphan_lines_count = orphan_lines.len();
        lines.append(&mut orphan_lines);

        for index in self.ordering.iter() {
            let draw_state = &self.draw_states[*index];
            if let Some(ref draw_state) = draw_state {
                lines.extend_from_slice(&draw_state.lines[..]);
            }
        }

        // !any(!done) is also true when iter() is empty, contrary to all(done)
        let finished = !self
            .draw_states
            .iter()
            .any(|ref x| !x.as_ref().map(|s| s.finished).unwrap_or(false));
        self.draw_target.apply_draw_state(ProgressDrawState {
            lines,
            orphan_lines: orphan_lines_count,
            force_draw: force_draw || orphan_lines_count > 0,
            move_cursor: self.move_cursor,
            finished,
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.draw_states.len() - self.free_set.len()
    }

    pub(crate) fn remove_idx(&mut self, idx: usize) {
        if self.free_set.contains(&idx) {
            return;
        }

        self.draw_states[idx].take();
        self.free_set.push(idx);
        self.ordering.retain(|&x| x != idx);

        assert!(
            self.len() == self.ordering.len(),
            "Draw state is inconsistent"
        );
    }
}

/// The drawn state of an element.
#[derive(Clone, Debug)]
pub(crate) struct ProgressDrawState {
    /// The lines to print (can contain ANSI codes)
    pub lines: Vec<String>,
    /// The number of lines that shouldn't be reaped by the next tick.
    pub orphan_lines: usize,
    /// True if the bar no longer needs drawing.
    pub finished: bool,
    /// True if drawing should be forced.
    pub force_draw: bool,
    /// True if we should move the cursor up when possible instead of clearing lines.
    pub move_cursor: bool,
}

impl ProgressDrawState {
    pub fn draw_to_term(&self, term: &Term) -> io::Result<()> {
        for line in &self.lines {
            term.write_line(line)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) enum Status {
    InProgress,
    DoneVisible,
    DoneHidden,
}

/// Target for draw operations
///
/// This tells a progress bar or a multi progress object where to paint to.
/// The draw target is a stateful wrapper over a drawing destination and
/// internally optimizes how often the state is painted to the output
/// device.
#[derive(Debug)]
pub struct ProgressDrawTarget {
    pub(crate) kind: ProgressDrawTargetKind,
}

impl ProgressDrawTarget {
    /// Draw to a buffered stdout terminal at a max of 15 times a second.
    ///
    /// For more information see `ProgressDrawTarget::to_term`.
    pub fn stdout() -> ProgressDrawTarget {
        ProgressDrawTarget::term(Term::buffered_stdout(), 15)
    }

    /// Draw to a buffered stderr terminal at a max of 15 times a second.
    ///
    /// This is the default draw target for progress bars.  For more
    /// information see `ProgressDrawTarget::to_term`.
    pub fn stderr() -> ProgressDrawTarget {
        ProgressDrawTarget::term(Term::buffered_stderr(), 15)
    }

    /// Draw to a buffered stdout terminal at a max of `refresh_rate` times a second.
    ///
    /// For more information see `ProgressDrawTarget::to_term`.
    pub fn stdout_with_hz(refresh_rate: u64) -> ProgressDrawTarget {
        ProgressDrawTarget::term(Term::buffered_stdout(), refresh_rate)
    }

    /// Draw to a buffered stderr terminal at a max of `refresh_rate` times a second.
    ///
    /// For more information see `ProgressDrawTarget::to_term`.
    pub fn stderr_with_hz(refresh_rate: u64) -> ProgressDrawTarget {
        ProgressDrawTarget::term(Term::buffered_stderr(), refresh_rate)
    }

    /// Draw to a buffered stdout terminal without max framerate.
    ///
    /// This is useful when data is known to come in very slowly and
    /// not rendering some updates would be a problem (for instance
    /// when messages are used extensively).
    ///
    /// For more information see `ProgressDrawTarget::to_term`.
    pub fn stdout_nohz() -> ProgressDrawTarget {
        ProgressDrawTarget::term(Term::buffered_stdout(), None)
    }

    /// Draw to a buffered stderr terminal without max framerate.
    ///
    /// This is useful when data is known to come in very slowly and
    /// not rendering some updates would be a problem (for instance
    /// when messages are used extensively).
    ///
    /// For more information see `ProgressDrawTarget::to_term`.
    pub fn stderr_nohz() -> ProgressDrawTarget {
        ProgressDrawTarget::term(Term::buffered_stderr(), None)
    }

    /// Draw to a terminal, optionally with a specific refresh rate.
    ///
    /// Progress bars are by default drawn to terminals however if the
    /// terminal is not user attended the entire progress bar will be
    /// hidden.  This is done so that piping to a file will not produce
    /// useless escape codes in that file.
    ///
    /// Will panic if refresh_rate is `Some(0)`. To disable rate limiting use `None` instead.
    #[allow(clippy::wrong_self_convention)]
    #[deprecated(since = "0.16.0", note = "Use `ProgressDrawTarget::term` instead")]
    pub fn to_term(term: Term, refresh_rate: impl Into<Option<u64>>) -> ProgressDrawTarget {
        ProgressDrawTarget::term(term, refresh_rate)
    }

    /// Draw to a terminal, optionally with a specific refresh rate.
    ///
    /// Progress bars are by default drawn to terminals however if the
    /// terminal is not user attended the entire progress bar will be
    /// hidden.  This is done so that piping to a file will not produce
    /// useless escape codes in that file.
    ///
    /// Will panic if refresh_rate is `Some(0)`. To disable rate limiting use `None` instead.
    pub fn term(term: Term, refresh_rate: impl Into<Option<u64>>) -> ProgressDrawTarget {
        let rate = refresh_rate
            .into()
            .map(|x| Duration::from_millis(1000 / x))
            .unwrap_or_else(|| Duration::from_secs(0));
        ProgressDrawTarget {
            kind: ProgressDrawTargetKind::Term {
                term,
                last_line_count: 0,
                rate,
                last_draw: Instant::now() - rate,
            },
        }
    }

    /// A hidden draw target.
    ///
    /// This forces a progress bar to be not rendered at all.
    pub fn hidden() -> ProgressDrawTarget {
        ProgressDrawTarget {
            kind: ProgressDrawTargetKind::Hidden,
        }
    }

    /// Returns true if the draw target is hidden.
    ///
    /// This is internally used in progress bars to figure out if overhead
    /// from drawing can be prevented.
    pub fn is_hidden(&self) -> bool {
        match self.kind {
            ProgressDrawTargetKind::Hidden => true,
            ProgressDrawTargetKind::Term { ref term, .. } => !term.is_term(),
            _ => false,
        }
    }

    /// Returns the current width of the draw target.
    fn width(&self) -> usize {
        match self.kind {
            ProgressDrawTargetKind::Term { ref term, .. } => term.size().1 as usize,
            ProgressDrawTargetKind::Remote { ref state, .. } => state.read().unwrap().width(),
            ProgressDrawTargetKind::Hidden => 0,
        }
    }

    /// Apply the given draw state (draws it).
    pub(crate) fn apply_draw_state(&mut self, draw_state: ProgressDrawState) -> io::Result<()> {
        let (term, last_line_count, last_draw) = match self.kind {
            ProgressDrawTargetKind::Term {
                ref term,
                ref mut last_line_count,
                rate,
                ref mut last_draw,
            } if draw_state.finished || draw_state.force_draw || last_draw.elapsed() > rate => {
                (term, last_line_count, last_draw)
            }
            ProgressDrawTargetKind::Remote { idx, ref state, .. } => {
                return state
                    .write()
                    .unwrap()
                    .draw(idx, draw_state)
                    .map_err(|e| io::Error::new(io::ErrorKind::Other, e));
            }
            // Hidden, finished, or no need to refresh yet
            _ => return Ok(()),
        };

        if !draw_state.lines.is_empty() && draw_state.move_cursor {
            term.move_cursor_up(*last_line_count)?;
        } else {
            term.clear_last_lines(*last_line_count)?;
        }

        draw_state.draw_to_term(term)?;
        term.flush()?;
        *last_line_count = draw_state.lines.len() - draw_state.orphan_lines;
        *last_draw = Instant::now();
        Ok(())
    }

    /// Properly disconnects from the draw target
    pub(crate) fn disconnect(&self) {
        match self.kind {
            ProgressDrawTargetKind::Term { .. } => {}
            ProgressDrawTargetKind::Remote { idx, ref state, .. } => {
                state
                    .write()
                    .unwrap()
                    .draw(
                        idx,
                        ProgressDrawState {
                            lines: vec![],
                            orphan_lines: 0,
                            finished: true,
                            force_draw: false,
                            move_cursor: false,
                        },
                    )
                    .ok();
            }
            ProgressDrawTargetKind::Hidden => {}
        };
    }
}
#[derive(Debug)]
pub(crate) enum ProgressDrawTargetKind {
    Term {
        term: Term,
        last_line_count: usize,
        rate: Duration,
        last_draw: Instant,
    },
    Remote {
        state: Arc<RwLock<MultiProgressState>>,
        idx: usize,
    },
    Hidden,
}
