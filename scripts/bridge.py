#!/usr/bin/env python3
"""Drive a running Riddle (dev build or `riddle bridge`) from the shell.

    scripts/bridge.py summary
    scripts/bridge.py list_finds '{"status":"open"}'
    scripts/bridge.py invoke move_to_spoil '{"digId":"...","messageIds":["..."]}' [--yes]
    scripts/bridge.py events            # tail the SSE stream

Reads port and token from the dev data directory's bridge.json (or
RIDDLE_BRIDGE_FILE). Confirmation: `invoke --yes` re-invokes with the
pending id it is handed, which is what a user clicking Confirm does.
"""
import json
import os
import sys
import urllib.request

DEFAULT = os.path.expanduser("~/Library/Application Support/dev.codenaked.riddle.dev/data/bridge.json")


def conn():
    path = os.environ.get("RIDDLE_BRIDGE_FILE", DEFAULT)
    with open(path) as f:
        d = json.load(f)
    return d["port"], d["token"]


def call(command, args):
    port, token = conn()
    req = urllib.request.Request(
        f"http://127.0.0.1:{port}/invoke",
        data=json.dumps({"command": command, "args": args}).encode(),
        headers={"Authorization": f"Bearer {token}", "Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=120) as r:
        return json.load(r)


def main():
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(2)
    cmd = sys.argv[1]
    if cmd == "events":
        port, token = conn()
        with urllib.request.urlopen(f"http://127.0.0.1:{port}/events?token={token}") as r:
            for line in r:
                line = line.decode().rstrip()
                if line.startswith("data:"):
                    print(line[5:].strip(), flush=True)
        return
    yes = "--yes" in sys.argv
    argv = [a for a in sys.argv[2:] if a != "--yes"]
    if cmd == "invoke":
        action = argv[0]
        args = json.loads(argv[1]) if len(argv) > 1 else {}
        dig = args.pop("digId", None) if isinstance(args, dict) and "digId" in args and action in ("refresh_now",) else None
        inv = {"action": action, "args": args, "principal": {"kind": "user"}, "digId": dig, "idempotencyKey": None, "confirmed": yes, "pendingId": None}
        out = call("invoke_action", {"invocation": inv})
        if not out["ok"] and out["error"]["kind"] == "needs_confirmation" and yes:
            pending = out["error"]["detail"]
            print(f"# confirming: {pending['summary']}", file=sys.stderr)
            inv["pendingId"] = pending["id"]
            out = call("invoke_action", {"invocation": inv})
    else:
        args = json.loads(argv[0]) if argv else {}
        out = call(cmd, args)
    print(json.dumps(out, indent=2))
    sys.exit(0 if out.get("ok") else 1)


if __name__ == "__main__":
    main()
