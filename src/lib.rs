//! A library for indicating the progress of an application to a user.
//!
//! Comes with progress bars and spinners currently.
#[cfg(unix)] extern crate libc;
extern crate parking_lot;

mod term;
mod progress;

pub use progress::{ProgressBar, MultiProgress, ProgressState, DrawState, DrawTarget, Style};
pub use term::Term;
