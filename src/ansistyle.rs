use std::fmt;
use std::collections::BTreeSet;

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

#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd)]
pub enum Style {
    Bold,
    Dim,
    Underlined,
    Blink,
    Reverse,
    Hidden,
}

impl Style {
    fn ansi_num(&self) -> usize {
        match *self {
            Style::Bold => 1,
            Style::Dim => 2,
            Style::Underlined => 4,
            Style::Blink => 5,
            Style::Reverse => 7,
            Style::Hidden => 8,
        }
    }
}

pub struct Styled<D> {
    fg: Option<Color>,
    bg: Option<Color>,
    styles: BTreeSet<Style>,
    val: D,
}

pub fn style<D>(val: D) -> Styled<D> {
    Styled {
        fg: None,
        bg: None,
        styles: BTreeSet::new(),
        val: val,
    }
}

impl<D> Styled<D> {
    pub fn black(mut self) -> Styled<D> {
        self.fg = Some(Color::Black);
        self
    }

    pub fn red(mut self) -> Styled<D> {
        self.fg = Some(Color::Red);
        self
    }

    pub fn green(mut self) -> Styled<D> {
        self.fg = Some(Color::Green);
        self
    }

    pub fn yellow(mut self) -> Styled<D> {
        self.fg = Some(Color::Yellow);
        self
    }

    pub fn blue(mut self) -> Styled<D> {
        self.fg = Some(Color::Blue);
        self
    }

    pub fn magenta(mut self) -> Styled<D> {
        self.fg = Some(Color::Magenta);
        self
    }

    pub fn cyan(mut self) -> Styled<D> {
        self.fg = Some(Color::Cyan);
        self
    }

    pub fn white(mut self) -> Styled<D> {
        self.fg = Some(Color::White);
        self
    }

    pub fn on_black(mut self) -> Styled<D> {
        self.bg = Some(Color::Black);
        self
    }

    pub fn on_red(mut self) -> Styled<D> {
        self.bg = Some(Color::Red);
        self
    }

    pub fn on_green(mut self) -> Styled<D> {
        self.bg = Some(Color::Green);
        self
    }

    pub fn on_yellow(mut self) -> Styled<D> {
        self.bg = Some(Color::Yellow);
        self
    }

    pub fn on_blue(mut self) -> Styled<D> {
        self.bg = Some(Color::Blue);
        self
    }

    pub fn on_magenta(mut self) -> Styled<D> {
        self.bg = Some(Color::Magenta);
        self
    }

    pub fn on_cyan(mut self) -> Styled<D> {
        self.bg = Some(Color::Cyan);
        self
    }

    pub fn on_white(mut self) -> Styled<D> {
        self.bg = Some(Color::White);
        self
    }

    pub fn bold(mut self) -> Styled<D> {
        self.styles.insert(Style::Bold);
        self
    }

    pub fn dim(mut self) -> Styled<D> {
        self.styles.insert(Style::Dim);
        self
    }

    pub fn underlined(mut self) -> Styled<D> {
        self.styles.insert(Style::Underlined);
        self
    }

    pub fn blink(mut self) -> Styled<D> {
        self.styles.insert(Style::Blink);
        self
    }

    pub fn reverse(mut self) -> Styled<D> {
        self.styles.insert(Style::Reverse);
        self
    }

    pub fn hidden(mut self) -> Styled<D> {
        self.styles.insert(Style::Hidden);
        self
    }
}

macro_rules! impl_fmt {
    ($name:ident) => {
        impl<D: fmt::$name> fmt::$name for Styled<D> {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                if let Some(fg) = self.fg {
                    write!(f, "\x1b[{}m", fg.ansi_num() + 30)?;
                }
                if let Some(bg) = self.bg {
                    write!(f, "\x1b[{}m", bg.ansi_num() + 40)?;
                }
                for style in &self.styles {
                    write!(f, "\x1b[{}m", style.ansi_num())?;
                }
                fmt::$name::fmt(&self.val, f)?;
                for style in &self.styles {
                    write!(f, "\x1b[2{}m", style.ansi_num())?;
                }
                if let Some(..) = self.bg {
                    write!(f, "\x1b[{}m", 49)?;
                }
                if let Some(..) = self.fg {
                    write!(f, "\x1b[{}m", 39)?;
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
