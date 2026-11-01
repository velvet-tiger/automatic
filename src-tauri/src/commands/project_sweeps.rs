//! Background queue for sweeps: work that re-syncs many projects after one
//! library change.
//!
//! A sweep over dozens of projects must not run on the main thread, where
//! Tauri runs plain commands, or the window stops responding. Commands write
//! the library, queue the sweep, and return. One worker runs the queue in
//! order and reports progress through a [`SweepReporter`].
//!
//! The main thread used to keep sweeps and project saves apart by accident.
//! [`hold_project_writes`] now does that on purpose: everything that reads a
//! project, changes it and writes it back holds the lock across all three.
//!
//! The GUI starts the worker in `setup`. Without one (tests, and the
//! `mcp-serve` process, which shares these commands) jobs run inline on the
//! calling thread.

use std::cell::Cell;
use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{mpsc, Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::Instant;

use serde::Serialize;

/// Tauri event carrying each [`SweepEvent`].
const SWEEP_EVENT: &str = "project-sweep-progress";

/// One project a sweep could not update.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SweepFailure {
    pub project: String,
    pub error: String,
}

/// Progress of one job. Sent when its total grows, after each project, and
/// once with `finished` set.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SweepEvent {
    pub job_id: u64,
    pub label: String,
    pub done: usize,
    pub total: usize,
    pub failed: Vec<SweepFailure>,
    pub finished: bool,
    pub duration_ms: u64,
}

/// Where progress goes. Injected so the queue runs without an `AppHandle`.
pub(crate) trait SweepReporter: Send + Sync {
    fn report(&self, event: &SweepEvent);
}

struct TauriSweepReporter(tauri::AppHandle);

impl SweepReporter for TauriSweepReporter {
    fn report(&self, event: &SweepEvent) {
        use tauri::Emitter;
        if let Err(e) = self.0.emit(SWEEP_EVENT, event) {
            eprintln!(
                "[automatic] could not report project update progress: {}",
                e
            );
        }
    }
}

#[derive(Default)]
struct Tally {
    done: usize,
    total: usize,
    failed: Vec<SweepFailure>,
}

/// Counts one job's projects. A job may run several sweeps, so the total
/// grows as each one finds its projects.
pub(crate) struct SweepProgress {
    job_id: u64,
    label: String,
    reporter: Option<Arc<dyn SweepReporter>>,
    started: Instant,
    tally: Mutex<Tally>,
}

impl SweepProgress {
    /// Progress nobody watches: startup housekeeping, tests, `mcp-serve`.
    /// Failures still reach stderr.
    pub(crate) fn silent() -> Self {
        Self::new(0, String::new(), None)
    }

    #[cfg(test)]
    pub(crate) fn reporting(reporter: Arc<dyn SweepReporter>) -> Self {
        Self::new(1, "test".to_string(), Some(reporter))
    }

    fn new(job_id: u64, label: String, reporter: Option<Arc<dyn SweepReporter>>) -> Self {
        Self {
            job_id,
            label,
            reporter,
            started: Instant::now(),
            tally: Mutex::new(Tally::default()),
        }
    }

    pub(crate) fn add_total(&self, projects: usize) {
        if projects == 0 {
            return;
        }
        self.update(|tally| tally.total += projects, false);
    }

    /// Record one project's result. `project` is its display name.
    pub(crate) fn project_finished(&self, project: &str, outcome: Result<(), String>) {
        if let Err(error) = &outcome {
            eprintln!("Failed to update project '{}': {}", project, error);
        }
        self.update(
            |tally| {
                tally.done += 1;
                if let Err(error) = outcome {
                    tally.failed.push(SweepFailure {
                        project: project.to_string(),
                        error,
                    });
                }
            },
            false,
        );
    }

    fn finish(&self) {
        self.update(|_| {}, true);
    }

    fn update(&self, change: impl FnOnce(&mut Tally), finished: bool) {
        let Some(reporter) = &self.reporter else {
            change(&mut lock_ignoring_poison(&self.tally));
            return;
        };
        let event = {
            let mut tally = lock_ignoring_poison(&self.tally);
            change(&mut tally);
            SweepEvent {
                job_id: self.job_id,
                label: self.label.clone(),
                done: tally.done,
                total: tally.total,
                failed: tally.failed.clone(),
                finished,
                duration_ms: self.started.elapsed().as_millis() as u64,
            }
        };
        reporter.report(&event);
    }
}

/// A panic in one job must not stop the queue or every later save, and what
/// these mutexes guard is still usable after one: a tally, a job list.
fn lock_ignoring_poison<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

type JobBody = Box<dyn FnOnce(&SweepProgress) + Send>;

struct QueuedJob {
    id: u64,
    key: Option<String>,
    label: String,
    body: JobBody,
}

#[derive(Default)]
struct QueueState {
    waiting: VecDeque<QueuedJob>,
    next_id: u64,
}

/// Jobs in the order they were queued.
#[derive(Default)]
struct SweepQueue {
    state: Mutex<QueueState>,
    ready: Condvar,
}

impl SweepQueue {
    /// Add a job, unless one with the same `key` is still waiting. That job
    /// reads the library when it starts, so it picks up this change too. A
    /// running job may already have passed some projects, so it never
    /// absorbs a new one. Returns whether the job was added.
    fn push(&self, key: Option<String>, label: String, body: JobBody) -> bool {
        let mut state = lock_ignoring_poison(&self.state);
        if key.is_some() && state.waiting.iter().any(|job| job.key == key) {
            return false;
        }
        state.next_id += 1;
        let id = state.next_id;
        state.waiting.push_back(QueuedJob {
            id,
            key,
            label,
            body,
        });
        self.ready.notify_one();
        true
    }

    fn next_job(&self) -> QueuedJob {
        let mut state = lock_ignoring_poison(&self.state);
        loop {
            if let Some(job) = state.waiting.pop_front() {
                return job;
            }
            state = self
                .ready
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    /// Queue `body` and block until the worker has run it. Never call this
    /// from a job, or while holding [`hold_project_writes`]: the worker needs
    /// both to get to `body`.
    fn run_and_wait<T, F>(&self, label: String, body: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce(&SweepProgress) -> Result<T, String> + Send + 'static,
    {
        let (result_tx, result_rx) = mpsc::channel();
        let stopped = format!("The update for {} stopped before it finished", label);
        self.push(
            None,
            label,
            Box::new(move |progress| {
                // The caller only goes away if it panicked, and then nobody
                // wants the result.
                let _ = result_tx.send(body(progress));
            }),
        );
        result_rx.recv().map_err(|_| stopped)?
    }
}

fn run_job(job: QueuedJob, reporter: &Arc<dyn SweepReporter>) {
    let progress = SweepProgress::new(job.id, job.label, Some(Arc::clone(reporter)));
    let body = job.body;
    if catch_unwind(AssertUnwindSafe(|| body(&progress))).is_err() {
        progress.project_finished(
            "Background update",
            Err("The update stopped unexpectedly. Sync the affected projects again.".to_string()),
        );
    }
    progress.finish();
}

fn run_worker(queue: &SweepQueue, reporter: Arc<dyn SweepReporter>) -> ! {
    loop {
        run_job(queue.next_job(), &reporter);
    }
}

static QUEUE: OnceLock<Arc<SweepQueue>> = OnceLock::new();

/// Start the worker. Without it jobs run inline, which is slower for the
/// caller but still correct, so a failure here does not stop the app.
pub(crate) fn start_sweep_worker(app: tauri::AppHandle) {
    let queue = Arc::new(SweepQueue::default());
    let worker_queue = Arc::clone(&queue);
    let reporter: Arc<dyn SweepReporter> = Arc::new(TauriSweepReporter(app));
    let spawned = std::thread::Builder::new()
        .name("project-sweeps".to_string())
        .spawn(move || run_worker(&worker_queue, reporter));
    match spawned {
        Ok(_) => {
            let _ = QUEUE.set(queue);
        }
        Err(e) => eprintln!(
            "[automatic] could not start the project update worker: {}",
            e
        ),
    }
}

/// Run `body` in the background and return now. Jobs with the same `key`
/// merge while waiting, so give a key only when `body` is fully described
/// by it and reads current state. `label` finishes the phrase "Updating
/// projects for ...".
pub(crate) fn queue_sweep<F>(key: Option<String>, label: String, body: F)
where
    F: FnOnce(&SweepProgress) + Send + 'static,
{
    match QUEUE.get() {
        Some(queue) => {
            queue.push(key, label, Box::new(body));
        }
        None => body(&SweepProgress::silent()),
    }
}

/// Run `body` in turn with queued jobs and return its result. For deletes
/// and renames, whose errors the caller must see and which must not overtake
/// an earlier save. Blocks, so the command must be `#[tauri::command(async)]`.
pub(crate) fn run_sweep_and_wait<T, F>(label: String, body: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&SweepProgress) -> Result<T, String> + Send + 'static,
{
    match QUEUE.get() {
        Some(queue) => queue.run_and_wait(label, body),
        None => body(&SweepProgress::silent()),
    }
}

// ── Project write lock ───────────────────────────────────────────────────────

/// Guards no data of its own, so a panicking holder leaves nothing broken.
static PROJECT_WRITES: Mutex<()> = Mutex::new(());

thread_local! {
    static HOLDS_PROJECT_WRITES: Cell<bool> = const { Cell::new(false) };
}

/// While alive, no other thread is reading, changing and writing a project.
pub(crate) struct ProjectWriteGuard {
    outer: Option<MutexGuard<'static, ()>>,
}

impl Drop for ProjectWriteGuard {
    fn drop(&mut self) {
        if self.outer.take().is_some() {
            HOLDS_PROJECT_WRITES.with(|held| held.set(false));
        }
    }
}

/// Take the project write lock, before reading a project that will be
/// written back. Hold it for one project at a time, because the main thread
/// can be waiting. A thread that already holds it gets a guard that does
/// nothing, so a command may call code that takes the lock too.
pub(crate) fn hold_project_writes() -> ProjectWriteGuard {
    if HOLDS_PROJECT_WRITES.with(Cell::get) {
        return ProjectWriteGuard { outer: None };
    }
    let outer = lock_ignoring_poison(&PROJECT_WRITES);
    HOLDS_PROJECT_WRITES.with(|held| held.set(true));
    ProjectWriteGuard { outer: Some(outer) }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    #[derive(Default)]
    pub(crate) struct RecordingReporter {
        pub(crate) events: Mutex<Vec<SweepEvent>>,
    }

    impl SweepReporter for RecordingReporter {
        fn report(&self, event: &SweepEvent) {
            self.events.lock().expect("events").push(event.clone());
        }
    }

    impl RecordingReporter {
        fn last(&self) -> SweepEvent {
            self.events
                .lock()
                .expect("events")
                .last()
                .expect("an event")
                .clone()
        }
    }

    fn reporter() -> (Arc<RecordingReporter>, Arc<dyn SweepReporter>) {
        let recording = Arc::new(RecordingReporter::default());
        let shared: Arc<dyn SweepReporter> = recording.clone();
        (recording, shared)
    }

    fn take_labels(queue: &SweepQueue) -> Vec<String> {
        let mut labels = Vec::new();
        while !lock_ignoring_poison(&queue.state).waiting.is_empty() {
            labels.push(queue.next_job().label);
        }
        labels
    }

    #[test]
    fn a_waiting_job_with_the_same_key_absorbs_the_new_one() {
        let queue = SweepQueue::default();
        let key = || Some("rule:a".to_string());

        assert!(queue.push(key(), "first".into(), Box::new(|_| {})));
        assert!(!queue.push(key(), "second".into(), Box::new(|_| {})));
        assert!(queue.push(Some("rule:b".into()), "other".into(), Box::new(|_| {})));

        assert_eq!(take_labels(&queue), vec!["first", "other"]);
    }

    #[test]
    fn a_running_job_does_not_absorb_a_new_one() {
        let queue = SweepQueue::default();
        let key = || Some("rule:a".to_string());
        queue.push(key(), "running".into(), Box::new(|_| {}));
        let running = queue.next_job();

        assert!(queue.push(key(), "after".into(), Box::new(|_| {})));

        assert_eq!(running.label, "running");
        assert_eq!(take_labels(&queue), vec!["after"]);
    }

    #[test]
    fn jobs_without_a_key_never_merge() {
        let queue = SweepQueue::default();
        assert!(queue.push(None, "one".into(), Box::new(|_| {})));
        assert!(queue.push(None, "two".into(), Box::new(|_| {})));
        assert_eq!(take_labels(&queue), vec!["one", "two"]);
    }

    #[test]
    fn jobs_run_in_the_order_they_were_queued() {
        let queue = SweepQueue::default();
        let (_recording, shared) = reporter();
        let order = Arc::new(Mutex::new(Vec::new()));
        for n in 0..3 {
            let order = Arc::clone(&order);
            queue.push(
                None,
                format!("job {n}"),
                Box::new(move |_| order.lock().expect("order").push(n)),
            );
        }
        for _ in 0..3 {
            run_job(queue.next_job(), &shared);
        }
        assert_eq!(*order.lock().expect("order"), vec![0, 1, 2]);
    }

    #[test]
    fn progress_names_each_failed_project_and_carries_on() {
        let queue = SweepQueue::default();
        let (recording, shared) = reporter();
        queue.push(
            None,
            "rule a".into(),
            Box::new(|progress| {
                progress.add_total(3);
                progress.project_finished("one", Ok(()));
                progress.project_finished("two", Err("disk full".to_string()));
                progress.project_finished("three", Ok(()));
            }),
        );

        run_job(queue.next_job(), &shared);

        let finished = recording.last();
        assert!(finished.finished);
        assert_eq!(finished.label, "rule a");
        assert_eq!((finished.done, finished.total), (3, 3));
        assert_eq!(
            finished.failed,
            vec![SweepFailure {
                project: "two".to_string(),
                error: "disk full".to_string(),
            }]
        );
        let unfinished = recording.events.lock().expect("events").len() - 1;
        assert_eq!(unfinished, 4, "the total, then one event per project");
    }

    #[test]
    fn a_job_that_finds_no_projects_still_reports_that_it_finished() {
        let queue = SweepQueue::default();
        let (recording, shared) = reporter();
        queue.push(None, "unused rule".into(), Box::new(|p| p.add_total(0)));

        run_job(queue.next_job(), &shared);

        let events = recording.events.lock().expect("events");
        assert_eq!(events.len(), 1);
        assert!(events[0].finished);
        assert_eq!(events[0].total, 0);
    }

    #[test]
    fn a_panicking_job_is_reported_and_the_queue_keeps_working() {
        let queue = SweepQueue::default();
        let (recording, shared) = reporter();
        queue.push(None, "bad".into(), Box::new(|_| panic!("job panic")));
        let ran_next = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&ran_next);
        queue.push(
            None,
            "next".into(),
            Box::new(move |_| flag.store(true, Ordering::SeqCst)),
        );

        run_job(queue.next_job(), &shared);
        let after_panic = recording.last();
        assert!(after_panic.finished);
        assert_eq!(after_panic.failed.len(), 1);

        run_job(queue.next_job(), &shared);
        assert!(ran_next.load(Ordering::SeqCst));
    }

    #[test]
    fn run_and_wait_returns_the_jobs_result_after_earlier_jobs() {
        let queue = Arc::new(SweepQueue::default());
        let (_recording, shared) = reporter();
        let order = Arc::new(Mutex::new(Vec::new()));
        let earlier = Arc::clone(&order);
        queue.push(
            None,
            "save".into(),
            Box::new(move |_| earlier.lock().expect("order").push("save")),
        );

        let worker_queue = Arc::clone(&queue);
        std::thread::spawn(move || run_worker(&worker_queue, shared));

        let waited = Arc::clone(&order);
        let ok = queue.run_and_wait("delete".into(), move |_| {
            waited.lock().expect("order").push("delete");
            Ok(7)
        });
        assert_eq!(ok, Ok(7));
        assert_eq!(*order.lock().expect("order"), vec!["save", "delete"]);

        let err: Result<(), String> =
            queue.run_and_wait("delete".into(), |_| Err("in use".to_string()));
        assert_eq!(err, Err("in use".to_string()));
    }

    #[test]
    fn run_and_wait_reports_a_job_that_died_instead_of_hanging() {
        let queue = Arc::new(SweepQueue::default());
        let (_recording, shared) = reporter();
        let worker_queue = Arc::clone(&queue);
        std::thread::spawn(move || run_worker(&worker_queue, shared));

        let result: Result<(), String> =
            queue.run_and_wait("rule a".into(), |_| panic!("job panic"));

        assert!(result.expect_err("stopped").contains("rule a"));
    }

    #[test]
    fn the_write_lock_keeps_threads_apart_and_can_be_taken_twice_on_one() {
        let first = hold_project_writes();
        let nested = hold_project_writes();
        drop(nested);

        let (taken_tx, taken_rx) = mpsc::channel();
        let other = std::thread::spawn(move || {
            let _guard = hold_project_writes();
            taken_tx.send(()).expect("send");
        });
        assert!(
            taken_rx.recv_timeout(Duration::from_millis(100)).is_err(),
            "dropping a nested guard must not release the lock"
        );

        drop(first);
        taken_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the other thread gets the lock");
        other.join().expect("join");
    }
}
