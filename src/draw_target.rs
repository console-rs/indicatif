use std::io;
use std::sync::{Arc, RwLock};
use std::time::Instant;

use console::Term;

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
    pub(crate) fn width(&self) -> usize {
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
    pub(crate) fn new(lines: Vec<String>, finished: bool) -> Self {
        Self {
            lines,
            orphan_lines: 0,
            finished,
            force_draw: false,
            move_cursor: false,
        }
    }

    pub fn draw_to_term(&self, term: &Term) -> io::Result<()> {
        for line in &self.lines {
            term.write_line(line)?;
        }
        Ok(())
    }
}
