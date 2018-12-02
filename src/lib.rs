//! indicatif is a library for Rust that helps you build command line
//! interfaces that report progress to users.  It comes with various
//! tools and utilities for formatting anything that indicates progress.
//!
//! Platform support:
//!
//! * Linux
//! * OS X
//! * Windows (colors require Windows 10)
//!
//! Best paired with other libraries in the family:
//!
//! * [console](https://docs.rs/console)
//! * [dialoguer](https://docs.rs/dialoguer)
//!
//! # Crate Contents
//!
//! * **Progress bars**
//!   * [`ProgressBar`](struct.ProgressBar.html) for bars and spinners
//!   * [`MultiProgress`](struct.MultiProgress.html) for multiple bars
//! * **Data Formatting**
//!   * [`HumanBytes`](struct.HumanBytes.html) for formatting bytes
//!   * [`DecimalBytes`](struct.DecimalBytes.html) for formatting bytes using SI prefixes
//!   * [`BinaryBytes`](struct.BinaryBytes.html) for formatting bytes using ISO/IEC prefixes
//!   * [`HumanDuration`](struct.HumanDuration.html) for formatting durations
//!
//! # Progress Bars and Spinners
//!
//! indicatif comes with a `ProgressBar` type that supports both bounded
//! progress bar uses as well as unbounded "spinner" type progress reports.
//! Progress bars are `Sync` and `Send` objects which means that they are
//! internally locked and can be passed from thread to thread.
//!
//! Additionally a `MultiProgress` utility is provided that can manage
//! rendering multiple progress bars at once (eg: from multiple threads).
//!
//! To whet your appetite, this is what this can look like:
//!
//! <img src="https://github.com/mitsuhiko/indicatif/raw/master/screenshots/yarn.gif?raw=true" width="60%">
//!
//! Progress bars are manually advanced and by default draw to stderr.
//! When you are done, the progress bar can be finished either visibly
//! (eg: the progress bar stays on the screen) or cleared (the progress
//! bar will be removed).
//!
//! ```rust
//! use indicatif::ProgressBar;
//!
//! let bar = ProgressBar::new(1000);
//! for _ in 0..1000 {
//!     bar.inc(1);
//!     // ...
//! }
//! bar.finish();
//! ```
//!
//! General progress bar behaviors:
//!
//! * if a non terminal is detected the progress bar will be completely
//!   hidden.  This makes piping programs to logfiles make sense out of
//!   the box.
//! * progress bars should be explicitly finished to reset the rendering
//!   for others.  Either by also clearing them or by replacing them with
//!   a new message / retaining the current message.
//! * the default template renders neither message nor prefix.
//!
//! # Templates
//!
//! Progress bars can be styled with simple format strings similar to the
//! ones in Rust itself.  The format for a placeholder is `{key:options}`
//! where the `options` part is optional.  If provided the format is this:
//!
//! ```text
//! [<^>]           for an optional alignment specification
//! WIDTH           an optional width as positive integer
//! !               an optional exclamation mark to enable truncation
//! .STYLE          an optional dot separated style string
//! /STYLE          an optional dot separated alternative style string
//! ```
//!
//! For the style component see `Styled::from_dotted_str` for more
//! information.
//!
//! Some examples for templates:
//!
//! ```text
//! [{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}
//! ```
//!
//! This sets a progress bar that is 40 characters wide and has cyan
//! as primary style color and blue as alternative style color.
//! Alternative styles are currently only used for progress bars.
//!
//! Example configuration:
//!
//! ```ignore
//! bar.set_style(ProgressStyle::default_bar()
//!     .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
//!     .progress_chars("##-"));
//! ```
//!
//! The following keys exist:
//!
//! * `bar`: renders a progress bar. By default 20 characters wide.  The
//!   style string is used to color the elapsed part, the alternative
//!   style is used for the bar that is yet to render.
//! * `wide_bar`: like `bar` but always fills the remaining space.
//! * `spinner`: renders the spinner (current tick char)
//! * `prefix`: renders the prefix set on the progress bar.
//! * `msg`: renders the currently set message on the progress bar.
//! * `wide_msg`: like `msg` but always fills the remaining space and truncates.
//! * `pos`: renders the current position of the bar as integer
//! * `len`: renders the total length of the bar as integer
//! * `bytes`: renders the current position of the bar as bytes.
//! * `percent`: renders the current position of the bar as a percentage of the total length.
//! * `total_bytes`: renders the total length of the bar as bytes.
//! * `elapsed_precise`: renders the elapsed time as `HH:MM:SS`.
//! * `elapsed`: renders the elapsed time as `42s`, `1m` etc.
//! * `eta_precise`: the remaining time (like `elapsed_precise`).
//! * `eta`: the remaining time (like `elapsed`).
//!
//! The design of the progress bar can be altered with the integrated
//! template functionality.  The template can be set by changing a
//! `ProgressStyle` and attaching it to the progress bar.
//!
//! # Human Readable Formatting
//!
//! There are some formatting wrappers for showing elapsed time and
//! file sizes for human users:
//!
//! ```ignore
//! use std::time::Instant;
//! use indicatif::{HumanDuration, HumanBytes};
//!
//! let started = Instant::now();
//! println!("The file is {} large", HumanBytes(file.size));
//! println!("The script took {}", HumanDuration(started.elapsed()));
//! ```
extern crate parking_lot;
extern crate regex;
#[macro_use]
extern crate lazy_static;
extern crate console;
extern crate number_prefix;

mod format;
mod progress;
mod utils;

pub use format::{BinaryBytes, DecimalBytes, FormattedDuration, HumanBytes, HumanDuration};
pub use progress::{
    MultiProgress, ProgressBar, ProgressBarIter, ProgressBarRead, ProgressDrawTarget, ProgressStyle,
};
