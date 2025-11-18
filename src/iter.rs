use std::borrow::Cow;
use std::io::{self, IoSliceMut};
use std::iter::FusedIterator;
#[cfg(feature = "tokio")]
use std::pin::Pin;
#[cfg(feature = "tokio")]
use std::task::{Context, Poll};
use std::time::Duration;

#[cfg(feature = "tokio")]
use tokio::io::{ReadBuf, SeekFrom};

use crate::progress_bar::ProgressBar;
use crate::state::ProgressFinish;
use crate::style::ProgressStyle;

/// Wraps an iterator to display its progress.
pub trait ProgressIterator
where
    Self: Sized + Iterator,
{
    /// Wrap an iterator with default styling. Uses [`Iterator::size_hint()`] to get length.
    /// Returns `Some(..)` only if `size_hint.1` is [`Some`]. If you want to create a progress bar
    /// even if `size_hint.1` returns [`None`] use [`progress_count()`](ProgressIterator::progress_count)
    /// or [`progress_with()`](ProgressIterator::progress_with) instead.
    fn try_progress(self) -> Option<ProgressBarIter<Self>> {
        self.size_hint()
            .1
            .map(|len| self.progress_count(u64::try_from(len).unwrap()))
    }

    /// Wrap an iterator with default styling.
    fn progress(self) -> ProgressBarIter<Self>
    where
        Self: ExactSizeIterator,
    {
        let len = u64::try_from(self.len()).unwrap();
        self.progress_count(len)
    }

    /// Wrap an iterator with an explicit element count.
    fn progress_count(self, len: u64) -> ProgressBarIter<Self> {
        self.progress_with(ProgressBar::new(len))
    }

    /// Wrap an iterator with a custom progress bar.
    fn progress_with(self, progress: ProgressBar) -> ProgressBarIter<Self>;

    /// Wrap an iterator with a progress bar and style it.
    fn progress_with_style(self, style: crate::ProgressStyle) -> ProgressBarIter<Self>
    where
        Self: ExactSizeIterator,
    {
        let len = u64::try_from(self.len()).unwrap();
        let bar = ProgressBar::new(len).with_style(style);
        self.progress_with(bar)
    }
}

/// Wraps an iterator to display its progress.
#[derive(Debug)]
pub struct ProgressBarIter<T> {
    pub(crate) it: T,
    pub progress: ProgressBar,
    pub(crate) seek_max: SeekMax,
}

impl<T> ProgressBarIter<T> {
    /// Builder-like function for setting underlying progress bar's style.
    ///
    /// See [`ProgressBar::with_style()`].
    pub fn with_style(mut self, style: ProgressStyle) -> Self {
        self.progress = self.progress.with_style(style);
        self
    }

    /// Builder-like function for setting underlying progress bar's prefix.
    ///
    /// See [`ProgressBar::with_prefix()`].
    pub fn with_prefix(mut self, prefix: impl Into<Cow<'static, str>>) -> Self {
        self.progress = self.progress.with_prefix(prefix);
        self
    }

    /// Builder-like function for setting underlying progress bar's message.
    ///
    /// See [`ProgressBar::with_message()`].
    pub fn with_message(mut self, message: impl Into<Cow<'static, str>>) -> Self {
        self.progress = self.progress.with_message(message);
        self
    }

    /// Builder-like function for setting underlying progress bar's position.
    ///
    /// See [`ProgressBar::with_position()`].
    pub fn with_position(mut self, position: u64) -> Self {
        self.progress = self.progress.with_position(position);
        self
    }

    /// Builder-like function for setting underlying progress bar's elapsed time.
    ///
    /// See [`ProgressBar::with_elapsed()`].
    pub fn with_elapsed(mut self, elapsed: Duration) -> Self {
        self.progress = self.progress.with_elapsed(elapsed);
        self
    }

    /// Builder-like function for setting underlying progress bar's finish behavior.
    ///
    /// See [`ProgressBar::with_finish()`].
    pub fn with_finish(mut self, finish: ProgressFinish) -> Self {
        self.progress = self.progress.with_finish(finish);
        self
    }
}

impl<S, T: Iterator<Item = S>> Iterator for ProgressBarIter<T> {
    type Item = S;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.it.next();

        if item.is_some() {
            self.progress.inc(1);
        } else if !self.progress.is_finished() {
            self.progress.finish_using_style();
        }

        item
    }
}

impl<T: ExactSizeIterator> ExactSizeIterator for ProgressBarIter<T> {
    fn len(&self) -> usize {
        self.it.len()
    }
}

impl<T: DoubleEndedIterator> DoubleEndedIterator for ProgressBarIter<T> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let item = self.it.next_back();

        if item.is_some() {
            self.progress.inc(1);
        } else if !self.progress.is_finished() {
            self.progress.finish_using_style();
        }

        item
    }
}

impl<T: FusedIterator> FusedIterator for ProgressBarIter<T> {}

impl<R: io::Read> io::Read for ProgressBarIter<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let inc = self.it.read(buf)?;
        self.progress.set_position(
            self.seek_max
                .update_seq(self.progress.position(), inc as u64),
        );
        Ok(inc)
    }

    fn read_vectored(&mut self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        let inc = self.it.read_vectored(bufs)?;
        self.progress.set_position(
            self.seek_max
                .update_seq(self.progress.position(), inc as u64),
        );
        Ok(inc)
    }

    fn read_to_string(&mut self, buf: &mut String) -> io::Result<usize> {
        let inc = self.it.read_to_string(buf)?;
        self.progress.set_position(
            self.seek_max
                .update_seq(self.progress.position(), inc as u64),
        );
        Ok(inc)
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        self.it.read_exact(buf)?;
        self.progress.set_position(
            self.seek_max
                .update_seq(self.progress.position(), buf.len() as u64),
        );
        Ok(())
    }
}

impl<R: io::BufRead> io::BufRead for ProgressBarIter<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.it.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.it.consume(amt);
        self.progress.set_position(
            self.seek_max
                .update_seq(self.progress.position(), amt.try_into().unwrap()),
        );
    }
}

impl<S: io::Seek> io::Seek for ProgressBarIter<S> {
    fn seek(&mut self, f: io::SeekFrom) -> io::Result<u64> {
        self.it.seek(f).map(|pos| {
            if f != io::SeekFrom::Current(0) {
                // this kind of seek is used to find the current position, but does not alter it
                // generally equivalent to stream_position()
                self.progress.set_position(self.seek_max.update_seek(pos));
            }

            pos
        })
    }
    // Pass this through to preserve optimizations that the inner I/O object may use here
    // Also avoid sending a set_position update when the position hasn't changed
    fn stream_position(&mut self) -> io::Result<u64> {
        self.it.stream_position()
    }
}

/// Calculates a more stable visual position from jittery seeks to show to the user.
///
/// It does so by holding the maximum position encountered out of the last HISTORY read/write positions.
/// As an optimization it deallocates the history when only sequential operations are performed RESET times in a row.
#[derive(Debug, Default)]
pub(crate) struct SeekMax<const RESET: u8 = 5, const HISTORY: usize = 10> {
    buf: Option<(Box<MaxRingBuf<HISTORY>>, u8)>,
}

impl<const RESET: u8, const HISTORY: usize> SeekMax<RESET, HISTORY> {
    fn update_seq(&mut self, prev_pos: u64, delta: u64) -> u64 {
        let new_pos = prev_pos + delta;
        let Some((buf, seq)) = &mut self.buf else {
            return new_pos;
        };

        *seq += 1;
        if *seq >= RESET {
            self.buf = None;
            return new_pos;
        }

        buf.update(new_pos);
        buf.max()
    }

    fn update_seek(&mut self, newpos: u64) -> u64 {
        let (b, seq) = self
            .buf
            .get_or_insert_with(|| (Box::new(MaxRingBuf::<HISTORY>::default()), 0));
        *seq = 0;
        b.update(newpos);
        b.max()
    }
}

/// Ring buffer that remembers the maximum contained value.
///
/// can be used to quickly calculate the maximum value of a history of data points.
#[derive(Debug)]
struct MaxRingBuf<const HISTORY: usize = 10> {
    history: [u64; HISTORY],
    // invariant_h: always a valid index into history
    head: u8,
    // invariant_m: always a valid index into history
    max_pos: u8,
}

impl<const HISTORY: usize> MaxRingBuf<HISTORY> {
    /// Adds a value to the history.
    /// Updates internal bookkeeping to remember the maximum value.
    ///
    /// # Performance:
    /// amortized O(1):
    /// each regular update is O(1).
    /// Only updates that overwrite the position the maximum was stored in with a smaller number do a seek of the buffer,
    /// searching for the new maximum.
    /// This only happens on average each 1/HISTORY and has a cost of HISTORY,
    /// therefore amortizing to O(1).
    ///
    /// In case there is some linear increase with jitter,
    ///   as expected in this specific use-case,
    /// as long as there is one bigger update each HISTORY updates the scan is never triggered at all.
    ///
    /// Worst case would be linearly decreasing values, which is still O(1).
    fn update(&mut self, new: u64) {
        // exploit invariant_h to eliminate bounds checks & panic code path
        let head = usize::from(self.head) % self.history.len();
        // exploit invariant_m to eliminate bounds checks & panic code path
        let max_pos = usize::from(self.max_pos) % self.history.len();

        // save max now in case it gets overwritten in the next line
        let prev_max = self.history[max_pos];
        self.history[head] = new;

        if new > prev_max {
            // This is now the new maximum
            self.max_pos = self.head;
        } else if self.max_pos == self.head && new < prev_max {
            // This was the maximum and may not be anymore
            // do a linear seek to find the new maximum
            let (idx, _val) = self
                .history
                .iter()
                .enumerate()
                .max_by_key(|(_, v)| *v)
                .expect("array has fixded size > 0");
            // invariant_m: idx is from an enumeration of history
            self.max_pos = idx.try_into().expect("history.len() <= u8::MAX");
        }

        // invariant_h: head is kept in bounds by %-ing with history.len()
        //     it is a ring buffer so wrapping around is expected behaviour.
        self.head = (self.head + 1) % (self.history.len() as u8);
    }

    /// Returns the maximum value out of the memorized entries
    fn max(&self) -> u64 {
        // exploit invariant_m to eliminate bounds checks & panic code path
        self.history[self.max_pos as usize % self.history.len()]
    }
}

impl<const HISTORY: usize> Default for MaxRingBuf<HISTORY> {
    fn default() -> Self {
        assert!(HISTORY <= u8::MAX.into());
        assert!(HISTORY > 0);
        Self {
            history: [0; HISTORY],
            // invariant_h: we asserted that history has at least one element, therefore index 0 is valid
            head: 0,
            // invariant_m: we asserted that history has at least one element, therefore index 0 is valid
            max_pos: 0,
        }
    }
}

#[cfg(feature = "tokio")]
#[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
impl<W: tokio::io::AsyncWrite + Unpin> tokio::io::AsyncWrite for ProgressBarIter<W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.it).poll_write(cx, buf).map(|poll| {
            poll.map(|inc| {
                let oldprog = self.progress.position();
                let newprog = self.seek_max.update_seq(oldprog, inc.try_into().unwrap());
                self.progress.set_position(newprog);
                inc
            })
        })
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.it).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.it).poll_shutdown(cx)
    }
}

#[cfg(feature = "tokio")]
#[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
impl<W: tokio::io::AsyncRead + Unpin> tokio::io::AsyncRead for ProgressBarIter<W> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let prev_len = buf.filled().len() as u64;
        let poll = Pin::new(&mut self.it).poll_read(cx, buf);
        if let Poll::Ready(_e) = &poll {
            let inc = buf.filled().len() as u64 - prev_len;
            let oldprog = self.progress.position();
            let newprog = self.seek_max.update_seq(oldprog, inc);
            self.progress.set_position(newprog);
        }
        poll
    }
}

#[cfg(feature = "tokio")]
#[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
impl<W: tokio::io::AsyncSeek + Unpin> tokio::io::AsyncSeek for ProgressBarIter<W> {
    fn start_seek(mut self: Pin<&mut Self>, position: SeekFrom) -> io::Result<()> {
        Pin::new(&mut self.it).start_seek(position)
    }

    fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        let poll = Pin::new(&mut self.it).poll_complete(cx);
        if let Poll::Ready(Ok(pos)) = &poll {
            let newpos = self.seek_max.update_seek(*pos);
            self.progress.set_position(newpos);
        }

        poll
    }
}

#[cfg(feature = "tokio")]
#[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
impl<W: tokio::io::AsyncBufRead + Unpin + tokio::io::AsyncRead> tokio::io::AsyncBufRead
    for ProgressBarIter<W>
{
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        let this = self.get_mut();
        Pin::new(&mut this.it).poll_fill_buf(cx)
    }

    fn consume(mut self: Pin<&mut Self>, amt: usize) {
        Pin::new(&mut self.it).consume(amt);
        let oldprog = self.progress.position();
        let newprog = self.seek_max.update_seq(oldprog, amt.try_into().unwrap());
        self.progress.set_position(newprog);
    }
}

#[cfg(feature = "futures")]
#[cfg_attr(docsrs, doc(cfg(feature = "futures")))]
impl<S: futures_core::Stream + Unpin> futures_core::Stream for ProgressBarIter<S> {
    type Item = S::Item;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        let item = std::pin::Pin::new(&mut this.it).poll_next(cx);
        match &item {
            std::task::Poll::Ready(Some(_)) => this.progress.inc(1),
            std::task::Poll::Ready(None) => this.progress.finish_using_style(),
            std::task::Poll::Pending => {}
        }
        item
    }
}

impl<W: io::Write> io::Write for ProgressBarIter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.it.write(buf).map(|inc| {
            self.progress.set_position(
                self.seek_max
                    .update_seq(self.progress.position(), inc as u64),
            );
            inc
        })
    }

    fn write_vectored(&mut self, bufs: &[io::IoSlice]) -> io::Result<usize> {
        self.it.write_vectored(bufs).map(|inc| {
            self.progress.set_position(
                self.seek_max
                    .update_seq(self.progress.position(), inc as u64),
            );
            inc
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        self.it.flush()
    }

    // write_fmt can not be captured with reasonable effort.
    // as it uses write_all internally by default that should not be a problem.
    // fn write_fmt(&mut self, fmt: fmt::Arguments) -> io::Result<()>;
}

impl<S, T: Iterator<Item = S>> ProgressIterator for T {
    fn progress_with(self, progress: ProgressBar) -> ProgressBarIter<Self> {
        ProgressBarIter {
            it: self,
            progress,
            seek_max: SeekMax::default(),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::iter::{ProgressBarIter, ProgressIterator};
    use crate::progress_bar::ProgressBar;
    use crate::ProgressStyle;

    #[test]
    fn it_can_wrap_an_iterator() {
        let v = [1, 2, 3];
        let wrap = |it: ProgressBarIter<_>| {
            assert_eq!(it.map(|x| x * 2).collect::<Vec<_>>(), vec![2, 4, 6]);
        };

        wrap(v.iter().progress());
        wrap(v.iter().progress_count(3));
        wrap({
            let pb = ProgressBar::new(v.len() as u64);
            v.iter().progress_with(pb)
        });
        wrap({
            let style = ProgressStyle::default_bar()
                .template("{wide_bar:.red} {percent}/100%")
                .unwrap();
            v.iter().progress_with_style(style)
        });
    }

    #[test]
    fn test_max_ring_buf() {
        use crate::iter::MaxRingBuf;
        let mut max = MaxRingBuf::<10>::default();
        max.update(100);
        assert_eq!(max.max(), 100);
        for i in 0..10 {
            max.update(99 - i);
        }
        assert_eq!(max.max(), 99);
    }
}
