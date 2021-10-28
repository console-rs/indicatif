use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{self, Write};

use console::{measure_text_width, Style};
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
#[cfg(feature = "improved_unicode")]
use unicode_segmentation::UnicodeSegmentation;

use crate::format::{BinaryBytes, DecimalBytes, FormattedDuration, HumanBytes, HumanDuration};
use crate::state::ProgressState;

/// Controls the rendering style of progress bars
#[derive(Clone)]
pub struct ProgressStyle {
    tick_strings: Vec<Box<str>>,
    progress_chars: Vec<Box<str>>,
    template: Box<str>,
    on_finish: ProgressFinish,
    // how unicode-big each char in progress_chars is
    char_width: usize,
    format_map: FormatMap,
}

#[cfg(feature = "improved_unicode")]
fn segment(s: &str) -> Vec<Box<str>> {
    UnicodeSegmentation::graphemes(s, true)
        .map(|s| s.into())
        .collect()
}

#[cfg(not(feature = "improved_unicode"))]
fn segment(s: &str) -> Vec<Box<str>> {
    s.chars().map(|x| x.to_string().into()).collect()
}

#[cfg(feature = "improved_unicode")]
fn measure(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

#[cfg(not(feature = "improved_unicode"))]
fn measure(s: &str) -> usize {
    s.chars().count()
}

/// finds the unicode-aware width of the passed grapheme cluters
/// panics on an empty parameter, or if the characters are not equal-width
fn width(c: &[Box<str>]) -> usize {
    c.iter()
        .map(|s| measure(s.as_ref()))
        .fold(None, |acc, new| {
            match acc {
                None => return Some(new),
                Some(old) => assert_eq!(old, new, "got passed un-equal width progress characters"),
            }
            acc
        })
        .unwrap()
}

impl ProgressStyle {
    /// Returns the default progress bar style for bars
    pub fn default_bar() -> ProgressStyle {
        let progress_chars = segment("█░");
        let char_width = width(&progress_chars);
        ProgressStyle {
            tick_strings: "⠁⠁⠉⠙⠚⠒⠂⠂⠒⠲⠴⠤⠄⠄⠤⠠⠠⠤⠦⠖⠒⠐⠐⠒⠓⠋⠉⠈⠈ "
                .chars()
                .map(|c| c.to_string().into())
                .collect(),
            progress_chars,
            char_width,
            template: "{wide_bar} {pos}/{len}".into(),
            on_finish: ProgressFinish::default(),
            format_map: FormatMap::default(),
        }
    }

    /// Returns the default progress bar style for spinners
    pub fn default_spinner() -> ProgressStyle {
        let progress_chars = segment("█░");
        let char_width = width(&progress_chars);
        ProgressStyle {
            tick_strings: "⠁⠁⠉⠙⠚⠒⠂⠂⠒⠲⠴⠤⠄⠄⠤⠠⠠⠤⠦⠖⠒⠐⠐⠒⠓⠋⠉⠈⠈ "
                .chars()
                .map(|c| c.to_string().into())
                .collect(),
            progress_chars,
            char_width,
            template: "{spinner} {msg}".into(),
            on_finish: ProgressFinish::default(),
            format_map: FormatMap::default(),
        }
    }

    /// Sets the tick character sequence for spinners
    pub fn tick_chars(mut self, s: &str) -> ProgressStyle {
        self.tick_strings = s.chars().map(|c| c.to_string().into()).collect();
        // Format bar will panic with some potentially confusing message, better to panic here
        // with a message explicitly informing of the problem
        assert!(
            self.tick_strings.len() >= 2,
            "at least 2 tick chars required"
        );
        self
    }

    /// Sets the tick string sequence for spinners
    pub fn tick_strings(mut self, s: &[&str]) -> ProgressStyle {
        self.tick_strings = s.iter().map(|s| s.to_string().into()).collect();
        // Format bar will panic with some potentially confusing message, better to panic here
        // with a message explicitly informing of the problem
        assert!(
            self.progress_chars.len() >= 2,
            "at least 2 tick strings required"
        );
        self
    }

    /// Sets the progress characters `(filled, current, to do)`
    ///
    /// You can pass more than three for a more detailed display.
    /// All passed grapheme clusters need to be of equal width.
    pub fn progress_chars(mut self, s: &str) -> ProgressStyle {
        self.progress_chars = segment(s);
        // Format bar will panic with some potentially confusing message, better to panic here
        // with a message explicitly informing of the problem
        assert!(
            self.progress_chars.len() >= 2,
            "at least 2 progress chars required"
        );
        self.char_width = width(&self.progress_chars);
        self
    }

    /// Adds a custom key that references a `&ProgressState` to the template
    pub fn with_key(mut self, key: &'static str, f: Format) -> ProgressStyle {
        self.format_map.0.insert(key, f);
        self
    }

    /// Sets the template string for the progress bar
    ///
    /// Review the [list of template keys](./index.html#templates) for more information.
    pub fn template(mut self, s: &str) -> ProgressStyle {
        self.template = s.into();
        self
    }

    /// Sets the finish behavior for the progress bar
    ///
    /// This behavior is invoked when [`ProgressBar`] or
    /// [`ProgressBarIter`] completes and
    /// [`ProgressBar::is_finished()`] is false.
    /// If you don't want the progress bar to be automatically finished then
    /// call `on_finish(None)`.
    ///
    /// [`ProgressBar`]: crate::ProgressBar
    /// [`ProgressBarIter`]: crate::ProgressBarIter
    /// [`ProgressBar::is_finished()`]: crate::ProgressBar::is_finished
    pub fn on_finish(mut self, finish: ProgressFinish) -> ProgressStyle {
        self.on_finish = finish;
        self
    }

    /// Returns the tick char for a given number
    #[deprecated(since = "0.13.0", note = "Deprecated in favor of get_tick_str")]
    pub fn get_tick_char(&self, idx: u64) -> char {
        self.get_tick_str(idx).chars().next().unwrap_or(' ')
    }

    /// Returns the tick string for a given number
    pub fn get_tick_str(&self, idx: u64) -> &str {
        &self.tick_strings[(idx as usize) % (self.tick_strings.len() - 1)]
    }

    /// Returns the tick char for the finished state
    #[deprecated(since = "0.13.0", note = "Deprecated in favor of get_final_tick_str")]
    pub fn get_final_tick_char(&self) -> char {
        self.get_final_tick_str().chars().next().unwrap_or(' ')
    }

    /// Returns the tick string for the finished state
    pub fn get_final_tick_str(&self) -> &str {
        &self.tick_strings[self.tick_strings.len() - 1]
    }

    /// Returns the finish behavior
    pub fn get_on_finish(&self) -> &ProgressFinish {
        &self.on_finish
    }

    pub(crate) fn format_bar(&self, fract: f32, width: usize, alt_style: Option<&Style>) -> String {
        // The number of clusters from progress_chars to write (rounding down).
        let width = width / self.char_width;
        // The number of full clusters (including a fractional component for a partially-full one).
        let fill = fract * width as f32;
        // The number of entirely full clusters (by truncating `fill`).
        let entirely_filled = fill as usize;
        // 1 if the bar is not entirely empty or full (meaning we need to draw the "current"
        // character between the filled and "to do" segment), 0 otherwise.
        let head = if fill > 0.0 && entirely_filled < width {
            1
        } else {
            0
        };

        let pb = self.progress_chars[0].repeat(entirely_filled);

        let cur = if head == 1 {
            // Number of fine-grained progress entries in progress_chars.
            let n = self.progress_chars.len().saturating_sub(2);
            let cur_char = if n <= 1 {
                // No fine-grained entries. 1 is the single "current" entry if we have one, the "to
                // do" entry if not.
                1
            } else {
                // Pick a fine-grained entry, ranging from the last one (n) if the fractional part
                // of fill is 0 to the first one (1) if the fractional part of fill is almost 1.
                n.saturating_sub((fill.fract() * n as f32) as usize)
            };
            self.progress_chars[cur_char].to_string()
        } else {
            "".into()
        };

        // Number of entirely empty clusters needed to fill the bar up to `width`.
        let bg = width.saturating_sub(entirely_filled).saturating_sub(head);
        let rest = self.progress_chars[self.progress_chars.len() - 1].repeat(bg);
        format!(
            "{}{}{}",
            pb,
            cur,
            alt_style.unwrap_or(&Style::new()).apply_to(rest)
        )
    }

    pub(crate) fn format_state(&self, state: &ProgressState) -> Vec<String> {
        static VAR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\}\})|\{(\{|[^{}}]+\})").unwrap());
        static KEY_RE: Lazy<Regex> = Lazy::new(|| {
            Regex::new(
                r"(?x)
                    ([^:]+)
                    (?:
                        :
                        ([<^>])?
                        ([0-9]+)?
                        (!)?
                        (?:\.([0-9a-z_]+(?:\.[0-9a-z_]+)*))?
                        (?:/([a-z_]+(?:\.[a-z_]+)*))?
                    )?
                ",
            )
            .unwrap()
        });

        let mut rv = vec![];
        for line in self.template.lines() {
            let mut wide_element = None;
            let s = VAR_RE.replace_all(line, |caps: &Captures<'_>| {
                if caps.get(1).is_some() {
                    return "}".into();
                }

                let key = &caps[2];
                if key == "{" {
                    return "{".into();
                }

                let mut var = TemplateVar {
                    key,
                    align: Alignment::Left,
                    truncate: false,
                    width: None,
                    style: None,
                    alt_style: None,
                    last_element: caps.get(0).unwrap().end() >= line.len(),
                };

                if let Some(opt_caps) = KEY_RE.captures(&key[..key.len() - 1]) {
                    if let Some(short_key) = opt_caps.get(1) {
                        var.key = short_key.as_str();
                    }
                    var.align = match opt_caps.get(2).map(|x| x.as_str()) {
                        Some("<") => Alignment::Left,
                        Some("^") => Alignment::Center,
                        Some(">") => Alignment::Right,
                        _ => Alignment::Left,
                    };
                    if let Some(width) = opt_caps.get(3) {
                        var.width = Some(width.as_str().parse().unwrap());
                    }
                    if opt_caps.get(4).is_some() {
                        var.truncate = true;
                    }
                    if let Some(style) = opt_caps.get(5) {
                        var.style = Some(Style::from_dotted_str(style.as_str()));
                    }
                    if let Some(alt_style) = opt_caps.get(6) {
                        var.alt_style = Some(Style::from_dotted_str(alt_style.as_str()));
                    }
                }

                let rv = if let Some(formatter) = self.format_map.0.get(var.key) {
                    formatter(state)
                } else {
                    match var.key {
                        "wide_bar" => {
                            wide_element = Some(var.duplicate_for_key("bar"));
                            "\x00".into()
                        }
                        "bar" => self.format_bar(
                            state.fraction(),
                            var.width.unwrap_or(20),
                            var.alt_style.as_ref(),
                        ),
                        "spinner" => state.current_tick_str().to_string(),
                        "wide_msg" => {
                            wide_element = Some(var.duplicate_for_key("msg"));
                            "\x00".into()
                        }
                        "msg" => state.message().to_string(),
                        "prefix" => state.prefix().to_string(),
                        "pos" => state.pos.to_string(),
                        "len" => state.len.to_string(),
                        "percent" => format!("{:.*}", 0, state.fraction() * 100f32),
                        "bytes" => format!("{}", HumanBytes(state.pos)),
                        "total_bytes" => format!("{}", HumanBytes(state.len)),
                        "decimal_bytes" => format!("{}", DecimalBytes(state.pos)),
                        "decimal_total_bytes" => format!("{}", DecimalBytes(state.len)),
                        "binary_bytes" => format!("{}", BinaryBytes(state.pos)),
                        "binary_total_bytes" => format!("{}", BinaryBytes(state.len)),
                        "elapsed_precise" => {
                            format!("{}", FormattedDuration(state.started.elapsed()))
                        }
                        "elapsed" => format!("{:#}", HumanDuration(state.started.elapsed())),
                        "per_sec" => format!("{:.4}/s", state.per_sec()),
                        "bytes_per_sec" => format!("{}/s", HumanBytes(state.per_sec() as u64)),
                        "binary_bytes_per_sec" => {
                            format!("{}/s", BinaryBytes(state.per_sec() as u64))
                        }
                        "eta_precise" => format!("{}", FormattedDuration(state.eta())),
                        "eta" => format!("{:#}", HumanDuration(state.eta())),
                        "duration_precise" => format!("{}", FormattedDuration(state.duration())),
                        "duration" => format!("{:#}", HumanDuration(state.duration())),
                        _ => "".into(),
                    }
                };

                match var.width {
                    Some(width) => {
                        let padded = PaddedStringDisplay {
                            str: &rv,
                            width,
                            align: var.align,
                            truncate: var.truncate,
                        };
                        match var.style {
                            Some(s) => s.apply_to(padded).to_string(),
                            None => padded.to_string(),
                        }
                    }
                    None => match var.style {
                        Some(s) => s.apply_to(rv).to_string(),
                        None => rv,
                    },
                }
            });

            rv.push(if let Some(ref var) = wide_element {
                let total_width = state.width();
                if var.key == "bar" {
                    let bar_width = total_width.saturating_sub(measure_text_width(&s));
                    s.replace(
                        "\x00",
                        &self.format_bar(state.fraction(), bar_width, var.alt_style.as_ref()),
                    )
                } else if var.key == "msg" {
                    let msg_width = total_width.saturating_sub(measure_text_width(&s));
                    let msg = PaddedStringDisplay {
                        str: state.message(),
                        width: msg_width,
                        align: var.align,
                        truncate: true,
                    }
                    .to_string();
                    s.replace(
                        "\x00",
                        if var.last_element {
                            msg.trim_end()
                        } else {
                            &msg
                        },
                    )
                } else {
                    unreachable!()
                }
            } else {
                s.to_string()
            });
        }

        rv
    }
}

struct PaddedStringDisplay<'a> {
    str: &'a str,
    width: usize,
    align: Alignment,
    truncate: bool,
}

impl<'a> fmt::Display for PaddedStringDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cols = measure_text_width(self.str);
        if cols >= self.width {
            return match self.truncate {
                true => f.write_str(self.str.get(..self.width).unwrap_or(self.str)),
                false => f.write_str(self.str),
            };
        }

        let diff = self.width.saturating_sub(cols);
        let (left_pad, right_pad) = match self.align {
            Alignment::Left => (0, diff),
            Alignment::Right => (diff, 0),
            Alignment::Center => (diff / 2, diff.saturating_sub(diff / 2)),
        };

        for _ in 0..left_pad {
            f.write_char(' ')?;
        }
        f.write_str(self.str)?;
        for _ in 0..right_pad {
            f.write_char(' ')?;
        }
        Ok(())
    }
}

#[derive(Clone, Default)]
struct FormatMap(HashMap<&'static str, Format>);

pub type Format = fn(&ProgressState) -> String;

/// Behavior of a progress bar when it is finished
///
/// This is invoked when a [`ProgressBar`] or [`ProgressBarIter`] completes and
/// [`ProgressBar::is_finished`] is false.
///
/// [`ProgressBar`]: crate::ProgressBar
/// [`ProgressBarIter`]: crate::ProgressBarIter
/// [`ProgressBar::is_finished`]: crate::ProgressBar::is_finished
#[derive(Clone, Debug)]
pub enum ProgressFinish {
    /// Finishes the progress bar and leaves the current message
    ///
    /// Same behavior as calling [`ProgressBar::finish()`](crate::ProgressBar::finish).
    AndLeave,
    /// Finishes the progress bar at current position and leaves the current message
    ///
    /// Same behavior as calling [`ProgressBar::finish_at_current_pos()`](crate::ProgressBar::finish_at_current_pos).
    AtCurrentPos,
    /// Finishes the progress bar and sets a message
    ///
    /// Same behavior as calling [`ProgressBar::finish_with_message()`](crate::ProgressBar::finish_with_message).
    WithMessage(Cow<'static, str>),
    /// Finishes the progress bar and completely clears it (this is the default)
    ///
    /// Same behavior as calling [`ProgressBar::finish_and_clear()`](crate::ProgressBar::finish_and_clear).
    AndClear,
    /// Finishes the progress bar and leaves the current message and progress
    ///
    /// Same behavior as calling [`ProgressBar::abandon()`](crate::ProgressBar::abandon).
    Abandon,
    /// Finishes the progress bar and sets a message, and leaves the current progress
    ///
    /// Same behavior as calling [`ProgressBar::abandon_with_message()`](crate::ProgressBar::abandon_with_message).
    AbandonWithMessage(Cow<'static, str>),
}

impl Default for ProgressFinish {
    fn default() -> Self {
        Self::AndClear
    }
}

#[derive(Debug)]
struct TemplateVar<'a> {
    pub key: &'a str,
    pub align: Alignment,
    pub truncate: bool,
    pub width: Option<usize>,
    pub style: Option<Style>,
    pub alt_style: Option<Style>,
    pub last_element: bool,
}

impl<'a> TemplateVar<'a> {
    fn duplicate_for_key<'b>(&self, key: &'b str) -> TemplateVar<'b> {
        TemplateVar {
            key,
            align: self.align,
            truncate: self.truncate,
            width: self.width,
            style: self.style.clone(),
            alt_style: self.alt_style.clone(),
            last_element: self.last_element,
        }
    }
}

#[derive(PartialEq, Eq, Debug, Copy, Clone)]
enum Alignment {
    Left,
    Center,
    Right,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw_target::ProgressDrawTarget;
    use crate::state::ProgressState;

    #[test]
    fn test_expand_template() {
        let mut style = ProgressStyle::default_bar();
        style.format_map.0.insert("foo", |_| "FOO".into());
        style.format_map.0.insert("bar", |_| "BAR".into());
        let state = ProgressState::new(10, ProgressDrawTarget::stdout());

        style.template = "{{ {foo} {bar} }}".into();
        let rv = style.format_state(&state);
        assert_eq!(&rv[0], "{ FOO BAR }");

        style.template = r#"{ "foo": "{foo}", "bar": {bar} }"#.into();
        let rv = style.format_state(&state);
        assert_eq!(&rv[0], r#"{ "foo": "FOO", "bar": BAR }"#);
    }

    #[test]
    fn test_expand_template_flags() {
        use console::set_colors_enabled;
        set_colors_enabled(true);
        let mut style = ProgressStyle::default_bar();
        style.format_map.0.insert("foo", |_| "XXX".into());
        let state = ProgressState::new(10, ProgressDrawTarget::stdout());

        style.template = "{foo:5}".into();
        let rv = style.format_state(&state);
        assert_eq!(&rv[0], "XXX  ");

        style.template = "{foo:.red.on_blue}".into();
        let rv = style.format_state(&state);
        assert_eq!(&rv[0], "\u{1b}[31m\u{1b}[44mXXX\u{1b}[0m");

        style.template = "{foo:^5.red.on_blue}".into();
        let rv = style.format_state(&state);
        assert_eq!(&rv[0], "\u{1b}[31m\u{1b}[44m XXX \u{1b}[0m");

        style.template = "{foo:^5.red.on_blue/green.on_cyan}".into();
        let rv = style.format_state(&state);
        assert_eq!(&rv[0], "\u{1b}[31m\u{1b}[44m XXX \u{1b}[0m");
    }
}
