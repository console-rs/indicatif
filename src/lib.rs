//! A library for indicating the progress of an application to a user.
//!
//! Comes with progress bars and spinners currently.
#[cfg(unix)] extern crate libc;
extern crate parking_lot;
extern crate regex;
#[macro_use] extern crate lazy_static;
extern crate unicode_width;

mod term;
mod progress;
mod utils;
mod ansistyle;

pub use progress::{ProgressBar, MultiProgress, ProgressState, DrawState,
                   DrawTarget, ProgressStyle};
pub use term::Term;
pub use ansistyle::{style, Styled, Color, Style, should_style, set_should_style};
