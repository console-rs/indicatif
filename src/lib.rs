//! A library for indicating the progress of an application to a user.
//!
//! Comes with progress bars and spinners currently.
#[cfg(unix)] extern crate libc;
#[cfg(windows)] extern crate winapi;
#[cfg(windows)] extern crate kernel32;
extern crate parking_lot;
extern crate regex;
#[macro_use] extern crate lazy_static;
extern crate unicode_width;
extern crate clicolors_control;

mod term;
mod progress;
mod utils;
mod ansistyle;
mod format;

pub use progress::{ProgressBar, MultiProgress, ProgressState, DrawState,
                   DrawTarget, ProgressStyle};
pub use term::Term;
pub use ansistyle::{style, Styled, Color, Style,
                    strip_ansi_codes, measure_text_width};
pub use format::{HumanDuration, FormattedDuration, HumanBytes};
