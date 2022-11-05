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
fn progress_bar_percent_with_no_length() {
    let in_mem = InMemoryTerm::new(10, 80);
    let pb = ProgressBar::with_draw_target(
        None,
        ProgressDrawTarget::term_like(Box::new(in_mem.clone())),
    )
    .with_style(ProgressStyle::with_template("{wide_bar} {percent}%").unwrap());

    assert_eq!(in_mem.contents(), String::new());

    pb.tick();

    assert_eq!(
        in_mem.contents(),
        "░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0%"
    );

    pb.set_length(10);

    pb.inc(1);
    assert_eq!(
        in_mem.contents(),
        "███████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 10%"
    );

    pb.finish();
    assert_eq!(
        in_mem.contents(),
        "███████████████████████████████████████████████████████████████████████████ 100%"
    );
}

#[test]
fn multi_progress_single_bar_and_leave() {
    let in_mem = InMemoryTerm::new(10, 80);
    let mp =
        MultiProgress::with_draw_target(ProgressDrawTarget::term_like(Box::new(in_mem.clone())));

    let pb1 = mp.add(ProgressBar::new(10).with_finish(ProgressFinish::AndLeave));

    assert_eq!(in_mem.contents(), String::new());

    pb1.tick();
    assert_eq!(
        in_mem.contents(),
        r#"░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/10"#
    );

    drop(pb1);
    assert_eq!(
        in_mem.contents(),
        r#"██████████████████████████████████████████████████████████████████████████ 10/10"#
    );
}

#[test]
fn multi_progress_single_bar_and_clear() {
    let in_mem = InMemoryTerm::new(10, 80);
    let mp =
        MultiProgress::with_draw_target(ProgressDrawTarget::term_like(Box::new(in_mem.clone())));

    let pb1 = mp.add(ProgressBar::new(10));

    assert_eq!(in_mem.contents(), String::new());

    pb1.tick();
    assert_eq!(
        in_mem.contents(),
        r#"░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/10"#
    );

    drop(pb1);
    assert_eq!(in_mem.contents(), "");
}
#[test]
fn multi_progress_two_bars() {
    let in_mem = InMemoryTerm::new(10, 80);
    let mp =
        MultiProgress::with_draw_target(ProgressDrawTarget::term_like(Box::new(in_mem.clone())));

    let pb1 = mp.add(ProgressBar::new(10).with_finish(ProgressFinish::AndLeave));
    let pb2 = mp.add(ProgressBar::new(5));

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

    drop(pb1);
    assert_eq!(
        in_mem.contents(),
        r#"
██████████████████████████████████████████████████████████████████████████ 10/10
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/5"#
            .trim_start()
    );

    drop(pb2);

    assert_eq!(
        in_mem.contents(),
        r#"██████████████████████████████████████████████████████████████████████████ 10/10"#
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
    assert_eq!(
        in_mem.contents(),
        r#"
██████████████████████████████████████████████████████████████████████████ 10/10
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/5
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/100"#
            .trim_start()
    );

    drop(pb2);
    assert_eq!(
        in_mem.contents(),
        r#"
██████████████████████████████████████████████████████████████████████████ 10/10
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/100"#
            .trim_start()
    );

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
        new_spinner.enable_steady_tick(Duration::from_millis(100));
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

#[test]
fn multi_progress_prune_zombies() {
    let in_mem = InMemoryTerm::new(10, 80);
    let mp =
        MultiProgress::with_draw_target(ProgressDrawTarget::term_like(Box::new(in_mem.clone())));

    let pb0 = mp
        .add(ProgressBar::new(10))
        .with_finish(ProgressFinish::AndLeave);
    let pb1 = mp.add(ProgressBar::new(15));
    pb0.tick();
    assert_eq!(
        in_mem.contents(),
        "░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/10"
    );

    pb0.inc(1);
    assert_eq!(
        in_mem.contents(),
        "███████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 1/10"
    );

    drop(pb0);

    // Clear the screen
    mp.clear().unwrap();

    // Write a line that we expect to remain. This helps ensure the adjustment to last_line_count is
    // working as expected, and `MultiState` isn't erasing lines when it shouldn't.
    in_mem.write_line("don't erase me plz").unwrap();

    // pb0 is dead, so only pb1 should be drawn from now on
    pb1.tick();
    assert_eq!(
        in_mem.contents(),
        "don't erase me plz\n░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/15"
    );
}

#[test]
fn multi_progress_prune_zombies_2() {
    let in_mem = InMemoryTerm::new(10, 80);
    let mp =
        MultiProgress::with_draw_target(ProgressDrawTarget::term_like(Box::new(in_mem.clone())));

    let pb1 = mp.add(ProgressBar::new(10).with_finish(ProgressFinish::AndLeave));
    let pb2 = mp.add(ProgressBar::new(5));
    let pb3 = mp
        .add(ProgressBar::new(100))
        .with_finish(ProgressFinish::Abandon);
    let pb4 = mp
        .add(ProgressBar::new(500))
        .with_finish(ProgressFinish::AndLeave);
    let pb5 = mp.add(ProgressBar::new(7));

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
        r#"
██████████████████████████████████████████████████████████████████████████ 10/10
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/100"#
            .trim_start()
    );

    mp.clear().unwrap();

    assert_eq!(in_mem.contents(), "");

    // A sacrificial line we expect shouldn't be touched
    in_mem.write_line("don't erase plz").unwrap();

    mp.println("Test friend :)").unwrap();
    assert_eq!(
        in_mem.contents(),
        r#"
don't erase plz
Test friend :)"#
            .trim_start()
    );

    pb4.tick();
    assert_eq!(
        in_mem.contents(),
        r#"
don't erase plz
Test friend :)
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/500"#
            .trim_start()
    );

    drop(pb4);
    assert_eq!(
        in_mem.contents(),
        r#"
don't erase plz
Test friend :)
████████████████████████████████████████████████████████████████████████ 500/500"#
            .trim_start()
    );

    mp.clear().unwrap();
    assert_eq!(in_mem.contents(), "don't erase plz\nTest friend :)");

    pb5.tick();
    assert_eq!(
        in_mem.contents(),
        r#"
don't erase plz
Test friend :)
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/7"#
            .trim_start()
    );

    mp.println("not your friend, buddy").unwrap();
    assert_eq!(
        in_mem.contents(),
        r#"
don't erase plz
Test friend :)
not your friend, buddy
░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 0/7"#
            .trim_start()
    );

    pb5.inc(1);
    assert_eq!(
        in_mem.contents(),
        r#"
don't erase plz
Test friend :)
not your friend, buddy
██████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 1/7"#
            .trim_start()
    );

    mp.clear().unwrap();
    in_mem.write_line("don't erase me either").unwrap();

    pb5.inc(1);
    assert_eq!(
        in_mem.contents(),
        r#"
don't erase plz
Test friend :)
not your friend, buddy
don't erase me either
█████████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░ 2/7"#
            .trim_start()
    );

    drop(pb5);

    assert_eq!(
        in_mem.contents(),
        r#"
don't erase plz
Test friend :)
not your friend, buddy
don't erase me either"#
            .trim_start()
    );
}

#[test]
fn basic_tab_expansion() {
    let in_mem = InMemoryTerm::new(10, 80);
    let mp =
        MultiProgress::with_draw_target(ProgressDrawTarget::term_like(Box::new(in_mem.clone())));

    let mut spinner = mp.add(ProgressBar::new_spinner().with_message("Test\t:)"));
    spinner.tick();

    // 8 is the default number of spaces
    assert_eq!(in_mem.contents(), "⠁ Test        :)");

    spinner.set_tab_width(4);
    assert_eq!(in_mem.contents(), "⠁ Test    :)");
}

#[test]
fn tab_expansion_in_template() {
    let in_mem = InMemoryTerm::new(10, 80);
    let mp =
        MultiProgress::with_draw_target(ProgressDrawTarget::term_like(Box::new(in_mem.clone())));

    let mut spinner = mp.add(
        ProgressBar::new_spinner()
            .with_message("Test\t:)")
            .with_prefix("Pre\tfix!")
            .with_style(ProgressStyle::with_template("{spinner}{prefix}\t{msg}").unwrap()),
    );

    spinner.tick();
    assert_eq!(in_mem.contents(), "⠁Pre        fix!        Test        :)");

    spinner.set_tab_width(4);
    assert_eq!(in_mem.contents(), "⠁Pre    fix!    Test    :)");

    spinner.set_tab_width(2);
    assert_eq!(in_mem.contents(), "⠁Pre  fix!  Test  :)");
}

#[test]
fn progress_style_tab_width_unification() {
    let in_mem = InMemoryTerm::new(10, 80);
    let mp =
        MultiProgress::with_draw_target(ProgressDrawTarget::term_like(Box::new(in_mem.clone())));

    // Style will have default of 8 spaces for tabs
    let style = ProgressStyle::with_template("{msg}\t{msg}").unwrap();

    let spinner = mp.add(
        ProgressBar::new_spinner()
            .with_message("OK")
            .with_tab_width(4),
    );

    // Setting the spinner's style to |style| should override the style's tab width with that of bar
    spinner.set_style(style);
    spinner.tick();
    assert_eq!(in_mem.contents(), "OK    OK");
}

#[test]
fn multi_progress_clear_println() {
    let in_mem = InMemoryTerm::new(10, 80);
    let mp =
        MultiProgress::with_draw_target(ProgressDrawTarget::term_like(Box::new(in_mem.clone())));

    mp.println("Test of println").unwrap();
    // Should have no effect
    mp.clear().unwrap();
    assert_eq!(in_mem.contents(), "Test of println");
}

#[test]
fn multi_progress_clear_zombies_no_ticks() {
    _multi_progress_clear_zombies(0);
}

#[test]
fn multi_progress_clear_zombies_one_tick() {
    _multi_progress_clear_zombies(1);
}

#[test]
fn multi_progress_clear_zombies_two_ticks() {
    _multi_progress_clear_zombies(2);
}

// In the old (broken) implementation, zombie handling sometimes worked differently depending on
// how many draws were between certain operations. Let's make sure that doesn't happen again.
fn _multi_progress_clear_zombies(ticks: usize) {
    let in_mem = InMemoryTerm::new(10, 80);
    let mp =
        MultiProgress::with_draw_target(ProgressDrawTarget::term_like(Box::new(in_mem.clone())));
    let style = ProgressStyle::with_template("{msg}").unwrap();

    let pb1 = mp.add(
        ProgressBar::new_spinner()
            .with_style(style.clone())
            .with_message("pb1"),
    );
    pb1.tick();

    let pb2 = mp.add(
        ProgressBar::new_spinner()
            .with_style(style)
            .with_message("pb2"),
    );

    pb2.tick();
    assert_eq!(in_mem.contents(), "pb1\npb2");

    pb1.finish_with_message("pb1 done");
    drop(pb1);
    assert_eq!(in_mem.contents(), "pb1 done\npb2");

    for _ in 0..ticks {
        pb2.tick();
    }

    mp.clear().unwrap();
    assert_eq!(in_mem.contents(), "");
}

// This test reproduces examples/multi.rs in a simpler form
#[test]
fn multi_zombie_handling() {
    let in_mem = InMemoryTerm::new(10, 80);
    let mp =
        MultiProgress::with_draw_target(ProgressDrawTarget::term_like(Box::new(in_mem.clone())));
    let style = ProgressStyle::with_template("{msg}").unwrap();

    let pb1 = mp.add(
        ProgressBar::new_spinner()
            .with_style(style.clone())
            .with_message("pb1"),
    );
    pb1.tick();
    let pb2 = mp.add(
        ProgressBar::new_spinner()
            .with_style(style.clone())
            .with_message("pb2"),
    );
    pb2.tick();
    let pb3 = mp.add(
        ProgressBar::new_spinner()
            .with_style(style)
            .with_message("pb3"),
    );
    pb3.tick();

    mp.println("pb1 done!").unwrap();
    pb1.finish_with_message("done");
    assert_eq!(in_mem.contents(), "pb1 done!\ndone\npb2\npb3");
    drop(pb1);

    assert_eq!(in_mem.contents(), "pb1 done!\ndone\npb2\npb3");

    pb2.tick();
    assert_eq!(in_mem.contents(), "pb1 done!\ndone\npb2\npb3");
    pb3.tick();
    assert_eq!(in_mem.contents(), "pb1 done!\ndone\npb2\npb3");

    mp.println("pb3 done!").unwrap();
    assert_eq!(in_mem.contents(), "pb1 done!\npb3 done!\npb2\npb3");

    pb3.finish_with_message("done");
    drop(pb3);

    pb2.tick();

    mp.println("pb2 done!").unwrap();
    pb2.finish_with_message("done");
    drop(pb2);

    assert_eq!(
        in_mem.contents(),
        "pb1 done!\npb3 done!\npb2 done!\ndone\ndone"
    );

    mp.clear().unwrap();

    assert_eq!(in_mem.contents(), "pb1 done!\npb3 done!\npb2 done!");
}

#[test]
fn multi_progress_multiline_msg() {
    let in_mem = InMemoryTerm::new(10, 80);
    let mp =
        MultiProgress::with_draw_target(ProgressDrawTarget::term_like(Box::new(in_mem.clone())));

    let pb1 = mp.add(ProgressBar::new_spinner().with_message("test1"));
    let pb2 = mp.add(ProgressBar::new_spinner().with_message("test2"));

    assert_eq!(in_mem.contents(), "");

    pb1.inc(1);
    pb2.inc(1);

    assert_eq!(
        in_mem.contents(),
        r#"
⠁ test1
⠁ test2
            "#
        .trim()
    );

    pb1.set_message("test1\n  test1 line2\n  test1 line3");

    assert_eq!(
        in_mem.contents(),
        r#"
⠁ test1
  test1 line2
  test1 line3
⠁ test2
            "#
        .trim()
    );

    pb1.inc(1);
    pb2.inc(1);

    assert_eq!(
        in_mem.contents(),
        r#"
⠉ test1
  test1 line2
  test1 line3
⠉ test2
            "#
        .trim()
    );

    pb2.set_message("test2\n  test2 line2");

    assert_eq!(
        in_mem.contents(),
        r#"
⠉ test1
  test1 line2
  test1 line3
⠉ test2
  test2 line2
            "#
        .trim()
    );

    pb1.set_message("single line again");

    assert_eq!(
        in_mem.contents(),
        r#"
⠉ single line again
⠉ test2
  test2 line2
            "#
        .trim()
    );

    pb1.finish_with_message("test1 done!");
    pb2.finish_with_message("test2 done!");

    assert_eq!(
        in_mem.contents(),
        r#"  test1 done!
  test2 done!"#
    );
}
