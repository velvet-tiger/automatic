# Test Coverage Analysis

**Date:** 2026-03-09  
**Status:** Active — gaps are being addressed

---

## Summary

| Metric | Value |
|--------|-------|
| Rust unit tests (existing) | ~168 across 21 files |
| Frontend tests | 0 (no test runner configured) |
| Non-trivial source files with tests | ~21 of ~75 (~28%) |

---

## What Is Well-Tested

| Module | Tests | Notes |
|--------|-------|-------|
| `core/ai.rs` | 57 | Path security deny-list, file reading, boundary cases |
| `agent/*.rs` (14 adapters) | ~56 | Detect + config write for every agent |
| `recommendations.rs` | 14 | Full CRUD + status transitions |
| `memory.rs` | 14 | CRUD + project isolation + persistence |
| `activity.rs` | 9 | Event log CRUD + ordering |
| `languages.rs` | 7 | Framework detection |
| `proxy.rs` | 5 | SSE parsing |

---

## Coverage Gaps

### Priority 1 — Critical

#### `core/env_crypto.rs` (0 tests)
AES-256-GCM encryption for MCP server env vars. A silent bug here corrupts stored API keys with no visible error. Requires testing encrypt/decrypt roundtrips, idempotency (double-encrypt is a no-op), plaintext passthrough on decrypt, and tampered-ciphertext rejection.

> **Action:** Tests added to `core/env_crypto.rs` — see `#[cfg(test)]` block.

#### `core/rules_injection.rs` (0 tests)
Injects rule content into project files using sentinel markers. Bugs produce corrupted or doubled content in agent instruction files. Test surface: `strip_rules_section`, `build_rules_section`, `inject_rules_into_project_file`, `sync_rules_to_dot_claude_rules`, staleness checks.

> **Action:** Tests added to `core/rules_injection.rs` — see `#[cfg(test)]` block.

#### `core/projects.rs` (0 tests)
Foundation CRUD for projects. Dual-location storage (registry pointer + project-directory config) has non-trivial branching. Tests cover: list (empty dir), save + read roundtrip (no directory), save + read roundtrip (with directory), delete, rename, invalid name rejection.

> **Action:** Tests added to `core/projects.rs` — see `#[cfg(test)]` block.

#### `mcp.rs` (~661 lines, 0 tests)
The primary MCP interface surface — 15+ tools. Regressions break all connected agents. Needs integration-style tests against the tool router. Deferred: requires `rmcp` in-process test harness research.

#### `oauth.rs` (~559 lines, 0 tests)
Full OAuth 2.1 implementation. Token parsing, state machine transitions, and error paths should be tested with mocked HTTP. Deferred: needs HTTP mock layer.

### Priority 2 — Important

#### `core/rules.rs` (0 tests)
`is_valid_machine_name` validator and CRUD (list, read, save, delete, install_default_rules). Pure file I/O — straightforward to test with temp dirs.

> **Action:** Tests added to `core/rules.rs` — see `#[cfg(test)]` block.

#### `core/mcp_servers.rs` (0 tests)
MCP server config CRUD with integrated env encryption/decryption. Critical integration point: plaintext goes in, encrypted form is stored, plaintext comes back out.

> **Action:** Tests added to `core/mcp_servers.rs` — see `#[cfg(test)]` block.

#### `core/settings.rs` (0 tests)
Settings read/write/reset. Default values, missing file handling, flag setters (mark_skill_installed, dismiss_welcome).

> **Action:** Tests added to `core/settings.rs` — see `#[cfg(test)]` block.

#### `sync/engine.rs`, `sync/drift.rs` (0 tests)
Sync orchestration and drift detection are user-visible (drift banners in UI). Need fixture project directories. Deferred: needs sync module refactor for path injection.

### Priority 3 — Lower

#### `core/skills.rs` (0 tests)
`scan_skills_dir`, `save_skill`, `delete_skill`, `sync_skill`. Uses `~/.agents/skills/` and `~/.claude/skills/` — testable with temp dirs using path-injected helpers.

> **Action:** Tests added to `core/skills.rs` — see `#[cfg(test)]` block.

#### `context.rs` (0 tests)
Project context snapshot generation for AI. Deferred: depends on many other modules; integration test preferred.

#### `commands/*.rs` (21 files, 0 tests)
Thin `#[tauri::command]` delegates. Low value to test independently; testing the `core/` functions they call is sufficient.

#### Frontend (`src/*.tsx`, 0 tests, no test runner)
No Vitest/Jest configured. If investing here, start with utility modules (`analytics.ts`, `flags.ts`, `theme.ts`) then move to components. Vitest is the natural choice given the Vite build setup.

---

## Testing Patterns Used in This Project

### Temp-dir isolation
All `core/` modules that operate on the filesystem accept paths as arguments (or have `_at()` variants). Tests create a `tempfile::TempDir` and pass its path, ensuring no real `~/.automatic-dev` data is touched.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_dir() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn save_and_read_roundtrip() {
        let dir = temp_dir();
        // pass dir.path() to functions under test
    }
}
```

### In-memory SQLite
Modules using SQLite (`recommendations.rs`, `activity.rs`) open `":memory:"` connections in tests, so no disk state is involved.

### Fixture helpers
Each test module defines small local helpers (`setup_conn()`, `stdio_servers()`, etc.) rather than using a shared test crate. This keeps tests self-contained.

---

## Running Tests

```bash
# All Rust tests
npm test
# or directly:
cd src-tauri && cargo test

# Single module
cd src-tauri && cargo test core::rules

# With output visible
cd src-tauri && cargo test -- --nocapture
```

---

## Progress Log

| Date | Action |
|------|--------|
| 2026-03-09 | Initial coverage analysis completed |
| 2026-03-09 | Tests added: `core/rules.rs`, `core/rules_injection.rs`, `core/env_crypto.rs`, `core/projects.rs`, `core/settings.rs`, `core/mcp_servers.rs`, `core/skills.rs` |
