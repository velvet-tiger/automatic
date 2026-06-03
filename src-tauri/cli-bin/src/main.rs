//! Console-subsystem CLI binary.
//!
//! The GUI binary (`automatic`, built from the parent `src/main.rs`) is
//! linked as a Windows GUI-subsystem app so launching it from Explorer
//! does not flash a console window. That same attribute is what stops
//! PowerShell and cmd.exe from waiting for it, which makes the in-process
//! CLI dispatch in `main.rs` useless for piping output on Windows.
//!
//! This second binary exists to solve that. It links as a normal console
//! subsystem app (no `windows_subsystem` attribute), so the shell waits
//! for it, stdout / stderr / exit codes all behave like any other CLI
//! tool, and there is no `AttachConsole` race. Its only job is to forward
//! argv into `automatic_lib::cli::run` — every command, every flag, and
//! every output format are shared with the dispatch path in the GUI
//! binary's `main.rs`.
//!
//! ## Why a separate workspace crate
//!
//! Tauri's bundler reads the active crate's `Cargo.toml` and treats every
//! `[[bin]]` entry as a candidate to bundle (and, on macOS, to `lipo`
//! into the universal target). A second `[[bin]]` inside the main crate
//! therefore caused the universal-apple-darwin bundle step to look for
//! `target/universal-apple-darwin/release/automatic-cli`, which the
//! `lipo` step never produced. Moving the CLI binary into a sibling
//! workspace member makes it invisible to Tauri while still sharing the
//! same `target/` directory, so the Windows install code finds the
//! produced file next to the GUI binary as before.
//!
//! On macOS and Linux this crate is not built by the release pipeline —
//! the GUI binary is the CLI binary on Unix via a symlink from `$PATH`.
//! On Windows the CI workflow runs `cargo build -p automatic-cli` before
//! invoking `tauri build`, and the Windows-only `tauri.windows.conf.json`
//! adds an `externalBin` entry so the result ships inside the installer.

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let code = automatic_lib::cli::run(argv);
    std::process::exit(code);
}
