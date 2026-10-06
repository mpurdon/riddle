#!/usr/bin/env bash
# Commit with a build-cost trailer.
#
#   scripts/commit.sh "<subject>" [<body-file>] [-- <path>...]
#
# CO_AUTHOR names the model in the Co-Authored-By trailer (default Claude Opus 5).
#
# Stages the given paths (or everything when none are given), computes wall time and token usage since the previous
# commit (or since the build started), appends a `Build-Stats:` trailer, and
# appends the same row to docs/BUILD_LEDGER.md so the ledger and the git log
# agree. The window marker lives in .build/ (untracked).
set -euo pipefail
cd "$(dirname "$0")/.."
SUBJECT="$1"; shift
BODY_FILE=""
if [ "${1:-}" != "--" ] && [ -n "${1:-}" ]; then BODY_FILE="$1"; shift; fi
if [ "${1:-}" = "--" ]; then shift; fi
PATHS=("$@")
MARK=.build/last-commit-at
mkdir -p .build
SINCE=$(cat "$MARK" 2>/dev/null || cat .build/build-started-at)
NOW=$(date -u +%Y-%m-%dT%H:%M:%SZ)
STATS=$(python3 scripts/usage.py --since "$SINCE" --until "$NOW" --trailer)
JSON=$(python3 scripts/usage.py --since "$SINCE" --until "$NOW" --json)

if [ ${#PATHS[@]} -gt 0 ]; then git add -- "${PATHS[@]}"; else git add -A; fi
{
  echo "$SUBJECT"
  echo
  if [ -n "$BODY_FILE" ]; then cat "$BODY_FILE"; echo; fi
  echo "$STATS"
  echo "Build-Window: $SINCE -> $NOW"
  echo
  echo "Co-Authored-By: ${CO_AUTHOR:-Claude Opus 5} <noreply@anthropic.com>"
} > .build/msg
git commit -q -F .build/msg
echo "$NOW" > "$MARK"
# The row cannot carry its own commit hash (amending would change it), so it
# carries a sequence number; the closing summary maps rows to hashes by subject.
SEQ=$(git rev-list --count HEAD)

# Ledger row
MIN=$(echo "$JSON" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(round(d["minutes"]))')
OUT=$(echo "$JSON" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d["total"]["output"])')
INP=$(echo "$JSON" | python3 -c 'import sys,json; d=json.load(sys.stdin); t=d["total"]; print(t["input"]+t["cache_create"]+t["cache_read"])')
TURNS=$(echo "$JSON" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d["total"]["messages"])')
AG=$(echo "$JSON" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(len([k for k in d["sources"] if k!="main"]))')
printf '| %s | %s | %s | %s | %s | %s | %s |\n' "$SEQ" "$SUBJECT" "$MIN" "$OUT" "$INP" "$TURNS" "$AG" >> docs/BUILD_LEDGER.md
git add docs/BUILD_LEDGER.md && git commit -q --amend --no-edit
echo "committed $(git rev-parse --short HEAD): $SUBJECT"
echo "$STATS"
