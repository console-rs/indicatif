use std::time::Instant;

use indicatif::{HumanDuration, ProgressBar};

fn many_units_of_easy_work(n: u64, label: &str) {
    let pb = ProgressBar::new(n);

    let mut sum = 0;
    let started = Instant::now();
    for i in 0..n {
        // Any quick computation, followed by an update to the progress bar.
        sum += 2 * i + 3;
        pb.inc(1);
    }
    pb.finish();
    let finished = started.elapsed();

    println!(
        "[{}] Sum ({}) calculated in {}",
        label,
        sum,
        HumanDuration(finished)
    );
}

fn main() {
    const N: u64 = 1 << 20;

    // Perform a long sequence of many simple computations monitored by a
    // default progress bar.
    many_units_of_easy_work(N, "Default progress bar ");

    // Perform the same sequence of many simple computations, but only redraw
    // after each 0.005% of additional progress.
    many_units_of_easy_work(N, "Draw delta is 0.005% ");

    // Perform the same sequence of many simple computations, but only redraw
    // after each 0.01% of additional progress.
    many_units_of_easy_work(N, "Draw delta is 0.01%  ");
}
