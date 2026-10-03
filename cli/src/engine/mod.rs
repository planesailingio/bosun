//! The plan/apply engine: render, compare, show, then write.

pub mod apply;
pub mod diff;
pub mod plan;
pub mod render;
pub mod state;

#[cfg(test)]
pub(crate) mod testkit {
    //! A complete fake world for engine tests: a repo with a manifest, a
    //! template, a mode-carrying file and a hook, plus a home directory to
    //! apply into.
    //!
    //! Sharing one harness across plan and apply tests is deliberate: it means
    //! the tests exercise the same path a real run takes, rather than each
    //! module's idea of it.

    use std::path::PathBuf;

    use super::apply::{ApplyContext, ApplyOptions, Outcome};
    use super::plan::{Entry, Plan, PlanOptions};
    use super::state::State;
    use crate::config::Config;
    use crate::config::local::LocalConfig;
    use crate::config::repo::RepoConfig;
    use crate::model::Filter;
    use crate::platform::{Arch, Os, Platform};

    const MANIFEST: &str = r#"
bosun: { schema: 2 }
groups:
  shell: { description: "zsh" }
  env:   { description: "zshenv" }
files:
  - { path: .zshrc.j2, group: shell }
  - { path: .zshenv.j2, group: env, mode: "0600" }
ensure:
  - { path: .profile, group: shell, line: "source ~/.config/zsh/bosun.zsh" }
hooks:
  - { name: probe, phase: after, trigger: once, script: hooks/probe.sh }
"#;

    const LOCAL: &str = r#"
machine: { hostname_alias: mbp }
"#;

    pub struct Harness {
        pub repo: tempfile::TempDir,
        pub home: tempfile::TempDir,
        pub bosun_home: tempfile::TempDir,
        pub local: std::cell::RefCell<LocalConfig>,
    }

    impl Harness {
        pub fn new() -> Self {
            let repo = tempfile::tempdir().unwrap();
            let files = repo.path().join("files");
            std::fs::create_dir_all(&files).unwrap();
            std::fs::create_dir_all(repo.path().join("hooks")).unwrap();

            std::fs::write(repo.path().join("bosun.yaml"), MANIFEST).unwrap();
            std::fs::write(
                files.join(".zshrc.j2"),
                "# zshrc\nexport BREW={{ brew_prefix }}\nexport ALIAS={{ machine.hostname_alias }}\n",
            )
            .unwrap();
            std::fs::write(files.join(".zshenv.j2"), "export USER_IS={{ user }}\n").unwrap();
            std::fs::write(
                repo.path().join("hooks/probe.sh"),
                "#!/bin/sh\necho ran > \"$BOSUN_REPO/hook-ran.txt\"\n",
            )
            .unwrap();

            Self {
                repo,
                home: tempfile::tempdir().unwrap(),
                bosun_home: tempfile::tempdir().unwrap(),
                local: std::cell::RefCell::new(serde_yaml_ng::from_str(LOCAL).unwrap()),
            }
        }

        /// Make the hook fail, to test partial-apply behaviour.
        pub fn break_hook(&self) {
            std::fs::write(
                self.repo.path().join("hooks/probe.sh"),
                "#!/bin/sh\nexit 7\n",
            )
            .unwrap();
        }

        pub fn set_group(&self, group: &str, enabled: bool) {
            self.local
                .borrow_mut()
                .groups
                .insert(group.to_string(), enabled);
        }

        pub fn platform(&self) -> Platform {
            Platform {
                os: Os::Darwin,
                arch: Arch::Arm64,
                home: self.home.path().to_path_buf(),
                user: "t".into(),
                hostname: "h".into(),
                brew_prefix: PathBuf::from("/opt/homebrew"),
            }
        }

        pub fn config(&self) -> Config {
            Config {
                repo: serde_yaml_ng::from_str::<RepoConfig>(MANIFEST).unwrap(),
                local: self.local.borrow().clone(),
            }
        }

        pub fn state_path(&self) -> PathBuf {
            self.bosun_home.path().join("state.yaml")
        }

        pub fn state(&self) -> State {
            State::load(&self.state_path()).unwrap()
        }

        pub fn plan(&self) -> Plan {
            self.plan_with(|_| {})
        }

        pub fn plan_with(&self, tweak: impl FnOnce(&mut PlanOptions<'_>)) -> Plan {
            let cfg = self.config();
            let platform = self.platform();
            let state = self.state();
            let mut opts = PlanOptions {
                cfg: &cfg,
                platform: &platform,
                state: &state,
                repo_dir: self.repo.path().to_path_buf(),
                files_dir: self.repo.path().join("files"),
                bosun_home: self.bosun_home.path().to_path_buf(),
                filter: Filter::default(),
                skip_hooks: false,
                force_hook: None,
            };
            tweak(&mut opts);
            super::plan::plan(&opts).unwrap()
        }

        pub fn apply(&self) -> Outcome {
            self.apply_with(ApplyOptions::default())
        }

        pub fn apply_with(&self, opts: ApplyOptions) -> Outcome {
            self.run_apply(opts).unwrap()
        }

        pub fn try_apply(&self) -> anyhow::Result<Outcome> {
            self.run_apply(ApplyOptions::default())
        }

        fn run_apply(&self, opts: ApplyOptions) -> anyhow::Result<Outcome> {
            let plan = self.plan();
            let mut state = self.state();
            let platform = self.platform();
            let ctx = ApplyContext {
                platform: &platform,
                repo_dir: self.repo.path().to_path_buf(),
                files_dir: self.repo.path().join("files"),
                bosun_home: self.bosun_home.path().to_path_buf(),
                backups_root: self.bosun_home.path().join("backups"),
                state_path: self.state_path(),
            };
            super::apply::apply(&plan, &mut state, &ctx, &opts, |_| {})
        }

        /// The plan entry for a displayed path, failing loudly if absent.
        pub fn entry<'p>(&self, plan: &'p Plan, display: &str) -> &'p Entry {
            plan.entries
                .iter()
                .find(|e| e.display == display)
                .unwrap_or_else(|| {
                    panic!(
                        "no entry for {display}; have: {:?}",
                        plan.entries.iter().map(|e| &e.display).collect::<Vec<_>>()
                    )
                })
        }
    }
}
