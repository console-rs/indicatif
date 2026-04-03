use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use indicatif::{MultiBar, MultiProgress, ProgressBar, ProgressStyle};
use once_cell::sync::Lazy;
use rand::rngs::ThreadRng;
use rand::{Rng, RngExt};

#[derive(Debug, Clone)]
enum Action {
    AddProgressBar(usize),
    IncProgressBar(usize),
}

#[derive(Clone, Debug)]
struct Elem {
    key: String,
    index: usize,
    indent: usize,
    len: u64,
}

static ELEMENTS: Lazy<[Elem; 9]> = Lazy::new(|| {
    [
        Elem {
            indent: 1,
            index: 0,
            len: 32,
            key: "jumps".to_string(),
        },
        Elem {
            indent: 2,
            index: 1,
            len: 32,
            key: "lazy".to_string(),
        },
        Elem {
            indent: 0,
            index: 0,
            len: 32,
            key: "the".to_string(),
        },
        Elem {
            indent: 3,
            index: 3,
            len: 32,
            key: "dog".to_string(),
        },
        Elem {
            indent: 2,
            index: 2,
            len: 32,
            key: "over".to_string(),
        },
        Elem {
            indent: 2,
            index: 1,
            len: 32,
            key: "brown".to_string(),
        },
        Elem {
            indent: 1,
            index: 1,
            len: 32,
            key: "quick".to_string(),
        },
        Elem {
            indent: 3,
            index: 5,
            len: 32,
            key: "a".to_string(),
        },
        Elem {
            indent: 3,
            index: 3,
            len: 32,
            key: "fox".to_string(),
        },
    ]
});

/// The example implements the tree-like collection of progress bars, where elements are
/// added on the fly and progress bars get incremented until all elements is added and
/// all progress bars finished.
/// On each iteration `get_action` function returns some action, and when the tree gets
/// complete, the function returns `None`, which finishes the loop.
fn main() {
    let mp = Arc::new(MultiProgress::new());
    let sty_main = ProgressStyle::with_template("{bar:40.green/yellow} {pos:>4}/{len:4}").unwrap();
    let sty_aux = ProgressStyle::with_template("{spinner:.green} {msg} {pos:>4}/{len:4}").unwrap();

    let pb_main = mp.add(MultiBar::new(ELEMENTS.iter().map(|e| e.len).sum()).with_style(sty_main));

    let tree: Arc<Mutex<Vec<(&Elem, ProgressBar)>>> =
        Arc::new(Mutex::new(Vec::with_capacity(ELEMENTS.len())));
    let tree2 = Arc::clone(&tree);

    let mp2 = Arc::clone(&mp);
    let _ = thread::spawn(move || {
        let mut rng = ThreadRng::default();
        pb_main.tick();
        loop {
            thread::sleep(Duration::from_millis(15));
            match get_action(&mut rng, &tree) {
                None => {
                    // all elements were exhausted
                    pb_main.finish();
                    return;
                }
                Some(Action::AddProgressBar(el_idx)) => {
                    let elem = &ELEMENTS[el_idx];
                    let pb = mp2.insert(
                        elem.index + 1,
                        MultiBar::new(elem.len)
                            .with_style(sty_aux.clone())
                            .with_message(format!("{}  {}", "  ".repeat(elem.indent), elem.key)),
                    );
                    tree.lock().unwrap().insert(elem.index, (elem, pb));
                }
                Some(Action::IncProgressBar(el_idx)) => {
                    let tree = tree.lock().unwrap();
                    let (elem, pb) = &tree[el_idx];
                    pb.inc(1);
                    let pos = pb.position();
                    if pos >= pb.length().unwrap() {
                        pb.finish_with_message(format!(
                            "{}{} {}",
                            "  ".repeat(elem.indent),
                            "✔",
                            elem.key
                        ));
                    }
                    pb_main.inc(1);
                }
            }
        }
    })
    .join();

    println!("===============================");
    println!("the tree should be the same as:");
    for (elem, _) in tree2.lock().unwrap().iter() {
        println!("{}  {}", "  ".repeat(elem.indent), elem.key);
    }
}

/// The function guarantees to return the action, that is valid for the current tree.
fn get_action(rng: &mut dyn Rng, tree: &Mutex<Vec<(&Elem, ProgressBar)>>) -> Option<Action> {
    let elem_len = ELEMENTS.len() as u64;
    let list_len = tree.lock().unwrap().len() as u64;
    let sum_free = tree
        .lock()
        .unwrap()
        .iter()
        .map(|(_, pb)| {
            let pos = pb.position();
            let len = pb.length().unwrap();
            len - pos
        })
        .sum::<u64>();

    if sum_free == 0 && list_len == elem_len {
        // nothing to do more
        None
    } else if sum_free == 0 && list_len < elem_len {
        // there is no place to make an increment
        Some(Action::AddProgressBar(tree.lock().unwrap().len()))
    } else {
        loop {
            let list = tree.lock().unwrap();
            let k = rng.random_range(0..17);
            if k == 0 && list_len < elem_len {
                return Some(Action::AddProgressBar(list.len()));
            } else {
                let l = (k % list_len) as usize;
                let (_, pb) = &list[l];
                let pos = pb.position();
                let len = pb.length();
                if pos < len.unwrap() {
                    return Some(Action::IncProgressBar(l));
                }
            }
        }
    }
}
