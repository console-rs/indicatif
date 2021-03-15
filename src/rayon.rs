use crate::{ProgressBar, ProgressBarIter};
use rayon::iter::{
    plumbing::Consumer, plumbing::Folder, plumbing::UnindexedConsumer, IndexedParallelIterator,
    ParallelIterator,
};
use std::convert::TryFrom;

/// Wraps a Rayon parallel iterator.
///
/// See [`ProgressIterator`](trait.ProgressIterator.html) for method
/// documentation.
pub trait ParallelProgressIterator
    where
        Self: Sized + ParallelIterator,
{
    /// Wrap an iterator with a custom progress bar.
    fn progress_with(self, progress: ProgressBar) -> ProgressBarIter<Self>;

    /// Wrap an iterator with an explicit element count.
    fn progress_count(self, len: u64) -> ProgressBarIter<Self> {
        self.progress_with(ProgressBar::new(len))
    }

    fn progress(self) -> ProgressBarIter<Self>
        where
            Self: IndexedParallelIterator,
    {
        let len = u64::try_from(self.len()).unwrap();
        self.progress_count(len)
    }
}

impl<S: Send, T: ParallelIterator<Item = S>> ParallelProgressIterator for T {
    fn progress_with(self, progress: ProgressBar) -> ProgressBarIter<Self> {
        ProgressBarIter { it: self, progress }
    }
}

struct ProgressConsumer<C> {
    base: C,
    progress: ProgressBar,
}

impl<C> ProgressConsumer<C> {
    fn new(base: C, progress: ProgressBar) -> Self {
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
            ProgressConsumer::new(right, self.progress),
            reducer,
        )
    }

    fn into_folder(self) -> Self::Folder {
        ProgressFolder {
            base: self.base.into_folder(),
            progress: self.progress,
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
    progress: ProgressBar,
}

impl<T, C: Folder<T>> Folder<T> for ProgressFolder<C> {
    type Result = C::Result;

    fn consume(self, item: T) -> Self {
        self.progress.inc(1);
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

impl<S: Send, T: ParallelIterator<Item = S>> ParallelIterator for ProgressBarIter<T> {
    type Item = S;

    fn drive_unindexed<C: UnindexedConsumer<Self::Item>>(self, consumer: C) -> C::Result {
        let consumer1 = ProgressConsumer::new(consumer, self.progress.clone());
        self.it.drive_unindexed(consumer1)
    }
}

#[cfg(test)]
mod test {
    use crate::{ProgressBar, ProgressBarIter, ParallelProgressIterator};
    use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

    #[test]
    fn it_can_wrap_a_parallel_iterator() {
        let v = vec![1, 2, 3];
        fn wrap<'a, T: ParallelIterator<Item = &'a i32>>(it: ProgressBarIter<T>) {
            assert_eq!(it.map(|x| x * 2).collect::<Vec<_>>(), vec![2, 4, 6]);
        }

        wrap(v.par_iter().progress_count(3));
        wrap({
            let pb = ProgressBar::new(v.len() as u64);
            v.par_iter().progress_with(pb)
        });
    }
}
