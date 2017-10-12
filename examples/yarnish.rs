extern crate indicatif;
extern crate console;
extern crate rand;

use std::thread;
use std::time::{Instant, Duration};
use rand::Rng;

use indicatif::{ProgressBar, ProgressStyle, MultiProgress, HumanDuration};
use console::{Emoji, style};


static PACKAGES: &'static [&'static str] = &[
    "fs-events",
    "my-awesome-module",
    "emoji-speaker",
    "wrap-ansi",
    "stream-browserify",
    "acorn-dynamic-import",
];

static COMMANDS: &'static [&'static str] = &[
    "cmake .",
    "make",
    "make clean",
    "gcc foo.c -o foo",
    "gcc bar.c -o bar",
    "./helper.sh rebuild-cache",
    "make all-clean",
    "make test",
];

static LOOKING_GLASS: Emoji = Emoji("🔍  ", "");
static TRUCK: Emoji = Emoji("🚚  ", "");
static CLIP: Emoji = Emoji("🔗  ", "");
static PAPER: Emoji = Emoji("📃  ", "");
static SPARKLE: Emoji = Emoji("✨ ", ":-)");


pub fn main() {
    let mut rng = rand::thread_rng();
    let started = Instant::now();
    let spinner_style = ProgressStyle::default_spinner()
        .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
        .template("{prefix:.bold.dim} {spinner} {wide_msg}");

    println!("{} {}Resolving packages...", style("[1/4]").bold().dim(), LOOKING_GLASS);
    println!("{} {}Fetching packages...", style("[2/4]").bold().dim(), TRUCK);

    println!("{} {}Linking dependencies...", style("[3/4]").bold().dim(), CLIP);
    let deps = 1232;
    let pb = ProgressBar::new(deps);
    for _ in 0..deps {
        pb.inc(1);
        thread::sleep(Duration::from_millis(3));
    }
    pb.finish_and_clear();

    println!("{} {}Building fresh packages...", style("[4/4]").bold().dim(), PAPER);
    let m = MultiProgress::new();
    for i in 0..4 {
        let count = rng.gen_range(30, 80);
        let pb = m.add(ProgressBar::new(count));
        pb.set_style(spinner_style.clone());
        pb.set_prefix(&format!("[{}/?]", i + 1));
        let _ = thread::spawn(move || {
            let mut rng = rand::thread_rng();
            let pkg = rng.choose(PACKAGES).unwrap();
            for _ in 0..count {
                let cmd = rng.choose(COMMANDS).unwrap();
                pb.set_message(&format!("{}: {}", pkg, cmd));
                pb.inc(1);
                thread::sleep(Duration::from_millis(rng.gen_range(25, 200)));
            }
            pb.finish_with_message("waiting...");
        });
    }
    m.join_and_clear().unwrap();

    println!("{} Done in {}", SPARKLE, HumanDuration(started.elapsed()));
}
