use std::io;
use std::sync::{Arc, RwLock, RwLockWriteGuard};
use std::time::Instant;

use console::Term;

use crate::progress_bar::ProgressBar;

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
        }
    }

    /// Apply the given draw state (draws it).
    pub(crate) fn drawable(&mut self) -> Option<Drawable<'_>> {
        match &mut self.kind {
            ProgressDrawTargetKind::Term {
                term,
                last_line_count,
                leaky_bucket,
                draw_state,
            } => {
                let has_capacity = leaky_bucket
                    .as_mut()
                    .map(|b| b.try_add_work())
                    .unwrap_or(true);

                match draw_state.force_draw || has_capacity {
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
                    force_draw: false,
                })
            }
            // Hidden, finished, or no need to refresh yet
            _ => None,
        }
    }

    /// Properly disconnects from the draw target
    pub(crate) fn disconnect(&self) {
        match self.kind {
            ProgressDrawTargetKind::Term { .. } => {}
            ProgressDrawTargetKind::Remote { idx, ref state, .. } => {
                let state = state.write().unwrap();
                let mut drawable = Drawable::Multi {
                    state,
                    idx,
                    force_draw: false,
                };

                let mut draw_state = drawable.state();
                draw_state.reset();
                draw_state.force_draw = true;
                drop(draw_state);
                let _ = drawable.draw();
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
        draw_state: ProgressDrawState,
    },
    Remote {
        state: Arc<RwLock<MultiProgressState>>,
        idx: usize,
    },
    Hidden,
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
    },
}

impl<'a> Drawable<'a> {
    pub(crate) fn state(&mut self) -> DrawStateWrapper<'_> {
        match self {
            Drawable::Term { draw_state, .. } => DrawStateWrapper {
                state: *draw_state,
                extra: None,
            },
            Drawable::Multi {
                state,
                idx,
                force_draw,
            } => {
                let state = &mut **state;
                let (states, orphans) = (&mut state.draw_states, &mut state.orphan_lines);
                match states.get_mut(*idx) {
                    Some(Some(draw_state)) => DrawStateWrapper {
                        state: draw_state,
                        extra: Some((orphans, force_draw)),
                    },
                    Some(inner) => {
                        *inner = Some(ProgressDrawState::new(Vec::new(), false));
                        DrawStateWrapper {
                            state: inner.as_mut().unwrap(),
                            extra: Some((orphans, force_draw)),
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
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
                ..
            } => state.draw(force_draw),
        }
    }
}

pub(crate) struct DrawStateWrapper<'a> {
    state: &'a mut ProgressDrawState,
    extra: Option<(&'a mut Vec<String>, &'a mut bool)>,
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
        if let Some((orphan_lines, force_draw)) = &mut self.extra {
            orphan_lines.extend(self.state.lines.drain(..self.state.orphan_lines));
            self.state.orphan_lines = 0;
            **force_draw = self.state.force_draw;
        }
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
    /// Controls how the multi progress is aligned if some of its progress bars get removed, default is `Top`
    pub(crate) alignment: MultiProgressAlignment,
    /// Orphaned lines are carried over across draw operations
    orphan_lines: Vec<String>,
}

impl MultiProgressState {
    pub(crate) fn new(draw_target: ProgressDrawTarget) -> Self {
        Self {
            draw_states: vec![],
            free_set: vec![],
            ordering: vec![],
            draw_target,
            move_cursor: false,
            alignment: Default::default(),
            orphan_lines: Vec::new(),
        }
    }

    pub(crate) fn insert(&mut self, location: InsertLocation) -> usize {
        let idx = match self.free_set.pop() {
            Some(idx) => {
                self.draw_states[idx] = None;
                idx
            }
            None => {
                self.draw_states.push(None);
                self.draw_states.len() - 1
            }
        };

        match location {
            InsertLocation::End => self.ordering.push(idx),
            InsertLocation::Index(pos) => {
                let pos = Ord::min(pos, self.ordering.len());
                self.ordering.insert(pos, idx);
            }
            InsertLocation::IndexFromBack(pos) => {
                let pos = self.ordering.len().saturating_sub(pos);
                self.ordering.insert(pos, idx);
            }
            InsertLocation::After(after) => {
                let after_idx = after.state.lock().unwrap().draw_target.remote().unwrap().1;
                let pos = self.ordering.iter().position(|i| *i == after_idx).unwrap();
                self.ordering.insert(pos + 1, idx);
            }
            InsertLocation::Before(before) => {
                let before_idx = before.state.lock().unwrap().draw_target.remote().unwrap().1;
                let pos = self.ordering.iter().position(|i| *i == before_idx).unwrap();
                self.ordering.insert(pos, idx);
            }
        }

        assert!(
            self.len() == self.ordering.len(),
            "Draw state is inconsistent"
        );

        idx
    }

    pub(crate) fn clear(&mut self) -> io::Result<()> {
        let (move_cursor, alignment) = (self.move_cursor, self.alignment);
        let mut drawable = match self.draw_target.drawable() {
            Some(drawable) => drawable,
            None => return Ok(()),
        };

        let mut draw_state = drawable.state();
        draw_state.reset();
        draw_state.force_draw = true;
        draw_state.move_cursor = move_cursor;
        draw_state.alignment = alignment;

        drop(draw_state);
        drawable.draw()
    }

    fn width(&self) -> usize {
        self.draw_target.width()
    }

    fn draw(&mut self, force_draw: bool) -> io::Result<()> {
        // the rest from here is only drawing, we can skip it.
        if self.draw_target.is_hidden() {
            return Ok(());
        }

        let mut drawable = match self.draw_target.drawable() {
            Some(drawable) => drawable,
            None => return Ok(()),
        };

        let mut draw_state = drawable.state();
        draw_state.reset();

        // Make orphaned lines appear at the top, so they can be properly
        // forgotten.
        let orphan_lines_count = self.orphan_lines.len();
        draw_state.lines.append(&mut self.orphan_lines);

        for index in self.ordering.iter() {
            if let Some(state) = &self.draw_states[*index] {
                draw_state.lines.extend_from_slice(&state.lines[..]);
            }
        }

        draw_state.orphan_lines = orphan_lines_count;
        draw_state.force_draw = force_draw || orphan_lines_count > 0;
        draw_state.move_cursor = self.move_cursor;
        draw_state.alignment = self.alignment;

        drop(draw_state);
        drawable.draw()
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

pub(crate) enum InsertLocation<'a> {
    End,
    Index(usize),
    IndexFromBack(usize),
    After(&'a ProgressBar),
    Before(&'a ProgressBar),
}

#[derive(Debug)]
struct LeakyBucket {
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
    /// True if drawing should be forced.
    pub force_draw: bool,
    /// True if we should move the cursor up when possible instead of clearing lines.
    pub move_cursor: bool,
    /// Controls how the multi progress is aligned if some of its progress bars get removed, default is `Top`
    pub alignment: MultiProgressAlignment,
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

    fn draw_to_term(&mut self, term: &Term, last_line_count: &mut usize) -> io::Result<()> {
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
                term.write_str(&" ".repeat(usize::from(term.size().1) - line_width))?;
            }
        }

        term.flush()?;
        *last_line_count = self.lines.len() - self.orphan_lines + shift;
        Ok(())
    }

    pub(crate) fn reset(&mut self) {
        self.lines.clear();
        self.orphan_lines = 0;
        self.force_draw = false;
        self.move_cursor = false;
        self.alignment = MultiProgressAlignment::default();
    }
}

/// Vertical alignment of a multi progress.
///
/// The alignment controls how the multi progress is aligned if some of its progress bars get removed.
/// E.g. `Top` alignment (default), when _progress bar 2_ is removed:
/// ```ignore
/// [0/100] progress bar 1        [0/100] progress bar 1
/// [0/100] progress bar 2   =>   [0/100] progress bar 3
/// [0/100] progress bar 3
/// ```
///
/// `Bottom` alignment
/// ```ignore
/// [0/100] progress bar 1
/// [0/100] progress bar 2   =>   [0/100] progress bar 1
/// [0/100] progress bar 3        [0/100] progress bar 3
/// ```
#[derive(Debug, Copy, Clone)]
pub enum MultiProgressAlignment {
    Top,
    Bottom,
}

impl Default for MultiProgressAlignment {
    fn default() -> Self {
        Self::Top
    }
}

#[cfg(test)]
mod tests {
    use crate::{MultiProgress, ProgressBar};

    #[test]
    fn multi_progress_modifications() {
        let mp = MultiProgress::new();
        let p0 = mp.add(ProgressBar::new(1));
        let p1 = mp.add(ProgressBar::new(1));
        let p2 = mp.add(ProgressBar::new(1));
        let p3 = mp.add(ProgressBar::new(1));
        mp.remove(&p2);
        mp.remove(&p1);
        let p4 = mp.insert(1, ProgressBar::new(1));

        let state = mp.state.read().unwrap();
        // the removed place for p1 is reused
        assert_eq!(state.draw_states.len(), 4);
        assert_eq!(state.len(), 3);

        // free_set may contain 1 or 2
        match state.free_set.last() {
            Some(1) => {
                assert_eq!(state.ordering, vec![0, 2, 3]);
                assert!(state.draw_states[1].is_none());
                assert_eq!(extract_index(&p4), 2);
            }
            Some(2) => {
                assert_eq!(state.ordering, vec![0, 1, 3]);
                assert!(state.draw_states[2].is_none());
                assert_eq!(extract_index(&p4), 1);
            }
            _ => unreachable!(),
        }

        assert_eq!(extract_index(&p0), 0);
        assert_eq!(extract_index(&p1), 1);
        assert_eq!(extract_index(&p2), 2);
        assert_eq!(extract_index(&p3), 3);
    }

    #[test]
    fn multi_progress_insert_from_back() {
        let mp = MultiProgress::new();
        let p0 = mp.add(ProgressBar::new(1));
        let p1 = mp.add(ProgressBar::new(1));
        let p2 = mp.add(ProgressBar::new(1));
        let p3 = mp.insert_from_back(1, ProgressBar::new(1));
        let p4 = mp.insert_from_back(10, ProgressBar::new(1));

        let state = mp.state.read().unwrap();
        assert_eq!(state.ordering, vec![4, 0, 1, 3, 2]);
        assert_eq!(extract_index(&p0), 0);
        assert_eq!(extract_index(&p1), 1);
        assert_eq!(extract_index(&p2), 2);
        assert_eq!(extract_index(&p3), 3);
        assert_eq!(extract_index(&p4), 4);
    }

    #[test]
    fn multi_progress_insert_after() {
        let mp = MultiProgress::new();
        let p0 = mp.add(ProgressBar::new(1));
        let p1 = mp.add(ProgressBar::new(1));
        let p2 = mp.add(ProgressBar::new(1));
        let p3 = mp.insert_after(&p2, ProgressBar::new(1));
        let p4 = mp.insert_after(&p0, ProgressBar::new(1));

        let state = mp.state.read().unwrap();
        assert_eq!(state.ordering, vec![0, 4, 1, 2, 3]);
        assert_eq!(extract_index(&p0), 0);
        assert_eq!(extract_index(&p1), 1);
        assert_eq!(extract_index(&p2), 2);
        assert_eq!(extract_index(&p3), 3);
        assert_eq!(extract_index(&p4), 4);
    }

    #[test]
    fn multi_progress_insert_before() {
        let mp = MultiProgress::new();
        let p0 = mp.add(ProgressBar::new(1));
        let p1 = mp.add(ProgressBar::new(1));
        let p2 = mp.add(ProgressBar::new(1));
        let p3 = mp.insert_before(&p0, ProgressBar::new(1));
        let p4 = mp.insert_before(&p2, ProgressBar::new(1));

        let state = mp.state.read().unwrap();
        assert_eq!(state.ordering, vec![3, 0, 1, 4, 2]);
        assert_eq!(extract_index(&p0), 0);
        assert_eq!(extract_index(&p1), 1);
        assert_eq!(extract_index(&p2), 2);
        assert_eq!(extract_index(&p3), 3);
        assert_eq!(extract_index(&p4), 4);
    }

    #[test]
    fn multi_progress_insert_before_and_after() {
        let mp = MultiProgress::new();
        let p0 = mp.add(ProgressBar::new(1));
        let p1 = mp.add(ProgressBar::new(1));
        let p2 = mp.add(ProgressBar::new(1));
        let p3 = mp.insert_before(&p0, ProgressBar::new(1));
        let p4 = mp.insert_after(&p3, ProgressBar::new(1));
        let p5 = mp.insert_after(&p3, ProgressBar::new(1));
        let p6 = mp.insert_before(&p1, ProgressBar::new(1));

        let state = mp.state.read().unwrap();
        assert_eq!(state.ordering, vec![3, 5, 4, 0, 6, 1, 2]);
        assert_eq!(extract_index(&p0), 0);
        assert_eq!(extract_index(&p1), 1);
        assert_eq!(extract_index(&p2), 2);
        assert_eq!(extract_index(&p3), 3);
        assert_eq!(extract_index(&p4), 4);
        assert_eq!(extract_index(&p5), 5);
        assert_eq!(extract_index(&p6), 6);
    }

    #[test]
    fn multi_progress_multiple_remove() {
        let mp = MultiProgress::new();
        let p0 = mp.add(ProgressBar::new(1));
        let p1 = mp.add(ProgressBar::new(1));
        // double remove beyond the first one have no effect
        mp.remove(&p0);
        mp.remove(&p0);
        mp.remove(&p0);

        let state = mp.state.read().unwrap();
        // the removed place for p1 is reused
        assert_eq!(state.draw_states.len(), 2);
        assert_eq!(state.free_set.len(), 1);
        assert_eq!(state.len(), 1);
        assert!(state.draw_states[0].is_none());
        assert_eq!(state.free_set.last(), Some(&0));

        assert_eq!(state.ordering, vec![1]);
        assert_eq!(extract_index(&p0), 0);
        assert_eq!(extract_index(&p1), 1);
    }

    fn extract_index(pb: &ProgressBar) -> usize {
        pb.state.lock().unwrap().draw_target.remote().unwrap().1
    }
}
