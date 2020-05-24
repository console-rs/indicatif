use std::borrow::Cow;
use std::cell::RefCell;
use std::iter::repeat;

use console::{measure_text_width, Style};

use crate::format::{BinaryBytes, DecimalBytes, FormattedDuration, HumanBytes, HumanDuration};
use crate::progress::ProgressState;
use crate::utils::{expand_template, pad_str};

use unicode_segmentation::UnicodeSegmentation;

/// Controls the rendering style of progress bars.
#[derive(Clone, Debug)]
pub struct ProgressStyle {
    tick_strings: Vec<Box<str>>,
    progress_chars: Vec<Box<str>>,
    template: Cow<'static, str>,
    // how unicode-big each char in progress_chars is
    char_width: usize,
}

fn segment(s: &str) -> Vec<Box<str>> {
    UnicodeSegmentation::graphemes(s, true)
        .map(|s| s.into())
        .collect()
}

/// finds the unicode-aware width of the passed grapheme cluters
/// panics on an empty parameter, or if the characters are not equal-width
fn width(c: &[Box<str>]) -> usize {
    c.iter()
        .map(|s| unicode_width::UnicodeWidthStr::width(s.as_ref()))
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
    /// Returns the default progress bar style for bars.
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
            template: Cow::Borrowed("{wide_bar} {pos}/{len}"),
        }
    }

    /// Returns the default progress bar style for spinners.
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
            template: Cow::Borrowed("{spinner} {msg}"),
        }
    }

    /// Sets the tick character sequence for spinners.
    pub fn tick_chars(mut self, s: &str) -> ProgressStyle {
        self.tick_strings = s.chars().map(|c| c.to_string().into()).collect();
        self
    }

    /// Sets the tick string sequence for spinners.
    pub fn tick_strings(mut self, s: &[&str]) -> ProgressStyle {
        self.tick_strings = s.iter().map(|s| s.to_string().into()).collect();
        self
    }

    /// Sets the progress characters `(filled, current, to do)`.
    /// You can pass more then three for a more detailed display.
    /// All passed grapheme clusters need to be of equal width.
    pub fn progress_chars(mut self, s: &str) -> ProgressStyle {
        self.progress_chars = segment(s);
        self.char_width = width(&self.progress_chars);
        self
    }

    /// Sets the template string for the progress bar.
    pub fn template(mut self, s: &str) -> ProgressStyle {
        self.template = Cow::Owned(s.into());
        self
    }

    /// Returns the tick char for a given number.
    #[deprecated(since = "0.13.0", note = "Deprecated in favor of get_tick_str")]
    pub fn get_tick_char(&self, idx: u64) -> char {
        self.get_tick_str(idx).chars().next().unwrap_or(' ')
    }

    /// Returns the tick string for a given number.
    pub fn get_tick_str(&self, idx: u64) -> &str {
        &self.tick_strings[(idx as usize) % (self.tick_strings.len() - 1)]
    }

    /// Returns the tick char for the finished state.
    #[deprecated(since = "0.13.0", note = "Deprecated in favor of get_final_tick_str")]
    pub fn get_final_tick_char(&self) -> char {
        self.get_final_tick_str().chars().next().unwrap_or(' ')
    }

    /// Returns the tick string for the finished state.
    pub fn get_final_tick_str(&self) -> &str {
        &self.tick_strings[self.tick_strings.len() - 1]
    }

    pub(crate) fn format_bar(
        &self,
        state: &ProgressState,
        width: usize,
        alt_style: Option<&Style>,
    ) -> String {
        // todo: this code could really use some comments
        let width = width / state.style.char_width;
        let pct = state.fraction();
        let fill = pct * width as f32;
        let fill = fill as usize;
        let head = if pct > 0.0 && fill < width { 1 } else { 0 };

        let pb = repeat(&state.style.progress_chars[0]).take(fill).fold(
            String::new(),
            |mut acc, new| {
                acc.push_str(&new);
                acc
            },
        );
        let cur = if head == 1 {
            let n = state.style.progress_chars.len().saturating_sub(2);
            let cur_char = if n == 0 {
                1
            } else {
                n.saturating_sub((fill * n) % n)
            };
            state.style.progress_chars[cur_char].to_string()
        } else {
            "".into()
        };
        let bg = width.saturating_sub(fill).saturating_sub(head);
        let rest = repeat(state.style.progress_chars.last().unwrap())
            .take(bg)
            .fold(String::new(), |mut acc, new| {
                acc.push_str(&new);
                acc
            });
        format!(
            "{}{}{}",
            pb,
            cur,
            alt_style.unwrap_or(&Style::new()).apply_to(rest)
        )
    }

    pub(crate) fn format_state(&self, state: &ProgressState) -> Vec<String> {
        let (pos, len) = state.position();
        let mut rv = vec![];

        for line in self.template.lines() {
            let wide_element = RefCell::new(None);

            let s = expand_template(line, |var| {
                let key = var.key;

                match key {
                    "wide_bar" => {
                        *wide_element.borrow_mut() = Some(var.duplicate_for_key("bar"));
                        "\x00".into()
                    }
                    "bar" => {
                        self.format_bar(state, var.width.unwrap_or(20), var.alt_style.as_ref())
                    }
                    "spinner" => state.current_tick_str().to_string(),
                    "wide_msg" => {
                        *wide_element.borrow_mut() = Some(var.duplicate_for_key("msg"));
                        "\x00".into()
                    }
                    "msg" => state.message().to_string(),
                    "prefix" => state.prefix().to_string(),
                    "pos" => pos.to_string(),
                    "len" => len.to_string(),
                    "percent" => format!("{:.*}", 0, state.fraction() * 100f32),
                    "bytes" => format!("{}", HumanBytes(state.pos)),
                    "total_bytes" => format!("{}", HumanBytes(state.len)),
                    "decimal_bytes" => format!("{}", DecimalBytes(state.pos)),
                    "decimal_total_bytes" => format!("{}", DecimalBytes(state.len)),
                    "binary_bytes" => format!("{}", BinaryBytes(state.pos)),
                    "binary_total_bytes" => format!("{}", BinaryBytes(state.len)),
                    "elapsed_precise" => format!("{}", FormattedDuration(state.started.elapsed())),
                    "elapsed" => format!("{:#}", HumanDuration(state.started.elapsed())),
                    "per_sec" => format!("{}/s", state.per_sec()),
                    "bytes_per_sec" => format!("{}/s", HumanBytes(state.per_sec())),
                    "eta_precise" => format!("{}", FormattedDuration(state.eta())),
                    "eta" => format!("{:#}", HumanDuration(state.eta())),
                    _ => "".into(),
                }
            });

            rv.push(if let Some(ref var) = *wide_element.borrow() {
                let total_width = state.width();
                if var.key == "bar" {
                    let bar_width = total_width.saturating_sub(measure_text_width(&s));
                    s.replace(
                        "\x00",
                        &self.format_bar(state, bar_width, var.alt_style.as_ref()),
                    )
                } else if var.key == "msg" {
                    let msg_width = total_width.saturating_sub(measure_text_width(&s));
                    let msg = pad_str(state.message(), msg_width, var.align, true);
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
