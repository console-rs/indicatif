//! indicatif is a library for Rust that helps you build command line
//! interfaces that report progress to users.  It comes with various
//! tools and utilities for formatting anything that indicates progress.
//!
//! # Progress Bars and Spinners
//!
//! indicatif comes with a `ProgressBar` type that support both bounded
//! progress bar uses as well as unbounded "spinner" type progress reports.
//! Progress bars are `Sync` and `Send` objects which means that they are
//! internally locked and can be passed from thread to thread.
//!
//! Progress bars are manually advanced and by default draw to stdout.
//! When you are done the progress bar can be finished either visibly
//! (eg: the progress bar stays on the screen) or cleared (the progress
//! bar will be removed).
//!
//! ```rust
//! use indicatif::ProgressBar;
//!
//! let bar = ProgressBar::new(1000);
//! for _ in 0..1000 {
//!     bar.inc();
//!     // ...
//! }
//! bar.finish();
//! ```
//!
//! The design of the progress bar can be altered with the integrated
//! template functionality.
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
                    strip_ansi_codes, measure_text_width,
                    colors_enabled, set_colors_enabled};
pub use format::{HumanDuration, FormattedDuration, HumanBytes};
