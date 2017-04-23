use std::io;
use std::iter::repeat;
use std::sync::mpsc::{channel, Sender, Receiver};

use term::Term;
use progress::{ProgressIndicator, DrawTarget, DrawState};


pub struct Multiplexer {
    objects: u32,
    term: Term,
    tx: Sender<(u32, DrawState)>,
    rx: Receiver<(u32, DrawState)>,
}

impl Multiplexer {
    pub fn new() -> Multiplexer {
        let (tx, rx) = channel();
        Multiplexer {
            objects: 0,
            term: Term::stdout(),
            tx: tx,
            rx: rx,
        }
    }

    pub fn add<T: ProgressIndicator>(&mut self, t: T) -> T {
        t.set_draw_target(DrawTarget::Remote(self.objects,
                                             self.tx.clone()));
        self.objects += 1;
        t
    }

    pub fn join(&self) -> io::Result<()> {
        let mut outstanding = repeat(true).take(self.objects as usize).collect::<Vec<_>>();
        let mut draw_states: Vec<Option<DrawState>> = outstanding.iter().map(|_| None).collect();

        //self.term.show_cursor(false)?;

        while outstanding.iter().any(|&x| x) {
            let (idx, draw_state) = self.rx.recv().unwrap();
            let idx = idx as usize;

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

        //self.term.show_cursor(true)?;

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

        Ok(())
    }
}
