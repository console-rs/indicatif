extern crate indicatif;

use std::thread;
use std::cmp::min;
use std::time::Duration;
use std::borrow::Cow;

use indicatif::{ProgressBar, ProgressStyle};

fn main() {
    let mut downloaded = 0;
    let total_size = 231231231;

    let pb = ProgressBar::new(total_size);
    let mut sty = ProgressStyle::default();
    sty.bar_template = Cow::Borrowed("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes}");
    sty.progress_chars = "#>-".chars().collect();
    pb.set_style(sty);

    while downloaded < total_size {
        let new = min(downloaded + 223211, total_size);
        downloaded = new;
        pb.set_position(new);
        thread::sleep(Duration::from_millis(12));
    }

    pb.finish_with_message("downloaded");
}
