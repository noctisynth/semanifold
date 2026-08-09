#![allow(clippy::unwrap_used)]

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

fn temporary_repository(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "semifold-cli-e2e-{name}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    let git_init = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(git_init.success());
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"app\"\nversion = \"1.0.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    root
}

fn temporary_project(name: &str, config: &str) -> PathBuf {
    let root = temporary_repository(name);
    fs::create_dir_all(root.join(".changes")).unwrap();
    fs::write(root.join(".changes/config.toml"), config).unwrap();
    fs::write(
        root.join(".changes/feature.md"),
        "app: patch:chore\n---\n\nExercise the CLI.\n",
    )
    .unwrap();
    root
}

fn run_smif(root: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_smif"))
        .args(arguments)
        .current_dir(root)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env_remove("GITHUB_ACTIONS")
        .env_remove("GITHUB_EVENT_PATH")
        .env_remove("GITHUB_REPOSITORY")
        .env_remove("GITHUB_SERVER_URL")
        .env_remove("GITHUB_TOKEN")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn config(package: &str) -> String {
    format!(
        "[branches]\nbase = \"main\"\nrelease = \"release\"\n\n[tags]\nchore = \"Chores\"\n\n[packages.app]\n# keep this comment\npath = \".\"\nresolver = \"rust\"\n{package}\n\n[resolver.rust.pre-check]\ntype = \"http\"\nurl = \"\"\n"
    )
}

#[test]
fn init_accepts_complete_arguments_with_stdin_closed() {
    let root = temporary_repository("init-arguments");

    let init = run_smif(
        &root,
        &[
            "init",
            "--resolvers",
            "rust",
            "--default-tags",
            "--base-branch",
            "main",
            "--release-branch",
            "release",
            "--no-write-ci",
        ],
    );

    assert!(init.status.success(), "{init:?}");
    let config = fs::read_to_string(root.join(".changes/config.toml")).unwrap();
    assert!(config.contains("base = \"main\""), "{config}");
    assert!(config.contains("release = \"release\""), "{config}");
    assert!(config.contains("[[resolver.rust.publish]]"), "{config}");
    assert!(config.contains("[tags]"), "{config}");
    assert!(!root.join(".github/workflows/semifold-ci.yaml").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn init_reports_the_missing_parameter_instead_of_prompting_without_stdin() {
    let root = temporary_repository("init-missing-arguments");

    let init = run_smif(&root, &["init"]);

    assert!(!init.status.success(), "{init:?}");
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&init.stdout),
        String::from_utf8_lossy(&init.stderr)
    );
    assert!(output.contains("--resolvers or --no-resolvers"), "{output}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn commit_accepts_complete_arguments_with_stdin_closed() {
    let root = temporary_project("commit-arguments", &config("channel = \"stable\""));

    let commit = run_smif(
        &root,
        &[
            "commit",
            "--name",
            "automated-change",
            "--package",
            "app=minor",
            "--tag",
            "chore",
            "--summary",
            "Exercise the parameter-only path.",
        ],
    );

    assert!(commit.status.success(), "{commit:?}");
    let changeset = fs::read_to_string(root.join(".changes/automated-change.md")).unwrap();
    assert!(changeset.contains("app: \"minor:chore\""), "{changeset}");
    assert!(
        changeset.contains("Exercise the parameter-only path."),
        "{changeset}"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn commit_level_applies_to_packages_without_an_inline_level() {
    let root = temporary_project("commit-default-level", &config("channel = \"stable\""));

    let commit = run_smif(
        &root,
        &[
            "commit",
            "--name",
            "default-level",
            "--package",
            "app",
            "--level",
            "major",
            "--no-tag",
            "--summary",
            "Use the default package level.",
        ],
    );

    assert!(commit.status.success(), "{commit:?}");
    let changeset = fs::read_to_string(root.join(".changes/default-level.md")).unwrap();
    assert!(changeset.contains("app: major"), "{changeset}");
    assert!(!changeset.contains("major:"), "{changeset}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn commit_reports_the_missing_parameter_instead_of_prompting_without_stdin() {
    let root = temporary_project("commit-missing-arguments", &config("channel = \"stable\""));

    let commit = run_smif(
        &root,
        &[
            "commit",
            "--name",
            "missing-tag",
            "--package",
            "app=patch",
            "--summary",
            "Missing an explicit tag choice.",
        ],
    );

    assert!(!commit.status.success(), "{commit:?}");
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&commit.stdout),
        String::from_utf8_lossy(&commit.stderr)
    );
    assert!(output.contains("--tag or --no-tag"), "{output}");
    assert!(!root.join(".changes/missing-tag.md").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn status_reports_the_complete_dependency_cycle() {
    let root = temporary_project(
        "dependency-cycle",
        "[branches]\nbase = \"main\"\nrelease = \"release\"\n\n[tags]\n\n[packages.a]\npath = \"a\"\nresolver = \"rust\"\n\n[packages.b]\npath = \"b\"\nresolver = \"rust\"\n\n[resolver.rust.pre-check]\ntype = \"http\"\nurl = \"\"\n",
    );
    fs::remove_file(root.join("Cargo.toml")).unwrap();
    fs::remove_file(root.join(".changes/feature.md")).unwrap();
    for (path, manifest) in [
        (
            "a",
            "[package]\nname = \"a\"\nversion = \"1.0.0\"\n\n[dependencies]\nb = { version = \"1\", path = \"../b\" }\n",
        ),
        (
            "b",
            "[package]\nname = \"b\"\nversion = \"1.0.0\"\n\n[dependencies]\na = { version = \"1\", path = \"../a\" }\n",
        ),
    ] {
        fs::create_dir_all(root.join(path)).unwrap();
        fs::write(root.join(path).join("Cargo.toml"), manifest).unwrap();
    }

    let status = run_smif(&root, &["status"]);
    assert!(!status.status.success(), "{status:?}");
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
    assert!(output.contains("a -> b -> a"), "{output}");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn status_rejects_an_empty_changeset_summary() {
    let root = temporary_project("empty-changeset-summary", &config("channel = \"stable\""));
    fs::write(
        root.join(".changes/feature.md"),
        "app: patch:chore\n---\n\n",
    )
    .unwrap();

    let status = run_smif(&root, &["status"]);
    assert!(!status.status.success(), "{status:?}");
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
    assert!(output.contains("failed to load changesets"), "{output}");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn status_and_dry_run_version_leave_the_workspace_unchanged() {
    let root = temporary_project("status-version", &config("channel = \"stable\""));
    let manifest = root.join("Cargo.toml");
    let changeset = root.join(".changes/feature.md");
    let manifest_before = fs::read_to_string(&manifest).unwrap();
    let changeset_before = fs::read_to_string(&changeset).unwrap();

    let status = run_smif(&root, &["status"]);
    assert!(status.status.success(), "{status:?}");
    let stdout = String::from_utf8(status.stdout).unwrap();
    assert!(stdout.contains("app"));
    assert!(stdout.contains("1.0.0"));
    assert!(stdout.contains("1.0.1"));

    let version = run_smif(&root, &["--dry-run", "version", "--allow-dirty"]);
    assert!(version.status.success(), "{version:?}");
    assert_eq!(fs::read_to_string(&manifest).unwrap(), manifest_before);
    assert_eq!(fs::read_to_string(&changeset).unwrap(), changeset_before);
    assert!(fs::read_dir(&root).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".smif-")
    }));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dry_run_validates_the_planned_changelog_without_writing_files() {
    let root = temporary_project(
        "dry-run-changelog-validation",
        &config("channel = \"stable\""),
    );
    let manifest = root.join("Cargo.toml");
    let changelog = root.join("CHANGELOG.md");
    let changeset = root.join(".changes/feature.md");
    fs::write(&changelog, "not a changelog\n").unwrap();
    let manifest_before = fs::read_to_string(&manifest).unwrap();
    let changelog_before = fs::read_to_string(&changelog).unwrap();
    let changeset_before = fs::read_to_string(&changeset).unwrap();

    let version = run_smif(&root, &["--dry-run", "version", "--allow-dirty"]);

    assert!(!version.status.success(), "{version:?}");
    assert_eq!(fs::read_to_string(&manifest).unwrap(), manifest_before);
    assert_eq!(fs::read_to_string(&changelog).unwrap(), changelog_before);
    assert_eq!(fs::read_to_string(&changeset).unwrap(), changeset_before);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dry_run_version_handles_a_node_changeset_in_a_mixed_workspace() {
    let root = temporary_project(
        "mixed-version",
        "[branches]\nbase = \"main\"\nrelease = \"release\"\n\n[tags]\nchore = \"Chores\"\n\n[packages.app]\npath = \".\"\nresolver = \"rust\"\n\n[packages.node-lib]\npath = \"node\"\nresolver = \"nodejs\"\n\n[resolver.rust.pre-check]\ntype = \"http\"\nurl = \"\"\n",
    );
    let node_manifest = root.join("node/package.json");
    fs::create_dir_all(node_manifest.parent().unwrap()).unwrap();
    fs::write(&node_manifest, r#"{"name":"node-lib","version":"1.0.0"}"#).unwrap();
    fs::write(
        root.join(".changes/feature.md"),
        "node-lib: patch:chore\n---\n\nExercise mixed-ecosystem versioning.\n",
    )
    .unwrap();
    let before = fs::read_to_string(&node_manifest).unwrap();

    let version = run_smif(&root, &["--dry-run", "version", "--allow-dirty"]);

    assert!(version.status.success(), "{version:?}");
    assert_eq!(fs::read_to_string(&node_manifest).unwrap(), before);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dry_run_version_runs_explicitly_allowed_post_version_commands() {
    let root = temporary_project(
        "dry-run-post-version",
        &format!(
            "{}\n[[resolver.rust.post-version]]\ncommand = \"sh\"\nargs = [\"-c\", \"touch dry-run-marker\"]\ndry-run = true\n",
            config("channel = \"stable\"")
        ),
    );
    let marker = root.join("dry-run-marker");

    let version = run_smif(&root, &["--dry-run", "version", "--allow-dirty"]);

    assert!(version.status.success(), "{version:?}");
    assert!(marker.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn inherited_post_version_output_precedes_each_completed_step() {
    let root = temporary_project(
        "sequential-post-version-output",
        &format!(
            "{}\n[[resolver.rust.post-version]]\ncommand = \"sh\"\nargs = [\"-c\", \"echo child-one >&2\"]\ndry-run = true\n\n[[resolver.rust.post-version]]\ncommand = \"sh\"\nargs = [\"-c\", \"echo child-two >&2\"]\ndry-run = true\n",
            config("channel = \"stable\"")
        ),
    );

    let version = run_smif(&root, &["--dry-run", "version", "--allow-dirty"]);

    assert!(version.status.success(), "{version:?}");
    let stdout = String::from_utf8(version.stdout).unwrap();
    let scope_lines = stdout
        .lines()
        .filter(|line| line.contains("app") && !line.contains("1.0"))
        .count();
    assert_eq!(scope_lines, 1, "{stdout}");
    assert!(!stdout.contains("echo child-one"), "{stdout}");
    assert!(!stdout.contains("echo child-two"), "{stdout}");
    let stderr = String::from_utf8(version.stderr).unwrap();
    let child_one = stderr.find("child-one\n").unwrap();
    let completed_one = stderr.find("sh -c echo child-one >&2").unwrap();
    let child_two = stderr.find("child-two\n").unwrap();
    let completed_two = stderr.find("sh -c echo child-two >&2").unwrap();
    assert!(child_one < completed_one, "{stderr}");
    assert!(completed_one < child_two, "{stderr}");
    assert!(child_two < completed_two, "{stderr}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn post_version_failure_keeps_applied_files_and_changesets_for_recovery() {
    let root = temporary_project(
        "post-version-failure",
        &format!(
            "{}\n[[resolver.rust.post-version]]\ncommand = \"sh\"\nargs = [\"-c\", \"exit 7\"]\n",
            config("channel = \"stable\"")
        ),
    );
    let manifest = root.join("Cargo.toml");
    let changelog = root.join("CHANGELOG.md");
    let changeset = root.join(".changes/feature.md");

    let version = run_smif(&root, &["version", "--allow-dirty"]);

    assert!(!version.status.success(), "{version:?}");
    assert!(
        fs::read_to_string(&manifest)
            .unwrap()
            .contains("version = \"1.0.1\"")
    );
    assert!(changelog.exists());
    assert!(changeset.exists());
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&version.stdout),
        String::from_utf8_lossy(&version.stderr)
    );
    assert!(output.contains("feature"), "{output}");
    assert!(output.contains("sh -c exit 7"), "{output}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn version_applies_manifest_and_changelog_edits_together() {
    let root = temporary_project("version-file-edits", &config("channel = \"stable\""));
    let manifest = root.join("Cargo.toml");
    let changelog = root.join("CHANGELOG.md");
    let changeset = root.join(".changes/feature.md");

    let version = run_smif(&root, &["version", "--allow-dirty"]);

    assert!(version.status.success(), "{version:?}");
    assert!(
        fs::read_to_string(&manifest)
            .unwrap()
            .contains("version = \"1.0.1\"")
    );
    assert_eq!(
        fs::read_to_string(&changelog).unwrap(),
        "# Changelog\n\n## v1.0.1\n\n### Chores\n\n- Exercise the CLI.\n"
    );
    assert!(!changeset.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn version_renders_custom_release_and_changeset_templates() {
    let root = temporary_project(
        "custom-changelog-templates",
        &format!(
            "{}\n[changelog]\ntemplate = '''Release {{{{ package.next_version }}}}\n{{% for section in sections %}}[{{{{ section.name }}}}]\n{{% for entry in section.entries %}}{{{{ entry.content }}}}{{% endfor %}}{{% endfor %}}'''\nchangeset-template = '''* {{{{ changeset.id }}}}: {{{{ changeset.summary }}}}'''\n",
            config("channel = \"stable\"")
        ),
    );
    let changelog = root.join("CHANGELOG.md");

    let version = run_smif(&root, &["version", "--allow-dirty"]);

    assert!(version.status.success(), "{version:?}");
    assert_eq!(
        fs::read_to_string(changelog).unwrap(),
        concat!(
            "# Changelog\n\n",
            "<!-- semifold:release version=1.0.1 -->\n",
            "Release 1.0.1\n",
            "[Chores]\n",
            "* feature: Exercise the CLI.\n",
            "<!-- semifold:release:end -->\n",
        )
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn dry_run_rejects_invalid_changelog_templates_without_side_effects() {
    let root = temporary_project(
        "invalid-changelog-template",
        &format!(
            "{}\n[changelog]\nchangeset-template = \"{{{{ changeset.unknown }}}}\"\n",
            config("channel = \"stable\"")
        ),
    );
    let manifest = root.join("Cargo.toml");
    let changeset = root.join(".changes/feature.md");
    let manifest_before = fs::read_to_string(&manifest).unwrap();
    let changeset_before = fs::read_to_string(&changeset).unwrap();

    let version = run_smif(&root, &["--dry-run", "version", "--allow-dirty"]);

    assert!(!version.status.success(), "{version:?}");
    assert_eq!(fs::read_to_string(manifest).unwrap(), manifest_before);
    assert_eq!(fs::read_to_string(changeset).unwrap(), changeset_before);
    assert!(!root.join("CHANGELOG.md").exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn version_renders_multiline_changesets_as_single_list_items() {
    let root = temporary_project(
        "version-multiline-changelog",
        &config("channel = \"stable\""),
    );
    let changelog = root.join("CHANGELOG.md");
    let first_changeset = root.join(".changes/feature.md");
    let second_changeset = root.join(".changes/follow-up.md");
    fs::write(
        &first_changeset,
        "---\napp: patch:chore\n---\n\nFirst line\n\nSecond line\nThird line\n",
    )
    .unwrap();
    fs::write(
        &second_changeset,
        "app: patch:chore\n---\n\nAnother changeset\n",
    )
    .unwrap();

    let version = run_smif(&root, &["version", "--allow-dirty"]);

    assert!(version.status.success(), "{version:?}");
    let content = fs::read_to_string(&changelog).unwrap();
    assert!(
        content.contains("- First line\n\n    Second line Third line\n\n"),
        "{content}"
    );
    assert!(!content.contains("\n    \n"), "{content}");
    assert!(content.contains("- Another changeset"), "{content}");
    assert!(!first_changeset.exists());
    assert!(!second_changeset.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn version_keeps_changesets_when_changelog_planning_fails() {
    let root = temporary_project(
        "version-changelog-validation",
        &config("channel = \"stable\""),
    );
    let manifest = root.join("Cargo.toml");
    let changelog = root.join("CHANGELOG.md");
    let changeset = root.join(".changes/feature.md");
    fs::write(&changelog, "not a changelog\n").unwrap();
    let manifest_before = fs::read_to_string(&manifest).unwrap();
    let changelog_before = fs::read_to_string(&changelog).unwrap();

    let version = run_smif(&root, &["version", "--allow-dirty"]);

    assert!(!version.status.success(), "{version:?}");
    assert_eq!(fs::read_to_string(&manifest).unwrap(), manifest_before);
    assert_eq!(fs::read_to_string(&changelog).unwrap(), changelog_before);
    assert!(changeset.exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn config_migrate_and_channel_check_preserve_expected_file_state() {
    let root = temporary_project(
        "config",
        &config("version-mode = { pre-release = { tag = \"alpha\" } }"),
    );
    let config_path = root.join(".changes/config.toml");
    let original = fs::read_to_string(&config_path).unwrap();

    let check = run_smif(&root, &["config", "migrate", "--check"]);
    assert!(!check.status.success());
    assert_eq!(fs::read_to_string(&config_path).unwrap(), original);

    let migrate = run_smif(&root, &["config", "migrate"]);
    assert!(migrate.status.success(), "{migrate:?}");
    let migrated = fs::read_to_string(&config_path).unwrap();
    assert!(migrated.contains("# keep this comment"));
    assert!(!migrated.contains("version-mode"));
    assert!(migrated.contains("channel = \"alpha\""));

    let channel_check = run_smif(
        &root,
        &["config", "channel", "clear", "--package", "app", "--check"],
    );
    assert!(!channel_check.status.success());
    assert_eq!(fs::read_to_string(&config_path).unwrap(), migrated);

    let clear = run_smif(&root, &["config", "channel", "clear", "--package", "app"]);
    assert!(clear.status.success(), "{clear:?}");
    assert!(
        !fs::read_to_string(&config_path)
            .unwrap()
            .contains("channel = \"alpha\"")
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn channel_preserve_is_consumed_only_after_successful_version() {
    let root = temporary_project("channel-preserve", &config(""));
    let config_path = root.join(".changes/config.toml");

    let set = run_smif(
        &root,
        &[
            "config",
            "channel",
            "set",
            "alpha",
            "--package",
            "app",
            "--bump",
            "preserve",
        ],
    );
    assert!(set.status.success(), "{set:?}");
    let configured = fs::read_to_string(&config_path).unwrap();
    assert!(configured.contains("channel-bump = \"preserve\""));

    let dry_run = run_smif(&root, &["--dry-run", "version", "--allow-dirty"]);
    assert!(dry_run.status.success(), "{dry_run:?}");
    assert!(
        fs::read_to_string(&config_path)
            .unwrap()
            .contains("channel-bump = \"preserve\"")
    );

    let version = run_smif(&root, &["version", "--allow-dirty"]);
    assert!(version.status.success(), "{version:?}");
    assert!(
        fs::read_to_string(root.join("Cargo.toml"))
            .unwrap()
            .contains("version = \"1.0.0-alpha.0\"")
    );
    assert!(
        !fs::read_to_string(&config_path)
            .unwrap()
            .contains("channel-bump")
    );

    fs::remove_dir_all(root).unwrap();
}
