use std::fmt::{Debug, Formatter};
use std::io;
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::Instant;
use generational_token_list::{GenerationalTokenList, ItemToken};

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
        self.internalize(InsertLocation::Before(before.token().unwrap()), pb)
    }

    /// Inserts a progress bar after an existing one.
    ///
    /// The progress bar added will have the draw target changed to a
    /// remote draw target that is intercepted by the multi progress
    /// object overriding custom `ProgressDrawTarget` settings.
    pub fn insert_after(&self, after: &ProgressBar, pb: ProgressBar) -> ProgressBar {
        self.internalize(InsertLocation::After(after.token().unwrap()), pb)
    }

    /// Removes a progress bar.
    ///
    /// The progress bar is removed only if it was previously inserted or added
    /// by the methods `MultiProgress::insert` or `MultiProgress::add`.
    /// If the passed progress bar does not satisfy the condition above,
    /// the `remove` method does nothing.
    pub fn remove(&self, pb: &ProgressBar) {
        let mut state = pb.state();
        let token = match &state.draw_target.remote() {
            Some((state, idx)) => {
                // Check that this progress bar is owned by the current MultiProgress.
                assert!(Arc::ptr_eq(&self.state, state));
                *idx
            }
            _ => return,
        };

        state.draw_target = ProgressDrawTarget::hidden();
        self.state.write().unwrap().remove_idx(token);
    }

    fn internalize(&self, location: InsertLocation, pb: ProgressBar) -> ProgressBar {
        let mut state = self.state.write().unwrap();

        let token = state.insert(location, pb.weak_bar_state());
        pb.set_draw_target(ProgressDrawTarget::new_remote(self.state.clone(), token));
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
    members: GenerationalTokenList<MultiStateMember>,
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
            members: GenerationalTokenList::new(),
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
        // Reap all consecutive 'zombie' progress bars from head of the list
        let mut zombies = vec![];
        let mut adjust = 0;
        for member in self.members.iter() {
            if !member.is_zombie {
                break;
            }

            zombies.push(member.token);
            adjust += member
                .draw_state
                .as_ref()
                .map(|d| d.lines.len())
                .unwrap_or_default();
        }

        for index in zombies {
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
        draw_state.orphan_lines = orphan_lines_count;

        if let Some(extra_lines) = &extra_lines {
            draw_state.lines.extend_from_slice(extra_lines.as_slice());
            draw_state.orphan_lines += extra_lines.len();
        }

        // Make orphaned lines appear at the top, so they can be properly forgotten.
        draw_state.lines.append(&mut self.orphan_lines);

        for member in self.members.iter_mut() {
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

    pub(crate) fn draw_state(&mut self, token: ItemToken) -> DrawStateWrapper<'_> {
        let member = self.members.get_mut(token).unwrap();
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

    fn insert(&mut self, location: InsertLocation, bar_state: Weak<Mutex<BarState>>) -> ItemToken {
        let create = |token| -> _ { MultiStateMember::new(bar_state, token) };

        match location {
            InsertLocation::End => self.members.push_back_with(create),
            InsertLocation::Index(pos) => {
                if let Some(token) = self.members.token_at(pos) {
                    self.members.insert_before_with(token, create)
                } else {
                    self.members.push_back_with(create)
                }
            }
            InsertLocation::IndexFromBack(pos) => {
                if let Some(token) = self.members.token_at_back(pos) {
                    self.members.insert_after_with(token, create)
                } else {
                    self.members.push_front_with(create)
                }
            }
            InsertLocation::After(after_idx) => self.members.insert_after_with(after_idx, create),
            InsertLocation::Before(before_idx) => self.members.insert_before_with(before_idx, create),
        }
    }

    fn clear(&mut self, now: Instant) -> io::Result<()> {
        match self.draw_target.drawable(true, now) {
            Some(drawable) => drawable.clear(),
            None => Ok(()),
        }
    }

    fn remove_idx(&mut self, token: ItemToken) {
        self.members.remove(token);
    }
}

struct MultiStateMember {
    /// Draw state will be `None` for members that haven't been drawn before.
    draw_state: Option<DrawState>,
    pb: Weak<Mutex<BarState>>,
    is_zombie: bool,
    token: ItemToken,
}

impl MultiStateMember {
    fn new(pb: Weak<Mutex<BarState>>, token: ItemToken) -> Self {
        Self {
            draw_state: None,
            pb,
            is_zombie: false,
            token,
        }
    }
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
    After(ItemToken),
    Before(ItemToken),
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

    macro_rules! check_order {
        ($mp:ident, $($pbs:ident),*) => {
            let state = $mp.state.read().unwrap();
            let tokens = state
                .members
                .iter_with_tokens()
                .map(|(token, _)| Some(token))
                .collect::<Vec<_>>();

            assert_eq!(tokens, vec![$($pbs.token()),+]);
        }
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

        check_order!(mp, p0, p4, p3);

        assert_eq!(p1.token(), None);
        assert_eq!(p2.token(), None);
    }

    #[test]
    fn multi_progress_insert_from_back() {
        let mp = MultiProgress::new();
        let p0 = mp.add(ProgressBar::new(1));
        let p1 = mp.add(ProgressBar::new(1));
        let p2 = mp.add(ProgressBar::new(1));
        let p3 = mp.insert_from_back(1, ProgressBar::new(1));
        let p4 = mp.insert_from_back(10, ProgressBar::new(1));

        check_order!(mp, p4, p0, p1, p3, p2);
    }

    #[test]
    fn multi_progress_insert_after() {
        let mp = MultiProgress::new();
        let p0 = mp.add(ProgressBar::new(1));
        let p1 = mp.add(ProgressBar::new(1));
        let p2 = mp.add(ProgressBar::new(1));
        let p3 = mp.insert_after(&p2, ProgressBar::new(1));
        let p4 = mp.insert_after(&p0, ProgressBar::new(1));

        check_order!(mp, p0, p4, p1, p2, p3);
    }

    #[test]
    fn multi_progress_insert_before() {
        let mp = MultiProgress::new();
        let p0 = mp.add(ProgressBar::new(1));
        let p1 = mp.add(ProgressBar::new(1));
        let p2 = mp.add(ProgressBar::new(1));
        let p3 = mp.insert_before(&p0, ProgressBar::new(1));
        let p4 = mp.insert_before(&p2, ProgressBar::new(1));

        check_order!(mp, p3, p0, p1, p4, p2);
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

        check_order!(mp, p3, p5, p4, p0, p6, p1, p2);
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

        check_order!(mp, p1);
        assert_eq!(p0.token(), None);
    }
}
