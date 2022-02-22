use std::thread;
use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rand::{thread_rng, Rng};

fn main() {
    let styles = [
        ("Rough bar:", "█  ", "red"),
        ("Fine bar: ", "█▉▊▋▌▍▎▏  ", "yellow"),
        ("Vertical: ", "█▇▆▅▄▃▂▁  ", "green"),
        ("Fade in:  ", "█▓▒░  ", "blue"),
        ("Blocky:   ", "█▛▌▖  ", "magenta"),
    ];

    let m = MultiProgress::new();

    let handles: Vec<_> = styles
        .iter()
        .map(|s| {
            let pb = m.add(ProgressBar::new(512));
            pb.set_style(
                ProgressStyle::default_bar()
                    .template(&format!("{{prefix:.bold}}▕{{bar:.{}}}▏{{msg}}", s.2))
                    .unwrap()
                    .progress_chars(s.1),
            );
            pb.set_prefix(s.0);
            let wait = Duration::from_millis(thread_rng().gen_range(10..30));
            thread::spawn(move || {
                for i in 0..512 {
                    pb.inc(1);
                    pb.set_message(format!("{:3}%", 100 * i / 512));
                    thread::sleep(wait);
                }
                pb.finish_with_message("100%");
            })
        })
        .collect();

    for h in handles {
        let _ = h.join();
    }
}
