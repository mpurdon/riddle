# Build plan

The overnight build of Riddle from `ARCHITECTURE.md`, carried out by a build
coordinator and parallel tracks. Each commit lands at a feature point and
carries its cost in the message (`Build-Stats`) and in `BUILD_LEDGER.md`.

## Phases

```
0  Skeleton                 git, ledger, house rules                   done
1  Foundation               errors, core model, store, observability   done
2  Shell                    Tauri adapter, IPC wrapper, Action runtime done
3  Parallel tracks          A mail · B sift · C ai · D ops · E ui      in progress
4  Integration              concrete Actions, commands, wiring, jobs
5  Verification             fixture Dig end to end, Jev live, Bedrock live, UI driven
6  CLI and MCP              riddle CLI verbs, MCP read-only server
7  Ledger summary           totals, deferred list, morning notes
```

## Tracks (phase 3)

Each track owns its crates and touches nothing else. Tracks depend only on
the foundation (`riddle-errors`, `riddle-core`, `riddle-store`,
`riddle-observability`) and on `riddle-actions`' trait for any Action they
declare. Cross-track needs go through traits in `riddle-core`.

| track | crates | delivers |
|---|---|---|
| A mail | riddle-mail, riddle-gmail, riddle-sync, riddle-projection, riddle-people, riddle-context, riddle-search, riddle-embeddings, riddle-credentials | `MailProvider` trait; MIME/HTML normalization; `FixtureProvider` with a realistic synthetic mailbox and runtime injection; Gmail OAuth PKCE loopback, REST client, history sync, batchModify, quota limiter, error mapping; sync engine (initial, incremental, full resync, attribution); projection outbox processor; People aggregation; Thread State; search operators over FTS; keychain wrapper |
| B sift | riddle-signals, riddle-judge, riddle-mesh, riddle-sift, riddle-finds, riddle-scheduler | system Signal vocabulary and Sets with state builders; registry service; calibration; Jev judge with SSM key resolution, rate limit, circuit breaker, raw capture; Null and Scripted judges; Mesh DSL parser, evaluator, scope resolution, defaults, simulation; the Sift pipeline with facts, gates, selection, judge, routes, confidence policy, escalation, guards, disposition, traces, shadow; Finds with dedupe and Temporal Sift |
| C ai | riddle-ai, riddle-agent, riddle-expeditions, riddle-privacy | `Llm` trait; Bedrock via the `dev` profile; `SignalRefiner` for escalation; versioned prompt files; usage rows; circuit breaker; Director loop over Actions with untrusted marking and egress gating; semantic-compiler flow; Expedition runner with plan, checkpoints, budget, resume; redaction and privacy statement |
| D ops | riddle-settings, riddle-jobs, riddle-activity | settings with serde defaults, atomic writes, schema version, compatibility tests; job queue with priority classes, rate limiters, checkpoints, pause reasons, Task Centre snapshot, sleep detection; Activity helpers and the undo engine (non-linear, Undo All, correction retraction, provider check hook) |
| E ui | apps/desktop/src | every product surface in `docs/COMMANDS.md` against mocks: morning summary, Finds, Unresolved, Spoil review, message and thread view with sanitized HTML, Task Centre, Activity with undo, Mesh editor with simulation preview, Signal registry, Strata, Director panel, Expeditions, Settings, Developer Mode (ops pane, log tail, in-flight, inspectors), keyboard table and help sheet, empty states, confirmation tiers |

## Integration (phase 4)

The coordinator wires the tracks: concrete Actions in `riddle-actions`,
Tauri commands per `docs/COMMANDS.md`, services registered in `AppState`,
the event bridge, job runners, tray and close-to-hide, notifications.

## Deferred until the user is back

Things that need input, a secret, or a decision that is theirs:

- Gmail OAuth client id (and secret if a Web client type is used): the
  Google Cloud project, consent screen and `gmail.modify` scope. Until then
  every Dig is a `Fixture` Dig; `riddle-gmail` is built and unit-tested
  against recorded responses.
- Google Workspace test account for the dev OAuth client (§59).
- Updater signing key, Developer ID certificate, notarization credentials
  (§6). The release workflow is written but not exercised.
- Homebrew tap name.
- Which Bedrock model tiers to standardise on and the per-Dig budgets.
  Defaults: escalation on Haiku 4.5, Director on Sonnet 5, budgets small.
- Whether the Jev key should move from SSM to the keychain for daily use.
- SQLCipher at rest (the store has the seam; the key custody is a decision).
- Launch-at-login and notification click behaviour need a signed build to
  test properly.
