# bosun

Machine and shell bootstrap for macOS and Linux: one binary that owns this
dotfiles repo and converges a machine onto it. Homebrew bundles, zsh, starship,
terminal themes, mise runtimes, macOS defaults and the Dock — planned like
terraform, applied atomically, and checked by hooks that declare their own
triggers.

The bosun is the crew member who keeps the ship's equipment in order. Client
context switching — identities, cloud credentials, kube contexts, per-client
ssh and git — is deliberately **not** bosun's job: that is
[hats](https://github.com/planesailingio/hats), a sibling tool. The two meet
at exactly three seams:

1. bosun's `.zshrc` evals `hats shell-init zsh` behind a `command -v` guard,
   so a machine without hats still gets a working shell.
2. bosun's starship config shows `$HATS_HAT`, the environment variable hats
   exports.
3. bosun manages `~/.config/git/style.gitconfig` (delta, aliases, LFS), which
   the hats-scaffolded `~/.gitconfig` includes; git skips the include when the
   file is missing.

Identity never enters bosun: `~/.gitconfig`, `~/.ssh/config`, `~/.terraformrc`
and `~/.tofurc` belong to hats, which scaffolds them once and then leaves them
alone.

## Quick start

Cold-start Mac (installs Xcode CLT, Homebrew, bosun and hats, then runs both
wizards):

```sh
sh -c "$(curl -fsLS https://raw.githubusercontent.com/planesailingio/bosun/main/bootstrap.sh)"
```

Machine that already has Homebrew:

```sh
brew install planesailingio/tools/bosun
bosun init      # clone the repo into ~/.bosun/repo, pick groups
bosun plan      # exactly what an apply would change, terraform style
bosun apply     # write the files, run the due hooks
```

`bosun plan` exits 2 when there is work to do and 0 when the machine is
converged, so it drops straight into scripts and CI.

## What it manages

Groups, chosen once at `bosun init`:

| Group    | Contents                                                          |
|----------|-------------------------------------------------------------------|
| `shell`  | `.zshrc`, `.zshenv`: history, completions, fzf, zoxide, atuin, tool init |
| `git`    | `~/.config/git/style.gitconfig`: delta, aliases, LFS               |
| `theme`  | Catppuccin Mocha for starship, bat, btop, k9s, lazygit             |
| `tools`  | mise global runtimes (python, node, go)                            |
| `editor` | VS Code extension list (installed by a hook)                       |

Hooks run around an apply and declare their triggers in `bosun.yaml` — `once`,
`always`, or `onchange` with explicit inputs: brew bundles, zsh setup (chsh,
bat cache, mise install), macOS defaults, the Dock, VS Code extensions.

Package bundles live under [brew/](brew/README.md): `core`, `devops`,
`pentest`, `dev`, or `full` (the default). Every bundle leads with `core`.

## Commands

| Command                      | Does                                                    |
|------------------------------|---------------------------------------------------------|
| `bosun init`                 | Clone the repo, ask the group questions, write config   |
| `bosun plan` / `diff`        | Preview changes; exit 2 when there are any              |
| `bosun apply`                | Write files, run due hooks; backups before overwrites   |
| `bosun render [--check]`     | Print or syntax-check a rendered file                   |
| `bosun brew <action> [set]`  | Install / check / cleanup / dump a package bundle       |
| `bosun hooks list\|run`      | Inspect and run hooks                                   |
| `bosun lint`                 | Manifest, templates, modes, hook scripts, shellcheck    |
| `bosun update [--check]`     | Move the clone to the tag matching this binary          |
| `bosun doctor`               | Is this machine ready, and what fixes it                |
| `bosun test [--container]`   | Smoke-test the bootstrap (zsh starts, hook guards)      |

## File layout

```
~/.bosun/
├── repo/          this repo, checked out at the tag matching the binary
├── config.yaml    machine-local wizard answers: repo URL, groups, machine values
├── state.yaml     what the last apply wrote, and hook markers
├── backups/<ts>/  pre-overwrite copies and pruned files
└── plans/         saved plans
```

Templates are minijinja (`.j2` suffix); the context is `os`, `arch`, `home`,
`user`, `hostname`, `brew_prefix`, `repo_dir`, `files_dir`, `version` and the
free-form `machine.*` map from `~/.bosun/config.yaml`. There are no secrets
and no identity in the context, by design.

## Versioning

The binary and the repo content ship from one `v*` tag. `bosun update` moves
the clone to the tag matching the binary; `bosun update --check` exits 0 in
step, 3 repo behind, 4 binary behind. After `brew upgrade bosun`, run
`bosun update`.

## Development

```sh
make build test lint      # cargo build / test / fmt-check + clippy
bosun test --container    # the Linux smoke test in .devcontainer/
make bump V=x.y.z         # bump, tag and push a release
make tap-update           # render Formula/bosun.rb and push it to the tap
```

Extracted from [planesailingio/hats](https://github.com/planesailingio/hats)
at v0.10.0, where the shared history lives.

## Licence

MIT.
