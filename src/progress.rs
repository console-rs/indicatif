use std::borrow::Cow;
use std::cell::RefCell;
use std::fmt;
use std::io;
use std::iter::repeat;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};

use console::{measure_text_width, Style, Term};
use format::{BinaryBytes, DecimalBytes, FormattedDuration, HumanBytes, HumanDuration};
use utils::{duration_to_secs, expand_template, pad_str, secs_to_duration, Estimate};

/// Controls the rendering style of progress bars.
#[derive(Clone, Debug)]
pub struct ProgressStyle {
    tick_chars: Vec<char>,
    progress_chars: Vec<char>,
    template: Cow<'static, str>,
}

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
                chan.lock().send((idx, draw_state)).unwrap();
            }
            ProgressDrawTargetKind::Hidden => {}
        }
        Ok(())
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

impl ProgressStyle {
    /// Returns the default progress bar style for bars.
    pub fn default_bar() -> ProgressStyle {
        ProgressStyle {
            tick_chars: "⠁⠁⠉⠙⠚⠒⠂⠂⠒⠲⠴⠤⠄⠄⠤⠠⠠⠤⠦⠖⠒⠐⠐⠒⠓⠋⠉⠈⠈ ".chars().collect(),
            progress_chars: "█░".chars().collect(),
            template: Cow::Borrowed("{wide_bar} {pos}/{len}"),
        }
    }

    /// Returns the default progress bar style for spinners.
    pub fn default_spinner() -> ProgressStyle {
        ProgressStyle {
            tick_chars: "⠁⠁⠉⠙⠚⠒⠂⠂⠒⠲⠴⠤⠄⠄⠤⠠⠠⠤⠦⠖⠒⠐⠐⠒⠓⠋⠉⠈⠈ ".chars().collect(),
            progress_chars: "█░".chars().collect(),
            template: Cow::Borrowed("{spinner} {msg}"),
        }
    }

    /// Sets the tick character sequence for spinners.
    pub fn tick_chars(mut self, s: &str) -> ProgressStyle {
        self.tick_chars = s.chars().collect();
        self
    }

    /// Sets the three progress characters `(filled, current, to do)`.
    pub fn progress_chars(mut self, s: &str) -> ProgressStyle {
        self.progress_chars = s.chars().collect();
        self
    }

    /// Sets the template string for the progress bar.
    pub fn template(mut self, s: &str) -> ProgressStyle {
        self.template = Cow::Owned(s.into());
        self
    }
}

impl ProgressStyle {
    /// Returns the tick char for a given number.
    pub fn get_tick_char(&self, idx: u64) -> char {
        self.tick_chars[(idx as usize) % (self.tick_chars.len() - 1)]
    }

    /// Returns the tick char for the finished state.
    pub fn get_final_tick_char(&self) -> char {
        self.tick_chars[self.tick_chars.len() - 1]
    }

    fn format_bar(&self, state: &ProgressState, width: usize, alt_style: Option<&Style>) -> String {
        let pct = state.fraction();
        let fill = pct * width as f32;
        let head = if pct > 0.0 && (fill as usize) < width {
            1
        } else {
            0
        };

        let bar = repeat(state.style.progress_chars[0])
            .take(fill as usize)
            .collect::<String>();
        let cur = if head == 1 {
            let n = state.style.progress_chars.len().saturating_sub(2);
            let cur_char = if n == 0 {
                1
            } else {
                n.saturating_sub((fill * n as f32) as usize % n)
            };
            state.style.progress_chars[cur_char].to_string()
        } else {
            "".into()
        };
        let bg = width.saturating_sub(fill as usize).saturating_sub(head);
        let rest = repeat(state.style.progress_chars.last().unwrap())
            .take(bg)
            .collect::<String>();
        format!(
            "{}{}{}",
            bar,
            cur,
            alt_style.unwrap_or(&Style::new()).apply_to(rest)
        )
    }

    fn format_state(&self, state: &ProgressState) -> Vec<String> {
        let (pos, len) = state.position();
        let mut rv = vec![];

        for line in self.template.lines() {
            let wide_element = RefCell::new(None);

            let s = expand_template(line, |var| {
                let key = var.key;

                match key {
                    "wide_bar" => {
                        *wide_element.borrow_mut() = Some(var.duplicate_for_key("bar"));
                        "\x00".into()
                    }
                    "bar" => {
                        self.format_bar(state, var.width.unwrap_or(20), var.alt_style.as_ref())
                    }
                    "spinner" => state.current_tick_char().to_string(),
                    "wide_msg" => {
                        *wide_element.borrow_mut() = Some(var.duplicate_for_key("msg"));
                        "\x00".into()
                    }
                    "msg" => state.message().to_string(),
                    "prefix" => state.prefix().to_string(),
                    "pos" => pos.to_string(),
                    "len" => len.to_string(),
                    "percent" => format!("{:.*}", 0, state.fraction() * 100f32),
                    "bytes" => format!("{}", HumanBytes(state.pos)),
                    "total_bytes" => format!("{}", HumanBytes(state.len)),
                    "decimal_bytes" => format!("{}", DecimalBytes(state.pos)),
                    "decimal_total_bytes" => format!("{}", DecimalBytes(state.len)),
                    "binary_bytes" => format!("{}", BinaryBytes(state.pos)),
                    "binary_total_bytes" => format!("{}", BinaryBytes(state.len)),
                    "elapsed_precise" => format!("{}", FormattedDuration(state.started.elapsed())),
                    "elapsed" => format!("{:#}", HumanDuration(state.started.elapsed())),
                    "eta_precise" => format!("{}", FormattedDuration(state.eta())),
                    "eta" => format!("{:#}", HumanDuration(state.eta())),
                    _ => "".into(),
                }
            });

            rv.push(if let Some(ref var) = *wide_element.borrow() {
                let total_width = state.width();
                if var.key == "bar" {
                    let bar_width = total_width.saturating_sub(measure_text_width(&s));
                    s.replace(
                        "\x00",
                        &self.format_bar(state, bar_width, var.alt_style.as_ref()),
                    )
                } else if var.key == "msg" {
                    let msg_width = total_width.saturating_sub(measure_text_width(&s));
                    let msg = pad_str(state.message(), msg_width, var.align, true);
                    s.replace(
                        "\x00",
                        if var.last_element {
                            msg.trim_right()
                        } else {
                            &msg
                        },
                    )
                } else {
                    unreachable!()
                }
            } else {
                s.to_string()
            });
        }

        rv
    }
}

/// The state of a progress bar at a moment in time.
struct ProgressState {
    style: ProgressStyle,
    draw_target: ProgressDrawTarget,
    width: Option<u16>,
    message: String,
    prefix: String,
    pos: u64,
    len: u64,
    tick: u64,
    draw_delta: u64,
    draw_next: u64,
    status: Status,
    started: Instant,
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
            Term::stdout().size().1 as usize
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
}

/// A progress bar or spinner.
pub struct ProgressBar {
    state: Arc<RwLock<ProgressState>>,
}

impl fmt::Debug for ProgressBar {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("ProgressBar").finish()
    }
}

impl ProgressBar {
    /// Creates a new progress bar with a given length.
    ///
    /// This progress bar by default draws directly to stderr, and refreshes
    /// a maximum of 15 times a second
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
                len: len,
                tick: 0,
                draw_delta: 1,
                draw_next: 1,
                status: Status::InProgress,
                started: Instant::now(),
                est: Estimate::new(),
                tick_thread: None,
                steady_tick: 0,
            })),
        }
    }

    /// Creates a new spinner.
    ///
    /// This spinner by default draws directly to stdout.  This adds the
    /// default spinner style to it.
    pub fn new_spinner() -> ProgressBar {
        let rv = ProgressBar::new(!0);
        rv.set_style(ProgressStyle::default_spinner());
        rv
    }

    /// Overrides the stored style.
    pub fn set_style(&self, style: ProgressStyle) {
        self.state.write().style = style;
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
        let mut state = self.state.write();
        state.steady_tick = ms;
        if state.tick_thread.is_some() {
            return;
        }

        let state_arc = self.state.clone();
        state.tick_thread = Some(thread::spawn(move || loop {
            thread::sleep(Duration::from_millis(ms));
            {
                let mut state = state_arc.write();
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
    }

    /// Undoes `enable_steady_tick`.
    pub fn disable_steady_tick(&self) {
        self.enable_steady_tick(0);
    }

    /// Limit redrawing of progress bar to every `n` steps.
    ///
    /// By default, the progress bar will redraw whenever its state advances.
    /// This setting is helpful in situations where the overhead of redrawing
    /// the progress bar dominates the computation whose progress is being
    /// reported.
    ///
    /// ```rust,no_run
    /// # use indicatif::ProgressBar;
    /// let n = 1_000_000;
    /// let pb = ProgressBar::new(n);
    /// pb.set_draw_delta(n / 100); // redraw every 1% of additional progress
    /// ```
    pub fn set_draw_delta(&self, n: u64) {
        let mut state = self.state.write();
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

    /// Print a log line above the progress bar.
    pub fn println<I: Into<String>>(&self, msg: I) {
        let mut state = self.state.write();

        let mut lines: Vec<String> = msg.into().lines().map(Into::into).collect();
        let orphan_lines = lines.len();
        lines.extend(state.style.format_state(&*state));

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

    /// Finishes the progress bar and leaves the current message.
    pub fn finish(&self) {
        self.update_and_draw(|state| {
            state.pos = state.len;
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
        self.state.write().draw_target = target;
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
        ProgressBarIter { bar: self, it: it }
    }

    fn update_and_draw<F: FnOnce(&mut ProgressState)>(&self, f: F) {
        let mut draw = false;
        {
            let mut state = self.state.write();
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
}

fn draw_state(state: &Arc<RwLock<ProgressState>>) -> io::Result<()> {
    let mut state = state.write();

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
    assert_eq!(pb.state.read().fraction(), 1.0);
}

#[test]
fn test_pbar_maxu64() {
    let pb = ProgressBar::new(!0);
    assert_eq!(pb.state.read().fraction(), 0.0);
}

#[test]
fn test_pbar_overflow() {
    let pb = ProgressBar::new(1);
    pb.set_draw_target(ProgressDrawTarget::hidden());
    pb.inc(2);
    pb.finish();
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("MultiProgress").finish()
    }
}

unsafe impl Sync for MultiProgress {}

impl MultiProgress {
    /// Creates a new multi progress object that draws to stderr.
    pub fn new() -> MultiProgress {
        let (tx, rx) = channel();
        MultiProgress {
            state: RwLock::new(MultiProgressState {
                objects: vec![],
                draw_target: ProgressDrawTarget::stderr(),
                move_cursor: false,
            }),
            joining: AtomicBool::new(false),
            tx: tx,
            rx: rx,
        }
    }

    /// Sets a different draw target for the multiprogress bar.
    pub fn set_draw_target(&self, target: ProgressDrawTarget) {
        self.state.write().draw_target = target;
    }

    /// Set whether we should try to move the cursor when possible instead of clearing lines.
    ///
    /// This can reduce flickering, but do not enable it if you intend to change the number of
    /// progress bars.
    pub fn set_move_cursor(&self, move_cursor: bool) {
        self.state.write().move_cursor = move_cursor;
    }

    /// Adds a progress bar.
    ///
    /// The progress bar added will have the draw target changed to a
    /// remote draw target that is intercepted by the multi progress
    /// object.
    pub fn add(&self, bar: ProgressBar) -> ProgressBar {
        let mut state = self.state.write();
        let idx = state.objects.len();
        state.objects.push(MultiObject {
            done: false,
            draw_state: None,
        });
        bar.set_draw_target(ProgressDrawTarget {
            kind: ProgressDrawTargetKind::Remote(idx, Mutex::new(self.tx.clone())),
        });
        bar
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
        let state = self.state.read();
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

        let move_cursor = self.state.read().move_cursor;
        while !self.is_done() {
            let (idx, draw_state) = self.rx.recv().unwrap();
            let ts = draw_state.ts;
            let force_draw = draw_state.finished || draw_state.force_draw;

            let mut state = self.state.write();
            if draw_state.finished {
                state.objects[idx].done = true;
            }
            state.objects[idx].draw_state = Some(draw_state);

            // the rest from here is only drawing, we can skip it.
            if state.draw_target.is_hidden() {
                continue;
            }

            let mut lines = vec![];
            for obj in state.objects.iter() {
                if let Some(ref draw_state) = obj.draw_state {
                    lines.extend_from_slice(&draw_state.lines[..]);
                }
            }

            let finished = !state.objects.iter().any(|ref x| x.done);
            state.draw_target.apply_draw_state(ProgressDrawState {
                lines,
                orphan_lines: 0,
                force_draw,
                move_cursor,
                finished,
                ts,
            })?;
        }

        if clear {
            let mut state = self.state.write();
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
        if self.state.read().is_finished() {
            return;
        }
        self.update_and_draw(|state| {
            state.status = Status::DoneHidden;
        });
    }
}

/// Iterator for `wrap_iter`.
#[derive(Debug)]
pub struct ProgressBarIter<'a, I> {
    bar: &'a ProgressBar,
    it: I,
}

impl<'a, I: Iterator> Iterator for ProgressBarIter<'a, I> {
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.it.next();

        if item.is_some() {
            self.bar.inc(1);
        }

        item
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_can_wrap_an_iterator() {
        let v = vec![1, 2, 3];
        let pb = ProgressBar::new(v.len() as u64);
        let w: Vec<_> = pb.wrap_iter(v.iter()).map(|x| x * 2).collect();
        assert_eq!(w, vec![2, 4, 6]);
    }

    #[test]
    fn progress_bar_sync_send() {
        let _: Box<Sync> = Box::new(ProgressBar::new(1));
        let _: Box<Send> = Box::new(ProgressBar::new(1));
        let _: Box<Sync> = Box::new(MultiProgress::new());
        let _: Box<Send> = Box::new(MultiProgress::new());
    }
}
