use std::io;
use std::sync::{Arc, RwLock, RwLockWriteGuard};
use std::time::Instant;

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
    pub fn term(term: Term, refresh_rate: impl Into<Option<u64>>) -> ProgressDrawTarget {
        ProgressDrawTarget {
            kind: ProgressDrawTargetKind::Term {
                term,
                last_line_count: 0,
                rate_limiter: refresh_rate.into().map(RateLimiter::new),
                draw_state: ProgressDrawState::new(Vec::new(), false),
            },
        }
    }

    /// Draw to a boxed object that implements the [`TermLike`] trait.
    pub fn term_like(term_like: Box<dyn TermLike>) -> ProgressDrawTarget {
        ProgressDrawTarget {
            kind: ProgressDrawTargetKind::TermLike {
                inner: term_like,
                last_line_count: 0,
                draw_state: ProgressDrawState::new(Vec::new(), false),
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
    pub(crate) fn width(&self) -> usize {
        match self.kind {
            ProgressDrawTargetKind::Term { ref term, .. } => term.size().1 as usize,
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
                let has_capacity = rate_limiter.as_mut().map(|b| b.allow(now)).unwrap_or(true);
                draw_state.force_draw = force_draw;
                match force_draw || has_capacity {
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
            } => {
                draw_state.force_draw = force_draw;
                Some(Drawable::TermLike {
                    term_like: &**inner,
                    last_line_count,
                    draw_state,
                })
            }
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
        rate_limiter: Option<RateLimiter>,
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
    leak_rate: f64,
    last_update: Instant,
    bucket: f64,
}

/// Rate limit but allow occasional bursts above desired rate
impl RateLimiter {
    fn new(leak_rate: u64) -> Self {
        Self {
            leak_rate: leak_rate as f64,
            last_update: Instant::now(),
            bucket: 0.0,
        }
    }

    /// try to add some work to the bucket
    /// return false if the bucket is already full and the work should be skipped
    fn allow(&mut self, now: Instant) -> bool {
        let ticks = (now - self.last_update).as_secs_f64() * self.leak_rate;
        self.bucket -= ticks;
        if self.bucket < 0.0 {
            self.bucket = 0.0;
        }
        self.last_update = now;

        if self.bucket < MAX_GROUP_SIZE {
            self.bucket += 1.0;
            true
        } else {
            false
        }
    }
}

const MAX_GROUP_SIZE: f64 = 32.0;

/// The drawn state of an element.
#[derive(Clone, Debug)]
pub(crate) struct ProgressDrawState {
    /// The lines to print (can contain ANSI codes)
    pub(crate) lines: Vec<String>,
    /// The number of lines that shouldn't be reaped by the next tick.
    pub(crate) orphan_lines: usize,
    /// True if drawing should be forced.
    force_draw: bool,
    /// True if we should move the cursor up when possible instead of clearing lines.
    pub(crate) move_cursor: bool,
    /// Controls how the multi progress is aligned if some of its progress bars get removed, default is `Top`
    pub(crate) alignment: MultiProgressAlignment,
}

impl ProgressDrawState {
    pub(crate) fn new(lines: Vec<String>, force_draw: bool) -> Self {
        Self {
            lines,
            orphan_lines: 0,
            force_draw,
            move_cursor: false,
            alignment: Default::default(),
        }
    }

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
                let line_width = console::measure_text_width(line);
                term.write_str(&" ".repeat(term.width().saturating_sub(line_width)))?;
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
