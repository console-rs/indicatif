// This test must be in a separate file (integration test binary) to run in its own process.
// It modifies the global static `IS_AMBIGUOUS_WIDE` via `Width::set_ambiguous_wide`.
// If run in parallel with other tests in the same process, it can cause them to fail.
// Specifically, creating a `ProgressStyle` with default characters `█░` will panic
// if CJK ambiguous width is true, because they have unequal widths under CJK.

#![cfg(feature = "in_memory")]

use indicatif::{InMemoryTerm, ProgressBar, ProgressDrawTarget, ProgressStyle, Width};
use pretty_assertions::assert_eq;

#[test]
#[cfg(feature = "unicode-width")]
fn width_cjk() {
    let message = "█████"; // 5 ambiguous characters (block).

    let in_mem = InMemoryTerm::new(10, 20);
    let pb = ProgressBar::with_draw_target(
        Some(10),
        ProgressDrawTarget::term_like(Box::new(in_mem.clone())),
    )
    .with_style(ProgressStyle::with_template("{msg:>10}").unwrap());
    pb.set_message(message);

    // Default (false) -> width 5, padded by 5 spaces
    pb.tick();
    assert_eq!(in_mem.contents(), "     █████");

    // Set to true -> width 10, padded by 0 spaces
    Width::set_default_cjk(true);
    pb.tick();
    assert_eq!(in_mem.contents(), "█████");

    // Reset
    Width::set_default_cjk(false);
}
