use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::{self, Write};
use std::mem;

use console::{measure_text_width, Style};
#[cfg(feature = "improved_unicode")]
use unicode_segmentation::UnicodeSegmentation;

use crate::format::{
    BinaryBytes, DecimalBytes, FormattedDuration, HumanBytes, HumanCount, HumanDuration,
};
use crate::state::ProgressState;

/// Controls the rendering style of progress bars
#[derive(Clone)]
pub struct ProgressStyle {
    tick_strings: Vec<Box<str>>,
    progress_chars: Vec<Box<str>>,
    template: Template,
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
        Self::new("{wide_bar} {pos}/{len}")
    }

    /// Returns the default progress bar style for spinners
    pub fn default_spinner() -> Self {
        Self::new("{spinner} {msg}")
    }

    fn new(template: &str) -> Self {
        let progress_chars = segment("█░");
        let char_width = width(&progress_chars);
        ProgressStyle {
            tick_strings: "⠁⠁⠉⠙⠚⠒⠂⠂⠒⠲⠴⠤⠄⠄⠤⠠⠠⠤⠦⠖⠒⠐⠐⠒⠓⠋⠉⠈⠈ "
                .chars()
                .map(|c| c.to_string().into())
                .collect(),
            progress_chars,
            char_width,
            template: Template::from_str(template),
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
        self.template = Template::from_str(s);
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

    pub(crate) fn current_tick_str(&self, state: &ProgressState) -> &str {
        match state.is_finished() {
            true => self.get_final_tick_str(),
            false => self.get_tick_str(state.tick),
        }
    }

    /// Returns the tick string for a given number
    pub fn get_tick_str(&self, idx: u64) -> &str {
        &self.tick_strings[(idx as usize) % (self.tick_strings.len() - 1)]
    }

    /// Returns the tick string for the finished state
    pub fn get_final_tick_str(&self) -> &str {
        &self.tick_strings[self.tick_strings.len() - 1]
    }

    /// Returns the finish behavior
    pub fn get_on_finish(&self) -> &ProgressFinish {
        &self.on_finish
    }

    fn format_bar(&self, fract: f32, width: usize, alt_style: Option<&Style>) -> BarDisplay<'_> {
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
            Some(cur_char)
        } else {
            None
        };

        // Number of entirely empty clusters needed to fill the bar up to `width`.
        let bg = width.saturating_sub(entirely_filled).saturating_sub(head);
        let rest = RepeatedStringDisplay {
            str: &self.progress_chars[self.progress_chars.len() - 1],
            num: bg,
        };

        BarDisplay {
            chars: &self.progress_chars,
            filled: entirely_filled,
            cur,
            rest: alt_style.unwrap_or(&Style::new()).apply_to(rest),
        }
    }

    pub(crate) fn format_state(&self, state: &ProgressState) -> Vec<String> {
        let mut cur = String::new();
        let mut buf = String::new();
        let mut rv = vec![];
        let mut wide = None;
        for part in &self.template.parts {
            match part {
                TemplatePart::Placeholder {
                    key,
                    align,
                    width,
                    truncate,
                    style,
                    alt_style,
                } => {
                    buf.clear();
                    if let Some(formatter) = self.format_map.0.get(key.as_str()) {
                        buf.push_str(&formatter(state));
                    } else {
                        match key.as_str() {
                            "wide_bar" => {
                                wide = Some(WideElement::Bar { alt_style });
                                buf.push('\x00');
                            }
                            "bar" => buf
                                .write_fmt(format_args!(
                                    "{}",
                                    self.format_bar(
                                        state.fraction(),
                                        width.unwrap_or(20) as usize,
                                        alt_style.as_ref(),
                                    )
                                ))
                                .unwrap(),
                            "spinner" => buf.push_str(state.current_tick_str()),
                            "wide_msg" => {
                                wide = Some(WideElement::Message { align });
                                buf.push('\x00');
                            }
                            "msg" => buf.push_str(state.message()),
                            "prefix" => buf.push_str(state.prefix()),
                            "pos" => buf.write_fmt(format_args!("{}", state.pos)).unwrap(),
                            "human_pos" => buf
                                .write_fmt(format_args!("{}", HumanCount(state.pos)))
                                .unwrap(),
                            "len" => buf.write_fmt(format_args!("{}", state.len)).unwrap(),
                            "human_len" => buf
                                .write_fmt(format_args!("{}", HumanCount(state.len)))
                                .unwrap(),
                            "percent" => buf
                                .write_fmt(format_args!("{:.*}", 0, state.fraction() * 100f32))
                                .unwrap(),
                            "bytes" => buf
                                .write_fmt(format_args!("{}", HumanBytes(state.pos)))
                                .unwrap(),
                            "total_bytes" => buf
                                .write_fmt(format_args!("{}", HumanBytes(state.len)))
                                .unwrap(),
                            "decimal_bytes" => buf
                                .write_fmt(format_args!("{}", DecimalBytes(state.pos)))
                                .unwrap(),
                            "decimal_total_bytes" => buf
                                .write_fmt(format_args!("{}", DecimalBytes(state.len)))
                                .unwrap(),
                            "binary_bytes" => buf
                                .write_fmt(format_args!("{}", BinaryBytes(state.pos)))
                                .unwrap(),
                            "binary_total_bytes" => buf
                                .write_fmt(format_args!("{}", BinaryBytes(state.len)))
                                .unwrap(),
                            "elapsed_precise" => buf
                                .write_fmt(format_args!(
                                    "{}",
                                    FormattedDuration(state.started.elapsed())
                                ))
                                .unwrap(),
                            "elapsed" => buf
                                .write_fmt(format_args!(
                                    "{:#}",
                                    HumanDuration(state.started.elapsed())
                                ))
                                .unwrap(),
                            "per_sec" => buf
                                .write_fmt(format_args!("{:.4}/s", state.per_sec()))
                                .unwrap(),
                            "bytes_per_sec" => buf
                                .write_fmt(format_args!("{}/s", HumanBytes(state.per_sec() as u64)))
                                .unwrap(),
                            "binary_bytes_per_sec" => buf
                                .write_fmt(format_args!(
                                    "{}/s",
                                    BinaryBytes(state.per_sec() as u64)
                                ))
                                .unwrap(),
                            "eta_precise" => buf
                                .write_fmt(format_args!("{}", FormattedDuration(state.eta())))
                                .unwrap(),
                            "eta" => buf
                                .write_fmt(format_args!("{:#}", HumanDuration(state.eta())))
                                .unwrap(),
                            "duration_precise" => buf
                                .write_fmt(format_args!("{}", FormattedDuration(state.duration())))
                                .unwrap(),
                            "duration" => buf
                                .write_fmt(format_args!("{:#}", HumanDuration(state.duration())))
                                .unwrap(),
                            _ => (),
                        }
                    };

                    match width {
                        Some(width) => {
                            let padded = PaddedStringDisplay {
                                str: &buf,
                                width: *width as usize,
                                align: *align,
                                truncate: *truncate,
                            };
                            match style {
                                Some(s) => cur
                                    .write_fmt(format_args!("{}", s.apply_to(padded)))
                                    .unwrap(),
                                None => cur.write_fmt(format_args!("{}", padded)).unwrap(),
                            }
                        }
                        None => match style {
                            Some(s) => cur.write_fmt(format_args!("{}", s.apply_to(&buf))).unwrap(),
                            None => cur.push_str(&buf),
                        },
                    }
                }
                TemplatePart::Literal(s) => cur.push_str(s),
                TemplatePart::NewLine => rv.push(match wide {
                    Some(inner) => inner.expand(mem::take(&mut cur), self, state, &mut buf),
                    None => mem::take(&mut cur),
                }),
            }
        }

        if !cur.is_empty() {
            rv.push(match wide {
                Some(inner) => inner.expand(mem::take(&mut cur), self, state, &mut buf),
                None => mem::take(&mut cur),
            })
        }
        rv
    }
}

#[derive(Clone, Copy)]
enum WideElement<'a> {
    Bar { alt_style: &'a Option<Style> },
    Message { align: &'a Alignment },
}

impl<'a> WideElement<'a> {
    fn expand(
        self,
        cur: String,
        style: &ProgressStyle,
        state: &ProgressState,
        buf: &mut String,
    ) -> String {
        let left = state.width().saturating_sub(measure_text_width(&cur));
        let left = left + 1; // Add 1 to cover the whole terminal width
        match self {
            Self::Bar { alt_style } => cur.replace(
                "\x00",
                &format!(
                    "{}",
                    style.format_bar(state.fraction(), left, alt_style.as_ref())
                ),
            ),
            WideElement::Message { align } => {
                buf.clear();
                buf.write_fmt(format_args!(
                    "{}",
                    PaddedStringDisplay {
                        str: state.message(),
                        width: left,
                        align: *align,
                        truncate: true,
                    }
                ))
                .unwrap();

                let trimmed = match cur.as_bytes().last() == Some(&b'\x00') {
                    true => buf.trim_end(),
                    false => buf,
                };

                cur.replace("\x00", trimmed)
            }
        }
    }
}

#[derive(Clone, Debug)]
struct Template {
    parts: Vec<TemplatePart>,
}

impl Template {
    fn from_str(s: &str) -> Self {
        use State::*;
        let (mut state, mut parts, mut buf) = (Literal, vec![], String::new());
        for c in s.chars() {
            let new = match (state, c) {
                (Literal, '{') => (MaybeOpen, None),
                (Literal, '\n') => {
                    if !buf.is_empty() {
                        parts.push(TemplatePart::Literal(mem::take(&mut buf)));
                    }
                    parts.push(TemplatePart::NewLine);
                    (Literal, None)
                }
                (Literal, '}') => (DoubleClose, Some('}')),
                (Literal, c) => (Literal, Some(c)),
                (DoubleClose, '}') => (Literal, None),
                (MaybeOpen, '{') => (Literal, Some('{')),
                (MaybeOpen, c) | (Key, c) if c.is_ascii_whitespace() => {
                    // If we find whitespace where the variable key is supposed to go,
                    // backtrack and act as if this was a literal.
                    buf.push(c);
                    let mut new = String::from("{");
                    new.push_str(&buf);
                    buf.clear();
                    parts.push(TemplatePart::Literal(new));
                    (Literal, None)
                }
                (MaybeOpen, c) if c != '}' && c != ':' => (Key, Some(c)),
                (Key, c) if c != '}' && c != ':' => (Key, Some(c)),
                (Key, ':') => (Align, None),
                (Key, '}') => (Literal, None),
                (Key, '!') if !buf.is_empty() => {
                    parts.push(TemplatePart::Placeholder {
                        key: mem::take(&mut buf),
                        align: Alignment::Left,
                        width: None,
                        truncate: true,
                        style: None,
                        alt_style: None,
                    });
                    (Width, None)
                }
                (Align, c) if c == '<' || c == '^' || c == '>' => {
                    if let Some(TemplatePart::Placeholder { align, .. }) = parts.last_mut() {
                        match c {
                            '<' => *align = Alignment::Left,
                            '^' => *align = Alignment::Center,
                            '>' => *align = Alignment::Right,
                            _ => (),
                        }
                    }

                    (Width, None)
                }
                (Align, c @ '0'..='9') => (Width, Some(c)),
                (Align, '!') | (Width, '!') => {
                    if let Some(TemplatePart::Placeholder { truncate, .. }) = parts.last_mut() {
                        *truncate = true;
                    }
                    (Width, None)
                }
                (Align, '.') => (FirstStyle, None),
                (Align, '}') => (Literal, None),
                (Width, c @ '0'..='9') => (Width, Some(c)),
                (Width, '.') => (FirstStyle, None),
                (Width, '}') => (Literal, None),
                (FirstStyle, '/') => (AltStyle, None),
                (FirstStyle, '}') => (Literal, None),
                (FirstStyle, c) => (FirstStyle, Some(c)),
                (AltStyle, '}') => (Literal, None),
                (AltStyle, c) => (AltStyle, Some(c)),
                (st, c) => panic!("unreachable state: {:?} @ {:?}", c, st),
            };

            match (state, new.0) {
                (MaybeOpen, Key) if !buf.is_empty() => {
                    parts.push(TemplatePart::Literal(mem::take(&mut buf)))
                }
                (Key, Align) | (Key, Literal) if !buf.is_empty() => {
                    parts.push(TemplatePart::Placeholder {
                        key: mem::take(&mut buf),
                        align: Alignment::Left,
                        width: None,
                        truncate: false,
                        style: None,
                        alt_style: None,
                    })
                }
                (Width, FirstStyle) | (Width, Literal) if !buf.is_empty() => {
                    if let Some(TemplatePart::Placeholder { width, .. }) = parts.last_mut() {
                        *width = Some(buf.parse().unwrap());
                        buf.clear();
                    }
                }
                (FirstStyle, AltStyle) | (FirstStyle, Literal) if !buf.is_empty() => {
                    if let Some(TemplatePart::Placeholder { style, .. }) = parts.last_mut() {
                        *style = Some(Style::from_dotted_str(&buf));
                        buf.clear();
                    }
                }
                (AltStyle, Literal) if !buf.is_empty() => {
                    if let Some(TemplatePart::Placeholder { alt_style, .. }) = parts.last_mut() {
                        *alt_style = Some(Style::from_dotted_str(&buf));
                        buf.clear();
                    }
                }
                (_, _) => (),
            }

            state = new.0;
            if let Some(c) = new.1 {
                buf.push(c);
            }
        }

        if matches!(state, Literal | DoubleClose) && !buf.is_empty() {
            parts.push(TemplatePart::Literal(buf));
        }

        Self { parts }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum TemplatePart {
    Literal(String),
    Placeholder {
        key: String,
        align: Alignment,
        width: Option<u16>,
        truncate: bool,
        style: Option<Style>,
        alt_style: Option<Style>,
    },
    NewLine,
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum State {
    Literal,
    MaybeOpen,
    DoubleClose,
    Key,
    Align,
    Width,
    FirstStyle,
    AltStyle,
}

struct BarDisplay<'a> {
    chars: &'a [Box<str>],
    filled: usize,
    cur: Option<usize>,
    rest: console::StyledObject<RepeatedStringDisplay<'a>>,
}

impl<'a> fmt::Display for BarDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for _ in 0..self.filled {
            f.write_str(&self.chars[0])?;
        }
        if let Some(cur) = self.cur {
            f.write_str(&self.chars[cur])?;
        }
        self.rest.fmt(f)
    }
}

struct RepeatedStringDisplay<'a> {
    str: &'a str,
    num: usize,
}

impl<'a> fmt::Display for RepeatedStringDisplay<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for _ in 0..self.num {
            f.write_str(self.str)?;
        }
        Ok(())
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

        style.template = Template::from_str("{{ {foo} {bar} }}");
        let rv = style.format_state(&state);
        assert_eq!(&rv[0], "{ FOO BAR }");

        style.template = Template::from_str(r#"{ "foo": "{foo}", "bar": {bar} }"#);
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

        style.template = Template::from_str("{foo:5}");
        let rv = style.format_state(&state);
        assert_eq!(&rv[0], "XXX  ");

        style.template = Template::from_str("{foo:.red.on_blue}");
        let rv = style.format_state(&state);
        assert_eq!(&rv[0], "\u{1b}[31m\u{1b}[44mXXX\u{1b}[0m");

        style.template = Template::from_str("{foo:^5.red.on_blue}");
        let rv = style.format_state(&state);
        assert_eq!(&rv[0], "\u{1b}[31m\u{1b}[44m XXX \u{1b}[0m");

        style.template = Template::from_str("{foo:^5.red.on_blue/green.on_cyan}");
        let rv = style.format_state(&state);
        assert_eq!(&rv[0], "\u{1b}[31m\u{1b}[44m XXX \u{1b}[0m");
    }
}
