//! A time limit for tests that wait on the file watcher (CT-EFF-008). The app's own disk
//! wait blocks with no timeout, as CT-EFF-008 requires, so a watcher bug would hang its
//! test for hours. `within` runs the test on a thread of its own and fails it when it has
//! not ended in time. Built only for tests, never shipped; `tests/idle_cpu.rs` allows this
//! file by name.

use std::sync::mpsc;
use std::time::Duration;

// @verifies CT-EFF-008
/// Runs `test` on a thread of its own and fails if it has not ended within `limit`. A
/// panic in `test` fails too, with its own message.
pub fn within(limit: Duration, test: impl FnOnce() + Send + 'static) {
    let (done, ended) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        test();
        let _ = done.send(());
    });
    match ended.recv_timeout(limit) {
        Ok(()) => thread.join().unwrap(),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            std::panic::resume_unwind(thread.join().unwrap_err())
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!("the test did not end within {} s", limit.as_secs())
        }
    }
}

// @verifies CT-EFF-008
#[cfg(test)]
mod tests {
    use super::*;

    // @verifies CT-EFF-008
    #[test]
    fn a_test_that_hangs_fails_and_one_that_ends_passes() {
        within(Duration::from_secs(5), || {});
        let hung = std::panic::catch_unwind(|| {
            within(Duration::from_millis(50), || {
                let (_keep, never) = mpsc::channel::<()>();
                let _ = never.recv();
            })
        });
        assert!(hung.is_err());
        let failed = std::panic::catch_unwind(|| within(Duration::from_secs(5), || panic!("no")));
        assert!(failed.is_err());
    }
}
