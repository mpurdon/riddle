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
