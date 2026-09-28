//! `bosun init` — set bosun up on a machine.
//!
//! Clone the dotfiles repo into `~/.bosun/repo`, ask which groups of files to
//! manage, then write `~/.bosun/config.yaml` and print the command list.
//!
//! Every question goes through the [`Prompter`](crate::ui::Prompter), so the
//! same code path runs unattended under `--non-interactive`/`--answers`. That
//! is what the integration test drives.

use anyhow::{Context, Result, bail};
use indexmap::IndexMap;

use crate::app::App;
use crate::cli::InitArgs;
use crate::config::local::LocalConfig;
use crate::config::repo::RepoConfig;

/// Default clone URL. Overridable with `--repo`, and asked for interactively.
const DEFAULT_REPO: &str = "https://github.com/planesailingio/bosun.git";

pub fn run(app: &mut App, args: &InitArgs) -> Result<()> {
    if app.paths.is_initialised() && !args.force {
        bail!(
            "bosun is already set up at {}. Re-run with --force to redo the wizard, \
             or edit the file directly.",
            app.paths.config.display()
        );
    }

    app.paths.ensure_dirs()?;

    let url = clone_repo(app, args)?;
    let manifest = RepoConfig::load(&app.paths.repo.join(crate::config::MANIFEST_NAME))
        .context("reading the repo's bosun.yaml")?;

    let groups = ask_groups(app, &manifest)?;

    let cfg = LocalConfig {
        meta: crate::config::local::LocalMeta { repo: Some(url) },
        groups,
        machine: IndexMap::new(),
    };

    cfg.save(&app.paths.config)
        .with_context(|| format!("writing {}", app.paths.config.display()))?;
    app.ui.ok(format!("wrote {}", app.paths.config.display()));

    report_missed_answers(app);
    print_next_steps(app);
    Ok(())
}

/// Clone the repo unless it is already there. Returns the URL recorded in the
/// config so `bosun update` knows where the clone came from.
fn clone_repo(app: &mut App, args: &InitArgs) -> Result<String> {
    let repo = app.repo();
    if repo.exists() {
        app.ui.ok(format!(
            "using the existing clone at {}",
            repo.path.display()
        ));
        return Ok(args
            .repo
            .clone()
            .unwrap_or_else(|| DEFAULT_REPO.to_string()));
    }

    let url = match &args.repo {
        Some(u) => u.clone(),
        None => app
            .ui
            .prompter
            .text("repo.url", "Dotfiles repo to clone", Some(DEFAULT_REPO))?,
    };

    app.ui.say(format!("Cloning {url}"));
    repo.clone_from(&url).with_context(|| {
        format!(
            "cloning {url}. For a private repo, use an SSH URL or sign in with `gh auth login` first."
        )
    })?;
    app.ui.ok(format!("cloned into {}", repo.path.display()));
    Ok(url)
}

fn ask_groups(app: &mut App, manifest: &RepoConfig) -> Result<IndexMap<String, bool>> {
    app.ui.heading("Which configuration should bosun manage?");
    app.ui
        .say("The whole machine gets the same set of files; this is asked once.");

    let mut answers = IndexMap::new();
    for (name, spec) in &manifest.groups {
        let enabled = app.ui.prompter.confirm(
            &format!("groups.{name}"),
            &format!("{name} — {}", spec.description),
            spec.default,
        )?;
        answers.insert(name.clone(), enabled);
    }
    Ok(answers)
}

/// An answers file that is missing keys silently takes defaults, which makes a
/// half-written fixture look like a pass. Say so instead.
fn report_missed_answers(app: &App) {
    if app.ui.prompter.is_interactive() {
        return;
    }
    app.ui
        .detail("questions answered from defaults are listed with -v");
}

fn print_next_steps(app: &App) {
    app.ui.heading("Ready");
    app.ui.say("Next:");
    app.ui
        .say("  bosun plan                preview what would change in your home directory");
    app.ui
        .say("  bosun apply               write the files and run the hooks");
    app.ui.say("");
    app.ui
        .say("Profile switching is `hats`, a separate tool: `hats init` sets it up.");
    app.ui.say("");
    app.ui.say("  bosun --help              everything else");
}

#[cfg(test)]
mod tests {
    // The init flow is covered end to end by tests/init.rs, which drives the
    // wizard with an answers file against a fixture repo.
}
