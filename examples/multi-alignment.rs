use std::time::Duration;

use indicatif::{
    MultiProgress, MultiProgressAlignment, ProgressBar, ProgressDrawTarget, ProgressStyle,
};
use std::collections::HashSet;
use std::fmt::Debug;

#[derive(Clone, Debug)]
struct Elem {
    key: usize,
    msg: String,
    progress_bar: Option<ProgressBar>,
}

fn main() {
    let n = ELEMS.len() - 5;
    let multi_progress = MultiProgress::new();
    multi_progress.set_move_cursor(false);
    multi_progress.set_alignment(MultiProgressAlignment::Bottom);
    multi_progress.set_draw_target(ProgressDrawTarget::stdout_nohz());

    let sty_main = ProgressStyle::default_bar().template("{bar:40.green/yellow} {pos:>4}/{len:4}");
    let sty_aux = ProgressStyle::default_bar().template("{spinner:.green} {msg}");
    let pb_main = multi_progress.add(ProgressBar::new(n as u64));
    pb_main.set_style(sty_main);
    pb_main.tick();

    let mut current_elems = vec![];
    for i in 0..n {
        let fresh_elems = make_fresh_elems(i);
        update_current_elems(&mut current_elems, &fresh_elems, &multi_progress, &sty_aux);

        for _ in 0..6 {
            for ce in &current_elems {
                if let Some(pb) = &ce.progress_bar {
                    pb.inc(1);
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        pb_main.inc(1);
    }
    for ce in &current_elems {
        if let Some(pb) = &ce.progress_bar {
            pb.finish();
        }
    }
    pb_main.finish();
}

fn make_fresh_elems(idx: usize) -> Vec<Elem> {
    let lengths = [1, 2, 3, 4, 5, 6, 7, 5, 3];
    let len = lengths[idx % lengths.len()];
    (idx..(idx + len))
        .into_iter()
        .map(|i| Elem {
            key: i,
            msg: ELEMS[i].to_string(),
            progress_bar: None,
        })
        .collect()
}

fn update_current_elems(
    current_elems: &mut Vec<Elem>,
    fresh_elems: &[Elem],
    multi_progress: &MultiProgress,
    sty_aux: &ProgressStyle,
) {
    let fresh_keys = fresh_elems.iter().map(|e| e.key).collect::<HashSet<_>>();
    let curr_min = current_elems
        .iter()
        .map(|e| e.key as i32)
        .min()
        .unwrap_or(current_elems.len() as i32);
    let curr_max = current_elems
        .iter()
        .map(|e| e.key as i32)
        .max()
        .unwrap_or(-1);
    let i0 = current_elems
        .iter()
        .enumerate()
        .find_map(|(i, e)| {
            if fresh_keys.contains(&e.key) {
                Some(i)
            } else {
                None
            }
        })
        .unwrap_or(0);
    let i1 = current_elems[i0..]
        .iter()
        .enumerate()
        .find_map(|(i, e)| {
            if !fresh_keys.contains(&e.key) {
                Some(i)
            } else {
                None
            }
        })
        .unwrap_or(current_elems.len());
    let j0 = fresh_elems
        .iter()
        .enumerate()
        .find_map(|(j, e)| {
            if curr_min <= e.key as i32 {
                Some(j)
            } else {
                None
            }
        })
        .unwrap_or(fresh_elems.len());
    let j1 = fresh_elems[j0..]
        .iter()
        .enumerate()
        .find_map(|(j, e)| {
            if curr_max < e.key as i32 {
                Some(j0 + j)
            } else {
                None
            }
        })
        .unwrap_or(fresh_elems.len());

    let to_remove = (0..i0)
        .chain(i1..(current_elems.len() - i0))
        .collect::<Vec<_>>();
    let to_insert = (0..j0).chain(j1..fresh_elems.len()).collect::<Vec<_>>();
    let to_change = (j0..j1).collect::<Vec<_>>();

    for _ in to_remove {
        let e = current_elems.remove(0);
        if let Some(pb) = e.progress_bar {
            multi_progress.remove(&pb);
        }
    }
    for j in to_insert {
        current_elems.insert(j, fresh_elems[j].clone());
        let e = &mut current_elems[j];
        let pb = multi_progress.insert(j, ProgressBar::new(1_000));
        pb.set_style(sty_aux.clone());
        pb.set_message(e.msg.clone());
        e.progress_bar = Some(pb);
    }
    for j in to_change {
        let e = fresh_elems[j].clone();
        current_elems[j].key = e.key;
        current_elems[j].msg = e.msg.clone();
        if let Some(pb) = &current_elems[j].progress_bar {
            pb.set_message(e.msg);
        }
    }
}

const ELEMS: [&str; 60] = [
    "Lorem",
    "Ipsum",
    "is",
    "simply",
    "dummy",
    "text",
    "of",
    "the",
    "printing",
    "and",
    "typesetting",
    "industry.",
    "Lorem",
    "Ipsum",
    "has",
    "been",
    "the",
    "industry's",
    "standard",
    "dummy",
    "text",
    "ever",
    "since",
    "the",
    "1500s,",
    "when",
    "an",
    "unknown",
    "printer",
    "took",
    "a",
    "galley",
    "of",
    "type",
    "and",
    "scrambled",
    "it",
    "to",
    "make",
    "a",
    "type",
    "specimen",
    "book.",
    "It",
    "has",
    "survived",
    "not",
    "only",
    "five",
    "centuries,",
    "but",
    "also",
    "the",
    "leap",
    "into",
    "electronic",
    "typesetting,",
    "remaining",
    "essentially",
    "unchanged.",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_both() {
        let xs = vec![];
        let ys = vec![];
        run_test(xs, ys);
    }

    #[test]
    fn empty_current() {
        let xs = vec![];
        let ys = vec![0, 1, 2];
        run_test(xs, ys);
    }

    #[test]
    fn empty_fresh() {
        let xs = vec![1, 2, 3];
        let ys = vec![];
        run_test(xs, ys);
    }
    #[test]
    fn non_overlapping_prepend() {
        let xs = (vec![10, 11, 12]);
        let ys = (vec![1, 2, 3, 4]);
        run_test(xs, ys);
    }
    #[test]
    fn non_overlapping_append() {
        let xs = vec![10, 11, 12];
        let ys = vec![13, 14, 15, 16];
        run_test(xs, ys);
    }
    #[test]
    fn overlapping_prepend() {
        let xs = vec![12, 13, 14];
        let ys = vec![10, 11, 12, 13];
        run_test(xs, ys);
    }
    #[test]
    fn overlapping_append() {
        let xs = vec![10, 11, 12];
        let ys = vec![11, 12, 13, 14];
        run_test(xs, ys);
    }
    #[test]
    fn current_contains_fresh() {
        let xs = vec![10, 11, 12, 13, 14];
        let ys = vec![11, 12, 13];
        run_test(xs, ys);
    }
    #[test]
    fn current_is_contained_in_fresh() {
        let xs = vec![12, 13, 14];
        let ys = vec![10, 11, 12, 13, 14, 15, 16];
        run_test(xs, ys);
    }

    fn run_test(xs: Vec<usize>, ys: Vec<usize>) {
        let mp = MultiProgress::new();
        let sty = ProgressStyle::default_spinner();
        let mut current = populate(xs);
        let fresh = populate(ys.clone());
        update_current_elems(&mut current, &fresh, &mp, &sty);
        assert_eq!(current, populate(ys));
    }

    fn populate(xs: Vec<usize>) -> Vec<Elem> {
        xs.into_iter()
            .map(|k| Elem {
                key: k,
                msg: ELEMS[k].to_string(),
                progress_bar: None,
            })
            .collect()
    }
}
