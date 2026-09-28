//! Typed errors for the parts of bosun where callers branch on the failure.
//!
//! Commands use `anyhow` for reporting; the engine and config layers return
//! these so tests can assert on the variant rather than on message text.

use std::path::PathBuf;

/// Something wrong with `bosun.yaml` or `~/.bosun/config.yaml`.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("{path} is not valid YAML")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_yaml_ng::Error,
    },

    #[error("could not read {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not write {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "{path} declares schema {found}, but this bosun only understands up to {supported}. \
         Run `brew upgrade bosun`."
    )]
    SchemaTooNew {
        path: PathBuf,
        found: u32,
        supported: u32,
    },

    #[error("bosun is not initialised on this machine. Run `bosun init` first.")]
    NotInitialised,
}

/// Something wrong with the dotfiles clone under `~/.bosun/repo`.
#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("no dotfiles repo at {path}. Run `bosun init` first.")]
    Missing { path: PathBuf },

    #[error("git {args} failed: {stderr}")]
    Git { args: String, stderr: String },

    #[error("could not run git")]
    Spawn {
        #[source]
        source: std::io::Error,
    },

    #[error(
        "the dotfiles repo has uncommitted changes at {path}. \
         Commit, stash, or re-run with --force."
    )]
    Dirty { path: PathBuf },

    #[error(
        "this bosun is {binary}, but the repo has no tag v{binary}. \
         The release may still be publishing; try again shortly, or `bosun update --check`."
    )]
    NoMatchingTag { binary: String },
}
