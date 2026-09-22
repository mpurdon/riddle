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
