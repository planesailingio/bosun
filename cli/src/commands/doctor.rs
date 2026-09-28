//! `bosun doctor` — is this machine ready, and if not, what should I run?
//!
//! Each check reports a status and, when it fails, the one command that fixes
//! it. Exit code is non-zero only for problems that block bosun, not for
//! optional tooling that is merely absent.

use anyhow::Result;

use crate::app::App;
use crate::cli::DoctorArgs;
use crate::config::Config;
use crate::repo::{BINARY_VERSION, VersionStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    /// Works, but something optional is missing.
    Warn,
    /// bosun cannot do its job until this is fixed.
    Fail,
}

impl Level {
    fn mark(self) -> &'static str {
        match self {
            Level::Ok => "ok  ",
            Level::Warn => "warn",
            Level::Fail => "FAIL",
        }
    }
}

#[derive(Debug)]
pub struct Check {
    pub name: String,
    pub level: Level,
    pub detail: String,
    /// The command that fixes it, if there is one.
    pub fix: Option<String>,
}

impl Check {
    fn ok(name: &str, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            level: Level::Ok,
            detail: detail.into(),
            fix: None,
        }
    }
    fn warn(name: &str, detail: impl Into<String>, fix: Option<&str>) -> Self {
        Self {
            name: name.into(),
            level: Level::Warn,
            detail: detail.into(),
            fix: fix.map(str::to_owned),
        }
    }
    fn fail(name: &str, detail: impl Into<String>, fix: Option<&str>) -> Self {
        Self {
            name: name.into(),
            level: Level::Fail,
            detail: detail.into(),
            fix: fix.map(str::to_owned),
        }
    }
}

/// Tools bosun or its hooks call. `required` ones are checked as failures.
const TOOLS: &[(&str, bool, &str)] = &[
    ("git", true, "cloning and updating the dotfiles repo"),
    ("zsh", false, "the shell the managed rc files target"),
    ("brew", false, "the Brewfile hook"),
];

pub fn run(app: &mut App, args: &DoctorArgs) -> Result<()> {
    let checks = collect(app);

    if args.json {
        let items: Vec<serde_json::Value> = checks
            .iter()
            .map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "level": match c.level {
                        Level::Ok => "ok",
                        Level::Warn => "warn",
                        Level::Fail => "fail",
                    },
                    "detail": c.detail,
                    "fix": c.fix,
                })
            })
            .collect();
        app.ui.say(serde_json::to_string(&items)?);
    } else {
        for c in &checks {
            app.ui
                .say(format!("{}  {:<12} {}", c.level.mark(), c.name, c.detail));
            if let Some(fix) = &c.fix {
                app.ui.say(format!("      {:<12} → {fix}", ""));
            }
        }
    }

    if checks.iter().any(|c| c.level == Level::Fail) {
        std::process::exit(1);
    }
    Ok(())
}

/// Run every check. Separated from printing so tests can assert on results.
pub fn collect(app: &App) -> Vec<Check> {
    let mut checks = Vec::new();

    checks.push(Check::ok("bosun", format!("v{BINARY_VERSION}")));

    // Platform.
    match app.platform() {
        Ok(p) => checks.push(Check::ok(
            "platform",
            format!(
                "{} {} (brew prefix {})",
                p.os,
                p.arch,
                p.brew_prefix.display()
            ),
        )),
        Err(e) => checks.push(Check::fail("platform", e.to_string(), None)),
    }

    // Tools.
    for (tool, required, why) in TOOLS {
        match which::which(tool) {
            Ok(path) => checks.push(Check::ok(tool, path.display().to_string())),
            Err(_) if *required => checks.push(Check::fail(
                tool,
                format!("not found; needed for {why}"),
                Some(&format!("brew install {tool}")),
            )),
            Err(_) => checks.push(Check::warn(
                tool,
                format!("not found; {why} will not work"),
                Some(&format!("brew install {tool}")),
            )),
        }
    }

    // Home directory and repo.
    if app.paths.root.is_dir() {
        checks.push(Check::ok(
            "bosun home",
            app.paths.root.display().to_string(),
        ));
    } else {
        checks.push(Check::warn(
            "bosun home",
            format!("{} does not exist", app.paths.root.display()),
            Some("bosun init"),
        ));
    }

    let repo = app.repo();
    match repo.status() {
        Ok(VersionStatus::Missing) => checks.push(Check::fail(
            "repo",
            format!("no clone at {}", repo.path.display()),
            Some("bosun init"),
        )),
        Ok(status @ VersionStatus::Match { .. }) => {
            checks.push(Check::ok("repo", status.summary()))
        }
        Ok(status @ VersionStatus::Untagged { .. }) => {
            checks.push(Check::warn("repo", status.summary(), None))
        }
        Ok(status @ VersionStatus::RepoBehind { .. }) => {
            checks.push(Check::warn("repo", status.summary(), Some("bosun update")))
        }
        Ok(status @ VersionStatus::RepoAhead { .. }) => checks.push(Check::warn(
            "repo",
            status.summary(),
            Some("brew upgrade bosun"),
        )),
        Err(e) => checks.push(Check::fail("repo", e.to_string(), Some("bosun init"))),
    }

    // Configuration.
    if !app.paths.is_initialised() {
        checks.push(Check::fail(
            "config",
            format!("{} is missing", app.paths.config.display()),
            Some("bosun init"),
        ));
        return checks;
    }

    match Config::load(&app.paths) {
        Ok(cfg) => {
            let on: Vec<&str> = cfg
                .repo
                .groups
                .keys()
                .map(String::as_str)
                .filter(|g| cfg.group_enabled(g))
                .collect();
            checks.push(Check::ok(
                "config",
                format!(
                    "{} group{} enabled: {}",
                    on.len(),
                    if on.len() == 1 { "" } else { "s" },
                    on.join(", ")
                ),
            ));

            let problems = cfg.problems();
            if problems.is_empty() {
                checks.push(Check::ok("manifest", "groups and files agree"));
            } else {
                for p in problems {
                    checks.push(Check::fail("manifest", p, Some("bosun lint")));
                }
            }
        }
        Err(e) => checks.push(Check::fail("config", e.to_string(), Some("bosun init"))),
    }

    checks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::GlobalOpts;

    fn app_in(dir: &std::path::Path) -> App {
        App::new(&GlobalOpts {
            bosun_home: Some(dir.to_path_buf()),
            non_interactive: true,
            answers: None,
            no_color: true,
            allow_mismatch: false,
            verbose: 0,
        })
        .unwrap()
    }

    #[test]
    fn an_uninitialised_machine_fails_on_repo_and_config() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_in(dir.path());
        let checks = collect(&app);

        let repo = checks.iter().find(|c| c.name == "repo").unwrap();
        assert_eq!(repo.level, Level::Fail);
        assert_eq!(repo.fix.as_deref(), Some("bosun init"));

        let config = checks.iter().find(|c| c.name == "config").unwrap();
        assert_eq!(config.level, Level::Fail);
    }

    #[test]
    fn git_is_checked_as_required() {
        let dir = tempfile::tempdir().unwrap();
        let checks = collect(&app_in(dir.path()));
        let git = checks.iter().find(|c| c.name == "git").unwrap();
        // Tests cannot run without git on PATH, so this must pass.
        assert_eq!(git.level, Level::Ok, "{}", git.detail);
    }

    #[test]
    fn optional_tooling_never_fails_the_run() {
        let dir = tempfile::tempdir().unwrap();
        let checks = collect(&app_in(dir.path()));
        for name in ["brew", "zsh"] {
            let c = checks.iter().find(|c| c.name == name).unwrap();
            assert_ne!(c.level, Level::Fail, "{name} must not be a hard failure");
        }
    }

    #[test]
    fn levels_render_a_fixed_width_marker() {
        assert_eq!(Level::Ok.mark().len(), 4);
        assert_eq!(Level::Warn.mark().len(), 4);
        assert_eq!(Level::Fail.mark().len(), 4);
    }
}
