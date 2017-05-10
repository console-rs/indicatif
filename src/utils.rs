use std::borrow::Cow;
use std::time::{Instant, Duration};
use std::ops;

use regex::{Regex, Captures};

use console::{Style, measure_text_width};

#[derive(PartialEq, PartialOrd, Copy, Clone, Default, Debug)]
pub struct Seconds(pub f64);

impl Seconds {
    pub fn from_duration(d: Duration) -> Seconds {
        Seconds(d.as_secs() as f64 + d.subsec_nanos() as f64 / 1_000_000_000f64)
    }

    pub fn to_duration(&self) -> Duration {
        let secs = self.0.trunc() as u64;
        let nanos = (self.0.fract() * 1_000_000_000f64) as u32;
        Duration::new(secs, nanos)
    }
}

impl ops::Add for Seconds {
    type Output = Seconds;
    fn add(self, rhs: Self) -> Seconds { Seconds(self.0 + rhs.0) }
}

impl ops::Mul<f64> for Seconds {
    type Output = Seconds;
    fn mul(self, rhs: f64) -> Seconds { Seconds(self.0 * rhs) }
}

impl ops::Div<f64> for Seconds {
    type Output = Seconds;
    fn div(self, rhs: f64) -> Seconds { Seconds(self.0 / rhs) }
}

pub struct Estimate {
    buf: Vec<Seconds>,
    buf_cap: usize,
    last_idx: usize,
    started: Instant,
}

impl Estimate {
    pub fn new() -> Estimate {
        Estimate {
            buf: vec![],
            buf_cap: 10,
            last_idx: 0,
            started: Instant::now(),
        }
    }

    pub fn record_step(&mut self, value: u64) {
        let item = if value == 0 {
            Seconds(0.0)
        } else {
            Seconds::from_duration(self.started.elapsed()) / value as f64
        };
        if self.buf.len() >= self.buf_cap {
            let idx = self.last_idx % self.buf.len();
            self.buf[idx] = item;
        } else {
            self.buf.push(item);
        }
        self.last_idx += 1;
    }

    pub fn time_per_step(&self) -> Seconds {
        match self.buf.len() {
            0 => Seconds(0.0),
            n => Seconds(self.buf.iter().map(|s| s.0).sum::<f64>()) / n as f64
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
}

pub fn expand_template<'a, F: Fn(&TemplateVar) -> String>(s: &'a str, f: F) -> Cow<'a, str> {
    lazy_static! {
        static ref VAR_RE: Regex = Regex::new(
            r"(\}\})|\{(\{|[^}]+\})").unwrap();
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
            ").unwrap();
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

pub fn pad_str<'a>(s: &'a str, width: usize,
                   align: Alignment, truncate: bool) -> Cow<'a, str> {
    let cols = measure_text_width(s);

    if cols >= width {
        return if truncate {
            Cow::Borrowed(&s[..width])
        }
        else {
            Cow::Borrowed(s)
        };
    }

    let diff = width - cols;

    let (left_pad, right_pad) = match align {
        Alignment::Left => (0, diff),
        Alignment::Right => (diff, 0),
        Alignment::Center => (diff / 2, diff - diff / 2),
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
    let rv = expand_template("{{ {foo} {bar} }}", |var| {
        var.key.to_uppercase()
    });
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
    let secs = Seconds::from_duration(duration);
    assert_eq!(secs.to_duration(), duration);
}

#[test]
fn test_seconds_ops() {
    let s = Seconds(5.0);
    assert_eq!(s + Seconds(6.0), Seconds(11.0));
    assert_eq!(s * 2.0, Seconds(10.0));
    assert_eq!(s / 2.0, Seconds(2.5));
}
