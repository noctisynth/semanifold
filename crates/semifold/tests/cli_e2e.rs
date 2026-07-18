use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

fn temporary_project(name: &str, config: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "semifold-cli-e2e-{name}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(root.join(".changes")).unwrap();
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
        .output()
        .unwrap()
}

fn config(package: &str) -> String {
    format!(
        "[branches]\nbase = \"main\"\nrelease = \"release\"\n\n[tags]\nchore = \"Chores\"\n\n[packages.app]\n# keep this comment\npath = \".\"\nresolver = \"rust\"\n{package}\n\n[resolver.rust.pre-check]\nurl = \"\"\n"
    )
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
