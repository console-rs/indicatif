#![cfg(feature = "in_memory")]

use indicatif::{InMemoryTerm, MultiProgress, ProgressBar, ProgressDrawTarget, ProgressFinish};

#[test]
fn basic_progress_bar() {
    let in_mem = InMemoryTerm::new(10, 80);
    let pb =
        ProgressBar::with_draw_target(10, ProgressDrawTarget::term_like(Box::new(in_mem.clone())));

    assert_eq!(in_mem.contents(), String::new());

    pb.tick();
    assert_eq!(
        in_mem.contents(),
        "░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/10"
    );

    pb.inc(1);
    assert_eq!(
        in_mem.contents(),
        "███████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 1/10"
    );

    pb.finish();
    assert_eq!(
        in_mem.contents(),
        "██████████████████████████████████████████████████████████████████████████ 10/10"
    );
}

#[test]
fn multi_progress() {
    let in_mem = InMemoryTerm::new(10, 80);
    let mp =
        MultiProgress::with_draw_target(ProgressDrawTarget::term_like(Box::new(in_mem.clone())));

    let pb1 = mp.add(ProgressBar::new(10).with_finish(ProgressFinish::AndLeave));
    let pb2 = mp.add(ProgressBar::new(5));
    let pb3 = mp.add(ProgressBar::new(100));

    assert_eq!(in_mem.contents(), String::new());

    pb1.tick();
    assert_eq!(
        in_mem.contents(),
        r#"░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/10"#
    );

    pb2.tick();

    assert_eq!(
        in_mem.contents(),
        r#"
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/10
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/5"#
            .trim_start()
    );

    pb3.tick();
    assert_eq!(
        in_mem.contents(),
        r#"
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/10
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/5
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/100"#
            .trim_start()
    );

    drop(pb1);
    drop(pb2);
    drop(pb3);

    assert_eq!(
        in_mem.contents(),
        r#"██████████████████████████████████████████████████████████████████████████ 10/10"#
    );
}
