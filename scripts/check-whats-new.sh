#!/usr/bin/env bash
# check-whats-new.sh <version> — fail unless WHATS_NEW.md documents <version>.
#
# WHATS_NEW.md is what the in-app "What's new" dialog shows, so a release with
# no entry ships a dialog that says nothing. The release gate and the /release
# skill both call this.
set -euo pipefail

version="${1:-}"
if [ -z "$version" ]; then
  echo "usage: scripts/check-whats-new.sh <version>" >&2
  exit 2
fi

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
file="${root}/WHATS_NEW.md"

if [ ! -f "$file" ]; then
  echo "error: ${file} is missing. Every release needs a user-facing entry; see the /release skill." >&2
  exit 1
fi

# `## <version>` on its own, or with a ` — <date>` suffix. Version dots are
# escaped so 0.1.3 doesn't match 0x1y3.
escaped="${version//./\\.}"
if grep -qE "^##[[:space:]]+v?${escaped}([[:space:]]|$)" "$file"; then
  echo "WHATS_NEW.md documents ${version}."
  exit 0
fi

cat >&2 <<MSG
error: WHATS_NEW.md has no entry for ${version}.

Add one at the top of WHATS_NEW.md, newest first:

  ## ${version} — $(date +%Y-%m-%d)

  ### Editor
  - **Key part**: what changed, in the user's words.

Only user-facing changes belong there. Run \`/release\` to have this written
for you, or re-run this check after editing the file.
MSG
exit 1
