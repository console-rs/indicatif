use clap::Parser;
use std::fmt::Debug;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use console::style;
use indicatif::{
    MultiProgress, MultiProgressAlignment, ProgressBar, ProgressBarBuilder, ProgressStyle,
};
use once_cell::sync::Lazy;
use rand::rngs::ThreadRng;
use rand::{Rng, RngExt};

#[derive(Debug, Clone)]
enum Action {
    ModifyTree(usize),
    IncProgressBar(usize),
    Stop,
}

#[derive(Clone, Debug)]
enum Elem {
    AddItem(Item),
    RemoveItem(Index),
}

#[derive(Clone, Debug)]
struct Item {
    key: String,
    index: usize,
    indent: usize,
}

#[derive(Clone, Debug)]
struct Index(usize);

const PB_LEN: u64 = 32;
static ELEM_IDX: AtomicUsize = AtomicUsize::new(0);

static ELEMENTS: Lazy<[Elem; 27]> = Lazy::new(|| {
    [
        Elem::AddItem(Item {
            indent: 9,
            index: 0,
            key: "dog".to_string(),
        }),
        Elem::AddItem(Item {
            indent: 0,
            index: 0,
            key: "temp_1".to_string(),
        }),
        Elem::AddItem(Item {
            indent: 8,
            index: 1,
            key: "lazy".to_string(),
        }),
        Elem::AddItem(Item {
            indent: 0,
            index: 1,
            key: "temp_2".to_string(),
        }),
        Elem::AddItem(Item {
            indent: 1,
            index: 0,
            key: "the".to_string(),
        }),
        Elem::AddItem(Item {
            indent: 0,
            index: 0,
            key: "temp_3".to_string(),
        }),
        Elem::AddItem(Item {
            indent: 7,
            index: 3,
            key: "a".to_string(),
        }),
        Elem::AddItem(Item {
            indent: 0,
            index: 3,
            key: "temp_4".to_string(),
        }),
        Elem::AddItem(Item {
            indent: 6,
            index: 2,
            key: "over".to_string(),
        }),
        Elem::RemoveItem(Index(6)),
        Elem::RemoveItem(Index(4)),
        Elem::RemoveItem(Index(3)),
        Elem::RemoveItem(Index(0)),
        Elem::AddItem(Item {
            indent: 0,
            index: 2,
            key: "temp_5".to_string(),
        }),
        Elem::AddItem(Item {
            indent: 4,
            index: 1,
            key: "fox".to_string(),
        }),
        Elem::AddItem(Item {
            indent: 0,
            index: 1,
            key: "temp_6".to_string(),
        }),
        Elem::AddItem(Item {
            indent: 2,
            index: 1,
            key: "quick".to_string(),
        }),
        Elem::AddItem(Item {
            indent: 0,
            index: 1,
            key: "temp_7".to_string(),
        }),
        Elem::AddItem(Item {
            indent: 5,
            index: 5,
            key: "jumps".to_string(),
        }),
        Elem::AddItem(Item {
            indent: 0,
            index: 5,
            key: "temp_8".to_string(),
        }),
        Elem::AddItem(Item {
            indent: 3,
            index: 4,
            key: "brown".to_string(),
        }),
        Elem::AddItem(Item {
            indent: 0,
            index: 3,
            key: "temp_9".to_string(),
        }),
        Elem::RemoveItem(Index(10)),
        Elem::RemoveItem(Index(7)),
        Elem::RemoveItem(Index(4)),
        Elem::RemoveItem(Index(3)),
        Elem::RemoveItem(Index(1)),
    ]
});

#[derive(Debug, Parser)]
pub struct Config {
    #[clap(long)]
    bottom_alignment: bool,
}

/// The example demonstrates the usage of `MultiProgress` and further extends `multi-tree` example.
/// Now the example has 3 different actions implemented, and the item tree can be modified
/// by inserting or removing progress bars. The progress bars to be removed eventually
/// have messages with pattern `"temp_*"`.
///
/// Also the command option `--bottom-alignment` is used to control the vertical alignment of the
/// `MultiProgress`. To enable this run it with
/// ```ignore
/// cargo run --example multi-tree-ext -- --bottom-alignment
/// ```
pub fn main() {
    let conf: Config = Config::parse();
    let mp = Arc::new(MultiProgress::new());
    let alignment = if conf.bottom_alignment {
        MultiProgressAlignment::Bottom
    } else {
        MultiProgressAlignment::Top
    };
    mp.set_alignment(alignment);
    let sty_main = ProgressStyle::with_template("{bar:40.green/yellow} {pos:>4}/{len:4}").unwrap();
    let sty_aux =
        ProgressStyle::with_template("[{pos:>2}/{len:2}] {prefix}{spinner:.green} {msg}").unwrap();
    let sty_fin = ProgressStyle::with_template("[{pos:>2}/{len:2}] {prefix}{msg}").unwrap();

    let pb_main = mp.register(
        ProgressBarBuilder::new(
            ELEMENTS
                .iter()
                .map(|e| match e {
                    Elem::AddItem(_) => PB_LEN,
                    Elem::RemoveItem(_) => 1,
                })
                .sum(),
        )
        .with_style(sty_main),
    );

    let mut items: Vec<(&Item, ProgressBar)> = Vec::with_capacity(ELEMENTS.len());

    let mp2 = Arc::clone(&mp);
    let mut rng = ThreadRng::default();
    pb_main.tick();
    loop {
        match get_action(&mut rng, &items) {
            Action::Stop => {
                // all elements were exhausted
                pb_main.finish();
                return;
            }
            Action::ModifyTree(elem_idx) => match &ELEMENTS[elem_idx] {
                Elem::AddItem(item) => {
                    let pb = mp2.register_at(
                        item.index,
                        ProgressBarBuilder::new(PB_LEN)
                            .with_style(sty_aux.clone())
                            .with_prefix("  ".repeat(item.indent))
                            .with_message(item.key.clone()),
                    );
                    items.insert(item.index, (item, pb));
                }
                Elem::RemoveItem(Index(index)) => {
                    let (_, pb) = items.remove(*index);
                    mp2.remove(&pb);
                    pb_main.inc(pb.length().unwrap() - pb.position());
                }
            },
            Action::IncProgressBar(item_idx) => {
                let (item, pb) = &items[item_idx];
                pb.inc(1);
                let pos = pb.position();
                if pos >= pb.length().unwrap() {
                    pb.set_style(sty_fin.clone());
                    pb.finish_with_message(format!("{} {}", style("✔").green(), item.key));
                }
                pb_main.inc(1);
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
}

/// The function guarantees to return the action, that is valid for the current tree.
fn get_action(rng: &mut dyn Rng, items: &[(&Item, ProgressBar)]) -> Action {
    let elem_idx = ELEM_IDX.load(Ordering::SeqCst);
    // the indices of those items, that not completed yet
    let uncompleted = items
        .iter()
        .enumerate()
        .filter(|(_, (_, pb))| {
            let pos = pb.position();
            pos < pb.length().unwrap()
        })
        .map(|(idx, _)| idx)
        .collect::<Vec<usize>>();
    let k = rng.random_range(0..16);
    if (k > 0 || k == 0 && elem_idx == ELEMENTS.len()) && !uncompleted.is_empty() {
        let idx = rng.random_range(0..uncompleted.len() as u64) as usize;
        Action::IncProgressBar(uncompleted[idx])
    } else if elem_idx < ELEMENTS.len() {
        ELEM_IDX.fetch_add(1, Ordering::SeqCst);
        Action::ModifyTree(elem_idx)
    } else {
        // nothing to do more
        Action::Stop
    }
}
