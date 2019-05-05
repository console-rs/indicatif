



use futures::Stream;
use indicatif::ProgressBar;
use std::time::Duration;
use tokio_core::reactor::{Core, Interval};

fn main() {
    // Plain progress bar, totaling 1024 steps.
    let steps = 1024;
    let pb = ProgressBar::new(steps);

    // Stream of events, triggering every 5ms.
    let mut tcore = Core::new().expect("failed to create core");
    let intv = Interval::new(Duration::from_millis(5), &tcore.handle())
        .expect("failed to create interval");

    // Future computation which runs for 100 interval events,
    // incrementing one step of the progress bar each time.
    let future = intv.take(steps).for_each(|_| Ok(pb.inc(1)));

    // Drive the future to completion, blocking until done.
    tcore.run(future).expect("failed to complete future");

    // Mark the progress bar as finished.
    pb.finish();
}
