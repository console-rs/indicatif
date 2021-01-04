// From https://github.com/mitsuhiko/indicatif/pull/166

use std::thread;
use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar};

fn main() {
    let mpb = MultiProgress::new();
    let pb1 = mpb.add(ProgressBar::new(30));
    let pb2 = mpb.add(ProgressBar::new(30));

    let _ = thread::spawn(move || {
        for _ in 0..30 {
            pb1.inc(1);
            thread::sleep(Duration::from_millis(1000));
        }
    });
    let _ = thread::spawn(move || {
        for _ in 0..30 {
            pb2.inc(1);
            thread::sleep(Duration::from_millis(1000));
        }
    });

    mpb.join().unwrap();
}
