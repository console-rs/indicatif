use indicatif::{MultiProgress, ProgressBar, ProgressFinish, ProgressStyle};
use std::sync::Arc;
use std::time::Duration;

struct Item {
    name: &'static str,
    steps: Vec<&'static str>,
}

fn main() {
    let mp = Arc::new(MultiProgress::new());
    let item_spinner_style = ProgressStyle::default_spinner().on_finish(ProgressFinish::AndLeave);

    let sty_main = ProgressStyle::default_bar().on_finish(ProgressFinish::AndLeave);
    let step_progress_style = ProgressStyle::default_bar()
        .template("{msg}: {bar:40.green/yellow} {pos:>4}/{len:4}")
        .on_finish(ProgressFinish::AndLeave);

    let items_to_process = vec![
        Item {
            name: "apples",
            steps: vec!["removing stem", "washing", "peeling", "coring", "slicing"],
        },
        Item {
            name: "pears",
            steps: vec!["removing stem", "washing", "coring", "dicing"],
        },
        Item {
            name: "oranges",
            steps: vec!["peeling", "segmenting", "removing pith"],
        },
        Item {
            name: "cherries",
            steps: vec!["washing", "pitting", "removing stems"],
        },
        Item {
            name: "bananas",
            steps: vec!["peeling", "slicing"],
        },
    ];

    let overall_progress =
        mp.add(ProgressBar::new(items_to_process.len() as u64).with_style(sty_main));
    overall_progress.tick();

    for item in items_to_process {
        let item_spinner = mp.insert_before(
            &overall_progress,
            ProgressBar::new_spinner()
                .with_message(item.name)
                .with_style(item_spinner_style.clone()),
        );
        item_spinner.enable_steady_tick(100);

        std::thread::sleep(Duration::from_secs(3));

        let mut next_step_target = item_spinner.clone();

        for step in item.steps {
            let step_progress = mp.insert_after(
                &next_step_target,
                ProgressBar::new(10)
                    .with_message(step)
                    .with_style(step_progress_style.clone()),
            );
            next_step_target = step_progress.clone();
            for _ in 0..9 {
                step_progress.inc(1);
                std::thread::sleep(Duration::from_millis(100));
            }
        }

        overall_progress.inc(1);
    }
}
