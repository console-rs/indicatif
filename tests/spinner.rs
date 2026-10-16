use indicatif::ProgressStyle;

#[test]
#[should_panic(expected = "at least 2 tick strings required")]
fn tick_strings_rejects_empty_sequence() {
    ProgressStyle::default_spinner().tick_strings(&[]);
}

#[test]
#[should_panic(expected = "at least 2 tick strings required")]
fn tick_strings_rejects_single_string() {
    ProgressStyle::default_spinner().tick_strings(&["-"]);
}

#[test]
fn tick_strings_accepts_minimum_sequence() {
    let style = ProgressStyle::default_spinner().tick_strings(&["-", "done"]);

    assert_eq!(style.get_tick_str(0), "-");
    assert_eq!(style.get_tick_str(1), "-");
    assert_eq!(style.get_final_tick_str(), "done");
}

#[test]
fn tick_strings_cycles_without_final_string() {
    let style = ProgressStyle::default_spinner().tick_strings(&["1", "2", "3", "4"]);

    assert_eq!(style.get_tick_str(0), "1");
    assert_eq!(style.get_tick_str(1), "2");
    assert_eq!(style.get_tick_str(2), "3");
    assert_eq!(style.get_tick_str(3), "1");
    assert_eq!(style.get_final_tick_str(), "4");
}
