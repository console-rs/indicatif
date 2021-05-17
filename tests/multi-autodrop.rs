use indicatif::{MultiProgress, ProgressBar};
use std::thread;
use std::time::Duration;

#[test]
fn main() {
    let pb = {
        let m = MultiProgress::new();
        m.add(ProgressBar::new(10))
        // The MultiProgress is dropped here.
    };

    {
        let pb2 = pb.clone();
        for _ in 0..10 {
            pb2.inc(1);
            thread::sleep(Duration::from_millis(50));
        }
    }

    pb.set_message("Done");
    pb.finish();

    println!("Done with MultiProgress");
}
