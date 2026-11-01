//! The lock behind every read-change-write of Automatic's shared state:
//! projects, the registry pointers, profiles, groups, templates and the
//! global MCP selection.
//!
//! These are files with no transactions, so two writers working from the
//! same read lose one change. Take [`hold_library_writes`] before the read.
//! The lock is per process: `mcp-serve` has its own, so it does not keep
//! that process and the GUI apart.

use std::cell::Cell;
use std::sync::{Mutex, MutexGuard};

/// Guards no data of its own, so a panicking holder leaves nothing broken.
static LIBRARY_WRITES: Mutex<()> = Mutex::new(());

thread_local! {
    static HOLDS_LIBRARY_WRITES: Cell<bool> = const { Cell::new(false) };
}

/// While alive, no other thread is reading, changing and writing shared state.
pub(crate) struct LibraryWriteGuard {
    outer: Option<MutexGuard<'static, ()>>,
}

impl Drop for LibraryWriteGuard {
    fn drop(&mut self) {
        if self.outer.take().is_some() {
            HOLDS_LIBRARY_WRITES.with(|held| held.set(false));
        }
    }
}

/// Take the library write lock. Hold it for one item at a time where a sweep
/// is involved, so waiting callers get a turn. There is one lock, so there
/// is no order to get wrong, and a thread that already holds it gets a guard
/// that does nothing, so a command may call code that takes it too. Never
/// wait on the sweep worker while holding it: the worker needs it.
pub(crate) fn hold_library_writes() -> LibraryWriteGuard {
    if holds_library_writes() {
        return LibraryWriteGuard { outer: None };
    }
    let outer = LIBRARY_WRITES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    HOLDS_LIBRARY_WRITES.with(|held| held.set(true));
    LibraryWriteGuard { outer: Some(outer) }
}

/// Whether this thread holds the lock.
pub(crate) fn holds_library_writes() -> bool {
    HOLDS_LIBRARY_WRITES.with(Cell::get)
}
