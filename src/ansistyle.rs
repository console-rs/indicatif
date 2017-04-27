use std::fmt;
use std::collections::BTreeSet;
use std::borrow::Cow;

use regex::Regex;
use unicode_width::UnicodeWidthStr;
use clicolors_control;

/// Returns `true` if colors should be enabled.
///
/// This honors the [clicolors spec](http://bixense.com/clicolors/).
///
/// * `CLICOLOR != 0`: ANSI colors are supported and should be used when the program isn't piped.
/// * `CLICOLOR == 0`: Don't output ANSI color escape codes.
/// * `CLICOLOR_FORCE != 0`: ANSI colors should be enabled no matter what.
///
/// This internally uses `clicolors-control`.
#[inline(always)]
pub fn colors_enabled() -> bool {
    clicolors_control::colors_enabled()
}

/// Forces colorization on or off.
///
/// This overrides the default for the current process and changes the return value of the
/// `colors_enabled` function.
///
/// This internally uses `clicolors-control`.
#[inline(always)]
pub fn set_colors_enabled(val: bool) {
    clicolors_control::set_colors_enabled(val)
}

/// Helper function to strip ansi codes.
pub fn strip_ansi_codes(s: &str) -> Cow<str> {
    lazy_static! {
        static ref STRIP_RE: Regex = Regex::new(
            r"[\x1b\x9b][\[()#;?]*(?:[0-9]{1,4}(?:;[0-9]{0,4})*)?[0-9A-PRZcf-nqry=><]").unwrap();
    }
    STRIP_RE.replace_all(s, "")
}

/// Measure the width of a string in terminal characters.
pub fn measure_text_width(s: &str) -> usize {
    strip_ansi_codes(s).width()
}

/// A terminal color.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Color {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
}

impl Color {
    #[inline(always)]
    fn ansi_num(&self) -> usize {
        match *self {
            Color::Black => 0,
            Color::Red => 1,
            Color::Green => 2,
            Color::Yellow => 3,
            Color::Blue => 4,
            Color::Magenta => 5,
            Color::Cyan => 6,
            Color::White => 7,
        }
    }
}

/// A terminal style attribute.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd)]
pub enum Attribute {
    Bold,
    Dim,
    Underlined,
    Blink,
    Reverse,
    Hidden,
}

impl Attribute {
    #[inline(always)]
    fn ansi_num(&self) -> usize {
        match *self {
            Attribute::Bold => 1,
            Attribute::Dim => 2,
            Attribute::Underlined => 4,
            Attribute::Blink => 5,
            Attribute::Reverse => 7,
            Attribute::Hidden => 8,
        }
    }
}

/// A stored style that can be applied.
#[derive(Clone)]
pub struct Style {
    fg: Option<Color>,
    bg: Option<Color>,
    attrs: BTreeSet<Attribute>,
    force: Option<bool>,
}

impl Style {

    /// Returns an empty default style.
    pub fn new() -> Style {
        Style {
            fg: None,
            bg: None,
            attrs: BTreeSet::new(),
            force: None,
        }
    }

    /// Apply the style to something that can be displayed.
    pub fn apply_to<D>(&self, val: D) -> StyledObject<D> {
        StyledObject {
            style: self.clone(),
            val: val
        }
    }

    /// Forces styling on or off.
    ///
    /// This overrides the detection from `clicolors-control`.
    #[inline(always)]
    pub fn force_styling(mut self, value: bool) -> Style {
        self.force = Some(value);
        self
    }

    /// Sets a foreground color.
    #[inline(always)]
    pub fn fg(mut self, color: Color) -> Style {
        self.fg = Some(color);
        self
    }

    /// Sets a background color.
    #[inline(always)]
    pub fn bg(mut self, color: Color) -> Style {
        self.bg = Some(color);
        self
    }

    /// Adds a attr.
    #[inline(always)]
    pub fn attr(mut self, attr: Attribute) -> Style {
        self.attrs.insert(attr);
        self
    }

    /// Applies attrs from a dotted string.
    ///
    /// Effectively the string is split at each dot and then the
    /// terms in between are applied.  For instance `red.on_blue` will
    /// create a string that is red on blue background.  Unknown terms
    /// are ignored.
    pub fn from_dotted_str(self, s: &str) -> Style {
        let mut rv = self;
        for part in s.split('.') {
            rv = match part {
                "black" => rv.black(),
                "red" => rv.red(),
                "green" => rv.green(),
                "yellow" => rv.yellow(),
                "blue" => rv.blue(),
                "magenta" => rv.magenta(),
                "cyan" => rv.cyan(),
                "white" => rv.white(),
                "on_black" => rv.on_black(),
                "on_red" => rv.on_red(),
                "on_green" => rv.on_green(),
                "on_yellow" => rv.on_yellow(),
                "on_blue" => rv.on_blue(),
                "on_magenta" => rv.on_magenta(),
                "on_cyan" => rv.on_cyan(),
                "on_white" => rv.on_white(),
                "bold" => rv.bold(),
                "dim" => rv.dim(),
                "underlined" => rv.underlined(),
                "blink" => rv.blink(),
                "reverse" => rv.reverse(),
                "hidden" => rv.hidden(),
                _ => { continue; }
            };
        }
        rv
    }

    #[inline(always)] pub fn black(self) -> Style { self.fg(Color::Black) }
    #[inline(always)] pub fn red(self) -> Style { self.fg(Color::Red) }
    #[inline(always)] pub fn green(self) -> Style { self.fg(Color::Green) }
    #[inline(always)] pub fn yellow(self) -> Style { self.fg(Color::Yellow) }
    #[inline(always)] pub fn blue(self) -> Style { self.fg(Color::Blue) }
    #[inline(always)] pub fn magenta(self) -> Style { self.fg(Color::Magenta) }
    #[inline(always)] pub fn cyan(self) -> Style { self.fg(Color::Cyan) }
    #[inline(always)] pub fn white(self) -> Style { self.fg(Color::White) }
    #[inline(always)] pub fn on_black(self) -> Style { self.bg(Color::Black) }
    #[inline(always)] pub fn on_red(self) -> Style { self.bg(Color::Red) }
    #[inline(always)] pub fn on_green(self) -> Style { self.bg(Color::Green) }
    #[inline(always)] pub fn on_yellow(self) -> Style { self.bg(Color::Yellow) }
    #[inline(always)] pub fn on_blue(self) -> Style { self.bg(Color::Blue) }
    #[inline(always)] pub fn on_magenta(self) -> Style { self.bg(Color::Magenta) }
    #[inline(always)] pub fn on_cyan(self) -> Style { self.bg(Color::Cyan) }
    #[inline(always)] pub fn on_white(self) -> Style { self.bg(Color::White) }
    #[inline(always)] pub fn bold(self) -> Style { self.attr(Attribute::Bold) }
    #[inline(always)] pub fn dim(self) -> Style { self.attr(Attribute::Dim) }
    #[inline(always)] pub fn underlined(self) -> Style { self.attr(Attribute::Underlined) }
    #[inline(always)] pub fn blink(self) -> Style { self.attr(Attribute::Blink) }
    #[inline(always)] pub fn reverse(self) -> Style { self.attr(Attribute::Reverse) }
    #[inline(always)] pub fn hidden(self) -> Style { self.attr(Attribute::Hidden) }
}

/// Wraps an object for formatting for styling.
///
/// Example:
///
/// ```rust,no_run
/// # use indicatif::style;
/// format!("Hello {}", style("World").cyan());
/// ```
///
/// This is a shortcut for making a new style and applying it
/// to a value:
///
/// ```rust,no_run
/// # use indicatif::Style;
/// format!("Hello {}", Style::new().cyan().apply_to("World"));
/// ```
pub fn style<D>(val: D) -> StyledObject<D> {
    Style::new().apply_to(val)
}

/// A formatting wrapper that can be styled for a terminal.
#[derive(Clone)]
pub struct StyledObject<D> {
    style: Style,
    val: D,
}

impl<D> StyledObject<D> {
    /// Forces styling on or off.
    ///
    /// This overrides the detection from `clicolors-control`.
    #[inline(always)]
    pub fn force_styling(mut self, value: bool) -> StyledObject<D> {
        self.style = self.style.force_styling(value);
        self
    }

    /// Sets a foreground color.
    #[inline(always)]
    pub fn fg(mut self, color: Color) -> StyledObject<D> {
        self.style = self.style.fg(color);
        self
    }

    /// Sets a background color.
    #[inline(always)]
    pub fn bg(mut self, color: Color) -> StyledObject<D> {
        self.style = self.style.bg(color);
        self
    }

    /// Adds a attr.
    #[inline(always)]
    pub fn attr(mut self, attr: Attribute) -> StyledObject<D> {
        self.style = self.style.attr(attr);
        self
    }

    /// Applies attrs from a dotted string.
    ///
    /// Effectively the string is split at each dot and then the
    /// terms in between are applied.  For instance `red.on_blue` will
    /// create a string that is red on blue background.  Unknown terms
    /// are ignored.
    pub fn from_dotted_str(mut self, s: &str) -> StyledObject<D> {
        self.style = self.style.from_dotted_str(s);
        self
    }

    #[inline(always)] pub fn black(self) -> StyledObject<D> { self.fg(Color::Black) }
    #[inline(always)] pub fn red(self) -> StyledObject<D> { self.fg(Color::Red) }
    #[inline(always)] pub fn green(self) -> StyledObject<D> { self.fg(Color::Green) }
    #[inline(always)] pub fn yellow(self) -> StyledObject<D> { self.fg(Color::Yellow) }
    #[inline(always)] pub fn blue(self) -> StyledObject<D> { self.fg(Color::Blue) }
    #[inline(always)] pub fn magenta(self) -> StyledObject<D> { self.fg(Color::Magenta) }
    #[inline(always)] pub fn cyan(self) -> StyledObject<D> { self.fg(Color::Cyan) }
    #[inline(always)] pub fn white(self) -> StyledObject<D> { self.fg(Color::White) }
    #[inline(always)] pub fn on_black(self) -> StyledObject<D> { self.bg(Color::Black) }
    #[inline(always)] pub fn on_red(self) -> StyledObject<D> { self.bg(Color::Red) }
    #[inline(always)] pub fn on_green(self) -> StyledObject<D> { self.bg(Color::Green) }
    #[inline(always)] pub fn on_yellow(self) -> StyledObject<D> { self.bg(Color::Yellow) }
    #[inline(always)] pub fn on_blue(self) -> StyledObject<D> { self.bg(Color::Blue) }
    #[inline(always)] pub fn on_magenta(self) -> StyledObject<D> { self.bg(Color::Magenta) }
    #[inline(always)] pub fn on_cyan(self) -> StyledObject<D> { self.bg(Color::Cyan) }
    #[inline(always)] pub fn on_white(self) -> StyledObject<D> { self.bg(Color::White) }
    #[inline(always)] pub fn bold(self) -> StyledObject<D> { self.attr(Attribute::Bold) }
    #[inline(always)] pub fn dim(self) -> StyledObject<D> { self.attr(Attribute::Dim) }
    #[inline(always)] pub fn underlined(self) -> StyledObject<D> { self.attr(Attribute::Underlined) }
    #[inline(always)] pub fn blink(self) -> StyledObject<D> { self.attr(Attribute::Blink) }
    #[inline(always)] pub fn reverse(self) -> StyledObject<D> { self.attr(Attribute::Reverse) }
    #[inline(always)] pub fn hidden(self) -> StyledObject<D> { self.attr(Attribute::Hidden) }
}

macro_rules! impl_fmt {
    ($name:ident) => {
        impl<D: fmt::$name> fmt::$name for StyledObject<D> {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                let mut reset = false;
                if self.style.force.unwrap_or_else(colors_enabled) {
                    if let Some(fg) = self.style.fg {
                        write!(f, "\x1b[{}m", fg.ansi_num() + 30)?;
                        reset = true;
                    }
                    if let Some(bg) = self.style.bg {
                        write!(f, "\x1b[{}m", bg.ansi_num() + 40)?;
                        reset = true;
                    }
                    for attr in &self.style.attrs {
                        write!(f, "\x1b[{}m", attr.ansi_num())?;
                        reset = true;
                    }
                }
                fmt::$name::fmt(&self.val, f)?;
                if reset {
                    write!(f, "\x1b[0m")?;
                }
                Ok(())
            }
        }
    }
}

impl_fmt!(Binary);
impl_fmt!(Debug);
impl_fmt!(Display);
impl_fmt!(LowerExp);
impl_fmt!(LowerHex);
impl_fmt!(Octal);
impl_fmt!(Pointer);
impl_fmt!(UpperExp);
impl_fmt!(UpperHex);


#[test]
fn test_text_width() {
    let s = style("foo").red().on_black().bold().force_styling(true).to_string();
    assert_eq!(measure_text_width(&s), 3);
}
