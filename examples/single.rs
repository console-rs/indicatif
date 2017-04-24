extern crate indicatif;

use std::thread;
use std::time::Duration;

use indicatif::ProgressBar;

fn main() {
    let pb = ProgressBar::new(1024);
    pb.enable_spinner();
    for i in 0..1024 {
        pb.set_message(&format!("item #{}", i + 1));
        pb.inc(1);
        thread::sleep(Duration::from_millis(8));
    }
    pb.finish_with_message("done");
}
