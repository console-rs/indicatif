use std::borrow::Cow;
use std::fmt;
use std::io;
use std::sync::RwLock;
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::{Duration, Instant};

use crate::draw_target::{
    MultiProgressAlignment, MultiProgressState, ProgressDrawState, ProgressDrawTarget,
};
use crate::state::{ProgressState, Status};
use crate::style::ProgressStyle;
use crate::{ProgressBarIter, ProgressIterator};

/// A progress bar or spinner
///
/// The progress bar is an [`Arc`] around its internal state. When the progress bar is cloned it
/// just increments the refcount (so the original and its clone share the same state).
#[derive(Clone)]
pub struct ProgressBar {
    state: Arc<Mutex<ProgressState>>,
}

impl fmt::Debug for ProgressBar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProgressBar").finish()
    }
}

impl ProgressBar {
    /// Creates a new progress bar with a given length
    ///
    /// This progress bar by default draws directly to stderr, and refreshes a maximum of 15 times
    /// a second. To change the refresh rate, set the draw target to one with a different refresh
    /// rate.
    pub fn new(len: u64) -> ProgressBar {
        ProgressBar::with_draw_target(len, ProgressDrawTarget::stderr())
    }

    /// Creates a completely hidden progress bar
    ///
    /// This progress bar still responds to API changes but it does not have a length or render in
    /// any way.
    pub fn hidden() -> ProgressBar {
        ProgressBar::with_draw_target(!0, ProgressDrawTarget::hidden())
    }

    /// Creates a new progress bar with a given length and draw target
    pub fn with_draw_target(len: u64, target: ProgressDrawTarget) -> ProgressBar {
        ProgressBar {
            state: Arc::new(Mutex::new(ProgressState::new(len, target))),
        }
    }

    /// A convenience builder-like function for a progress bar with a given style
    pub fn with_style(self, style: ProgressStyle) -> ProgressBar {
        self.state.lock().unwrap().style = style;
        self
    }

    /// A convenience builder-like function for a progress bar with a given prefix
    pub fn with_prefix(self, prefix: impl Into<Cow<'static, str>>) -> ProgressBar {
        self.state.lock().unwrap().prefix = prefix.into();
        self
    }

    /// A convenience builder-like function for a progress bar with a given message
    pub fn with_message(self, message: impl Into<Cow<'static, str>>) -> ProgressBar {
        self.state.lock().unwrap().message = message.into();
        self
    }

    /// A convenience builder-like function for a progress bar with a given position
    pub fn with_position(self, pos: u64) -> ProgressBar {
        self.state.lock().unwrap().pos = pos;
        self
    }

    /// Creates a new spinner
    ///
    /// This spinner by default draws directly to stderr. This adds the default spinner style to it.
    pub fn new_spinner() -> ProgressBar {
        let rv = ProgressBar::new(!0);
        rv.set_style(ProgressStyle::default_spinner());
        rv
    }

    /// Overrides the stored style
    ///
    /// This does not redraw the bar. Call [`ProgressBar::tick()`] to force it.
    pub fn set_style(&self, style: ProgressStyle) {
        self.state.lock().unwrap().style = style;
    }

    /// Spawns a background thread to tick the progress bar
    ///
    /// When this is enabled a background thread will regularly tick the progress bar in the given
    /// interval (in milliseconds). This is useful to advance progress bars that are very slow by
    /// themselves.
    ///
    /// When steady ticks are enabled, calling [`ProgressBar::tick()`] on a progress bar does not
    /// have any effect.
    pub fn enable_steady_tick(&self, ms: u64) {
        let mut state = self.state.lock().unwrap();
        state.steady_tick = ms;
        if state.tick_thread.is_some() {
            return;
        }

        // Using a weak pointer is required to prevent a potential deadlock. See issue #133
        let state_arc = Arc::downgrade(&self.state);
        state.tick_thread = Some(thread::spawn(move || Self::steady_tick(state_arc, ms)));

        ::std::mem::drop(state);
        // use the side effect of tick to force the bar to tick.
        self.tick();
    }

    fn steady_tick(state_arc: Weak<Mutex<ProgressState>>, mut ms: u64) {
        loop {
            thread::sleep(Duration::from_millis(ms));
            if let Some(state_arc) = state_arc.upgrade() {
                let mut state = state_arc.lock().unwrap();
                if state.is_finished() || state.steady_tick == 0 {
                    state.steady_tick = 0;
                    state.tick_thread = None;
                    break;
                }
                if state.tick != 0 {
                    state.tick = state.tick.saturating_add(1);
                }
                ms = state.steady_tick;

                state.draw().ok();
            } else {
                break;
            }
        }
    }

    /// Undoes [`ProgressBar::enable_steady_tick()`]
    pub fn disable_steady_tick(&self) {
        self.enable_steady_tick(0);
    }

    /// Limit redrawing of progress bar to every `n` steps
    ///
    /// By default, the progress bar will redraw whenever its state advances. This setting is
    /// helpful in situations where the overhead of redrawing the progress bar dominates the
    /// computation whose progress is being reported.
    ///
    /// If `n` is greater than 0, operations that change the progress bar such as
    /// [`ProgressBar::tick()`], [`ProgressBar::set_message()`] and [`ProgressBar::set_length()`]
    /// will no longer cause the progress bar to be redrawn, and will only be shown once the
    /// position advances by `n` steps.
    ///
    /// ```rust,no_run
    /// # use indicatif::ProgressBar;
    /// let n = 1_000_000;
    /// let pb = ProgressBar::new(n);
    /// pb.set_draw_delta(n / 100); // redraw every 1% of additional progress
    /// ```
    ///
    /// Note that `ProgressDrawTarget` may impose additional buffering of redraws.
    pub fn set_draw_delta(&self, n: u64) {
        let mut state = self.state.lock().unwrap();
        state.draw_delta = n;
        state.draw_next = state.pos.saturating_add(state.draw_delta);
    }

    /// Sets the refresh rate of progress bar to `n` updates per seconds
    ///
    /// This is similar to `set_draw_delta` but automatically adapts to a constant refresh rate
    /// regardless of how consistent the progress is.
    ///
    /// This parameter takes precedence on `set_draw_delta` if different from 0.
    ///
    /// ```rust,no_run
    /// # use indicatif::ProgressBar;
    /// let n = 1_000_000;
    /// let pb = ProgressBar::new(n);
    /// pb.set_draw_rate(25); // aims at redrawing at most 25 times per seconds.
    /// ```
    ///
    /// Note that the [`ProgressDrawTarget`] may impose additional buffering of redraws.
    pub fn set_draw_rate(&self, n: u64) {
        let mut state = self.state.lock().unwrap();
        state.draw_rate = n;
        state.draw_next = state
            .pos
            .saturating_add((state.per_sec() / n as f64) as u64);
    }

    /// Manually ticks the spinner or progress bar
    ///
    /// This automatically happens on any other change to a progress bar.
    pub fn tick(&self) {
        self.update_and_draw(|state| {
            if state.steady_tick == 0 || state.tick == 0 {
                state.tick = state.tick.saturating_add(1);
            }
        });
    }

    /// Advances the position of the progress bar by `delta`
    pub fn inc(&self, delta: u64) {
        self.update_and_draw(|state| {
            state.pos = state.pos.saturating_add(delta);
            if state.steady_tick == 0 || state.tick == 0 {
                state.tick = state.tick.saturating_add(1);
            }
        })
    }

    /// A quick convenience check if the progress bar is hidden
    pub fn is_hidden(&self) -> bool {
        self.state.lock().unwrap().draw_target.is_hidden()
    }

    /// Indicates that the progress bar finished
    pub fn is_finished(&self) -> bool {
        self.state.lock().unwrap().is_finished()
    }

    /// Print a log line above the progress bar
    ///
    /// If the progress bar was added to a [`MultiProgress`], the log line will be
    /// printed above all other progress bars.
    ///
    /// Note that if the progress bar is hidden (which by default happens if the progress bar is
    /// redirected into a file) `println()` will not do anything either.
    pub fn println<I: AsRef<str>>(&self, msg: I) {
        let mut state = self.state.lock().unwrap();

        let mut lines: Vec<String> = msg.as_ref().lines().map(Into::into).collect();
        let orphan_lines = lines.len();
        if state.should_render() && !state.draw_target.is_hidden() {
            lines.extend(state.style.format_state(&*state));
        }

        let draw_state = ProgressDrawState {
            lines,
            orphan_lines,
            finished: state.is_finished(),
            force_draw: true,
            move_cursor: false,
            alignment: Default::default(),
        };

        state.draw_target.apply_draw_state(draw_state).ok();
    }

    /// Sets the position of the progress bar
    pub fn set_position(&self, pos: u64) {
        self.update_and_draw(|state| {
            state.draw_next = pos;
            state.pos = pos;
            if state.steady_tick == 0 || state.tick == 0 {
                state.tick = state.tick.saturating_add(1);
            }
        })
    }

    /// Sets the length of the progress bar
    pub fn set_length(&self, len: u64) {
        self.update_and_draw(|state| {
            state.len = len;
        })
    }

    /// Increase the length of the progress bar
    pub fn inc_length(&self, delta: u64) {
        self.update_and_draw(|state| {
            state.len = state.len.saturating_add(delta);
        })
    }

    /// Sets the current prefix of the progress bar
    ///
    /// For the prefix to be visible, the `{prefix}` placeholder must be present in the template
    /// (see [`ProgressStyle`]).
    pub fn set_prefix(&self, prefix: impl Into<Cow<'static, str>>) {
        let prefix = prefix.into();
        self.update_and_draw(|state| {
            state.prefix = prefix;
            if state.steady_tick == 0 || state.tick == 0 {
                state.tick = state.tick.saturating_add(1);
            }
        })
    }

    /// Sets the current message of the progress bar
    ///
    /// For the message to be visible, the `{msg}` placeholder must be present in the template (see
    /// [`ProgressStyle`]).
    pub fn set_message(&self, msg: impl Into<Cow<'static, str>>) {
        let msg = msg.into();
        self.update_and_draw(|state| {
            state.message = msg;
            if state.steady_tick == 0 || state.tick == 0 {
                state.tick = state.tick.saturating_add(1);
            }
        })
    }

    /// Creates a new weak reference to this `ProgressBar`
    pub fn downgrade(&self) -> WeakProgressBar {
        WeakProgressBar {
            state: Arc::downgrade(&self.state),
        }
    }

    /// Resets the ETA calculation
    ///
    /// This can be useful if the progress bars made a large jump or was paused for a prolonged
    /// time.
    pub fn reset_eta(&self) {
        self.update_and_draw(|state| {
            state.est.reset(state.pos);
        });
    }

    /// Resets elapsed time
    pub fn reset_elapsed(&self) {
        self.update_and_draw(|state| {
            state.started = Instant::now();
        });
    }

    /// Resets all of the progress bar state
    pub fn reset(&self) {
        self.reset_eta();
        self.reset_elapsed();
        self.update_and_draw(|state| {
            state.draw_next = 0;
            state.pos = 0;
            state.status = Status::InProgress;
        });
    }

    /// Finishes the progress bar and leaves the current message
    pub fn finish(&self) {
        self.state.lock().unwrap().finish();
    }

    /// Finishes the progress bar at current position and leaves the current message
    pub fn finish_at_current_pos(&self) {
        self.state.lock().unwrap().finish_at_current_pos();
    }

    /// Finishes the progress bar and sets a message
    ///
    /// For the message to be visible, the `{msg}` placeholder must be present in the template (see
    /// [`ProgressStyle`]).
    pub fn finish_with_message(&self, msg: impl Into<Cow<'static, str>>) {
        self.state.lock().unwrap().finish_with_message(msg);
    }

    /// Finishes the progress bar and completely clears it
    pub fn finish_and_clear(&self) {
        self.state.lock().unwrap().finish_and_clear();
    }

    /// Finishes the progress bar and leaves the current message and progress
    pub fn abandon(&self) {
        self.state.lock().unwrap().abandon();
    }

    /// Finishes the progress bar and sets a message, and leaves the current progress
    ///
    /// For the message to be visible, the `{msg}` placeholder must be present in the template (see
    /// [`ProgressStyle`]).
    pub fn abandon_with_message(&self, msg: impl Into<Cow<'static, str>>) {
        self.state.lock().unwrap().abandon_with_message(msg);
    }

    /// Finishes the progress bar using the behavior stored in the [`ProgressStyle`]
    ///
    /// See [`ProgressStyle::on_finish()`].
    pub fn finish_using_style(&self) {
        self.state.lock().unwrap().finish_using_style();
    }

    /// Sets a different draw target for the progress bar
    ///
    /// This can be used to draw the progress bar to stderr (this is the default):
    ///
    /// ```rust,no_run
    /// # use indicatif::{ProgressBar, ProgressDrawTarget};
    /// let pb = ProgressBar::new(100);
    /// pb.set_draw_target(ProgressDrawTarget::stderr());
    /// ```
    ///
    /// **Note:** Calling this method on a [`ProgressBar`] linked with a [`MultiProgress`] (after
    /// running [`MultiProgress::add`]) will unlink this progress bar. If you don't want this
    /// behavior, call [`MultiProgress::set_draw_target`] instead.
    pub fn set_draw_target(&self, target: ProgressDrawTarget) {
        let mut state = self.state.lock().unwrap();
        state.draw_target.disconnect();
        state.draw_target = target;
    }

    /// Wraps an [`Iterator`] with the progress bar
    ///
    /// ```rust,no_run
    /// # use indicatif::ProgressBar;
    /// let v = vec![1, 2, 3];
    /// let pb = ProgressBar::new(3);
    /// for item in pb.wrap_iter(v.iter()) {
    ///     // ...
    /// }
    /// ```
    pub fn wrap_iter<It: Iterator>(&self, it: It) -> ProgressBarIter<It> {
        it.progress_with(self.clone())
    }

    /// Wraps an [`io::Read`] with the progress bar
    ///
    /// ```rust,no_run
    /// # use std::fs::File;
    /// # use std::io;
    /// # use indicatif::ProgressBar;
    /// # fn test () -> io::Result<()> {
    /// let source = File::open("work.txt")?;
    /// let mut target = File::create("done.txt")?;
    /// let pb = ProgressBar::new(source.metadata()?.len());
    /// io::copy(&mut pb.wrap_read(source), &mut target);
    /// # Ok(())
    /// # }
    /// ```
    pub fn wrap_read<R: io::Read>(&self, read: R) -> ProgressBarIter<R> {
        ProgressBarIter {
            progress: self.clone(),
            it: read,
        }
    }

    /// Wraps an [`io::Write`] with the progress bar
    ///
    /// ```rust,no_run
    /// # use std::fs::File;
    /// # use std::io;
    /// # use indicatif::ProgressBar;
    /// # fn test () -> io::Result<()> {
    /// let mut source = File::open("work.txt")?;
    /// let target = File::create("done.txt")?;
    /// let pb = ProgressBar::new(source.metadata()?.len());
    /// io::copy(&mut source, &mut pb.wrap_write(target));
    /// # Ok(())
    /// # }
    /// ```
    pub fn wrap_write<W: io::Write>(&self, write: W) -> ProgressBarIter<W> {
        ProgressBarIter {
            progress: self.clone(),
            it: write,
        }
    }

    fn update_and_draw<F: FnOnce(&mut ProgressState)>(&self, f: F) {
        // Delegate to the wrapped state.
        let mut state = self.state.lock().unwrap();
        state.update_and_draw(f);
    }

    /// Returns the current position
    pub fn position(&self) -> u64 {
        self.state.lock().unwrap().pos
    }

    /// Returns the current length
    pub fn length(&self) -> u64 {
        self.state.lock().unwrap().len
    }

    /// Returns the current ETA
    pub fn eta(&self) -> Duration {
        self.state.lock().unwrap().eta()
    }

    /// Returns the current rate of progress
    pub fn per_sec(&self) -> f64 {
        self.state.lock().unwrap().per_sec()
    }

    /// Returns the current expected duration
    pub fn duration(&self) -> Duration {
        self.state.lock().unwrap().duration()
    }

    /// Returns the current elapsed time
    pub fn elapsed(&self) -> Duration {
        self.state.lock().unwrap().started.elapsed()
    }
}

/// Manages multiple progress bars from different threads
#[derive(Debug)]
pub struct MultiProgress {
    state: Arc<RwLock<MultiProgressState>>,
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
            state: Arc::new(RwLock::new(MultiProgressState {
                draw_states: Vec::new(),
                free_set: Vec::new(),
                ordering: vec![],
                draw_target,
                move_cursor: false,
                alignment: Default::default(),
            })),
        }
    }

    /// Sets a different draw target for the multiprogress bar.
    pub fn set_draw_target(&self, target: ProgressDrawTarget) {
        let mut state = self.state.write().unwrap();
        state.draw_target.disconnect();
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
        self.push(None, pb)
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
        self.push(Some(index), pb)
    }

    fn push(&self, pos: Option<usize>, pb: ProgressBar) -> ProgressBar {
        let mut state = self.state.write().unwrap();
        let idx = match state.free_set.pop() {
            Some(idx) => {
                state.draw_states[idx] = None;
                idx
            }
            None => {
                state.draw_states.push(None);
                state.draw_states.len() - 1
            }
        };

        match pos {
            Some(pos) if pos < state.ordering.len() => state.ordering.insert(pos, idx),
            _ => state.ordering.push(idx),
        }

        assert!(
            state.len() == state.ordering.len(),
            "Draw state is inconsistent"
        );

        pb.set_draw_target(ProgressDrawTarget::new_remote(self.state.clone(), idx));
        pb
    }

    /// Removes a progress bar.
    ///
    /// The progress bar is removed only if it was previously inserted or added
    /// by the methods `MultiProgress::insert` or `MultiProgress::add`.
    /// If the passed progress bar does not satisfy the condition above,
    /// the `remove` method does nothing.
    pub fn remove(&self, pb: &ProgressBar) {
        let idx = match &pb.state.lock().unwrap().draw_target.remote() {
            Some((state, idx)) => {
                // Check that this progress bar is owned by the current MultiProgress.
                assert!(Arc::ptr_eq(&self.state, state));
                *idx
            }
            _ => return,
        };

        self.state.write().unwrap().remove_idx(idx);
    }

    pub fn clear(&self) -> io::Result<()> {
        let mut state = self.state.write().unwrap();
        let move_cursor = state.move_cursor;
        let alignment = state.alignment;
        state.draw_target.apply_draw_state(ProgressDrawState {
            lines: vec![],
            orphan_lines: 0,
            finished: true,
            force_draw: true,
            move_cursor,
            alignment,
        })?;

        Ok(())
    }
}

/// A weak reference to a `ProgressBar`.
///
/// Useful for creating custom steady tick implementations
#[derive(Clone)]
pub struct WeakProgressBar {
    state: Weak<Mutex<ProgressState>>,
}

impl WeakProgressBar {
    /// Attempts to upgrade the Weak pointer to a [`ProgressBar`], delaying dropping of the inner
    /// value if successful. Returns `None` if the inner value has since been dropped.
    ///
    /// [`ProgressBar`]: struct.ProgressBar.html
    pub fn upgrade(&self) -> Option<ProgressBar> {
        self.state.upgrade().map(|state| ProgressBar { state })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::float_cmp)]
    #[test]
    fn test_pbar_zero() {
        let pb = ProgressBar::new(0);
        assert_eq!(pb.state.lock().unwrap().fraction(), 1.0);
    }

    #[allow(clippy::float_cmp)]
    #[test]
    fn test_pbar_maxu64() {
        let pb = ProgressBar::new(!0);
        assert_eq!(pb.state.lock().unwrap().fraction(), 0.0);
    }

    #[test]
    fn test_pbar_overflow() {
        let pb = ProgressBar::new(1);
        pb.set_draw_target(ProgressDrawTarget::hidden());
        pb.inc(2);
        pb.finish();
    }

    #[test]
    fn test_get_position() {
        let pb = ProgressBar::new(1);
        pb.set_draw_target(ProgressDrawTarget::hidden());
        pb.inc(2);
        let pos = pb.position();
        assert_eq!(pos, 2);
    }

    #[test]
    fn test_weak_pb() {
        let pb = ProgressBar::new(0);
        let weak = pb.downgrade();
        assert!(weak.upgrade().is_some());
        ::std::mem::drop(pb);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn test_draw_delta_deadlock() {
        // see issue #187
        let mpb = MultiProgress::new();
        let pb = mpb.add(ProgressBar::new(1));
        pb.set_draw_delta(2);
        drop(pb);
    }

    #[test]
    fn test_abandon_deadlock() {
        let mpb = MultiProgress::new();
        let pb = mpb.add(ProgressBar::new(1));
        pb.set_draw_delta(2);
        pb.abandon();
        drop(pb);
    }

    #[test]
    fn late_pb_drop() {
        let pb = ProgressBar::new(10);
        let mpb = MultiProgress::new();
        // This clone call is required to trigger a now fixed bug.
        // See <https://github.com/mitsuhiko/indicatif/pull/141> for context
        #[allow(clippy::redundant_clone)]
        mpb.add(pb.clone());
    }

    #[test]
    fn it_can_wrap_a_reader() {
        let bytes = &b"I am an implementation of io::Read"[..];
        let pb = ProgressBar::new(bytes.len() as u64);
        let mut reader = pb.wrap_read(bytes);
        let mut writer = Vec::new();
        io::copy(&mut reader, &mut writer).unwrap();
        assert_eq!(writer, bytes);
    }

    #[test]
    fn it_can_wrap_a_writer() {
        let bytes = b"implementation of io::Read";
        let mut reader = &bytes[..];
        let pb = ProgressBar::new(bytes.len() as u64);
        let writer = Vec::new();
        let mut writer = pb.wrap_write(writer);
        io::copy(&mut reader, &mut writer).unwrap();
        assert_eq!(writer.it, bytes);
    }

    #[test]
    fn progress_bar_sync_send() {
        let _: Box<dyn Sync> = Box::new(ProgressBar::new(1));
        let _: Box<dyn Send> = Box::new(ProgressBar::new(1));
        let _: Box<dyn Sync> = Box::new(MultiProgress::new());
        let _: Box<dyn Send> = Box::new(MultiProgress::new());
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

    #[test]
    fn multi_progress_hidden() {
        let mpb = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mpb.add(ProgressBar::new(123));
        pb.finish();
    }
}
