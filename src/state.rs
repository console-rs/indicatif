use std::borrow::Cow;
use std::fmt;
use std::io;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::style::{ProgressFinish, ProgressStyle};
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
    pub(crate) fn new(len: u64, draw_target: ProgressDrawTarget) -> Self {
        ProgressState {
            style: ProgressStyle::default_bar(),
            draw_target,
            message: "".into(),
            prefix: "".into(),
            pos: 0,
            len,
            tick: 0,
            draw_delta: 0,
            draw_rate: 0,
            draw_next: 0,
            status: Status::InProgress,
            started: Instant::now(),
            est: Estimate::new(),
            tick_thread: None,
            steady_tick: 0,
        }
    }

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

    /// The expected ETA
    pub fn eta(&self) -> Duration {
        if self.len == !0 || self.is_finished() {
            return Duration::new(0, 0);
        }
        let t = self.est.seconds_per_step();
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
        let avg_time = self.est.seconds_per_step();
        if avg_time == 0.0 {
            0
        } else {
            (1.0 / avg_time) as u64
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
            self.est.record_step(new_pos, Instant::now());
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
        ProgressDrawTarget {
            kind: ProgressDrawTargetKind::Term {
                term,
                last_line_count: 0,
                leaky_bucket: refresh_rate.into().map(|rate| LeakyBucket {
                    bucket: MAX_GROUP_SIZE,
                    leak_rate: rate as f64,
                    last_update: Instant::now(),
                }),
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
        let (term, last_line_count) = match self.kind {
            ProgressDrawTargetKind::Term {
                ref term,
                ref mut last_line_count,
                leaky_bucket: None,
            } => (term, last_line_count),
            ProgressDrawTargetKind::Term {
                ref term,
                ref mut last_line_count,
                leaky_bucket: Some(ref mut leaky_bucket),
            } => {
                if draw_state.finished || draw_state.force_draw || leaky_bucket.try_add_work() {
                    (term, last_line_count)
                } else {
                    // rate limited
                    return Ok(());
                }
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
        leaky_bucket: Option<LeakyBucket>,
    },
    Remote {
        state: Arc<RwLock<MultiProgressState>>,
        idx: usize,
    },
    Hidden,
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

/// Ring buffer with constant capacity. Used by `ProgressBar`s to display `{eta}`, `{eta_precise}`,
/// and `{*_per_sec}`.
pub(crate) struct Estimate {
    buf: Box<[f64; 15]>,
    /// Lower 4 bits signify the current length, meaning how many values of `buf` are actually
    /// meaningful (and not just set to 0 by initialization).
    ///
    /// The upper 4 bits signify the last used index in the `buf`. The estimate is currently
    /// implemented as a ring buffer and when recording a new step the oldest value is overwritten.
    /// Last index is the most recently used position, and as elements are always stored with
    /// insertion order, `last_index + 1` is the least recently used position and is the first
    /// to be overwritten.
    data: u8,
    start_time: Instant,
    start_value: u64,
}

impl Estimate {
    fn len(&self) -> u8 {
        self.data & 0x0F
    }

    fn set_len(&mut self, len: u8) {
        // Sanity check to make sure math is correct as otherwise it could result in unexpected bugs
        debug_assert!(len < 16);
        self.data = (self.data & 0xF0) | len;
    }

    fn last_idx(&self) -> u8 {
        (self.data & 0xF0) >> 4
    }

    fn set_last_idx(&mut self, last_idx: u8) {
        // This will wrap last_idx on overflow (setting to 16 will result in 0); this is fine
        // because Estimate::buf is 15 elements long and this is a ring buffer, so overwriting
        // the oldest value is correct
        self.data = ((last_idx & 0x0F) << 4) | (self.data & 0x0F);
    }

    fn new() -> Self {
        let this = Self {
            buf: Box::new([0.0; 15]),
            data: 0,
            start_time: Instant::now(),
            start_value: 0,
        };
        // Make sure not to break anything accidentally as self.data can't handle bufs longer than
        // 15 elements (not enough space in a u8)
        debug_assert!(this.buf.len() < 16);
        this
    }

    pub(crate) fn reset(&mut self, start_value: u64) {
        self.start_time = Instant::now();
        self.start_value = start_value;
        self.data = 0;
    }

    fn record_step(&mut self, value: u64, current_time: Instant) {
        let elapsed = current_time - self.start_time;
        let item = {
            let divisor = value.saturating_sub(self.start_value) as f64;
            if divisor == 0.0 {
                0.0
            } else {
                duration_to_secs(elapsed) / divisor
            }
        };

        self.push(item);
    }

    /// Adds the `value` into the buffer, overwriting the oldest one if full, or increasing length
    /// by 1 and appending otherwise.
    fn push(&mut self, value: f64) {
        let len = self.len();
        let last_idx = self.last_idx();

        if self.buf.len() > usize::from(len) {
            // Buffer isn't yet full, increase it's size
            self.set_len(len + 1);
            self.buf[usize::from(last_idx)] = value;
        } else {
            // Buffer is full, overwrite the oldest value
            let idx = last_idx % len;
            self.buf[usize::from(idx)] = value;
        }

        self.set_last_idx(last_idx + 1);
    }

    /// Average time per step in seconds, using rolling buffer of last 15 steps
    fn seconds_per_step(&self) -> f64 {
        let len = self.len();
        self.buf[0..usize::from(len)].iter().sum::<f64>() / f64::from(len)
    }
}

impl fmt::Debug for Estimate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Estimate")
            .field("buf", &self.buf)
            .field("len", &self.len())
            .field("last_idx", &self.last_idx())
            .field("start_time", &self.start_time)
            .field("start_value", &self.start_value)
            .finish()
    }
}

#[derive(Debug)]
pub(crate) struct LeakyBucket {
    leak_rate: f64,
    last_update: Instant,
    bucket: f64,
}

/// Rate limit but allow occasional bursts above desired rate
impl LeakyBucket {
    /// try to add some work to the bucket
    /// return false if the bucket is already full and the work should be skipped
    fn try_add_work(&mut self) -> bool {
        self.leak();
        if self.bucket < MAX_GROUP_SIZE {
            self.bucket += 1.0;
            true
        } else {
            false
        }
    }

    fn leak(&mut self) {
        let ticks = self.last_update.elapsed().as_secs_f64() * self.leak_rate;
        self.bucket -= ticks;
        if self.bucket < 0.0 {
            self.bucket = 0.0;
        }
        self.last_update = Instant::now();
    }
}

const MAX_GROUP_SIZE: f64 = 32.0;

pub fn duration_to_secs(d: Duration) -> f64 {
    d.as_secs() as f64 + f64::from(d.subsec_nanos()) / 1_000_000_000f64
}

pub fn secs_to_duration(s: f64) -> Duration {
    let secs = s.trunc() as u64;
    let nanos = (s.fract() * 1_000_000_000f64) as u32;
    Duration::new(secs, nanos)
}

#[derive(Debug)]
pub(crate) enum Status {
    InProgress,
    DoneVisible,
    DoneHidden,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_per_step() {
        let test_rate = |items_per_second| {
            let mut est = Estimate::new();
            let mut current_time = est.start_time;
            let mut current_value = 0;
            for _ in 0..est.buf.len() {
                current_value += items_per_second;
                current_time += Duration::from_secs(1);
                est.record_step(current_value, current_time);
            }
            let avg_seconds_per_step = est.seconds_per_step();

            assert!(avg_seconds_per_step > 0.0);
            assert!(avg_seconds_per_step.is_finite());

            let expected_rate = 1.0 / items_per_second as f64;
            let absolute_error = (avg_seconds_per_step - expected_rate).abs();
            assert!(
                absolute_error < f64::EPSILON,
                "Expected rate: {}, actual: {}, absolute error: {}",
                expected_rate,
                avg_seconds_per_step,
                absolute_error
            );
        };

        test_rate(1);
        test_rate(1_000);
        test_rate(1_000_000);
        test_rate(1_000_000_000);
        test_rate(1_000_000_001);
        test_rate(100_000_000_000);
        test_rate(1_000_000_000_000);
        test_rate(100_000_000_000_000);
        test_rate(1_000_000_000_000_000);
    }

    #[test]
    fn test_duration_stuff() {
        let duration = Duration::new(42, 100_000_000);
        let secs = duration_to_secs(duration);
        assert_eq!(secs_to_duration(secs), duration);
    }
}
