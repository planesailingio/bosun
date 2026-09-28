#!/bin/sh
# bootstrap.sh — cold-start a fresh Mac. Dependency-free POSIX sh.
#
# Usage on a new machine:
#   sh -c "$(curl -fsLS https://raw.githubusercontent.com/planesailingio/bosun/main/bootstrap.sh)"
#
# Order: Xcode CLT (git) -> Homebrew -> bosun + hats -> hats init -> bosun init.
# Every step is idempotent, so re-running on a configured machine is a no-op.
#
# `hats init` runs first: it asks who you are and which hats (profiles) you
# wear, and scaffolds the identity-shaped files (~/.gitconfig, ~/.ssh/config).
# `bosun init` then clones this repo into ~/.bosun/repo and asks which groups
# of files to manage. Nothing is written to your home directory until you run
# `bosun apply`, and `bosun plan` shows exactly what that would change first.
set -eu

TAP="${BOSUN_TAP:-planesailingio/tools}"
log() { printf '\033[1;34m==>\033[0m %s\n' "$1"; }
warn() { printf '\033[1;33m==>\033[0m %s\n' "$1"; }

# 1. Xcode Command Line Tools (provides git, clang, make) ─────────────────────
if ! xcode-select -p >/dev/null 2>&1; then
  log "Installing Xcode Command Line Tools — accept the GUI prompt."
  xcode-select --install || true
  # Wait for the install to finish before continuing.
  until xcode-select -p >/dev/null 2>&1; do sleep 15; done
else
  log "Xcode Command Line Tools present."
fi

# 2. Homebrew ─────────────────────────────────────────────────────────────────
if ! command -v brew >/dev/null 2>&1; then
  log "Installing Homebrew."
  NONINTERACTIVE=1 /bin/bash -c \
    "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
else
  log "Homebrew present."
fi

# Put brew on PATH for this script (arch-aware: Apple Silicon vs Intel).
if [ -x /opt/homebrew/bin/brew ]; then
  eval "$(/opt/homebrew/bin/brew shellenv)"
elif [ -x /usr/local/bin/brew ]; then
  eval "$(/usr/local/bin/brew shellenv)"
else
  warn "brew not found on PATH after install — aborting."; exit 1
fi

# 3. bosun and hats ───────────────────────────────────────────────────────────
for tool in bosun hats; do
  if ! command -v "${tool}" >/dev/null 2>&1; then
    log "Installing ${tool} from ${TAP}."
    brew install "${TAP}/${tool}"
  else
    log "${tool} present ($(${tool} --version))."
  fi
done

# 4. Set up ───────────────────────────────────────────────────────────────────
# hats first, so the identity and the base skeletons exist before anything else.
log "Running hats init."
hats init

log "Running bosun init."
bosun init

cat <<'EOF'

Next:
  bosun plan          see what would change in your home directory
  bosun apply         write the files and run the setup hooks
  hats secrets fetch  pull tokens down, if you configured a secrets provider
  hats hat sync       create the per-hat files and VS Code profiles

Then open a new terminal (set its font to a Nerd Font) and run `hat` to
switch client context.
EOF
