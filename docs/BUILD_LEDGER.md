# Build ledger

One row per commit. Time is wall-clock minutes between commits; tokens are read
from the session transcript by `scripts/usage.py` (output tokens, and total
input including cache writes and reads); turns are assistant messages; agents
is the number of subagents that contributed in the window. A summary is
appended at the end of the build.

Caveat: the session transcript records subagent output tokens as a
streaming placeholder, so `scripts/usage.py` estimates subagent output from
content length (chars / 3.5) and says so in the trailer. Rows before a55db67
used the raw placeholder for the five parallel tracks and undercount their
output; the closing summary recomputes every window from the transcript with
the estimate applied.

| # | subject | min | output tok | input tok | turns | agents |
|---|---|---|---|---|---|---|
| 1 | chore: repository skeleton, build ledger and agent rules | 12 | 22885 | 3298128 | 34 | 0 |
| 2 | feat(core): domain model, typed errors, store and observability foundation | 19 | 104390 | 4908615 | 24 | 0 |
| 3 | feat(desktop): Tauri shell, instrumented IPC, developer commands, Action runtime | 12 | 34420 | 6202961 | 22 | 0 |
| 4 | docs: build plan, IPC command spec, Conversation types and Director events | 4 | 13582 | 920256 | 3 | 0 |
| 5 | chore: CI and release workflows, version check, contributing, security, changelog, troubleshooting | 9 | 37737 | 17428112 | 153 | 5 |
| 6 | feat(app): riddle-app headless core with read services, event bridge, read commands and invoke_action | 6 | 18165 | 11199671 | 60 | 5 |
| 7 | feat(cli,mcp): riddle CLI twin and MCP stdio server over the shared core | 6 | 15284 | 8769678 | 36 | 5 |
| 8 | chore(ledger): estimate subagent output tokens from content; document the caveat | 1 | 5214 | 1925402 | 6 | 1 |
| 9 | feat(app): command router and loopback dev bridge (HTTP + SSE) for automation; riddle bridge; tray, close-to-hide, focus refresh | 12 | 122946 | 15869692 | 54 | 5 |
| 10 | feat(ai): LLM boundary on Bedrock, escalation refiner, Director tool loop, Expeditions, privacy | 13 | 174353 | 27179661 | 81 | 5 |
| 11 | chore(build): line-tables debuginfo and none for dependencies (22 GB target → 1.2 GB); DirectorReads over app read services | 8 | 66242 | 13648894 | 35 | 4 |
| 12 | feat(app): attach the AI boundary — Bedrock LLM, escalation refiner, Director, Expeditions; usage rows to the index store; director_send | 3 | 17589 | 9107088 | 22 | 4 |
| 13 | feat(mail): provider trait, fixture mailbox, Gmail client and OAuth, sync engine, projection outbox, people, context, search | 4 | 22132 | 8699208 | 22 | 4 |
| 14 | feat(sift): Signal registry and Sets, Jev judge with SSM key, Mesh DSL and simulation, the Sift pipeline, Finds and Temporal Sift | 4 | 30858 | 9987017 | 25 | 4 |
| 15 | feat(app): domain assembly, judge attachment, the processing engine, end-to-end and live tests | 19 | 72263 | 58337102 | 120 | 2 |
| 16 | feat(ops): settings with compatibility tests, durable job queue and scheduler, Activity ledger and the undo engine | 1 | 1356 | 2289398 | 4 | 1 |
| 17 | feat(app): settings, job runner and undo wired; boot sequence; headless serve; desktop boots the core | 10 | 33215 | 21280270 | 36 | 1 |
| 18 | feat(ui): the React client — every product and Developer surface against runtime-faithful mocks | 0 | 276 | 586095 | 1 | 0 |
| 19 | feat: bridge transport in the client, Playwright against the real core, contract reconciliation, Director on Sonnet 4.5 | 13 | 24935 | 18341000 | 30 | 0 |
| 20 | style: rustfmt across the workspace; compat test expects the Sonnet 4.5 default | 4 | 1651 | 3211093 | 5 | 0 |
| 21 | feat: add_fixture_dig honours privacy; Mesh simulation preview verified against real history; README status and build plan updated | 6 | 9049 | 6511520 | 10 | 0 |
| 22 | feat: Sift Activity with provenance and Undo All; export/import Meshes, unsubscribe, compare_signal_versions; non-incremental builds | 12 | 24900 | 17635692 | 26 | 0 |

## Summary

The build ran from 01:05 to about 04:15 local time on 2026-09-22.

Build window 2026-09-22T04:55:00Z → 2026-09-22T07:49:53Z (2.9 h wall). First commit 2026-09-22 01:06:47 -0400, last 2026-09-22 03:49:47 -0400. 22 commits, 30 crates, 50,723 lines of Rust, 14,028 lines of TypeScript.

### Coordinator session (from the transcript)

| metric | value |
|---|---|
| assistant turns | 288 |
| tool calls | 286 |
| output tokens | 418,597 |
| fresh input tokens | 580 |
| cache writes | 666,696 |
| cache reads | 122,332,023 |

### Parallel tracks (harness-reported totals; transcript output estimated from content)

| track | wall min | tool uses | harness tokens | transcript output (est.) | cache writes | cache reads |
|---|---|---|---|---|---|---|
| A mail | 59 | 81 | 426,101 | 136,720 | 404,991 | 19,166,181 |
| B sift | 56 | 88 | 425,207 | 120,281 | 359,823 | 20,079,289 |
| C ai | 42 | 82 | 381,198 | 87,908 | 348,515 | 17,400,320 |
| D ops | 65 | 85 | 349,246 | 89,259 | 835,604 | 16,554,036 |
| E ui | 85 | 172 | 626,489 | 230,651 | 1,193,484 | 65,932,267 |

Tracks ran concurrently for about 1.5 h of wall time; their summed duration is 5.1 h. Harness-reported track tokens: 2,208,241.

### Everything together

Output tokens 1,083,416 (of which 650,721 estimated for subagents), fresh input 1,606, cache writes 3,809,113, cache reads 261,464,116, 801 assistant turns, 794 tool calls.

### Morning notes

- Tuning, not building, is the next step: on the fixture Dig the escalation rate is about 50% and the Unresolved rate about 38% against the §65 targets of ≤10% and ≤5%. The Spoil margin (0.15 above threshold), the GitHub Mesh's `ELSE → Escalate` route, and calibration from a labelled sample are the levers; the Shadow Period is designed to gather exactly that evidence.
- Escalation sends all 27 Signal definitions per call (~8k tokens); trimming to the Signals the Meshes in scope actually read would cut that cost several-fold.
- The deferred list in `docs/BUILD_PLAN.md` needs the Gmail OAuth client id before any real mailbox can be connected.

### External spend (from Riddle's own usage ledger and the live test output)

- Jev (TypeSafe): about 1,000 calls across the live tests, `riddle serve` and the dev instance, roughly $0.02 in total at $0.042 per million input tokens.
- Bedrock: Haiku 4.5 escalations at about $0.009 each (roughly 110 across all runs, about $1.0), plus two Director turns on Sonnet 4.5 (about $0.12) and Track C's live probes (under $0.01). Total external spend for the night is about $1.2.
- The escalation tail dominates, as ARCHITECTURE.md §18 predicted: the judge cost per message is two orders of magnitude below one LLM refinement.

| 27 | feat: rule packs, importance, sign-in codes, failed payments, Hold/Ask/Allow mailbox writes | 19961 | 3937814 | 1489649746 | 4134 | 56 |
| 28 | feat: Community and Creators, ahead of Payments and Social | 92 | 18892 | 11793370 | 23 | 0 |
