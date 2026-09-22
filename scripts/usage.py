#!/usr/bin/env python3
"""Token and time accounting for the build ledger.

Reads the Claude Code session transcript (and every subagent transcript
beneath it), de-duplicates streamed assistant messages by message id, and sums
token usage inside a time window. Used by `scripts/commit.sh` to stamp each
commit with what it cost to produce, and by the ledger summary at the end.

    usage.py --since 2026-09-22T05:00:00Z [--until ...] [--json]

Token classes follow the API's usage object: fresh input, cache creation,
cache read, and output. Subagents are attributed by agent id; the main
session is "main". Nothing here is an estimate: every number is read from
the transcript the API wrote.
"""
import argparse
import glob
import json
import os
from collections import defaultdict
from datetime import datetime, timezone

PROJECT_DIR = os.path.expanduser("~/.claude/projects/-Users-mp-Projects-riddle")


def parse_ts(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00"))


def iter_assistant(path):
    with open(path, encoding="utf-8") as f:
        for line in f:
            try:
                d = json.loads(line)
            except json.JSONDecodeError:
                continue
            if d.get("type") != "assistant":
                continue
            m = d.get("message") or {}
            u = m.get("usage")
            if not u or not d.get("timestamp"):
                continue
            yield d["timestamp"], m.get("id"), m.get("model"), u, m.get("content") or []


def collect(since, until):
    """Return {source: stats} for messages whose timestamp falls in the window."""
    per_source = {}
    files = [(f, "main") for f in glob.glob(os.path.join(PROJECT_DIR, "*.jsonl"))]
    for f in glob.glob(os.path.join(PROJECT_DIR, "*", "subagents", "agent-*.jsonl")):
        files.append((f, os.path.basename(f)[len("agent-"):-len(".jsonl")]))

    for path, source in files:
        best = {}  # message id -> (ts, model, usage, tool_calls)
        for ts, mid, model, u, content in iter_assistant(path):
            t = parse_ts(ts)
            if t < since or (until and t > until):
                continue
            tools = sum(1 for c in content if isinstance(c, dict) and c.get("type") == "tool_use")
            prev = best.get(mid)
            # Streaming writes the same message several times; keep the fullest.
            if prev is None or u.get("output_tokens", 0) >= prev[2].get("output_tokens", 0):
                best[mid] = (t, model, u, max(tools, prev[3] if prev else 0))
        if not best:
            continue
        s = per_source.setdefault(source, defaultdict(int))
        s["messages"] += len(best)
        models = set()
        first = last = None
        for t, model, u, tools in best.values():
            s["input"] += u.get("input_tokens", 0)
            s["cache_create"] += u.get("cache_creation_input_tokens", 0)
            s["cache_read"] += u.get("cache_read_input_tokens", 0)
            s["output"] += u.get("output_tokens", 0)
            s["tool_calls"] += tools
            models.add(model or "?")
            first = t if first is None or t < first else first
            last = t if last is None or t > last else last
        s["_models"] = sorted(models)
        s["_first"] = first.isoformat() if first else None
        s["_last"] = last.isoformat() if last else None
    return per_source


def fmt_k(n):
    return f"{n/1000:.1f}k" if n >= 1000 else str(n)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--since", required=True)
    ap.add_argument("--until")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--trailer", action="store_true", help="one-line summary for a commit trailer")
    a = ap.parse_args()
    since = parse_ts(a.since)
    until = parse_ts(a.until) if a.until else None
    now = until or datetime.now(timezone.utc)

    per = collect(since, until)
    tot = defaultdict(int)
    for s in per.values():
        for k in ("messages", "input", "cache_create", "cache_read", "output", "tool_calls"):
            tot[k] += s[k]
    minutes = (now - since).total_seconds() / 60
    agents = {k: v for k, v in per.items() if k != "main"}

    if a.json:
        print(json.dumps({"since": since.isoformat(), "until": now.isoformat(), "minutes": round(minutes, 1),
                          "total": dict(tot), "sources": {k: dict(v) for k, v in per.items()}}, indent=2, default=str))
        return
    if a.trailer:
        main_s = per.get("main", defaultdict(int))
        print(f"Build-Stats: {minutes:.0f} min wall; "
              f"out {fmt_k(tot['output'])}, in {fmt_k(tot['input'])}, "
              f"cache-write {fmt_k(tot['cache_create'])}, cache-read {fmt_k(tot['cache_read'])}; "
              f"{tot['messages']} turns, {tot['tool_calls']} tool calls; "
              f"main out {fmt_k(main_s['output'])}, {len(agents)} agents out {fmt_k(sum(v['output'] for v in agents.values()))}")
        return

    print(f"Window: {since.isoformat()} -> {now.isoformat()} ({minutes:.1f} min)")
    print(f"{'source':<20}{'turns':>7}{'tools':>7}{'output':>10}{'input':>9}{'cache-w':>10}{'cache-r':>11}")
    for name, s in sorted(per.items(), key=lambda kv: (kv[0] != "main", kv[0])):
        print(f"{name:<20}{s['messages']:>7}{s['tool_calls']:>7}{s['output']:>10}{s['input']:>9}{s['cache_create']:>10}{s['cache_read']:>11}")
    print(f"{'TOTAL':<20}{tot['messages']:>7}{tot['tool_calls']:>7}{tot['output']:>10}{tot['input']:>9}{tot['cache_create']:>10}{tot['cache_read']:>11}")


if __name__ == "__main__":
    main()
