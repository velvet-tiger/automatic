//! `automatic memory ...` — list, get, set, search project memory.

use super::output::{emit, emit_status, OutputOptions};
use super::{CliError, MemoryAction};
use crate::memory as memory_store;

pub fn dispatch(action: MemoryAction, opts: OutputOptions) -> Result<(), CliError> {
    match action {
        MemoryAction::List { project, pattern } => list(&project, pattern.as_deref(), opts),
        MemoryAction::Get { project, key } => get(&project, &key, opts),
        MemoryAction::Set {
            project,
            key,
            value,
            source,
        } => set(&project, &key, &value, &source, opts),
        MemoryAction::Search { project, query } => search(&project, &query, opts),
    }
}

fn list(project: &str, pattern: Option<&str>, opts: OutputOptions) -> Result<(), CliError> {
    if opts.json {
        // Structured form: serialise the on-disk DB. `list_memories` returns
        // a markdown string which is not useful to scripts.
        let db = memory_store::read_memory_db(project).map_err(CliError::from)?;
        let filtered = filter_db(&db, pattern);
        emit(opts, &filtered, String::new).map_err(CliError::Io)
    } else {
        let report = memory_store::list_memories(project, pattern).map_err(CliError::from)?;
        emit_status(opts, "ok", &report).map_err(CliError::Io)
    }
}

fn get(project: &str, key: &str, opts: OutputOptions) -> Result<(), CliError> {
    if opts.json {
        let db = memory_store::read_memory_db(project).map_err(CliError::from)?;
        match db.get(key) {
            Some(entry) => emit(opts, entry, String::new).map_err(CliError::Io),
            None => Err(CliError::NotFound(format!(
                "memory key '{}' not found",
                key
            ))),
        }
    } else {
        let report = memory_store::get_memory(project, key).map_err(CliError::from)?;
        emit_status(opts, "ok", &report).map_err(CliError::Io)
    }
}

fn set(
    project: &str,
    key: &str,
    value: &str,
    source: &str,
    opts: OutputOptions,
) -> Result<(), CliError> {
    let message =
        memory_store::store_memory(project, key, value, Some(source)).map_err(CliError::from)?;
    emit_status(opts, "ok", &message).map_err(CliError::Io)
}

fn search(project: &str, query: &str, opts: OutputOptions) -> Result<(), CliError> {
    if opts.json {
        let db = memory_store::read_memory_db(project).map_err(CliError::from)?;
        let needle = query.to_lowercase();
        let matches: Vec<_> = db
            .iter()
            .filter(|(k, v)| {
                k.to_lowercase().contains(&needle) || v.value.to_lowercase().contains(&needle)
            })
            .map(|(k, v)| serde_json::json!({ "key": k, "entry": v }))
            .collect();
        emit(opts, &matches, String::new).map_err(CliError::Io)
    } else {
        let report = memory_store::search_memories(project, query).map_err(CliError::from)?;
        emit_status(opts, "ok", &report).map_err(CliError::Io)
    }
}

/// Apply the same case-insensitive substring filter `list_memories` uses
/// when called via the human path, so JSON and human output agree on which
/// entries are visible.
fn filter_db(
    db: &memory_store::MemoryDb,
    pattern: Option<&str>,
) -> std::collections::BTreeMap<String, memory_store::MemoryEntry> {
    let needle = pattern.map(|p| p.to_lowercase());
    let mut out = std::collections::BTreeMap::new();
    for (k, v) in db.iter() {
        if let Some(n) = &needle {
            if !k.to_lowercase().contains(n) {
                continue;
            }
        }
        out.insert(k.clone(), v.clone());
    }
    out
}
