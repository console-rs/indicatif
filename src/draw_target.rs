use std::io;
use std::sync::{Arc, RwLock, RwLockWriteGuard};
use std::time::{Duration, Instant};

use console::Term;

use crate::multi::{MultiProgressAlignment, MultiProgressState};
use crate::TermLike;

/// Target for draw operations
///
/// This tells a progress bar or a multi progress object where to paint to.
/// The draw target is a stateful wrapper over a drawing destination and
/// internally optimizes how often the state is painted to the output
/// device.
#[derive(Debug)]
pub struct ProgressDrawTarget {
    kind: ProgressDrawTargetKind,
}

impl ProgressDrawTarget {
    /// Draw to a buffered stdout terminal at a max of 15 times a second.
    ///
    /// For more information see `ProgressDrawTarget::to_term`.
    pub fn stdout() -> ProgressDrawTarget {
        ProgressDrawTarget::term(Term::buffered_stdout(), 20)
    }

    /// Draw to a buffered stderr terminal at a max of 15 times a second.
    ///
    /// This is the default draw target for progress bars.  For more
    /// information see `ProgressDrawTarget::to_term`.
    pub fn stderr() -> ProgressDrawTarget {
        ProgressDrawTarget::term(Term::buffered_stderr(), 20)
    }

    /// Draw to a buffered stdout terminal at a max of `refresh_rate` times a second.
    ///
    /// For more information see `ProgressDrawTarget::to_term`.
    pub fn stdout_with_hz(refresh_rate: u8) -> ProgressDrawTarget {
        ProgressDrawTarget::term(Term::buffered_stdout(), refresh_rate)
    }

    /// Draw to a buffered stderr terminal at a max of `refresh_rate` times a second.
    ///
    /// For more information see `ProgressDrawTarget::to_term`.
    pub fn stderr_with_hz(refresh_rate: u8) -> ProgressDrawTarget {
        ProgressDrawTarget::term(Term::buffered_stderr(), refresh_rate)
    }

    pub(crate) fn new_remote(state: Arc<RwLock<MultiProgressState>>, idx: usize) -> Self {
        Self {
            kind: ProgressDrawTargetKind::Remote { state, idx },
        }
    }

    /// Draw to a terminal, optionally with a specific refresh rate.
    ///
    /// Progress bars are by default drawn to terminals however if the
    /// terminal is not user attended the entire progress bar will be
    /// hidden.  This is done so that piping to a file will not produce
    /// useless escape codes in that file.
    ///
    /// Will panic if refresh_rate is `Some(0)`. To disable rate limiting use `None` instead.
    pub fn term(term: Term, refresh_rate: u8) -> ProgressDrawTarget {
        ProgressDrawTarget {
            kind: ProgressDrawTargetKind::Term {
                term,
                last_line_count: 0,
                rate_limiter: RateLimiter::new(refresh_rate),
                draw_state: ProgressDrawState::default(),
            },
        }
    }

    /// Draw to a boxed object that implements the [`TermLike`] trait.
    pub fn term_like(term_like: Box<dyn TermLike>) -> ProgressDrawTarget {
        ProgressDrawTarget {
            kind: ProgressDrawTargetKind::TermLike {
                inner: term_like,
                last_line_count: 0,
                draw_state: ProgressDrawState::default(),
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
    pub(crate) fn width(&self) -> u16 {
        match self.kind {
            ProgressDrawTargetKind::Term { ref term, .. } => term.size().1,
            ProgressDrawTargetKind::Remote { ref state, .. } => state.read().unwrap().width(),
            ProgressDrawTargetKind::Hidden => 0,
            ProgressDrawTargetKind::TermLike { ref inner, .. } => inner.width(),
        }
    }

    /// Apply the given draw state (draws it).
    pub(crate) fn drawable(&mut self, force_draw: bool, now: Instant) -> Option<Drawable<'_>> {
        match &mut self.kind {
            ProgressDrawTargetKind::Term {
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
            ProgressDrawTargetKind::Remote { idx, state, .. } => {
                let state = state.write().unwrap();
                Some(Drawable::Multi {
                    idx: *idx,
                    state,
                    force_draw,
                    now,
                })
            }
            ProgressDrawTargetKind::TermLike {
                inner,
                last_line_count,
                draw_state,
            } => Some(Drawable::TermLike {
                term_like: &**inner,
                last_line_count,
                draw_state,
            }),
            // Hidden, finished, or no need to refresh yet
            _ => None,
        }
    }

    /// Properly disconnects from the draw target
    pub(crate) fn disconnect(&self, now: Instant) {
        match self.kind {
            ProgressDrawTargetKind::Term { .. } => {}
            ProgressDrawTargetKind::Remote { idx, ref state, .. } => {
                let state = state.write().unwrap();
                let _ = Drawable::Multi {
                    state,
                    idx,
                    force_draw: true,
                    now,
                }
                .clear();
            }
            ProgressDrawTargetKind::Hidden => {}
            ProgressDrawTargetKind::TermLike { .. } => {}
        };
    }

    pub(crate) fn remote(&self) -> Option<(&Arc<RwLock<MultiProgressState>>, usize)> {
        match &self.kind {
            ProgressDrawTargetKind::Remote { state, idx } => Some((state, *idx)),
            _ => None,
        }
    }
}

#[derive(Debug)]
enum ProgressDrawTargetKind {
    Term {
        term: Term,
        last_line_count: usize,
        rate_limiter: RateLimiter,
        draw_state: ProgressDrawState,
    },
    Remote {
        state: Arc<RwLock<MultiProgressState>>,
        idx: usize,
    },
    Hidden,
    TermLike {
        inner: Box<dyn TermLike>,
        last_line_count: usize,
        draw_state: ProgressDrawState,
    },
}

pub(crate) enum Drawable<'a> {
    Term {
        term: &'a Term,
        last_line_count: &'a mut usize,
        draw_state: &'a mut ProgressDrawState,
    },
    Multi {
        state: RwLockWriteGuard<'a, MultiProgressState>,
        idx: usize,
        force_draw: bool,
        now: Instant,
    },
    TermLike {
        term_like: &'a dyn TermLike,
        last_line_count: &'a mut usize,
        draw_state: &'a mut ProgressDrawState,
    },
}

impl<'a> Drawable<'a> {
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
            } => state.draw(force_draw, now),
            Drawable::TermLike {
                term_like,
                last_line_count,
                draw_state,
            } => draw_state.draw_to_term(term_like, last_line_count),
        }
    }
}

pub(crate) struct DrawStateWrapper<'a> {
    state: &'a mut ProgressDrawState,
    orphan_lines: Option<&'a mut Vec<String>>,
}

impl<'a> DrawStateWrapper<'a> {
    pub(crate) fn for_term(state: &'a mut ProgressDrawState) -> Self {
        Self {
            state,
            orphan_lines: None,
        }
    }

    pub(crate) fn for_multi(
        state: &'a mut ProgressDrawState,
        orphan_lines: &'a mut Vec<String>,
    ) -> Self {
        Self {
            state,
            orphan_lines: Some(orphan_lines),
        }
    }
}

impl std::ops::Deref for DrawStateWrapper<'_> {
    type Target = ProgressDrawState;

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
            orphaned.extend(self.state.lines.drain(..self.state.orphan_lines));
            self.state.orphan_lines = 0;
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
        let elapsed = now - self.prev;
        let remaining = (MAX_BURST - self.capacity) as u128;
        self.capacity += Ord::min(remaining, elapsed.as_millis() / self.interval as u128) as u8;
        let interval_nanos = self.interval as u128 * 1_000_000;
        self.prev = now - Duration::from_nanos((elapsed.as_nanos() % interval_nanos) as u64);

        match self.capacity.checked_sub(1) {
            Some(new) => {
                self.capacity = new;
                true
            }
            None => false,
        }
    }
}

const MAX_BURST: u8 = 20;

/// The drawn state of an element.
#[derive(Clone, Debug, Default)]
pub(crate) struct ProgressDrawState {
    /// The lines to print (can contain ANSI codes)
    pub(crate) lines: Vec<String>,
    /// The number of lines that shouldn't be reaped by the next tick.
    pub(crate) orphan_lines: usize,
    /// True if we should move the cursor up when possible instead of clearing lines.
    pub(crate) move_cursor: bool,
    /// Controls how the multi progress is aligned if some of its progress bars get removed, default is `Top`
    pub(crate) alignment: MultiProgressAlignment,
}

impl ProgressDrawState {
    fn draw_to_term(
        &mut self,
        term: &(impl TermLike + ?Sized),
        last_line_count: &mut usize,
    ) -> io::Result<()> {
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

        let len = self.lines.len();
        for (idx, line) in self.lines.iter().enumerate() {
            if idx + 1 != len {
                term.write_line(line)?;
            } else {
                // Don't append a '\n' if this is the last line
                term.write_str(line)?;
                // Keep the cursor on the right terminal side
                // So that next user writes/prints will happen on the next line
                let term_width = term.width() as usize;
                let line_width = console::measure_text_width(line);
                term.write_str(&" ".repeat(term_width.saturating_sub(line_width)))?;
            }
        }

        term.flush()?;
        *last_line_count = self.lines.len() - self.orphan_lines + shift;
        Ok(())
    }

    fn reset(&mut self) {
        self.lines.clear();
        self.orphan_lines = 0;
    }
}
