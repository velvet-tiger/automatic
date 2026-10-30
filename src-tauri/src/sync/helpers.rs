use std::collections::HashSet;

use serde_json::{json, Map, Value};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::core::store_reads::StoreRead;
use crate::core::Project;

/// Largest project file [`read_project_text`] reads, in bytes.
pub(crate) const MAX_PROJECT_READ_BYTES: u64 = 1024 * 1024;

/// Read a project file that drift detection compares, refusing anything that
/// is not plain project text.
///
/// Drift runs on a timer against cloned repositories, so a committed
/// symlink to `/dev/zero`, a FIFO, or a huge file must not hang or exhaust
/// the check, and nothing outside the project may reach the drift diff.
///
/// - The path is resolved in full, so a symlink anywhere along it (the file
///   itself or a parent such as `.claude -> ~/.claude`) counts.  It must end
///   at a regular file inside `root` and outside any `.git` directory.
///   Automatic's own links (`.claude/skills/x` -> `.agents/skills/x`) stay
///   inside the project, so they pass.
/// - Anything else (FIFO, socket, device, a dangling link, a link that
///   leaves the project) is refused.  Callers that treat a dangling link as
///   absent check `exists()` or `is_file()` first.
/// - At most [`MAX_PROJECT_READ_BYTES`] are read, and a larger file is
///   refused.  Non-UTF-8 content is refused too, as `read_to_string` does.
///
/// Returns `Ok(None)` when nothing exists at `path`, and `Err(reason)` when
/// the file is refused or cannot be read.  The type check uses `stat`, so a
/// FIFO is never opened.  A file swapped for a FIFO between that check and
/// the open can still block the open; closing that race needs `O_NONBLOCK`,
/// which std does not expose portably.  A swap to any other file is caught
/// by comparing the opened file with the checked one.
pub(crate) fn read_project_text(root: &Path, path: &Path) -> Result<Option<String>, String> {
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Failed to inspect {}: {e}", path.display())),
    }
    let target = project_file_target(root, path)?;
    let checked = fs::metadata(&target)
        .map_err(|e| format!("Failed to inspect {}: {e}", path.display()))?;
    if !checked.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }

    let file =
        fs::File::open(&target).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let opened = file
        .metadata()
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    if !same_file(&checked, &opened) {
        return Err(format!("{} changed while it was being read", path.display()));
    }
    let mut bytes = Vec::new();
    file.take(MAX_PROJECT_READ_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    if bytes.len() as u64 > MAX_PROJECT_READ_BYTES {
        return Err(format!(
            "{} is larger than {MAX_PROJECT_READ_BYTES} bytes",
            path.display()
        ));
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| format!("{} is not UTF-8 text", path.display()))
}

/// The fully resolved `path`, when it lies inside `root` and outside any
/// `.git` directory.  A `.git/config` target could carry credentials in a
/// remote URL, and drift shows file contents.
fn project_file_target(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let outside = || {
        format!(
            "{} resolves to something other than a file inside the project",
            path.display()
        )
    };
    let (Ok(target), Ok(root)) = (fs::canonicalize(path), fs::canonicalize(root)) else {
        return Err(outside());
    };
    if resolves_inside(&root, &target) {
        Ok(target)
    } else {
        Err(outside())
    }
}

/// `true` when the canonical `target` lies inside the canonical `root` and
/// outside any `.git` directory.
fn resolves_inside(root: &Path, target: &Path) -> bool {
    target.strip_prefix(root).is_ok_and(|rel| {
        !rel.components()
            .any(|c| c.as_os_str().eq_ignore_ascii_case(".git"))
    })
}

/// Why sync must not write a file or create a directory at `path`, or
/// `None` when it may.
///
/// The write-side twin of [`read_project_text`].  A cloned repository can
/// commit `CLAUDE.md -> ~/.bashrc`, `.cursor -> /elsewhere` or a FIFO, and a
/// plain `fs::write` follows the link or blocks.  So:
///
/// - An existing `path` is resolved in full and must land inside `root`,
///   outside any `.git` directory, on a regular file or a directory.  A
///   FIFO, socket or device is refused, and so is a dangling link, since
///   writing would create its target.  Links that stay inside the project
///   (`CLAUDE.md -> AGENTS.md`) pass.  A file where a directory is expected
///   (or the reverse) is left for the write itself to fail on: it cannot
///   reach outside the project.
/// - For an absent `path`, the nearest existing ancestor must resolve inside
///   `root`.  That catches a linked parent such as `.claude -> ~/.claude`.
/// - A `..` component is refused outright.  The ancestor walk is lexical, so
///   `new/../../x` would pass it at `root`, and `create_dir_all` would then
///   resolve the `..` and create `x` beside the project.  Names from the
///   committed config or from agent frontmatter can carry one.
///
/// The check runs before the write, so a path swapped in between is not
/// caught.  The threat is committed repository content, not a concurrent
/// attacker, and std has no portable `O_NOFOLLOW`/`openat` to close the gap.
pub(crate) fn project_write_refusal(root: &Path, path: &Path) -> Option<String> {
    let given_root = root;
    let Ok(root) = fs::canonicalize(root) else {
        return Some(format!("project directory {} cannot be resolved", root.display()));
    };
    let outside = || {
        format!(
            "{} resolves to somewhere outside the project",
            path.display()
        )
    };
    // Only the part below the root may not hold `..`: a project registered
    // as `/a/../proj` is the user's own choice of path.
    let below_root = path
        .strip_prefix(given_root)
        .or_else(|_| path.strip_prefix(&root))
        .unwrap_or(path);
    if below_root
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Some(format!("{} contains a `..` component", path.display()));
    }
    // Walk up from `path` to the first entry that exists.  NotADirectory
    // means a parent is a regular file (Cline's legacy `.clinerules` file
    // under `.clinerules/automatic.md`), which is still a step up.
    let mut candidate = path;
    loop {
        match fs::symlink_metadata(candidate) {
            Ok(_) => break,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                match candidate.parent() {
                    Some(parent) => candidate = parent,
                    None => return Some(outside()),
                }
            }
            Err(e) => return Some(format!("Failed to inspect {}: {e}", candidate.display())),
        }
    }
    let Ok(target) = fs::canonicalize(candidate) else {
        return Some(format!("{} is a link to nothing", candidate.display()));
    };
    if !resolves_inside(&root, &target) {
        return Some(outside());
    }
    let Ok(meta) = fs::metadata(&target) else {
        return Some(format!("{} cannot be inspected", candidate.display()));
    };
    if meta.is_file() || meta.is_dir() {
        None
    } else {
        Some(format!(
            "{} is not a regular file or directory",
            candidate.display()
        ))
    }
}

/// Write `contents` to `path` after [`project_write_refusal`] clears it,
/// creating missing parent directories.  A refusal is returned as `Err`.
pub(crate) fn write_project_file(
    root: &Path,
    path: &Path,
    contents: impl AsRef<[u8]>,
) -> Result<(), String> {
    if let Some(reason) = project_write_refusal(root, path) {
        return Err(format!("Refused to write {}: {reason}", path.display()));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }
    fs::write(path, contents).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

/// `create_dir_all(dir)` after [`project_write_refusal`] clears it.
pub(crate) fn create_project_dir(root: &Path, dir: &Path) -> Result<(), String> {
    if let Some(reason) = project_write_refusal(root, dir) {
        return Err(format!("Refused to create {}: {reason}", dir.display()));
    }
    fs::create_dir_all(dir).map_err(|e| format!("Failed to create {}: {e}", dir.display()))
}

/// [`write_project_file`] for one file among many (a sub-agent, a command,
/// a rule).  A refused target is skipped with a log line so the other files
/// still sync; drift reports the refused file as unreadable.  Returns
/// whether the file was written.
pub(crate) fn write_project_file_or_skip(
    root: &Path,
    path: &Path,
    contents: impl AsRef<[u8]>,
) -> Result<bool, String> {
    if let Some(reason) = project_write_refusal(root, path) {
        eprintln!("[automatic] sync: not writing {}: {reason}", path.display());
        return Ok(false);
    }
    write_project_file(root, path, contents).map(|()| true)
}

/// Why the Silent-mode directory `effective_dir` must not be used, or
/// `None` when it is the project root, absent, or inside the project.
///
/// A committed `.automatic/silent -> /elsewhere` (or a linked `.automatic`)
/// would otherwise send every synced file out of the project.
pub(crate) fn escaped_silent_dir(dir: &Path, effective_dir: &Path) -> Option<String> {
    if effective_dir == dir {
        return None;
    }
    if fs::symlink_metadata(effective_dir).is_err() {
        // Absent: the ancestors still matter, since creating the directory
        // would follow a linked `.automatic`.
        return project_write_refusal(dir, effective_dir);
    }
    let inside = match (fs::canonicalize(effective_dir), fs::canonicalize(dir)) {
        (Ok(silent), Ok(root)) => resolves_inside(&root, &silent),
        _ => false,
    };
    (!inside).then(|| {
        format!(
            "{} resolves to somewhere outside the project",
            effective_dir.display()
        )
    })
}

/// Why `agent`'s root MCP config files in `dir` must not be read, or `None`.
///
/// Shared by drift's Silent-mode problem check and autodetect's discovery,
/// which both read these files through the uncontained discovery reader.
pub(crate) fn refused_root_mcp_file(agent: &dyn crate::agent::Agent, dir: &Path) -> Option<String> {
    agent
        .mcp_merge_inputs(dir)
        .into_iter()
        .chain(agent.owned_config_paths(dir))
        // Absent and dangling paths fail discovery's own read harmlessly;
        // a FIFO or device must reach the check.
        .filter(|p| fs::metadata(p).is_ok_and(|m| !m.is_dir()))
        .find_map(|p| read_project_text(dir, &p).err())
}

/// `true` when `a` and `b` describe the same file.  Off unix there is no
/// stable file id in std, so only the type is compared.
fn same_file(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        a.dev() == b.dev() && a.ino() == b.ino()
    }
    #[cfg(not(unix))]
    {
        a.is_file() == b.is_file()
    }
}

/// Load the configs of the named MCP servers from the Automatic registry.
///
/// Pass the names the caller goes on to look up in the returned map. Only
/// those servers are read, so a server that cannot be read blocks the
/// projects and agents that use it and no others. A name is matched against
/// the registry listing exactly, and a name with no registry entry is left
/// out of the map.
///
/// A selected server that exists but cannot be read is an error. Leaving it
/// out would make the caller treat it as deleted and drop it from agent
/// config files.
pub(crate) fn load_mcp_server_configs(
    server_names: &[String],
) -> Result<Map<String, Value>, String> {
    let wanted: HashSet<&str> = server_names.iter().map(String::as_str).collect();
    let mut servers = Map::new();

    for name in crate::core::list_mcp_server_configs()? {
        if !wanted.contains(name.as_str()) {
            continue;
        }
        let raw = crate::core::read_mcp_server_config_if_present(&name)
            .map_err(|e| format!("MCP server '{}' could not be read: {}", name, e))?;
        // Deleted between the listing and the read.
        let Some(raw) = raw else {
            continue;
        };
        let config = serde_json::from_str::<Value>(&raw)
            .map_err(|e| format!("MCP server '{}' holds invalid JSON: {}", name, e))?;
        servers.insert(name, config);
    }

    Ok(servers)
}

/// Read all skill contents from the global registry for the given names.
///
/// Uses the raw SKILL.md content (without companion resource formatting) so
/// that the content written to project skill files during sync matches exactly
/// what drift detection compares against.  Companion files are handled
/// separately by `copy_skills_to_project` which copies the full directory.
pub(crate) fn load_skill_contents(skill_names: &[String]) -> Vec<(String, String)> {
    let mut contents = Vec::new();
    for name in skill_names {
        match crate::core::read_skill_raw(name) {
            Ok(content) if !content.is_empty() => {
                contents.push((name.clone(), content));
            }
            _ => {}
        }
    }
    contents
}

/// Build the combined skill contents and custom skill name list for a project,
/// deduplicating custom_skills entries whose names already appear in the
/// project's library-backed `skills` list.  Library wins because
/// `copy_skills_to_project` writes library content to disk; a stale
/// custom_skills snapshot would otherwise produce perpetual drift.
pub(crate) fn build_skill_contents(project: &Project) -> (Vec<(String, String)>, Vec<String>) {
    let mut skill_contents = load_skill_contents(&project.skills);
    let library_skill_names: HashSet<&str> = project.skills.iter().map(|s| s.as_str()).collect();
    let custom_skills = project.custom_skills.as_deref().unwrap_or(&[]);
    for cs in custom_skills {
        if library_skill_names.contains(cs.name.as_str()) {
            continue;
        }
        skill_contents.push((cs.name.clone(), cs.content.clone()));
    }
    let custom_skill_names: Vec<String> = custom_skills
        .iter()
        .filter(|s| !library_skill_names.contains(s.name.as_str()))
        .map(|s| s.name.clone())
        .collect();
    (skill_contents, custom_skill_names)
}

/// Prune `custom_skills` entries whose name already appears in the project's
/// library-backed `skills` list.  Returns `true` if any entries were removed.
pub(crate) fn prune_shadowed_custom_skills(project: &mut Project) -> bool {
    let library_names: HashSet<&str> = project.skills.iter().map(|s| s.as_str()).collect();
    let Some(custom) = project.custom_skills.as_mut() else {
        return false;
    };
    let before = custom.len();
    custom.retain(|cs| !library_names.contains(cs.name.as_str()));
    let changed = custom.len() != before;
    if custom.is_empty() {
        project.custom_skills = None;
    }
    changed
}

/// Which project-scoped custom asset kind diverged from disk.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CustomAssetKind {
    Skill,
    Rule,
    Agent,
    Command,
}

/// Conflict when a project-scoped custom asset's on-disk file differs from the
/// content stored in the project config.
///
/// Unlike library assets (Automatic is source of truth and overwrites on sync),
/// custom skills/rules/agents/commands are authored per-project. When disk and
/// stored content diverge, the user must choose: adopt the on-disk version into
/// Automatic, or overwrite disk with the stored version. Sync favours on-disk
/// and will not silently clobber it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CustomAssetConflict {
    pub kind: CustomAssetKind,
    /// Display / machine name of the asset (skill dir, rule name, agent name, command name).
    pub name: String,
    /// Relative path of the on-disk file from the project (or silent) root.
    pub path: String,
    pub disk_content: String,
    pub automatic_content: String,
}

/// Managed header written at the top of `.automatic/instructions/*.md` files.
const INSTRUCTIONS_MANAGED_HEADER: &str =
    "<!-- managed by Automatic — do not edit by hand -->\n\n";

fn strip_instructions_managed_header(content: &str) -> String {
    content
        .strip_prefix(INSTRUCTIONS_MANAGED_HEADER)
        .unwrap_or(content)
        .trim_end()
        .to_string()
}

fn custom_rule_slug(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// A project-scoped custom asset whose on-disk file [`read_project_text`]
/// refused or could not read.
#[derive(Debug, Clone)]
pub(crate) struct UnreadableCustomAsset {
    pub kind: CustomAssetKind,
    pub name: String,
    /// Relative path of the on-disk file from the project (or silent) root.
    pub path: String,
    /// The agent whose directory holds the file, or `None` for a file in a
    /// location every agent shares (`.agents/`, `.automatic/instructions/`).
    pub agent_id: Option<String>,
    pub reason: String,
}

/// What [`collect_custom_asset_conflicts`] found on disk.
///
/// An unreadable file has no content to adopt, so it is not a conflict, but
/// sync must still leave it alone: writing would follow a symlink out of the
/// project.  [`conflicting_names`] therefore counts both lists.
#[derive(Debug, Default)]
pub(crate) struct CustomAssetScan {
    pub conflicts: Vec<CustomAssetConflict>,
    pub unreadable: Vec<UnreadableCustomAsset>,
}

impl CustomAssetScan {
    fn extend(&mut self, other: CustomAssetScan) {
        self.conflicts.extend(other.conflicts);
        self.unreadable.extend(other.unreadable);
    }

    fn push_unreadable(
        &mut self,
        kind: CustomAssetKind,
        name: &str,
        path: String,
        agent_id: Option<&str>,
        reason: String,
    ) {
        self.unreadable.push(UnreadableCustomAsset {
            kind,
            name: name.to_string(),
            path,
            agent_id: agent_id.map(str::to_string),
            reason,
        });
    }
}

/// Collect conflicts for all project-scoped custom assets (skills, rules,
/// agents, commands). Missing on-disk files are not conflicts — sync creates
/// them from the stored snapshot.
pub(crate) fn collect_custom_asset_conflicts(project: &Project, dir: &Path) -> CustomAssetScan {
    let mut scan = CustomAssetScan::default();
    scan.extend(collect_custom_skill_conflicts(project, dir));
    scan.extend(collect_custom_rule_conflicts(project, dir));
    scan.extend(collect_custom_agent_conflicts(project, dir));
    scan.extend(collect_custom_command_conflicts(project, dir));
    scan
}

/// Compare each non-library `custom_skills` entry against
/// `.agents/skills/<name>/SKILL.md` under `dir`.
pub(crate) fn collect_custom_skill_conflicts(project: &Project, dir: &Path) -> CustomAssetScan {
    let library_names: HashSet<&str> = project.skills.iter().map(|s| s.as_str()).collect();
    let custom = project.custom_skills.as_deref().unwrap_or(&[]);
    let mut scan = CustomAssetScan::default();

    for cs in custom {
        if library_names.contains(cs.name.as_str()) {
            continue;
        }
        let skill_md = dir
            .join(".agents")
            .join("skills")
            .join(&cs.name)
            .join("SKILL.md");
        let path = format!(".agents/skills/{}/SKILL.md", cs.name);
        let disk_content = match read_project_text(dir, &skill_md) {
            Ok(Some(c)) => c,
            Ok(None) => continue,
            Err(reason) => {
                scan.push_unreadable(CustomAssetKind::Skill, &cs.name, path, None, reason);
                continue;
            }
        };
        if disk_content == cs.content {
            continue;
        }
        scan.conflicts.push(CustomAssetConflict {
            kind: CustomAssetKind::Skill,
            name: cs.name.clone(),
            path,
            disk_content,
            automatic_content: cs.content.clone(),
        });
    }

    scan
}

/// Compare each `custom_rules` entry against
/// `.automatic/instructions/custom-<slug>.md` under the real project directory
/// (instructions always live at the project root, even in Silent mode).
pub(crate) fn collect_custom_rule_conflicts(project: &Project, _dir: &Path) -> CustomAssetScan {
    // Custom rule files always live under the real project directory.
    let root = Path::new(&project.directory);
    let mut scan = CustomAssetScan::default();

    for cr in &project.custom_rules {
        if cr.content.trim().is_empty() {
            continue;
        }
        let slug = custom_rule_slug(&cr.name);
        let file = root
            .join(".automatic")
            .join("instructions")
            .join(format!("custom-{}.md", slug));
        let path = format!(".automatic/instructions/custom-{}.md", slug);
        let raw = match read_project_text(root, &file) {
            Ok(Some(c)) => c,
            Ok(None) => continue,
            Err(reason) => {
                scan.push_unreadable(CustomAssetKind::Rule, &cr.name, path, None, reason);
                continue;
            }
        };
        let disk_body = strip_instructions_managed_header(&raw);
        let stored_body = cr.content.trim_end().to_string();
        if disk_body == stored_body {
            continue;
        }
        scan.conflicts.push(CustomAssetConflict {
            kind: CustomAssetKind::Rule,
            name: cr.name.clone(),
            path,
            disk_content: disk_body,
            automatic_content: stored_body,
        });
    }

    scan
}

/// Compare each `custom_agents` entry against its file under every
/// configured agent's `agents_dir`, reporting the first that differs or is
/// refused.
pub(crate) fn collect_custom_agent_conflicts(project: &Project, dir: &Path) -> CustomAssetScan {
    let custom = project.custom_agents.as_deref().unwrap_or(&[]);
    let mut scan = CustomAssetScan::default();

    for ca in custom {
        let machine_name = extract_agent_machine_name(&ca.content)
            .unwrap_or_else(|| ca.name.to_lowercase().replace(' ', "-"));

        for agent_id in &project.agents {
            let Some(agent_instance) = crate::agent::from_id(agent_id) else {
                continue;
            };
            let Some(agents_dir) = agent_instance.agents_dir(dir) else {
                continue;
            };
            // Read the file sync writes.  A compound name such as Copilot's
            // `{name}.agent.md` is not `{name}.{ext}`, and a scan that misses
            // the file lets the next sync overwrite a hand edit.
            let file_path = agents_dir.join(agent_instance.agent_file_name(&machine_name));
            let relative = file_path
                .strip_prefix(dir)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| file_path.display().to_string());
            let disk_content = match read_project_text(dir, &file_path) {
                Ok(Some(c)) => c,
                Ok(None) => continue,
                Err(reason) => {
                    scan.push_unreadable(
                        CustomAssetKind::Agent,
                        &ca.name,
                        relative,
                        Some(agent_id),
                        reason,
                    );
                    break; // one entry per custom agent
                }
            };
            let expected =
                agent_instance.convert_agent_content(&ca.content, &machine_name);
            if disk_content == expected {
                // Matches here.  Keep looking: a file in another agent's
                // directory can still differ, or be a link out of the
                // project that sync must not write through.
                continue;
            }
            scan.conflicts.push(CustomAssetConflict {
                kind: CustomAssetKind::Agent,
                name: ca.name.clone(),
                path: relative,
                disk_content,
                automatic_content: expected,
            });
            break; // one conflict entry per custom agent
        }
    }

    scan
}

/// Compare each `custom_commands` entry against `.agents/commands/<name>.md`.
pub(crate) fn collect_custom_command_conflicts(project: &Project, dir: &Path) -> CustomAssetScan {
    let custom = project.custom_commands.as_deref().unwrap_or(&[]);
    let mut scan = CustomAssetScan::default();

    for cc in custom {
        let cmd_path = dir
            .join(".agents")
            .join("commands")
            .join(format!("{}.md", cc.name));
        let path = format!(".agents/commands/{}.md", cc.name);
        let disk_content = match read_project_text(dir, &cmd_path) {
            Ok(Some(c)) => c,
            Ok(None) => continue,
            Err(reason) => {
                scan.push_unreadable(CustomAssetKind::Command, &cc.name, path, None, reason);
                continue;
            }
        };
        let expected = crate::agent::render_markdown_command(&cc.content);
        if disk_content == expected {
            continue;
        }
        scan.conflicts.push(CustomAssetConflict {
            kind: CustomAssetKind::Command,
            name: cc.name.clone(),
            path,
            disk_content,
            automatic_content: expected,
        });
    }

    scan
}

/// Names of assets of a given kind that sync must not write: those that
/// conflict with disk and those whose disk file is unreadable.
pub(crate) fn conflicting_names(scan: &CustomAssetScan, kind: CustomAssetKind) -> HashSet<String> {
    scan.conflicts
        .iter()
        .filter(|c| c.kind == kind)
        .map(|c| c.name.clone())
        .chain(
            scan.unreadable
                .iter()
                .filter(|u| u.kind == kind)
                .map(|u| u.name.clone()),
        )
        .collect()
}

/// Find the Automatic binary path.
///
/// Delegates to the canonical resolver so the sync engine, drift check, and
/// MCP registry all emit the identical path string regardless of how the
/// current process was invoked (GUI bundle vs CLI symlink).
pub(crate) fn find_automatic_binary() -> String {
    crate::core::automatic_binary_path()
}

/// Strip any legacy `<!-- automatic:skills:start -->…<!-- automatic:skills:end -->`
/// managed section from a project file.  Returns the path if the file was
/// modified, or None if no cleanup was needed.
pub(crate) fn clean_project_file(dir: &PathBuf, filename: &str) -> Result<Option<String>, String> {
    let path = dir.join(filename);
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;

    let start_marker = "<!-- automatic:skills:start -->";
    let end_marker = "<!-- automatic:skills:end -->";

    if let (Some(start), Some(end)) = (content.find(start_marker), content.find(end_marker)) {
        let before = &content[..start];
        let after = &content[end + end_marker.len()..];
        let cleaned = format!(
            "{}{}",
            before.trim_end(),
            if after.trim().is_empty() {
                "\n".to_string()
            } else {
                format!("\n\n{}", after.trim_start())
            }
        );
        fs::write(&path, cleaned).map_err(|e| e.to_string())?;
        Ok(Some(path.display().to_string()))
    } else {
        Ok(None)
    }
}

/// The `AUTOMATIC_PROJECT` value for a project's agent configs: the
/// project `id`. The id is committed with the repo and agent configs such
/// as `.mcp.json` are often committed too, so a machine-local `local_key`
/// would be wrong on every other machine. `mcp-serve` uses the working
/// directory to pick between checkouts that share an id.
///
/// A project with no id yet (its startup backfill failed) gets its name,
/// which `mcp-serve` still resolves. Its configs drift once the id exists,
/// and the next sync writes the id.
pub(crate) fn automatic_project_env_value(project: &crate::core::Project) -> &str {
    if project.id.is_empty() {
        &project.name
    } else {
        &project.id
    }
}

pub(crate) fn add_unique(items: &mut Vec<String>, value: &str) -> bool {
    if items.iter().any(|v| v == value) {
        false
    } else {
        items.push(value.to_string());
        true
    }
}

/// The `url` of a server config, or an empty string when it has none. A
/// server's OAuth token is filed under this URL.
fn server_url(config: &Value) -> &str {
    config.get("url").and_then(|v| v.as_str()).unwrap_or_default()
}

/// Build the selected MCP server map for a project, applying all
/// transformations that the sync engine uses (stripping internal `_`-prefixed
/// fields, substituting OAuth proxy configs for HTTP servers with stored
/// tokens).
///
/// Both `engine.rs` and `drift.rs` must use this function to ensure the
/// expected config matches what is actually written to disk.
///
/// `automatic_project` is the value of the `automatic` entry's
/// `AUTOMATIC_PROJECT` env var. Callers pass
/// [`automatic_project_env_value`].
///
/// An error means the secret store could not say whether an HTTP server has
/// an OAuth token. That decides between the proxy stub and the server's own
/// URL, so there is no map to return: writing the URL over a stub would cut
/// a signed-in server off from its token. The error names the server.
///
/// `read` says who is asking. A sync passes [`StoreRead::Now`]. The drift
/// check runs on a timer and passes [`StoreRead::Background`], so it does not
/// ask a store that has just failed.
pub(crate) fn build_selected_servers(
    automatic_project: &str,
    server_names: &[String],
    mcp_config: &Map<String, Value>,
    read: StoreRead,
) -> Result<Map<String, Value>, String> {
    let mut selected_servers = Map::new();
    let automatic_binary = find_automatic_binary();

    // Always include the Automatic MCP server
    selected_servers.insert(
        "automatic".to_string(),
        json!({
            "command": automatic_binary,
            "args": ["mcp-serve"],
            "env": {
                "AUTOMATIC_PROJECT": automatic_project
            }
        }),
    );

    // Add project-selected MCP servers from the Automatic registry.
    // Strip Automatic-internal fields (prefixed with `_`) before writing to agent files.
    //
    // For HTTP servers that have a stored OAuth token in the keychain, we emit
    // a local stdio proxy config instead of the remote URL.  This keeps the
    // token out of every project file — the proxy loads it from the keychain
    // at runtime.
    for server_name in server_names {
        if server_name == "automatic" {
            continue;
        }
        if let Some(server_config) = mcp_config.get(server_name) {
            // A config that cannot work is left out of every agent file.
            // Drift detection builds its expected map through this same
            // function, so a skipped server never reads as drift.
            if crate::core::mcp_sync_block_reason(server_config).is_some() {
                continue;
            }
            let cleaned = strip_internal_fields(server_config.clone());

            // Check if this is an HTTP server with a stored OAuth token. Only
            // HTTP and SSE servers can have one, and reading the keychain is
            // expensive enough that stdio servers should not pay for a lookup
            // whose result is discarded.
            let is_http = cleaned
                .get("type")
                .and_then(|v| v.as_str())
                .map(|t| t == "http" || t == "sse")
                .unwrap_or(false);

            if is_http
                && crate::proxy::oauth_token_presence(server_name, server_url(&cleaned), read)?
            {
                // Emit a local proxy config instead of the remote URL.
                selected_servers.insert(
                    server_name.clone(),
                    json!({
                        "command": automatic_binary,
                        "args": ["mcp-proxy", server_name],
                    }),
                );
            } else {
                // Empty env values are left as-is here: they are the canonical
                // "inherit from the environment" marker, and each agent renders
                // them in its own placeholder syntax via
                // `agent::prepare_mcp_servers` at write time.
                selected_servers.insert(server_name.clone(), cleaned);
            }
        }
    }

    Ok(selected_servers)
}

/// Build the selected MCP server map for a *global* (user-level) write,
/// applying the same OAuth-proxy substitution and internal-field stripping as
/// [`build_selected_servers`] but with the project-only concerns removed:
///
/// - The `"automatic"` builtin is NOT injected — its config bakes in a
///   project name via `AUTOMATIC_PROJECT` and registering it at multiple
///   scopes makes Claude Code deduplicate and drop tools.  See the doc on
///   [`crate::core::ensure_automatic_in_global_mcp`].
/// - `AUTOMATIC_PROJECT` env is therefore absent from every entry.
///
/// OAuth'd HTTP servers still become `mcp-proxy` stubs — the proxy loads the
/// token from the keychain and does not depend on any project context.
///
/// An error means the same as it does for [`build_selected_servers`].
pub(crate) fn build_global_selected_servers(
    server_names: &[String],
    mcp_config: &Map<String, Value>,
) -> Result<Map<String, Value>, String> {
    let mut selected_servers = Map::new();
    let automatic_binary = find_automatic_binary();

    for server_name in server_names {
        if server_name == "automatic" {
            continue;
        }
        let Some(server_config) = mcp_config.get(server_name) else {
            continue;
        };
        if crate::core::mcp_sync_block_reason(server_config).is_some() {
            continue;
        }
        let cleaned = strip_internal_fields(server_config.clone());

        let is_http = cleaned
            .get("type")
            .and_then(|v| v.as_str())
            .map(|t| t == "http" || t == "sse")
            .unwrap_or(false);

        if is_http && crate::proxy::has_oauth_token(server_name, server_url(&cleaned))? {
            selected_servers.insert(
                server_name.clone(),
                json!({
                    "command": automatic_binary,
                    "args": ["mcp-proxy", server_name],
                }),
            );
        } else {
            selected_servers.insert(server_name.clone(), cleaned);
        }
    }

    Ok(selected_servers)
}

/// Validation messages for one MCP server, as reported after a sync.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct McpServerNote {
    pub server: String,
    pub messages: Vec<String>,
}

/// What config validation decided for a set of servers: which ones the sync
/// guard leaves out of agent files, and which ones sync with warnings.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct McpConfigSyncNotes {
    pub skipped: Vec<McpServerNote>,
    pub warnings: Vec<McpServerNote>,
}

impl McpConfigSyncNotes {
    pub fn is_empty(&self) -> bool {
        self.skipped.is_empty() && self.warnings.is_empty()
    }
}

/// Validate each named server and sort the results into skipped and warned.
/// A discovered config is never skipped, so its errors are listed with the
/// warnings.  Servers missing from the registry are ignored here; the sync
/// already drops them.
pub(crate) fn collect_mcp_config_notes(
    server_names: &[String],
    mcp_config: &Map<String, Value>,
) -> McpConfigSyncNotes {
    use crate::core::McpFindingSeverity;

    let mut notes = McpConfigSyncNotes::default();
    for server_name in server_names {
        let Some(config) = mcp_config.get(server_name) else {
            continue;
        };
        let validation = crate::core::validate_mcp_config(config);
        let errors = validation.messages(McpFindingSeverity::Error);
        let mut warnings = validation.messages(McpFindingSeverity::Warning);

        if crate::core::mcp_sync_block_reason(config).is_some() {
            notes.skipped.push(McpServerNote {
                server: server_name.clone(),
                messages: errors,
            });
            continue;
        }
        let mut messages = errors;
        messages.append(&mut warnings);
        if !messages.is_empty() {
            notes.warnings.push(McpServerNote {
                server: server_name.clone(),
                messages,
            });
        }
    }
    notes
}

/// Remove fields whose names start with `_` from a JSON object.
/// These are Automatic-internal metadata fields (e.g. `_author`) that should
/// never be written to agent configuration files.
pub(crate) fn strip_internal_fields(mut value: Value) -> Value {
    if let Value::Object(ref mut map) = value {
        map.retain(|key, _| !key.starts_with('_'));
    }
    value
}

/// Extract the machine name (slug) from agent frontmatter content.
/// Returns the `name` field converted to lowercase with spaces replaced by hyphens.
pub fn extract_agent_machine_name(content: &str) -> Option<String> {
    if !content.starts_with("---\n") {
        return None;
    }
    let end = content[4..].find("\n---")?;
    let yaml = &content[4..end + 4];
    for line in yaml.lines() {
        let line = line.trim();
        if let Some(name_val) = line.strip_prefix("name:") {
            let name = name_val.trim().trim_matches('"').trim_matches('\'');
            if !name.is_empty() {
                return Some(name.to_lowercase().replace(' ', "-"));
            }
        }
    }
    None
}

/// Sync project-local custom agents to a project's agents directory.
///
/// For each custom agent:
/// 1. Extract the machine name from the agent's frontmatter
/// 2. Write to `agents_dir/{machine_name}.md`
///
/// Agents whose names appear in `skip_names` are left untouched on disk
/// (conflict: favour on-disk content until the user resolves it).
///
/// Returns the list of files written.
pub(crate) fn sync_custom_agents(
    root: &Path,
    agents_dir: &std::path::Path,
    custom_agents: &[crate::core::CustomAgent],
    agent: &dyn crate::agent::Agent,
    skip_names: &HashSet<String>,
) -> Result<Vec<String>, String> {
    if custom_agents.is_empty() {
        return Ok(Vec::new());
    }

    create_project_dir(root, agents_dir)?;

    let mut written = Vec::new();

    for custom_agent in custom_agents {
        if skip_names.contains(&custom_agent.name) {
            continue;
        }
        let machine_name = extract_agent_machine_name(&custom_agent.content)
            .unwrap_or_else(|| custom_agent.name.to_lowercase().replace(' ', "-"));
        if !crate::agent::is_plain_file_name(&machine_name) {
            eprintln!(
                "[automatic] sync: not syncing agent '{machine_name}': not a plain file name"
            );
            continue;
        }
        let converted_content = agent.convert_agent_content(&custom_agent.content, &machine_name);
        let path = agents_dir.join(agent.agent_file_name(&machine_name));

        if write_project_file_or_skip(root, &path, &converted_content)? {
            written.push(path.display().to_string());
        }
    }

    Ok(written)
}

/// Sync workspace user agents (from `~/.automatic/agents/`) to a project's
/// agents directory.
///
/// For each selected agent:
/// 1. Read the agent content from the global registry
/// 2. Convert to the target format if needed (e.g., TOML for Codex)
/// 3. Write to `agents_dir/{agent_file_name(machine_name)}`
/// 4. Remove stale *managed* agent files not in the selected list (but NOT
///    custom agents, and NOT anything the user placed there by hand)
pub(crate) fn sync_user_agents(
    root: &Path,
    agents_dir: &std::path::Path,
    user_agent_names: &[String],
    custom_agent_names: &[String],
    agent: &dyn crate::agent::Agent,
) -> Result<Vec<String>, String> {
    create_project_dir(root, agents_dir)?;

    let mut written = Vec::new();
    // Full filenames, not bare machine names: `agent_file_name` may add a
    // compound extension (Copilot's `{name}.agent.md`), and `Path::extension`
    // only ever sees the last dot-segment, so comparing by filename is the
    // only way to recover the same name that went in — see
    // `Agent::agent_file_name`'s doc comment.
    let mut expected_file_names: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    // Write each selected agent
    for name in user_agent_names {
        if let Ok(content) = crate::core::read_subagent(name) {
            let user_agent: crate::core::Subagent =
                serde_json::from_str(&content).map_err(|e| e.to_string())?;
            let machine_name = extract_agent_machine_name(&user_agent.content)
                .unwrap_or_else(|| name.to_lowercase().replace(' ', "-"));
            if !crate::agent::is_plain_file_name(&machine_name) {
                eprintln!(
                    "[automatic] sync: not syncing agent '{machine_name}': not a plain file name"
                );
                continue;
            }
            let converted_content = agent.convert_agent_content(&user_agent.content, &machine_name);
            let file_name = agent.agent_file_name(&machine_name);
            let path = agents_dir.join(&file_name);

            if write_project_file_or_skip(root, &path, &converted_content)? {
                written.push(path.display().to_string());
            }
            expected_file_names.insert(file_name);
        }
    }

    // Also add custom agent filenames to the expected set so they're not
    // removed as stale.
    for name in custom_agent_names {
        expected_file_names.insert(agent.agent_file_name(name));
    }

    // Remove stale *managed* agent files: not in the expected set, and
    // carrying the automatic-managed marker. A file that fails the marker
    // check is left alone unconditionally — it might be stale, or it might
    // be something the user wrote directly in this directory; without the
    // marker there is no way to tell, so the safe default is to preserve it.
    if agents_dir.exists() {
        if let Ok(entries) = fs::read_dir(agents_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
                    continue;
                };
                if expected_file_names.contains(file_name)
                    || !crate::agent::is_managed_agent_file(&path)
                {
                    continue;
                }
                if fs::remove_file(&path).is_ok() {
                    written.push(path.display().to_string());
                }
            }
        }
    }

    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::{ClaudeCode, GitHubCopilot};
    use crate::core::{save_subagent, CustomAgent};
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    // ── read_project_text ──────────────────────────────────────────────────

    #[test]
    fn read_project_text_reads_files_and_links_inside_the_project() {
        let project = tempfile::tempdir().unwrap();
        let file = project.path().join("AGENTS.md");
        fs::write(&file, "hello\n").unwrap();
        assert_eq!(
            read_project_text(project.path(), &file).unwrap().as_deref(),
            Some("hello\n")
        );
        assert_eq!(
            read_project_text(project.path(), &project.path().join("absent.md")).unwrap(),
            None
        );

        #[cfg(unix)]
        {
            let link = project.path().join("CLAUDE.md");
            std::os::unix::fs::symlink(&file, &link).unwrap();
            assert_eq!(
                read_project_text(project.path(), &link).unwrap().as_deref(),
                Some("hello\n")
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn read_project_text_refuses_a_symlink_that_leaves_the_project() {
        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.json");
        fs::write(&secret, "{}").unwrap();
        let link = project.path().join(".mcp.json");
        std::os::unix::fs::symlink(&secret, &link).unwrap();

        let err = read_project_text(project.path(), &link).unwrap_err();
        assert!(err.contains("inside the project"), "{err}");

        let dev_zero = project.path().join("zero.json");
        std::os::unix::fs::symlink("/dev/zero", &dev_zero).unwrap();
        assert!(read_project_text(project.path(), &dev_zero).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn read_project_text_refuses_a_symlink_into_dot_git() {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir_all(project.path().join(".git")).unwrap();
        let config = project.path().join(".git").join("config");
        fs::write(&config, "[remote]\n").unwrap();
        let link = project.path().join("AGENTS.md");
        std::os::unix::fs::symlink(&config, &link).unwrap();

        assert!(read_project_text(project.path(), &link).is_err());
    }

    /// A FIFO would block `read_to_string` until a writer appears.  The
    /// refusal comes from its file type, so the read never opens it.
    #[cfg(unix)]
    #[test]
    fn read_project_text_refuses_a_fifo_without_blocking() {
        let project = tempfile::tempdir().unwrap();
        let fifo = project.path().join(".mcp.json");
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("run mkfifo");
        assert!(status.success());

        let (tx, rx) = std::sync::mpsc::channel();
        let root = project.path().to_path_buf();
        std::thread::spawn(move || {
            let _ = tx.send(read_project_text(&root, &fifo));
        });
        let err = rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("read blocked on the FIFO")
            .unwrap_err();
        assert!(err.contains("not a regular file"), "{err}");
    }

    /// A parent directory linked out of the project counts, not only a link
    /// at the file itself.
    #[cfg(unix)]
    #[test]
    fn read_project_text_refuses_a_parent_directory_that_leaves_the_project() {
        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("x.md"), "SECRET").unwrap();
        std::os::unix::fs::symlink(outside.path(), project.path().join(".claude")).unwrap();

        let err = read_project_text(project.path(), &project.path().join(".claude/x.md"))
            .unwrap_err();
        assert!(err.contains("inside the project"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn read_project_text_refuses_a_dangling_symlink() {
        let project = tempfile::tempdir().unwrap();
        let link = project.path().join("AGENTS.md");
        std::os::unix::fs::symlink(project.path().join("gone.md"), &link).unwrap();

        assert!(read_project_text(project.path(), &link).is_err());
    }

    #[test]
    fn read_project_text_refuses_an_oversized_file() {
        let project = tempfile::tempdir().unwrap();
        let file = project.path().join("big.md");
        fs::write(&file, vec![b'a'; MAX_PROJECT_READ_BYTES as usize + 1]).unwrap();

        let err = read_project_text(project.path(), &file).unwrap_err();
        assert!(err.contains("larger than"), "{err}");

        fs::write(&file, vec![b'a'; MAX_PROJECT_READ_BYTES as usize]).unwrap();
        assert!(read_project_text(project.path(), &file).unwrap().is_some());
    }

    fn tmp() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct HomeGuard {
        original: Option<String>,
    }

    impl HomeGuard {
        fn set(path: &std::path::Path) -> Self {
            let original = std::env::var("HOME").ok();
            unsafe {
                std::env::set_var("HOME", path);
            }
            Self { original }
        }
    }

    impl Drop for HomeGuard {
        fn drop(&mut self) {
            unsafe {
                if let Some(original) = &self.original {
                    std::env::set_var("HOME", original);
                } else {
                    std::env::remove_var("HOME");
                }
            }
        }
    }

    #[test]
    fn sync_custom_agents_uses_frontmatter_name_and_fallback_name() {
        let dir = tmp();
        let agents_dir = dir.path().join("agents");
        let custom_agents = vec![
            CustomAgent {
                name: "Display Name".to_string(),
                content: "---\nname: Planner Agent\n---\n\nPlan carefully.\n".to_string(),
            },
            CustomAgent {
                name: "Fallback Agent".to_string(),
                content: "No frontmatter here.\n".to_string(),
            },
        ];

        let written = sync_custom_agents(
            dir.path(),
            &agents_dir,
            &custom_agents,
            &ClaudeCode,
            &HashSet::new(),
        )
        .expect("sync");

        assert_eq!(written.len(), 2);
        assert!(agents_dir.join("planner-agent.md").exists());
        assert!(agents_dir.join("fallback-agent.md").exists());
        assert!(fs::read_to_string(agents_dir.join("planner-agent.md"))
            .expect("read planner")
            .contains("Plan carefully."));
    }

    #[test]
    fn sync_user_agents_writes_selected_and_removes_stale_non_custom_files() {
        let _lock = env_lock().lock().expect("env lock");
        let home = tmp();
        let _home_guard = HomeGuard::set(home.path());
        let project = tmp();
        let agents_dir = project.path().join("agents");
        fs::create_dir_all(&agents_dir).expect("create agents dir");

        save_subagent(
            "reviewer-agent",
            "Reviewer Agent",
            "---\nname: Reviewer Agent\n---\n\nReview thoroughly.\n",
        )
        .expect("save reviewer subagent");

        // Simulates a file Automatic itself wrote in an earlier sync for a
        // user agent that has since been removed from the project: carries
        // the managed marker, so the stale sweep must remove it.
        let stale_managed_path = agents_dir.join("stale-agent.md");
        fs::write(
            &stale_managed_path,
            "---\nname: Stale Agent\nautomatic-managed: true\n---\n\nOld content.\n",
        )
        .expect("write stale managed agent");

        // A file the user placed in this directory directly — no marker.
        // Regardless of whether its name happens to collide with anything
        // expected, the stale sweep must never touch it: there is no way to
        // tell it apart from Automatic's own output without the marker, so
        // the safe default is to leave it alone.
        let hand_authored_path = agents_dir.join("hand-authored.md");
        fs::write(
            &hand_authored_path,
            "---\nname: Hand Authored\n---\n\nI wrote this myself.\n",
        )
        .expect("write hand-authored agent");

        let custom_path = agents_dir.join("custom-agent.md");
        fs::write(&custom_path, "---\nname: Custom Agent\n---\n\nKeep me.\n")
            .expect("write custom agent");

        let written = sync_user_agents(
            project.path(),
            &agents_dir,
            &["reviewer-agent".to_string()],
            &["custom-agent".to_string()],
            &ClaudeCode,
        )
        .expect("sync user agents");

        assert!(written.iter().any(|p| p.ends_with("reviewer-agent.md")));
        assert!(written.iter().any(|p| p.ends_with("stale-agent.md")));
        assert!(agents_dir.join("reviewer-agent.md").exists());
        assert!(!stale_managed_path.exists());
        assert!(hand_authored_path.exists());
        assert!(custom_path.exists());
        assert!(fs::read_to_string(agents_dir.join("reviewer-agent.md"))
            .expect("read synced user agent")
            .contains("Review thoroughly."));
    }

    /// GitHub Copilot's `{name}.agent.md` compound extension is exactly the
    /// case that broke the old `file_stem()`-based stale sweep: `mine.agent.md`
    /// stems to `mine.agent`, which fails `is_valid_agent_machine_name` and
    /// so was neither recognised as expected nor swept as stale. The marker
    /// gate now makes that moot — the file simply carries no
    /// `automatic-managed: true` marker, so it survives on that basis alone,
    /// regardless of whether its name happens to collide with Automatic's
    /// own convention.
    #[test]
    fn copilot_hand_written_agent_file_survives_the_stale_sweep() {
        let _lock = env_lock().lock().expect("env lock");
        let home = tmp();
        let _home_guard = HomeGuard::set(home.path());
        let project = tmp();
        let agents_dir = project.path().join("agents");
        fs::create_dir_all(&agents_dir).expect("create agents dir");

        let hand_written_path = agents_dir.join("mine.agent.md");
        fs::write(
            &hand_written_path,
            "---\nname: Mine\n---\n\nI wrote this myself.\n",
        )
        .expect("write hand-written copilot agent");

        sync_user_agents(project.path(), &agents_dir, &[], &[], &GitHubCopilot)
            .expect("sync user agents");

        assert!(
            hand_written_path.exists(),
            "hand-written .github/agents/mine.agent.md must survive the stale sweep"
        );
    }

    #[test]
    fn automatic_project_is_the_id_or_the_name_without_one() {
        let mut project = crate::core::Project {
            name: "site".to_string(),
            id: "5b1f0c7e-0000-4000-8000-0000000000f1".to_string(),
            local_key: "5b1f0c7e-0000-4000-8000-0000000000f2".to_string(),
            ..Default::default()
        };
        assert_eq!(automatic_project_env_value(&project), project.id);
        let servers = build_selected_servers(automatic_project_env_value(&project), &[], &Map::new(), StoreRead::Now)
            .expect("no servers to check");
        assert_eq!(
            servers["automatic"]["env"]["AUTOMATIC_PROJECT"],
            project.id.as_str(),
            "the committed id, never the machine-local local_key"
        );

        project.id.clear();
        assert_eq!(automatic_project_env_value(&project), "site");
    }

    // ── load_mcp_server_configs ──────────────────────────────────────────────

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn load_mcp_server_configs_fails_for_a_selected_server_it_cannot_read() {
        let home = tmp();
        crate::core::with_test_home(home.path().to_path_buf(), || {
            crate::core::save_mcp_server_config("loader-good", r#"{"command":"npx"}"#)
                .expect("seed");
            crate::core::write_undecryptable_mcp_server_config("loader-locked");

            let err = load_mcp_server_configs(&names(&["loader-good", "loader-locked"]))
                .expect_err("a selected server that cannot be read must stop the load");
            assert!(
                err.contains("loader-locked"),
                "error names the server: {}",
                err
            );
        });
    }

    #[test]
    fn load_mcp_server_configs_ignores_an_unreadable_server_that_is_not_selected() {
        let home = tmp();
        crate::core::with_test_home(home.path().to_path_buf(), || {
            crate::core::save_mcp_server_config("loader-used", r#"{"command":"npx"}"#)
                .expect("seed");
            crate::core::write_undecryptable_mcp_server_config("loader-unused-locked");

            let loaded = load_mcp_server_configs(&names(&["loader-used"])).expect("load");
            assert_eq!(loaded.keys().collect::<Vec<_>>(), vec!["loader-used"]);
            assert_eq!(loaded["loader-used"]["command"], "npx");
        });
    }

    #[test]
    fn load_mcp_server_configs_skips_a_selected_name_with_no_registry_entry() {
        let home = tmp();
        crate::core::with_test_home(home.path().to_path_buf(), || {
            crate::core::save_mcp_server_config("loader-present", r#"{"command":"npx"}"#)
                .expect("seed");

            let loaded = load_mcp_server_configs(&names(&["loader-present", "loader-absent"]))
                .expect("load");
            assert_eq!(loaded.keys().collect::<Vec<_>>(), vec!["loader-present"]);
        });
    }

    #[test]
    fn load_mcp_server_configs_reads_nothing_for_an_empty_selection() {
        let home = tmp();
        crate::core::with_test_home(home.path().to_path_buf(), || {
            crate::core::write_undecryptable_mcp_server_config("loader-idle-locked");

            let loaded = load_mcp_server_configs(&[]).expect("load");
            assert!(loaded.is_empty());
        });
    }

    #[test]
    fn a_server_gets_a_proxy_stub_only_for_a_token_filed_under_its_url() {
        // The secret store is shared by the whole test process, so the
        // server name is one no other test uses.
        let server = "sync-stub-fixture";
        let names = vec![server.to_string()];
        let registry = |url: &str| {
            let mut registry = Map::new();
            registry.insert(server.to_string(), json!({ "type": "http", "url": url }));
            registry
        };
        crate::proxy::store_oauth_token(server, "https://mcp.example.com/mcp", "token")
            .expect("store token");

        let signed_in = build_selected_servers(
            "sync-stub-project",
            &names,
            &registry("https://mcp.example.com/mcp"),
            StoreRead::Now,
        )
        .expect("build");
        assert_eq!(signed_in[server]["args"], json!(["mcp-proxy", server]));

        let other_url = build_selected_servers(
            "sync-stub-project",
            &names,
            &registry("https://other.example/mcp"),
            StoreRead::Now,
        )
        .expect("build");
        assert_eq!(other_url[server]["url"], "https://other.example/mcp");
        assert!(other_url[server].get("command").is_none());
    }

    /// One HTTP server in a registry map, for the builders.
    fn http_registry(server: &str, url: &str) -> Map<String, Value> {
        let mut registry = Map::new();
        registry.insert(server.to_string(), json!({ "type": "http", "url": url }));
        registry
    }

    /// The store is asked about every HTTP server, signed in or not. A
    /// server that never had a token therefore also stops the build when the
    /// store cannot be read: nothing tells it apart from one whose token
    /// cannot be seen.
    #[test]
    fn a_server_that_never_signed_in_also_stops_the_build_when_the_store_cannot_be_read() {
        use crate::core::secret_store::{set_test_fault, TestFault};

        let server = "sync-stub-never-signed-in-fixture";
        let url = "https://mcp.example.com/mcp";
        let names = vec![server.to_string()];
        let registry = http_registry(server, url);
        let entry = crate::proxy::oauth_token_entry(server, url);

        set_test_fault(&entry, Some(TestFault::Unreadable));
        let built = build_selected_servers("sync-stub-project", &names, &registry, StoreRead::Now);
        set_test_fault(&entry, None);

        let err = built.expect_err("unreadable");
        assert!(err.contains(server), "the error names the server: {err}");
    }

    #[test]
    fn a_token_that_cannot_be_read_stops_the_build_and_is_not_remembered() {
        use crate::core::secret_store::{set_test_fault, TestFault};

        let server = "sync-stub-unreadable-fixture";
        let url = "https://mcp.example.com/mcp";
        let names = vec![server.to_string()];
        let registry = http_registry(server, url);
        // Stored straight into the store, so the presence cache has no
        // answer and the build has to read.
        let entry = crate::proxy::oauth_token_entry(server, url);
        crate::core::secret_store::store_secret(&entry, "token").expect("store token");

        set_test_fault(&entry, Some(TestFault::Unreadable));
        let project = build_selected_servers("sync-stub-project", &names, &registry, StoreRead::Now);
        let global = build_global_selected_servers(&names, &registry);
        set_test_fault(&entry, None);

        // No map comes back, so nothing can write the server's URL over a
        // proxy stub.
        for err in [project.expect_err("unreadable"), global.expect_err("unreadable")] {
            assert!(err.contains(server), "the error names the server: {err}");
        }

        // The failed read was not cached as "no token".
        let project = build_selected_servers("sync-stub-project", &names, &registry, StoreRead::Now).expect("build");
        assert_eq!(project[server]["args"], json!(["mcp-proxy", server]));
        let global = build_global_selected_servers(&names, &registry).expect("build");
        assert_eq!(global[server]["args"], json!(["mcp-proxy", server]));
    }

    // ── Contained writes ───────────────────────────────────────────────

    #[cfg(unix)]
    fn link(target: &Path, at: &Path) {
        std::os::unix::fs::symlink(target, at).expect("symlink");
    }

    #[cfg(unix)]
    #[test]
    fn write_refusal_allows_project_paths_and_links_that_stay_inside() {
        let dir = tmp();
        let root = dir.path();
        fs::write(root.join("AGENTS.md"), "x").expect("write");
        link(Path::new("AGENTS.md"), &root.join("CLAUDE.md"));
        // Cline's legacy `.clinerules` file sits where a directory will go.
        fs::write(root.join(".clinerules"), "legacy").expect("write");

        assert_eq!(project_write_refusal(root, &root.join("CLAUDE.md")), None);
        assert_eq!(project_write_refusal(root, &root.join("new/deep/file.md")), None);
        assert_eq!(project_write_refusal(root, &root.join(".clinerules/automatic.md")), None);

        // A project registered through a `..` path is the user's choice.
        let name = root.file_name().expect("name");
        let roundabout = root.join("..").join(name);
        assert_eq!(project_write_refusal(&roundabout, &roundabout.join("CLAUDE.md")), None);
    }

    #[cfg(unix)]
    #[test]
    fn write_refusal_refuses_links_out_dangling_links_git_and_fifos() {
        let dir = tmp();
        let outside = tmp();
        let root = dir.path();
        link(outside.path(), &root.join(".claude"));
        link(&outside.path().join("missing"), &root.join("dangling.md"));
        fs::create_dir_all(root.join(".git")).expect("mkdir .git");
        link(&root.join(".git"), &root.join(".cursor"));
        let status = std::process::Command::new("mkfifo")
            .arg(root.join("fifo.md"))
            .status()
            .expect("run mkfifo");
        assert!(status.success());

        for refused in [
            ".claude",
            ".claude/agents/new.md",
            "dangling.md",
            ".git/config",
            ".cursor/mcp.json",
            "fifo.md",
            "new/../../escaped/x.md",
        ] {
            assert!(
                project_write_refusal(root, &root.join(refused)).is_some(),
                "{refused} must be refused"
            );
        }
        assert!(write_project_file(root, &root.join(".claude/x.md"), "x").is_err());
        assert!(write_project_file(root, &root.join("new/../../escaped/x.md"), "x").is_err());
        assert!(!root.parent().expect("parent").join("escaped").exists());
        assert!(entry_count(outside.path()) == 0, "nothing may reach outside");
    }

    fn entry_count(dir: &Path) -> usize {
        fs::read_dir(dir).expect("read dir").count()
    }

    /// Case (b): a custom agent's file committed as a link out of the
    /// project is skipped, and the other agents still sync.
    #[cfg(unix)]
    #[test]
    fn sync_custom_agents_skips_a_file_that_links_out_of_the_project() {
        let dir = tmp();
        let outside = tmp();
        let secret = outside.path().join("secret");
        fs::write(&secret, "OUTSIDE").expect("write secret");
        let agents_dir = dir.path().join(".claude").join("agents");
        fs::create_dir_all(&agents_dir).expect("mkdir");
        link(&secret, &agents_dir.join("planner.md"));
        let custom_agents = vec![
            CustomAgent {
                name: "Planner".into(),
                content: "---\nname: planner\n---\nPlan.\n".into(),
            },
            CustomAgent {
                name: "Reviewer".into(),
                content: "---\nname: reviewer\n---\nReview.\n".into(),
            },
        ];

        let written = sync_custom_agents(
            dir.path(),
            &agents_dir,
            &custom_agents,
            &ClaudeCode,
            &HashSet::new(),
        )
        .expect("sync");

        assert_eq!(fs::read_to_string(&secret).expect("read"), "OUTSIDE");
        assert_eq!(written.len(), 1, "{written:?}");
        assert!(agents_dir.join("reviewer.md").is_file());
    }

    /// Case (b): the conflict scan reads the custom agent's file under every
    /// agent directory, not only the first that has one.  A matching file
    /// under `.claude/agents/` no longer hides a link under `.codex/agents/`.
    #[cfg(unix)]
    #[test]
    fn custom_agent_scan_checks_every_agent_directory() {
        let dir = tmp();
        let outside = tmp();
        let content = "---\nname: planner\ndescription: Plans\n---\nPlan.\n";
        let project = Project {
            name: "scan".into(),
            directory: dir.path().display().to_string(),
            agents: vec!["claude".into(), "codex".into()],
            custom_agents: Some(vec![CustomAgent {
                name: "Planner".into(),
                content: content.into(),
            }]),
            ..Default::default()
        };
        let claude_dir = dir.path().join(".claude").join("agents");
        fs::create_dir_all(&claude_dir).expect("mkdir");
        fs::write(
            claude_dir.join("planner.md"),
            crate::agent::Agent::convert_agent_content(&ClaudeCode, content, "planner"),
        )
        .expect("write matching file");
        let codex_dir = dir.path().join(".codex").join("agents");
        fs::create_dir_all(&codex_dir).expect("mkdir");
        let secret = outside.path().join("secret");
        fs::write(&secret, "OUTSIDE").expect("write secret");
        link(&secret, &codex_dir.join("planner.toml"));

        let scan = collect_custom_agent_conflicts(&project, dir.path());

        assert!(
            conflicting_names(&scan, CustomAssetKind::Agent).contains("Planner"),
            "the linked file under .codex/agents must hold the agent back"
        );
    }

    /// The scan reads Copilot's `{name}.agent.md`, so a hand edit there is
    /// reported as a conflict and sync leaves it alone.
    #[test]
    fn custom_agent_scan_reads_copilot_agent_md_file() {
        let dir = tmp();
        let content = "---\nname: planner\ndescription: Plans\n---\nPlan.\n";
        let project = Project {
            name: "scan".into(),
            directory: dir.path().display().to_string(),
            agents: vec!["copilot".into()],
            custom_agents: Some(vec![CustomAgent {
                name: "Planner".into(),
                content: content.into(),
            }]),
            ..Default::default()
        };
        let agents_dir = dir.path().join(".github").join("agents");
        fs::create_dir_all(&agents_dir).expect("mkdir");
        fs::write(agents_dir.join("planner.agent.md"), "hand edited\n").expect("write");

        let scan = collect_custom_agent_conflicts(&project, dir.path());

        assert_eq!(scan.conflicts.len(), 1, "{:?}", scan.conflicts);
        assert_eq!(scan.conflicts[0].path, ".github/agents/planner.agent.md");
        assert_eq!(scan.conflicts[0].disk_content, "hand edited\n");
    }
}
