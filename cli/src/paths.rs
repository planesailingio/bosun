//! Every path bosun owns on the machine, resolved once and passed around.
//!
//! Layout (overridable in full with `BOSUN_HOME`):
//!
//! ```text
//! ~/.bosun/
//! ├── repo/          git clone, checked out at the tag matching this binary
//! ├── config.yaml    machine-local wizard answers: repo URL, groups, machine
//! ├── state.yaml     last-applied manifest and hook markers
//! ├── backups/<ts>/  pre-overwrite copies and pruned files
//! └── plans/         saved plans
//! ```

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Resolved locations of everything under the bosun home directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BosunPaths {
    pub root: PathBuf,
    pub repo: PathBuf,
    pub config: PathBuf,
    pub state: PathBuf,
    pub backups: PathBuf,
    pub plans: PathBuf,
}

impl BosunPaths {
    /// Derive every path from a bosun home directory.
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            repo: root.join("repo"),
            config: root.join("config.yaml"),
            state: root.join("state.yaml"),
            backups: root.join("backups"),
            plans: root.join("plans"),
            root,
        }
    }

    /// `--bosun-home`, else `$BOSUN_HOME`, else `~/.bosun`.
    ///
    /// The home directory itself is resolved through `directories`, which
    /// honours `$HOME` on unix, so tests can point a whole run at a tempdir.
    pub fn resolve(override_root: Option<&Path>) -> Result<Self> {
        if let Some(root) = override_root {
            return Ok(Self::with_root(root));
        }
        let home = directories::UserDirs::new()
            .context("could not determine the home directory (is $HOME set?)")?
            .home_dir()
            .to_path_buf();
        Ok(Self::with_root(home.join(".bosun")))
    }

    /// Create the directories bosun writes into. Files are created on demand.
    pub fn ensure_dirs(&self) -> Result<()> {
        for dir in [&self.root, &self.backups, &self.plans] {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        Ok(())
    }

    /// True once `bosun init` has written a config file.
    pub fn is_initialised(&self) -> bool {
        self.config.is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_every_path_from_the_root() {
        let p = BosunPaths::with_root("/tmp/b");
        assert_eq!(p.repo, PathBuf::from("/tmp/b/repo"));
        assert_eq!(p.config, PathBuf::from("/tmp/b/config.yaml"));
        assert_eq!(p.state, PathBuf::from("/tmp/b/state.yaml"));
    }

    #[test]
    fn override_wins_over_home() {
        let p = BosunPaths::resolve(Some(Path::new("/custom"))).unwrap();
        assert_eq!(p.root, PathBuf::from("/custom"));
    }

    #[test]
    fn ensure_dirs_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let p = BosunPaths::with_root(tmp.path().join("bosun"));
        p.ensure_dirs().unwrap();
        p.ensure_dirs().unwrap();
        assert!(p.backups.is_dir());
        assert!(p.plans.is_dir());
        assert!(!p.is_initialised());
    }
}
