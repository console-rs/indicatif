use crate::progress::ProgressBar;

/// Wraps an iterator to display its progress.
pub trait ProgressIterator
where
    Self: Sized + Iterator,
{
    /// Wrap an iterator with default styling. Attempt to guess iterator
    /// length using `Iterator::size_hint`.
    fn progress(self) -> ProgressBarIter<Self> {
        let n = match self.size_hint() {
            (_, Some(n)) => n as u64,
            _ => 0,
        };
        self.progress_count(n)
    }

    /// Wrap an iterator with an explicit element count.
    fn progress_count(self, len: u64) -> ProgressBarIter<Self> {
        self.progress_with(ProgressBar::new(len))
    }

    /// Wrap an iterator with a custom progress bar.
    fn progress_with(self, progress: ProgressBar) -> ProgressBarIter<Self>;
}

/// Wraps an iterator to display its progress.
pub struct ProgressBarIter<T> {
    it: T,
    progress: ProgressBar,
}

impl<S, T: Iterator<Item = S>> Iterator for ProgressBarIter<T> {
    type Item = S;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.it.next();

        if next.is_some() {
            self.progress.inc(1);
        } else {
            self.progress.finish();
        }

        next
    }
}

impl<S, T: Iterator<Item = S>> ProgressIterator for T {
    fn progress_with(self, progress: ProgressBar) -> ProgressBarIter<Self> {
        ProgressBarIter { it: self, progress }
    }
}

#[cfg(feature = "rayon")]
pub mod rayon_support {
    use super::*;
    use rayon::iter::{
        plumbing::Consumer, plumbing::Folder, plumbing::UnindexedConsumer, ParallelIterator,
    };
    use std::sync::{Arc, Mutex};

    pub struct ParProgressBarIter<T> {
        it: T,
        progress: Arc<Mutex<ProgressBar>>,
    }

    /// Wraps a Rayon parallel iterator.
    ///
    /// See [`ProgressIterator`](trait.ProgressIterator.html) for method
    /// documentation.
    pub trait ParallelProgressIterator
    where
        Self: Sized,
    {
        /// Wrap an iterator with a custom progress bar.
        fn progress_with(self, progress: ProgressBar) -> ParProgressBarIter<Self>;

        /// Wrap an iterator with an explicit element count.
        fn progress_count(self, len: u64) -> ParProgressBarIter<Self> {
            self.progress_with(ProgressBar::new(len))
        }

        /// Wrap an iterator with default styling. Contrary to `std::iter::Iterator`,
        /// `ParallelProgressIterator` does not have a `size_hint` function. Due to this
        /// the resulting progress bar will always show a length of `0` as there is no
        /// way to determine the iterator's length without consuming it in the process.
        fn progress(self) -> ParProgressBarIter<Self> {
            self.progress_count(0)
        }
    }

    impl<S: Send, T: ParallelIterator<Item = S>> ParallelProgressIterator for T {
        fn progress_with(self, progress: ProgressBar) -> ParProgressBarIter<Self> {
            ParProgressBarIter {
                it: self,
                progress: Arc::new(Mutex::new(progress)),
            }
        }
    }

    struct ProgressConsumer<C> {
        base: C,
        progress: Arc<Mutex<ProgressBar>>,
    }

    impl<C> ProgressConsumer<C> {
        fn new(base: C, progress: Arc<Mutex<ProgressBar>>) -> Self {
            ProgressConsumer { base, progress }
        }
    }

    impl<T, C: Consumer<T>> Consumer<T> for ProgressConsumer<C> {
        type Folder = ProgressFolder<C::Folder>;
        type Reducer = C::Reducer;
        type Result = C::Result;

        fn split_at(self, index: usize) -> (Self, Self, Self::Reducer) {
            let (left, right, reducer) = self.base.split_at(index);
            (
                ProgressConsumer::new(left, self.progress.clone()),
                ProgressConsumer::new(right, self.progress.clone()),
                reducer,
            )
        }

        fn into_folder(self) -> Self::Folder {
            ProgressFolder {
                base: self.base.into_folder(),
                progress: self.progress.clone(),
            }
        }

        fn full(&self) -> bool {
            self.base.full()
        }
    }

    impl<T, C: UnindexedConsumer<T>> UnindexedConsumer<T> for ProgressConsumer<C> {
        fn split_off_left(&self) -> Self {
            ProgressConsumer::new(self.base.split_off_left(), self.progress.clone())
        }

        fn to_reducer(&self) -> Self::Reducer {
            self.base.to_reducer()
        }
    }

    struct ProgressFolder<C> {
        base: C,
        progress: Arc<Mutex<ProgressBar>>,
    }

    impl<T, C: Folder<T>> Folder<T> for ProgressFolder<C> {
        type Result = C::Result;

        fn consume(self, item: T) -> Self {
            self.progress.lock().unwrap().inc(1);
            ProgressFolder {
                base: self.base.consume(item),
                progress: self.progress,
            }
        }

        fn complete(self) -> C::Result {
            self.base.complete()
        }

        fn full(&self) -> bool {
            self.base.full()
        }
    }

    impl<S: Send, T: ParallelIterator<Item = S>> ParallelIterator for ParProgressBarIter<T> {
        type Item = S;

        fn drive_unindexed<C: UnindexedConsumer<Self::Item>>(self, consumer: C) -> C::Result {
            let consumer1 = ProgressConsumer::new(consumer, self.progress.clone());
            self.it.drive_unindexed(consumer1)
        }
    }

    #[cfg(test)]
    mod test {
        use super::ParProgressBarIter;
        use crate::iter::rayon_support::ParallelProgressIterator;
        use crate::progress::ProgressBar;
        use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

        #[test]
        fn it_can_wrap_a_parallel_iterator() {
            let v = vec![1, 2, 3];
            let wrap = |it: ParProgressBarIter<_>| {
                assert_eq!(it.map(|x| x * 2).collect::<Vec<_>>(), vec![2, 4, 6]);
            };

            wrap(v.par_iter().progress());
            wrap(v.par_iter().progress_count(3));
            wrap({
                let pb = ProgressBar::new(v.len() as u64);
                v.par_iter().progress_with(pb)
            });
        }
    }
}

#[cfg(test)]
mod test {
    use crate::iter::{ProgressBarIter, ProgressIterator};
    use crate::progress::ProgressBar;

    #[test]
    fn it_can_wrap_an_iterator() {
        let v = vec![1, 2, 3];
        let wrap = |it: ProgressBarIter<_>| {
            assert_eq!(it.map(|x| x * 2).collect::<Vec<_>>(), vec![2, 4, 6]);
        };

        wrap(v.iter().progress());
        wrap(v.iter().progress_count(3));
        wrap({
            let pb = ProgressBar::new(v.len() as u64);
            v.iter().progress_with(pb)
        });
    }
}
