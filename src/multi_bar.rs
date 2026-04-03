use std::borrow::Cow;
use std::fmt;
use std::time::Duration;

use crate::draw_target::ProgressDrawTarget;
use crate::progress_bar::ProgressBar;
use crate::state::ProgressFinish;
use crate::style::ProgressStyle;

/// A deferred progress bar configuration for use with [`MultiProgress`].
///
/// Unlike [`ProgressBar`], a `MultiBar` never draws to the terminal on its own.
/// Configuration is captured and applied only when the `MultiBar` is added to a
/// [`MultiProgress`] via methods like [`MultiProgress::add`].
///
/// This avoids a common footgun where a [`ProgressBar`] is configured (triggering
/// premature draws to stderr) before being added to a [`MultiProgress`], causing
/// screen corruption. See [#677] for details.
///
/// Dropping a `MultiBar` without adding it to a [`MultiProgress`] is a no-op —
/// no resources are allocated until materialization.
///
/// # Migration
///
/// Passing a [`ProgressBar`] directly to [`MultiProgress::add`] is supported
/// for backwards compatibility but will be removed in a future release. To
/// migrate, replace:
///
/// ```rust,ignore
/// let pb = mp.add(ProgressBar::new(100));
/// pb.set_message("downloading");
/// pb.enable_steady_tick(Duration::from_millis(100));
/// ```
///
/// with:
///
/// ```rust,ignore
/// let pb = mp.add(
///     MultiBar::new(100)
///         .with_message("downloading")
///         .with_steady_tick(Duration::from_millis(100))
/// );
/// ```
///
/// # Example
///
/// ```rust
/// use std::time::Duration;
/// use indicatif::{MultiBar, MultiProgress, ProgressStyle};
///
/// let mp = MultiProgress::new();
/// let pb = mp.add(
///     MultiBar::new(100)
///         .with_style(ProgressStyle::with_template("{bar:40} {pos}/{len} {msg}").unwrap())
///         .with_message("downloading")
///         .with_steady_tick(Duration::from_millis(100))
/// );
/// ```
///
/// [`MultiProgress`]: crate::MultiProgress
/// [`MultiProgress::add`]: crate::MultiProgress::add
/// [`ProgressBar`]: crate::ProgressBar
/// [#677]: https://github.com/console-rs/indicatif/issues/677
#[derive(Clone)]
pub struct MultiBar {
    len: Option<u64>,
    style: Option<ProgressStyle>,
    message: Option<Cow<'static, str>>,
    prefix: Option<Cow<'static, str>>,
    position: Option<u64>,
    elapsed: Option<Duration>,
    tab_width: Option<usize>,
    on_finish: Option<ProgressFinish>,
    steady_tick: Option<Duration>,
}

// ProgressStyle doesn't implement Debug, so we print all other fields
impl fmt::Debug for MultiBar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MultiBar")
            .field("len", &self.len)
            .field("message", &self.message)
            .field("prefix", &self.prefix)
            .field("position", &self.position)
            .field("elapsed", &self.elapsed)
            .field("tab_width", &self.tab_width)
            .field("on_finish", &self.on_finish)
            .field("steady_tick", &self.steady_tick)
            .finish_non_exhaustive()
    }
}

impl MultiBar {
    fn base() -> Self {
        Self {
            len: None,
            style: None,
            message: None,
            prefix: None,
            position: None,
            elapsed: None,
            tab_width: None,
            on_finish: None,
            steady_tick: None,
        }
    }

    /// Creates a new `MultiBar` with a given length.
    pub fn new(len: u64) -> Self {
        Self {
            len: Some(len),
            ..Self::base()
        }
    }

    /// Creates a new `MultiBar` without a specified length.
    pub fn no_length() -> Self {
        Self::base()
    }

    /// Creates a new spinner-style `MultiBar` (no length, default spinner style).
    pub fn new_spinner() -> Self {
        Self {
            style: Some(ProgressStyle::default_spinner()),
            ..Self::base()
        }
    }

    /// Sets the style for the progress bar.
    pub fn with_style(mut self, style: ProgressStyle) -> Self {
        self.style = Some(style);
        self
    }

    /// Sets the tab width for the progress bar.
    pub fn with_tab_width(mut self, tab_width: usize) -> Self {
        self.tab_width = Some(tab_width);
        self
    }

    /// Sets the prefix for the progress bar.
    ///
    /// For the prefix to be visible, the `{prefix}` placeholder must be present in the template
    /// (see [`ProgressStyle`]).
    pub fn with_prefix(mut self, prefix: impl Into<Cow<'static, str>>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// Sets the message for the progress bar.
    ///
    /// For the message to be visible, the `{msg}` placeholder must be present in the template
    /// (see [`ProgressStyle`]).
    pub fn with_message(mut self, message: impl Into<Cow<'static, str>>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Sets the initial position for the progress bar.
    pub fn with_position(mut self, pos: u64) -> Self {
        self.position = Some(pos);
        self
    }

    /// Sets the elapsed time for the progress bar.
    ///
    /// # Panics
    ///
    /// Panics during materialization if `elapsed` is larger than the time since system boot
    /// (inherited from [`ProgressBar::with_elapsed`]).
    pub fn with_elapsed(mut self, elapsed: Duration) -> Self {
        self.elapsed = Some(elapsed);
        self
    }

    /// Sets the finish behavior for the progress bar.
    pub fn with_finish(mut self, finish: ProgressFinish) -> Self {
        self.on_finish = Some(finish);
        self
    }

    /// Enables steady tick with the given interval after materialization.
    ///
    /// The tick thread will only be started when this `MultiBar` is added to a
    /// [`MultiProgress`].
    ///
    /// [`MultiProgress`]: crate::MultiProgress
    pub fn with_steady_tick(mut self, interval: Duration) -> Self {
        self.steady_tick = Some(interval);
        self
    }

    /// Materialize into a [`ProgressBar`] with the given draw target.
    ///
    /// `is_stderr` indicates whether the [`MultiProgress`]'s draw target is stderr,
    /// used for style color detection. Another thread could call
    /// [`MultiProgress::set_draw_target`] after we read `is_stderr` but before the
    /// style is applied, making the stderr detection stale. This is an inherited
    /// limitation matching the existing [`ProgressBar`] path.
    ///
    /// [`MultiProgress`]: crate::MultiProgress
    /// [`MultiProgress::set_draw_target`]: crate::MultiProgress::set_draw_target
    pub(crate) fn materialize(
        self,
        draw_target: ProgressDrawTarget,
        is_stderr: bool,
    ) -> ProgressBar {
        // INVARIANT: tab_width must be set before prefix/message (affects tab expansion).
        // INVARIANT: draw target must be set after all style/content configuration.
        // INVARIANT: steady_tick must be last (spawns a thread that reads the draw target).

        // 1. Create with hidden target (no draws possible)
        let pb = ProgressBar::with_draw_target(self.len, ProgressDrawTarget::hidden());

        // 2. Apply tab_width FIRST — with_prefix/with_message use the current
        //    tab_width for tab expansion, so this must precede them.
        let pb = if let Some(tw) = self.tab_width {
            pb.with_tab_width(tw)
        } else {
            pb
        };

        // 3. Apply style, adjusting for stderr color detection if needed
        let pb = if let Some(mut style) = self.style {
            if is_stderr {
                style.set_for_stderr();
            }
            pb.with_style(style)
        } else {
            pb
        };

        // 4. Apply remaining configuration
        let pb = if let Some(prefix) = self.prefix {
            pb.with_prefix(prefix)
        } else {
            pb
        };
        let pb = if let Some(msg) = self.message {
            pb.with_message(msg)
        } else {
            pb
        };
        let pb = if let Some(pos) = self.position {
            pb.with_position(pos)
        } else {
            pb
        };
        let pb = if let Some(elapsed) = self.elapsed {
            pb.with_elapsed(elapsed)
        } else {
            pb
        };
        let pb = if let Some(finish) = self.on_finish {
            pb.with_finish(finish)
        } else {
            pb
        };

        // 5. Set the real draw target (MultiProgress remote target)
        pb.set_draw_target(draw_target);

        // 6. Start steady tick LAST (spawns thread, must draw to correct target)
        if let Some(interval) = self.steady_tick {
            pb.enable_steady_tick(interval);
        }

        pb
    }
}

/// Input type for [`MultiProgress`] add/insert methods.
///
/// This type is public because it appears in method signatures, but you should not need
/// to construct it directly. Pass a [`ProgressBar`] or [`MultiBar`] to [`MultiProgress`]
/// methods instead.
///
/// [`MultiProgress`]: crate::MultiProgress
#[doc(hidden)]
#[non_exhaustive]
pub enum MultiProgressInput {
    ProgressBar(ProgressBar),
    MultiBar(Box<MultiBar>),
}

impl MultiProgressInput {
    pub(crate) fn materialize(
        self,
        draw_target: ProgressDrawTarget,
        is_stderr: bool,
    ) -> ProgressBar {
        match self {
            Self::ProgressBar(pb) => {
                pb.set_draw_target(draw_target);
                pb
            }
            Self::MultiBar(mb) => mb.materialize(draw_target, is_stderr),
        }
    }
}

/// Backwards-compatible conversion. Will be removed in a future release;
/// use [`MultiBar`] instead.
impl From<ProgressBar> for MultiProgressInput {
    fn from(pb: ProgressBar) -> Self {
        Self::ProgressBar(pb)
    }
}

impl From<MultiBar> for MultiProgressInput {
    fn from(mb: MultiBar) -> Self {
        Self::MultiBar(Box::new(mb))
    }
}

#[cfg(test)]
mod tests {
    use crate::multi::MultiProgress;
    use crate::style::ProgressStyle;

    use super::*;

    #[test]
    fn multi_bar_new_sets_length() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.add(MultiBar::new(42));
        assert_eq!(pb.length(), Some(42));
    }

    #[test]
    fn multi_bar_no_length() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.add(MultiBar::no_length());
        assert_eq!(pb.length(), None);
    }

    #[test]
    fn multi_bar_new_spinner_has_no_length() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.add(MultiBar::new_spinner());
        assert_eq!(pb.length(), None);
    }

    #[test]
    fn multi_bar_with_message() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.add(MultiBar::new(10).with_message("hello"));
        assert_eq!(pb.message(), "hello");
    }

    #[test]
    fn multi_bar_with_prefix() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.add(MultiBar::new(10).with_prefix("[1/3]"));
        assert_eq!(pb.prefix(), "[1/3]");
    }

    #[test]
    fn multi_bar_with_position() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.add(MultiBar::new(100).with_position(50));
        assert_eq!(pb.position(), 50);
    }

    #[test]
    fn multi_bar_with_style() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let style = ProgressStyle::with_template("{msg}").unwrap();
        let pb = mp.add(MultiBar::new(10).with_style(style));
        pb.set_message("test");
        assert_eq!(pb.message(), "test");
    }

    #[test]
    fn multi_bar_with_elapsed() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.add(MultiBar::new(100).with_elapsed(Duration::from_secs(42)));
        assert!(pb.elapsed() >= Duration::from_secs(42));
    }

    #[test]
    fn multi_bar_with_tab_width() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.add(MultiBar::new(10).with_tab_width(4).with_message("a\tb"));
        assert_eq!(pb.tab_width(), 4);
    }

    #[test]
    fn multi_bar_builder_chaining() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.add(
            MultiBar::new(100)
                .with_message("downloading")
                .with_prefix("[1/3]")
                .with_position(25),
        );
        assert_eq!(pb.length(), Some(100));
        assert_eq!(pb.message(), "downloading");
        assert_eq!(pb.prefix(), "[1/3]");
        assert_eq!(pb.position(), 25);
    }

    #[test]
    fn backwards_compat_progress_bar_still_works() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = crate::ProgressBar::new(100);
        let pb = mp.add(pb);
        assert_eq!(pb.length(), Some(100));
    }

    #[test]
    fn multi_bar_with_finish() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.add(MultiBar::new(10).with_finish(ProgressFinish::AndLeave));
        pb.finish();
        assert!(pb.is_finished());
    }

    #[test]
    fn multi_bar_clone() {
        let mb = MultiBar::new(100).with_message("test").with_prefix("pfx");
        let mb2 = mb.clone();
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.add(mb2);
        assert_eq!(pb.message(), "test");
        assert_eq!(pb.prefix(), "pfx");
    }

    #[test]
    fn multi_bar_insert_before_after() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let p0 = mp.add(MultiBar::new(1));
        let p1 = mp.add(MultiBar::new(2));
        let p2 = mp.insert_after(&p1, MultiBar::new(3));
        let p3 = mp.insert_before(&p0, MultiBar::new(4));
        assert_eq!(p0.length(), Some(1));
        assert_eq!(p1.length(), Some(2));
        assert_eq!(p2.length(), Some(3));
        assert_eq!(p3.length(), Some(4));
    }
}
