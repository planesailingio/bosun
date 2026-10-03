#!/bin/sh
# One-shot: split a bosun-managed ~/.zshrc into the managed fragment plus a
# thin ~/.zshrc that the user and their installers own.
#
# Up to this release bosun wrote the whole of ~/.zshrc, so anything an
# installer appended (oMLX, Wine, nvm, conda) was lost on every converge. The
# config now lives in ~/.config/zsh/bosun.zsh and bosun guarantees one `source`
# line in ~/.zshrc via `ensure:`. By the time this runs, that line has already
# been appended to the old fat file — so all that is left is to drop the part
# bosun used to own and keep the part it never did.
#
# Everything below the old template's final line is an append by something
# else, so that is the boundary. A file that does not carry the old banner is
# already thin (or was never bosun's), and is left alone.
set -eu

ZSHRC="${HOME}/.zshrc"
BANNER='# ~/.zshrc — interactive shell config. Managed by'
TAIL_MARKER='[ -f ~/.claude/scripts/hunt.sh ] && source ~/.claude/scripts/hunt.sh'
SOURCE_LINE='[ -r ~/.config/zsh/bosun.zsh ] && source ~/.config/zsh/bosun.zsh   # bosun'

[ -f "${ZSHRC}" ] || exit 0

if ! head -n 1 "${ZSHRC}" | grep -q "${BANNER}"; then
  # Already split, or never bosun's to begin with.
  exit 0
fi

BACKUP_DIR="${BOSUN_HOME}/backups/zshrc-split-$(date -u +%Y%m%dT%H%M%SZ)"
mkdir -p "${BACKUP_DIR}"
cp "${ZSHRC}" "${BACKUP_DIR}/.zshrc"

TMP="${ZSHRC}.bosun-tmp"
printf '%s\n' "${SOURCE_LINE}" > "${TMP}"

# Lines after the old template's tail marker: appended by someone else, so they
# are kept. The marker itself belongs to the template and moves to the fragment.
KEPT=0
if grep -qF "${TAIL_MARKER}" "${ZSHRC}"; then
  EXTRA="$(awk -v marker="${TAIL_MARKER}" '
    found { print }
    index($0, marker) { found = 1 }
  ' "${ZSHRC}" | grep -vxF "${SOURCE_LINE}" || true)"
  if [ -n "${EXTRA}" ]; then
    printf '\n' >> "${TMP}"
    printf '%s\n' "${EXTRA}" >> "${TMP}"
    KEPT=$(printf '%s\n' "${EXTRA}" | grep -c '' || true)
  fi
fi

mv "${TMP}" "${ZSHRC}"

echo "==> ~/.zshrc is now yours: one sourced line plus ${KEPT} line(s) kept from installers."
echo "    The config moved to ~/.config/zsh/bosun.zsh."
echo "    The previous file is at ${BACKUP_DIR}/.zshrc"
