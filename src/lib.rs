//! A library for indicating the progress of an application to a user.
//!
//! Comes with progress bars and spinners currently.
#[cfg(unix)] extern crate libc;
extern crate parking_lot;
extern crate regex;
#[macro_use] extern crate lazy_static;

mod term;
mod progress;
mod utils;

pub use progress::{ProgressBar, MultiProgress, ProgressState, DrawState, DrawTarget, Style};
pub use term::Term;
