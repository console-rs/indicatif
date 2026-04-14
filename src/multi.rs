use std::fmt::{Debug, Formatter};
use std::io;
use std::sync::{Arc, RwLock};
use std::thread::panicking;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use crate::draw_target::{
    visual_line_count, DrawState, DrawStateWrapper, LineAdjust, LineType, ProgressDrawTarget,
    VisualLines,
};
use crate::progress_bar::ProgressBar;
use crate::progress_bar_builder::ProgressBarBuilder;
#[cfg(all(target_arch = "wasm32", feature = "wasmbind"))]
use web_time::Instant;

/// Manages multiple progress bars, potentially from different threads.
///
/// # `ProgressBar` lifecycle in a `MultiProgress`
/// This section was written to help you avoid unexpected behavior when using `MultiProgress`. The two most common
/// issues that users face are:
///
/// 1. Inadvertent draws prior to adding to the `MultiProgress`
/// 2. `ProgressBar`s getting dropped too soon
///
/// ## Inadvertent draws
/// `MultiProgress` can only coordinate drawing progress bars on the screen if it is aware of them. A common bug is to
/// create a `ProgressBar`, accidentally cause it to draw (or tick), and then later add it to the `MultiProgress`. This
/// can lead to screen corruption since `MultiProgress` has no way to "undo" whatever the `ProgressBar` did before
/// the bar came under its purview.
///
/// Here's an example of potentially problematic code. The bar is created at (1) but added to the `MultiProgress` at
/// (2).
///
/// ```rust,ignore
/// // Bad code, do not use!
/// let m = MultiProgress::new();
/// let pb = ProgressBar::new(100);      // (1)
/// // It's awfully tempting to touch
/// // `pb`, before it's added to `m`...
/// m.add(pb);                           // (2)
/// ```
///
/// Instead, create the `ProgressBar` and add it to the `MultiProgress` as a single call:
///
/// ```rust,ignore
/// // Better code
/// let m = MultiProgress::new();
/// let pb = m.add(ProgressBar::new(100));
/// // Then style/exercise it as you please:
/// // e.g. pb.set_style()
/// ```
///
/// Future work may deprecate the "bad" API and steer users toward the "good" model. See <https://github.com/console-rs/indicatif/issues/677> for example.
///
/// ## Premature drops
/// Consider this code, with an overall "total" `ProgressBar` and an individual `ProgressBar` for each of 5 jobs. The
/// intention is that when each job bar finishes, it stays on the screen with the "DONE!" message. Also, during job
/// processing, we call [`MultiProgress::suspend`] to temporarily clear the terminal and manually print some extra
/// messages.
///
/// ```rust
/// use indicatif::{MultiProgress, ProgressBar, ProgressFinish, ProgressStyle};
/// use std::borrow::Cow;
///
/// fn main() {
///     let m = MultiProgress::new();
///     let sty = ProgressStyle::with_template(
///         "{prefix:<10} [{elapsed}] {bar:20.red/blue} {pos:>7}/{len:7} {msg}",
///     )
///     .unwrap()
///     .progress_chars("##-");
///
///     let total = m.add(ProgressBar::new(10));
///     total.set_style(sty.clone());
///     total.set_prefix("total");
///     for i in 0..5 {
///         let name = format!("Job #{i}");
///         let pb = m.insert_before(
///             &total,
///             ProgressBar::new(3).with_finish(ProgressFinish::WithMessage(Cow::Borrowed("DONE!"))),
///         );
///
///         pb.set_style(sty.clone());
///         pb.set_prefix(name);
///         for _ in 0..3 {
///             // Temporarily clear the screen so we can print a message to the terminal
///             m.suspend(|| {
///                 eprintln!("from job #{i}...");
///             });
///
///             pb.inc(1);
///         }
///         pb.finish_using_style();
///         total.inc(1);
///     }
///
///     total.finish();
/// }
///
/// ```
///
/// Running the example, we see that it is broken:
///
#[doc = include_str!("../screenshots/mp-drop-before.svg")]
///
/// The issue is that at the end of each loop iteration, `pb` is dropped. Conceptually `MultiProgress` only maintains
/// weak references to `ProgressBar`s. At the next loop iteration, `suspend` causes `MultiProgress` to clear the screen.
/// `MultiProgress`' "zombie" algorithm ensures the dropped (zombie) bar is not left behind on the screen. But
/// `MultiProgress` can't reconstitute the 'finish' state (i.e. "DONE!" text), since the bar no longer exists.
///
/// The solution is to ensure each `ProgressBar` lives long enough:
/// ```rust,ignore
/// // Vec to hold handles
/// let mut pbs = vec![];
/// for i in 0..5 {
///     let name = format!("Job #{i}");
///     let pb = m.insert_before(
///         &total,
///         ProgressBar::new(3).with_finish(ProgressFinish::WithMessage(Cow::Borrowed("DONE!"))),
///     );
///     // Stash a handle to the pb to keep it alive till end of loop
///     pbs.push(pb.clone());
///
///     pb.set_style(sty.clone());
///     pb.set_prefix(name);
///     // ... snipped ...
/// }
/// ```
///
#[doc = include_str!("../screenshots/mp-drop-after.svg")]
///
/// ## The "zombie" algorithm
/// The "zombie" algorithm is a compromise. If the user lets a `ProgressBar` drop, then it is taken as a strong hint that
/// we can forget about it. But, the [`MultiProgress::println`] method advertises the ability to print a message above
/// **all** progress bars.
///
/// As a compromise, `MultiProgress` will keep track of how many lines of text were last printed to the screen, even for
/// `ProgressBar`s that have dropped. But the next time `MultiProgress` clears the screen, e.g. for a
/// [`MultiProgress::suspend`] or [`MultiProgress::println`], any so-called "zombie lines" at the head of the list are wiped
/// but then not re-drawn. If you really want those lines to be persisted on screen, then keep the `ProgressBar`s around
/// longer, as described in the previous section.
///
#[derive(Debug, Clone)]
pub struct MultiProgress {
    pub(crate) state: Arc<RwLock<MultiState>>,
}

impl Default for MultiProgress {
    fn default() -> Self {
        Self::with_draw_target(ProgressDrawTarget::stderr())
    }
}

impl MultiProgress {
    /// Creates a new multi progress object.
    ///
    /// Progress bars added to this object by default draw directly to stderr, and refresh
    /// a maximum of 15 times a second. To change the refresh rate [set] the [draw target] to
    /// one with a different refresh rate.
    ///
    /// [set]: MultiProgress::set_draw_target
    /// [draw target]: ProgressDrawTarget
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new multi progress object with the given draw target.
    pub fn with_draw_target(draw_target: ProgressDrawTarget) -> Self {
        Self {
            state: Arc::new(RwLock::new(MultiState::new(draw_target))),
        }
    }

    /// Sets a different draw target for the multiprogress bar.
    ///
    /// Use [`MultiProgress::with_draw_target`] to set the draw target during creation.
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
        self.state
            .write()
            .unwrap()
            .draw_target
            .set_move_cursor(move_cursor);
    }

    /// Set alignment flag
    pub fn set_alignment(&self, alignment: MultiProgressAlignment) {
        self.state.write().unwrap().alignment = alignment;
    }

    /// Registers a [`ProgressBarBuilder`] with this [`MultiProgress`].
    ///
    /// The builder is materialized into a [`ProgressBar`] whose draw target is
    /// a remote target intercepted by this [`MultiProgress`]. The resulting bar
    /// is positioned below all other bars currently in the [`MultiProgress`].
    ///
    /// Using a [`ProgressBarBuilder`] avoids the footgun of configuring a
    /// [`ProgressBar`] (which triggers draws to stderr) before adding it to a
    /// [`MultiProgress`]. See [#677] for details.
    ///
    /// [#677]: https://github.com/console-rs/indicatif/issues/677
    pub fn register(&self, builder: ProgressBarBuilder) -> ProgressBar {
        self.internalize_builder(InsertLocation::End, builder)
    }

    /// Registers a [`ProgressBarBuilder`] at the given index.
    ///
    /// If `index` is greater than or equal to the number of currently tracked
    /// progress bars, the bar is added to the end of the list.
    pub fn register_at(&self, index: usize, builder: ProgressBarBuilder) -> ProgressBar {
        self.internalize_builder(InsertLocation::Index(index), builder)
    }

    /// Registers a [`ProgressBarBuilder`] at the given index, counting from the back.
    ///
    /// If `index` is greater than or equal to the number of currently tracked
    /// progress bars, the bar is added to the start of the list.
    pub fn register_from_back(&self, index: usize, builder: ProgressBarBuilder) -> ProgressBar {
        self.internalize_builder(InsertLocation::IndexFromBack(index), builder)
    }

    /// Registers a [`ProgressBarBuilder`] before an existing progress bar.
    pub fn register_before(
        &self,
        before: &ProgressBar,
        builder: ProgressBarBuilder,
    ) -> ProgressBar {
        self.internalize_builder(InsertLocation::Before(before.index().unwrap()), builder)
    }

    /// Registers a [`ProgressBarBuilder`] after an existing progress bar.
    pub fn register_after(&self, after: &ProgressBar, builder: ProgressBarBuilder) -> ProgressBar {
        self.internalize_builder(InsertLocation::After(after.index().unwrap()), builder)
    }

    /// Adds a progress bar.
    ///
    /// The progress bar added will have the draw target changed to a
    /// remote draw target that is intercepted by the multi progress
    /// object overriding custom [`ProgressDrawTarget`] settings.
    ///
    /// The progress bar will be positioned below all other bars currently
    /// in the [`MultiProgress`].
    ///
    /// Adding a [`ProgressBar`] that is already a member of the [`MultiProgress`]
    /// will have no effect.
    #[deprecated(
        note = "use `MultiProgress::register` with a `ProgressBarBuilder` instead to avoid premature draws (see #677)"
    )]
    pub fn add(&self, pb: ProgressBar) -> ProgressBar {
        self.internalize_pb(InsertLocation::End, pb)
    }

    /// Inserts a progress bar at the given index.
    ///
    /// The progress bar inserted at position `index` will have the draw
    /// target changed to a remote draw target that is intercepted by the
    /// multi progress object overriding custom [`ProgressDrawTarget`] settings.
    ///
    /// If `index` is greater than or equal to the number of currently tracked
    /// progress bars, the bar is added to the end of the list.
    ///
    /// Inserting a [`ProgressBar`] that is already a member of the [`MultiProgress`]
    /// will have no effect.
    #[deprecated(
        note = "use `MultiProgress::register_at` with a `ProgressBarBuilder` instead to avoid premature draws (see #677)"
    )]
    pub fn insert(&self, index: usize, pb: ProgressBar) -> ProgressBar {
        self.internalize_pb(InsertLocation::Index(index), pb)
    }

    /// Inserts a progress bar at the given index, counting from the back.
    ///
    /// The progress bar is inserted counting from the end of the list.
    ///
    /// If `index` is greater than or equal to the number of currently tracked
    /// progress bars, the bar is added to the start of the list.
    ///
    /// Inserting a [`ProgressBar`] that is already a member of the [`MultiProgress`]
    /// will have no effect.
    #[deprecated(
        note = "use `MultiProgress::register_from_back` with a `ProgressBarBuilder` instead to avoid premature draws (see #677)"
    )]
    pub fn insert_from_back(&self, index: usize, pb: ProgressBar) -> ProgressBar {
        self.internalize_pb(InsertLocation::IndexFromBack(index), pb)
    }

    /// Inserts a progress bar before an existing one.
    ///
    /// The resulting progress bar will have the draw target changed to a
    /// remote draw target that is intercepted by the multi progress
    /// object overriding custom [`ProgressDrawTarget`] settings.
    ///
    /// Inserting a [`ProgressBar`] that is already a member of the [`MultiProgress`]
    /// will have no effect.
    #[deprecated(
        note = "use `MultiProgress::register_before` with a `ProgressBarBuilder` instead to avoid premature draws (see #677)"
    )]
    pub fn insert_before(&self, before: &ProgressBar, pb: ProgressBar) -> ProgressBar {
        self.internalize_pb(InsertLocation::Before(before.index().unwrap()), pb)
    }

    /// Inserts a progress bar after an existing one.
    ///
    /// The resulting progress bar will have the draw target changed to a
    /// remote draw target that is intercepted by the multi progress
    /// object overriding custom [`ProgressDrawTarget`] settings.
    ///
    /// Inserting a [`ProgressBar`] that is already a member of the [`MultiProgress`]
    /// will have no effect.
    #[deprecated(
        note = "use `MultiProgress::register_after` with a `ProgressBarBuilder` instead to avoid premature draws (see #677)"
    )]
    pub fn insert_after(&self, after: &ProgressBar, pb: ProgressBar) -> ProgressBar {
        self.internalize_pb(InsertLocation::After(after.index().unwrap()), pb)
    }

    /// Removes a progress bar.
    ///
    /// The progress bar is removed only if it was previously added to this
    /// [`MultiProgress`] via any of the [`register`](MultiProgress::register) /
    /// `register_*` methods (or the deprecated [`add`](MultiProgress::add) /
    /// `insert*` methods). If the passed progress bar does not satisfy the
    /// condition above, the `remove` method does nothing.
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

    fn internalize_pb(&self, location: InsertLocation, pb: ProgressBar) -> ProgressBar {
        let mut state = self.state.write().unwrap();
        let idx = state.insert(location);
        drop(state);

        let draw_target = ProgressDrawTarget::new_remote(self.state.clone(), idx);
        pb.set_draw_target(draw_target);
        pb
    }

    fn internalize_builder(
        &self,
        location: InsertLocation,
        builder: ProgressBarBuilder,
    ) -> ProgressBar {
        // Phase 1: read is_stderr under a brief read lock.
        let is_stderr = self.state.read().unwrap().draw_target.is_stderr();

        // Phase 2: build the ProgressBar (panic-safe — no MultiState slot is held,
        // so a panic here cannot leak a slot).
        let (pb, steady_tick) = builder.build_unregistered(is_stderr);

        // Phase 3: reserve the slot and wire up the remote draw target.
        let idx = self.state.write().unwrap().insert(location);
        let draw_target = ProgressDrawTarget::new_remote(self.state.clone(), idx);
        pb.set_draw_target(draw_target);

        // Phase 4: start steady tick LAST (spawns a thread that reads the draw target).
        if let Some(interval) = steady_tick {
            pb.enable_steady_tick(interval);
        }

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
    /// Controls how the multi progress is aligned if some of its progress bars get removed, default is `Top`
    alignment: MultiProgressAlignment,
    /// Lines to be drawn above everything else in the MultiProgress. These specifically come from
    /// calling `ProgressBar::println` on a pb that is connected to a `MultiProgress`.
    orphan_lines: Vec<LineType>,
    /// The count of currently visible zombie lines.
    zombie_lines_count: VisualLines,
}

impl MultiState {
    fn new(draw_target: ProgressDrawTarget) -> Self {
        Self {
            members: vec![],
            free_set: vec![],
            ordering: vec![],
            draw_target,
            alignment: MultiProgressAlignment::default(),
            orphan_lines: Vec::new(),
            zombie_lines_count: VisualLines::default(),
        }
    }

    pub(crate) fn mark_zombie(&mut self, index: usize) {
        let width = self.width().map(usize::from);

        let member = &mut self.members[index];

        // If the zombie is the first visual bar then we can reap it right now instead of
        // deferring it to the next draw.
        if index != self.ordering.first().copied().unwrap() {
            member.is_zombie = true;
            return;
        }

        let line_count = member
            .draw_state
            .as_ref()
            .zip(width)
            .map(|(d, width)| d.visual_line_count(.., width))
            .unwrap_or_default();

        // Track the total number of zombie lines on the screen
        self.zombie_lines_count = self.zombie_lines_count.saturating_add(line_count);

        // Make `DrawTarget` forget about the zombie lines so that they aren't cleared on next draw.
        self.draw_target
            .adjust_last_line_count(LineAdjust::Keep(line_count));

        self.remove_idx(index);
    }

    pub(crate) fn draw(
        &mut self,
        mut force_draw: bool,
        extra_lines: Option<Vec<LineType>>,
        now: Instant,
    ) -> io::Result<()> {
        if panicking() {
            return Ok(());
        }

        let width = match self.width() {
            Some(width) => width as usize,
            None => return Ok(()),
        };

        // Assumption: if extra_lines is not None, then it has at least one line
        debug_assert_eq!(
            extra_lines.is_some(),
            extra_lines.as_ref().map(Vec::len).unwrap_or_default() > 0
        );

        let mut reap_indices = vec![];

        // Reap all consecutive 'zombie' progress bars from head of the list.
        let mut adjust = VisualLines::default();
        for &index in &self.ordering {
            let member = &self.members[index];
            if !member.is_zombie {
                break;
            }

            let line_count = member
                .draw_state
                .as_ref()
                .map(|d| d.visual_line_count(.., width))
                .unwrap_or_default();
            // Track the total number of zombie lines on the screen.
            self.zombie_lines_count += line_count;

            // Track the number of zombie lines that will be drawn by this call to draw.
            adjust += line_count;

            reap_indices.push(index);
        }

        // If this draw is due to a `println`, then we need to erase all the zombie lines.
        // This is because `println` is supposed to appear above all other elements in the
        // `MultiProgress`.
        if extra_lines.is_some() {
            self.draw_target
                .adjust_last_line_count(LineAdjust::Clear(self.zombie_lines_count));
            self.zombie_lines_count = VisualLines::default();
        }

        let orphan_visual_line_count = visual_line_count(&self.orphan_lines, width);
        force_draw |= orphan_visual_line_count > VisualLines::default();
        let mut drawable = match self.draw_target.drawable(force_draw, now) {
            Some(drawable) => drawable,
            None => return Ok(()),
        };

        let mut draw_state = drawable.state();
        draw_state.alignment = self.alignment;

        if let Some(extra_lines) = &extra_lines {
            draw_state.lines.extend_from_slice(extra_lines.as_slice());
        }

        // Add lines from `ProgressBar::println` call.
        draw_state.lines.append(&mut self.orphan_lines);

        for index in &self.ordering {
            let member = &self.members[*index];
            if let Some(state) = &member.draw_state {
                draw_state.lines.extend_from_slice(&state.lines[..]);
            }
        }

        drop(draw_state);
        let drawable = drawable.draw();

        for index in reap_indices {
            self.remove_idx(index);
        }

        // The zombie lines were drawn for the last time, so make `DrawTarget` forget about them
        // so they aren't cleared on next draw.
        if extra_lines.is_none() {
            self.draw_target
                .adjust_last_line_count(LineAdjust::Keep(adjust));
        }

        drawable
    }

    pub(crate) fn println<I: AsRef<str>>(&mut self, msg: I, now: Instant) -> io::Result<()> {
        let msg = msg.as_ref();

        // If msg is "", make sure a line is still printed
        let lines: Vec<LineType> = match msg.is_empty() {
            false => msg.lines().map(|l| LineType::Text(Into::into(l))).collect(),
            true => vec![LineType::Empty],
        };

        self.draw(true, Some(lines), now)
    }

    pub(crate) fn draw_state(&mut self, idx: usize) -> DrawStateWrapper<'_> {
        let member = self.members.get_mut(idx).unwrap();
        // alignment is handled by the `MultiProgress`'s underlying draw target, so there is no
        // point in propagating it here.
        let state = member.draw_state.get_or_insert(DrawState::default());

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

    pub(crate) fn width(&self) -> Option<u16> {
        self.draw_target.width()
    }

    fn insert(&mut self, location: InsertLocation) -> usize {
        let idx = if let Some(idx) = self.free_set.pop() {
            self.members[idx] = MultiStateMember::default();
            idx
        } else {
            self.members.push(MultiStateMember::default());
            self.members.len() - 1
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
            Some(mut drawable) => {
                // Make the clear operation also wipe out zombie lines
                drawable.adjust_last_line_count(LineAdjust::Clear(self.zombie_lines_count));
                self.zombie_lines_count = VisualLines::default();
                drawable.clear()
            }
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
    /// Whether the corresponding progress bar (more precisely, `BarState`) has been dropped.
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
/// E.g. [`Top`](MultiProgressAlignment::Top) alignment (default), when _progress bar 2_ is removed:
/// ```ignore
/// [0/100] progress bar 1        [0/100] progress bar 1
/// [0/100] progress bar 2   =>   [0/100] progress bar 3
/// [0/100] progress bar 3
/// ```
///
/// [`Bottom`](MultiProgressAlignment::Bottom) alignment
/// ```ignore
/// [0/100] progress bar 1
/// [0/100] progress bar 2   =>   [0/100] progress bar 1
/// [0/100] progress bar 3        [0/100] progress bar 3
/// ```
#[derive(Debug, Copy, Clone, Default)]
pub enum MultiProgressAlignment {
    #[default]
    Top,
    Bottom,
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
    use crate::{MultiProgress, ProgressBar, ProgressBarBuilder, ProgressDrawTarget};

    #[test]
    #[allow(deprecated)]
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
        let pb = mpb.register(ProgressBarBuilder::new(123));
        pb.finish();
    }

    #[test]
    fn multi_progress_modifications() {
        let mp = MultiProgress::new();
        let p0 = mp.register(ProgressBarBuilder::new(1));
        let p1 = mp.register(ProgressBarBuilder::new(1));
        let p2 = mp.register(ProgressBarBuilder::new(1));
        let p3 = mp.register(ProgressBarBuilder::new(1));
        mp.remove(&p2);
        mp.remove(&p1);
        let p4 = mp.register_at(1, ProgressBarBuilder::new(1));

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
    fn multi_progress_register_from_back() {
        let mp = MultiProgress::new();
        let p0 = mp.register(ProgressBarBuilder::new(1));
        let p1 = mp.register(ProgressBarBuilder::new(1));
        let p2 = mp.register(ProgressBarBuilder::new(1));
        let p3 = mp.register_from_back(1, ProgressBarBuilder::new(1));
        let p4 = mp.register_from_back(10, ProgressBarBuilder::new(1));

        let state = mp.state.read().unwrap();
        assert_eq!(state.ordering, vec![4, 0, 1, 3, 2]);
        assert_eq!(p0.index().unwrap(), 0);
        assert_eq!(p1.index().unwrap(), 1);
        assert_eq!(p2.index().unwrap(), 2);
        assert_eq!(p3.index().unwrap(), 3);
        assert_eq!(p4.index().unwrap(), 4);
    }

    #[test]
    fn multi_progress_register_after() {
        let mp = MultiProgress::new();
        let p0 = mp.register(ProgressBarBuilder::new(1));
        let p1 = mp.register(ProgressBarBuilder::new(1));
        let p2 = mp.register(ProgressBarBuilder::new(1));
        let p3 = mp.register_after(&p2, ProgressBarBuilder::new(1));
        let p4 = mp.register_after(&p0, ProgressBarBuilder::new(1));

        let state = mp.state.read().unwrap();
        assert_eq!(state.ordering, vec![0, 4, 1, 2, 3]);
        assert_eq!(p0.index().unwrap(), 0);
        assert_eq!(p1.index().unwrap(), 1);
        assert_eq!(p2.index().unwrap(), 2);
        assert_eq!(p3.index().unwrap(), 3);
        assert_eq!(p4.index().unwrap(), 4);
    }

    #[test]
    fn multi_progress_register_before() {
        let mp = MultiProgress::new();
        let p0 = mp.register(ProgressBarBuilder::new(1));
        let p1 = mp.register(ProgressBarBuilder::new(1));
        let p2 = mp.register(ProgressBarBuilder::new(1));
        let p3 = mp.register_before(&p0, ProgressBarBuilder::new(1));
        let p4 = mp.register_before(&p2, ProgressBarBuilder::new(1));

        let state = mp.state.read().unwrap();
        assert_eq!(state.ordering, vec![3, 0, 1, 4, 2]);
        assert_eq!(p0.index().unwrap(), 0);
        assert_eq!(p1.index().unwrap(), 1);
        assert_eq!(p2.index().unwrap(), 2);
        assert_eq!(p3.index().unwrap(), 3);
        assert_eq!(p4.index().unwrap(), 4);
    }

    #[test]
    fn multi_progress_register_before_and_after() {
        let mp = MultiProgress::new();
        let p0 = mp.register(ProgressBarBuilder::new(1));
        let p1 = mp.register(ProgressBarBuilder::new(1));
        let p2 = mp.register(ProgressBarBuilder::new(1));
        let p3 = mp.register_before(&p0, ProgressBarBuilder::new(1));
        let p4 = mp.register_after(&p3, ProgressBarBuilder::new(1));
        let p5 = mp.register_after(&p3, ProgressBarBuilder::new(1));
        let p6 = mp.register_before(&p1, ProgressBarBuilder::new(1));

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
        let p0 = mp.register(ProgressBarBuilder::new(1));
        let p1 = mp.register(ProgressBarBuilder::new(1));
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

    #[test]
    #[allow(deprecated)]
    fn mp_no_crash_double_add() {
        let mp = MultiProgress::new();
        let pb = mp.add(ProgressBar::new(10));
        mp.add(pb);
    }
}
