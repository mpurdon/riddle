#!/usr/bin/env python3
"""Append the closing summary to docs/BUILD_LEDGER.md from the transcript,
the git log, and the per-track figures the harness reported."""
import json
import subprocess
import sys
from datetime import datetime, timezone

sys.path.insert(0, "scripts")
import usage  # noqa: E402

START = "2026-09-22T04:55:00Z"
# Reported by the harness in each track's completion notification.
TRACKS = [
    ("A mail", "a3b419725b649d13f", 426_101, 81, 58.9),
    ("B sift", "abb6bd7460855005d", 425_207, 88, 56.1),
    ("C ai", "a7307c696f522820c", 381_198, 82, 41.6),
    ("D ops", "afe7c8f7a724782a3", 349_246, 85, 65.1),
    ("E ui", "aa17cb4292b5d2496", 626_489, 172, 84.7),
]

since = usage.parse_ts(START)
now = datetime.now(timezone.utc)
per = usage.collect(since, None)
tot = {k: 0 for k in ("messages", "input", "cache_create", "cache_read", "output", "output_estimated", "tool_calls")}
for s in per.values():
    for k in tot:
        tot[k] += s.get(k, 0)
main = per.get("main", {})
minutes = (now - since).total_seconds() / 60

def sh(cmd):
    return subprocess.run(cmd, shell=True, capture_output=True, text=True).stdout.strip()

commits = int(sh("git rev-list --count HEAD"))
first = sh("git log --reverse --format='%ci' | head -1")
last = sh("git log -1 --format='%ci'")
rust = int(sh("find crates apps/desktop/src-tauri/src -name '*.rs' | xargs cat | wc -l"))
ts = int(sh("find apps/desktop/src -name '*.ts' -o -name '*.tsx' | grep -v bindings | xargs cat | wc -l"))
crates = int(sh("ls crates | wc -l"))

def k(n):
    return f"{n/1000:.0f}k" if n >= 1000 else str(n)

out = []
out.append("\n## Summary\n")
out.append(f"Build window {START} → {now.strftime('%Y-%m-%dT%H:%M:%SZ')} ({minutes/60:.1f} h wall). First commit {first}, last {last}. {commits} commits, {crates} crates, {rust:,} lines of Rust, {ts:,} lines of TypeScript.\n")
out.append("### Coordinator session (from the transcript)\n")
out.append("| metric | value |\n|---|---|")
out.append(f"| assistant turns | {main.get('messages', 0)} |")
out.append(f"| tool calls | {main.get('tool_calls', 0)} |")
out.append(f"| output tokens | {main.get('output', 0):,} |")
out.append(f"| fresh input tokens | {main.get('input', 0):,} |")
out.append(f"| cache writes | {main.get('cache_create', 0):,} |")
out.append(f"| cache reads | {main.get('cache_read', 0):,} |")
out.append("\n### Parallel tracks (harness-reported totals; transcript output estimated from content)\n")
out.append("| track | wall min | tool uses | harness tokens | transcript output (est.) | cache writes | cache reads |\n|---|---|---|---|---|---|---|")
for name, aid, toks, tools, mins in TRACKS:
    s = per.get(aid, {})
    out.append(f"| {name} | {mins:.0f} | {tools} | {toks:,} | {s.get('output', 0):,} | {s.get('cache_create', 0):,} | {s.get('cache_read', 0):,} |")
harness_total = sum(t[2] for t in TRACKS)
out.append(f"\nTracks ran concurrently for about 1.5 h of wall time; their summed duration is {sum(t[4] for t in TRACKS)/60:.1f} h. Harness-reported track tokens: {harness_total:,}.\n")
out.append("### Everything together\n")
out.append(f"Output tokens {tot['output']:,} (of which {tot['output_estimated']:,} estimated for subagents), fresh input {tot['input']:,}, cache writes {tot['cache_create']:,}, cache reads {tot['cache_read']:,}, {tot['messages']} assistant turns, {tot['tool_calls']} tool calls.\n")
out.append("### External spend (from Riddle's own usage ledger and the live test output)\n")
out.append("- Jev (TypeSafe): about 1,000 calls across the live tests, `riddle serve` and the dev instance, roughly $0.02 in total at $0.042 per million input tokens.")
out.append("- Bedrock: Haiku 4.5 escalations at about $0.009 each (roughly 110 across all runs, about $1.0), plus two Director turns on Sonnet 4.5 (about $0.12) and Track C's live probes (under $0.01). Total external spend for the night is about $1.2.")
out.append("- The escalation tail dominates, as ARCHITECTURE.md §18 predicted: the judge cost per message is two orders of magnitude below one LLM refinement.\n")
open("docs/BUILD_LEDGER.md", "a").write("\n".join(out) + "\n")
print("\n".join(out))
