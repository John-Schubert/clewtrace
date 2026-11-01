//! The burst timer of the project watcher (CT-EFF-008). It is the only first-party code
//! outside tests and probes that waits with a timeout, and it is allowed to by name in
//! `tests/idle_cpu.rs`: the timeout runs only from the first event of a burst to its
//! end. While no files change, the thread blocks in `recv`, which has no timeout.

use std::collections::BTreeSet;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use super::{Batch, Seen};

// @implements CT-EFF-008, CT-EFF-007
/// Gathers what `seen` receives into bursts and passes each to `on_batch`. It sleeps in
/// `recv` until the first event of a burst, with no timer. The burst then ends once no
/// event has come for `quiet`, or after `longest`. Returns when the sender is dropped.
pub(super) fn debounce(
    seen: &Receiver<Seen>,
    quiet: Duration,
    longest: Duration,
    mut on_batch: impl FnMut(Batch),
) {
    while let Ok(first) = seen.recv() {
        let mut batch = Batch::default();
        let mut paths = BTreeSet::new();
        let mut add = |s: Seen, batch: &mut Batch| match s {
            Seen::Paths(p) => paths.extend(p),
            Seen::Everything => batch.everything = true,
            Seen::Folder(f) => {
                batch.everything = true;
                batch.folders.push(f);
            }
            Seen::Trouble(t) => {
                if !batch.trouble.contains(&t) {
                    batch.trouble.push(t);
                }
            }
        };
        add(first, &mut batch);
        let started = Instant::now();
        let mut open = true;
        loop {
            let left = longest.saturating_sub(started.elapsed());
            if left.is_zero() {
                break;
            }
            match seen.recv_timeout(quiet.min(left)) {
                Ok(s) => add(s, &mut batch),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    open = false;
                    break;
                }
            }
        }
        batch.paths = paths.into_iter().collect();
        on_batch(batch);
        if !open {
            return;
        }
    }
}

// @verifies CT-EFF-008, CT-EFF-007
#[cfg(test)]
mod tests {
    use super::*;
    use crate::watch::out_of_watches;
    use std::sync::mpsc;

    // @verifies CT-EFF-008
    #[test]
    fn a_burst_of_events_is_one_batch_after_the_burst_ends() {
        let (tx, rx) = mpsc::channel();
        let (out, got) = mpsc::channel();
        let quiet = Duration::from_millis(80);
        let thread = std::thread::spawn(move || {
            debounce(&rx, quiet, Duration::from_secs(5), |b| out.send(b).unwrap());
        });
        for name in ["a", "b", "a"] {
            tx.send(Seen::Paths(vec![name.into()])).unwrap();
            std::thread::sleep(Duration::from_millis(10));
        }
        let batch = got.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(batch.paths, ["a", "b"]);
        assert!(!batch.everything);
        assert!(got.recv_timeout(quiet * 3).is_err(), "one batch per burst");
        drop(tx);
        thread.join().unwrap();
    }

    // @verifies CT-EFF-008
    #[test]
    fn a_long_burst_is_cut_into_batches() {
        let (tx, rx) = mpsc::channel();
        let (out, got) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let (quiet, most) = (Duration::from_millis(100), Duration::from_millis(150));
            debounce(&rx, quiet, most, |b| out.send(b).unwrap());
        });
        for _ in 0..20 {
            tx.send(Seen::Paths(vec!["a".into()])).unwrap();
            std::thread::sleep(Duration::from_millis(20));
        }
        drop(tx);
        thread.join().unwrap();
        assert!(got.try_iter().count() >= 2);
    }

    // @verifies CT-EFF-008, CT-EFF-007
    #[test]
    fn a_folder_change_asks_for_everything_and_trouble_is_passed_on() {
        let (tx, rx) = mpsc::channel();
        let (out, got) = mpsc::channel();
        tx.send(Seen::Paths(vec!["a".into()])).unwrap();
        tx.send(Seen::Everything).unwrap();
        tx.send(Seen::Trouble(out_of_watches())).unwrap();
        drop(tx);
        debounce(
            &rx,
            Duration::from_millis(20),
            Duration::from_secs(5),
            |b| out.send(b).unwrap(),
        );
        let batch = got.recv().unwrap();
        assert!(batch.everything);
        assert_eq!(batch.trouble.len(), 1);
    }
}
