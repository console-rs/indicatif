use progress::ProgressBar;

macro_rules! progress_iterator_trait {
    ($trait:ident , $struct:ident) => {
        pub trait $trait
        where
            Self: Sized,
        {
            fn progress_with(self, progress: ProgressBar) -> $struct<Self>;

            fn progress_count(self, len: u64) -> $struct<Self> {
                self.progress_with(ProgressBar::new(len))
            }

            fn progress(self) -> $struct<Self> {
                self.progress_count(0)
            }
        }
    };
}

progress_iterator_trait! { ProgressIterator, ProgressBarIter }

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

#[cfg(feature = "indicatif-rayon")]
pub mod rayon {
    use super::*;
    use rayon::iter::{
        plumbing::Consumer, plumbing::Folder, plumbing::UnindexedConsumer, ParallelIterator,
    };
    use std::sync::{Arc, Mutex};
    pub struct ParProgressBarIter<T> {
        it: T,
        progress: Arc<Mutex<ProgressBar>>,
    }

    progress_iterator_trait! { ParallelProgressIterator, ParProgressBarIter }

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
        use iter::{rayon::ParallelProgressIterator, ProgressIterator};
        use progress::ProgressBar;
        use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

        #[test]
        fn it_can_wrap_a_parallel_iterator() {
            let v = vec![1, 2, 3];
            let pb = ProgressBar::new(v.len() as u64);
            let w: Vec<_> = v.par_iter().progress_with(pb).map(|x| x * 2).collect();
            assert_eq!(w, vec![2, 4, 6]);
        }

    }
}

#[cfg(test)]
mod test {
    use iter::ProgressIterator;
    use progress::ProgressBar;

    #[test]
    fn it_can_wrap_an_iterator() {
        let v = vec![1, 2, 3];
        let pb = ProgressBar::new(v.len() as u64);
        let w: Vec<_> = v.iter().progress_with(pb).map(|x| x * 2).collect();
        assert_eq!(w, vec![2, 4, 6]);
    }
}
