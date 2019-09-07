use std::fmt;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::sync::{Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::style::ProgressStyle;
use crate::utils::{duration_to_secs, secs_to_duration, Estimate};
use console::Term;

/// The drawn state of an element.
#[derive(Clone, Debug)]
struct ProgressDrawState {
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
    /// Time when the draw state was created.
    pub ts: Instant,
}

#[derive(Debug)]
enum Status {
    InProgress,
    DoneVisible,
    DoneHidden,
}

enum ProgressDrawTargetKind {
    Term(Term, Option<ProgressDrawState>, Option<Duration>),
    Remote(usize, Mutex<Sender<(usize, ProgressDrawState)>>),
    Hidden,
}

/// Target for draw operations
///
/// This tells a progress bar or a multi progress object where to paint to.
/// The draw target is a stateful wrapper over a drawing destination and
/// internally optimizes how often the state is painted to the output
/// device.
pub struct ProgressDrawTarget {
    kind: ProgressDrawTargetKind,
}

impl ProgressDrawTarget {
    /// Draw to a buffered stdout terminal at a max of 15 times a second.
    ///
    /// For more information see `ProgressDrawTarget::to_term`.
    pub fn stdout() -> ProgressDrawTarget {
        ProgressDrawTarget::to_term(Term::buffered_stdout(), Some(15))
    }

    /// Draw to a buffered stderr terminal at a max of 15 times a second.
    ///
    /// This is the default draw target for progress bars.  For more
    /// information see `ProgressDrawTarget::to_term`.
    pub fn stderr() -> ProgressDrawTarget {
        ProgressDrawTarget::to_term(Term::buffered_stderr(), Some(15))
    }

    /// Draw to a buffered stdout terminal without max framerate.
    ///
    /// This is useful when data is known to come in very slowly and
    /// not rendering some updates would be a problem (for instance
    /// when messages are used extensively).
    ///
    /// For more information see `ProgressDrawTarget::to_term`.
    pub fn stdout_nohz() -> ProgressDrawTarget {
        ProgressDrawTarget::to_term(Term::buffered_stdout(), None)
    }

    /// Draw to a buffered stderr terminal without max framerate.
    ///
    /// This is useful when data is known to come in very slowly and
    /// not rendering some updates would be a problem (for instance
    /// when messages are used extensively).
    ///
    /// For more information see `ProgressDrawTarget::to_term`.
    pub fn stderr_nohz() -> ProgressDrawTarget {
        ProgressDrawTarget::to_term(Term::buffered_stderr(), None)
    }

    /// Draw to a terminal, optionally with a specific refresh rate.
    ///
    /// Progress bars are by default drawn to terminals however if the
    /// terminal is not user attended the entire progress bar will be
    /// hidden.  This is done so that piping to a file will not produce
    /// useless escape codes in that file.
    pub fn to_term(term: Term, refresh_rate: Option<u64>) -> ProgressDrawTarget {
        let rate = refresh_rate.map(|x| Duration::from_millis(1000 / x));
        ProgressDrawTarget {
            kind: ProgressDrawTargetKind::Term(term, None, rate),
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
            ProgressDrawTargetKind::Term(ref term, ..) => !term.is_term(),
            _ => false,
        }
    }

    /// Apply the given draw state (draws it).
    fn apply_draw_state(&mut self, draw_state: ProgressDrawState) -> io::Result<()> {
        // no need to apply anything to hidden draw targets.
        if self.is_hidden() {
            return Ok(());
        }
        match self.kind {
            ProgressDrawTargetKind::Term(ref term, ref mut last_state, rate) => {
                let last_draw = last_state.as_ref().map(|x| x.ts);
                if draw_state.finished
                    || draw_state.force_draw
                    || rate.is_none()
                    || last_draw.is_none()
                    || last_draw.unwrap().elapsed() > rate.unwrap()
                {
                    if let Some(ref last_state) = *last_state {
                        if !draw_state.lines.is_empty() && draw_state.move_cursor {
                            last_state.move_cursor(term)?;
                        } else {
                            last_state.clear_term(term)?;
                        }
                    }
                    draw_state.draw_to_term(term)?;
                    term.flush()?;
                    *last_state = Some(draw_state);
                }
            }
            ProgressDrawTargetKind::Remote(idx, ref chan) => {
                chan.lock().unwrap().send((idx, draw_state)).unwrap();
            }
            ProgressDrawTargetKind::Hidden => {}
        }
        Ok(())
    }

    /// Properly disconnects from the draw target
    fn disconnect(&self) {
        match self.kind {
            ProgressDrawTargetKind::Term(_, _, _) => {}
            ProgressDrawTargetKind::Remote(idx, ref chan) => {
                chan.lock()
                    .unwrap()
                    .send((
                        idx,
                        ProgressDrawState {
                            lines: vec![],
                            orphan_lines: 0,
                            finished: true,
                            force_draw: false,
                            move_cursor: false,
                            ts: Instant::now(),
                        },
                    ))
                    .ok();
            }
            ProgressDrawTargetKind::Hidden => {}
        };
    }
}

impl ProgressDrawState {
    pub fn clear_term(&self, term: &Term) -> io::Result<()> {
        term.clear_last_lines(self.lines.len() - self.orphan_lines)
    }

    pub fn move_cursor(&self, term: &Term) -> io::Result<()> {
        term.move_cursor_up(self.lines.len() - self.orphan_lines)
    }

    pub fn draw_to_term(&self, term: &Term) -> io::Result<()> {
        for line in &self.lines {
            term.write_line(line)?;
        }
        Ok(())
    }
}

impl Drop for ProgressDrawTarget {
    fn drop(&mut self) {
        self.disconnect();
    }
}

/// The state of a progress bar at a moment in time.
pub(crate) struct ProgressState {
    pub(crate) style: ProgressStyle,
    pub(crate) pos: u64,
    pub(crate) len: u64,
    pub(crate) tick: u64,
    pub(crate) started: Instant,
    draw_target: ProgressDrawTarget,
    width: Option<u16>,
    message: String,
    prefix: String,
    draw_delta: u64,
    draw_next: u64,
    status: Status,
    est: Estimate,
    tick_thread: Option<thread::JoinHandle<()>>,
    steady_tick: u64,
}

impl ProgressState {
    /// Returns the character that should be drawn for the
    /// current spinner character.
    pub fn current_tick_char(&self) -> char {
        if self.is_finished() {
            self.style.get_final_tick_char()
        } else {
            self.style.get_tick_char(self.tick)
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
    pub fn should_render(&self) -> bool {
        match self.status {
            Status::DoneHidden => false,
            _ => true,
        }
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

    /// Returns the position of the status bar as `(pos, len)` tuple.
    pub fn position(&self) -> (u64, u64) {
        (self.pos, self.len)
    }

    /// Returns the current message of the progress bar.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the current prefix of the progress bar.
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// The entire draw width
    pub fn width(&self) -> usize {
        if let Some(width) = self.width {
            width as usize
        } else {
            Term::stderr().size().1 as usize
        }
    }

    /// Return the current average time per step
    pub fn avg_time_per_step(&self) -> Duration {
        self.est.time_per_step()
    }

    /// The expected ETA
    pub fn eta(&self) -> Duration {
        if self.len == !0 || self.is_finished() {
            return Duration::new(0, 0);
        }
        let t = duration_to_secs(self.avg_time_per_step());
        // add 0.75 to leave 0.25 sec of 0s for the user
        secs_to_duration(t * self.len.saturating_sub(self.pos) as f64 + 0.75)
    }

    /// The number of steps per second
    pub fn per_sec(&self) -> u64 {
        let avg_time = self.avg_time_per_step().as_nanos();
        if avg_time == 0 {
            0
        } else {
            (1_000_000_000 / avg_time) as u64
        }
    }
}

/// A progress bar or spinner.
///
/// The progress bar is an `Arc` around an internal state.  When the progress
/// bar is cloned it just increments the refcount which means the bar is
/// shared with the original one.
#[derive(Clone)]
pub struct ProgressBar {
    state: Arc<RwLock<ProgressState>>,
}

impl fmt::Debug for ProgressBar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProgressBar").finish()
    }
}

impl ProgressBar {
    /// Creates a new progress bar with a given length.
    ///
    /// This progress bar by default draws directly to stderr, and refreshes
    /// a maximum of 15 times a second. To change the refresh rate set the
    /// draw target to one with a different refresh rate.
    pub fn new(len: u64) -> ProgressBar {
        ProgressBar::with_draw_target(len, ProgressDrawTarget::stderr())
    }

    /// Creates a completely hidden progress bar.
    ///
    /// This progress bar still responds to API changes but it does not
    /// have a length or render in any way.
    pub fn hidden() -> ProgressBar {
        ProgressBar::with_draw_target(!0, ProgressDrawTarget::hidden())
    }

    /// Creates a new progress bar with a given length and draw target.
    pub fn with_draw_target(len: u64, target: ProgressDrawTarget) -> ProgressBar {
        ProgressBar {
            state: Arc::new(RwLock::new(ProgressState {
                style: ProgressStyle::default_bar(),
                draw_target: target,
                width: None,
                message: "".into(),
                prefix: "".into(),
                pos: 0,
                len,
                tick: 0,
                draw_delta: 0,
                draw_next: 0,
                status: Status::InProgress,
                started: Instant::now(),
                est: Estimate::new(),
                tick_thread: None,
                steady_tick: 0,
            })),
        }
    }

    /// A convenience builder-like function for a progress bar with a given style.
    pub fn with_style(self, style: ProgressStyle) -> ProgressBar {
        self.state.write().unwrap().style = style;
        self
    }

    /// Creates a new spinner.
    ///
    /// This spinner by default draws directly to stderr.  This adds the
    /// default spinner style to it.
    pub fn new_spinner() -> ProgressBar {
        let rv = ProgressBar::new(!0);
        rv.set_style(ProgressStyle::default_spinner());
        rv
    }

    /// Overrides the stored style.
    ///
    /// This does not redraw the bar.  Call `tick` to force it.
    pub fn set_style(&self, style: ProgressStyle) {
        self.state.write().unwrap().style = style;
    }

    /// Spawns a background thread to tick the progress bar.
    ///
    /// When this is enabled a background thread will regularly tick the
    /// progress back in the given interval (milliseconds).  This is
    /// useful to advance progress bars that are very slow by themselves.
    ///
    /// When steady ticks are enabled calling `.tick()` on a progress
    /// bar does not do anything.
    pub fn enable_steady_tick(&self, ms: u64) {
        let mut state = self.state.write().unwrap();
        state.steady_tick = ms;
        if state.tick_thread.is_some() {
            return;
        }

        let state_arc = self.state.clone();
        state.tick_thread = Some(thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(ms));
            {
                let mut state = state_arc.write().unwrap();
                if state.is_finished() || state.steady_tick == 0 {
                    state.steady_tick = 0;
                    state.tick_thread = None;
                    break;
                }
                if state.tick != 0 {
                    state.tick = state.tick.saturating_add(1);
                }
            }

            draw_state(&state_arc).ok();
        }));

        // use the sideeffect of tick to force the bar to tick.
        ::std::mem::drop(state);
        self.tick();
    }

    /// Undoes `enable_steady_tick`.
    pub fn disable_steady_tick(&self) {
        self.enable_steady_tick(0);
    }

    /// Limit redrawing of progress bar to every `n` steps. Defaults to 0.
    ///
    /// By default, the progress bar will redraw whenever its state advances.
    /// This setting is helpful in situations where the overhead of redrawing
    /// the progress bar dominates the computation whose progress is being
    /// reported.
    ///
    /// If `n` is greater than 0, operations that change the progress bar such
    /// as `.tick()`, `.set_message()` and `.set_length()` will no longer cause
    /// the progress bar to be redrawn, and will only be shown once the
    /// position advances by `n` steps.
    ///
    /// ```rust,no_run
    /// # use indicatif::ProgressBar;
    /// let n = 1_000_000;
    /// let pb = ProgressBar::new(n);
    /// pb.set_draw_delta(n / 100); // redraw every 1% of additional progress
    /// ```
    pub fn set_draw_delta(&self, n: u64) {
        let mut state = self.state.write().unwrap();
        state.draw_delta = n;
        state.draw_next = state.pos.saturating_add(state.draw_delta);
    }

    /// Manually ticks the spinner or progress bar.
    ///
    /// This automatically happens on any other change to a progress bar.
    pub fn tick(&self) {
        self.update_and_draw(|state| {
            if state.steady_tick == 0 || state.tick == 0 {
                state.tick = state.tick.saturating_add(1);
            }
        });
    }

    /// Advances the position of a progress bar by delta.
    pub fn inc(&self, delta: u64) {
        self.update_and_draw(|state| {
            state.pos = state.pos.saturating_add(delta);
            if state.steady_tick == 0 || state.tick == 0 {
                state.tick = state.tick.saturating_add(1);
            }
        })
    }

    /// A quick convenience check if the progress bar is hidden.
    pub fn is_hidden(&self) -> bool {
        self.state.read().unwrap().draw_target.is_hidden()
    }

    /// Print a log line above the progress bar.
    ///
    /// If the progress bar was added to a `MultiProgress`, the log line will be
    /// printed above all other progress bars.
    ///
    /// Note that if the progress bar is hidden (which by default happens if
    /// the progress bar is redirected into a file) println will not do
    /// anything either.
    pub fn println<I: Into<String>>(&self, msg: I) {
        let mut state = self.state.write().unwrap();

        let mut lines: Vec<String> = msg.into().lines().map(Into::into).collect();
        let orphan_lines = lines.len();
        if state.should_render() {
            lines.extend(state.style.format_state(&*state));
        }

        let draw_state = ProgressDrawState {
            lines,
            orphan_lines,
            finished: state.is_finished(),
            force_draw: true,
            move_cursor: false,
            ts: Instant::now(),
        };

        state.draw_target.apply_draw_state(draw_state).ok();
    }

    /// Sets the position of the progress bar.
    pub fn set_position(&self, pos: u64) {
        self.update_and_draw(|state| {
            state.draw_next = pos;
            state.pos = pos;
            if state.steady_tick == 0 || state.tick == 0 {
                state.tick = state.tick.saturating_add(1);
            }
        })
    }

    /// Sets the length of the progress bar.
    pub fn set_length(&self, len: u64) {
        self.update_and_draw(|state| {
            state.len = len;
        })
    }

    /// Sets the current prefix of the progress bar.
    pub fn set_prefix(&self, prefix: &str) {
        let prefix = prefix.to_string();
        self.update_and_draw(|state| {
            state.prefix = prefix;
            if state.steady_tick == 0 || state.tick == 0 {
                state.tick = state.tick.saturating_add(1);
            }
        })
    }

    /// Sets the current message of the progress bar.
    pub fn set_message(&self, msg: &str) {
        let msg = msg.to_string();
        self.update_and_draw(|state| {
            state.message = msg;
            if state.steady_tick == 0 || state.tick == 0 {
                state.tick = state.tick.saturating_add(1);
            }
        })
    }

    /// Resets the ETA calculation.
    ///
    /// This can be useful if progress bars make a huge jump or were
    /// paused for a prolonged time.
    pub fn reset_eta(&self) {
        self.update_and_draw(|state| {
            state.est.reset();
        });
    }

    /// Resets elapsed time
    pub fn reset_elapsed(&self) {
        self.update_and_draw(|state| {
            state.started = Instant::now();
        });
    }

    pub fn reset(&self) {
        self.reset_eta();
        self.reset_elapsed();
        self.update_and_draw(|state| {
            state.draw_next = 0;
            state.pos = 0;
            state.status = Status::InProgress;
        });

    }

    /// Finishes the progress bar and leaves the current message.
    pub fn finish(&self) {
        self.update_and_draw(|state| {
            state.pos = state.len;
            state.draw_next = state.pos;
            state.status = Status::DoneVisible;
        });
    }

    /// Finishes the progress bar at current position and leaves the current message.
    pub fn finish_at_current_pos(&self) {
        self.update_and_draw(|state| {
            state.draw_next = state.pos;
            state.status = Status::DoneVisible;
        });
    }

    /// Finishes the progress bar and sets a message.
    pub fn finish_with_message(&self, msg: &str) {
        let msg = msg.to_string();
        self.update_and_draw(|state| {
            state.message = msg;
            state.pos = state.len;
            state.draw_next = state.pos;
            state.status = Status::DoneVisible;
        });
    }

    /// Finishes the progress bar and completely clears it.
    pub fn finish_and_clear(&self) {
        self.update_and_draw(|state| {
            state.pos = state.len;
            state.draw_next = state.pos;
            state.status = Status::DoneHidden;
        });
    }

    /// Sets a different draw target for the progress bar.
    ///
    /// This can be used to draw the progress bar to stderr
    /// for instance:
    ///
    /// ```rust,no_run
    /// # use indicatif::{ProgressBar, ProgressDrawTarget};
    /// let pb = ProgressBar::new(100);
    /// pb.set_draw_target(ProgressDrawTarget::stderr());
    /// ```
    pub fn set_draw_target(&self, target: ProgressDrawTarget) {
        let mut state = self.state.write().unwrap();
        state.draw_target.disconnect();
        state.draw_target = target;
    }

    /// Wraps an iterator with the progress bar.
    ///
    /// ```rust,norun
    /// # use indicatif::ProgressBar;
    /// let v = vec![1, 2, 3];
    /// let pb = ProgressBar::new(3);
    /// for item in pb.wrap_iter(v.iter()) {
    ///     // ...
    /// }
    /// ```
    pub fn wrap_iter<It: Iterator>(&self, it: It) -> ProgressBarIter<It> {
        ProgressBarIter {
            bar: self.clone(),
            it,
        }
    }

    /// Wraps a Reader with the progress bar.
    ///
    /// ```rust,norun
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
    pub fn wrap_read<R: io::Read>(&self, read: R) -> ProgressBarRead<R> {
        ProgressBarRead {
            bar: self.clone(),
            read,
        }
    }

    fn update_and_draw<F: FnOnce(&mut ProgressState)>(&self, f: F) {
        let mut draw = false;
        {
            let mut state = self.state.write().unwrap();
            let old_pos = state.pos;
            f(&mut state);
            let new_pos = state.pos;
            if new_pos != old_pos {
                state.est.record_step(new_pos);
            }
            if new_pos >= state.draw_next {
                state.draw_next = new_pos.saturating_add(state.draw_delta);
                draw = true;
            }
        }
        if draw {
            self.draw().ok();
        }
    }

    fn draw(&self) -> io::Result<()> {
        draw_state(&self.state)
    }

    pub fn position(&self) -> u64 {
        self.state.read().pos
    }
}

fn draw_state(state: &Arc<RwLock<ProgressState>>) -> io::Result<()> {
    let mut state = state.write().unwrap();

    // we can bail early if the draw target is hidden.
    if state.draw_target.is_hidden() {
        return Ok(());
    }

    let draw_state = ProgressDrawState {
        lines: if state.should_render() {
            state.style.format_state(&*state)
        } else {
            vec![]
        },
        orphan_lines: 0,
        finished: state.is_finished(),
        force_draw: false,
        move_cursor: false,
        ts: Instant::now(),
    };
    state.draw_target.apply_draw_state(draw_state)
}

#[test]
fn test_pbar_zero() {
    let pb = ProgressBar::new(0);
    assert_eq!(pb.state.read().unwrap().fraction(), 1.0);
}

#[test]
fn test_pbar_maxu64() {
    let pb = ProgressBar::new(!0);
    assert_eq!(pb.state.read().unwrap().fraction(), 0.0);
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

struct MultiObject {
    done: bool,
    draw_state: Option<ProgressDrawState>,
}

struct MultiProgressState {
    objects: Vec<MultiObject>,
    draw_target: ProgressDrawTarget,
    move_cursor: bool,
}

/// Manages multiple progress bars from different threads.
pub struct MultiProgress {
    state: RwLock<MultiProgressState>,
    joining: AtomicBool,
    tx: Sender<(usize, ProgressDrawState)>,
    rx: Receiver<(usize, ProgressDrawState)>,
}

impl fmt::Debug for MultiProgress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MultiProgress").finish()
    }
}

unsafe impl Sync for MultiProgress {}

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
        let (tx, rx) = channel();
        MultiProgress {
            state: RwLock::new(MultiProgressState {
                objects: vec![],
                draw_target,
                move_cursor: false,
            }),
            joining: AtomicBool::new(false),
            tx,
            rx,
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

    /// Adds a progress bar.
    ///
    /// The progress bar added will have the draw target changed to a
    /// remote draw target that is intercepted by the multi progress
    /// object overriding custom `ProgressDrawTarget` settings.
    pub fn add(&self, pb: ProgressBar) -> ProgressBar {
        let mut state = self.state.write().unwrap();
        let idx = state.objects.len();
        state.objects.push(MultiObject {
            done: false,
            draw_state: None,
        });
        pb.set_draw_target(ProgressDrawTarget {
            kind: ProgressDrawTargetKind::Remote(idx, Mutex::new(self.tx.clone())),
        });
        pb
    }

    /// Waits for all progress bars to report that they are finished.
    ///
    /// You need to call this as this will request the draw instructions
    /// from the remote progress bars.  Not calling this will deadlock
    /// your program.
    pub fn join(&self) -> io::Result<()> {
        self.join_impl(false)
    }

    /// Works like `join` but clears the progress bar in the end.
    pub fn join_and_clear(&self) -> io::Result<()> {
        self.join_impl(true)
    }

    fn is_done(&self) -> bool {
        let state = self.state.read().unwrap();
        if state.objects.is_empty() {
            return true;
        }
        for obj in &state.objects {
            if !obj.done {
                return false;
            }
        }
        true
    }

    fn join_impl(&self, clear: bool) -> io::Result<()> {
        if self.joining.load(Ordering::Acquire) {
            panic!("Already joining!");
        }
        self.joining.store(true, Ordering::Release);

        let move_cursor = self.state.read().unwrap().move_cursor;
        while !self.is_done() {
            let (idx, draw_state) = self.rx.recv().unwrap();
            let ts = draw_state.ts;
            let force_draw = draw_state.finished || draw_state.force_draw;

            let mut state = self.state.write().unwrap();
            if draw_state.finished {
                state.objects[idx].done = true;
            }

            // Split orphan lines out of the draw state, if any
            let (orphan_lines, lines) = if draw_state.orphan_lines == 0 {
                (vec![], draw_state.lines)
            } else {
                let split = draw_state.lines.split_at(draw_state.orphan_lines);
                (split.0.to_vec(), split.1.to_vec())
            };

            let draw_state = ProgressDrawState {
                lines,
                orphan_lines: 0,
                ..draw_state
            };

            state.objects[idx].draw_state = Some(draw_state);

            // the rest from here is only drawing, we can skip it.
            if state.draw_target.is_hidden() {
                continue;
            }

            let mut lines = vec![];

            // Make orphaned lines appear at the top, so they can be properly
            // forgotten.
            let orphan_lines_count = orphan_lines.len();
            lines.extend(orphan_lines);

            for obj in state.objects.iter() {
                if let Some(ref draw_state) = obj.draw_state {
                    lines.extend_from_slice(&draw_state.lines[..]);
                }
            }

            // !any(!done) is also true when iter() is empty, contrary to all(done)
            let finished = !state.objects.iter().any(|ref x| !x.done);
            state.draw_target.apply_draw_state(ProgressDrawState {
                lines,
                orphan_lines: orphan_lines_count,
                force_draw,
                move_cursor,
                finished,
                ts,
            })?;
        }

        if clear {
            let mut state = self.state.write().unwrap();
            state.draw_target.apply_draw_state(ProgressDrawState {
                lines: vec![],
                orphan_lines: 0,
                finished: true,
                force_draw: true,
                move_cursor,
                ts: Instant::now(),
            })?;
        }

        self.joining.store(false, Ordering::Release);

        Ok(())
    }
}

impl Drop for ProgressBar {
    fn drop(&mut self) {
        if self.state.read().unwrap().is_finished() {
            return;
        }
        self.update_and_draw(|state| {
            state.status = Status::DoneHidden;
        });
    }
}

/// Iterator for `wrap_iter`.
#[derive(Debug)]
pub struct ProgressBarIter<I> {
    bar: ProgressBar,
    it: I,
}

impl<I: Iterator> Iterator for ProgressBarIter<I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.it.next();

        if item.is_some() {
            self.bar.inc(1);
        }

        item
    }
}

/// Reader for `wrap_read`.
#[derive(Debug)]
pub struct ProgressBarRead<R> {
    bar: ProgressBar,
    read: R,
}

impl<R: io::Read> io::Read for ProgressBarRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let inc = self.read.read(buf)?;
        self.bar.inc(inc as u64);
        Ok(inc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn progress_bar_sync_send() {
        let _: Box<dyn Sync> = Box::new(ProgressBar::new(1));
        let _: Box<dyn Send> = Box::new(ProgressBar::new(1));
        let _: Box<dyn Sync> = Box::new(MultiProgress::new());
        let _: Box<dyn Send> = Box::new(MultiProgress::new());
    }
}
