use std::borrow::Cow;
use std::fmt;
use std::time::Duration;

use crate::draw_target::ProgressDrawTarget;
use crate::progress_bar::ProgressBar;
use crate::state::ProgressFinish;
use crate::style::ProgressStyle;

/// A deferred progress bar configuration for use with [`MultiProgress`].
///
/// Unlike [`ProgressBar`], a `ProgressBarBuilder` never draws to the terminal on
/// its own. Configuration is captured and applied only when the builder is
/// registered with a [`MultiProgress`] via methods like
/// [`MultiProgress::register`].
///
/// This avoids a common footgun where a [`ProgressBar`] is configured (triggering
/// premature draws to stderr) before being added to a [`MultiProgress`], causing
/// screen corruption. See [#677] for details.
///
/// Dropping a `ProgressBarBuilder` without registering it is a no-op — no
/// resources are allocated until materialization.
///
/// # Migration
///
/// Passing a [`ProgressBar`] directly to [`MultiProgress::add`] is deprecated.
/// To migrate, replace:
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
/// let pb = mp.register(
///     ProgressBarBuilder::new(100)
///         .with_message("downloading")
///         .with_steady_tick(Duration::from_millis(100))
/// );
/// ```
///
/// ## Behavior difference: style colors for non-stderr draw targets
///
/// When migrating code that configures a style on a [`MultiProgress`] whose
/// draw target is **not** stderr (e.g. [`ProgressDrawTarget::term_like`]),
/// there is a subtle behavior difference. The old path created a
/// [`ProgressBar`] that defaulted to stderr, then applied the style via
/// [`ProgressBar::set_style`]; because that method's stderr-color handling
/// checks the pb's *current* draw target, a freshly-constructed bar always
/// took the stderr branch and the stderr-color variant of the style was
/// applied before the remote draw target was swapped in. The new path reads
/// the MultiProgress's draw target to decide whether to apply stderr color
/// handling, so custom draw targets get their natural (non-stderr) color
/// treatment. This is usually the desired behavior, but it can produce
/// different colors than the deprecated path for the same style.
///
/// [`ProgressDrawTarget::term_like`]: crate::ProgressDrawTarget::term_like
/// [`ProgressBar::set_style`]: crate::ProgressBar::set_style
///
/// # Example
///
/// ```rust
/// use std::time::Duration;
/// use indicatif::{ProgressBarBuilder, MultiProgress, ProgressStyle};
///
/// let mp = MultiProgress::new();
/// let pb = mp.register(
///     ProgressBarBuilder::new(100)
///         .with_style(ProgressStyle::with_template("{bar:40} {pos}/{len} {msg}").unwrap())
///         .with_message("downloading")
///         .with_steady_tick(Duration::from_millis(100))
/// );
/// ```
///
/// [`MultiProgress`]: crate::MultiProgress
/// [`MultiProgress::register`]: crate::MultiProgress::register
/// [`MultiProgress::add`]: crate::MultiProgress::add
/// [`ProgressBar`]: crate::ProgressBar
/// [#677]: https://github.com/console-rs/indicatif/issues/677
#[derive(Clone)]
pub struct ProgressBarBuilder {
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
impl fmt::Debug for ProgressBarBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ProgressBarBuilder")
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

impl ProgressBarBuilder {
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

    /// Creates a new `ProgressBarBuilder` with a given length.
    pub fn new(len: u64) -> Self {
        Self {
            len: Some(len),
            ..Self::base()
        }
    }

    /// Creates a new `ProgressBarBuilder` without a specified length.
    pub fn no_length() -> Self {
        Self::base()
    }

    /// Creates a new spinner-style `ProgressBarBuilder` (no length, default spinner style).
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
    /// The tick thread will only be started when this builder is registered
    /// with a [`MultiProgress`].
    ///
    /// [`MultiProgress`]: crate::MultiProgress
    pub fn with_steady_tick(mut self, interval: Duration) -> Self {
        self.steady_tick = Some(interval);
        self
    }

    /// Build an unregistered [`ProgressBar`] with a hidden draw target and return
    /// it along with the optional steady_tick interval to apply later.
    ///
    /// This is **not** a user-facing "build a hidden progress bar" helper — it is
    /// the panic-safe phase of materialization used internally by
    /// [`MultiProgress::register`]. It applies all builder configuration (any of
    /// which may panic — e.g. [`ProgressBar::with_elapsed`] panics if the elapsed
    /// duration exceeds the time since system boot) BEFORE the [`MultiProgress`]
    /// reserves a slot for the bar, so a panic here cannot leak a slot. The
    /// caller is expected to reserve a slot, then call
    /// [`ProgressBar::set_draw_target`] and [`ProgressBar::enable_steady_tick`]
    /// to complete materialization.
    ///
    /// `is_stderr` indicates whether the [`MultiProgress`]'s draw target is stderr,
    /// used for style color detection. Another thread could call
    /// [`MultiProgress::set_draw_target`] after we read `is_stderr` but before the
    /// style is applied, making the stderr detection stale. This matches the
    /// TOCTOU window that exists on the deprecated [`MultiProgress::add`] path.
    ///
    /// [`MultiProgress`]: crate::MultiProgress
    /// [`MultiProgress::register`]: crate::MultiProgress::register
    /// [`MultiProgress::add`]: crate::MultiProgress::add
    /// [`MultiProgress::set_draw_target`]: crate::MultiProgress::set_draw_target
    pub(crate) fn build_unregistered(self, is_stderr: bool) -> (ProgressBar, Option<Duration>) {
        // PARITY: every `ProgressBar::with_*` method that has a corresponding
        // field on `ProgressBarBuilder` must also be applied here. When adding a
        // new `with_*` method to `ProgressBar`, mirror it onto `ProgressBarBuilder`
        // and extend this chain. There is no compile-time enforcement of this
        // parity.
        //
        // ORDER (defense-in-depth, not strictly load-bearing today):
        // `BarState::set_tab_width` currently propagates the new tab_width into
        // the existing prefix/message/style, and `BarState::set_style` re-applies
        // `state.tab_width` to the new style — so swapping the order of
        // `with_tab_width`/`with_style`/`with_prefix`/`with_message` would
        // produce the same output under today's implementation. We keep
        // tab_width first anyway so a future refactor of either `set_tab_width`
        // or `set_style` that drops the cross-propagation doesn't silently
        // change tab-expansion behavior.

        // 1. Create with a hidden target (no draws possible during construction).
        let pb = ProgressBar::with_draw_target(self.len, ProgressDrawTarget::hidden());

        // 2. Apply tab_width first so subsequent steps that read `state.tab_width`
        //    (prefix/message tab expansion, style tab_width) observe the user value.
        let pb = if let Some(tw) = self.tab_width {
            pb.with_tab_width(tw)
        } else {
            pb
        };

        // 3. Apply style, adjusting for stderr color detection if needed.
        let pb = if let Some(mut style) = self.style {
            if is_stderr {
                style.set_for_stderr();
            }
            pb.with_style(style)
        } else {
            pb
        };

        // 4. Apply remaining configuration.
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

        (pb, self.steady_tick)
    }
}

#[cfg(test)]
mod tests {
    use crate::multi::MultiProgress;
    use crate::style::ProgressStyle;

    use super::*;

    #[test]
    fn builder_new_sets_length() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.register(ProgressBarBuilder::new(42));
        assert_eq!(pb.length(), Some(42));
    }

    #[test]
    fn builder_no_length() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.register(ProgressBarBuilder::no_length());
        assert_eq!(pb.length(), None);
    }

    #[test]
    fn builder_new_spinner_has_no_length() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.register(ProgressBarBuilder::new_spinner());
        assert_eq!(pb.length(), None);
    }

    #[test]
    fn builder_with_message() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.register(ProgressBarBuilder::new(10).with_message("hello"));
        assert_eq!(pb.message(), "hello");
    }

    #[test]
    fn builder_with_prefix() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.register(ProgressBarBuilder::new(10).with_prefix("[1/3]"));
        assert_eq!(pb.prefix(), "[1/3]");
    }

    #[test]
    fn builder_with_position() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.register(ProgressBarBuilder::new(100).with_position(50));
        assert_eq!(pb.position(), 50);
    }

    #[test]
    fn builder_with_style() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let style = ProgressStyle::with_template("{msg}").unwrap();
        let pb = mp.register(ProgressBarBuilder::new(10).with_style(style));
        pb.set_message("test");
        assert_eq!(pb.message(), "test");
    }

    #[test]
    fn builder_with_elapsed() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.register(ProgressBarBuilder::new(100).with_elapsed(Duration::from_secs(42)));
        assert!(pb.elapsed() >= Duration::from_secs(42));
    }

    #[test]
    fn builder_with_tab_width() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.register(
            ProgressBarBuilder::new(10)
                .with_tab_width(4)
                .with_message("a\tb"),
        );
        assert_eq!(pb.tab_width(), 4);
    }

    #[test]
    fn builder_chaining() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.register(
            ProgressBarBuilder::new(100)
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
    #[allow(deprecated)]
    fn backwards_compat_progress_bar_still_works() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = crate::ProgressBar::new(100);
        let pb = mp.add(pb);
        assert_eq!(pb.length(), Some(100));
    }

    #[test]
    fn builder_with_finish() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.register(ProgressBarBuilder::new(10).with_finish(ProgressFinish::AndLeave));
        pb.finish();
        assert!(pb.is_finished());
    }

    #[test]
    fn builder_clone() {
        let builder = ProgressBarBuilder::new(100)
            .with_message("test")
            .with_prefix("pfx");
        let builder2 = builder.clone();
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let pb = mp.register(builder2);
        assert_eq!(pb.message(), "test");
        assert_eq!(pb.prefix(), "pfx");
    }

    #[test]
    fn builder_register_before_after() {
        let mp = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());
        let p0 = mp.register(ProgressBarBuilder::new(1));
        let p1 = mp.register(ProgressBarBuilder::new(2));
        let p2 = mp.register_after(&p1, ProgressBarBuilder::new(3));
        let p3 = mp.register_before(&p0, ProgressBarBuilder::new(4));
        assert_eq!(p0.length(), Some(1));
        assert_eq!(p1.length(), Some(2));
        assert_eq!(p2.length(), Some(3));
        assert_eq!(p3.length(), Some(4));
    }
}
