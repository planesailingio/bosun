//! End-to-end coverage of `bosun init` and the commands that read what it
//! writes.
//!
//! The wizard runs unattended here through `--answers`, which is the point of
//! routing every question through one `Prompter`: the interactive and the
//! recorded paths are the same code.

use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;

/// A throwaway dotfiles repo with a manifest, tagged so version checks have
/// something real to compare against.
fn fixture_repo(dir: &Path, tag: Option<&str>) -> PathBuf {
    let repo = dir.join("dotfiles");
    std::fs::create_dir_all(repo.join("files/.config")).unwrap();
    std::fs::create_dir_all(repo.join("hooks")).unwrap();
    std::fs::write(
        repo.join("bosun.yaml"),
        r#"
bosun:
  schema: 1
groups:
  shell: { description: "zsh config", default: true }
  theme: { description: "colours", default: true }
  extras: { description: "optional bits", default: false }
files:
  - { path: .zshrc.j2, group: shell }
  - { path: .config/starship.toml, group: theme }
  - { path: .extras-file, group: extras }
hooks:
  - { name: probe, phase: after, trigger: once, script: hooks/probe.sh }
"#,
    )
    .unwrap();
    std::fs::write(
        repo.join("files/.zshrc.j2"),
        "# zshrc\nexport BREW={{ brew_prefix }}\n",
    )
    .unwrap();
    std::fs::write(
        repo.join("files/.config/starship.toml"),
        "add_newline = false\n",
    )
    .unwrap();
    std::fs::write(repo.join("files/.extras-file"), "extras\n").unwrap();
    std::fs::write(repo.join("hooks/probe.sh"), "#!/bin/sh\nexit 0\n").unwrap();

    let git = |args: &[&str]| {
        let out = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "--quiet", "-b", "main"]);
    git(&["config", "user.email", "t@example.com"]);
    git(&["config", "user.name", "T"]);
    git(&["add", "."]);
    git(&["commit", "--quiet", "-m", "manifest"]);
    if let Some(t) = tag {
        git(&["tag", "-a", t, "-m", t]);
    }
    repo
}

const ANSWERS: &str = r##"
answers:
  groups.shell: true
  groups.theme: true
  groups.extras: false
"##;

struct Env {
    _dir: tempfile::TempDir,
    home: PathBuf,
    /// Stands in for `$HOME`, where the managed files go.
    user_home: PathBuf,
    repo: PathBuf,
    answers: PathBuf,
}

impl Env {
    fn new(tag: Option<&str>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("bosun-home");
        let user_home = dir.path().join("home");
        std::fs::create_dir_all(&user_home).unwrap();
        let repo = fixture_repo(dir.path(), tag);
        let answers = dir.path().join("answers.yaml");
        std::fs::write(&answers, ANSWERS).unwrap();
        Self {
            _dir: dir,
            home,
            user_home,
            repo,
            answers,
        }
    }

    fn bosun(&self) -> Command {
        let mut c = Command::cargo_bin("bosun").unwrap();
        c.arg("--bosun-home")
            .arg(&self.home)
            .arg("--no-color")
            .arg("--allow-mismatch")
            // Managed files land in a throwaway home, never the developer's.
            .env("HOME", &self.user_home)
            .env_remove("BOSUN_HOME")
            .env_remove("BOSUN_DEV");
        c
    }

    fn init(&self) -> Command {
        let mut c = self.bosun();
        c.arg("--answers")
            .arg(&self.answers)
            .arg("init")
            .arg("--repo")
            .arg(&self.repo);
        c
    }

    fn config_text(&self) -> String {
        std::fs::read_to_string(self.home.join("config.yaml")).unwrap()
    }
}

#[test]
fn init_clones_the_repo_and_writes_a_config() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();

    assert!(
        env.home.join("repo/bosun.yaml").is_file(),
        "repo was not cloned"
    );
    assert!(
        env.home.join("config.yaml").is_file(),
        "config was not written"
    );
    assert!(env.home.join("backups").is_dir());
    assert!(env.home.join("plans").is_dir());

    let cfg = env.config_text();
    assert!(
        cfg.starts_with("# ~/.bosun/config.yaml"),
        "header missing:\n{cfg}"
    );
}

#[test]
fn the_wizard_records_every_answer_it_was_given() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();
    let cfg = env.config_text();

    assert!(cfg.contains("shell: true"));
    assert!(cfg.contains("theme: true"));
    assert!(cfg.contains("extras: false"));
    assert!(cfg.contains("repo:"));
}

#[test]
fn init_refuses_to_clobber_an_existing_setup_without_force() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();
    env.init()
        .assert()
        .failure()
        .stderr(predicate::str::contains("already set up"));
    env.init().arg("--force").assert().success();
}

#[test]
fn plan_shows_the_enabled_files_and_apply_writes_them() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();

    // A plan with work exits 2, terraform style, and lists only the files in
    // enabled groups.
    env.bosun()
        .arg("plan")
        .assert()
        .code(2)
        .stdout(predicate::str::contains("~/.zshrc"))
        .stdout(predicate::str::contains("~/.config/starship.toml"))
        .stdout(predicate::str::contains("~/.extras-file").not())
        .stdout(predicate::str::contains("probe"));

    env.bosun().args(["apply", "--yes"]).assert().success();

    let zshrc = std::fs::read_to_string(env.user_home.join(".zshrc")).unwrap();
    assert!(zshrc.contains("export BREW="), "{zshrc}");
    assert!(!zshrc.contains("{{"), "template markers left: {zshrc}");
    assert!(env.user_home.join(".config/starship.toml").is_file());
    assert!(!env.user_home.join(".extras-file").exists());

    // Settled: a second plan has nothing to do and exits 0.
    env.bosun().arg("plan").assert().code(0);
}

#[test]
fn lint_passes_on_the_fixture_repo() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();
    env.bosun()
        .args(["lint", "--no-external"])
        .assert()
        .success()
        .stdout(predicate::str::contains("look fine"));
}

#[test]
fn doctor_passes_on_a_freshly_initialised_machine() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();
    env.bosun()
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("groups enabled"))
        .stdout(predicate::str::contains("shell, theme"));
}

#[test]
fn doctor_before_init_fails_and_says_to_run_init() {
    let env = Env::new(Some("v0.1.0"));
    env.bosun()
        .arg("doctor")
        .assert()
        .failure()
        .stdout(predicate::str::contains("bosun init"));
}

#[test]
fn version_reports_the_binary_and_the_repo() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();
    env.bosun()
        .args(["version", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains(concat!(
            "\"version\":\"",
            env!("CARGO_PKG_VERSION")
        )))
        .stdout(predicate::str::contains("\"repo_tag\""));
}

/// `--check` is the machine-readable half of the lockstep: 0 in step, 3 repo
/// behind, 4 binary behind.
#[test]
fn update_check_reports_a_repo_behind_the_binary() {
    // Tagged with something older than this binary will ever be.
    let env = Env::new(Some("v0.0.1"));
    env.init().assert().success();
    env.bosun()
        .args(["update", "--check"])
        .assert()
        .code(3)
        .stdout(predicate::str::contains("bosun update"));
}

#[test]
fn update_check_reports_a_repo_ahead_of_the_binary() {
    let env = Env::new(Some("v999.0.0"));
    env.init().assert().success();
    env.bosun()
        .args(["update", "--check"])
        .assert()
        .code(4)
        .stdout(predicate::str::contains("brew upgrade"));
}

#[test]
fn update_check_is_clean_when_the_tags_agree() {
    let env = Env::new(Some(concat!("v", env!("CARGO_PKG_VERSION"))));
    env.init().assert().success();
    env.bosun().args(["update", "--check"]).assert().code(0);
}

#[test]
fn update_explains_a_tag_that_has_not_been_published_yet() {
    let env = Env::new(Some("v0.0.1"));
    env.init().assert().success();
    env.bosun()
        .arg("update")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no tag"));
}

#[test]
fn a_repo_with_a_newer_schema_is_refused_with_an_upgrade_hint() {
    let env = Env::new(Some("v0.1.0"));
    env.init().assert().success();
    std::fs::write(env.home.join("repo/bosun.yaml"), "bosun:\n  schema: 99\n").unwrap();
    env.bosun()
        .args(["lint", "--no-external"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("brew upgrade bosun"));
}

#[test]
fn completions_are_generated_for_zsh() {
    let env = Env::new(None);
    env.bosun()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_bosun"));
}
