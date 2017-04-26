use std::borrow::Cow;
use std::time::Duration;

use regex::{Regex, Captures};

use ansistyle::{style, measure_text_width};


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
    pub style: Option<&'a str>,
    pub alt_style: Option<&'a str>,
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
                var.style = Some(style.as_str());
            }
            if let Some(alt_style) = opt_caps.get(6) {
                var.alt_style = Some(alt_style.as_str());
            }
        }
        let mut rv = f(&var);
        if let Some(width) = var.width {
            rv = pad_str(&rv, width, var.align, var.truncate).to_string()
        }
        if let Some(s) = var.style {
            rv = style(rv).from_dotted_str(s).to_string();
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

pub fn format_duration(dur: Duration) -> String {
    let mut t = dur.as_secs();
    let seconds = t % 60;
    t /= 60;
    let minutes = t % 60;
    t /= 60;
    let hours = t % 24;
    t /= 24;
    if t > 0 {
        let days = t;
        format!("{}d {:02}:{:02}:{:02}", days, hours, minutes, seconds)
    } else {
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    }
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
    let rv = expand_template("{foo:5}", |var| {
        assert_eq!(var.key, "foo");
        assert_eq!(var.width, Some(5));
        "XXX".into()
    });
    assert_eq!(&rv, "XXX  ");

    let rv = expand_template("{foo:.red.on_blue}", |var| {
        assert_eq!(var.key, "foo");
        assert_eq!(var.width, Some(5));
        assert_eq!(var.align, Alignment::Center);
        assert_eq!(var.style, Some("red.on_blue"));
        "XXX".into()
    });
    assert_eq!(&rv, "\u{1b}[31m\u{1b}[44mXXX\u{1b}[0m");

    let rv = expand_template("{foo:^5.red.on_blue}", |var| {
        assert_eq!(var.key, "foo");
        assert_eq!(var.width, Some(5));
        assert_eq!(var.align, Alignment::Center);
        assert_eq!(var.style, Some("red.on_blue"));
        "XXX".into()
    });
    assert_eq!(&rv, "\u{1b}[31m\u{1b}[44m XXX \u{1b}[0m");

    let rv = expand_template("{foo:^5.red.on_blue/green.on_cyan}", |var| {
        assert_eq!(var.key, "foo");
        assert_eq!(var.width, Some(5));
        assert_eq!(var.align, Alignment::Center);
        assert_eq!(var.style, Some("red.on_blue"));
        assert_eq!(var.alt_style, Some("green.on_cyan"));
        "XXX".into()
    });
    assert_eq!(&rv, "\u{1b}[31m\u{1b}[44m XXX \u{1b}[0m");
}
