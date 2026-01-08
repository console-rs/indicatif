use std::thread;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};

fn main() {
    let template = "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})";
    let template_finish = "[{elapsed_precise}] [{wide_bar:.green/blue}] {total_bytes}";

    let style = ProgressStyle::with_template(template).unwrap();
    let finish_style = ProgressStyle::with_template(template_finish).unwrap();

    let pb = ProgressBar::new(1024)
        .with_style(style)
        .with_finish_style(finish_style)
        .with_finish(indicatif::ProgressFinish::AndLeave);

    for _ in 0..1024 {
        thread::sleep(Duration::from_millis(5));
        pb.inc(1);
    }

    pb.finish_using_style();
}
