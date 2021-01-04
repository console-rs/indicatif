use std::thread;
use std::time::Duration;

use indicatif::ProgressBar;

fn main() {
    let p = ProgressBar::new_spinner();
    for _ in 0..10 {
        p.set_message("doing fast work (you should rarely see this)");
        // Any sleep shorter than the default redraw interval (15 Hz) should have the same effect,
        // as should not sleeping at all.
        thread::sleep(Duration::from_millis(5));
        p.set_message("doing slow work");
        thread::sleep(Duration::from_secs(1));
    }
}
