use indicatif::{ProgressBar, ProgressStyle};
use std::thread;

use std::time::Duration;

const TOTAL: u64 = 13;

fn main() {
    let progress: u64 = 0;

    let bar = ProgressBar::new(TOTAL);

    let style =
        ProgressStyle::with_template("{percent:-5.green.on_black}% - {pos}/{len} - {percent}%")
            .unwrap()
            .progress_chars("#>-");

    bar.set_style(style);

    bar.set_position(progress);

    {
        for _ in 0..TOTAL {
            bar.inc(1);
            thread::sleep(Duration::from_nanos(123456789));
        }
    }

    bar.finish();
}
