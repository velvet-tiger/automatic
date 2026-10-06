//! Validation for stdio MCP servers launched through `docker run` (or
//! `podman run`).
//!
//! A stdio MCP server talks JSON-RPC over stdin/stdout.  A container started
//! without `-i` has no stdin, so the server reads EOF and exits at once; one
//! started without `--rm` leaves a stopped container behind on every launch.
//! Agents relaunch a failing server many times a day, so a config missing both
//! flags fills the machine with dead containers and never works.
//!
//! Everything here is pure: it inspects a config value and returns findings.
//! Callers decide what to do with them (block a save, skip a sync, warn).

use serde::Serialize;
use serde_json::Value;

/// Marker set on configs imported from an agent's own config file.  The user
/// wrote that entry by hand, so the sync guard reports its errors as warnings
/// instead of removing the server from the file it came from.  Saving the
/// config from the editor drops the marker.
pub const DISCOVERED_MARKER: &str = "_discovered";

/// How serious a finding is.  Only `Error` blocks a save or a sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum McpFindingSeverity {
    Error,
    Warning,
    Info,
}

/// One problem found in an MCP server config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpConfigFinding {
    pub severity: McpFindingSeverity,
    /// Stable machine-readable id, e.g. `docker_missing_interactive`.
    pub code: String,
    /// Plain-language explanation shown to the user.
    pub message: String,
}

/// An official hosted endpoint that replaces a container image.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteEquivalent {
    pub title: String,
    pub url: String,
    /// Transport type as stored in a config: `http` or `sse`.
    pub transport: String,
    /// Shape of the auth header the endpoint accepts, for display only.  No
    /// token is ever filled in here.
    pub auth_header: String,
}

/// What Automatic knows about a published MCP server image.
struct KnownImage {
    /// Image repository without tag or digest.
    repository: &'static str,
    title: &'static str,
    /// Environment variables the server cannot work without.
    required_env: &'static [&'static str],
    remote_url: Option<&'static str>,
    remote_transport: &'static str,
    remote_auth_header: &'static str,
}

/// Images with known requirements or an official remote equivalent.  Add an
/// entry here to teach the validator about another server.
const KNOWN_IMAGES: &[KnownImage] = &[KnownImage {
    repository: "ghcr.io/github/github-mcp-server",
    title: "GitHub",
    required_env: &["GITHUB_PERSONAL_ACCESS_TOKEN"],
    remote_url: Some("https://api.githubcopilot.com/mcp/"),
    remote_transport: "http",
    remote_auth_header: "Authorization: Bearer <token>",
}];

/// Result of validating one MCP server config.
#[derive(Debug, Clone, Default, Serialize)]
pub struct McpConfigValidation {
    pub findings: Vec<McpConfigFinding>,
    /// True when [`apply_docker_stdio_fix`] would change the arguments.
    pub can_fix: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_equivalent: Option<RemoteEquivalent>,
}

impl McpConfigValidation {
    pub fn has_errors(&self) -> bool {
        self.findings
            .iter()
            .any(|f| f.severity == McpFindingSeverity::Error)
    }

    /// Messages of the given severity, in the order they were found.
    pub fn messages(&self, severity: McpFindingSeverity) -> Vec<String> {
        self.findings
            .iter()
            .filter(|f| f.severity == severity)
            .map(|f| f.message.clone())
            .collect()
    }
}

/// The `docker run` flags that matter to a stdio MCP server, plus the image.
#[derive(Debug, Default, PartialEq, Eq)]
struct DockerRunArgs {
    interactive: bool,
    remove: bool,
    tty: bool,
    /// Names from `-e NAME` / `--env NAME` given without a value: the
    /// container receives whatever the launching process has for `NAME`.
    passthrough_env: Vec<String>,
    /// Every name given to `-e` / `--env`, with or without a value.
    env_names: Vec<String>,
    image: Option<String>,
}

/// Long flags of `docker run` / `podman run` that take no value.  Every other
/// long flag consumes the next argument unless written as `--flag=value`.
const BOOLEAN_LONG_FLAGS: &[&str] = &[
    "detach",
    "disable-content-trust",
    "help",
    "init",
    "interactive",
    "no-healthcheck",
    "oom-kill-disable",
    "privileged",
    "publish-all",
    "quiet",
    "read-only",
    "replace",
    "rm",
    "rmi",
    "sig-proxy",
    "tty",
];

/// Short flags that take no value and may be combined, as in `-it`.
const BOOLEAN_SHORT_FLAGS: &[char] = &['d', 'i', 'P', 'q', 't'];

/// True when `command` names the docker or podman CLI, by bare name or path.
fn is_container_cli(command: &str) -> bool {
    let name = command
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let name = name.strip_suffix(".exe").unwrap_or(&name);
    name == "docker" || name == "podman"
}

/// A boolean flag written as `--flag=false` is off; any other form is on.
fn boolean_flag_value(value: Option<&str>) -> bool {
    !matches!(value, Some(v) if v.eq_ignore_ascii_case("false") || v == "0")
}

fn record_env(parsed: &mut DockerRunArgs, spec: &str) {
    match spec.split_once('=') {
        Some((name, _)) => parsed.env_names.push(name.to_string()),
        None => {
            parsed.env_names.push(spec.to_string());
            parsed.passthrough_env.push(spec.to_string());
        }
    }
}

/// Parse the arguments that follow `run`.  Flags are only read up to the image
/// reference; everything after it belongs to the container, so `-i` placed
/// there does not attach stdin.
fn parse_docker_run_args(args: &[String]) -> DockerRunArgs {
    let mut parsed = DockerRunArgs::default();
    let mut rest = args.iter();

    while let Some(arg) = rest.next() {
        if arg == "--" {
            parsed.image = rest.next().cloned();
            break;
        }

        if let Some(long) = arg.strip_prefix("--") {
            let (name, inline) = match long.split_once('=') {
                Some((name, value)) => (name, Some(value)),
                None => (long, None),
            };
            if BOOLEAN_LONG_FLAGS.contains(&name) {
                let on = boolean_flag_value(inline);
                match name {
                    "interactive" => parsed.interactive = on,
                    "rm" => parsed.remove = on,
                    "tty" => parsed.tty = on,
                    _ => {}
                }
                continue;
            }
            let value = match inline {
                Some(value) => Some(value.to_string()),
                None => rest.next().cloned(),
            };
            if name == "env" {
                if let Some(spec) = value {
                    record_env(&mut parsed, &spec);
                }
            }
            continue;
        }

        if arg.len() > 1 && arg.starts_with('-') {
            let cluster = &arg[1..];
            for (idx, flag) in cluster.char_indices() {
                if BOOLEAN_SHORT_FLAGS.contains(&flag) {
                    match flag {
                        'i' => parsed.interactive = true,
                        't' => parsed.tty = true,
                        _ => {}
                    }
                    continue;
                }
                // A value-taking short flag ends the cluster: its value is the
                // remainder (`-eFOO`) or the next argument (`-e FOO`).
                let attached = &cluster[idx + flag.len_utf8()..];
                let value = if attached.is_empty() {
                    rest.next().cloned()
                } else {
                    Some(attached.trim_start_matches('=').to_string())
                };
                if flag == 'e' {
                    if let Some(spec) = value {
                        record_env(&mut parsed, &spec);
                    }
                }
                break;
            }
            continue;
        }

        parsed.image = Some(arg.clone());
        break;
    }

    parsed
}

/// Split an image reference into its repository and tag.  A digest-pinned
/// reference reports the digest as its tag so it is never treated as floating.
fn split_image_reference(image: &str) -> (&str, Option<&str>) {
    if let Some((repository, digest)) = image.split_once('@') {
        let repository = match repository.rsplit_once(':') {
            Some((repo, tag)) if !tag.contains('/') => repo,
            _ => repository,
        };
        return (repository, Some(digest));
    }
    // A colon before the last `/` is a registry port, not a tag.
    match image.rsplit_once(':') {
        Some((repository, tag)) if !tag.contains('/') => (repository, Some(tag)),
        _ => (image, None),
    }
}

fn known_image(repository: &str) -> Option<&'static KnownImage> {
    KNOWN_IMAGES
        .iter()
        .find(|known| known.repository.eq_ignore_ascii_case(repository))
}

fn string_args(config: &Value) -> Vec<String> {
    config
        .get("args")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The arguments after `run`, when `config` is a stdio server launched through
/// `docker run` or `podman run`.  `None` for every other config.
fn docker_run_args(config: &Value) -> Option<Vec<String>> {
    let transport = config.get("type").and_then(Value::as_str).unwrap_or("stdio");
    if transport != "stdio" && transport != "local" {
        return None;
    }
    let command = config.get("command").and_then(Value::as_str)?;
    if !is_container_cli(command) {
        return None;
    }
    let args = string_args(config);
    if args.first().map(String::as_str) != Some("run") {
        return None;
    }
    Some(args[1..].to_vec())
}

fn finding(severity: McpFindingSeverity, code: &str, message: String) -> McpConfigFinding {
    McpConfigFinding {
        severity,
        code: code.to_string(),
        message,
    }
}

/// Validate one MCP server config.  Configs that are not `docker run` stdio
/// servers produce no findings.
pub fn validate_mcp_config(config: &Value) -> McpConfigValidation {
    let Some(run_args) = docker_run_args(config) else {
        return McpConfigValidation::default();
    };
    let parsed = parse_docker_run_args(&run_args);
    let mut findings = Vec::new();

    if !parsed.interactive {
        findings.push(finding(
            McpFindingSeverity::Error,
            "docker_missing_interactive",
            "`-i` is missing. The container gets no stdin, so the server exits as soon as it starts."
                .to_string(),
        ));
    }
    if !parsed.remove {
        findings.push(finding(
            McpFindingSeverity::Warning,
            "docker_missing_rm",
            "`--rm` is missing. Every launch leaves a stopped container behind.".to_string(),
        ));
    }
    if parsed.tty {
        findings.push(finding(
            McpFindingSeverity::Warning,
            "docker_tty",
            "`-t` allocates a terminal, which corrupts the JSON-RPC messages sent over stdio. Remove it."
                .to_string(),
        ));
    }

    let config_env = config.get("env").and_then(Value::as_object);
    for name in &parsed.passthrough_env {
        if config_env.is_some_and(|env| env.contains_key(name)) {
            continue;
        }
        findings.push(finding(
            McpFindingSeverity::Warning,
            "docker_env_not_set",
            format!(
                "`-e {name}` passes `{name}` into the container, but it is not listed under Environment Variables. It reaches the container empty unless the agent's own environment sets it."
            ),
        ));
    }

    let mut remote_equivalent = None;
    if let Some(image) = parsed.image.as_deref() {
        let (repository, tag) = split_image_reference(image);
        if let Some(known) = known_image(repository) {
            for required in known.required_env {
                if parsed.env_names.iter().any(|name| name == required) {
                    continue;
                }
                findings.push(finding(
                    McpFindingSeverity::Warning,
                    "docker_required_env_missing",
                    format!(
                        "The {} server needs `{required}`, but no `-e {required}` flag passes it into the container.",
                        known.title
                    ),
                ));
            }
            remote_equivalent = known.remote_url.map(|url| RemoteEquivalent {
                title: known.title.to_string(),
                url: url.to_string(),
                transport: known.remote_transport.to_string(),
                auth_header: known.remote_auth_header.to_string(),
            });
        }
        match tag {
            None => findings.push(finding(
                McpFindingSeverity::Info,
                "docker_image_untagged",
                format!("`{image}` has no tag, so Docker uses `latest` and the version can change between launches."),
            )),
            Some("latest") => findings.push(finding(
                McpFindingSeverity::Info,
                "docker_image_latest",
                format!("`{image}` uses `latest`, so the version can change between launches."),
            )),
            Some(_) => {}
        }
    } else {
        findings.push(finding(
            McpFindingSeverity::Error,
            "docker_missing_image",
            "No image follows the `docker run` flags, so there is nothing to launch.".to_string(),
        ));
    }

    McpConfigValidation {
        can_fix: !parsed.interactive || !parsed.remove,
        findings,
        remote_equivalent,
    }
}

/// Insert `-i` and `--rm` directly after `run` when they are missing.  Every
/// other argument keeps its position.  Arguments that are not a `docker run`
/// invocation are returned unchanged, and applying the fix twice changes
/// nothing the second time.
pub fn apply_docker_stdio_fix(command: &str, args: &[String]) -> Vec<String> {
    if !is_container_cli(command) || args.first().map(String::as_str) != Some("run") {
        return args.to_vec();
    }
    let parsed = parse_docker_run_args(&args[1..]);
    let mut fixed = vec![args[0].clone()];
    if !parsed.interactive {
        fixed.push("-i".to_string());
    }
    if !parsed.remove {
        fixed.push("--rm".to_string());
    }
    fixed.extend_from_slice(&args[1..]);
    fixed
}

/// True when `config` carries the [`DISCOVERED_MARKER`].
pub fn is_discovered_config(config: &Value) -> bool {
    config
        .get(DISCOVERED_MARKER)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// The reason a config must not be written to agent files, or `None` when it
/// may be synced.  Discovered configs are never blocked: the entry already
/// lives in the user's own agent file, and leaving it out would delete it.
pub fn mcp_sync_block_reason(config: &Value) -> Option<String> {
    if is_discovered_config(config) {
        return None;
    }
    let errors = validate_mcp_config(config).messages(McpFindingSeverity::Error);
    if errors.is_empty() {
        None
    } else {
        Some(errors.join(" "))
    }
}

/// Add the [`DISCOVERED_MARKER`] to a config read from an agent's own file,
/// but only when the sync guard would otherwise block it.  Valid configs are
/// stored untouched.  Returns the input unchanged when it is not JSON, leaving
/// the save path to report that error.
pub fn mark_discovered_if_blocked(config_json: &str) -> String {
    let Ok(mut config) = serde_json::from_str::<Value>(config_json) else {
        return config_json.to_string();
    };
    if mcp_sync_block_reason(&config).is_none() {
        return config_json.to_string();
    }
    let Some(obj) = config.as_object_mut() else {
        return config_json.to_string();
    };
    obj.insert(DISCOVERED_MARKER.to_string(), Value::Bool(true));
    serde_json::to_string_pretty(&config).unwrap_or_else(|_| config_json.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const IMAGE: &str = "ghcr.io/example/server:1.2.3";

    fn docker(args: &[&str]) -> Value {
        json!({ "type": "stdio", "command": "docker", "args": args })
    }

    fn codes(config: &Value) -> Vec<String> {
        validate_mcp_config(config)
            .findings
            .into_iter()
            .map(|f| f.code)
            .collect()
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn separate_interactive_and_rm_flags_pass() {
        assert!(codes(&docker(&["run", "-i", "--rm", IMAGE])).is_empty());
    }

    #[test]
    fn combined_short_flags_count_as_interactive_and_flag_the_tty() {
        assert_eq!(
            codes(&docker(&["run", "-it", "--rm", IMAGE])),
            vec!["docker_tty"]
        );
    }

    #[test]
    fn long_interactive_flag_passes() {
        assert!(codes(&docker(&["run", "--interactive", "--rm", IMAGE])).is_empty());
    }

    #[test]
    fn interactive_flag_after_the_image_does_not_count() {
        assert_eq!(
            codes(&docker(&["run", "--rm", IMAGE, "-i"])),
            vec!["docker_missing_interactive"]
        );
    }

    #[test]
    fn passthrough_env_without_a_config_entry_warns() {
        assert_eq!(
            codes(&docker(&["run", "-i", "--rm", "-e", "FOO", IMAGE])),
            vec!["docker_env_not_set"]
        );
    }

    #[test]
    fn passthrough_env_with_a_config_entry_passes() {
        let mut config = docker(&["run", "-i", "--rm", "-e", "FOO", IMAGE]);
        // An empty value is the "inherit from the environment" marker and
        // still counts as declared.
        config["env"] = json!({ "FOO": "" });
        assert!(codes(&config).is_empty());
    }

    #[test]
    fn env_with_an_inline_value_passes() {
        assert!(codes(&docker(&["run", "-i", "--rm", "--env=FOO=bar", IMAGE])).is_empty());
        assert!(codes(&docker(&["run", "-i", "--rm", "-e", "FOO=${FOO}", IMAGE])).is_empty());
    }

    #[test]
    fn env_file_value_is_not_mistaken_for_the_image() {
        let parsed =
            parse_docker_run_args(&strings(&["-i", "--rm", "--env-file", "secrets.env", IMAGE]));
        assert_eq!(parsed.image.as_deref(), Some(IMAGE));
        assert!(codes(&docker(&["run", "-i", "--rm", "--env-file", "secrets.env", IMAGE]))
            .is_empty());
    }

    #[test]
    fn value_flags_before_the_image_are_skipped() {
        let parsed = parse_docker_run_args(&strings(&[
            "--name", "mcp", "-v", "/a:/b", "-p8080:80", "-i", IMAGE, "--rm",
        ]));
        assert_eq!(parsed.image.as_deref(), Some(IMAGE));
        assert!(parsed.interactive);
        assert!(!parsed.remove);
    }

    #[test]
    fn non_docker_commands_are_not_checked() {
        let config = json!({ "type": "stdio", "command": "npx", "args": ["run", "thing"] });
        assert!(validate_mcp_config(&config).findings.is_empty());
        let remote = json!({ "type": "http", "url": "https://example.com/mcp" });
        assert!(validate_mcp_config(&remote).findings.is_empty());
        let not_run = json!({ "type": "stdio", "command": "docker", "args": ["compose", "up"] });
        assert!(validate_mcp_config(&not_run).findings.is_empty());
    }

    #[test]
    fn docker_paths_podman_and_exe_are_recognised() {
        for command in [
            "/usr/local/bin/docker",
            "podman",
            "docker.exe",
            r"C:\Program Files\Docker\docker.exe",
        ] {
            let config = json!({ "command": command, "args": ["run", IMAGE] });
            assert!(
                validate_mcp_config(&config).has_errors(),
                "{command} should be validated"
            );
        }
    }

    #[test]
    fn untagged_and_latest_images_are_noted() {
        assert_eq!(
            codes(&docker(&["run", "-i", "--rm", "ghcr.io/example/server"])),
            vec!["docker_image_untagged"]
        );
        assert_eq!(
            codes(&docker(&["run", "-i", "--rm", "localhost:5000/server:latest"])),
            vec!["docker_image_latest"]
        );
        // A registry port is not a tag.
        assert_eq!(
            codes(&docker(&["run", "-i", "--rm", "localhost:5000/server"])),
            vec!["docker_image_untagged"]
        );
        assert!(codes(&docker(&["run", "-i", "--rm", "ghcr.io/example/server@sha256:abc"]))
            .is_empty());
    }

    #[test]
    fn the_shipped_github_config_reports_one_error_and_two_warnings() {
        let config = docker(&["run", "ghcr.io/github/github-mcp-server:0.31.0"]);
        let result = validate_mcp_config(&config);
        assert_eq!(result.messages(McpFindingSeverity::Error).len(), 1);
        assert_eq!(result.messages(McpFindingSeverity::Warning).len(), 2);
        assert!(result.messages(McpFindingSeverity::Info).is_empty());
        assert_eq!(
            codes(&config),
            vec![
                "docker_missing_interactive",
                "docker_missing_rm",
                "docker_required_env_missing"
            ]
        );
        assert!(result.can_fix);
        assert_eq!(
            result.remote_equivalent.map(|r| r.url).as_deref(),
            Some("https://api.githubcopilot.com/mcp/")
        );
    }

    #[test]
    fn remote_equivalent_matches_any_tag() {
        for image in [
            "ghcr.io/github/github-mcp-server",
            "ghcr.io/github/github-mcp-server:latest",
            "ghcr.io/github/github-mcp-server@sha256:abc",
        ] {
            let result = validate_mcp_config(&docker(&["run", "-i", "--rm", image]));
            assert!(result.remote_equivalent.is_some(), "{image}");
        }
    }

    #[test]
    fn fix_inserts_flags_after_run_and_keeps_argument_order() {
        let args = strings(&["run", "-e", "FOO", IMAGE, "--verbose"]);
        assert_eq!(
            apply_docker_stdio_fix("docker", &args),
            strings(&["run", "-i", "--rm", "-e", "FOO", IMAGE, "--verbose"])
        );
    }

    #[test]
    fn fix_adds_only_the_missing_flag() {
        let args = strings(&["run", "--rm", IMAGE]);
        assert_eq!(
            apply_docker_stdio_fix("docker", &args),
            strings(&["run", "-i", "--rm", IMAGE])
        );
    }

    #[test]
    fn fix_is_idempotent() {
        let once = apply_docker_stdio_fix("docker", &strings(&["run", IMAGE]));
        assert_eq!(apply_docker_stdio_fix("docker", &once), once);
    }

    #[test]
    fn fix_leaves_other_commands_alone() {
        let args = strings(&["run", "thing"]);
        assert_eq!(apply_docker_stdio_fix("npx", &args), args);
    }

    #[test]
    fn sync_is_blocked_on_errors_but_not_on_warnings() {
        assert!(mcp_sync_block_reason(&docker(&["run", IMAGE])).is_some());
        assert!(mcp_sync_block_reason(&docker(&["run", "-i", IMAGE])).is_none());
    }

    #[test]
    fn discovered_configs_are_never_blocked() {
        let marked = mark_discovered_if_blocked(&docker(&["run", IMAGE]).to_string());
        let config: Value = serde_json::from_str(&marked).unwrap();
        assert!(is_discovered_config(&config));
        assert!(mcp_sync_block_reason(&config).is_none());
        // The findings themselves are still reported.
        assert!(validate_mcp_config(&config).has_errors());
    }

    #[test]
    fn valid_discovered_configs_are_stored_untouched() {
        let raw = docker(&["run", "-i", "--rm", IMAGE]).to_string();
        assert_eq!(mark_discovered_if_blocked(&raw), raw);
    }
}
