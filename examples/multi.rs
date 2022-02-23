use std::thread;
use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

fn main() {
    let m = MultiProgress::new();
    let sty = ProgressStyle::with_template(
        "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
    )
    .unwrap()
    .progress_chars("##-");

    let pb = m.add(ProgressBar::new(128));
    pb.set_style(sty.clone());

    let pb2 = m.insert_after(&pb, ProgressBar::new(128));
    pb2.set_style(sty.clone());

    let pb3 = m.insert_after(&pb2, ProgressBar::new(1024));
    pb3.set_style(sty);

    m.println("starting!").unwrap();

    let m_clone = m.clone();
    let h1 = thread::spawn(move || {
        for i in 0..128 {
            pb.set_message(format!("item #{}", i + 1));
            pb.inc(1);
            thread::sleep(Duration::from_millis(15));
        }
        m_clone.println("pb1 is done!").unwrap();
        pb.finish_with_message("done");
    });

    let m_clone = m.clone();
    let h2 = thread::spawn(move || {
        for _ in 0..3 {
            pb2.set_position(0);
            for i in 0..128 {
                pb2.set_message(format!("item #{}", i + 1));
                pb2.inc(1);
                thread::sleep(Duration::from_millis(8));
            }
        }
        m_clone.println("pb2 is done!").unwrap();
        pb2.finish_with_message("done");
    });

    let m_clone = m.clone();
    let _ = thread::spawn(move || {
        for i in 0..1024 {
            pb3.set_message(format!("item #{}", i + 1));
            pb3.inc(1);
            thread::sleep(Duration::from_millis(2));
        }
        m_clone.println("pb3 is done!").unwrap();
        pb3.finish_with_message("done");
    });

    let _ = h1.join();
    let _ = h2.join();
    m.clear().unwrap();
}
