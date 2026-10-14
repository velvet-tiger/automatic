//! How a presence check may use the secret store, and what it keeps of a
//! read that failed.
//!
//! A failed read is never kept as "nothing stored". What is kept, for a
//! short time and for background checks only, is the failure itself.
//!
//! With a locked keyring or a denied keychain every read can raise a prompt.
//! The drift check runs on a timer, and a timer that read the store on each
//! tick would raise one every few seconds. A background check therefore
//! reports the failure it last saw, as the same error, without asking the
//! store again for some minutes.
//!
//! A check the user asked for reads the store, except in the few seconds
//! after a failure. One action often checks the same entry many times: a
//! saved skill re-syncs every project that uses it, and each sync asks about
//! the same servers. Without the pause, one locked keychain would raise a
//! prompt per project.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Who is asking whether a secret is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoreRead {
    /// The user asked for something that needs the answer, such as a sync or
    /// a settings page. The store is read, unless it failed for this entry
    /// in the last few seconds.
    Now,
    /// A timer, or a page that only decides what to show. A failure seen in
    /// the last few minutes is reported again without a read.
    Background,
}

impl StoreRead {
    /// How long a failed read is reported again before the store is asked.
    fn retry_after(self) -> Duration {
        match self {
            StoreRead::Now => NOW_RETRY_AFTER,
            StoreRead::Background => BACKGROUND_RETRY_AFTER,
        }
    }
}

/// How long a background check reports a failure before it reads again.
const BACKGROUND_RETRY_AFTER: Duration = Duration::from_secs(5 * 60);

/// How long a check the user asked for reports a failure before it reads
/// again. Long enough to cover one action that syncs many projects, short
/// enough that a retry after unlocking the keychain reads the store.
///
/// Tests that give the store a fault read it again at once, so the pause is
/// off for them. The pause itself is tested in this file.
const NOW_RETRY_AFTER: Duration = if cfg!(test) {
    Duration::ZERO
} else {
    Duration::from_secs(10)
};

/// The presence reads that failed, by store entry.
#[derive(Default)]
pub(crate) struct FailedReads {
    failures: Mutex<HashMap<String, (Instant, String)>>,
}

impl FailedReads {
    /// The error to report for `entry` without reading the store, or `None`
    /// when the store should be read.
    pub(crate) fn remembered(&self, entry: &str, read: StoreRead) -> Option<String> {
        self.remembered_for(entry, read, read.retry_after())
    }

    /// [`Self::remembered`] with the wait given. A user who asked and got a
    /// remembered failure is told so, and that trying again soon will read
    /// the store. Without that, a retry straight after a mistaken "Deny"
    /// returns the same error with nothing to explain it.
    fn remembered_for(&self, entry: &str, read: StoreRead, max_age: Duration) -> Option<String> {
        let error = self.remembered_within(entry, max_age)?;
        Some(match read {
            StoreRead::Now => format!(
                "{} (The secret store failed a moment ago and was not asked again. Try again in a few seconds.)",
                error
            ),
            StoreRead::Background => error,
        })
    }

    fn remembered_within(&self, entry: &str, max_age: Duration) -> Option<String> {
        let failures = self.failures.lock().ok()?;
        let (failed_at, error) = failures.get(entry)?;
        (failed_at.elapsed() < max_age).then(|| error.clone())
    }

    /// Keep the outcome of a read of `entry`: its error when it failed, and
    /// nothing when the store answered.
    pub(crate) fn record<T>(&self, entry: &str, outcome: &Result<T, String>) {
        let Ok(mut failures) = self.failures.lock() else {
            return;
        };
        match outcome {
            Ok(_) => failures.remove(entry),
            Err(error) => failures.insert(entry.to_string(), (Instant::now(), error.clone())),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &str = "an-entry";
    const LONG: Duration = Duration::from_secs(3600);

    fn failed() -> FailedReads {
        let reads = FailedReads::default();
        reads.record::<bool>(ENTRY, &Err("the keychain is locked".to_string()));
        reads
    }

    #[test]
    fn a_recent_failure_is_reported_without_a_read() {
        let reads = failed();

        assert_eq!(
            reads.remembered_within(ENTRY, LONG),
            Some("the keychain is locked".to_string())
        );
        assert_eq!(reads.remembered_within("another-entry", LONG), None);
    }

    #[test]
    fn the_store_is_read_again_once_the_failure_is_old() {
        assert_eq!(failed().remembered_within(ENTRY, Duration::ZERO), None);
    }

    #[test]
    fn a_background_check_waits_longer_than_one_the_user_asked_for() {
        assert!(StoreRead::Background.retry_after() > StoreRead::Now.retry_after());
        assert_eq!(StoreRead::Background.retry_after(), BACKGROUND_RETRY_AFTER);
        // The pause is off in tests, so a test reads the store at once.
        assert_eq!(StoreRead::Now.retry_after(), Duration::ZERO);
        assert_eq!(failed().remembered(ENTRY, StoreRead::Now), None);
        assert!(failed().remembered(ENTRY, StoreRead::Background).is_some());
    }

    #[test]
    fn a_user_who_gets_a_remembered_failure_is_told_to_try_again() {
        let reads = failed();

        let told = reads.remembered_for(ENTRY, StoreRead::Now, LONG).expect("remembered");
        assert!(told.starts_with("the keychain is locked"), "unexpected error: {told}");
        assert!(told.contains("Try again in a few seconds"), "unexpected error: {told}");

        // A timer has no one to tell.
        assert_eq!(
            reads.remembered_for(ENTRY, StoreRead::Background, LONG),
            Some("the keychain is locked".to_string())
        );
    }

    #[test]
    fn a_read_the_store_answered_forgets_the_failure() {
        let reads = failed();

        reads.record(ENTRY, &Ok(true));

        assert_eq!(reads.remembered_within(ENTRY, LONG), None);
    }
}
