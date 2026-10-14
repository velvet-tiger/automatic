//! How a presence check may use the secret store, and what it keeps of a
//! read that failed.
//!
//! A failed read is never kept as "nothing stored". What is kept, for a
//! short time and for background checks only, is the failure itself.
//!
//! The drift check runs on a timer. With a locked keyring or a denied
//! keychain every read can raise a prompt, and a timer that read the store on
//! each tick would raise one every few seconds. A background check therefore
//! reports the failure it last saw, as the same error, without asking the
//! store again. A check the user asked for always reads.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Who is asking whether a secret is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoreRead {
    /// The user asked for something that needs the answer, such as a sync or
    /// a settings page. The store is read.
    Now,
    /// A timer, or a page that only decides what to show. A failure seen in
    /// the last few minutes is reported again without a read.
    Background,
}

/// How long a background check reports a failure before it reads again.
const BACKGROUND_RETRY_AFTER: Duration = Duration::from_secs(5 * 60);

/// The presence reads that failed, by store entry.
#[derive(Default)]
pub(crate) struct FailedReads {
    failures: Mutex<HashMap<String, (Instant, String)>>,
}

impl FailedReads {
    /// The error to report for `entry` without reading the store, or `None`
    /// when the store should be read.
    pub(crate) fn remembered(&self, entry: &str, read: StoreRead) -> Option<String> {
        self.remembered_within(entry, read, BACKGROUND_RETRY_AFTER)
    }

    fn remembered_within(&self, entry: &str, read: StoreRead, max_age: Duration) -> Option<String> {
        if read == StoreRead::Now {
            return None;
        }
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
    fn a_background_check_reports_a_recent_failure_without_a_read() {
        let reads = failed();

        assert_eq!(
            reads.remembered_within(ENTRY, StoreRead::Background, LONG),
            Some("the keychain is locked".to_string())
        );
        assert_eq!(reads.remembered_within("another-entry", StoreRead::Background, LONG), None);
    }

    #[test]
    fn a_check_the_user_asked_for_always_reads() {
        assert_eq!(failed().remembered_within(ENTRY, StoreRead::Now, LONG), None);
    }

    #[test]
    fn a_background_check_reads_again_once_the_failure_is_old() {
        assert_eq!(failed().remembered_within(ENTRY, StoreRead::Background, Duration::ZERO), None);
    }

    #[test]
    fn a_read_the_store_answered_forgets_the_failure() {
        let reads = failed();

        reads.record(ENTRY, &Ok(true));

        assert_eq!(reads.remembered_within(ENTRY, StoreRead::Background, LONG), None);
    }
}
