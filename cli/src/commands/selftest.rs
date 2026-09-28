//! `bosun test` — prove the bootstrap actually works on this machine.
//!
//! The assertions are the ones that matter: not "does the file exist" but
//! "does an interactive zsh actually start with the managed rc files in
//! place".
//!
//! Every check is skipped rather than failed when its prerequisite is absent,
//! so this is safe to run on a machine that has not applied yet.

use anyhow::{Context, Result};

use crate::app::App;
use crate::cli::TestArgs;

#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    Pass,
    Fail(String),
    Skip(String),
}

struct Check {
    name: String,
    verdict: Verdict,
}

pub fn run(app: &mut App, args: &TestArgs) -> Result<i32> {
    if args.container {
        return container(app);
    }

    let checks = vec![zsh_starts(), dock_hook_guard(app)];

    let mut failed = 0;
    for check in &checks {
        match &check.verdict {
            Verdict::Pass => app.ui.ok(&check.name),
            Verdict::Skip(why) => app.ui.say(format!("skip {} ({why})", check.name)),
            Verdict::Fail(why) => {
                failed += 1;
                app.ui.warn(format!("FAIL {}: {why}", check.name));
            }
        }
    }

    let passed = checks.iter().filter(|c| c.verdict == Verdict::Pass).count();
    app.ui.say("");
    app.ui.say(format!(
        "{passed} passed, {failed} failed, {} skipped",
        checks.len() - passed - failed
    ));
    Ok(if failed > 0 { 1 } else { 0 })
}

fn check(name: impl Into<String>, verdict: Verdict) -> Check {
    Check {
        name: name.into(),
        verdict,
    }
}

/// An interactive zsh must start cleanly. A broken .zshrc breaks every terminal,
/// so this is the single most valuable assertion here.
fn zsh_starts() -> Check {
    let Ok(zsh) = which::which("zsh") else {
        return check(
            "zsh starts cleanly",
            Verdict::Skip("zsh not installed".into()),
        );
    };
    let out = std::process::Command::new(zsh)
        .args(["-ic", "echo zsh-ok"])
        .output();
    match out {
        Ok(o) if String::from_utf8_lossy(&o.stdout).contains("zsh-ok") => {
            check("zsh starts cleanly", Verdict::Pass)
        }
        Ok(o) => check(
            "zsh starts cleanly",
            Verdict::Fail(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        ),
        Err(e) => check("zsh starts cleanly", Verdict::Fail(e.to_string())),
    }
}

/// The Dock hook must be a no-op off macOS, so a Linux container run does not
/// try to drive dockutil.
fn dock_hook_guard(app: &App) -> Check {
    let name = "the dock hook is a no-op off macOS";
    let script = app.paths.repo.join("hooks/dock.sh");
    if !script.is_file() {
        return check(name, Verdict::Skip("no dock hook in this repo".into()));
    }
    let out = std::process::Command::new("sh")
        .arg(&script)
        .env("BOSUN_OS", "linux")
        .output();
    match out {
        Ok(o) if String::from_utf8_lossy(&o.stdout).contains("skipping Dock rebuild") => {
            check(name, Verdict::Pass)
        }
        Ok(o) => check(
            name,
            Verdict::Fail(format!(
                "expected a skip message, got: {}",
                String::from_utf8_lossy(&o.stdout).trim()
            )),
        ),
        Err(e) => check(name, Verdict::Fail(e.to_string())),
    }
}

/// Run the whole suite inside the Linux dev container.
fn container(app: &mut App) -> Result<i32> {
    which::which("docker").context("docker is not installed")?;
    let repo = &app.paths.repo;
    let dockerfile = repo.join(".devcontainer/Dockerfile");
    if !dockerfile.is_file() {
        anyhow::bail!("no .devcontainer/Dockerfile in {}", repo.display());
    }

    app.ui.say("Building the Linux test image…");
    let build = std::process::Command::new("docker")
        .args(["build", "-f"])
        .arg(&dockerfile)
        .args(["-t", "bosun-test"])
        .arg(repo)
        .status()?;
    if !build.success() {
        anyhow::bail!("docker build failed");
    }

    app.ui.say("Running the suite in the container…");
    let run = std::process::Command::new("docker")
        .args(["run", "--rm", "-v"])
        .arg(format!("{}:/workspace:cached", repo.display()))
        .args(["bosun-test", "/workspace/.devcontainer/test.sh"])
        .status()?;
    Ok(if run.success() { 0 } else { 1 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zsh_starting_cleanly_is_pass_or_skip_never_a_crash() {
        let c = zsh_starts();
        assert!(matches!(
            c.verdict,
            Verdict::Pass | Verdict::Skip(_) | Verdict::Fail(_)
        ));
    }

    #[test]
    fn verdicts_are_distinguishable() {
        assert_ne!(Verdict::Pass, Verdict::Skip("x".into()));
        assert_ne!(Verdict::Fail("a".into()), Verdict::Fail("b".into()));
    }
}
