# Build ledger

One row per commit. Time is wall-clock minutes between commits; tokens are read
from the session transcript by `scripts/usage.py` (output tokens, and total
input including cache writes and reads); turns are assistant messages; agents
is the number of subagents that contributed in the window. A summary is
appended at the end of the build.

| commit | subject | min | output tok | input tok | turns | agents |
|---|---|---|---|---|---|---|
| c056823 | chore: repository skeleton, build ledger and agent rules | 12 | 22885 | 3298128 | 34 | 0 |
| 4a4a812 | feat(core): domain model, typed errors, store and observability foundation | 19 | 104390 | 4908615 | 24 | 0 |
| 6f640cd | feat(desktop): Tauri shell, instrumented IPC, developer commands, Action runtime | 12 | 34420 | 6202961 | 22 | 0 |
| 3c0df07 | docs: build plan, IPC command spec, Conversation types and Director events | 4 | 13582 | 920256 | 3 | 0 |
| 134d54f | chore: CI and release workflows, version check, contributing, security, changelog, troubleshooting | 9 | 37737 | 17428112 | 153 | 5 |
| 9182960 | feat(app): riddle-app headless core with read services, event bridge, read commands and invoke_action | 6 | 18165 | 11199671 | 60 | 5 |
| b39b6f9 | feat(cli,mcp): riddle CLI twin and MCP stdio server over the shared core | 6 | 15284 | 8769678 | 36 | 5 |
