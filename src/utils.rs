use std::borrow::Cow;
use std::time::{Duration, Instant};

use regex::{Captures, Regex};

use console::{measure_text_width, Style};

pub fn duration_to_secs(d: Duration) -> f64 {
    d.as_secs() as f64 + d.subsec_nanos() as f64 / 1_000_000_000f64
}

pub fn secs_to_duration(s: f64) -> Duration {
    let secs = s.trunc() as u64;
    let nanos = (s.fract() * 1_000_000_000f64) as u32;
    Duration::new(secs, nanos)
}

pub struct Estimate {
    buf: Vec<f64>,
    buf_cap: usize,
    last_idx: usize,
    started: Option<(Instant, u64)>,
}

impl Estimate {
    pub fn new() -> Estimate {
        Estimate {
            buf: vec![],
            buf_cap: 10,
            last_idx: 0,
            started: None,
        }
    }

    pub fn record_step(&mut self, value: u64) {
        // record initial position
        let (started_time, started_value) = match self.started {
            None => {
                let rv = (Instant::now(), value);
                self.started = Some(rv);
                rv
            }
            Some(value) => value,
        };

        let item = if value == 0 {
            0.0
        } else {
            duration_to_secs(started_time.elapsed()) / (value.saturating_sub(started_value)) as f64
        };
        if self.buf.len() >= self.buf_cap {
            let idx = self.last_idx % self.buf.len();
            self.buf[idx] = item;
        } else {
            self.buf.push(item);
        }
        self.last_idx += 1;
    }

    pub fn time_per_step(&self) -> Duration {
        if self.buf.is_empty() {
            Duration::new(0, 0)
        } else {
            secs_to_duration(self.buf.iter().sum::<f64>() / self.buf.len() as f64)
        }
    }
}

#[derive(PartialEq, Eq, Debug, Copy, Clone)]
pub enum Alignment {
    Left,
    Center,
    Right,
}

#[derive(Debug)]
pub struct TemplateVar<'a> {
    pub key: &'a str,
    pub align: Alignment,
    pub truncate: bool,
    pub width: Option<usize>,
    pub style: Option<Style>,
    pub alt_style: Option<Style>,
    pub last_element: bool,
}

impl<'a> TemplateVar<'a> {
    pub fn duplicate_for_key<'b>(&self, key: &'b str) -> TemplateVar<'b> {
        TemplateVar {
            key: key,
            align: self.align,
            truncate: self.truncate,
            width: self.width,
            style: self.style.clone(),
            alt_style: self.alt_style.clone(),
            last_element: self.last_element,
        }
    }
}

pub fn expand_template<'a, F: Fn(&TemplateVar) -> String>(s: &'a str, f: F) -> Cow<'a, str> {
    lazy_static! {
        static ref VAR_RE: Regex = Regex::new(r"(\}\})|\{(\{|[^}]+\})").unwrap();
        static ref KEY_RE: Regex = Regex::new(
            r"(?x)
                ([^:]+)
                (?:
                    :
                    ([<^>])?
                    (\d+)?
                    (!)?
                    (?:\.([a-z_]+(?:\.[a-z_]+)*))?
                    (?:/([a-z_]+(?:\.[a-z_]+)*))?
                )?
            "
        ).unwrap();
    }
    VAR_RE.replace_all(s, |caps: &Captures| {
        if caps.get(1).is_some() {
            return "}".into();
        }
        let key = &caps[2];
        if key == "{" {
            return "{".into();
        }
        let mut var = TemplateVar {
            key: key,
            align: Alignment::Left,
            truncate: false,
            width: None,
            style: None,
            alt_style: None,
            last_element: caps.get(0).unwrap().end() >= s.len(),
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
        let mut rv = f(&var);
        if let Some(width) = var.width {
            rv = pad_str(&rv, width, var.align, var.truncate).to_string()
        }
        if let Some(s) = var.style {
            rv = s.apply_to(rv).to_string();
        }
        rv
    })
}

pub fn pad_str<'a>(s: &'a str, width: usize, align: Alignment, truncate: bool) -> Cow<'a, str> {
    let cols = measure_text_width(s);

    if cols >= width {
        return if truncate {
            Cow::Borrowed(s.get(..width).unwrap_or(s))
        } else {
            Cow::Borrowed(s)
        };
    }

    let diff = width.saturating_sub(cols);

    let (left_pad, right_pad) = match align {
        Alignment::Left => (0, diff),
        Alignment::Right => (diff, 0),
        Alignment::Center => (diff / 2, diff.saturating_sub(diff / 2)),
    };

    let mut rv = String::new();
    for _ in 0..left_pad {
        rv.push(' ');
    }
    rv.push_str(s);
    for _ in 0..right_pad {
        rv.push(' ');
    }
    Cow::Owned(rv)
}

#[test]
fn test_expand_template() {
    let rv = expand_template("{{ {foo} {bar} }}", |var| var.key.to_uppercase());
    assert_eq!(&rv, "{ FOO BAR }");
}

#[test]
fn test_expand_template_flags() {
    use console::set_colors_enabled;
    set_colors_enabled(true);

    let rv = expand_template("{foo:5}", |var| {
        assert_eq!(var.key, "foo");
        assert_eq!(var.width, Some(5));
        "XXX".into()
    });
    assert_eq!(&rv, "XXX  ");

    let rv = expand_template("{foo:.red.on_blue}", |var| {
        assert_eq!(var.key, "foo");
        assert_eq!(var.width, None);
        assert_eq!(var.align, Alignment::Left);
        assert_eq!(var.style, Some(Style::new().red().on_blue()));
        "XXX".into()
    });
    assert_eq!(&rv, "\u{1b}[31m\u{1b}[44mXXX\u{1b}[0m");

    let rv = expand_template("{foo:^5.red.on_blue}", |var| {
        assert_eq!(var.key, "foo");
        assert_eq!(var.width, Some(5));
        assert_eq!(var.align, Alignment::Center);
        assert_eq!(var.style, Some(Style::new().red().on_blue()));
        "XXX".into()
    });
    assert_eq!(&rv, "\u{1b}[31m\u{1b}[44m XXX \u{1b}[0m");

    let rv = expand_template("{foo:^5.red.on_blue/green.on_cyan}", |var| {
        assert_eq!(var.key, "foo");
        assert_eq!(var.width, Some(5));
        assert_eq!(var.align, Alignment::Center);
        assert_eq!(var.style, Some(Style::new().red().on_blue()));
        assert_eq!(var.alt_style, Some(Style::new().green().on_cyan()));
        "XXX".into()
    });
    assert_eq!(&rv, "\u{1b}[31m\u{1b}[44m XXX \u{1b}[0m");
}

#[test]
fn test_duration_stuff() {
    let duration = Duration::new(42, 100_000_000);
    let secs = duration_to_secs(duration);
    assert_eq!(secs_to_duration(secs), duration);
}
