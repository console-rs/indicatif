//! Example of asynchronous multiple progress bars.
//!
//! The child bars are added to the main one. Once a child bar is complete it
//! gets removed from the rendering and the main bar advances by 1 unit.
//!
//! Run with
//!
//! ```not_rust
//! cargo run --example tokio
//! ```
//!
use futures::stream::{self, StreamExt};
use rand::{thread_rng, Rng};
use std::sync::Arc;
use tokio::time::{sleep, Duration};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

const MAX_CONCURRENT_ITEMS: usize = 5;

#[tokio::main]
async fn main() {
    // Creates a new multi-progress object.
    let multi = Arc::new(MultiProgress::new());
    let style = ProgressStyle::with_template("{bar:40.green/yellow} {pos:>7}/{len:7}").unwrap();

    // Create the main progress bar.
    let mut rng = thread_rng();
    let items = rng.gen_range(MAX_CONCURRENT_ITEMS..MAX_CONCURRENT_ITEMS * 3);
    let main = Arc::new(multi.add(ProgressBar::new(items as u64).with_style(style.clone())));
    main.tick();

    // Add the child progress bars.
    let _pbs = stream::iter(0..items)
        .map(|_i| add_bar(main.clone(), multi.clone()))
        .buffer_unordered(MAX_CONCURRENT_ITEMS)
        .collect::<Vec<_>>()
        .await;
    main.finish_with_message("done");
}

async fn add_bar(main: Arc<ProgressBar>, multi: Arc<MultiProgress>) {
    // Create a child bar and add it to the main one.
    let mut rng = thread_rng();
    let length: u64 = rng.gen_range(128..1024);
    let sleep_ms: u64 = rng.gen_range(5..10);
    let style = ProgressStyle::with_template("{bar:40.cyan/blue} {pos:>7}/{len:7}").unwrap();
    let pb = multi.add(ProgressBar::new(length).with_style(style.clone()));

    // Simulate some work.
    for _ in 0..length {
        pb.inc(1);
        sleep(Duration::from_millis(sleep_ms)).await;
    }
    // Remove the bar once complete.
    pb.finish_and_clear();

    // Advance the main progress bar.
    main.inc(1);
}
