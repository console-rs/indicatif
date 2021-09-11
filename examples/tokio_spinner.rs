use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;
use tokio::time::{interval, sleep};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .template("{spinner:.blue} {msg}"),
    );

    let infinity = async {
        let mut intv = interval(Duration::from_millis(120));

        pb.set_message("Calculating...");
        loop {
            intv.tick().await;
            pb.tick();
        }
    };

    let long_processing = async {
        sleep(Duration::from_secs(3)).await;
    };

    tokio::select! {
        _ = infinity => {},
        _ = long_processing => {
            pb.finish_and_clear();
            pb.println("Done");
        }
    }
}
