//! Configuration: the repo manifest plus the machine-local file, merged.
//!
//! Two files, one type. [`Config`] is what every command sees, so no command
//! has to know which half a value came from.

pub mod condition;
pub mod local;
pub mod repo;

use crate::error::ConfigError;
use crate::paths::BosunPaths;
use local::LocalConfig;
use repo::RepoConfig;

/// The manifest file inside the dotfiles repo.
pub const MANIFEST_NAME: &str = "bosun.yaml";
/// Directory inside the repo holding the managed files.
pub const FILES_DIR: &str = "files";

/// Repo manifest and machine config, loaded together.
#[derive(Debug, Clone)]
pub struct Config {
    pub repo: RepoConfig,
    pub local: LocalConfig,
}

impl Config {
    pub fn load(paths: &BosunPaths) -> Result<Self, ConfigError> {
        let local = LocalConfig::load(&paths.config)?;
        let repo = RepoConfig::load(&paths.repo.join(MANIFEST_NAME))?;
        Ok(Self { repo, local })
    }

    /// Groups enabled on this machine, honouring the manifest default for any
    /// group the wizard has not been asked about yet (a group added upstream
    /// after `bosun init` ran).
    pub fn group_enabled(&self, group: &str) -> bool {
        match self.local.groups.get(group) {
            Some(answer) => *answer,
            None => self.repo.groups.get(group).is_some_and(|g| g.default),
        }
    }

    /// Every problem `bosun lint` should report about the configuration pair.
    pub fn problems(&self) -> Vec<String> {
        self.repo.group_problems()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::repo::GroupSpec;

    fn config(repo_yaml: &str, local_yaml: &str) -> Config {
        Config {
            repo: serde_yaml_ng::from_str(repo_yaml).unwrap(),
            local: serde_yaml_ng::from_str(local_yaml).unwrap(),
        }
    }

    #[test]
    fn an_unanswered_group_uses_the_manifest_default() {
        let c = config(
            "groups:\n  shell: { description: s, default: true }\n  extra: { description: e, default: false }\n",
            "groups: {}\n",
        );
        assert!(c.group_enabled("shell"));
        assert!(!c.group_enabled("extra"));
    }

    #[test]
    fn the_machine_answer_overrides_the_manifest_default() {
        let c = config(
            "groups:\n  shell: { description: s, default: true }\n",
            "groups: { shell: false }\n",
        );
        assert!(!c.group_enabled("shell"));
    }

    #[test]
    fn an_unknown_group_is_disabled_rather_than_assumed() {
        let c = config("groups: {}\n", "groups: {}\n");
        assert!(!c.group_enabled("ghost"));
    }

    #[test]
    fn a_clean_pair_has_no_problems() {
        let c = config(
            "groups:\n  shell: { description: s }\nfiles:\n  - { path: .zshrc.j2, group: shell }\n",
            "groups: { shell: true }\n",
        );
        assert_eq!(c.problems(), Vec::<String>::new());
    }

    #[test]
    fn group_spec_default_defaults_to_true() {
        let g: GroupSpec = serde_yaml_ng::from_str("description: d\n").unwrap();
        assert!(g.default);
    }
}
