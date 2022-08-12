use std::fmt::{Debug, Formatter};
use std::io;
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::thread::panicking;
use std::time::Instant;

use crate::draw_target::{DrawState, DrawStateWrapper, ProgressDrawTarget};
use crate::progress_bar::ProgressBar;
use crate::state::BarState;

/// Manages multiple progress bars from different threads
#[derive(Debug, Clone)]
pub struct MultiProgress {
    pub(crate) state: Arc<RwLock<MultiState>>,
}

impl Default for MultiProgress {
    fn default() -> MultiProgress {
        MultiProgress::with_draw_target(ProgressDrawTarget::stderr())
    }
}

impl MultiProgress {
    /// Creates a new multi progress object.
    ///
    /// Progress bars added to this object by default draw directly to stderr, and refresh
    /// a maximum of 15 times a second. To change the refresh rate set the draw target to
    /// one with a different refresh rate.
    pub fn new() -> MultiProgress {
        MultiProgress::default()
    }

    /// Creates a new multi progress object with the given draw target.
    pub fn with_draw_target(draw_target: ProgressDrawTarget) -> MultiProgress {
        MultiProgress {
            state: Arc::new(RwLock::new(MultiState::new(draw_target))),
        }
    }

    /// Sets a different draw target for the multiprogress bar.
    pub fn set_draw_target(&self, target: ProgressDrawTarget) {
        let mut state = self.state.write().unwrap();
        state.draw_target.disconnect(Instant::now());
        state.draw_target = target;
    }

    /// Set whether we should try to move the cursor when possible instead of clearing lines.
    ///
    /// This can reduce flickering, but do not enable it if you intend to change the number of
    /// progress bars.
    pub fn set_move_cursor(&self, move_cursor: bool) {
        self.state.write().unwrap().move_cursor = move_cursor;
    }

    /// Set alignment flag
    pub fn set_alignment(&self, alignment: MultiProgressAlignment) {
        self.state.write().unwrap().alignment = alignment;
    }

    /// Adds a progress bar.
    ///
    /// The progress bar added will have the draw target changed to a
    /// remote draw target that is intercepted by the multi progress
    /// object overriding custom `ProgressDrawTarget` settings.
    pub fn add(&self, pb: ProgressBar) -> ProgressBar {
        self.internalize(InsertLocation::End, pb)
    }

    /// Inserts a progress bar.
    ///
    /// The progress bar inserted at position `index` will have the draw
    /// target changed to a remote draw target that is intercepted by the
    /// multi progress object overriding custom `ProgressDrawTarget` settings.
    ///
    /// If `index >= MultiProgressState::objects.len()`, the progress bar
    /// is added to the end of the list.
    pub fn insert(&self, index: usize, pb: ProgressBar) -> ProgressBar {
        self.internalize(InsertLocation::Index(index), pb)
    }

    /// Inserts a progress bar from the back.
    ///
    /// The progress bar inserted at position `MultiProgressState::objects.len() - index`
    /// will have the draw target changed to a remote draw target that is
    /// intercepted by the multi progress object overriding custom
    /// `ProgressDrawTarget` settings.
    ///
    /// If `index >= MultiProgressState::objects.len()`, the progress bar
    /// is added to the start of the list.
    pub fn insert_from_back(&self, index: usize, pb: ProgressBar) -> ProgressBar {
        self.internalize(InsertLocation::IndexFromBack(index), pb)
    }

    /// Inserts a progress bar before an existing one.
    ///
    /// The progress bar added will have the draw target changed to a
    /// remote draw target that is intercepted by the multi progress
    /// object overriding custom `ProgressDrawTarget` settings.
    pub fn insert_before(&self, before: &ProgressBar, pb: ProgressBar) -> ProgressBar {
        self.internalize(InsertLocation::Before(before.index().unwrap()), pb)
    }

    /// Inserts a progress bar after an existing one.
    ///
    /// The progress bar added will have the draw target changed to a
    /// remote draw target that is intercepted by the multi progress
    /// object overriding custom `ProgressDrawTarget` settings.
    pub fn insert_after(&self, after: &ProgressBar, pb: ProgressBar) -> ProgressBar {
        self.internalize(InsertLocation::After(after.index().unwrap()), pb)
    }

    /// Removes a progress bar.
    ///
    /// The progress bar is removed only if it was previously inserted or added
    /// by the methods `MultiProgress::insert` or `MultiProgress::add`.
    /// If the passed progress bar does not satisfy the condition above,
    /// the `remove` method does nothing.
    pub fn remove(&self, pb: &ProgressBar) {
        let mut state = pb.state();
        let idx = match &state.draw_target.remote() {
            Some((state, idx)) => {
                // Check that this progress bar is owned by the current MultiProgress.
                assert!(Arc::ptr_eq(&self.state, state));
                *idx
            }
            _ => return,
        };

        state.draw_target = ProgressDrawTarget::hidden();
        self.state.write().unwrap().remove_idx(idx);
    }

    fn internalize(&self, location: InsertLocation, pb: ProgressBar) -> ProgressBar {
        let mut state = self.state.write().unwrap();

        let idx = state.insert(location);
        pb.set_draw_target(ProgressDrawTarget::new_remote(self.state.clone(), idx));
        state.members.get_mut(idx).unwrap().pb = pb.weak_bar_state();
        pb
    }

    /// Print a log line above all progress bars in the [`MultiProgress`]
    ///
    /// If the draw target is hidden (e.g. when standard output is not a terminal), `println()`
    /// will not do anything.
    pub fn println<I: AsRef<str>>(&self, msg: I) -> io::Result<()> {
        let mut state = self.state.write().unwrap();
        state.println(msg, Instant::now())
    }

    /// Hide all progress bars temporarily, execute `f`, then redraw the [`MultiProgress`]
    ///
    /// Executes 'f' even if the draw target is hidden.
    ///
    /// Useful for external code that writes to the standard output.
    ///
    /// **Note:** The internal lock is held while `f` is executed. Other threads trying to print
    /// anything on the progress bar will be blocked until `f` finishes.
    /// Therefore, it is recommended to avoid long-running operations in `f`.
    pub fn suspend<F: FnOnce() -> R, R>(&self, f: F) -> R {
        let mut state = self.state.write().unwrap();
        state.suspend(f, Instant::now())
    }

    pub fn clear(&self) -> io::Result<()> {
        self.state.write().unwrap().clear(Instant::now())
    }

    pub fn is_hidden(&self) -> bool {
        self.state.read().unwrap().is_hidden()
    }
}

#[derive(Debug)]
pub(crate) struct MultiState {
    /// The collection of states corresponding to progress bars
    members: Vec<MultiStateMember>,
    /// Set of removed bars, should have corresponding members in the `members` vector with a
    /// `draw_state` of `None`.
    free_set: Vec<usize>,
    /// Indices to the `draw_states` to maintain correct visual order
    ordering: Vec<usize>,
    /// Target for draw operation for MultiProgress
    draw_target: ProgressDrawTarget,
    /// Whether or not to just move cursor instead of clearing lines
    move_cursor: bool,
    /// Controls how the multi progress is aligned if some of its progress bars get removed, default is `Top`
    alignment: MultiProgressAlignment,
    /// Orphaned lines are carried over across draw operations
    orphan_lines: Vec<String>,
}

impl MultiState {
    fn new(draw_target: ProgressDrawTarget) -> Self {
        Self {
            members: vec![],
            free_set: vec![],
            ordering: vec![],
            draw_target,
            move_cursor: false,
            alignment: Default::default(),
            orphan_lines: Vec::new(),
        }
    }

    pub(crate) fn draw(
        &mut self,
        mut force_draw: bool,
        extra_lines: Option<Vec<String>>,
        now: Instant,
    ) -> io::Result<()> {
        if panicking() {
            return Ok(());
        }

        // Reap all consecutive 'zombie' progress bars from head of the list
        let mut adjust = 0;
        while let Some(index) = self.ordering.first().copied() {
            let member = &self.members[index];
            if !member.is_zombie {
                break;
            }

            adjust += member
                .draw_state
                .as_ref()
                .map(|d| d.lines.len())
                .unwrap_or_default();
            self.remove_idx(index);
        }

        // Adjust last line count so we don't clear too many lines
        if let Some(last_line_count) = self.draw_target.last_line_count() {
            *last_line_count -= adjust;
        }

        let orphan_lines_count = self.orphan_lines.len();
        force_draw |= orphan_lines_count > 0;
        let mut drawable = match self.draw_target.drawable(force_draw, now) {
            Some(drawable) => drawable,
            None => return Ok(()),
        };

        let mut draw_state = drawable.state();
        draw_state.orphan_lines_count = orphan_lines_count;

        if let Some(extra_lines) = &extra_lines {
            draw_state.lines.extend_from_slice(extra_lines.as_slice());
            draw_state.orphan_lines_count += extra_lines.len();
        }

        // Make orphaned lines appear at the top, so they can be properly forgotten.
        draw_state.lines.append(&mut self.orphan_lines);

        for index in self.ordering.iter() {
            let member = &mut self.members[*index];
            if let Some(state) = &member.draw_state {
                draw_state.lines.extend_from_slice(&state.lines[..]);
            }

            // Mark the dead progress bar as a zombie - will be reaped on next draw
            if member.pb.upgrade().is_none() {
                member.is_zombie = true;
            }
        }

        drop(draw_state);
        drawable.draw()
    }

    pub(crate) fn println<I: AsRef<str>>(&mut self, msg: I, now: Instant) -> io::Result<()> {
        let msg = msg.as_ref();

        // If msg is "", make sure a line is still printed
        let lines: Vec<String> = match msg.is_empty() {
            false => msg.lines().map(Into::into).collect(),
            true => vec![String::new()],
        };

        self.draw(true, Some(lines), now)
    }

    pub(crate) fn draw_state(&mut self, idx: usize) -> DrawStateWrapper<'_> {
        let member = self.members.get_mut(idx).unwrap();
        let state = member.draw_state.get_or_insert(DrawState {
            move_cursor: self.move_cursor,
            alignment: self.alignment,
            ..Default::default()
        });

        DrawStateWrapper::for_multi(state, &mut self.orphan_lines)
    }

    pub(crate) fn is_hidden(&self) -> bool {
        self.draw_target.is_hidden()
    }

    pub(crate) fn suspend<F: FnOnce() -> R, R>(&mut self, f: F, now: Instant) -> R {
        self.clear(now).unwrap();
        let ret = f();
        self.draw(true, None, Instant::now()).unwrap();
        ret
    }

    pub(crate) fn width(&self) -> u16 {
        self.draw_target.width()
    }

    fn insert(&mut self, location: InsertLocation) -> usize {
        let idx = match self.free_set.pop() {
            Some(idx) => {
                self.members[idx] = MultiStateMember::default();
                idx
            }
            None => {
                self.members.push(MultiStateMember::default());
                self.members.len() - 1
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
            InsertLocation::After(after_idx) => {
                let pos = self.ordering.iter().position(|i| *i == after_idx).unwrap();
                self.ordering.insert(pos + 1, idx);
            }
            InsertLocation::Before(before_idx) => {
                let pos = self.ordering.iter().position(|i| *i == before_idx).unwrap();
                self.ordering.insert(pos, idx);
            }
        }

        assert_eq!(
            self.len(),
            self.ordering.len(),
            "Draw state is inconsistent"
        );

        idx
    }

    fn clear(&mut self, now: Instant) -> io::Result<()> {
        match self.draw_target.drawable(true, now) {
            Some(drawable) => drawable.clear(),
            None => Ok(()),
        }
    }

    fn remove_idx(&mut self, idx: usize) {
        if self.free_set.contains(&idx) {
            return;
        }

        self.members[idx] = MultiStateMember::default();
        self.free_set.push(idx);
        self.ordering.retain(|&x| x != idx);

        assert_eq!(
            self.len(),
            self.ordering.len(),
            "Draw state is inconsistent"
        );
    }

    fn len(&self) -> usize {
        self.members.len() - self.free_set.len()
    }
}

#[derive(Default)]
struct MultiStateMember {
    /// Draw state will be `None` for members that haven't been drawn before, or for entries that
    /// correspond to something in the free set.
    draw_state: Option<DrawState>,
    /// This will be a valid reference unless the containing member is actually in the free set.
    pb: Weak<Mutex<BarState>>,
    is_zombie: bool,
}

impl Debug for MultiStateMember {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MultiStateElement")
            .field("draw_state", &self.draw_state)
            .field("is_zombie", &self.is_zombie)
            .finish_non_exhaustive()
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

enum InsertLocation {
    End,
    Index(usize),
    IndexFromBack(usize),
    After(usize),
    Before(usize),
}

#[cfg(test)]
mod tests {
    use crate::{MultiProgress, ProgressBar, ProgressDrawTarget};

    #[test]
    fn late_pb_drop() {
        let pb = ProgressBar::new(10);
        let mpb = MultiProgress::new();
        // This clone call is required to trigger a now fixed bug.
        // See <https://github.com/console-rs/indicatif/pull/141> for context
        #[allow(clippy::redundant_clone)]
        mpb.add(pb.clone());
    }

    #[test]
    fn progress_bar_sync_send() {
        let _: Box<dyn Sync> = Box::new(ProgressBar::new(1));
        let _: Box<dyn Send> = Box::new(ProgressBar::new(1));
        let _: Box<dyn Sync> = Box::new(MultiProgress::new());
        let _: Box<dyn Send> = Box::new(MultiProgress::new());
    }

    #[test]
    fn multi_progress_hidden() {
        let mpb = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mpb.add(ProgressBar::new(123));
        pb.finish();
    }

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
        assert_eq!(state.members.len(), 4);
        assert_eq!(state.len(), 3);

        // free_set may contain 1 or 2
        match state.free_set.last() {
            Some(1) => {
                assert_eq!(state.ordering, vec![0, 2, 3]);
                assert!(state.members[1].draw_state.is_none());
                assert_eq!(p4.index().unwrap(), 2);
            }
            Some(2) => {
                assert_eq!(state.ordering, vec![0, 1, 3]);
                assert!(state.members[2].draw_state.is_none());
                assert_eq!(p4.index().unwrap(), 1);
            }
            _ => unreachable!(),
        }

        assert_eq!(p0.index().unwrap(), 0);
        assert_eq!(p1.index(), None);
        assert_eq!(p2.index(), None);
        assert_eq!(p3.index().unwrap(), 3);
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
        assert_eq!(p0.index().unwrap(), 0);
        assert_eq!(p1.index().unwrap(), 1);
        assert_eq!(p2.index().unwrap(), 2);
        assert_eq!(p3.index().unwrap(), 3);
        assert_eq!(p4.index().unwrap(), 4);
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
        assert_eq!(p0.index().unwrap(), 0);
        assert_eq!(p1.index().unwrap(), 1);
        assert_eq!(p2.index().unwrap(), 2);
        assert_eq!(p3.index().unwrap(), 3);
        assert_eq!(p4.index().unwrap(), 4);
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
        assert_eq!(p0.index().unwrap(), 0);
        assert_eq!(p1.index().unwrap(), 1);
        assert_eq!(p2.index().unwrap(), 2);
        assert_eq!(p3.index().unwrap(), 3);
        assert_eq!(p4.index().unwrap(), 4);
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
        assert_eq!(p0.index().unwrap(), 0);
        assert_eq!(p1.index().unwrap(), 1);
        assert_eq!(p2.index().unwrap(), 2);
        assert_eq!(p3.index().unwrap(), 3);
        assert_eq!(p4.index().unwrap(), 4);
        assert_eq!(p5.index().unwrap(), 5);
        assert_eq!(p6.index().unwrap(), 6);
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
        assert_eq!(state.members.len(), 2);
        assert_eq!(state.free_set.len(), 1);
        assert_eq!(state.len(), 1);
        assert!(state.members[0].draw_state.is_none());
        assert_eq!(state.free_set.last(), Some(&0));

        assert_eq!(state.ordering, vec![1]);
        assert_eq!(p0.index(), None);
        assert_eq!(p1.index().unwrap(), 1);
    }
}
