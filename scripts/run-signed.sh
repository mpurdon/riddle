#!/bin/sh
# Cargo runner for macOS: sign the app binary with a stable identity before
# running it, then exec it. Every rebuild of an unsigned (ad-hoc) binary is a
# new code identity to the keychain, so "Always Allow" on the Gmail token
# never stuck and each `tauri dev` asked again. With a real identity the ACL
# names the certificate, and one "Always Allow" lasts. Test binaries and
# anything that is not a Riddle app binary run untouched.
#
# The identity comes from RIDDLE_CODESIGN_IDENTITY, else the first
# "Developer ID Application" or "Apple Development" identity in the keychain;
# with neither, the binary runs as cargo built it.
set -e
bin="$1"
case "$(basename "$bin")" in
  riddle|riddle-desktop)
    identity="${RIDDLE_CODESIGN_IDENTITY:-}"
    if [ -z "$identity" ]; then
      identity="$(security find-identity -v -p codesigning 2>/dev/null \
        | grep -E 'Developer ID Application|Apple Development' | head -1 \
        | sed -E 's/.*"(.*)".*/\1/')"
    fi
    if [ -n "$identity" ]; then
      # Skip when the binary already carries this identity (cargo did not relink).
      if ! codesign -dv "$bin" 2>&1 | grep -q "Authority=$identity"; then
        codesign --force --sign "$identity" --timestamp=none "$bin" >/dev/null 2>&1 \
          || echo "run-signed: signing failed; running unsigned" >&2
      fi
    fi
    ;;
esac
exec "$@"
