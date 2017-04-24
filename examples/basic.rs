extern crate indicatif;

use std::thread;
use std::time::Duration;

use indicatif::{ProgressBar, MultiProgress};

fn main() {
    let mut m = MultiProgress::new();

    let pb = m.add(ProgressBar::new(128));
    pb.enable_spinner();
    let _ = thread::spawn(move || {
        for i in 0..128 {
            pb.set_message(&format!("item #{}", i + 1));
            pb.inc(1);
            thread::sleep(Duration::from_millis(15));
        }
        pb.finish_with_message("done");
    });

    let pb = m.add(ProgressBar::new(256));
    pb.enable_spinner();
    let _ = thread::spawn(move || {
        for i in 0..256 {
            pb.set_message(&format!("item #{}", i + 1));
            pb.inc(1);
            thread::sleep(Duration::from_millis(8));
        }
        pb.finish_with_message("done");
    });

    let pb = m.add(ProgressBar::new(1024));
    pb.enable_spinner();
    let _ = thread::spawn(move || {
        for i in 0..1024 {
            pb.set_message(&format!("item #{}", i + 1));
            pb.inc(1);
            thread::sleep(Duration::from_millis(1));
        }
        pb.finish_with_message("done");
    });

    m.join_and_clear().unwrap();
}
