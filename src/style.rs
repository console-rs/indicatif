use std::borrow::Cow;
use std::cell::RefCell;
use std::iter::repeat;

use console::{measure_text_width, Style};
use format::{BinaryBytes, DecimalBytes, FormattedDuration, HumanBytes, HumanDuration};
use progress::ProgressState;
use utils::{expand_template, pad_str};

/// Controls the rendering style of progress bars.
#[derive(Clone, Debug)]
pub struct ProgressStyle {
    tick_chars: Vec<char>,
    progress_chars: Vec<char>,
    template: Cow<'static, str>,
}

impl ProgressStyle {
    /// Returns the default progress bar style for bars.
    pub fn default_bar() -> ProgressStyle {
        ProgressStyle {
            tick_chars: "⠁⠁⠉⠙⠚⠒⠂⠂⠒⠲⠴⠤⠄⠄⠤⠠⠠⠤⠦⠖⠒⠐⠐⠒⠓⠋⠉⠈⠈ ".chars().collect(),
            progress_chars: "█░".chars().collect(),
            template: Cow::Borrowed("{wide_bar} {pos}/{len}"),
        }
    }

    /// Returns the default progress bar style for spinners.
    pub fn default_spinner() -> ProgressStyle {
        ProgressStyle {
            tick_chars: "⠁⠁⠉⠙⠚⠒⠂⠂⠒⠲⠴⠤⠄⠄⠤⠠⠠⠤⠦⠖⠒⠐⠐⠒⠓⠋⠉⠈⠈ ".chars().collect(),
            progress_chars: "█░".chars().collect(),
            template: Cow::Borrowed("{spinner} {msg}"),
        }
    }

    /// Sets the tick character sequence for spinners.
    pub fn tick_chars(mut self, s: &str) -> ProgressStyle {
        self.tick_chars = s.chars().collect();
        self
    }

    /// Sets the three progress characters `(filled, current, to do)`.
    pub fn progress_chars(mut self, s: &str) -> ProgressStyle {
        self.progress_chars = s.chars().collect();
        self
    }

    /// Sets the template string for the progress bar.
    pub fn template(mut self, s: &str) -> ProgressStyle {
        self.template = Cow::Owned(s.into());
        self
    }

    /// Returns the tick char for a given number.
    pub fn get_tick_char(&self, idx: u64) -> char {
        self.tick_chars[(idx as usize) % (self.tick_chars.len() - 1)]
    }

    /// Returns the tick char for the finished state.
    pub fn get_final_tick_char(&self) -> char {
        self.tick_chars[self.tick_chars.len() - 1]
    }

    pub(crate) fn format_bar(
        &self,
        state: &ProgressState,
        width: usize,
        alt_style: Option<&Style>,
    ) -> String {
        let pct = state.fraction();
        let fill = pct * width as f32;
        let head = if pct > 0.0 && (fill as usize) < width {
            1
        } else {
            0
        };

        let bar = repeat(state.style.progress_chars[0])
            .take(fill as usize)
            .collect::<String>();
        let cur = if head == 1 {
            let n = state.style.progress_chars.len().saturating_sub(2);
            let cur_char = if n == 0 {
                1
            } else {
                n.saturating_sub((fill * n as f32) as usize % n)
            };
            state.style.progress_chars[cur_char].to_string()
        } else {
            "".into()
        };
        let bg = width.saturating_sub(fill as usize).saturating_sub(head);
        let rest = repeat(state.style.progress_chars.last().unwrap())
            .take(bg)
            .collect::<String>();
        format!(
            "{}{}{}",
            bar,
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
                    "spinner" => state.current_tick_char().to_string(),
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
                            msg.trim_right()
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
