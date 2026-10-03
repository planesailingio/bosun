#!/bin/sh
# Linux smoke test for the dotfiles, run inside the dev container.
#
# Builds bosun from the mounted source, sets it up unattended against a
# throwaway HOME, applies the files, and runs `bosun test`.
#
# The repo is mounted read-write but nothing here writes to it except cargo's
# target directory.
set -eu

WORKSPACE="${WORKSPACE:-/workspace}"
FAKE_HOME="$(mktemp -d)"
BOSUN_HOME="$(mktemp -d)"
ANSWERS="$(mktemp)"

echo "==> building bosun"
cd "${WORKSPACE}"
cargo build --release --locked -p bosun
BOSUN="${WORKSPACE}/target/release/bosun"

# A tagged clone, because bosun deliberately refuses a repo whose tag does not
# match the binary. Cloning also proves the manifest and files are committed.
echo "==> preparing a tagged clone"
SRC="$(mktemp -d)/dotfiles"
git clone --quiet "${WORKSPACE}" "${SRC}"
git -C "${SRC}" -c user.email=ci@example.com -c user.name=CI \
    tag -a "v$("${BOSUN}" --version | awk '{print $2}')" -m ci 2>/dev/null || true

cat > "${ANSWERS}" <<'ANSWERS_EOF'
answers:
  groups.shell: true
  groups.git: true
  groups.theme: true
  groups.tools: true
  groups.editor: false
ANSWERS_EOF

echo "==> bosun init"
HOME="${FAKE_HOME}" "${BOSUN}" --bosun-home "${BOSUN_HOME}" --no-color \
    --answers "${ANSWERS}" init --repo "${SRC}"

echo "==> bosun plan"
# Exit code 2 means "there are changes", which is what a fresh home should say.
set +e
HOME="${FAKE_HOME}" "${BOSUN}" --bosun-home "${BOSUN_HOME}" --no-color plan --skip-hooks >/dev/null
rc=$?
set -e
[ "${rc}" -eq 2 ] || { echo "FAIL: expected plan to report changes (exit 2), got ${rc}"; exit 1; }

echo "==> bosun apply"
HOME="${FAKE_HOME}" "${BOSUN}" --bosun-home "${BOSUN_HOME}" --no-color apply --yes --only-files

echo "==> the git style fragment is in place"
[ -f "${FAKE_HOME}/.config/git/style.gitconfig" ] \
    || { echo "FAIL: ~/.config/git/style.gitconfig was not written"; exit 1; }

echo "==> the zsh fragment is in place and ~/.zshrc only sources it"
[ -f "${FAKE_HOME}/.config/zsh/bosun.zsh" ] \
    || { echo "FAIL: ~/.config/zsh/bosun.zsh was not written"; exit 1; }
# On a fresh machine `ensure:` creates ~/.zshrc holding nothing but the line.
# Anything more means bosun has gone back to owning the whole file.
[ "$(grep -cv '^[[:space:]]*$' "${FAKE_HOME}/.zshrc")" -eq 1 ] \
    || { echo "FAIL: ~/.zshrc should hold exactly the sourced line:"; cat "${FAKE_HOME}/.zshrc"; exit 1; }
grep -q 'source ~/.config/zsh/bosun.zsh' "${FAKE_HOME}/.zshrc" \
    || { echo "FAIL: ~/.zshrc does not source the fragment"; exit 1; }

echo "==> bosun plan is now clean"
set +e
HOME="${FAKE_HOME}" "${BOSUN}" --bosun-home "${BOSUN_HOME}" --no-color plan --skip-hooks >/dev/null
rc=$?
set -e
[ "${rc}" -eq 0 ] || { echo "FAIL: expected a clean plan (exit 0), got ${rc}"; exit 1; }

echo "==> bosun lint"
HOME="${FAKE_HOME}" "${BOSUN}" --bosun-home "${BOSUN_HOME}" --no-color lint --no-external

echo "==> bosun test"
HOME="${FAKE_HOME}" "${BOSUN}" --bosun-home "${BOSUN_HOME}" --no-color test

echo "==> all checks passed"
