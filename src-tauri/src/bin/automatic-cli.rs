//! Console-subsystem CLI binary.
//!
//! The GUI binary (`automatic`, built from `src/main.rs`) is linked as a
//! Windows GUI subsystem app so launching it from Explorer does not flash
//! a console. That same flag is what stops PowerShell and cmd.exe from
//! waiting for it, which makes the in-process CLI dispatch in `main.rs`
//! useless for piping output on Windows.
//!
//! This second binary exists to solve that. It links as a normal console
//! subsystem app (no `windows_subsystem` attribute), so the shell waits
//! for it, stdout / stderr / exit codes all behave like any other CLI
//! tool, and there is no `AttachConsole` race. Its only job is to forward
//! argv into `automatic_lib::cli::run` — every command, every flag, and
//! every output format are shared with the dispatch path in `main.rs`.
//!
//! On Unix the symlink-from-PATH-to-`automatic` flow still works the
//! same way, so this binary is mainly distributed (and installed) on
//! Windows. It is harmless on Unix too — `cli::run` is identical.

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let code = automatic_lib::cli::run(argv);
    std::process::exit(code);
}
