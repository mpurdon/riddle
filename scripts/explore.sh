#!/bin/sh
# Serve the UX study site in explore/ on localhost with whatever static
# server is installed, falling back to Python's built-in one.
#   scripts/explore.sh [port]      default port 8181
set -e
root="$(cd "$(dirname "$0")/.." && pwd)/explore"
port="${1:-8181}"
url="http://127.0.0.1:$port"

if command -v static-web-server >/dev/null 2>&1; then
  echo "static-web-server: $url  (root $root)"
  exec static-web-server --root "$root" --port "$port" --host 127.0.0.1
elif command -v caddy >/dev/null 2>&1; then
  echo "caddy: $url  (root $root)"
  exec caddy file-server --root "$root" --listen "127.0.0.1:$port"
elif command -v miniserve >/dev/null 2>&1; then
  echo "miniserve: $url  (root $root)"
  exec miniserve "$root" --index index.html --interface 127.0.0.1 --port "$port"
else
  echo "python http.server: $url  (root $root)"
  echo "For a proper server: brew install static-web-server   (or caddy, or miniserve)"
  exec python3 -m http.server "$port" --bind 127.0.0.1 --directory "$root"
fi
