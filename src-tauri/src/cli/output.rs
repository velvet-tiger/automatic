use serde::Serialize;

/// Shared output format flag for every CLI verb.
#[derive(Debug, Clone, Copy, Default)]
pub struct OutputOptions {
    pub json: bool,
    pub quiet: bool,
}

/// Print a value as either pretty-printed JSON (`--json`) or as the supplied
/// human string. The human closure is only invoked when not in JSON mode, so
/// callers can build the human form lazily.
pub fn emit<T, F>(opts: OutputOptions, value: &T, human: F) -> Result<(), String>
where
    T: Serialize,
    F: FnOnce() -> String,
{
    if opts.json {
        let rendered = serde_json::to_string_pretty(value)
            .map_err(|e| format!("failed to serialise output as JSON: {}", e))?;
        println!("{}", rendered);
        return Ok(());
    }
    if !opts.quiet {
        println!("{}", human());
    }
    Ok(())
}

/// Emit a single-line confirmation. In `--json` mode this becomes a small
/// object so scripts can still parse it; in human mode it is just the line.
pub fn emit_status(opts: OutputOptions, status: &str, message: &str) -> Result<(), String> {
    if opts.json {
        let body = serde_json::json!({ "status": status, "message": message });
        let rendered = serde_json::to_string_pretty(&body)
            .map_err(|e| format!("failed to serialise status as JSON: {}", e))?;
        println!("{}", rendered);
        return Ok(());
    }
    if !opts.quiet {
        println!("{}", message);
    }
    Ok(())
}

/// Print pre-serialised JSON text verbatim (used when `core::*` already
/// returns a JSON string, e.g. `core::projects::read_project`).
pub fn emit_raw_json(opts: OutputOptions, raw: &str, human_fallback: &str) -> Result<(), String> {
    if opts.json {
        println!("{}", raw);
        return Ok(());
    }
    if !opts.quiet {
        println!("{}", human_fallback);
    }
    Ok(())
}
