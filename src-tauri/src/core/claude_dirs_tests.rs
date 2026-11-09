use super::*;
use serde_json::json;
use std::process::Command;

// ── Merge ─────────────────────────────────────────────────────────────────────

fn set(paths: &[&str]) -> BTreeSet<String> {
    paths.iter().map(|p| p.to_string()).collect()
}

fn root(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        _ => panic!("not an object"),
    }
}

fn owned(written: &[&str], declined: &[&str]) -> OwnedEntries {
    OwnedEntries {
        written: set(written),
        declined: set(declined),
    }
}

fn merge(
    file: Value,
    before: &OwnedEntries,
    shared: &[&str],
    desired: &[&str],
) -> DirsMerge {
    merge_directories(root(file), before, &set(shared), &set(desired), Path::new("settings.json"))
        .expect("merge")
}

#[test]
fn a_new_entry_is_added_and_recorded() {
    let out = merge(json!({}), &OwnedEntries::default(), &[], &["/a"]);

    assert!(out.changed);
    assert_eq!(Value::Object(out.root), json!({"permissions": {"additionalDirectories": ["/a"]}}));
    assert_eq!(out.entries, owned(&["/a"], &[]));
}

#[test]
fn every_other_key_and_the_block_setting_are_left_alone() {
    let file = json!({
        "model": "opus",
        "permissions": {
            "allow": ["Bash"],
            "blockWritesOutsideWorkingDirectories": true,
            "additionalDirectories": ["/theirs"]
        }
    });

    let added = merge(file, &OwnedEntries::default(), &[], &["/a"]);
    let removed = merge(Value::Object(added.root.clone()), &added.entries, &[], &[]);

    let expected_after_add = json!({
        "model": "opus",
        "permissions": {
            "allow": ["Bash"],
            "blockWritesOutsideWorkingDirectories": true,
            "additionalDirectories": ["/theirs", "/a"]
        }
    });
    assert_eq!(Value::Object(added.root), expected_after_add);
    assert_eq!(
        Value::Object(removed.root),
        json!({
            "model": "opus",
            "permissions": {
                "allow": ["Bash"],
                "blockWritesOutsideWorkingDirectories": true,
                "additionalDirectories": ["/theirs"]
            }
        })
    );
}

#[test]
fn an_entry_the_user_added_is_never_recorded_or_removed() {
    let file = json!({"permissions": {"additionalDirectories": ["/a"]}});

    let wanted = merge(file, &OwnedEntries::default(), &[], &["/a"]);
    assert!(!wanted.changed);
    assert!(wanted.entries.written.is_empty(), "{:?}", wanted.entries);

    let unwanted = merge(Value::Object(wanted.root), &wanted.entries, &[], &[]);
    assert!(!unwanted.changed);
    assert_eq!(
        Value::Object(unwanted.root),
        json!({"permissions": {"additionalDirectories": ["/a"]}})
    );
}

#[test]
fn removing_the_last_entry_removes_an_emptied_list_and_permissions() {
    let out = merge(
        json!({"permissions": {"additionalDirectories": ["/a"]}}),
        &owned(&["/a"], &[]),
        &[],
        &[],
    );

    assert!(out.changed);
    assert_eq!(Value::Object(out.root), json!({}));
    assert!(out.entries.is_empty());
}

#[test]
fn a_path_another_record_owns_stays() {
    let file = json!({"permissions": {"additionalDirectories": ["/a"]}});

    let out = merge(file, &owned(&["/a"], &[]), &["/a"], &[]);

    assert!(!out.changed);
    assert!(out.entries.written.is_empty());
}

#[test]
fn a_path_another_record_wrote_is_shared_not_taken_for_the_users() {
    let file = json!({"permissions": {"additionalDirectories": ["/a"]}});

    let out = merge(file, &OwnedEntries::default(), &["/a"], &["/a"]);

    assert_eq!(out.entries.written, set(&["/a"]));
}

#[test]
fn an_entry_the_user_deleted_is_not_added_back_until_it_stops_being_wanted() {
    let deleted = merge(json!({"permissions": {"additionalDirectories": ["/other"]}}), &owned(&["/a"], &[]), &[], &["/a"]);
    assert!(!deleted.changed, "the deleted entry came back");
    assert_eq!(deleted.entries, owned(&[], &["/a"]));

    let still = merge(json!({"permissions": {"additionalDirectories": ["/other"]}}), &deleted.entries, &[], &["/a"]);
    assert!(!still.changed);

    let dropped = merge(json!({"permissions": {"additionalDirectories": ["/other"]}}), &still.entries, &[], &[]);
    assert!(dropped.entries.is_empty());

    let wanted_again = merge(json!({"permissions": {"additionalDirectories": ["/other"]}}), &dropped.entries, &[], &["/a"]);
    assert!(wanted_again.changed);
    assert_eq!(wanted_again.entries.written, set(&["/a"]));
}

#[test]
fn entries_that_are_not_text_are_kept() {
    let out = merge(
        json!({"permissions": {"additionalDirectories": [7, "/a"]}}),
        &owned(&["/a"], &[]),
        &[],
        &[],
    );

    assert_eq!(
        Value::Object(out.root),
        json!({"permissions": {"additionalDirectories": [7]}})
    );
}

#[test]
fn a_file_with_the_wrong_shape_is_refused() {
    for file in [
        json!({"permissions": []}),
        json!({"permissions": {"additionalDirectories": "/a"}}),
    ] {
        let result = merge_directories(
            root(file.clone()),
            &OwnedEntries::default(),
            &BTreeSet::new(),
            &set(&["/a"]),
            Path::new("settings.json"),
        );
        assert!(result.is_err(), "{file} was accepted");
    }
}

// ── Target ────────────────────────────────────────────────────────────────────

fn facts() -> TargetFacts {
    TargetFacts {
        silent: false,
        read_only: false,
        local_file: Some(LocalFileState::Untracked),
        trust: TrustFlag::Clear,
        preference: WritableFoldersTarget::PreferLocal,
        user_consent: false,
    }
}

#[test]
fn a_normal_untracked_project_uses_its_local_file() {
    assert_eq!(choose_target(&facts()), DirsTarget::Local);
}

#[test]
fn each_reason_falls_back_to_user_settings_and_needs_consent() {
    use UserSettingsReason as R;
    let cases: Vec<(TargetFacts, R)> = vec![
        (TargetFacts { silent: true, ..facts() }, R::Silent),
        (TargetFacts { read_only: true, ..facts() }, R::ReadOnly),
        (
            TargetFacts { local_file: Some(LocalFileState::Tracked), ..facts() },
            R::Tracked,
        ),
        (
            TargetFacts { local_file: Some(LocalFileState::NotRepositoryRoot), ..facts() },
            R::NotRepositoryRoot,
        ),
        (
            TargetFacts { local_file: Some(LocalFileState::Unknown), ..facts() },
            R::GitUnavailable,
        ),
        (TargetFacts { trust: TrustFlag::SeenTracked, ..facts() }, R::SeenTracked),
        (
            TargetFacts { preference: WritableFoldersTarget::AlwaysUser, ..facts() },
            R::Preferred,
        ),
    ];
    for (case, reason) in cases {
        assert_eq!(choose_target(&case), DirsTarget::NeedsConsent(reason));
        let agreed = TargetFacts { user_consent: true, ..case };
        assert_eq!(choose_target(&agreed), DirsTarget::User(reason));
    }
}

#[test]
fn a_silent_or_read_only_project_never_gets_the_local_file() {
    for case in [
        TargetFacts { silent: true, user_consent: true, ..facts() },
        TargetFacts { read_only: true, user_consent: true, ..facts() },
    ] {
        assert_ne!(choose_target(&case), DirsTarget::Local);
    }
}

// ── Folders ───────────────────────────────────────────────────────────────────

fn candidate(path: &str, resolved: Option<&str>, source: FolderSource) -> Candidate {
    Candidate {
        source,
        path: path.to_string(),
        resolved: resolved.map(PathBuf::from),
        expected: None,
    }
}

#[test]
fn the_own_folder_and_folders_inside_it_are_left_out_and_missing_ones_reported() {
    let candidates = vec![
        candidate("/own", Some("/own"), FolderSource::Extra),
        candidate("/own/sub", Some("/own/sub"), FolderSource::Extra),
        candidate("/b", Some("/real/b"), FolderSource::Extra),
        candidate("/gone", None, FolderSource::Extra),
        candidate("/real/b", Some("/real/b"), FolderSource::MainCheckout),
    ];

    let resolved = resolve_folders(Path::new("/own"), &candidates, &denied());

    assert_eq!(
        resolved.wanted,
        vec![WantedFolder {
            path: "/real/b".into(),
            source: FolderSource::Extra
        }]
    );
    assert_eq!(resolved.missing.len(), 1);
    assert_eq!(resolved.missing[0].path, "/gone");
}

// ── Integration ───────────────────────────────────────────────────────────────

fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(["-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A folder `name` under `base`, holding a git repository with one commit.
fn repo(base: &Path, name: &str) -> PathBuf {
    let dir = base.join(name);
    fs::create_dir_all(&dir).expect("mkdir");
    git(&dir, &["init", "-q"]);
    fs::write(dir.join("README.md"), "readme\n").expect("write");
    git(&dir, &["add", "README.md"]);
    git(&dir, &["commit", "-q", "-m", "init"]);
    dir
}

/// Register a Claude Code project at `dir` and read it back.
fn register(dir: &Path, name: &str, change: impl FnOnce(&mut Project)) -> Project {
    let mut project = Project {
        name: name.to_string(),
        directory: dir.to_str().expect("utf-8").to_string(),
        agents: vec![CLAUDE_AGENT.to_string()],
        ..Default::default()
    };
    change(&mut project);
    prepare_new_project_keys(&mut project).expect("keys");
    save_project(&project.local_key, &serde_json::to_string(&project).expect("json"))
        .expect("save");
    reread(&project)
}

fn reread(project: &Project) -> Project {
    serde_json::from_str(&read_project(&project.local_key).expect("read")).expect("parse")
}

/// Save a group of `members` with the switch set, and join each member to it.
fn group(name: &str, members: &[&Project], switch: bool) {
    let keys: Vec<&str> = members.iter().map(|p| p.local_key.as_str()).collect();
    let data = json!({ "name": name, "projects": keys, "agents_edit_across_group": switch });
    save_group(name, &data.to_string()).expect("save group");
    for member in members {
        confirm_project_group(&member.local_key, name).expect("join");
    }
}

fn sync(project: &Project) -> WritableFoldersSync {
    let outcome = sync_writable_folders(&reread(project)).expect("sync");
    assert!(outcome.errors.is_empty(), "{:?}", outcome.errors);
    outcome
}

fn consent() {
    set_writable_folders_user_consent(true).expect("consent");
}

fn directories_in(path: &Path) -> Vec<String> {
    let Ok(raw) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let value: Value = serde_json::from_str(&raw).expect("json");
    value["permissions"]["additionalDirectories"]
        .as_array()
        .map(|list| list.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default()
}

fn local_file(dir: &Path) -> PathBuf {
    local_settings_path(dir)
}

fn user_file() -> PathBuf {
    user_settings_path().expect("user settings")
}

fn canonical(dir: &Path) -> String {
    fs::canonicalize(dir).expect("canonical").to_str().expect("utf-8").to_string()
}

struct Workspace {
    _tmp: tempfile::TempDir,
    base: PathBuf,
}

fn workspace() -> Workspace {
    let tmp = tempfile::tempdir().expect("tempdir");
    let base = fs::canonicalize(tmp.path()).expect("canonical");
    Workspace { _tmp: tmp, base }
}

#[test]
fn a_group_member_goes_into_the_local_file_of_a_normal_untracked_project() {
    let _home = temp_test_home();
    let ws = workspace();
    let a = register(&repo(&ws.base, "a"), "a", |_| {});
    let b = register(&repo(&ws.base, "b"), "b", |_| {});
    group("team", &[&a, &b], true);

    let outcome = sync(&a);

    assert_eq!(outcome.target, Some(DirsTarget::Local));
    assert_eq!(directories_in(&local_file(Path::new(&a.directory))), vec![canonical(Path::new(&b.directory))]);
    assert!(!user_file().exists(), "user settings were written");
}

#[test]
fn the_switch_off_wants_nothing() {
    let _home = temp_test_home();
    let ws = workspace();
    let a = register(&repo(&ws.base, "a"), "a", |_| {});
    let b = register(&repo(&ws.base, "b"), "b", |_| {});
    group("team", &[&a, &b], false);

    let outcome = sync(&a);

    assert!(outcome.wanted.is_empty());
    assert!(!local_file(Path::new(&a.directory)).exists());
}

#[test]
fn a_silent_project_without_consent_writes_nothing_and_asks() {
    let _home = temp_test_home();
    let ws = workspace();
    let a = register(&repo(&ws.base, "a"), "a", |p| p.mode = ProjectMode::Silent);
    let b = register(&repo(&ws.base, "b"), "b", |_| {});
    group("team", &[&a, &b], true);

    let outcome = sync(&a);

    assert_eq!(
        outcome.target,
        Some(DirsTarget::NeedsConsent(UserSettingsReason::Silent))
    );
    assert!(!user_file().exists());
    assert!(!local_file(Path::new(&a.directory)).exists());
    let findings = writable_folders_findings(&reread(&a)).expect("check").expect("findings");
    assert!(findings.ask_for_user_settings);
    assert_eq!(findings.uncovered, vec![canonical(Path::new(&b.directory))]);

    consent();
    sync(&a);

    assert_eq!(directories_in(&user_file()), vec![canonical(Path::new(&b.directory))]);
    assert!(!local_file(Path::new(&a.directory)).exists(), "silent folder was written");
}

#[test]
fn a_read_only_project_goes_to_user_settings_and_its_folder_is_untouched() {
    let _home = temp_test_home();
    let ws = workspace();
    let a = register(&repo(&ws.base, "a"), "a", |_| {});
    let b = register(&repo(&ws.base, "b"), "b", |_| {});
    group("team", &[&a, &b], true);
    set_project_read_only(&a.local_key, true).expect("read only");
    consent();

    let outcome = sync(&a);

    assert_eq!(outcome.target, Some(DirsTarget::User(UserSettingsReason::ReadOnly)));
    assert_eq!(directories_in(&user_file()), vec![canonical(Path::new(&b.directory))]);
    assert!(!local_file(Path::new(&a.directory)).exists());
    // The read-only member is still a folder its sibling may edit.
    sync(&b);
    assert_eq!(directories_in(&local_file(Path::new(&b.directory))), vec![canonical(Path::new(&a.directory))]);
}

#[test]
fn a_local_file_git_tracks_moves_the_entries_to_user_settings_and_is_left_alone() {
    let _home = temp_test_home();
    let ws = workspace();
    let a_dir = repo(&ws.base, "a");
    let a = register(&a_dir, "a", |_| {});
    let b = register(&repo(&ws.base, "b"), "b", |_| {});
    group("team", &[&a, &b], true);
    sync(&a);
    let mut local: Value =
        serde_json::from_str(&fs::read_to_string(local_file(&a_dir)).expect("read")).expect("json");
    local["model"] = json!("opus");
    fs::write(local_file(&a_dir), local.to_string()).expect("add a key");
    git(&a_dir, &["add", "-f", ".claude/settings.local.json"]);
    git(&a_dir, &["commit", "-q", "-m", "track"]);
    consent();

    let outcome = sync(&a);

    assert_eq!(outcome.target, Some(DirsTarget::User(UserSettingsReason::Tracked)));
    assert_eq!(directories_in(&user_file()), vec![canonical(Path::new(&b.directory))]);
    // A tracked file belongs to the repository: Automatic leaves it, and says
    // which of its entries are still there.
    let tracked = local.to_string();
    assert_eq!(fs::read_to_string(local_file(&a_dir)).unwrap(), tracked);
    let findings = writable_folders_findings(&reread(&a)).expect("check").expect("findings");
    assert_eq!(findings.left_in_tracked_local, vec![canonical(Path::new(&b.directory))]);
}

fn write_claude_state(entries: Value) {
    let path = claude_state_path().expect("path");
    fs::write(path, json!({ "projects": entries }).to_string()).expect("write");
}

#[test]
fn the_sticky_flag_needs_user_settings_until_the_folder_is_trusted() {
    let _home = temp_test_home();
    let ws = workspace();
    let a_dir = repo(&ws.base, "a");
    let a = register(&a_dir, "a", |_| {});
    let b = register(&repo(&ws.base, "b"), "b", |_| {});
    group("team", &[&a, &b], true);
    consent();
    let key = canonical(&a_dir);
    write_claude_state(json!({ key.clone(): { "localSettingsSeenGitTracked": true } }));

    let flagged = sync(&a);
    assert_eq!(flagged.target, Some(DirsTarget::User(UserSettingsReason::SeenTracked)));
    assert_eq!(directories_in(&user_file()), vec![canonical(Path::new(&b.directory))]);

    write_claude_state(json!({ key: {
        "localSettingsSeenGitTracked": true,
        "hasTrustDialogAccepted": true
    } }));
    let trusted = sync(&a);

    assert_eq!(trusted.target, Some(DirsTarget::Local));
    assert!(directories_in(&user_file()).is_empty(), "the user entry was not moved");
    assert_eq!(directories_in(&local_file(&a_dir)), vec![canonical(Path::new(&b.directory))]);
}

#[test]
fn entries_the_user_added_to_user_settings_are_never_removed() {
    let _home = temp_test_home();
    let ws = workspace();
    let a = register(&repo(&ws.base, "a"), "a", |p| p.mode = ProjectMode::Silent);
    let b = register(&repo(&ws.base, "b"), "b", |_| {});
    let b_path = canonical(Path::new(&b.directory));
    fs::create_dir_all(user_file().parent().unwrap()).unwrap();
    fs::write(
        user_file(),
        json!({"permissions": {"additionalDirectories": ["/their/own", b_path.clone()]}}).to_string(),
    )
    .unwrap();
    group("team", &[&a, &b], true);
    consent();
    sync(&a);

    group("team", &[&a, &b], false);
    sync(&a);

    assert_eq!(directories_in(&user_file()), vec!["/their/own".to_string(), b_path]);
}

#[test]
fn a_shared_path_stays_in_user_settings_while_another_project_wants_it() {
    let _home = temp_test_home();
    let ws = workspace();
    let shared = repo(&ws.base, "shared");
    let shared_path = canonical(&shared);
    let silent = |p: &mut Project| {
        p.mode = ProjectMode::Silent;
        p.local_writable_folders = vec![shared.to_str().unwrap().to_string()];
    };
    let a = register(&repo(&ws.base, "a"), "a", silent);
    let c = register(&repo(&ws.base, "c"), "c", silent);
    consent();
    sync(&a);
    sync(&c);
    assert_eq!(directories_in(&user_file()), vec![shared_path.clone()]);

    set_project_writable_folders(&a.local_key, &[]).expect("clear a");
    sync(&a);
    assert_eq!(directories_in(&user_file()), vec![shared_path], "c still wants it");

    set_project_writable_folders(&c.local_key, &[]).expect("clear c");
    sync(&c);
    assert!(directories_in(&user_file()).is_empty());
}

#[test]
fn leaving_a_group_removes_the_entries_from_every_member() {
    let _home = temp_test_home();
    let ws = workspace();
    let a = register(&repo(&ws.base, "a"), "a", |_| {});
    let b = register(&repo(&ws.base, "b"), "b", |_| {});
    let c = register(&repo(&ws.base, "c"), "c", |_| {});
    crate::commands::save_group(
        "team",
        &json!({"name": "team", "projects": [], "agents_edit_across_group": true}).to_string(),
        None,
    )
    .expect("create");
    let all = json!({
        "name": "team",
        "projects": [a.local_key, b.local_key, c.local_key],
        "agents_edit_across_group": true
    });
    let joining = vec![a.local_key.clone(), b.local_key.clone(), c.local_key.clone()];
    crate::commands::save_group("team", &all.to_string(), Some(joining)).expect("join all");
    let path = |p: &Project| canonical(Path::new(&p.directory));
    let mut expected_a = vec![path(&b), path(&c)];
    expected_a.sort();
    let mut got_a = directories_in(&local_file(Path::new(&a.directory)));
    got_a.sort();
    assert_eq!(got_a, expected_a, "the save did not fan out");

    // Data from an editor that does not know the switch keeps it on.
    let without_c = json!({ "name": "team", "projects": [a.local_key, b.local_key] });
    crate::commands::save_group("team", &without_c.to_string(), None).expect("c leaves");

    assert_eq!(directories_in(&local_file(Path::new(&a.directory))), vec![path(&b)]);
    assert_eq!(directories_in(&local_file(Path::new(&b.directory))), vec![path(&a)]);
    assert!(directories_in(&local_file(Path::new(&c.directory))).is_empty());
}

#[test]
fn a_member_that_did_not_join_on_this_machine_is_not_a_target() {
    let _home = temp_test_home();
    let ws = workspace();
    let a = register(&repo(&ws.base, "a"), "a", |_| {});
    let b = register(&repo(&ws.base, "b"), "b", |_| {});
    let data = json!({ "name": "team", "projects": [a.local_key, b.local_key], "agents_edit_across_group": true });
    save_group("team", &data.to_string()).expect("save");
    confirm_project_group(&a.local_key, "team").expect("a joins");

    assert!(sync(&a).wanted.is_empty());
}

#[test]
fn a_missing_member_is_a_note_not_a_problem() {
    let _home = temp_test_home();
    let ws = workspace();
    let a = register(&repo(&ws.base, "a"), "a", |_| {});
    let b_dir = repo(&ws.base, "b");
    let b = register(&b_dir, "b", |_| {});
    group("team", &[&a, &b], true);
    fs::remove_dir_all(&b_dir).expect("remove b");

    let findings = writable_folders_findings(&reread(&a)).expect("check").expect("findings");

    assert!(findings.wanted.is_empty());
    assert_eq!(findings.missing.len(), 1);
    let report = crate::sync::check_project_problems(&reread(&a)).expect("problems");
    assert!(report.notes.iter().any(|n| n.contains("is not on this machine")), "{:?}", report.notes);
    assert!(report.problems.is_empty(), "{:?}", report.problems);
}

#[test]
fn a_worktree_also_gets_its_main_checkout() {
    let _home = temp_test_home();
    let ws = workspace();
    let main = repo(&ws.base, "main");
    git(&main, &["worktree", "add", "-q", "-b", "feature", "../wt"]);
    let project = register(&main, "main", |p| p.manage_gitignore = true);

    crate::sync::sync_project(&project).expect("sync");

    let worktree = ws.base.join("wt");
    assert_eq!(directories_in(&local_file(&worktree)), vec![canonical(&main)]);
    assert!(!local_file(&main).exists(), "the main checkout wants nothing");
}

#[test]
fn a_local_file_a_worktree_branch_tracks_is_left_as_it_is() {
    let _home = temp_test_home();
    let ws = workspace();
    let main = repo(&ws.base, "main");
    git(&main, &["worktree", "add", "-q", "-b", "feature", "../wt"]);
    let project = register(&main, "main", |_| {});
    crate::sync::sync_project(&project).expect("first sync");
    let worktree = ws.base.join("wt");
    git(&worktree, &["add", "-f", ".claude/settings.local.json"]);
    git(&worktree, &["commit", "-q", "-m", "track"]);
    let tracked = fs::read_to_string(local_file(&worktree)).expect("read");

    crate::sync::sync_project(&reread(&project)).expect("second sync");

    assert_eq!(fs::read_to_string(local_file(&worktree)).expect("read"), tracked);
}

#[test]
fn the_block_without_a_counting_entry_is_a_problem_with_the_json_to_paste() {
    let _home = temp_test_home();
    let ws = workspace();
    let a_dir = repo(&ws.base, "a");
    let a = register(&a_dir, "a", |p| p.mode = ProjectMode::Silent);
    let b = register(&repo(&ws.base, "b"), "b", |_| {});
    group("team", &[&a, &b], true);
    fs::create_dir_all(a_dir.join(".claude")).unwrap();
    fs::write(
        a_dir.join(".claude/settings.json"),
        json!({"permissions": {"blockWritesOutsideWorkingDirectories": true}}).to_string(),
    )
    .unwrap();

    let asking = crate::sync::check_project_problems(&reread(&a)).expect("problems");
    assert!(asking.writable_folders_target.is_some());
    assert!(asking.problems.is_empty(), "the question comes first: {:?}", asking.problems);

    set_writable_folders_notice_hidden(&a.local_key, true).expect("not now");
    let report = crate::sync::check_project_problems(&reread(&a)).expect("problems");

    assert!(report.writable_folders_target.is_none());
    let problem = report
        .problems
        .iter()
        .find(|p| p.kind == crate::sync::ProjectProblemKind::WritableFoldersBlocked)
        .expect("problem");
    let b_path = canonical(Path::new(&b.directory));
    assert!(problem.solution[0].contains(&paste_json(&[b_path])), "{:?}", problem.solution);
}

#[test]
fn the_drift_check_leaves_the_local_file_alone() {
    let _home = temp_test_home();
    let ws = workspace();
    let a = register(&repo(&ws.base, "a"), "a", |_| {});
    let b = register(&repo(&ws.base, "b"), "b", |_| {});
    group("team", &[&a, &b], true);
    crate::sync::sync_project(&reread(&a)).expect("sync");
    assert!(local_file(Path::new(&a.directory)).is_file());

    let drift = crate::sync::check_project_drift(&reread(&a)).expect("drift");

    let shown = serde_json::to_string(&drift).expect("json");
    assert!(!shown.contains("settings.local.json"), "{shown}");
}

#[test]
fn deleting_a_project_removes_its_user_settings_entries() {
    let _home = temp_test_home();
    let ws = workspace();
    let a = register(&repo(&ws.base, "a"), "a", |p| p.mode = ProjectMode::Silent);
    let b = register(&repo(&ws.base, "b"), "b", |_| {});
    group("team", &[&a, &b], true);
    consent();
    sync(&a);
    assert!(!directories_in(&user_file()).is_empty());

    crate::commands::delete_project(&a.local_key).expect("delete");

    assert!(directories_in(&user_file()).is_empty());
}

#[test]
fn withdrawing_consent_removes_the_user_settings_entries() {
    let _home = temp_test_home();
    let ws = workspace();
    let a = register(&repo(&ws.base, "a"), "a", |p| p.mode = ProjectMode::Silent);
    let b = register(&repo(&ws.base, "b"), "b", |_| {});
    group("team", &[&a, &b], true);
    consent();
    sync(&a);

    set_writable_folders_user_consent(false).expect("withdraw");
    sync(&a);

    assert!(directories_in(&user_file()).is_empty());
}

#[test]
fn a_settings_page_save_keeps_the_consent_and_target() {
    let _home = temp_test_home();
    set_writable_folders_target(WritableFoldersTarget::AlwaysUser).expect("target");

    crate::commands::write_settings(Settings::default()).expect("page save");

    let stored = read_settings().expect("read");
    assert!(stored.writable_folders_user_consent);
    assert_eq!(stored.writable_folders_target, WritableFoldersTarget::AlwaysUser);
}

// ── Fix round 1 ───────────────────────────────────────────────────────────────

fn denied() -> DeniedPaths {
    DeniedPaths {
        home: PathBuf::from("/home/me"),
        sensitive: vec![
            PathBuf::from("/home/me/.claude"),
            PathBuf::from("/home/me/.claude.json"),
            PathBuf::from("/home/me/.automatic"),
        ],
        sensitive_folders: vec![
            PathBuf::from("/home/me/.claude"),
            PathBuf::from("/home/me/.automatic"),
        ],
    }
}

#[test]
fn a_missing_list_is_a_fresh_start_not_a_deletion() {
    for file in [json!({}), json!({"permissions": {}}), json!({"permissions": {"allow": []}})] {
        let out = merge(file.clone(), &owned(&["/a"], &["/b"]), &[], &["/a", "/b"]);
        assert_eq!(out.entries, owned(&["/a", "/b"], &[]), "{file}");
    }
}

#[test]
fn a_deleted_local_file_gets_its_entries_back() {
    let _home = temp_test_home();
    let ws = workspace();
    let a = register(&repo(&ws.base, "a"), "a", |_| {});
    let b = register(&repo(&ws.base, "b"), "b", |_| {});
    group("team", &[&a, &b], true);
    sync(&a);
    fs::remove_file(local_file(Path::new(&a.directory))).expect("git clean");

    sync(&a);

    assert_eq!(directories_in(&local_file(Path::new(&a.directory))), vec![canonical(Path::new(&b.directory))]);
}

#[test]
fn broad_and_sensitive_folders_are_refused_whatever_their_source() {
    let refused = [
        "/",
        "/home",
        "/home/me",
        "/home/me/.claude",
        "/home/me/.claude/projects",
        "/home/me/.claude/hooks",
        "/home/me/.automatic",
        "/home/me/.automatic/projects",
    ];
    for folder in refused {
        assert!(refusal_reason(Path::new(folder), &denied()).is_some(), "{folder}");
    }
    for allowed in ["/home/me/code/b", "/home/me/.claude.json.d", "/home/me/.claudex", "/srv/b"] {
        assert!(refusal_reason(Path::new(allowed), &denied()).is_none(), "{allowed}");
    }
    let sources = [
        FolderSource::Extra,
        FolderSource::MainCheckout,
        FolderSource::GroupMember { group: "g".into(), project: "p".into() },
    ];
    let candidates: Vec<Candidate> = sources
        .iter()
        .map(|source| candidate("/home/me", Some("/home/me"), source.clone()))
        .collect();

    let resolved = resolve_folders(Path::new("/srv/own"), &candidates, &denied());

    assert!(resolved.wanted.is_empty());
    assert_eq!(resolved.refused.len(), 3);
}

#[test]
fn setting_the_home_folder_or_automatics_folder_is_refused() {
    let _home = temp_test_home();
    let ws = workspace();
    let a = register(&repo(&ws.base, "a"), "a", |_| {});
    let home = super::super::paths::home_dir().unwrap();
    let registry = get_automatic_dir().unwrap().join("projects");
    let claude_hooks = home.join(".claude").join("hooks");
    for folder in [&registry, &claude_hooks] {
        fs::create_dir_all(folder).unwrap();
    }
    for folder in [
        home.clone(),
        get_automatic_dir().unwrap(),
        registry,
        claude_hooks,
        PathBuf::from("/"),
    ] {
        let err = set_project_writable_folders(&a.local_key, &[folder.display().to_string()])
            .expect_err("refused");
        assert!(err.contains("cannot be allowed"), "{err}");
    }
    assert!(reread(&a).local_writable_folders.is_empty());
}

#[cfg(unix)]
#[test]
fn an_extra_folder_whose_link_was_swapped_is_not_written() {
    let _home = temp_test_home();
    let ws = workspace();
    let a = register(&repo(&ws.base, "a"), "a", |_| {});
    let real = repo(&ws.base, "real");
    let other = repo(&ws.base, "other");
    let link = ws.base.join("link");
    std::os::unix::fs::symlink(&real, &link).unwrap();
    set_project_writable_folders(&a.local_key, &[link.display().to_string()]).expect("set");
    assert_eq!(reread(&a).local_writable_folders, vec![canonical(&real)], "stored canonical");
    sync(&a);
    fs::remove_file(&link).unwrap();
    // The stored path itself is swapped for a link to somewhere else.
    fs::rename(&real, ws.base.join("real-moved")).unwrap();
    std::os::unix::fs::symlink(&other, &real).unwrap();

    sync(&a);

    assert!(directories_in(&local_file(Path::new(&a.directory))).is_empty());
    let findings = writable_folders_findings(&reread(&a)).expect("check").expect("findings");
    assert_eq!(findings.refused.len(), 1);
    assert!(findings.refused[0].reason.contains("now leads to"), "{:?}", findings.refused);
}

#[test]
fn deleting_a_member_removes_it_from_the_others_and_its_own_local_file() {
    let _home = temp_test_home();
    let ws = workspace();
    let a = register(&repo(&ws.base, "a"), "a", |_| {});
    let b = register(&repo(&ws.base, "b"), "b", |_| {});
    group("team", &[&a, &b], true);
    sync(&a);
    sync(&b);

    crate::commands::delete_project(&b.local_key).expect("delete");

    assert!(directories_in(&local_file(Path::new(&a.directory))).is_empty(), "a still lists b");
    assert!(directories_in(&local_file(Path::new(&b.directory))).is_empty(), "b's file kept a");
}

#[test]
fn joining_a_group_refreshes_every_member() {
    let _home = temp_test_home();
    let ws = workspace();
    let a = register(&repo(&ws.base, "a"), "a", |_| {});
    let b = register(&repo(&ws.base, "b"), "b", |_| {});
    let c = register(&repo(&ws.base, "c"), "c", |_| {});
    group("team", &[&a, &b], true);
    sync(&a);
    sync(&b);
    // The group lists c, through a teammate's save, before c joins here.
    let data = json!({
        "name": "team",
        "projects": [a.local_key, b.local_key, c.local_key],
        "agents_edit_across_group": true
    });
    save_group("team", &data.to_string()).expect("list c");

    crate::commands::join_group("team", &c.local_key).expect("join");

    let c_path = canonical(Path::new(&c.directory));
    assert!(directories_in(&local_file(Path::new(&a.directory))).contains(&c_path));
    assert!(directories_in(&local_file(Path::new(&b.directory))).contains(&c_path));
}

#[test]
fn a_record_another_process_changed_is_merged_not_overwritten() {
    let _home = temp_test_home();
    let (base, seen) = read_records_seen().expect("read");
    let mut ours = base.clone();
    set_record(&mut ours.user_settings, "mine", "/a", owned(&["/x"], &[]));
    let mut theirs = DirectoryRecords::default();
    set_record(&mut theirs.user_settings, "theirs", "/b", owned(&["/y"], &[]));
    fs::create_dir_all(get_automatic_dir().unwrap()).unwrap();
    fs::write(records_path().unwrap(), serde_json::to_string(&theirs).unwrap()).unwrap();

    commit_records("mine", &ours, base, seen).expect("commit");

    let stored = read_records().expect("read");
    assert_eq!(record_of(&stored.user_settings, "mine", "/a"), owned(&["/x"], &[]));
    assert_eq!(record_of(&stored.user_settings, "theirs", "/b"), owned(&["/y"], &[]));
}

#[test]
fn a_stale_settings_save_cannot_bring_back_withdrawn_consent() {
    let _home = temp_test_home();
    consent();
    let stale = read_settings().expect("read");
    assert!(stale.writable_folders_user_consent);

    set_writable_folders_user_consent(false).expect("withdraw");
    write_settings(&stale).expect("stale save");

    assert!(!read_settings().unwrap().writable_folders_user_consent);
}

#[test]
fn a_moved_project_drops_the_old_folders_user_entries() {
    let _home = temp_test_home();
    let ws = workspace();
    let old_dir = repo(&ws.base, "a");
    let a = register(&old_dir, "a", |p| p.mode = ProjectMode::Silent);
    let b = register(&repo(&ws.base, "b"), "b", |_| {});
    group("team", &[&a, &b], true);
    consent();
    sync(&a);
    let b_path = canonical(Path::new(&b.directory));
    assert_eq!(directories_in(&user_file()), vec![b_path.clone()]);
    // The folder moves, and a new folder appears at the old path.
    let new_dir = ws.base.join("a-moved");
    fs::rename(&old_dir, &new_dir).unwrap();
    fs::create_dir_all(&old_dir).unwrap();
    let mut moved = reread(&a);
    moved.directory = new_dir.display().to_string();
    save_project(&moved.local_key, &serde_json::to_string(&moved).unwrap()).unwrap();
    group("team", &[&b], true);

    sync(&moved);

    assert!(directories_in(&user_file()).is_empty(), "the old folder's entry is still there");
    let records = read_records().unwrap();
    assert!(records
        .user_settings
        .get(&moved.local_key)
        .is_none_or(|folders| !folders.contains_key(&canonical(&old_dir))));
}

#[test]
fn an_unparsable_claude_state_counts_as_flagged() {
    let _home = temp_test_home();
    let ws = workspace();
    let a = register(&repo(&ws.base, "a"), "a", |_| {});
    let b = register(&repo(&ws.base, "b"), "b", |_| {});
    group("team", &[&a, &b], true);
    fs::write(claude_state_path().unwrap(), "{ not json").unwrap();

    let outcome = sync(&a);

    assert_eq!(
        outcome.target,
        Some(DirsTarget::NeedsConsent(UserSettingsReason::TrustUnknown))
    );
    assert!(!local_file(Path::new(&a.directory)).exists());
    let findings = writable_folders_findings(&reread(&a)).expect("check").expect("findings");
    assert!(findings.unreadable.iter().any(|u| u.contains(".claude.json")), "{:?}", findings.unreadable);
}

#[test]
fn a_project_awaiting_confirmation_is_left_alone() {
    let _home = temp_test_home();
    let ws = workspace();
    let a = register(&repo(&ws.base, "a"), "a", |_| {});
    let b = register(&repo(&ws.base, "b"), "b", |_| {});
    group("team", &[&a, &b], true);
    let mut awaiting = reread(&a);
    awaiting.local_awaiting_confirmation = true;

    let outcome = sync_writable_folders(&awaiting).expect("sync");

    assert_eq!(outcome, WritableFoldersSync::default());
    assert!(!local_file(Path::new(&a.directory)).exists());
    let report = crate::sync::check_project_problems(&awaiting).expect("problems");
    assert!(report.notes.iter().any(|n| n.contains("until you keep it")), "{:?}", report.notes);
}
