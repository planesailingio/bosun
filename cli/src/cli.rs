//! The command-line surface.

use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "bosun",
    version,
    about = "Machine and shell bootstrap: dotfiles, brew bundles and hooks",
    long_about = None,
    propagate_version = true
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalOpts,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, Args)]
pub struct GlobalOpts {
    /// Use this directory instead of ~/.bosun
    #[arg(long, global = true, value_name = "DIR", env = "BOSUN_HOME")]
    pub bosun_home: Option<PathBuf>,

    /// Never prompt: take every default, or the answers file
    #[arg(long, global = true)]
    pub non_interactive: bool,

    /// YAML file of recorded wizard answers (implies --non-interactive)
    #[arg(long, global = true, value_name = "FILE")]
    pub answers: Option<PathBuf>,

    /// Disable coloured output
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Work with a repo whose tag does not match this binary
    #[arg(long, global = true)]
    pub allow_mismatch: bool,

    /// Show more detail (repeatable)
    #[arg(short, long, global = true, action = ArgAction::Count)]
    pub verbose: u8,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Set up bosun on this machine: clone the repo and run the wizard
    Init(InitArgs),

    /// Move the dotfiles repo to the tag matching this binary
    Update(UpdateArgs),

    /// Show version, repo tag, and whether they agree
    Version(VersionArgs),

    /// Check that this machine has what bosun needs
    Doctor(DoctorArgs),

    /// Preview what an apply would change, terraform style
    Plan(PlanArgs),

    /// Write the managed files and run any due hooks
    Apply(ApplyArgs),

    /// Show the diff without the hook plan (alias for `plan --skip-hooks`)
    Diff(DiffArgs),

    /// Render a managed file and print it, or syntax-check it
    Render(RenderArgs),

    /// Install, check or clean up a package bundle from brew/
    Brew(BrewArgs),

    /// List or run the repo's hooks
    Hooks(HooksArgs),

    /// Check the manifest, templates and shell scripts
    Lint(LintArgs),

    /// Check the bootstrap actually works on this machine
    Test(TestArgs),

    /// Generate a shell completion script
    Completions(CompletionsArgs),
}

#[derive(Debug, Args)]
pub struct InitArgs {
    /// Clone URL for the dotfiles repo
    #[arg(long, value_name = "URL")]
    pub repo: Option<String>,

    /// Re-run the wizard over an existing configuration
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Report the versions and exit: 0 in step, 3 repo behind, 4 binary behind
    #[arg(long)]
    pub check: bool,

    /// Update even though the clone has uncommitted changes
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct VersionArgs {
    /// Machine-readable output
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Machine-readable output
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct PlanArgs {
    /// Only these paths (repeatable), e.g. --target .zshrc
    #[arg(long, value_name = "PATH")]
    pub target: Vec<String>,

    /// Only these groups (repeatable)
    #[arg(long, value_name = "GROUP")]
    pub group: Vec<String>,

    /// Do not consider hooks
    #[arg(long)]
    pub skip_hooks: bool,
}

#[derive(Debug, Args)]
pub struct ApplyArgs {
    /// Only these paths (repeatable)
    #[arg(long, value_name = "PATH")]
    pub target: Vec<String>,

    /// Only these groups (repeatable)
    #[arg(long, value_name = "GROUP")]
    pub group: Vec<String>,

    /// Do not ask for confirmation
    #[arg(long, short = 'y')]
    pub yes: bool,

    /// Write files, skip hooks
    #[arg(long, conflicts_with = "hooks_only")]
    pub only_files: bool,

    /// Run hooks, write no files
    #[arg(long)]
    pub hooks_only: bool,

    /// Leave files that are no longer managed in place
    #[arg(long, conflicts_with = "force_prune")]
    pub no_prune: bool,

    /// Remove unmanaged files even if they were edited locally
    #[arg(long)]
    pub force_prune: bool,
}

#[derive(Debug, Args)]
pub struct DiffArgs {
    /// Only these paths (repeatable)
    #[arg(long, value_name = "PATH")]
    pub target: Vec<String>,

    /// Only these groups (repeatable)
    #[arg(long, value_name = "GROUP")]
    pub group: Vec<String>,
}

#[derive(Debug, Args)]
pub struct RenderArgs {
    /// Only these paths (repeatable); default is every managed file
    #[arg(long, value_name = "PATH")]
    pub target: Vec<String>,

    /// Syntax-check the output instead of printing it
    #[arg(long)]
    pub check: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum BrewAction {
    /// Install everything in the bundle
    Install,
    /// Report what is missing, install nothing
    Check,
    /// List (or with --force, remove) what is not in the bundle
    Cleanup,
    /// Snapshot this machine into Brewfile.new
    Dump,
}

/// A named set of packages under `brew/`. Every bundle includes `core`, so
/// `bosun brew install devops` means core + devops — you never get a machine
/// without the base set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum BrewBundle {
    /// Base set only: shell, git, data wrangling, secrets, comms
    Core,
    /// core + clusters, cloud CLIs, IaC, containers
    Devops,
    /// core + offensive security and recon
    Pentest,
    /// core + languages, service clients, code SAST, release tooling
    Dev,
    /// Everything: core + devops + pentest + dev
    #[default]
    Full,
}

impl BrewBundle {
    /// The bundle files this expands to, in install order. `core` leads every
    /// list: the composition lives here rather than in the Brewfiles so the
    /// files stay flat, declarative and free of Ruby include tricks.
    pub fn files(self) -> &'static [&'static str] {
        match self {
            Self::Core => &["core"],
            Self::Devops => &["core", "devops"],
            Self::Pentest => &["core", "pentest"],
            Self::Dev => &["core", "dev"],
            Self::Full => &["core", "devops", "pentest", "dev"],
        }
    }
}

#[derive(Debug, Args)]
pub struct BrewArgs {
    #[arg(value_enum)]
    pub action: BrewAction,

    /// Which bundle to act on; defaults to `full`
    #[arg(value_enum, default_value_t = BrewBundle::Full)]
    pub bundle: BrewBundle,

    /// For cleanup: actually uninstall
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args)]
pub struct HooksArgs {
    #[command(subcommand)]
    pub command: HooksCommand,
}

#[derive(Debug, Subcommand)]
pub enum HooksCommand {
    /// Show every hook and whether it is due
    List,
    /// Run one hook
    Run {
        name: String,
        /// Run even if it is not due
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, Args)]
pub struct LintArgs {
    /// Skip shellcheck and gitleaks even if installed
    #[arg(long)]
    pub no_external: bool,
}

#[derive(Debug, Args)]
pub struct TestArgs {
    /// Run the suite inside the Linux dev container instead
    #[arg(long)]
    pub container: bool,
}

#[derive(Debug, Args)]
pub struct CompletionsArgs {
    /// Shell to generate for
    #[arg(value_enum)]
    pub shell: clap_complete::Shell,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_command_tree_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn global_flags_are_accepted_after_the_subcommand() {
        let cli = Cli::try_parse_from(["bosun", "version", "--json", "-vv"]).unwrap();
        assert_eq!(cli.global.verbose, 2);
        assert!(matches!(
            cli.command,
            Command::Version(VersionArgs { json: true })
        ));
    }

    #[test]
    fn bosun_home_can_come_from_a_flag() {
        let cli = Cli::try_parse_from(["bosun", "--bosun-home", "/tmp/b", "doctor"]).unwrap();
        assert_eq!(cli.global.bosun_home.unwrap().to_str().unwrap(), "/tmp/b");
    }

    #[test]
    fn update_check_parses() {
        let cli = Cli::try_parse_from(["bosun", "update", "--check"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Update(UpdateArgs { check: true, .. })
        ));
    }
}
