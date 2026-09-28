//! bosun — the crew member who keeps the ship's equipment in order.
//!
//! A single binary that owns a dotfiles repo and converges a machine onto it:
//! Homebrew bundles, zsh, starship, terminal themes, macOS defaults and the
//! hooks that wire them up. It replaces chezmoi, a Makefile, six `run_`
//! scripts and a pile of hand-written zsh.
//!
//! Per-client context switching is not bosun's job: that is `hats`, a sibling
//! tool. bosun's templates carry exactly two hats-shaped lines — the guarded
//! `hats shell-init` eval in `.zshrc` and the `$HATS_HAT` starship module —
//! and both are no-ops on a machine without hats.

pub mod app;
pub mod cli;
pub mod commands;
pub mod config;
pub mod engine;
pub mod error;
pub mod hooks;
pub mod model;
pub mod paths;
pub mod platform;
pub mod repo;
pub mod ui;
