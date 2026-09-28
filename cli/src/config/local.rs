//! `~/.bosun/config.yaml` — everything specific to this machine.
//!
//! The repo ships defaults (zsh, starship, themes, bundles); this file holds
//! the wizard's answers for this machine: where the repo is, which groups are
//! on, and any free-form template values.

use std::path::Path;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::error::ConfigError;

const HEADER: &str = "\
# ~/.bosun/config.yaml — machine-local configuration, written by `bosun init`.
#
# This file is NOT in the dotfiles repo. Edit by hand or re-run `bosun init`.
";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LocalConfig {
    #[serde(default)]
    pub meta: LocalMeta,
    /// Wizard answers, one per group declared in the repo manifest.
    #[serde(default)]
    pub groups: IndexMap<String, bool>,
    /// Free-form values exposed to templates as `machine.*`.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub machine: IndexMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LocalMeta {
    /// Clone URL for the dotfiles repo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
}

impl LocalConfig {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        if !path.exists() {
            return Err(ConfigError::NotInitialised);
        }
        let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        serde_yaml_ng::from_str(&text).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Write atomically: render to a sibling temp file, then rename, so an
    /// interrupted write cannot leave a half-parsed config behind.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let body = serde_yaml_ng::to_string(self).map_err(|source| ConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        let contents = format!("{HEADER}{body}");

        let parent = path.parent().unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
        let tmp = path.with_extension("yaml.tmp");
        std::fs::write(&tmp, contents).map_err(|source| ConfigError::Write {
            path: tmp.clone(),
            source,
        })?;
        std::fs::rename(&tmp, path).map_err(|source| ConfigError::Write {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_yaml() {
        let mut cfg = LocalConfig::default();
        cfg.meta.repo = Some("https://example.com/dotfiles.git".into());
        cfg.groups.insert("shell".into(), true);
        cfg.groups.insert("editor".into(), false);
        cfg.machine.insert("hostname_label".into(), "mbp".into());

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        cfg.save(&path).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# ~/.bosun/config.yaml"), "header missing");

        let back = LocalConfig::load(&path).unwrap();
        assert!(!back.groups["editor"]);
        assert_eq!(back.machine["hostname_label"], "mbp");
        assert_eq!(
            back.meta.repo.as_deref(),
            Some("https://example.com/dotfiles.git")
        );
    }

    #[test]
    fn a_missing_config_reports_not_initialised() {
        let dir = tempfile::tempdir().unwrap();
        let err = LocalConfig::load(&dir.path().join("nope.yaml")).unwrap_err();
        assert!(matches!(err, ConfigError::NotInitialised));
        assert!(err.to_string().contains("bosun init"));
    }

    #[test]
    fn saving_is_atomic_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.yaml");
        LocalConfig::default().save(&path).unwrap();
        assert!(path.is_file());
        assert!(!path.with_extension("yaml.tmp").exists());
    }
}
