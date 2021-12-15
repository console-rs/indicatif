use std::borrow::Cow;
use std::fmt;
use std::io;
use std::thread;
use std::time::{Duration, Instant};

use crate::draw_target::{ProgressDrawState, ProgressDrawTarget};
use crate::style::{ProgressFinish, ProgressStyle};

/// The state of a progress bar at a moment in time.
pub struct ProgressState {
    pub(crate) style: ProgressStyle,
    pub pos: u64,
    pub len: u64,
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
        self.style.current_tick_str(self)
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
        secs_to_duration(t * self.len.saturating_sub(self.pos) as f64)
    }

    /// The expected total duration (that is, elapsed time + expected ETA)
    pub fn duration(&self) -> Duration {
        if self.len == !0 || self.is_finished() {
            return Duration::new(0, 0);
        }
        self.started.elapsed() + self.eta()
    }

    /// The number of steps per second
    pub fn per_sec(&self) -> f64 {
        let per_sec = 1.0 / self.est.seconds_per_step();
        if per_sec.is_nan() {
            0.0
        } else {
            per_sec
        }
    }

    /// Call the provided `FnOnce` to update the state. Then redraw the
    /// progress bar if the state has changed.
    pub fn update_and_draw<F: FnOnce(&mut ProgressState)>(&mut self, f: F) {
        if self.update(f) {
            self.draw(false).ok();
        }
    }

    /// Call the provided `FnOnce` to update the state. Then unconditionally redraw the
    /// progress bar.
    pub fn update_and_force_draw<F: FnOnce(&mut ProgressState)>(&mut self, f: F) {
        self.update(|state| {
            state.draw_next = state.pos;
            f(state);
        });
        self.draw(true).ok();
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
                (self.per_sec() / self.draw_rate as f64) as u64
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

    pub(crate) fn draw(&mut self, force_draw: bool) -> io::Result<()> {
        // we can bail early if the draw target is hidden.
        if self.draw_target.is_hidden() {
            return Ok(());
        }

        let lines = match self.should_render() {
            true => self.style.format_state(self),
            false => Vec::new(),
        };

        // `|| self.is_finished()` should not be needed here, but we used to always for draw for
        // finished progress bar, so it's kept as to not cause compatibility issues in weird cases.
        let draw_state = ProgressDrawState::new(lines, force_draw || self.is_finished());
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
