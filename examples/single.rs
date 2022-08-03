use std::thread;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

fn main() {
    // create a progressbar. Default refresh rate is 20 times per second (every 50 ms)
    let pb = ProgressBar::new(1024);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{wide_bar} {percent}%{msg}] ({per_sec})")
            .unwrap(),
    );
    for _ in 0..1024 {
        pb.inc(1);
        thread::sleep(Duration::from_millis(5));
    }
    pb.finish_with_message(" done");

    // create a progressbar that refreshes once per second (every 1000 ms)
    let pb_1hz = ProgressBar::with_draw_target(Some(1024), ProgressDrawTarget::stderr_with_hz(1));
    pb_1hz.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{wide_bar} {percent}%{msg}] ({per_sec})")
            .unwrap(),
    );
    for _ in 0..1024 {
        pb_1hz.inc(1);
        thread::sleep(Duration::from_millis(5));
    }
    pb_1hz.finish_with_message(" done");
}
