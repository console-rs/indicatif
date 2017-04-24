use std::io;
use std::iter::repeat;
use std::borrow::Cow;
use std::sync::mpsc::{channel, Sender, Receiver};

use parking_lot::RwLock;

use term::Term;

#[derive(Default)]
pub struct Style {
    tick_chars: Vec<char>,
    progress_chars: Vec<char>,
}

#[derive(Default, Clone)]
pub struct DrawState {
    pub lines: Vec<String>,
    pub finished: bool,
}

enum Status {
    InProgress,
    DoneVisible,
    DoneHidden,
}

pub enum DrawTarget {
    Term(Term, Option<DrawState>),
    Remote(usize, Sender<(usize, DrawState)>),
    Hidden,
}

impl DrawTarget {
    pub fn stdout() -> DrawTarget {
        DrawTarget::Term(Term::stdout(), None)
    }

    pub fn get_draw_state(&self, state: &ProgressState) -> DrawState {
        let (pos, len) = state.position();
        let mut lines = vec![];
        if state.should_render() {
            lines.push(format!("{}  {} / {} | {}",
                               state.current_tick_char(),
                               pos, len, state.message()));
        }
        DrawState {
            lines: lines,
            finished: state.is_finished(),
        }
    }

    pub fn update(&mut self, draw_state: DrawState) -> io::Result<()> {
        match *self {
            DrawTarget::Term(ref term, ref mut last_state) => {
                if let Some(ref last_state) = *last_state {
                    last_state.clear_term(term)?;
                }
                draw_state.draw_to_term(term)?;
                term.flush()?;
                *last_state = Some(draw_state);
            }
            DrawTarget::Remote(idx, ref chan) => {
                chan.send((idx, draw_state));
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

impl Style {
    pub fn new() -> Style {
        Default::default()
    }

    pub fn default() -> Style {
        Style {
            tick_chars: "⠁⠁⠉⠙⠚⠒⠂⠂⠒⠲⠴⠤⠄⠄⠤⠠⠠⠤⠦⠖⠒⠐⠐⠒⠓⠋⠉⠈⠈ ".chars().collect(),
            progress_chars: "██░".chars().collect(),
        }
    }

    pub fn get_tick_char(&self, idx: u64) -> char {
        self.tick_chars[(idx as usize) % (self.tick_chars.len() - 1)]
    }

    pub fn get_final_tick_char(&self) -> char {
        self.tick_chars[self.tick_chars.len() - 1]
    }

    pub fn get_progress_char(&self, idx: u64) -> char {
        self.progress_chars[(idx as usize) % self.progress_chars.len()]
    }
}

pub struct ProgressState {
    style: Style,
    draw_target: DrawTarget,
    message: String,
    pos: u64,
    len: u64,
    tick: u64,
    status: Status,
}

impl ProgressState {
    pub fn current_tick_char(&self) -> char {
        if self.is_finished() {
            self.style.get_final_tick_char()
        } else {
            self.style.get_tick_char(self.tick)
        }
    }

    pub fn has_spinner(&self) -> bool {
        self.tick != !0
    }

    pub fn has_progress(&self) -> bool {
        self.len != !0
    }

    pub fn is_finished(&self) -> bool {
        match self.status {
            Status::InProgress => false,
            Status::DoneVisible => true,
            Status::DoneHidden => true,
        }
    }

    pub fn should_render(&self) -> bool {
        match self.status {
            Status::DoneHidden => false,
            _ => true,
        }
    }

    pub fn position(&self) -> (u64, u64) {
        (self.pos, self.len)
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

pub struct ProgressBar {
    state: RwLock<ProgressState>,
}

impl ProgressBar {
    pub fn new(len: u64) -> ProgressBar {
        ProgressBar {
            state: RwLock::new(ProgressState {
                style: Style::default(),
                draw_target: DrawTarget::stdout(),
                message: "".into(),
                pos: 0,
                len: len,
                tick: !0,
                status: Status::InProgress,
            }),
        }
    }

    fn update_state<F: FnOnce(&mut ProgressState)>(&self, f: F) {
        {
            let mut state = self.state.write();
            f(&mut state);
        }
        self.draw();
    }

    pub fn draw(&self) -> io::Result<()> {
        let mut state = self.state.write();
        let draw_state = state.draw_target.get_draw_state(&*state);
        state.draw_target.update(draw_state)
    }

    pub fn enable_spinner(&self) {
        self.update_state(|mut state| {
            if state.tick == !0 {
                state.tick = 0;
            }
        });
    }

    pub fn disable_spinner(&self) {
        self.update_state(|mut state| {
            if state.tick != !0 {
                state.tick = !0;
            }
        });
    }

    pub fn tick(&self) {
        self.update_state(|mut state| {
            if state.tick == !0 {
                state.tick = 0;
            } else {
                state.tick += 1;
            }
        });
    }

    pub fn inc(&self, delta: u64) {
        self.update_state(|mut state| {
            state.pos += delta;
            if state.tick != !0 {
                state.tick += 1;
            }
        })
    }

    pub fn set_length(&self, len: u64) {
        self.update_state(|mut state| {
            state.len = len;
        })
    }

    pub fn set_message(&self, msg: &str) {
        let msg = msg.to_string();
        self.update_state(|mut state| {
            state.message = msg;
        })
    }

    pub fn finish_with_message(&self, msg: &str) {
        let msg = msg.to_string();
        self.update_state(|mut state| {
            state.message = msg;
            state.pos = state.len;
            state.status = Status::DoneVisible;
        });
    }

    pub fn finish_and_clear(&self) {
        self.update_state(|mut state| {
            state.pos = state.len;
            state.status = Status::DoneHidden;
        });
    }

    pub fn set_draw_target(&self, target: DrawTarget) {
        self.state.write().draw_target = target;
    }
}


pub struct MultiProgress {
    objects: usize,
    term: Term,
    tx: Sender<(usize, DrawState)>,
    rx: Receiver<(usize, DrawState)>,
}

impl MultiProgress {
    pub fn new() -> MultiProgress {
        let (tx, rx) = channel();
        MultiProgress {
            objects: 0,
            term: Term::stdout(),
            tx: tx,
            rx: rx,
        }
    }

    pub fn add(&mut self, bar: ProgressBar) -> ProgressBar {
        bar.set_draw_target(DrawTarget::Remote(self.objects,
                                               self.tx.clone()));
        self.objects += 1;
        bar
    }

    pub fn join(self) -> io::Result<()> {
        let mut outstanding = repeat(true).take(self.objects as usize).collect::<Vec<_>>();
        let mut draw_states: Vec<Option<DrawState>> = outstanding.iter().map(|_| None).collect();

        while outstanding.iter().any(|&x| x) {
            let (idx, draw_state) = self.rx.recv().unwrap();

            if draw_state.finished {
                outstanding[idx] = false;
            }

            // clear
            {
                let to_clear = draw_states.iter().map(|ref item_opt| {
                    if let Some(ref item) = **item_opt {
                        item.lines.len()
                    } else {
                        0
                    }
                }).sum();
                self.term.clear_last_lines(to_clear)?;
            }

            // update
            draw_states[idx] = Some(draw_state);

            // redraw
            for draw_state_opt in draw_states.iter() {
                if let Some(ref draw_state) = *draw_state_opt {
                    draw_state.draw_to_term(&self.term)?;
                }
            }

            self.term.flush()?;
        }

        Ok(())
    }
}

impl Drop for ProgressBar {
    fn drop(&mut self) {
        if self.state.read().is_finished() {
            return;
        }
        self.update_state(|mut state| {
            state.status = Status::DoneHidden;
        });
    }
}
