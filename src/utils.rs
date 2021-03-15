use std::borrow::Cow;
use std::time::{Duration, Instant};

use regex::{Captures, Regex};

use console::{measure_text_width, Style};

pub fn duration_to_secs(d: Duration) -> f64 {
    d.as_secs() as f64 + f64::from(d.subsec_nanos()) / 1_000_000_000f64
}

pub fn secs_to_duration(s: f64) -> Duration {
    let secs = s.trunc() as u64;
    let nanos = (s.fract() * 1_000_000_000f64) as u32;
    Duration::new(secs, nanos)
}

pub struct Estimate {
    buf: Box<[f64; 15]>,
    data: u8,
    started: Option<(Instant, u64)>,
}

impl Estimate {
    fn len(&self) -> u8 {
        self.data & 0x0F
    }

    fn set_len(&mut self, len: u8) {
        // Sanity check to make sure math is correct as otherwise it could result in unexpected bugs
        debug_assert!(len < 16);
        self.data = (self.data & 0xF0) | len;
    }

    fn last_idx(&self) -> u8 {
        (self.data & 0xF0) >> 4
    }

    fn set_last_idx(&mut self, last_idx: u8) {
        // This will wrap last_idx on overflow (setting to 16 will result in 0); this is fine
        // because Estimate::buf is 15 elements long
        self.data = ((last_idx & 0x0F) << 4) | (self.data & 0x0F);
    }

    pub fn new() -> Self {
        let this = Self {
            buf: Box::new([0.0; 15]),
            data: 0,
            started: Some((<Instant>::now(), 0)),
        };
        // Make sure not to break anything accidentally as self.data can't handle bufs longer than
        // 15 elements
        debug_assert!(this.buf.len() < 16);
        this
    }

    pub fn reset(&mut self) {
        self.started = None;
        self.data = 0;
    }

    pub fn record_step(&mut self, value: u64) {
        // record initial position
        let (started_time, started_value) =
            *self.started.get_or_insert_with(|| (Instant::now(), value));

        let item = {
            let divisor = value.saturating_sub(started_value) as f64;
            if divisor == 0.0 {
                0.0
            } else {
                duration_to_secs(started_time.elapsed()) / divisor
            }
        };
        let len = self.len();
        let last_idx = self.last_idx();
        if self.buf.len() <= usize::from(len) {
            let idx = last_idx % len;
            self.buf[usize::from(idx)] = item;
        } else {
            self.set_len(len + 1);
            self.buf[usize::from(last_idx)] = item;
        }
        self.set_last_idx(last_idx + 1);
    }

    pub fn time_per_step(&self) -> Duration {
        let len = self.len();
        secs_to_duration(self.buf[0..usize::from(len)].iter().sum::<f64>() / f64::from(len))
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

pub fn expand_template<F: FnMut(&TemplateVar<'_>) -> String>(s: &str, mut f: F) -> Cow<'_, str> {
    lazy_static::lazy_static! {
        static ref VAR_RE: Regex = Regex::new(r"(\}\})|\{(\{|[^{}}]+\})").unwrap();
        static ref KEY_RE: Regex = Regex::new(
            r"(?x)
                ([^:]+)
                (?:
                    :
                    ([<^>])?
                    ([0-9]+)?
                    (!)?
                    (?:\.([a-z_]+(?:\.[a-z_]+)*))?
                    (?:/([a-z_]+(?:\.[a-z_]+)*))?
                )?
            "
        )
        .unwrap();
    }
    VAR_RE.replace_all(s, |caps: &Captures<'_>| {
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

pub fn pad_str(s: &str, width: usize, align: Alignment, truncate: bool) -> Cow<'_, str> {
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
    let rv = expand_template(r#"{ "foo": "{foo}", "bar": {bar} }"#, |var| {
        var.key.to_uppercase()
    });
    assert_eq!(&rv, r#"{ "foo": "FOO", "bar": BAR }"#);
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
