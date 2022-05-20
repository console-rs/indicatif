#![cfg(feature = "in_memory")]

use std::time::Duration;

use indicatif::{
    InMemoryTerm, MultiProgress, ProgressBar, ProgressDrawTarget, ProgressFinish, ProgressStyle,
    TermLike,
};

#[test]
fn basic_progress_bar() {
    let in_mem = InMemoryTerm::new(10, 80);
    let pb = ProgressBar::with_draw_target(
        Some(10),
        ProgressDrawTarget::term_like(Box::new(in_mem.clone())),
    );

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
fn progress_bar_builder_method_order() {
    let in_mem = InMemoryTerm::new(10, 80);
    // Test that `with_style` doesn't overwrite the message or prefix
    let pb = ProgressBar::with_draw_target(
        Some(10),
        ProgressDrawTarget::term_like(Box::new(in_mem.clone())),
    )
    .with_message("crate")
    .with_prefix("Downloading")
    .with_style(
        ProgressStyle::with_template("{prefix:>12.cyan.bold} {msg}: {wide_bar} {pos}/{len}")
            .unwrap(),
    );

    assert_eq!(in_mem.contents(), String::new());

    pb.tick();
    assert_eq!(
        in_mem.contents(),
        " Downloading crate: ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/10"
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

#[test]
fn multi_progress_println() {
    let in_mem = InMemoryTerm::new(10, 80);
    let mp =
        MultiProgress::with_draw_target(ProgressDrawTarget::term_like(Box::new(in_mem.clone())));

    let pb1 = mp.add(ProgressBar::new(10));
    let pb2 = mp.add(ProgressBar::new(5));
    let pb3 = mp.add(ProgressBar::new(100));

    assert_eq!(in_mem.contents(), "");

    pb1.inc(2);
    mp.println("message printed :)").unwrap();

    assert_eq!(
        in_mem.contents(),
        r#"
message printed :)
███████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 2/10
            "#
        .trim()
    );

    mp.println("another great message!").unwrap();
    assert_eq!(
        in_mem.contents(),
        r#"
message printed :)
another great message!
███████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 2/10
            "#
        .trim()
    );

    pb2.inc(1);
    pb3.tick();
    mp.println("one last message").unwrap();

    assert_eq!(
        in_mem.contents(),
        r#"
message printed :)
another great message!
one last message
███████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 2/10
███████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 1/5
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/100
        "#
        .trim()
    );

    drop(pb1);
    drop(pb2);
    drop(pb3);

    assert_eq!(
        in_mem.contents(),
        r#"
message printed :)
another great message!
one last message"#
            .trim()
    );
}

#[test]
fn multi_progress_suspend() {
    let in_mem = InMemoryTerm::new(10, 80);
    let mp =
        MultiProgress::with_draw_target(ProgressDrawTarget::term_like(Box::new(in_mem.clone())));

    let pb1 = mp.add(ProgressBar::new(10));
    let pb2 = mp.add(ProgressBar::new(10));

    assert_eq!(in_mem.contents(), "");

    pb1.inc(2);
    mp.println("message printed :)").unwrap();

    assert_eq!(
        in_mem.contents(),
        r#"
message printed :)
███████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 2/10
            "#
        .trim()
    );

    mp.suspend(|| {
        in_mem.write_line("This is write_line output!").unwrap();
        in_mem.write_line("And so is this").unwrap();
        in_mem.move_cursor_down(1).unwrap();
    });

    assert_eq!(
        in_mem.contents(),
        r#"
message printed :)
This is write_line output!
And so is this

███████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 2/10
            "#
        .trim()
    );

    pb2.inc(1);
    mp.println("Another line printed").unwrap();

    assert_eq!(
        in_mem.contents(),
        r#"
message printed :)
This is write_line output!
And so is this

Another line printed
███████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 2/10
███████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 1/10
            "#
        .trim()
    );

    drop(pb1);
    drop(pb2);

    assert_eq!(
        in_mem.contents(),
        r#"
message printed :)
This is write_line output!
And so is this

Another line printed"#
            .trim()
    );
}

#[test]
fn ticker_drop() {
    let in_mem = InMemoryTerm::new(10, 80);
    let mp =
        MultiProgress::with_draw_target(ProgressDrawTarget::term_like(Box::new(in_mem.clone())));

    let mut spinner: Option<ProgressBar> = None;

    for i in 0..5 {
        let new_spinner = mp.add(
            ProgressBar::new_spinner()
                .with_finish(ProgressFinish::AndLeave)
                .with_message(format!("doing stuff {}", i)),
        );
        new_spinner.enable_steady_tick(Duration::from_millis(50));
        spinner.replace(new_spinner);
    }

    drop(spinner);
    assert_eq!(
        in_mem.contents(),
        "  doing stuff 0\n  doing stuff 1\n  doing stuff 2\n  doing stuff 3\n  doing stuff 4"
    );
}

#[test]
fn manually_inc_ticker() {
    let in_mem = InMemoryTerm::new(10, 80);
    let mp =
        MultiProgress::with_draw_target(ProgressDrawTarget::term_like(Box::new(in_mem.clone())));

    let spinner = mp.add(ProgressBar::new_spinner().with_message("msg"));

    assert_eq!(in_mem.contents(), "");

    spinner.inc(1);
    assert_eq!(in_mem.contents(), "⠁ msg");

    spinner.inc(1);
    assert_eq!(in_mem.contents(), "⠉ msg");

    // set_message / set_prefix shouldn't increase tick
    spinner.set_message("new message");
    spinner.set_prefix("prefix");
    assert_eq!(in_mem.contents(), "⠉ new message");
}
