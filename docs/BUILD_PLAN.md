# Build plan

The overnight build of Riddle from `ARCHITECTURE.md`, carried out by a build
coordinator and parallel tracks. Each commit lands at a feature point and
carries its cost in the message (`Build-Stats`) and in `BUILD_LEDGER.md`.

## Phases

```
0  Skeleton                 git, ledger, house rules                   done
1  Foundation               errors, core model, store, observability   done
2  Shell                    Tauri adapter, IPC wrapper, Action runtime done
3  Parallel tracks          A mail · B sift · C ai · D ops · E ui      done (5 agents, ~6 h wall in parallel)
4  Integration              riddle-app: assembly, engine, boot, bridge done
5  Verification             fixture Dig end to end, Jev live, Bedrock live, UI driven headless   done
6  CLI and MCP              riddle CLI verbs, MCP stdio server         done
7  Ledger summary           totals, deferred list, morning notes       done (BUILD_LEDGER.md)
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

## Integration (phase 4), as built

`crates/riddle-app` is the headless application every client shares:

- `assemble` opens stores and the Action runtime; `domains` attaches
  providers, credentials, the projection hook, Finds, seeds and every
  domain Action; `ai` attaches the LLM, refiner, Director and Expeditions;
  `ops` attaches settings, the job queue and runner, undo, the Task Centre
  source and sleep detection; `boot` sequences them
- `engine` runs the passes (sync → Sift backlog → Finds → projection →
  Temporal Sift); job handlers call it and yield with checkpoints
- `dispatch` is the one command router; the Tauri commands, the loopback
  `bridge` (HTTP + SSE, dev builds only) and the CLI all go through it
- `riddle serve` runs the whole core headless with the bridge; the React
  client attaches with `?bridge=port:token`; Playwright drives it

What the morning launch will show: the dev data directory already holds
a 90-message Fixture Dig ("Work") sifted through Jev with Bedrock
escalation, so Finds, Unresolved, Activity and traces are populated.

## Known gaps after the first build

- Gmail is unit-tested against recorded responses only; the live OAuth
  flow, sync and projection against a real mailbox are unexercised. The
  consent URL `connect_gmail` builds was checked by hand (endpoint, scope,
  PKCE S256, loopback redirect, `access_type=offline`), but no real token
  has ever been exchanged.
- The native Tauri window was verified before the screen locked (shell,
  DEV badge, build stamp, close-to-hide); every later UI check ran
  headless through the bridge. Tray menu and Dock reopen are written but
  not exercised.
- Notifications use the plugin only; the Swift helper for macOS 26 is
  not built. Updater and Update Preview are wired to nothing until a
  signing key exists.
- Fairness between Digs on the shared judge bucket is FIFO.
- Sonnet 5 and Opus 5 are unpriced in the catalog and Sonnet 5 is not
  enabled on the dev account; the Director runs on Sonnet 4.5.
- Escalation cost dominates: ~$0.009 per escalated message on Haiku 4.5
  against ~$0.00002 per judge call. The per-Dig escalation budget exists
  but the prompt could be trimmed (it sends all 27 definitions).
- No SQLCipher yet; the store has the seam.

## Deferred until the user is back

Things that need input, a secret, or a decision that is theirs:

- Gmail OAuth client id: the Google Cloud project, consent screen and
  `gmail.modify` scope. `docs/GMAIL_SETUP.md` has the console steps and the
  Internal-vs-External decision. Until then every Dig is a `Fixture` Dig;
  `riddle-gmail` is built and unit-tested against recorded responses, and
  `connect_gmail` wires the flow to a Dig but has never run against a real
  client id.
- Google Workspace test account for the dev OAuth client (§59).
- Updater signing key, Developer ID certificate, notarization credentials
  (§6). The release workflow is written but not exercised.
- Homebrew tap name.
- Which Bedrock model tiers to standardise on and the per-Dig budgets.
  Defaults: escalation on Haiku 4.5, Director on Sonnet 4.5 (Sonnet 5 is
  not enabled on the account), budgets small.
- Whether the Jev key should move from SSM to the keychain for daily use.
- SQLCipher at rest (the store has the seam; the key custody is a decision).
- Launch-at-login and notification click behaviour need a signed build to
  test properly.
