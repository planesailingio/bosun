#!/bin/sh
# Installs a bundle from brew/. Re-runs whenever any bundle file or this script
# changes: the trigger inputs are declared in bosun.yaml, so no hash comment is
# needed here.
#
# The bundle is chosen with BOSUN_BREW_BUNDLE; it defaults to `full`, which is
# what a single flat Brewfile used to mean. The composition itself arrives as
# BOSUN_BREW_BUNDLE_FILES, computed by bosun from BrewBundle::files(), so it
# lives in exactly one place. The hook composes for itself rather than shelling
# out to `bosun brew`, because it must also filter for Linux.
set -eu

# Homebrew prefix and OS come from bosun as BOSUN_BREW_PREFIX / BOSUN_OS, so
# this script needs no templating: it is plain sh that runs identically
# everywhere.
BREW_PREFIX="${BOSUN_BREW_PREFIX:-/opt/homebrew}"
[ -x "${BREW_PREFIX}/bin/brew" ] && eval "$(${BREW_PREFIX}/bin/brew shellenv)"

BUNDLE="${BOSUN_BREW_BUNDLE:-full}"
SETS="${BOSUN_BREW_BUNDLE_FILES:-core devops pentest dev}"
BREW_DIR="${BOSUN_REPO}/brew"

COMPOSED="$(mktemp)"
trap 'rm -f "${COMPOSED}"' EXIT

for set in ${SETS}; do
  f="${BREW_DIR}/${set}.Brewfile"
  [ -f "${f}" ] || { echo "no bundle file at ${f}"; exit 1; }
  cat "${f}" >> "${COMPOSED}"
  printf '\n' >> "${COMPOSED}"
done

if [ "${BOSUN_OS:-}" = "darwin" ]; then
  echo "==> brew bundle (${BUNDLE}: ${SETS})"
  # --no-upgrade keeps existing installs fast; drop it to also upgrade each time.
  brew bundle install --no-upgrade --file="${COMPOSED}"
else
  # Linux (dev container): casks and a few formulae are macOS-only. Filter them
  # so `brew bundle` does not abort. The macOS bundles stay the one source of
  # truth rather than being forked per platform.
  echo "==> brew bundle on Linux (${BUNDLE}: ${SETS}) — filtering macOS-only entries"
  LINUX_BREWFILE="$(mktemp)"
  trap 'rm -f "${COMPOSED}" "${LINUX_BREWFILE}"' EXIT
  grep -vE '^[[:space:]]*(cask|mas|cask_args)\b' "${COMPOSED}" \
    | grep -vE '^[[:space:]]*brew "(dockutil|mas|reattach-to-user-namespace|trash|m-cli)"' \
    > "${LINUX_BREWFILE}"
  brew bundle install --no-upgrade --file="${LINUX_BREWFILE}" || {
    echo "!! brew bundle had failures (some Linux formulae may be unavailable) — continuing"
  }
fi
