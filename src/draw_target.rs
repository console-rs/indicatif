use std::io;
use std::sync::{Arc, RwLock, RwLockWriteGuard};
use std::thread::panicking;
use std::time::Duration;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use console::Term;
#[cfg(target_arch = "wasm32")]
use instant::Instant;

use crate::multi::{MultiProgressAlignment, MultiState};
use crate::TermLike;

/// Target for draw operations
///
/// This tells a progress bar or a multi progress object where to paint to.
/// The draw target is a stateful wrapper over a drawing destination and
/// internally optimizes how often the state is painted to the output
/// device.
#[derive(Debug)]
pub struct ProgressDrawTarget {
    kind: TargetKind,
}

impl ProgressDrawTarget {
    /// Draw to a buffered stdout terminal at a max of 20 times a second.
    ///
    /// For more information see [`ProgressDrawTarget::term`].
    pub fn stdout() -> Self {
        Self::term(Term::buffered_stdout(), 20)
    }

    /// Draw to a buffered stderr terminal at a max of 20 times a second.
    ///
    /// This is the default draw target for progress bars.  For more
    /// information see [`ProgressDrawTarget::term`].
    pub fn stderr() -> Self {
        Self::term(Term::buffered_stderr(), 20)
    }

    /// Draw to a buffered stdout terminal at a max of `refresh_rate` times a second.
    ///
    /// For more information see [`ProgressDrawTarget::term`].
    pub fn stdout_with_hz(refresh_rate: u8) -> Self {
        Self::term(Term::buffered_stdout(), refresh_rate)
    }

    /// Draw to a buffered stderr terminal at a max of `refresh_rate` times a second.
    ///
    /// For more information see [`ProgressDrawTarget::term`].
    pub fn stderr_with_hz(refresh_rate: u8) -> Self {
        Self::term(Term::buffered_stderr(), refresh_rate)
    }

    pub(crate) fn new_remote(state: Arc<RwLock<MultiState>>, idx: usize) -> Self {
        Self {
            kind: TargetKind::Multi { state, idx },
        }
    }

    /// Draw to a terminal, with a specific refresh rate.
    ///
    /// Progress bars are by default drawn to terminals however if the
    /// terminal is not user attended the entire progress bar will be
    /// hidden.  This is done so that piping to a file will not produce
    /// useless escape codes in that file.
    ///
    /// Will panic if refresh_rate is `0`.
    pub fn term(term: Term, refresh_rate: u8) -> Self {
        Self {
            kind: TargetKind::Term {
                term,
                last_line_count: 0,
                rate_limiter: RateLimiter::new(refresh_rate),
                draw_state: DrawState::default(),
            },
        }
    }

    /// Draw to a boxed object that implements the [`TermLike`] trait.
    pub fn term_like(term_like: Box<dyn TermLike>) -> Self {
        Self {
            kind: TargetKind::TermLike {
                inner: term_like,
                last_line_count: 0,
                rate_limiter: None,
                draw_state: DrawState::default(),
            },
        }
    }

    /// Draw to a boxed object that implements the [`TermLike`] trait,
    /// with a specific refresh rate.
    pub fn term_like_with_hz(term_like: Box<dyn TermLike>, refresh_rate: u8) -> Self {
        Self {
            kind: TargetKind::TermLike {
                inner: term_like,
                last_line_count: 0,
                rate_limiter: Option::from(RateLimiter::new(refresh_rate)),
                draw_state: DrawState::default(),
            },
        }
    }

    /// A hidden draw target.
    ///
    /// This forces a progress bar to be not rendered at all.
    pub fn hidden() -> Self {
        Self {
            kind: TargetKind::Hidden,
        }
    }

    /// Returns true if the draw target is hidden.
    ///
    /// This is internally used in progress bars to figure out if overhead
    /// from drawing can be prevented.
    pub fn is_hidden(&self) -> bool {
        match self.kind {
            TargetKind::Hidden => true,
            TargetKind::Term { ref term, .. } => !term.is_term(),
            TargetKind::Multi { ref state, .. } => state.read().unwrap().is_hidden(),
            _ => false,
        }
    }

    /// Returns the current width of the draw target.
    pub(crate) fn width(&self) -> u16 {
        match self.kind {
            TargetKind::Term { ref term, .. } => term.size().1,
            TargetKind::Multi { ref state, .. } => state.read().unwrap().width(),
            TargetKind::Hidden => 0,
            TargetKind::TermLike { ref inner, .. } => inner.width(),
        }
    }

    /// Notifies the backing `MultiProgress` (if applicable) that the associated progress bar should
    /// be marked a zombie.
    pub(crate) fn mark_zombie(&self) {
        if let TargetKind::Multi { idx, state } = &self.kind {
            state.write().unwrap().mark_zombie(*idx);
        }
    }

    /// Apply the given draw state (draws it).
    pub(crate) fn drawable(&mut self, force_draw: bool, now: Instant) -> Option<Drawable<'_>> {
        match &mut self.kind {
            TargetKind::Term {
                term,
                last_line_count,
                rate_limiter,
                draw_state,
            } => {
                if !term.is_term() {
                    return None;
                }

                match force_draw || rate_limiter.allow(now) {
                    true => Some(Drawable::Term {
                        term,
                        last_line_count,
                        draw_state,
                    }),
                    false => None, // rate limited
                }
            }
            TargetKind::Multi { idx, state, .. } => {
                let state = state.write().unwrap();
                Some(Drawable::Multi {
                    idx: *idx,
                    state,
                    force_draw,
                    now,
                })
            }
            TargetKind::TermLike {
                inner,
                last_line_count,
                rate_limiter,
                draw_state,
            } => match force_draw || rate_limiter.as_mut().map_or(true, |r| r.allow(now)) {
                true => Some(Drawable::TermLike {
                    term_like: &**inner,
                    last_line_count,
                    draw_state,
                }),
                false => None, // rate limited
            },
            // Hidden, finished, or no need to refresh yet
            _ => None,
        }
    }

    /// Properly disconnects from the draw target
    pub(crate) fn disconnect(&self, now: Instant) {
        match self.kind {
            TargetKind::Term { .. } => {}
            TargetKind::Multi { idx, ref state, .. } => {
                let state = state.write().unwrap();
                let _ = Drawable::Multi {
                    state,
                    idx,
                    force_draw: true,
                    now,
                }
                .clear();
            }
            TargetKind::Hidden => {}
            TargetKind::TermLike { .. } => {}
        };
    }

    pub(crate) fn remote(&self) -> Option<(&Arc<RwLock<MultiState>>, usize)> {
        match &self.kind {
            TargetKind::Multi { state, idx } => Some((state, *idx)),
            _ => None,
        }
    }

    pub(crate) fn adjust_last_line_count(&mut self, adjust: LineAdjust) {
        self.kind.adjust_last_line_count(adjust);
    }
}

#[derive(Debug)]
enum TargetKind {
    Term {
        term: Term,
        last_line_count: usize,
        rate_limiter: RateLimiter,
        draw_state: DrawState,
    },
    Multi {
        state: Arc<RwLock<MultiState>>,
        idx: usize,
    },
    Hidden,
    TermLike {
        inner: Box<dyn TermLike>,
        last_line_count: usize,
        rate_limiter: Option<RateLimiter>,
        draw_state: DrawState,
    },
}

impl TargetKind {
    /// Adjust `last_line_count` such that the next draw operation keeps/clears additional lines
    fn adjust_last_line_count(&mut self, adjust: LineAdjust) {
        let last_line_count: &mut usize = match self {
            Self::Term {
                last_line_count, ..
            } => last_line_count,
            Self::TermLike {
                last_line_count, ..
            } => last_line_count,
            _ => return,
        };

        match adjust {
            LineAdjust::Clear(count) => *last_line_count = last_line_count.saturating_add(count),
            LineAdjust::Keep(count) => *last_line_count = last_line_count.saturating_sub(count),
        }
    }
}

pub(crate) enum Drawable<'a> {
    Term {
        term: &'a Term,
        last_line_count: &'a mut usize,
        draw_state: &'a mut DrawState,
    },
    Multi {
        state: RwLockWriteGuard<'a, MultiState>,
        idx: usize,
        force_draw: bool,
        now: Instant,
    },
    TermLike {
        term_like: &'a dyn TermLike,
        last_line_count: &'a mut usize,
        draw_state: &'a mut DrawState,
    },
}

impl<'a> Drawable<'a> {
    /// Adjust `last_line_count` such that the next draw operation keeps/clears additional lines
    pub(crate) fn adjust_last_line_count(&mut self, adjust: LineAdjust) {
        let last_line_count: &mut usize = match self {
            Drawable::Term {
                last_line_count, ..
            } => last_line_count,
            Drawable::TermLike {
                last_line_count, ..
            } => last_line_count,
            _ => return,
        };

        match adjust {
            LineAdjust::Clear(count) => *last_line_count = last_line_count.saturating_add(count),
            LineAdjust::Keep(count) => *last_line_count = last_line_count.saturating_sub(count),
        }
    }

    pub(crate) fn state(&mut self) -> DrawStateWrapper<'_> {
        let mut state = match self {
            Drawable::Term { draw_state, .. } => DrawStateWrapper::for_term(draw_state),
            Drawable::Multi { state, idx, .. } => state.draw_state(*idx),
            Drawable::TermLike { draw_state, .. } => DrawStateWrapper::for_term(draw_state),
        };

        state.reset();
        state
    }

    pub(crate) fn clear(mut self) -> io::Result<()> {
        let state = self.state();
        drop(state);
        self.draw()
    }

    pub(crate) fn draw(self) -> io::Result<()> {
        match self {
            Drawable::Term {
                term,
                last_line_count,
                draw_state,
            } => draw_state.draw_to_term(term, last_line_count),
            Drawable::Multi {
                mut state,
                force_draw,
                now,
                ..
            } => state.draw(force_draw, None, now),
            Drawable::TermLike {
                term_like,
                last_line_count,
                draw_state,
            } => draw_state.draw_to_term(term_like, last_line_count),
        }
    }
}

pub(crate) enum LineAdjust {
    /// Adds to `last_line_count` so that the next draw also clears those lines
    Clear(usize),
    /// Subtracts from `last_line_count` so that the next draw retains those lines
    Keep(usize),
}

pub(crate) struct DrawStateWrapper<'a> {
    state: &'a mut DrawState,
    orphan_lines: Option<&'a mut Vec<String>>,
}

impl<'a> DrawStateWrapper<'a> {
    pub(crate) fn for_term(state: &'a mut DrawState) -> Self {
        Self {
            state,
            orphan_lines: None,
        }
    }

    pub(crate) fn for_multi(state: &'a mut DrawState, orphan_lines: &'a mut Vec<String>) -> Self {
        Self {
            state,
            orphan_lines: Some(orphan_lines),
        }
    }
}

impl std::ops::Deref for DrawStateWrapper<'_> {
    type Target = DrawState;

    fn deref(&self) -> &Self::Target {
        self.state
    }
}

impl std::ops::DerefMut for DrawStateWrapper<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.state
    }
}

impl Drop for DrawStateWrapper<'_> {
    fn drop(&mut self) {
        if let Some(orphaned) = &mut self.orphan_lines {
            orphaned.extend(self.state.lines.drain(..self.state.orphan_lines_count));
            self.state.orphan_lines_count = 0;
        }
    }
}

#[derive(Debug)]
struct RateLimiter {
    interval: u16, // in milliseconds
    capacity: u8,
    prev: Instant,
}

/// Rate limit but allow occasional bursts above desired rate
impl RateLimiter {
    fn new(rate: u8) -> Self {
        Self {
            interval: 1000 / (rate as u16), // between 3 and 1000 milliseconds
            capacity: MAX_BURST,
            prev: Instant::now(),
        }
    }

    fn allow(&mut self, now: Instant) -> bool {
        if now < self.prev {
            return false;
        }

        let elapsed = now - self.prev;
        // If `capacity` is 0 and not enough time (`self.interval` ms) has passed since
        // `self.prev` to add new capacity, return `false`. The goal of this method is to
        // make this decision as efficient as possible.
        if self.capacity == 0 && elapsed < Duration::from_millis(self.interval as u64) {
            return false;
        }

        // We now calculate `new`, the number of ms, since we last returned `true`,
        // and `remainder`, which represents a number of ns less than 1ms which we cannot
        // convert into capacity now, so we're saving it for later.
        let (new, remainder) = (
            elapsed.as_millis() / self.interval as u128,
            elapsed.as_nanos() % (self.interval as u128 * 1_000_000),
        );

        // We add `new` to `capacity`, subtract one for returning `true` from here,
        // then make sure it does not exceed a maximum of `MAX_BURST`, then store it.
        self.capacity = Ord::min(MAX_BURST as u128, (self.capacity as u128) + new - 1) as u8;
        // Store `prev` for the next iteration after subtracting the `remainder`.
        // Just use `unwrap` here because it shouldn't be possible for this to underflow.
        self.prev = now
            .checked_sub(Duration::from_nanos(remainder as u64))
            .unwrap();
        true
    }
}

const MAX_BURST: u8 = 20;

/// The drawn state of an element.
#[derive(Clone, Debug, Default)]
pub(crate) struct DrawState {
    /// The lines to print (can contain ANSI codes)
    pub(crate) lines: Vec<String>,
    /// The number of lines that shouldn't be reaped by the next tick.
    pub(crate) orphan_lines_count: usize,
    /// True if we should move the cursor up when possible instead of clearing lines.
    pub(crate) move_cursor: bool,
    /// Controls how the multi progress is aligned if some of its progress bars get removed, default is `Top`
    pub(crate) alignment: MultiProgressAlignment,
}

impl DrawState {
    fn draw_to_term(
        &mut self,
        term: &(impl TermLike + ?Sized),
        last_line_count: &mut usize,
    ) -> io::Result<()> {
        if panicking() {
            return Ok(());
        }

        if !self.lines.is_empty() && self.move_cursor {
            term.move_cursor_up(*last_line_count)?;
        } else {
            // Fork of console::clear_last_lines that assumes that the last line doesn't contain a '\n'
            let n = *last_line_count;
            term.move_cursor_up(n.saturating_sub(1))?;
            for i in 0..n {
                term.clear_line()?;
                if i + 1 != n {
                    term.move_cursor_down(1)?;
                }
            }
            term.move_cursor_up(n.saturating_sub(1))?;
        }

        let shift = match self.alignment {
            MultiProgressAlignment::Bottom if self.lines.len() < *last_line_count => {
                let shift = *last_line_count - self.lines.len();
                for _ in 0..shift {
                    term.write_line("")?;
                }
                shift
            }
            _ => 0,
        };

        let term_height = term.height() as usize;
        let term_width = term.width() as usize;
        let len = self.lines.len();
        let mut real_len = 0;
        let mut last_line_filler = 0;
        for (idx, line) in self.lines.iter().enumerate() {
            let line_width = console::measure_text_width(line);
            let diff = if line.is_empty() {
                // Empty line are new line
                1
            } else {
                // Calculate real length based on terminal width
                // This take in account linewrap from terminal
                let terminal_len = (line_width as f64 / term_width as f64).ceil() as usize;

                // If the line is effectively empty (for example when it consists
                // solely of ANSI color code sequences, count it the same as a
                // new line. If the line is measured to be len = 0, we will
                // subtract with overflow later.
                usize::max(terminal_len, 1)
            };
            if real_len + diff > term_height {
                break;
            }
            real_len += diff;
            if idx != 0 {
                term.write_line("")?;
            }
            term.write_str(line)?;
            if idx + 1 == len {
                // Keep the cursor on the right terminal side
                // So that next user writes/prints will happen on the next line
                last_line_filler = term_width.saturating_sub(line_width);
            }
        }
        term.write_str(&" ".repeat(last_line_filler))?;

        term.flush()?;
        *last_line_count = real_len - self.orphan_lines_count + shift;
        Ok(())
    }

    fn reset(&mut self) {
        self.lines.clear();
        self.orphan_lines_count = 0;
    }
}

#[cfg(test)]
mod tests {
    use crate::{MultiProgress, ProgressBar, ProgressDrawTarget};

    #[test]
    fn multi_is_hidden() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());

        let pb = mp.add(ProgressBar::new(100));
        assert!(mp.is_hidden());
        assert!(pb.is_hidden());
    }
}
