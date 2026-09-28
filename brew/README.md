# Package bundles

Named sets of Homebrew packages, composed by `bosun brew install <bundle>`
(and by the `brew-bundle` hook during `bosun apply`).

| Bundle    | Composition                     | For                                         |
|-----------|---------------------------------|---------------------------------------------|
| `core`    | core                            | Any machine: shell, git, wrangling, comms   |
| `devops`  | core + devops                   | Clusters, cloud CLIs, IaC, containers       |
| `pentest` | core + pentest                  | Offensive security and recon                |
| `dev`     | core + dev                      | Languages, service clients, release tooling |
| `full`    | core + devops + pentest + dev   | Everything (the default)                    |

Every bundle leads with `core`, so no machine gets a role set without the base
set. The composition lives in `BrewBundle::files()` in `cli/src/cli.rs`; the
`brew-bundle` hook receives it as `BOSUN_BREW_BUNDLE_FILES` so the two cannot
drift. Choose a bundle for the hook with `BOSUN_BREW_BUNDLE=<name>`.

The files stay flat and declarative — no Ruby include tricks. A new bundle
needs three edits: the `<name>.Brewfile` here, `BrewBundle` in
`cli/src/cli.rs`, and the `onchange` inputs of the `brew-bundle` hook in
`bosun.yaml`.

On Linux (the dev container), casks and a few macOS-only formulae are filtered
out by the hook; the macOS bundles stay the single source of truth.
