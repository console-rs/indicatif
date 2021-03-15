use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use console::Term;
use indicatif::{ProgressBar, ProgressStyle};
use rand::Rng;

static CRATES: &[&str] = &[
    "console",
    "lazy_static",
    "libc",
    "regex",
    "regex-syntax",
    "terminal_size",
    "libc",
    "unicode-width",
    "lazy_static",
    "number_prefix",
    "regex",
    "rand",
    "getrandom",
    "cfg-if",
    "libc",
    "rand_chacha",
    "ppv-lite86",
    "rand_core",
    "getrandom",
    "rand_core",
    "tokio",
    "bytes",
    "pin-project-lite",
    "slab",
    "indicatif",
    "cargo(example)",
];

fn main() {
    // number of cpus, constant
    let cpus = 4;

    // mimic cargo progress bar although it behaves a bit different
    let pb = ProgressBar::new(CRATES.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            // note that bar size is fixed unlike cargo which is dynamic
            // and also the truncation in cargo uses trailers (`...`)
            .template(if Term::stdout().size().1 > 80 {
                "{prefix:>12.green} [{bar:57}] {pos}/{len} {wide_msg}"
            } else {
                "{prefix:>12.green} [{bar:57}] {pos}/{len}"
            })
            .progress_chars("#> "),
    );
    pb.set_prefix("Building");

    // process in another thread
    // crates to be iterated but not exactly a tree
    let crates = Arc::new(Mutex::new(CRATES.iter()));
    let (tx, rx) = mpsc::channel();
    for n in 0..cpus {
        let tx = tx.clone();
        let crates = crates.clone();
        thread::spawn(move || {
            let mut rng = rand::thread_rng();
            loop {
                let krate = crates.lock().unwrap().next();
                // notify main thread if n thread is processing a crate
                tx.send((n, krate)).unwrap();
                if let Some(krate) = krate {
                    thread::sleep(Duration::from_millis(
                        // last compile and linking is always slow, let's mimic that
                        if CRATES[CRATES.len() - 1] == *krate {
                            rng.gen_range(1000..2000)
                        } else {
                            rng.gen_range(50..300)
                        },
                    ));
                } else {
                    break;
                }
            }
        });
    }
    // drop tx to stop waiting
    drop(tx);

    // do progress drawing in main thread
    let mut processing = vec![None; cpus];
    while let Ok((n, krate)) = rx.recv() {
        processing[n] = krate;
        let crates: Vec<&str> = processing.iter().filter_map(|t| t.copied()).collect();
        pb.set_message(&crates.join(", "));
        if krate.is_some() {
            // crate is built
            pb.inc(1);
        }
    }
    pb.finish_and_clear();
}
