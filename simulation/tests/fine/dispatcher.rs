//! The dispatcher: every part of a job run once, on threads kept between
//! jobs, borrowing what the caller holds; a part's panic raised to the
//! caller, after every part is done.
//!
//! `cargo test`

use simulation::Dispatcher;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Every part runs once a job, job after job, and a job borrowing the
/// caller's data sees it whole.
#[test]
fn every_part_runs_once_a_job() {
    let dispatcher = Dispatcher::new(4);
    assert_eq!(dispatcher.threads(), 4);
    let runs: Vec<AtomicUsize> = (0..4).map(|_| AtomicUsize::new(0)).collect();
    let numbers: Vec<usize> = (0..1000).collect();
    let sums: Vec<AtomicUsize> = (0..4).map(|_| AtomicUsize::new(0)).collect();
    for _ in 0..100 {
        dispatcher.run(&|part| {
            runs[part].fetch_add(1, Ordering::Relaxed);
            let sum: usize = numbers.iter().skip(part).step_by(4).sum();
            sums[part].store(sum, Ordering::Relaxed);
        });
    }
    assert!(runs.iter().all(|runs| runs.load(Ordering::Relaxed) == 100));
    assert_eq!(sums.iter().map(|sum| sum.load(Ordering::Relaxed)).sum::<usize>(), numbers.iter().sum());
}

/// One thread is the caller's alone.
#[test]
fn one_thread_runs_on_the_caller() {
    let dispatcher = Dispatcher::new(1);
    let caller = std::thread::current().id();
    dispatcher.run(&|part| assert_eq!((part, std::thread::current().id()), (0, caller)));
}

/// A part's panic on a worker reaches the caller, and the dispatcher
/// still runs jobs after.
#[test]
fn a_workers_panic_reaches_the_caller() {
    let dispatcher = Dispatcher::new(3);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        dispatcher.run(&|part| assert_ne!(part, 2, "part 2 fails"));
    }));
    assert!(result.is_err());
    let ran = AtomicUsize::new(0);
    dispatcher.run(&|_| {
        ran.fetch_add(1, Ordering::Relaxed);
    });
    assert_eq!(ran.load(Ordering::Relaxed), 3);
}
