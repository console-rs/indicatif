use std::borrow::Cow;
use std::fmt;
use std::io;
use std::thread;
use std::time::{Duration, Instant};

use crate::draw_target::ProgressDrawTarget;
use crate::style::ProgressStyle;

pub(crate) struct BarState {
    pub(crate) draw_target: ProgressDrawTarget,
    pub(crate) on_finish: ProgressFinish,
    pub(crate) style: ProgressStyle,
    pub(crate) state: ProgressState,
}

impl BarState {
    pub(crate) fn new(len: u64, draw_target: ProgressDrawTarget) -> Self {
        Self {
            draw_target,
            on_finish: ProgressFinish::default(),
            style: ProgressStyle::default_bar(),
            state: ProgressState::new(len),
        }
    }

    /// Finishes the progress bar using the [`ProgressFinish`] behavior stored
    /// in the [`ProgressStyle`].
    pub(crate) fn finish_using_style(&mut self, now: Instant, finish: ProgressFinish) {
        let (mut pos, mut status) = (None, Status::DoneVisible);
        match finish {
            ProgressFinish::AndLeave => pos = Some(self.state.len),
            ProgressFinish::WithMessage(msg) => {
                pos = Some(self.state.len);
                self.state.message = msg;
            }
            ProgressFinish::AndClear => {
                pos = Some(self.state.len);
                status = Status::DoneHidden;
            }
            ProgressFinish::Abandon => {}
            ProgressFinish::AbandonWithMessage(msg) => self.state.message = msg,
        }

        self.update(now, true, |state| {
            if let Some(pos) = pos {
                state.pos = pos;
            }
            state.status = status;
        });
    }

    /// Mutate the state, then draw if necessary
    pub(crate) fn update<F: FnOnce(&mut ProgressState)>(
        &mut self,
        now: Instant,
        force_draw: bool,
        f: F,
    ) {
        let updated = self.state.update(now, f);
        if force_draw || updated {
            self.draw(force_draw, now).ok();
        }
    }

    pub(crate) fn draw(&mut self, mut force_draw: bool, now: Instant) -> io::Result<()> {
        // we can bail early if the draw target is hidden.
        if self.draw_target.is_hidden() {
            return Ok(());
        }

        let width = self.draw_target.width();
        force_draw |= self.state.is_finished();
        let mut drawable = match self.draw_target.drawable(force_draw, now) {
            Some(drawable) => drawable,
            None => return Ok(()),
        };

        // `|| self.is_finished()` should not be needed here, but we used to always for draw for
        // finished progress bar, so it's kept as to not cause compatibility issues in weird cases.
        let mut draw_state = drawable.state();

        if self.state.should_render() {
            self.style
                .format_state(&self.state, &mut draw_state.lines, width);
        }

        drop(draw_state);
        drawable.draw()
    }
}

impl Drop for BarState {
    fn drop(&mut self) {
        // Progress bar is already finished.  Do not need to do anything.
        if self.state.is_finished() {
            return;
        }

        self.finish_using_style(Instant::now(), self.on_finish.clone());
    }
}

/// The state of a progress bar at a moment in time.
pub struct ProgressState {
    pub pos: u64,
    pub len: u64,
    pub(crate) tick: u64,
    pub(crate) started: Instant,
    pub(crate) message: Cow<'static, str>,
    pub(crate) prefix: Cow<'static, str>,
    pub(crate) status: Status,
    pub(crate) est: Estimator,
    pub(crate) tick_thread: Option<thread::JoinHandle<()>>,
    pub(crate) steady_tick: u64,
}

impl ProgressState {
    pub(crate) fn new(len: u64) -> Self {
        Self {
            message: "".into(),
            prefix: "".into(),
            pos: 0,
            len,
            tick: 0,
            status: Status::InProgress,
            started: Instant::now(),
            est: Estimator::new(Instant::now()),
            tick_thread: None,
            steady_tick: 0,
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
    pub(crate) fn should_render(&self) -> bool {
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
    pub(crate) fn message(&self) -> &str {
        &self.message
    }

    /// Returns the current prefix of the progress bar.
    pub(crate) fn prefix(&self) -> &str {
        &self.prefix
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
    pub(crate) fn duration(&self) -> Duration {
        if self.len == !0 || self.is_finished() {
            return Duration::new(0, 0);
        }
        self.started.elapsed() + self.eta()
    }

    /// The number of steps per second
    pub(crate) fn per_sec(&self) -> f64 {
        if matches!(&self.status, Status::InProgress) {
            let per_sec = 1.0 / self.est.seconds_per_step();
            if per_sec.is_nan() {
                0.0
            } else {
                per_sec
            }
        } else {
            self.len as f64 / self.started.elapsed().as_secs_f64()
        }
    }

    /// Call the provided `FnOnce` to update the state. If a draw should be run, returns `true`.
    pub(crate) fn update<F: FnOnce(&mut ProgressState)>(&mut self, now: Instant, f: F) -> bool {
        let old = (self.pos, self.tick);
        f(self);
        let new = (self.pos, self.tick);
        if new.0 != old.0 {
            self.est.record(new.0, now);
        }

        old != new
    }
}

/// Estimate the number of seconds per step
///
/// Ring buffer with constant capacity. Used by `ProgressBar`s to display `{eta}`,
/// `{eta_precise}`, and `{*_per_sec}`.
pub(crate) struct Estimator {
    steps: [f64; 16],
    pos: u8,
    full: bool,
    prev: (u64, Instant),
}

impl Estimator {
    fn new(now: Instant) -> Self {
        Self {
            steps: [0.0; 16],
            pos: 0,
            full: false,
            prev: (0, now),
        }
    }

    fn record(&mut self, value: u64, current_time: Instant) {
        let elapsed = current_time - self.prev.1;
        let divisor = value.saturating_sub(self.prev.0) as f64;
        let mut batch = 0.0;
        if divisor != 0.0 {
            batch = duration_to_secs(elapsed) / divisor;
        };

        self.steps[self.pos as usize] = batch;
        self.pos = (self.pos + 1) % 16;
        if !self.full && self.pos == 0 {
            self.full = true;
        }
    }

    pub(crate) fn reset(&mut self, start: u64, now: Instant) {
        self.pos = 0;
        self.full = false;
        self.prev = (start, now);
    }

    /// Average time per step in seconds, using rolling buffer of last 15 steps
    fn seconds_per_step(&self) -> f64 {
        let len = self.len();
        self.steps[0..len].iter().sum::<f64>() / len as f64
    }

    fn len(&self) -> usize {
        match self.full {
            true => 16,
            false => self.pos as usize,
        }
    }
}

impl fmt::Debug for Estimator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Estimate")
            .field("steps", &&self.steps[..self.len()])
            .field("prev", &self.prev)
            .finish()
    }
}

/// Behavior of a progress bar when it is finished
///
/// This is invoked when a [`ProgressBar`] or [`ProgressBarIter`] completes and
/// [`ProgressBar::is_finished`] is false.
///
/// [`ProgressBar`]: crate::ProgressBar
/// [`ProgressBarIter`]: crate::ProgressBarIter
/// [`ProgressBar::is_finished`]: crate::ProgressBar::is_finished
#[derive(Clone, Debug)]
pub enum ProgressFinish {
    /// Finishes the progress bar and leaves the current message
    ///
    /// Same behavior as calling [`ProgressBar::finish()`](crate::ProgressBar::finish).
    AndLeave,
    /// Finishes the progress bar and sets a message
    ///
    /// Same behavior as calling [`ProgressBar::finish_with_message()`](crate::ProgressBar::finish_with_message).
    WithMessage(Cow<'static, str>),
    /// Finishes the progress bar and completely clears it (this is the default)
    ///
    /// Same behavior as calling [`ProgressBar::finish_and_clear()`](crate::ProgressBar::finish_and_clear).
    AndClear,
    /// Finishes the progress bar and leaves the current message and progress
    ///
    /// Same behavior as calling [`ProgressBar::abandon()`](crate::ProgressBar::abandon).
    Abandon,
    /// Finishes the progress bar and sets a message, and leaves the current progress
    ///
    /// Same behavior as calling [`ProgressBar::abandon_with_message()`](crate::ProgressBar::abandon_with_message).
    AbandonWithMessage(Cow<'static, str>),
}

impl Default for ProgressFinish {
    fn default() -> Self {
        Self::AndClear
    }
}

fn duration_to_secs(d: Duration) -> f64 {
    d.as_secs() as f64 + f64::from(d.subsec_nanos()) / 1_000_000_000f64
}

fn secs_to_duration(s: f64) -> Duration {
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
            let mut est = Estimator::new(Instant::now());
            let mut current_time = est.prev.1;
            let mut current_value = 0;
            for _ in 0..est.steps.len() {
                current_value += items_per_second;
                current_time += Duration::from_secs(1);
                est.record(current_value, current_time);
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
