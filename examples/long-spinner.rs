use std::thread;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle, TICKER_BARRIER};

fn main() {
    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(120));
    pb.set_style(
        ProgressStyle::with_template("{spinner:.blue} {msg}")
            .unwrap()
            // For more spinners check out the cli-spinners project:
            // https://github.com/sindresorhus/cli-spinners/blob/master/spinners.json
            .tick_strings(&[
                "▹▹▹▹▹",
                "▸▹▹▹▹",
                "▹▸▹▹▹",
                "▹▹▸▹▹",
                "▹▹▹▸▹",
                "▹▹▹▹▸",
                "▪▪▪▪▪",
            ]),
    );
    pb.set_message("Calculating...");

    // Wait long enough for the `Ticker` to make it inside the loop and to the first barrier.

    // Note: if you uncomment this sleep, the program will deadlock because the drop(pb)
    // below will cause the ticker loop to never run, so a call to TICKER_BARRIER.wait()
    // will never be made in Ticker.
    thread::sleep(Duration::from_millis(200));

    drop(pb);

    TICKER_BARRIER.wait();
}
