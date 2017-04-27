use std::io;
use std::iter::repeat;
use std::borrow::Cow;
use std::cell::RefCell;
use std::time::{Duration, Instant};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::RwLock;

use term::Term;
use utils::{expand_template, Estimate, duration_to_secs, secs_to_duration};
use format::{FormattedDuration, HumanDuration, HumanBytes};
use ansistyle::{style, measure_text_width};

/// Controls the rendering style of progress bars.
#[derive(Clone)]
pub struct ProgressStyle {
    tick_chars: Vec<char>,
    progress_chars: Vec<char>,
    template: Cow<'static, str>,
}

/// The drawn state of an element.
#[derive(Clone)]
pub struct DrawState {
    /// The lines to print (can contain ANSI codes)
    pub lines: Vec<String>,
    /// True if the bar no longer needs drawing.
    pub finished: bool,
    /// True if drawing should be forced.
    pub force_draw: bool,
    /// Time when the draw state was created.
    pub ts: Instant,
}

enum Status {
    InProgress,
    DoneVisible,
    DoneHidden,
}

/// Target for draw operations
pub enum DrawTarget {
    /// Draws into a terminal
    Term(Term, Option<DrawState>, Option<Duration>),
    /// Draws to a remote receiver
    Remote(usize, Sender<(usize, DrawState)>),
    /// Do not draw at all
    Hidden,
}

impl DrawTarget {
    /// Draw to a terminal, optionally with a refresh rate.
    pub fn to_term(term: Term, refresh_rate: Option<u64>) -> DrawTarget {
        let rate = refresh_rate.map(|x| Duration::from_millis(1000 / x));
        DrawTarget::Term(term, None, rate)
    }

    /// Draw to a buffered stdout terminal at a max of 15 times a second.
    pub fn stdout() -> DrawTarget {
        DrawTarget::to_term(Term::buffered_stdout(), Some(15))
    }

    /// Draw to a buffered stderr terminal at a max of 15 times a second.
    pub fn stderr() -> DrawTarget {
        DrawTarget::to_term(Term::buffered_stderr(), Some(15))
    }

    /// Apply the given draw state (draws it).
    pub fn apply_draw_state(&mut self, draw_state: DrawState) -> io::Result<()> {
        match *self {
            DrawTarget::Term(ref term, ref mut last_state, rate) => {
                let last_draw = last_state.as_ref().map(|x| x.ts);
                if draw_state.finished ||
                   draw_state.force_draw ||
                   rate.is_none() ||
                   last_draw.is_none() ||
                   last_draw.unwrap().elapsed() > rate.unwrap() {
                    if let Some(ref last_state) = *last_state {
                        last_state.clear_term(term)?;
                    }
                    draw_state.draw_to_term(term)?;
                    term.flush()?;
                    *last_state = Some(draw_state);
                }
            }
            DrawTarget::Remote(idx, ref chan) => {
                chan.send((idx, draw_state)).unwrap();
            }
            DrawTarget::Hidden => {}
        }
        Ok(())
    }
}

impl DrawState {
    pub fn clear_term(&self, term: &Term) -> io::Result<()> {
        term.clear_last_lines(self.lines.len())
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
            progress_chars: "██░".chars().collect(),
            template: Cow::Borrowed("{wide_bar} {pos}/{len}"),
        }
    }

    /// Returns the default progress bar style for spinners.
    pub fn default_spinner() -> ProgressStyle {
        ProgressStyle {
            tick_chars: "⠁⠁⠉⠙⠚⠒⠂⠂⠒⠲⠴⠤⠄⠄⠤⠠⠠⠤⠦⠖⠒⠐⠐⠒⠓⠋⠉⠈⠈ ".chars().collect(),
            progress_chars: "██░".chars().collect(),
            template: Cow::Borrowed("{spinner} {msg}"),
        }
    }

    /// Creates a hidden style.
    pub fn hidden() -> ProgressStyle {
        ProgressStyle {
            tick_chars: vec![' ', ' '],
            progress_chars: vec![' ', ' ', ' '],
            template: Cow::Borrowed(""),
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

    fn format_bar(&self, state: &ProgressState, width: usize,
                      alt_style: &str) -> String {
        let pct = state.percent();
        let mut fill = (pct * width as f32) as usize;
        let mut head = 0;
        if fill > 0 && !state.is_finished() {
            fill -= 1;
            head = 1;
        }

        let bar = repeat(state.style.progress_chars[0])
            .take(fill).collect::<String>();
        let cur = if head == 1 {
            state.style.progress_chars[1].to_string()
        } else {
            "".into()
        };
        let rest = repeat(state.style.progress_chars[2])
            .take(width - fill - head).collect::<String>();
        format!("{}{}{}", bar, cur, style(rest).from_dotted_str(alt_style))
    }

    pub fn format_state(&self, state: &ProgressState) -> Vec<String> {
        let (pos, len) = state.position();
        let mut rv = vec![];

        for line in self.template.lines() {
            let need_wide_bar = RefCell::new(None);

            let s = expand_template(line, |var| {
                let key = var.key;
                if key == "wide_bar" {
                    *need_wide_bar.borrow_mut() = Some(
                        var.alt_style.unwrap_or("").to_string());
                    "\x00".into()
                } else if key == "bar" {
                    self.format_bar(state, var.width.unwrap_or(20),
                                    var.alt_style.unwrap_or(""))
                } else if key == "spinner" {
                    state.current_tick_char().to_string()
                } else if key == "msg" {
                    state.message().to_string()
                } else if key == "pos" {
                    pos.to_string()
                } else if key == "len" {
                    len.to_string()
                } else if key == "bytes" {
                    format!("{}", HumanBytes(state.pos))
                } else if key == "total_bytes" {
                    format!("{}", HumanBytes(state.len))
                } else if key == "elapsed_precise" {
                    format!("{}", FormattedDuration(state.started.elapsed()))
                } else if key == "elapsed" {
                    format!("{:#}", HumanDuration(state.started.elapsed()))
                } else if key == "eta_precise" {
                    format!("{}", FormattedDuration(state.eta()))
                } else if key == "eta" {
                    format!("{:#}", HumanDuration(state.eta()))
                } else {
                    "".into()
                }
            });

            rv.push(if let Some(ref style) = *need_wide_bar.borrow() {
                let total_width = state.width();
                let bar_width = total_width - measure_text_width(&s);
                s.replace("\x00", &self.format_bar(state, bar_width, &style))
            } else {
                s.to_string()
            });
        }

        rv
    }
}

/// The state of a progress bar at a moment in time.
pub struct ProgressState {
    style: ProgressStyle,
    draw_target: DrawTarget,
    width: Option<u16>,
    message: String,
    pos: u64,
    len: u64,
    tick: u64,
    status: Status,
    started: Instant,
    est: Estimate,
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

    /// Indicates that a spinner should be drawn.
    pub fn has_spinner(&self) -> bool {
        self.tick != !0
    }

    /// Indicates that a progress bar should be drawn.
    pub fn has_progress(&self) -> bool {
        self.len != !0
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

    /// Returns the completion in percent
    pub fn percent(&self) -> f32 {
        if self.len == !0 {
            0.0
        } else {
            self.pos as f32 / self.len as f32
        }
    }

    /// Returns the position of the status bar as `(pos, len)` tuple.
    pub fn position(&self) -> (u64, u64) {
        (self.pos, self.len)
    }

    /// Returns the current message of the progress bar.
    pub fn message(&self) -> &str {
        &self.message
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
        secs_to_duration(t * (self.len - self.pos) as f64 + 0.75)
    }
}

/// A progress bar or spinner.
pub struct ProgressBar {
    state: RwLock<ProgressState>,
}

unsafe impl Sync for ProgressBar {}

impl ProgressBar {
    /// Creates a new progress bar with a given length.
    ///
    /// This progress bar by default draws directly to stdout.
    pub fn new(len: u64) -> ProgressBar {
        ProgressBar {
            state: RwLock::new(ProgressState {
                style: ProgressStyle::default_bar(),
                draw_target: DrawTarget::stdout(),
                width: None,
                message: "".into(),
                pos: 0,
                len: len,
                tick: 0,
                status: Status::InProgress,
                started: Instant::now(),
                est: Estimate::new(),
            }),
        }
    }

    /// Creates a completely hidden progress bar.
    ///
    /// This progress bar still responds to API changes but it does not
    /// have a length or render in any way.
    pub fn hidden() -> ProgressBar {
        let rv = ProgressBar::new(!0);
        rv.set_style(ProgressStyle::hidden());
        rv.set_draw_target(DrawTarget::Hidden);
        rv
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

    /// Manually ticks the spinner or progress bar.
    ///
    /// This automatically happens on any other change to a progress bar.
    pub fn tick(&self) {
        self.update_and_draw(|mut state| {
            state.tick += 1;
        });
    }

    /// Advances the position of a progress bar by delta.
    pub fn inc(&self, delta: u64) {
        self.update_and_draw(|mut state| {
            state.pos += delta;
            state.tick += 1;
        })
    }

    /// Sets the position of the progress bar.
    pub fn set_position(&self, pos: u64) {
        self.update_and_draw(|mut state| {
            state.pos = pos;
            state.tick += 1;
        })
    }

    /// Sets the length of the progress bar.
    pub fn set_length(&self, len: u64) {
        self.update_and_draw(|mut state| {
            state.len = len;
        })
    }

    /// Sets the current message of the progress bar.
    pub fn set_message(&self, msg: &str) {
        let msg = msg.to_string();
        self.update_and_draw(|mut state| {
            state.message = msg;
            state.tick += 1;
        })
    }

    /// Finishes the progress bar and leaves the current message.
    pub fn finish(&self) {
        self.update_and_draw(|mut state| {
            state.pos = state.len;
            state.status = Status::DoneVisible;
        });
    }

    /// Finishes the progress bar and sets a message.
    pub fn finish_with_message(&self, msg: &str) {
        let msg = msg.to_string();
        self.update_and_draw(|mut state| {
            state.message = msg;
            state.pos = state.len;
            state.status = Status::DoneVisible;
        });
    }

    /// Finishes the progress bar and completely clears it.
    pub fn finish_and_clear(&self) {
        self.update_and_draw(|mut state| {
            state.pos = state.len;
            state.status = Status::DoneHidden;
        });
    }

    /// Sets a different draw target for the progress bar.
    ///
    /// This can be used to draw the progress bar to stderr
    /// for instance:
    ///
    /// ```rust,no_run
    /// # use indicatif::{ProgressBar, DrawTarget};
    /// let pb = ProgressBar::new(100);
    /// pb.set_draw_target(DrawTarget::stderr());
    /// ```
    pub fn set_draw_target(&self, target: DrawTarget) {
        self.state.write().draw_target = target;
    }

    fn update_and_draw<F: FnOnce(&mut ProgressState)>(&self, f: F) {
        {
            let mut state = self.state.write();
            let old_pos = state.pos;
            f(&mut state);
            let new_pos = state.pos;
            if new_pos != old_pos {
                state.est.record_step(new_pos);
            }
        }
        self.draw().ok();
    }

    fn draw(&self) -> io::Result<()> {
        let mut state = self.state.write();
        let draw_state = DrawState {
            lines: if state.should_render() {
                state.style.format_state(&*state)
            } else {
                vec![]
            },
            finished: state.is_finished(),
            force_draw: false,
            ts: Instant::now(),
        };
        state.draw_target.apply_draw_state(draw_state)
    }
}


struct MultiObject {
    done: bool,
    draw_state: Option<DrawState>,
}

struct MultiProgressState {
    objects: Vec<MultiObject>,
    draw_target: DrawTarget,
}

/// Manages multiple progress bars from different threads.
pub struct MultiProgress {
    state: RwLock<MultiProgressState>,
    joining: AtomicBool,
    tx: Sender<(usize, DrawState)>,
    rx: Receiver<(usize, DrawState)>,
}

unsafe impl Sync for MultiProgress {}

impl MultiProgress {
    /// Creates a new multi progress object that draws to stdout.
    pub fn new() -> MultiProgress {
        let (tx, rx) = channel();
        MultiProgress {
            state: RwLock::new(MultiProgressState {
                objects: vec![],
                draw_target: DrawTarget::stdout(),
            }),
            joining: AtomicBool::new(false),
            tx: tx,
            rx: rx,
        }
    }

    /// Sets a different draw target for the multiprogress bar.
    pub fn set_draw_target(&self, target: DrawTarget) {
        self.state.write().draw_target = target;
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
        bar.set_draw_target(DrawTarget::Remote(idx, self.tx.clone()));
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

        while !self.is_done() {
            let (idx, draw_state) = self.rx.recv().unwrap();
            let ts = draw_state.ts;
            let force_draw = draw_state.finished || draw_state.force_draw;

            let mut state = self.state.write();
            if draw_state.finished {
                state.objects[idx].done = true;
            }
            state.objects[idx].draw_state = Some(draw_state);

            let mut lines = vec![];
            for obj in state.objects.iter() {
                if let Some(ref draw_state) = obj.draw_state {
                    lines.extend_from_slice(&draw_state.lines[..]);
                }
            }

            let finished = !state.objects.iter().any(|ref x| x.done);
            state.draw_target.apply_draw_state(DrawState {
                lines: lines,
                force_draw: force_draw,
                finished: finished,
                ts: ts,
            })?;
        }

        if clear {
            let mut state = self.state.write();
            state.draw_target.apply_draw_state(DrawState {
                lines: vec![],
                finished: true,
                force_draw: true,
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
        self.update_and_draw(|mut state| {
            state.status = Status::DoneHidden;
        });
    }
}
