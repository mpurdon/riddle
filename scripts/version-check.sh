#!/usr/bin/env bash
# One source of truth for the version: the workspace Cargo.toml. Everything
# else must match, and an optional argument (a tag) must match too.
set -euo pipefail
cd "$(dirname "$0")/.."
WS=$(grep -m1 '^version = ' Cargo.toml | sed 's/version = "\(.*\)"/\1/')
TAURI=$(jq -r .version apps/desktop/src-tauri/tauri.conf.json)
PKG=$(jq -r .version apps/desktop/package.json)
FAIL=0
for pair in "tauri.conf.json:$TAURI" "package.json:$PKG"; do
  name=${pair%%:*}; v=${pair#*:}
  if [ "$v" != "$WS" ]; then echo "version mismatch: $name has $v, Cargo.toml has $WS"; FAIL=1; fi
done
if [ "${1:-}" != "" ] && [ "$1" != "$WS" ]; then echo "tag $1 does not match version $WS"; FAIL=1; fi
[ $FAIL -eq 0 ] && echo "version $WS consistent"
exit $FAIL
