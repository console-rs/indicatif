#![allow(dead_code)]

use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use once_cell::sync::Lazy;
use std::sync::mpsc::{channel, Sender};
use std::thread;
use std::time::{Duration, Instant};

use console::{style, Emoji};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rand::seq::SliceRandom;
use rand::Rng;

static PACKAGES: &[&str] = &[
    "fs-events",
    "my-awesome-module",
    "emoji-speaker",
    "wrap-ansi",
    "stream-browserify",
    "acorn-dynamic-import",
];

static COMMANDS: &[&str] = &[
    "cmake .",
    "make",
    "make clean",
    "gcc foo.c -o foo",
    "gcc bar.c -o bar",
    "./helper.sh rebuild-cache",
    "make all-clean",
    "make test",
];

static SERVER: Emoji<'_, '_> = Emoji("🖥️  ", "");
static CONFIG: Emoji<'_, '_> = Emoji("⚙️  ", "");
static CONNECTION: Emoji<'_, '_> = Emoji("🔗  ", "");
static PROCESS: Emoji<'_, '_> = Emoji("🔄  ", "");
static _SPARKLE: Emoji<'_, '_> = Emoji("✨ ", ":-)");

const MIN_PARALLEL_PROCESSES: usize = 3;
const SPAWN_PARALLEL_PROCESSES_THRESHOLD: usize = 6;
const MAX_PARALLEL_PROCESSES: usize = 16;

static SPINNER_STYLE: Lazy<ProgressStyle> = Lazy::new(|| {
    ProgressStyle::with_template("{prefix:.bold.dim} {spinner} {wide_msg}")
        .unwrap()
        .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
});

pub fn main() {
    let mut rng = rand::thread_rng();
    let ongoing_processes = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = channel::<CompletionMessage>();
    let _started = Instant::now();
    let _spinner_style = ProgressStyle::with_template("{prefix:.bold.dim} {spinner} {wide_msg}")
        .unwrap()
        .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ");

    println!(
        "{} {}Initializing build server...",
        style("[1/4]").bold().dim(),
        SERVER
    );
    println!(
        "{} {}Loading server configurations...",
        style("[2/4]").bold().dim(),
        CONFIG
    );

    println!(
        "{} {}Establishing connections with package source servers...",
        style("[3/4]").bold().dim(),
        CONNECTION
    );
    let deps = 1232;
    let pb = ProgressBar::new(deps);
    for _ in 0..deps {
        thread::sleep(Duration::from_millis(3));
        pb.inc(1);
    }
    pb.finish_and_clear();

    println!(
        "{} {}Ongoing processing of incoming build requests...",
        style("[4/4]").bold().dim(),
        PROCESS
    );
    let m = MultiProgress::new();
    let mut process_id_counter = 1;
    loop {
        while let Ok(msg) = rx.try_recv() {
            m.remove(&msg.progress_bar);
            m.println(format!("Process {} completed: {}", msg.id, msg.result)).unwrap();
            ongoing_processes.fetch_sub(1, Ordering::SeqCst);
        }

        let current_processes = ongoing_processes.load(Ordering::SeqCst);

        if current_processes < MIN_PARALLEL_PROCESSES {
            for _ in 0..(MAX_PARALLEL_PROCESSES - current_processes) {
                spawn_new_process(&m, ongoing_processes.clone(), tx.clone(), process_id_counter);
                process_id_counter += 1;
            }
        } else if current_processes <= SPAWN_PARALLEL_PROCESSES_THRESHOLD {
            let max_at_once = MAX_PARALLEL_PROCESSES - current_processes;
            for _ in 0.. rng.gen_range(1..max_at_once) {
                spawn_new_process(&m, ongoing_processes.clone(), tx.clone(), process_id_counter);
                process_id_counter += 1;
            }
        }

        thread::sleep(Duration::from_millis(100));
    }

}

fn spawn_new_process(
    m: &MultiProgress,
    ongoing_processes: Arc<AtomicUsize>,
    tx: Sender<CompletionMessage>,
    process_id: usize,
) {
    let mut rng = rand::thread_rng();
    let count = rng.gen_range(20..120);
    let pb = m.add(ProgressBar::new(count));
    pb.set_style(SPINNER_STYLE.clone());
    pb.set_prefix(format!("[{}/?]", process_id));

    ongoing_processes.fetch_add(1, Ordering::SeqCst);

    let tx_clone = tx.clone();
    thread::spawn(move || {
        let mut rng = rand::thread_rng();
        let pkg = PACKAGES.choose(&mut rng).unwrap();
        for _ in 0..count {
            let cmd = COMMANDS.choose(&mut rng).unwrap();
            thread::sleep(Duration::from_millis(rng.gen_range(25..200)));
            pb.set_message(format!("{pkg}: {cmd}"));
            pb.inc(1);
        }
        pb.finish_with_message("done");

        let _ = tx_clone.send(CompletionMessage {
            progress_bar: pb.clone(),
            id: process_id,
            result: "Success. Response send to client.".to_string(),
        });
    });
}

struct CompletionMessage {
    progress_bar: ProgressBar,
    id: usize,
    result: String,
}
