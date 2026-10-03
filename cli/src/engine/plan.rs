//! Working out what an apply would change, without changing anything.
//!
//! This is the half chezmoi never gave: before writing, say exactly which files
//! appear, which change and how, which permissions move, and which files bosun
//! used to own but no longer does.
//!
//! `plan` is also the single code path behind `apply` (which re-plans), `diff`
//! and `render`, so what you are shown and what happens cannot diverge.

use std::path::PathBuf;

use anyhow::Result;

use crate::config::Config;
use crate::engine::diff::{self, Body, Stats};
use crate::engine::render::{RenderContext, Renderer};
use crate::engine::state::{self, State};
use crate::hooks::{self, HookPlan};
use crate::model::{Filter, ManagedFile};
use crate::platform::Platform;

/// What will happen to one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Not there yet.
    Create,
    /// Content differs.
    Update,
    /// Content matches; only the mode is wrong.
    ModeChange { from: u32, to: u32 },
    /// Already correct.
    Unchanged,
    /// Managed before, not managed now. `safe` means it is unmodified since
    /// bosun wrote it, so removing it loses nothing.
    Destroy { safe: bool },
    /// Something is in the way that bosun will not overwrite blindly.
    Conflict { why: String },
    /// A line from `ensure:` is missing from a file bosun does not own, and
    /// will be appended. The rest of the file is left exactly as it is.
    EnsureLine,
}

impl Action {
    /// The single character that opens the line, in the style of a terraform
    /// plan.
    pub fn marker(&self) -> char {
        match self {
            Action::Create => '+',
            Action::Update => '~',
            Action::ModeChange { .. } => '!',
            Action::Unchanged => '=',
            Action::Destroy { .. } => '-',
            Action::Conflict { .. } => '×',
            Action::EnsureLine => '+',
        }
    }

    pub fn label(&self) -> String {
        match self {
            Action::Create => "create".into(),
            Action::Update => "update".into(),
            Action::ModeChange { from, to } => format!("mode {from:04o} → {to:04o}"),
            Action::Unchanged => "unchanged".into(),
            Action::Destroy { safe: true } => "destroy".into(),
            Action::Destroy { safe: false } => "destroy (modified locally)".into(),
            Action::Conflict { why } => format!("conflict: {why}"),
            Action::EnsureLine => "ensure line".into(),
        }
    }

    pub fn is_change(&self) -> bool {
        !matches!(self, Action::Unchanged)
    }
}

/// One line of the plan.
#[derive(Debug, Clone)]
pub struct Entry {
    pub target: PathBuf,
    /// `~/.zshrc` rather than the full path.
    pub display: String,
    pub action: Action,
    pub stats: Stats,
    pub body: Body,
    /// Rendered content to write. None for destroys and conflicts.
    pub contents: Option<Vec<u8>>,
    pub mode: u32,
    pub dir_mode: Option<u32>,
}

/// Counts for the summary line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Summary {
    pub add: usize,
    pub change: usize,
    pub destroy: usize,
    pub mode: usize,
    pub unchanged: usize,
    pub conflict: usize,
    pub hooks: usize,
}

impl Summary {
    pub fn has_changes(&self) -> bool {
        self.add + self.change + self.destroy + self.mode + self.conflict + self.hooks > 0
    }

    /// The terraform-style one-liner.
    pub fn line(&self) -> String {
        format!(
            "Plan: {} to add, {} to change, {} to destroy, {} permission change{}; {} hook{} to run.",
            self.add,
            self.change,
            self.destroy,
            self.mode,
            if self.mode == 1 { "" } else { "s" },
            self.hooks,
            if self.hooks == 1 { "" } else { "s" },
        )
    }
}

/// Everything an apply would do.
#[derive(Debug)]
pub struct Plan {
    pub entries: Vec<Entry>,
    /// Targets covered by `ensure:`. bosun owns one line of each, so apply
    /// drops any managed-file record it still holds for them — a leftover
    /// record from when the file *was* managed would make it a prune
    /// candidate the day the ensure entry goes away.
    pub ensured: Vec<PathBuf>,
    pub hooks: Vec<HookPlan>,
    pub summary: Summary,
    /// Notes for the plan header (repo untagged, ...).
    pub notes: Vec<String>,
}

/// Inputs to planning.
pub struct PlanOptions<'a> {
    pub cfg: &'a Config,
    pub platform: &'a Platform,
    pub state: &'a State,
    pub repo_dir: PathBuf,
    pub files_dir: PathBuf,
    pub bosun_home: PathBuf,
    pub filter: Filter,
    pub skip_hooks: bool,
    pub force_hook: Option<String>,
}

pub fn plan(opts: &PlanOptions<'_>) -> Result<Plan> {
    let files = crate::model::expand(opts.cfg, &opts.files_dir, opts.platform, &opts.filter)?;

    let ctx = RenderContext::build(opts.cfg, opts.platform, &opts.repo_dir, &opts.files_dir);
    let renderer = Renderer::new(&ctx);

    let mut entries = Vec::new();
    let mut summary = Summary::default();

    for file in &files {
        let entry = classify(&renderer, file, opts)?;
        match &entry.action {
            Action::Create => summary.add += 1,
            Action::Update => summary.change += 1,
            Action::ModeChange { .. } => summary.mode += 1,
            Action::Unchanged => summary.unchanged += 1,
            Action::Conflict { .. } => summary.conflict += 1,
            Action::Destroy { .. } => summary.destroy += 1,
            Action::EnsureLine => summary.add += 1,
        }
        entries.push(entry);
    }

    let ensure = ensure_entries(opts);
    let ensured: Vec<PathBuf> = ensure.iter().map(|e| e.target.clone()).collect();
    for entry in ensure {
        match &entry.action {
            Action::Unchanged => summary.unchanged += 1,
            Action::Conflict { .. } => summary.conflict += 1,
            _ => summary.add += 1,
        }
        entries.push(entry);
    }

    // Files bosun owned last time but does not now: a disabled group, or a file
    // removed upstream. Only detectable from state.
    // `ensure:` targets are listed unconditionally — group gates and filters
    // included — because bosun only ever owns one line of them. Leaving one off
    // this list would make a file it used to manage look like an orphan, and an
    // unmodified orphan is deleted. That is how a migration from a managed
    // `~/.zshrc` to an ensured line would eat someone's shell config.
    let managed: Vec<PathBuf> = files
        .iter()
        .map(|f| f.target.clone())
        .chain(
            opts.cfg
                .repo
                .ensure
                .iter()
                .map(|e| opts.platform.home.join(&e.path)),
        )
        .collect();
    let unfiltered = opts.filter.targets.is_empty() && opts.filter.groups.is_empty();
    if unfiltered {
        for orphan in opts.state.orphans(&managed) {
            let recorded = opts.state.file(&orphan);
            let current = std::fs::read(&orphan).ok();
            let safe = match (recorded, &current) {
                // Gone already: nothing to do, so do not list it.
                (_, None) => continue,
                (Some(r), Some(bytes)) => r.hash == state::hash(bytes),
                (None, Some(_)) => false,
            };
            summary.destroy += 1;
            entries.push(Entry {
                display: display_path(&orphan, opts.platform),
                target: orphan,
                action: Action::Destroy { safe },
                stats: Stats::default(),
                body: Body::None,
                contents: None,
                mode: 0,
                dir_mode: None,
            });
        }
    }

    entries.sort_by(|a, b| a.target.cmp(&b.target));

    let hooks = if opts.skip_hooks {
        Vec::new()
    } else {
        hooks::due(
            &opts.cfg.repo.hooks,
            &opts.repo_dir,
            opts.platform,
            opts.state,
            opts.force_hook.as_deref(),
        )
    };
    summary.hooks = hooks.len();

    Ok(Plan {
        entries,
        ensured,
        hooks,
        summary,
        notes: Vec::new(),
    })
}

fn classify(renderer: &Renderer<'_>, file: &ManagedFile, opts: &PlanOptions<'_>) -> Result<Entry> {
    let contents = renderer.render(file)?;
    let want_mode = file.mode_or_default();
    let display = file.display(&opts.platform.home);

    // A directory or symlink where a file should be is never silently replaced.
    let meta = std::fs::symlink_metadata(&file.target).ok();
    if let Some(m) = &meta {
        let why = if m.is_dir() {
            Some("a directory is in the way")
        } else if m.file_type().is_symlink() {
            Some("the target is a symlink")
        } else {
            None
        };
        if let Some(why) = why {
            return Ok(Entry {
                target: file.target.clone(),
                display,
                action: Action::Conflict { why: why.into() },
                stats: Stats::default(),
                body: Body::None,
                contents: None,
                mode: want_mode,
                dir_mode: file.dir_mode,
            });
        }
    }

    let Some(existing) = std::fs::read(&file.target).ok() else {
        let lines = std::str::from_utf8(&contents)
            .map(|t| t.lines().count())
            .unwrap_or(0);
        return Ok(Entry {
            target: file.target.clone(),
            display,
            action: Action::Create,
            stats: Stats {
                added: lines,
                removed: 0,
            },
            body: Body::Whole { lines },
            contents: Some(contents),
            mode: want_mode,
            dir_mode: file.dir_mode,
        });
    };

    let have_mode = current_mode(&file.target).unwrap_or(want_mode);

    if existing == contents {
        let action = if have_mode == want_mode {
            Action::Unchanged
        } else {
            Action::ModeChange {
                from: have_mode,
                to: want_mode,
            }
        };
        return Ok(Entry {
            target: file.target.clone(),
            display,
            action,
            stats: Stats::default(),
            body: Body::None,
            contents: Some(contents),
            mode: want_mode,
            dir_mode: file.dir_mode,
        });
    }

    let stats = match (
        std::str::from_utf8(&existing),
        std::str::from_utf8(&contents),
    ) {
        (Ok(o), Ok(n)) => Stats::of(o, n),
        _ => Stats::default(),
    };

    Ok(Entry {
        target: file.target.clone(),
        display,
        action: Action::Update,
        stats,
        body: diff::body(&existing, &contents),
        contents: Some(contents),
        mode: want_mode,
        dir_mode: file.dir_mode,
    })
}

/// Plan the `ensure:` lines.
///
/// Unlike a managed file there is nothing to render and nothing to diff: the
/// only question is whether the line is already somewhere in the target. If it
/// is not, the content to write is the whole existing file plus the line, so
/// the ordinary atomic-write-with-backup path in `apply` carries it.
fn ensure_entries(opts: &PlanOptions<'_>) -> Vec<Entry> {
    let mut out = Vec::new();

    for spec in &opts.cfg.repo.ensure {
        if !opts.cfg.group_enabled(&spec.group) || !spec.condition.matches(opts.platform) {
            continue;
        }
        let target = opts.platform.home.join(&spec.path);
        let group_ok = opts.filter.groups.is_empty() || opts.filter.groups.contains(&spec.group);
        let target_ok = opts.filter.targets.is_empty()
            || opts
                .filter
                .targets
                .iter()
                .any(|t| target.ends_with(t) || spec.path.ends_with(t) || target == *t);
        if !(group_ok && target_ok) {
            continue;
        }

        let display = display_path(&target, opts.platform);

        // Same rule as a managed file: a directory or symlink is never
        // silently replaced.
        if let Ok(m) = std::fs::symlink_metadata(&target) {
            let why = if m.is_dir() {
                Some("a directory is in the way")
            } else if m.file_type().is_symlink() {
                Some("the target is a symlink")
            } else {
                None
            };
            if let Some(why) = why {
                out.push(Entry {
                    target,
                    display,
                    action: Action::Conflict { why: why.into() },
                    stats: Stats::default(),
                    body: Body::None,
                    contents: None,
                    mode: 0o644,
                    dir_mode: None,
                });
                continue;
            }
        }

        let existing = std::fs::read_to_string(&target).ok();
        let wanted = spec.line.trim_end();

        if let Some(text) = &existing
            && text.lines().any(|l| l.trim_end() == wanted)
        {
            out.push(Entry {
                target,
                display,
                action: Action::Unchanged,
                stats: Stats::default(),
                body: Body::None,
                contents: None,
                mode: 0o644,
                dir_mode: None,
            });
            continue;
        }

        let mut next = existing.unwrap_or_default();
        if !next.is_empty() {
            if !next.ends_with('\n') {
                next.push('\n');
            }
            // A blank line so the appended line reads as bosun's, not as part
            // of whatever an installer wrote above it.
            next.push('\n');
        }
        next.push_str(wanted);
        next.push('\n');

        out.push(Entry {
            target,
            display,
            action: Action::EnsureLine,
            stats: Stats {
                added: 1,
                removed: 0,
            },
            body: Body::None,
            contents: Some(next.into_bytes()),
            mode: 0o644,
            dir_mode: None,
        });
    }

    out
}

fn display_path(path: &std::path::Path, platform: &Platform) -> String {
    match path.strip_prefix(&platform.home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

#[cfg(unix)]
fn current_mode(path: &std::path::Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .ok()
        .map(|m| m.permissions().mode() & 0o7777)
}

#[cfg(not(unix))]
fn current_mode(_path: &std::path::Path) -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::testkit::*;

    #[test]
    fn a_fresh_home_is_all_creates() {
        let t = Harness::new();
        let p = t.plan();
        assert_eq!(p.summary.add, 3, "two files and one ensured line");
        assert_eq!(p.summary.change, 0);
        assert!(p.summary.has_changes());
        assert!(p.summary.line().contains("3 to add"));
    }

    #[test]
    fn after_applying_the_plan_is_empty() {
        let t = Harness::new();
        t.apply();
        let p = t.plan();
        assert_eq!(p.summary.add, 0);
        assert_eq!(p.summary.unchanged, 3);
        assert!(!p.summary.has_changes());
    }

    #[test]
    fn editing_a_target_shows_an_update_with_a_line_diff() {
        let t = Harness::new();
        t.apply();
        std::fs::write(t.home.path().join(".zshrc"), "hand edited\n").unwrap();

        let p = t.plan();
        assert_eq!(p.summary.change, 1);
        let e = t.entry(&p, "~/.zshrc");
        assert_eq!(e.action, Action::Update);
        match &e.body {
            Body::Unified(d) => {
                assert!(d.contains("-hand edited"), "{d}");
                assert!(d.contains('+'), "{d}");
            }
            other => panic!("expected a unified diff, got {other:?}"),
        }
    }

    #[test]
    fn a_wrong_mode_alone_is_reported_as_a_permission_change() {
        let t = Harness::new();
        t.apply();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let zshenv = t.home.path().join(".zshenv");
            std::fs::set_permissions(&zshenv, std::fs::Permissions::from_mode(0o644)).unwrap();

            let p = t.plan();
            assert_eq!(p.summary.mode, 1);
            let e = t.entry(&p, "~/.zshenv");
            assert_eq!(
                e.action,
                Action::ModeChange {
                    from: 0o644,
                    to: 0o600
                }
            );
            assert!(e.action.label().contains("0600"));
        }
    }

    #[test]
    fn disabling_a_group_turns_its_files_into_destroys() {
        let t = Harness::new();
        t.apply();
        t.set_group("env", false);

        let p = t.plan();
        assert_eq!(p.summary.destroy, 1);
        let e = t.entry(&p, "~/.zshenv");
        assert_eq!(e.action, Action::Destroy { safe: true });
    }

    /// A file the user has edited must never be quietly removed.
    #[test]
    fn a_locally_modified_orphan_is_flagged_as_unsafe() {
        let t = Harness::new();
        t.apply();
        std::fs::write(t.home.path().join(".zshenv"), "mine now\n").unwrap();
        t.set_group("env", false);

        let plan = t.plan();
        let e = t.entry(&plan, "~/.zshenv");
        assert_eq!(e.action, Action::Destroy { safe: false });
        assert!(e.action.label().contains("modified locally"));
    }

    #[test]
    fn an_orphan_already_deleted_by_hand_is_not_listed() {
        let t = Harness::new();
        t.apply();
        std::fs::remove_file(t.home.path().join(".zshenv")).unwrap();
        t.set_group("env", false);
        assert_eq!(t.plan().summary.destroy, 0);
    }

    #[test]
    fn a_directory_in_the_way_is_a_conflict_not_an_overwrite() {
        let t = Harness::new();
        std::fs::create_dir_all(t.home.path().join(".zshrc")).unwrap();
        let p = t.plan();
        assert_eq!(p.summary.conflict, 1);
        let e = t.entry(&p, "~/.zshrc");
        assert!(matches!(e.action, Action::Conflict { .. }));
        assert!(
            e.contents.is_none(),
            "a conflict must not carry content to write"
        );
    }

    #[test]
    fn an_ensure_line_is_appended_to_a_file_bosun_does_not_own() {
        let t = Harness::new();
        let profile = t.home.path().join(".profile");
        std::fs::write(
            &profile,
            "# written by some installer\nexport PATH=/x:$PATH\n",
        )
        .unwrap();

        let p = t.plan();
        let e = t.entry(&p, "~/.profile");
        assert_eq!(e.action, Action::EnsureLine);

        let next = String::from_utf8(e.contents.clone().unwrap()).unwrap();
        assert!(
            next.starts_with("# written by some installer\nexport PATH=/x:$PATH\n"),
            "the installer's lines are kept verbatim: {next:?}"
        );
        assert!(
            next.ends_with("source ~/.config/zsh/bosun.zsh\n"),
            "{next:?}"
        );
    }

    #[test]
    fn an_ensure_line_already_present_is_unchanged_wherever_it_sits() {
        let t = Harness::new();
        std::fs::write(
            t.home.path().join(".profile"),
            "source ~/.config/zsh/bosun.zsh\nexport PATH=/x:$PATH\n",
        )
        .unwrap();
        let p = t.plan();
        assert_eq!(t.entry(&p, "~/.profile").action, Action::Unchanged);
    }

    #[test]
    fn a_missing_ensure_target_is_created_holding_just_the_line() {
        let t = Harness::new();
        t.apply();
        let got = std::fs::read_to_string(t.home.path().join(".profile")).unwrap();
        assert_eq!(got, "source ~/.config/zsh/bosun.zsh\n");
    }

    #[test]
    fn ensuring_twice_appends_once() {
        let t = Harness::new();
        std::fs::write(t.home.path().join(".profile"), "export PATH=/x:$PATH\n").unwrap();
        t.apply();
        let once = std::fs::read_to_string(t.home.path().join(".profile")).unwrap();
        t.apply();
        let twice = std::fs::read_to_string(t.home.path().join(".profile")).unwrap();
        assert_eq!(once, twice);
        assert_eq!(twice.matches("bosun.zsh").count(), 1);
    }

    #[test]
    fn a_symlinked_ensure_target_is_a_conflict_not_an_append() {
        let t = Harness::new();
        let real = t.home.path().join("elsewhere");
        std::fs::write(&real, "x\n").unwrap();
        std::os::unix::fs::symlink(&real, t.home.path().join(".profile")).unwrap();

        let p = t.plan();
        let e = t.entry(&p, "~/.profile");
        assert!(matches!(e.action, Action::Conflict { .. }));
        assert!(e.contents.is_none(), "a conflict must not carry content");
    }

    /// The regression that would eat a shell config. Moving `~/.zshrc` from a
    /// managed file to an ensured line leaves it recorded in state but no
    /// longer in `files:`, which is the exact shape of an orphan — and an
    /// unmodified orphan is deleted on a plain apply.
    #[test]
    fn an_ensure_target_is_never_an_orphan_even_after_it_stops_being_managed() {
        let t = Harness::new();
        t.apply();

        // Pretend .profile used to be a managed file: that is what a migration
        // from `files:` to `ensure:` leaves behind.
        let profile = t.home.path().join(".profile");
        let body = std::fs::read(&profile).unwrap();
        let mut state = t.state();
        state.record_file(&profile, &body, Some(0o644));
        state.save(&t.state_path()).unwrap();

        let p = t.plan();
        assert_eq!(
            p.summary.destroy,
            0,
            "{:?}",
            p.entries
                .iter()
                .map(|e| (&e.display, e.action.label()))
                .collect::<Vec<_>>()
        );
        assert_eq!(t.entry(&p, "~/.profile").action, Action::Unchanged);
    }

    /// bosun owns one line, so it must not claim the whole file in state.
    #[test]
    fn an_ensure_target_is_not_recorded_as_a_managed_file() {
        let t = Harness::new();
        t.apply();
        assert!(
            t.state().file(&t.home.path().join(".profile")).is_none(),
            "a partly-owned file must stay out of the managed-file state"
        );
        assert!(t.state().file(&t.home.path().join(".zshrc")).is_some());
    }

    /// Migrating a file from `files:` to `ensure:` leaves a whole-file hash
    /// behind. Left there, it would prune the file the day the ensure entry
    /// went away, so apply clears it.
    #[test]
    fn applying_forgets_a_managed_record_left_over_from_before_the_migration() {
        let t = Harness::new();
        let profile = t.home.path().join(".profile");
        std::fs::write(&profile, "export PATH=/x:$PATH\n").unwrap();

        let mut state = t.state();
        state.record_file(&profile, b"whatever bosun wrote back then", Some(0o644));
        state.save(&t.state_path()).unwrap();

        t.apply();
        assert!(
            t.state().file(&profile).is_none(),
            "the stale managed-file record must be cleared"
        );
        assert_eq!(t.plan().summary.destroy, 0);
    }

    #[test]
    fn a_target_filter_restricts_the_plan() {
        let t = Harness::new();
        let p = t.plan_with(|o| o.filter.targets = vec![PathBuf::from(".zshrc")]);
        assert_eq!(p.entries.len(), 1);
        assert_eq!(p.entries[0].display, "~/.zshrc");
    }

    #[test]
    fn hooks_appear_in_the_plan_and_can_be_skipped() {
        let t = Harness::new();
        assert_eq!(t.plan().summary.hooks, 1);
        assert_eq!(t.plan_with(|o| o.skip_hooks = true).summary.hooks, 0);
    }

    #[test]
    fn the_summary_line_reads_like_a_terraform_plan() {
        let s = Summary {
            add: 3,
            change: 2,
            destroy: 1,
            mode: 1,
            hooks: 2,
            ..Default::default()
        };
        assert_eq!(
            s.line(),
            "Plan: 3 to add, 2 to change, 1 to destroy, 1 permission change; 2 hooks to run."
        );
    }

    #[test]
    fn markers_are_distinct_per_action() {
        let markers = [
            Action::Create.marker(),
            Action::Update.marker(),
            Action::Destroy { safe: true }.marker(),
            Action::ModeChange { from: 0, to: 0 }.marker(),
            Action::Unchanged.marker(),
        ];
        let unique: std::collections::BTreeSet<_> = markers.iter().collect();
        assert_eq!(unique.len(), markers.len());
    }
}
