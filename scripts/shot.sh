#!/usr/bin/env bash
# Screenshot the Riddle window: scripts/shot.sh <out.png> [process-name]
# Uses the window's on-screen bounds from System Events (needs Accessibility
# permission for the terminal) and captures that region.
set -euo pipefail
OUT="$1"; PROC="${2:-riddle-desktop}"
read -r X Y W H < <(osascript -e "tell application \"System Events\" to tell process \"$PROC\" to get {position, size} of window 1" | tr -d ',' )
screencapture -x -R "${X},${Y},${W},${H}" "$OUT"
echo "captured ${W}x${H} at ${X},${Y} -> $OUT"
