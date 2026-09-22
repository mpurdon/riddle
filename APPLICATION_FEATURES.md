# Desktop Application Features Playbook

Distilled on 2026-09-22 from the desktop applications under `~/Projects`. Every pattern names the project and file where it was actually done, so it can be lifted rather than reinvented. Section 16 is the checklist for a new app; sections 1 to 15 are the reasoning and the evidence; section 17 is the list of things that have bitten me more than once.

How to use this file:

- **New app.** Walk the Day 0 checklist in §16, copy the stack defaults from §15, and link this file from the project's README or CLAUDE.md as "Conventions: ~/Projects/application_features.md".
- **Existing app.** Audit it against §3 to §14. The regrets in §17 are the highest-yield fixes.
- **Design docs.** Riddle's architecture is included where it designs something the shipped apps do not yet have (marked "designed in Riddle").

## Sources

| Project | Stack | Maturity | Contributed most |
|---|---|---|---|
| Grey Eminence | SwiftUI + AppKit, SwiftData, Sparkle | v0.50, 166 tags, used daily | Updates, versioned schemas, backups, AI cost ledger, activity bars, What's New |
| Sleipnir | Tauri 2, Bun, Rust | v0.1.13, Homebrew, signed, used daily | Safety UX, dev/prod split, tours, docs site, release gate |
| Pontifex | Tauri 2, Rust | v0.1.2, Homebrew | Developer tab, logging discipline, confirmations re-checked in Rust, notifier helper |
| CORA | Tauri 2, Rust, SQLite | unreleased, used daily | Agent/UI parity, audit with undo, circuit breaker, context inspector, experiments lab |
| Warpcore (Visual Builder + CLI) | Tauri 2 + Python sidecar | beta, active | Append-only audit, plan/diff before write, sidecar CLI, NDJSON progress, settings by ownership |
| Relevant Reviews (my fork of Besendorfer's) | Tauri 2 | released upstream | Tauri updater pipeline, stable download asset, CLI twin |
| vpb (macOS) | SwiftUI MenuBarExtra + network system extension | working | Signing script, menu-bar restraint, one critical alert, snapshot rendering |
| NommoBar | Swift menu bar | pre-commit | Hot-plug handling, protocol docs, App Nap |
| gitf-gui | Godot | prototype | Snapshot-on-join plus event-stream UI protocol |
| Riddle | design docs only | docs | Action layer, provenance, undo, developer-mode inspectors |

Smaller or abandoned: evalu8 (Electron), a task-viewer spike at work (Tauri v1), three Flutter macOS apps. They appear only in §17.

---

## 1. Principles

These are the rules the repos state in their own words. The source is the file that says it.

| Principle | Where it is written |
|---|---|
| "Every AWS call happens in Rust; the webview never sees credentials." | Pontifex `README.md:17` |
| "The token itself never crosses the IPC boundary — Rust-side callers only." | CORA `src-tauri/src/secrets.rs:21` |
| "Production changes are allowed — they just should not be one keystroke away." | Pontifex `README.md:231` |
| "safety by construction, not by dialog" and "Visible, reversible, and never silently destructive" | Sleipnir `docs-src/safety.md:13,98` |
| "Nothing is ever filed silently." / "Nothing is written until you say so." | Pontifex `docs/jira-integration.md`; Sleipnir `docs-src/getting-started.md:84` |
| "An audit trail that can be edited after the fact is not an audit trail." | Warpcore `src/warpcore/audit/recorder.py:6` |
| "Never claim an action happened unless its tool result says so." / "Prefer doing over describing." | CORA `src-tauri/src/analysis/chat.rs:40,44` |
| "Retrying a single call is the right answer to a blip. It is the wrong answer to an outage." | CORA `src-tauri/src/health.rs:3` |
| "a usage row is bookkeeping, never a reason to fail the work" / "never fail a command over telemetry" / "logging must never be able to fail the thing it is describing" | CORA `usage.rs:41`; Warpcore `cli/main.py:48`; Pontifex `src/lib/ipc.ts:74` |
| "a bad guess is not a neutral default" | Pontifex `src-tauri/src/lib.rs:41` |
| "a fake bar would be worse than none" | Grey Eminence `Services/TransientActivityCoordinator.swift` |
| "Every fact can answer why" / "Derived, not stored" | Warpcore `visual-builder/DESIGN_GUIDANCE.md §5.6`, `UX_PLAN.md §3` |
| "Modals are for tasks with progress, and for nothing else" | Warpcore `DESIGN_GUIDANCE.md §7` |
| "Width is the scarcest thing in the menu bar" | vpb `macos/VpbApp/UsageModel.swift` |
| "the click IS the reconnect" / "recognition over recall" | Sleipnir `src/App.tsx:211`, `src-tauri/src/config/schema.rs:69` |
| "Comments only when the WHY is non-obvious. Don't narrate WHAT." | Grey Eminence `Resources/Docs/CONTRIBUTING.md:31-46` |
| "No intelligent decision without structured telemetry." | Riddle `ARCHITECTURE.md §92` |
| "The UI and the Director are two ways of interacting with the same Riddle." | Riddle `ARCHITECTURE.md §3` |

Distilled:

1. **The core owns the truth; the UI is an adapter.** Rust or Swift holds state, credentials, and every external call. The webview or view layer renders and asks.
2. **Credentials never reach the UI layer.** Not tokens, not URLs that act as tokens, not shell strings.
3. **Dangerous actions are hard to do by accident and easy to reverse.** Confirmation is structural (hold, type the name, two-step) and is re-checked in the backend, never only in the dialog.
4. **Every mutation leaves a trail that cannot be edited**, with actor, before and after, and a way back.
5. **Preview before write.** Plan, diff, pre-flight, simulate. The first API call happens after the user has seen the plan.
6. **Bookkeeping never fails the work.** Logging, usage rows, telemetry, and update checks degrade silently.
7. **Honest UI.** No fake progress, no claimed actions, no green animation over nothing. Disabled controls say why.
8. **Developer mode is a product feature.** It exposes everything and grants nothing.
9. **Agents, CLIs and clicks go through the same audited actions.**
10. **Reversible by construction.** Soft delete, restore that refuses to clobber, purge as the only lossy path.
11. **Write the why down where it lives.** Rationale comments cite dated incidents; roadmaps say what is not planned.

---

## 2. Architecture shape

**Core and adapter.** The native core (Rust crate or Swift package) is the application. Tauri commands and SwiftUI views are thin. Pontifex, CORA and Sleipnir all keep a one-to-one TypeScript mirror of the command surface (`src/lib/tauri.ts` in Sleipnir, `src/lib/ipc.ts` in Pontifex and CORA). Riddle formalises this as "Tauri is an adapter; Actions are the canonical API" (`ARCHITECTURE.md §7, §8`).

**One instrumented invoke wrapper.** Every IPC call goes through one function that logs start and finish, warns when slow (2 s), keeps an in-flight registry, and never logs argument values because "they include whole schema documents" (Pontifex `src/lib/ipc.ts:112-150`). CORA's wrapper additionally returns only `*_present` booleans for secrets (`src/lib/ipc.ts:110`).

**Typed errors.** Commands return `{kind, message}` so the UI can offer the right affordance, such as a Sign In button on an auth error, and tests can sniff SDK error codes (Pontifex `src-tauri/src/error.rs`). Sleipnir centralises presentation in `formatError` (`src/lib/errors.ts`).

**Action layer as capability boundary.** CORA's assistant can only mutate through `execute_action`, which dispatches to the same commands a click uses, gated by an explicit `is_action` allowlist (`src-tauri/src/analysis/chat.rs:402-404, 577-579`). Warpcore's GUI shells out to the bundled CLI for everything, so "there is no simulation" (`visual-builder/README.md:110`), through one `warpcore_command()` constructor (`src-tauri/src/main.rs:195`). Sleipnir's CLI reuses the same `state` and `engage` modules as the app. Riddle generalises this: Actions own input and output contracts, validation, authorization, risk metadata, idempotency, audit, and undo (`ARCHITECTURE.md §8`).

**Ship a CLI twin or headless mode.** Relevant Reviews has `rr`, a Bash CLI producing the same manifest the app opens. Warpcore bundles its CLI as a sidecar. Sleipnir exposes `sleipnir creds --profile` as an AWS `credential_process` resolver (`src-tauri/src/cli.rs`). NommoBar ships `nommoctl` as the hardware probe. The CLI doubles as the automation and agent hook.

**Pure logic separated from transport and UI.** NommoBar splits `RazerProtocol` (pure, golden-tested) from `DeviceTransport` (IOKit) from the app (`Package.swift`). Grey Eminence extracts pure functions for testability as a habit (`HindsightReassignment`, `DiskSpace.warning`).

**Tenant isolation.** CORA keeps one SQLite per GitHub org under `app_data/orgs/<login>/cora.sqlite` with the rule "Data never crosses org boundaries" (`src-tauri/src/orgs.rs:1-5`).

**Generated bindings.** CORA generates TypeScript types from Rust with ts-rs (`npm run bindings`).

**Backend-driven UI protocol.** gitf-gui's `GAME_UI_PROTOCOL.md` defines a snapshot on join, a typed event stream with a visual-cue column per event, inbound commands, and a scenario mock server that replays three scripted runs (`mock_server/server.py`). That recipe is independent of Godot.

**Heavy data stays native.** Lists are paged over IPC; the webview never runs SQL. Events are for signals; Channels are for volume (Riddle review, `ARCHITECTURE-REVIEW.md §5.4`).

---

## 3. Distribution, signing, updates, versioning

**Release pipeline (Tauri).** Tag push `v*` triggers: verify the tag equals the version in `tauri.conf.json`; run `vitest` and `cargo test --lib`; build; sign and notarize when `APPLE_*` secrets exist; a hard gate that runs `codesign --verify`, `xcrun stapler validate`, and `spctl` and refuses unnotarized output; package as `.app.tar.gz`; create the GitHub release idempotently so a rerun reuses published bytes; `sed` the version and sha256 into the Homebrew cask (Sleipnir `.github/workflows/release.yml:120-144`; Pontifex `.github/workflows/release.yml:1-136`). A separate `validate-tap.yml` proves the tap token works before release day (Sleipnir).

**Release pipeline (Swift).** Tag push: `xcodegen`, restore entitlements, import the certificate into a temporary keychain, run tests, archive with hardened runtime, `notarytool` and staple, `create-dmg`, Sparkle EdDSA sign, generate `appcast.xml`, publish (Grey Eminence `.github/workflows/release.yml`; playbook in `docs/auto-update-playbook.md`). vpb's `macos/scripts/sign-and-notarize.sh` signs inside out (extension first, then app) and timestamps `CFBundleVersion` because "macOS dedupes system extensions by version".

**Stable download link.** Upload a fixed-name asset with `--clobber` so the README link never rots (Relevant Reviews `.github/workflows/release.yml`).

**Auto-update.** Tauri: `tauri-plugin-updater` with `createUpdaterArtifacts`, endpoint `releases/latest/download/latest.json`, minisign public key in config; silent check 5 s after launch then every 6 h; an `UpdateBanner` state machine idle → checking → available → downloading (progress) → ready → Relaunch, dismissible (Relevant Reviews `app/src/App.tsx:116`, `app/src/components/UpdateBanner.tsx`). Swift: Sparkle 2 with the feed pointing at the latest GitHub release asset, launch-time check forced, and Sparkle disabled entirely in Debug so it cannot overwrite the DerivedData build (Grey Eminence `Services/Updates/SparkleUpdaterDelegate.swift:72-80`). A `StartupUpdateGate` blocks maintenance and auto-record until the update check answers, capped at 10 s and 120 s (`Services/Updates/StartupUpdateGate.swift`).

**Key custody.** The updater private key strands every install if lost. Keep it in the password manager and as a CI secret, and say so in the release playbook.

**Version discipline.** One source of truth for the version, checked by CI. A Claude Code PreToolUse hook denies `git tag` when the tag does not match `MARKETING_VERSION` and denies commits touching Swift without a bump (Grey Eminence `.claude/hooks/check-version-bump.sh`). Warpcore's regret is the version hand-duplicated across `pyproject.toml`, `package.json`, `Cargo.toml` and `tauri.conf.json`.

**Build stamp.** Inject git sha, dirty flag and build time at build (Warpcore `visual-builder/src-tauri/build.rs`) and show it in the title, About pane and footer. Grey Eminence uses a UTC timestamp as the build number.

**Dev builds look different and live apart.** Sleipnir keys dev on `cfg!(debug_assertions)` (deliberately not `tauri::is_dev()`): dev uses `~/.sleipnir-dev`, a `.dev` keychain service suffix, and `sleipnir-dev.log`, and wears a gold DEV badge because "magenta is reserved for production danger" (`src-tauri/src/paths.rs:18-32`, `src/App.css:250-263`). Grey Eminence shows an orange "DEV BUILD · vX" capsule compiled out of Release (`App/DevBuildBanner.swift`). Warpcore titles the window "dev <sha>*".

**Sign dev builds too.** A stable Developer ID on local builds keeps keychain ACLs and macOS TCC grants (microphone, screen) from resetting on every rebuild (Pontifex `scripts/cargo-signed.sh`; Grey Eminence `project.yml:48-52`).

**Changelog and What's New.** Bundle `CHANGELOG.md` and render it in-app with read tracking (Grey Eminence `Features/Help/ChangelogView.swift`). A post-update What's New sheet with "Try it" deep links, a curated `FeatureHighlightCatalog` described as "the trailer, not the changelog", and NEW badges that retire on first interaction (`Features/WhatsNew/`). Pontifex and Sleipnir rely on `gh release --generate-notes`, which is the minimum.

**Helper binaries and sidecars.** Version a helper by a hash of its inputs so a stale installed copy is replaced (Pontifex `scripts/build-notifier.sh:14-18`, `src-tauri/src/watch/notify.rs:145-160`). Bundle a CLI through Tauri `externalBin`, never rely on PATH, and show the CLI version and build time in About (Warpcore `src-tauri/src/main.rs:171-190`). Smoke-test the frozen binary with a stripped environment; UPX is off because it "corrupts signatures on macOS" (Warpcore `packaging/build.py:74-95`, `warpcore.spec:90`).

**macOS gotchas recorded.** DMG creation needs Finder AppleScript and "cannot run headless", so ship `.app.tar.gz` from CI (Pontifex). `xcodegen generate` wipes entitlements, so the release job restores them (Grey Eminence `CONTRIBUTING.md:17-21`). Xcode 26 broke Developer ID export for network extensions; vpb hand-signs.

---

## 4. Developer mode and diagnostics

**Toggle.** A `developerMode` setting in Settings unlocks a Developer tab or pane (Pontifex `shell.tsx:26`, CORA `models.rs:443`, Grey Eminence `DeveloperSettingsView.swift`, Sleipnir `DeveloperTab.tsx`, Warpcore `GeneralSection.tsx`). It never grants additional authority (Riddle `ARCHITECTURE.md §91`).

**What the Developer pane shows** (union across the apps; build the first five on Day 0):

1. Runtime: version, debug or release, framework version, effective log level, build stamp, OS (Pontifex `developer-page.tsx:175-217`).
2. Storage: config, cache and log directories with sizes and file counts, "Reveal in Finder", "Open config file", caches with a Clear button (Pontifex); database path and size (WAL-aware), record counts, schema and seed versions (Grey Eminence).
3. Log viewer: live tail, filter by category, level and text, follow, copy, clear, expandable JSON payload per row (Pontifex, Sleipnir `DeveloperTab.tsx`, CORA `DeveloperPane.tsx`, Grey Eminence `LogView.swift`).
4. In-flight IPC calls with elapsed milliseconds (Pontifex `src/lib/ipc.ts:97-110`).
5. Copy diagnostics block (Pontifex). This should grow into a redacted diagnostics bundle export; no app has one yet.
6. Effective settings as JSON (CORA Internals tab).
7. Prompt editor against the built-in default (CORA `PromptEditor.tsx`; Grey Eminence runtime overrides in `Services/AI/PromptStore.swift`).
8. AI usage and cost (CORA Usage tab; Grey Eminence Settings → AI Usage).
9. Context inspector attributing every prompt byte to an origin with token estimates and the projected next-request size (CORA `analysis/types.rs:470-524`, `ContextDrawer.tsx`).
10. Agent trace: tool, thought and status steps (CORA `TraceSteps.tsx`).
11. Experiments lab: captured AI config variants run like for like with a weighted findings dashboard (CORA `src-tauri/src/experiments.rs`).
12. Effort gauge in a feature header: calls per poll, fetched versus kept (Pontifex Watch, `README.md:614-623`).
13. A Raw tab on every inspector subject, with an Overview → Evidence → Raw ladder (Warpcore `DESIGN_GUIDANCE.md`).
14. Replay and rescoring tools (Grey Eminence `TranscriptTestView.swift`; dedup debug rows).
15. A console handle such as `window.__warpcore` in dev builds and Cmd+R reload (Warpcore `src/main.tsx:15-33`).
16. A `--snapshot <dir>` flag that renders a window to PNG so design iteration is not "a guessing game" (vpb `macos/VpbApp/Snapshot.swift`).

**Out-of-app probes.** Keep a runnable example that reproduces client construction outside the app (CORA `src-tauri/examples/bedrock_probe.rs`), a CLI probe for hardware (`nommoctl raw/list/frame`), and a `--dev` flag that turns on tracing, writes a session manifest, and is read back by `warpcore debug session` (Warpcore `src/warpcore/devmode.py`, `cli/debug.py`).

**Designed in Riddle, not yet built anywhere.** Message, Signal, Mesh, Escalation, Director and Action inspectors; a live pipeline view; cost and decision-quality observatories; exportable redacted traces; a demo mode that "must never fabricate telemetry" (`ARCHITECTURE.md §67-90`).

**Feature flags.** None of the apps has a flag system and none needed one. Developer toggles and `UserDefaults` are enough until there is a second user.

---

## 5. Logging

**Tauri default.** `tauri-plugin-log` v2 to `LogDir` plus stdout in all builds, 5 MB file cap, format `[rfc3339][LEVEL][category]`, noisy crates (AWS SDK, hyper, rustls) pinned to warn (Pontifex `src-tauri/src/lib.rs:143-183`; Sleipnir `lib.rs:107-115`). Files land in `~/Library/Logs/<bundle id>/`; say so in the troubleshooting docs.

**Swift default.** `os.Logger` with a subsystem and named categories (vpb `dev.vpb`: settings, widget, setup, rules, proxy, flows, relay), plus a file log with size-based rotation. Grey Eminence added rotation after `system.log` reached 735 MB (`Services/Logging/LogManager.swift:55-62`). Put the `log stream --predicate 'subsystem == "..."'` incantation in the README (vpb).

**Fixed category vocabulary.** Categories are an enum with `linfo!`, `lwarn!`, `ldebug!` macros and a `Timed` guard that warns when an operation exceeds 3 s (Pontifex `src-tauri/src/logging.rs`). CORA tags by source (poller, github, bedrock, analysis, app, ui, usage).

**Structured payloads.** Append ` ||PAYLOAD||<json>` to a line and let the in-app viewer parse and expand it (Sleipnir `src-tauri/src/applog.rs`, `src/lib/logs.ts`). CLIs write JSONL daily files (Warpcore `utils/logging.py`).

**Never log secrets or argument values.** The invoke wrapper logs names and timings only (Pontifex). botocore stays at INFO even in dev because DEBUG "dumps full signed requests including credential material" (Warpcore `devmode.py:71-73`). Imported environment values are redacted by default (`discovery/config_emitter.py`). Document what logs may contain, such as account IDs and ARNs (Sleipnir `docs-src/troubleshooting.md:174-177`).

**Webview logs flow to the backend log.** A `ui_log` command and per-route error boundaries that write to the `ui` category (Pontifex `src/app/error-boundary.tsx`); `window.onerror` appended to `frontend-errors.log` (CORA `src/main.tsx:12-47`).

**Level from settings, applied next launch.** Writes to shared registries always log at info regardless of level (Pontifex `README.md:914`).

**Helpers and sub-entities log separately.** The notifier helper writes its own `notifier.log` (Pontifex `notifier/main.swift:16-18`). Grey Eminence keeps a per-meeting `meeting.log`.

**Logging never fails the work.** A failure to create the log directory does not abort (Warpcore `utils/logging.py:109-116`). stderr is for humans, stdout for machine-readable output (`logging.py:102`).

**In-app tail.** Seek-from-end `tail_lines` (Pontifex `commands/devtools.rs`) or a 512 KB window (Warpcore `read_text_tail`), never load the whole file.

**Gaps across every app:** a redaction layer, a diagnostics bundle export, and a retention policy. Riddle separates product Activity from software logs (`ARCHITECTURE.md §47`), which is the right split.

---

## 6. Audit trail, provenance, safety, undo

**Audit store.** Append-only JSONL per environment with before and after snapshots, actor, command, API calls and an `undo_kind` (Warpcore `audit/models.py`, `audit/recorder.py`). SQLite `audit` table with old and new values and an `undone` flag, plus `undo_audit` that reverses mute, untrack and priority (CORA `store.rs:78-87`, `commands.rs:498-560`). Every mutating command logs a structured entry (Sleipnir `commands.rs:49-172`). The audit view has "no clear action on purpose" (Warpcore `DESIGN_GUIDANCE.md §6.4`).

**Actor attribution.** Assistant writes are tagged "via assistant" (CORA `chat.rs:32`). Riddle's actor set is USER, DIRECTOR, SIFT, MESH, AUTOMATION, SYSTEM, and every classification records its decision mechanism (`ARCHITECTURE.md §47-49`).

**Provenance on anything written into shared files.** Exported AWS profiles carry `# written by Pontifex — <account> as <role>` (`aws/config_writer.rs:11-29`). Sleipnir neutralises rather than deletes with a reversible `# sleipnir-disabled:` prefix and never rewrites a co-managed file wholesale (`aws_config_editor.rs`). Warpcore records provenance per imported resource and distinguishes adopted from managed (`discovery/provenance.py`, `orchestrator/compiler.py:17-20`).

**Preview before write.** A diff-first confirmation dialog showing live versus draft (Pontifex `confirm-save-dialog.tsx`); an import pre-flight plan; a Jira preview that "shows the exact ticket before anything is written"; a "Test notification" button. Warpcore's `diff` shows property changes, code redeploys and IAM gained or lost, and `--progress` emits the plan "before the first API call" (`cli/main.py:358`); the Activity drawer lists scenarios a plan's deletes would break. Sleipnir's discovery review table writes nothing until approved. Riddle simulates a Mesh change against history and shows the count of affected messages before applying (`ARCHITECTURE.md §38`).

**Confirmation tiers, from lightest to heaviest.**

1. Confirm in place, never a modal, with a single error banner inside the drawer (Sleipnir `ProjectPanel.tsx:30`, `App.tsx:256`).
2. Two-step `confirm-danger` buttons for merge and close, and a Shift modifier on analyse "so it isn't a slip" (CORA `MainApp.tsx:751-775, 2168`).
3. Type the exact name to delete a schema or save to a protected environment (Pontifex); a project lock that opens locked, unlocks by typing the project name, and relocks on environment switch (Warpcore `ProjectLock.tsx`).
4. Press and hold for 1,200 ms with a fill animation and "⏱ KEEP HOLDING UNTIL FULL" coaching on early release, reserved for the one dangerous combination, ADMIN plus PRD (Sleipnir `HoldButton.tsx`).
5. Agent mutations pause as a pending action with editable text until the user confirms (CORA `chat.rs:1-5, 40`).

**Re-check in the backend.** `confirm_name` and `confirm_protected` are validated in Rust, not only in the dialog (Pontifex `commands/schemas.rs:515-540`). Mode fallback never escalates privilege, covered by the test `readonly_request_never_escalates` (Sleipnir `engage.rs:166-175`).

**Programmatic drivers confirm on their own side.** `--progress` implies `--yes` because "a program driving the CLI confirms on its own side" (Warpcore `cli/main.py:493`).

**Undo.** Soft delete into RECENTLY DELETED with Restore and Delete Forever; restore refuses a name clash because "an undo that silently replaced your current work would be worse than no undo at all" (Sleipnir `config/schema.rs:92-131`). AI tidy-ups never delete: dropped items become "Won't Do" with a reason and a Restore, capped at 60% drops (Grey Eminence CHANGELOG 0.48.0). Audit undo plans first and refuses if the target drifted (`audit/undo.py`). Irreversible actions are labelled where they happen: "Merges are permanent" (CORA `HistoryDrawer.tsx:9`). Canvas undo coalesces edits within 400 ms and excludes selection changes (Warpcore `store/useStore.ts:234`). Riddle makes undo architectural: reversible Actions define inverse behaviour, bulk actions undo as one Activity, and anything irreversible declares it before execution (`ARCHITECTURE.md §50`).

**Risk metadata.** Riddle's Actions carry `mutating`, `reversible`, `external_side_effect`, `risk`, `confirmation_policy` and autonomy levels OBSERVE, SUGGEST, ACT, TRUSTED (`§51`). CORA classifies findings by severity and files as critical, important or mechanical. Sleipnir colours production red everywhere and never lets a mode fallback escalate.

**Persistence gate.** Replace `try? save()` with a gate that logs every save with its call site and flips an `isFaulted` flag after three consecutive critical failures (Grey Eminence `Services/Persistence/PersistenceGate.swift`). Keep raw AI responses for replay (`AIIntelligenceService.swift:20-23`).

---

## 7. Agent-native and AI

**Providers.** Amazon Bedrock Converse and ConverseStream are the default (Pontifex `commands/bedrock.rs`, CORA `engine.rs`, Grey Eminence `Services/AI/`), with Anthropic API, the Claude CLI as a subprocess, OpenAI and local models where needed. Relevant Reviews dispatches on model-string shape: an ARN means Bedrock, anything else spawns `claude --print --model` (`app/src-tauri/src/ai.rs`).

**No hard-coded model.** Import the catalog from a shared settings file (`~/.claude/model-settings.json`) or list live; pick the "fast model" as the catalog entry containing "haiku" (Pontifex `bedrock.rs:409-420`). CORA runs four tiers, main, drill, chat and scout, and records the evidence for each effort setting in the code ("on four PRs run at both levels (2026-09-06) the findings held while the run took 30-50% less time", `models.rs:421-437`).

**Streaming.** `converse_stream` emitting `bedrock://delta` and `bedrock://done` events (Pontifex `bedrock.rs:255-292`).

**Prompt caching.** CachePoints on system and tools plus a rolling message checkpoint, with a cache-free retry, and cache tokens shown in usage (CORA `engine.rs:445-470, 954-970`).

**Bounded tools.** Character caps, batch limits, a shared search rate gate, an exploration budget of 24 turns after which "the research tools are withdrawn", and schema-drift coercion instead of spending retry turns (CORA `analysis/tools.rs`, `engine.rs:22-27, 2160`). A scout pre-read for oversized diffs (`analysis/scout.rs`).

**Bounded output.** Model output is "a validated draft you must save" and "everything else is deterministic code, not a model" (Pontifex `README.md:226-229`). A model "cannot add or drop a finding" (Warpcore `visual-builder/README.md:311`). The Ask prompt demands inline `[n]` citations and "Never invent meetings, people, dates", and citations become links (Grey Eminence `AskPromptBuilder.swift:7-27`). A JSON decoder that strips fences and salvages truncated output (Grey Eminence `AIResponseDecoder`; Relevant Reviews `extract_json_array`).

**Agent and UI parity.** The assistant's actions "run through the same audited paths as a click" (CORA `chat.rs:577`). Riddle's Action layer is the general form: the Director invokes Actions, never the store; prompts are not authorization (`ARCHITECTURE.md §9, §51`).

**Prompts.** Constants in code with a "kept in sync with the validator" comment (Pontifex `bedrock.rs:21`), or runtime-editable markdown overrides with `{{placeholders}}` under App Support (Grey Eminence `PromptStore.swift`). Nobody versions prompts yet; CORA's experiments lab and Riddle's Signal lifecycle (propose, test, compare versions, promote with approval, `§30-35`) are the designs to grow into.

**Cost.** Per-request usage rows with a family rate table, where "anything we can't price is reported as unpriced rather than quietly counted as free" (CORA `usage.rs:1-9`). Task-local attribution, a token ledger, pricing estimates, 30-day rollups and 90-day retention (Grey Eminence `AIUsage.swift`, `AIPricing.swift`). Riddle designs a cost observatory with cost per 1,000 items and escalation rate (`§84`).

**Separate principals.** Bedrock authenticates with a different AWS profile from the data registry so the model never holds the data's permissions (Pontifex `state.rs:81-94`).

**Resilience.** A per-endpoint circuit breaker in Tauri state with a probe slot that cannot wedge (CORA `health.rs`).

**Running user CLIs from a bundle.** Probe `~/.local/bin`, `/opt/homebrew/bin`, `~/.nvm`, `~/.volta` because ".app bundles inherit a minimal PATH", and set `CLAUDECODE=""` to prevent recursive invocation (Relevant Reviews `ai.rs`).

**MCP.** Expose the action layer read-only first. Warpcore's `warpcore mcp` (FastMCP, stdio) is read-only except two `write_*` tools that touch only project files (`src/warpcore/mcp_server.py`).

**Local-first AI.** On-device ASR, diarization and embeddings; cloud only for reasoning; one sentence stating what leaves the device and to whom (Grey Eminence). Riddle plans per-account cloud exclusion (`§57`).

**Security for agents that read untrusted content.** Tag mail, documents and tool results as untrusted; disable autonomous egress after ingesting untrusted content in the same task; create rules only from explicit user actions; show real Action arguments in approval prompts. The incidents and controls are collected in Riddle `ARCHITECTURE-REVIEW.md §5.3`.

---

## 8. Security and credentials

**Keychain.** The `keyring` crate with `apple-native` and `windows-native` features enabled, because the default is an in-memory mock "which cost us every refresh token" (Sleipnir `Cargo.toml:28-31`). One wrapper that rewrites the confusing macOS refusal message (Pontifex `keychain.rs:26-48`). One keychain item per org, migrated from four items to one blob in code (Sleipnir `secrets.rs:51-73`). Swift: KeychainAccess with `afterFirstUnlockThisDeviceOnly` and a lock-guarded cache (Grey Eminence `KeychainHelper.swift`).

**Nothing secret crosses IPC.** Only `*_present` booleans reach the webview (CORA `ipc.ts:110`); TypeScript types carry account and role only (Sleipnir `types.ts:81`). A webhook URL "is a bearer credential in disguise" and is stored in the keychain (CORA `secrets.rs:50`).

**Capabilities and CSP.** Per-window capability files; the callout window gets "events + dragging only, no credential-adjacent surface" (CORA `capabilities/callout.json`). Custom commands "sit outside the capability ACL", so credential commands are guarded with `require_main` (CORA `commands.rs:18-29`). Strict CSP `default-src 'self'; connect-src 'self' ipc:` (Pontifex `tauri.conf.json:28`, CORA). Sleipnir and Warpcore ship `csp: null`, which is a regret to fix.

**Files.** Config holding a token is chmod 0600 (Relevant Reviews `config.rs`). Atomic writes through a helper with private and public modes (Sleipnir `atomic_file.rs`). Privileged file commands require `.warpcore` in the path and no `..`, and refuse to clear `state` (Warpcore `main.rs:1316, 1528`). Terminal launches recompute the command backend-side "so the page never hands a shell string across" (Warpcore `README.md:279`).

**Share credentials with the user's CLI tooling in its native format.** SSO tokens are written to `~/.aws/sso/cache` in the AWS CLI's format so terminal and app share one sign-in (Pontifex); the refresh token is stripped from the CLI-compatible cache (Sleipnir `token_cache.rs:61-64`). "Only profile names are stored — never credentials" (Pontifex `README.md:778`).

**macOS permissions.** Stable signing so TCC grants survive rebuilds; error strings that deep-link to the right System Settings pane (`x-apple.systempreferences:...Privacy_ScreenCapture`, Grey Eminence `ScreenShareSettingsView.swift:188`); security-scoped bookmarks for user-selected directories; entitlements deliberate and documented, including sandbox off "on purpose" with the reason (vpb `VpbApp.entitlements`).

**Untrusted HTML.** Sanitize markdown (`rehype-sanitize`, CORA); proxy private images through Rust as data URLs (CORA `github/attachments.rs`); render foreign HTML in a sandboxed `srcdoc` iframe with remote content blocked (Riddle review `§5.4`).

**Never.** `nodeIntegration: true`, `webSecurity: false`, credentials in `localStorage` (evalu8) or a committed `.env` (office_time).

---

## 9. Background work, OS integration, resilience

**Poll loops.** One tokio task per scope; `tokio::select!` on a sleep versus a `Notify` trigger so "Refresh now" and timers share one loop; exponential backoff to 300 s; slow down under rate limit; gentler cadence for background scopes (CORA `github/poller.rs:27-115`). Overlap window and catch-up cap (15 s default, 90 s overlap, 1 h cap) and parking on auth failure with wake-up via credential-change subscribers (Pontifex `watch/runtime.rs`, `aws/clients.rs:49-55`). Concurrency 3 with backoff for throttled APIs (Sleipnir `sso.rs:19-29`).

**Sleep and wake.** A 30 s tick detects sleep from a wall-clock gap and logs "resumed after Ns asleep" (Sleipnir `lib.rs:59-103`, with the `CLOCK_UPTIME_RAW` rationale). A lifecycle coordinator flushes on terminate, pauses on sleep and resumes on wake (Grey Eminence `AppLifecycleCoordinator.swift`). `visibilitychange` and `focus` trigger a refresh pass, and an auth card retries on window focus (Sleipnir `useAppState.ts:24-55`; CORA `AwsAuthCard.tsx:6-8`). `ProcessInfo.beginActivity` defeats App Nap (NommoBar `NommoBarApp.swift`).

**Budgets and guards.** A daily auto-analyse budget, in-flight dedupe, and a persistent-critical re-alert every 5 min (CORA `store.rs:572`). An idle guard asks "keep watching?" after 4 h and stops after 5 min of silence (Pontifex `idle-guard.tsx`).

**Progress.** Named events per feature (`report://progress`, `discover:progress`, `engage:progress`). NDJSON `--progress` from the CLI with the plan emitted before the first API call, consumed by a pure, tested frontend reducer (Warpcore `cli/run_progress.py`, `src/utils/deployEvents.ts`). A step-labelled loading view with cancel (Relevant Reviews `LoadingView.tsx`). An Xcode-style bottom activity bar with named steps and a minimum 1 s visible time (Grey Eminence `TransientActivityCoordinator.swift`). Cancellable subprocesses via process-group kill, with the PyInstaller reparenting caveat (Warpcore `main.rs:740-900`).

**Tray and menu bar.** Tray menu with Toggle Callout, Reset Callout Position, Open, Refresh Now, Quit (CORA `lib.rs:300-341`). `MenuBarExtra` with a state icon and Start, Pause, Stop (Grey Eminence). `NSStatusItem` plus a manually placed borderless `NSPanel` so it lands correctly on external displays, with a global click-outside monitor (NommoBar). A template image until warn or critical "the way the system battery stays monochrome until it is actually low", and title text only at critical (vpb `MenuBarGauge.swift`, `UsageModel.swift`). Close hides the window while work is armed, the Dock reopen restores it, and a Dock badge shows the count (Pontifex `runtime.rs:292`). `tauri-plugin-window-state` minus VISIBLE, plus an off-screen rescue after monitor changes (CORA `lib.rs:26-97`). `unhideWithoutActivation` via objc2 to hand focus back (CORA `commands.rs:2453`).

**Notifications.** On macOS 26 the legacy API behind `tauri-plugin-notification` "reports success and shows nothing", so Pontifex ships a 150-line Swift `LSUIElement` helper using `UNUserNotificationCenter`, built by `scripts/build-notifier.sh`, registered with `lsregister`, launched with `open -g -n -a`, writing a verdict JSON that the Test button reads, with a bundle-id generation bump to re-prompt after a dismissed permission dialog (`src-tauri/notifier/main.swift`, `watch/notify.rs`). macOS gives no click callback, so the next app activation within 120 s claims the notification (CORA `notify.rs:7-18`). Reserve one interruptive alert, "the one thing vpb is allowed to put in front of you", only for failures across two or more sources sustained over 5 min, re-presented only when the finding set changes (vpb `CriticalAlert.swift`). Actionable notifications and a daily scheduled reminder (Grey Eminence `Services/Notifications/`).

**Launch at login.** `SMAppService` with `.requiresApproval` handled and the setting read back after write, so the UI reports "what the system now says rather than what was asked for" (vpb `SettingsModel.swift:88-118`). Tauri: `tauri-plugin-autostart`. Regret: unwired toggles (Grey Eminence) and absence everywhere else.

**Devices and hot-plug.** IOHIDManager match and removal callbacks, a 2 s settle, re-apply persisted state, one persistent handle per session because the device reverts if closed, a 4 s re-stream, a 0.12 s UI debounce, a serial queue, and a watchdog; "do NOT retry-storm the device" (NommoBar `DeviceManager.swift`, `docs/PROTOCOL.md`).

**Offline.** "Background refresh gives up quietly" (Sleipnir `orgs-and-sessions.md:73`); typed errors carry a sign-in affordance; a health badge distinguishes intent (ENGAGED) from delivery (STALE) (Sleipnir `commands.rs:452-527`).

**Designed in Riddle.** Priority classes for jobs (interactive, incoming, catch-up, backfill, experiments), per-resource rate limiters, checkpointed durable jobs, and a user-facing Task Centre with progress and cancel (Riddle `ARCHITECTURE-REVIEW.md §2.21`).

---

## 10. Settings, configuration, onboarding, help

**Split by ownership.** Declarative, shareable, hand-editable config (`~/.sleipnir/config.toml`) versus runtime state (`state.json`) (Sleipnir `state.rs:1-9`). Team-owned project file (`warpcore.yaml`) versus workstation preferences in `localStorage`; "Application settings are the only thing in the app that is allowed to look like a settings page" and "Configuration belongs to the object it configures" (Warpcore `DESIGN_GUIDANCE.md §2, §4.2`). Settings separate from high-churn state (`settings.json` versus `watches.json`) so a form save cannot clobber live data (Pontifex `watch/store.rs:1-12`).

**Backend-owned settings.** Written atomically (tmp and rename), `#[serde(default)]` on every field, a regression test that a file written before a feature still loads (Pontifex `settings.rs:503-518, 645-659`; Sleipnir `config/mod.rs:326`), value migrations in code, and a light-path update for display preferences that avoids invalidating credentials (Pontifex `state.rs:96-112`).

**Locations.** Tauri app data (`~/Library/Application Support/<bundle id>/`), a dot directory for hand-edited files (`~/.sleipnir/`), `~/.config/<app>/` for flat key=value config (Relevant Reviews), with dev-suffixed variants.

**First run.** Bootstrap from tools the user already has: `~/.aws/config`, Claude Code settings, a known repo path; rank profile guesses by "how likely the profile is to work right now" (Pontifex `lib.rs:38-80`). A four-step onboarding when no credential exists and a hotkey sheet shown once (CORA `MainApp.tsx:2884, 2247`). A profile sheet if identity is empty and What's New suppressed for new users (Grey Eminence `ContentView.swift:277-306`). Welcome cards (Warpcore) and a numbered setup view, DAEMON, TRUST, TERMINAL SESSIONS, NETWORK EXTENSION (vpb `SetupView`).

**Tours.** Data-driven `TOURS` pointing at `data-tour` anchors; missing anchors fall back or skip; progress in `localStorage`; offered once when the app is empty; copy rules "four words or fewer, say what a control is for" (Sleipnir `src/tour/`, `tours.ts:9-16`).

**Help.** Bundled README, guide, CHANGELOG and LICENSE rendered in a Help window (Grey Eminence `HelpDocsView.swift`). A zero-dependency docs site built from `docs-src/` markdown with front-matter ordering, GitHub alerts, a search index, prev and next, a link checker and a Pages deploy (Sleipnir `scripts/build-docs.ts`, `check-docs-links.ts`). The README as the manual when there is no site (Pontifex, 935 lines). A `?` shortcut sheet (Warpcore). Settings cross-links as navigation objects so any "change this in settings" text is a link (Grey Eminence `SettingsNavigation.swift`). Settings search that hides non-matching fields (CORA `SettingsView.tsx:195-210`).

**Settings UI.** Tabs or panes (Pontifex, CORA 11, Grey Eminence 12), `⌘,`, toggle switches because "settings never use bare checkboxes" (CORA), step-ladder sliders for intervals, and defaults "chosen for the headline feature" (NommoBar).

---

## 11. UX conventions

**Theming.** Colours are tokens only; one CSS file per theme; a test fails on a missing token; the chosen theme is cached in `localStorage` to avoid a flash (Pontifex `index.css`, `themes.test.ts`, `theme-context.tsx`). CSS-variable themes, ten built-ins, clonable customs, per-tenant selection, cross-window sync via the `storage` event (CORA `src/lib/theme.ts`). Dark-only when the aesthetic demands it: Norse (Sleipnir `theme/tokens.css`), cyber (vpb `CyberTheme.swift`, "glow washes out on a light background"). The rule: "Chrome stays ink-neutral; the only chroma is live status" (CORA `styles.css:1`); production is red everywhere (Sleipnir); "colour only for state and Deploy/Destroy" (Warpcore).

**Naming motif.** Every app has one and states it in the README: Latin with a Varro quote (Pontifex), Norse with horse watermark and runic close buttons (Sleipnir), Star Trek (Warpcore), Razer green (NommoBar), archaeology (Riddle). Icons come from a script, not a file (Pontifex `icons/make-icon.py`, Sleipnir `scripts/gen_icons.py`).

**Keyboard.** One table drives both the handler and the help sheet "so a shortcut can't exist without being documented" (CORA `MainApp.tsx:2128`; Warpcore `useShortcuts.ts` and `ShortcutsHelp.tsx` kept in step). A `⌘K` palette where the object space is large (Warpcore). Space and Enter drive the hold button; Escape closes editors and tours.

**Errors.** Inline, not toasts: a single error banner inside the drawer (Sleipnir), an `ErrorBox` with a sign-in affordance (Pontifex `ui.tsx`), per-route error boundaries. Toasts only where a fleeting acknowledgement is right (Relevant Reviews). Disabled buttons carry a clickable `blockedReason` (Warpcore `deployRunner.ts:14`).

**Status.** A live credential badge in the header (Pontifex), an org lamp with countdown and holder badges (Sleipnir), a three-lamp `StatusStrip` (CORA), an ambient status dot per node (Warpcore).

**Empty states are instructions.** "NO SERVICES YET — RUN THE ORG SCAN FIRST" (Sleipnir); "Plug the speakers in via USB… Controls appear automatically." (NommoBar `PanelView.swift:154`); 35 `ContentUnavailableView`s (Grey Eminence).

**Modals only for tasks with progress.** Everything else confirms in place (Warpcore `DESIGN_GUIDANCE.md §7`; Sleipnir).

**Tooltips and copy.** `tip()` sets `data-tip` and `aria-label` together (CORA `Tooltip.tsx:22`); 118 `.help()` tooltips (Grey Eminence). Copy buttons write HTML and plain text (Grey Eminence `CopyButton`).

**Layout.** Rail plus slide-out drawer, with the drawer growing the native window by 680 px (Sleipnir `drawerWindow.ts`); rail plus detail tabs plus assistant panel plus drawers (CORA); `NavigationSplitView` with coloured SF Symbols (Grey Eminence). Per-window zoom with layout correction (CORA `zoom.ts`). Text size preference via `dynamicTypeSize` (Grey Eminence).

**Primitives.** A small in-house set per app: `Slab`, `Chamfer`, `SegmentedMeter` "reads as instrumentation" (Sleipnir); `Modal`, `Segmented`, `StepSlider`, `Badge`, `EmptyState`, `ErrorBox`, `Note`, `CopyButton` (Pontifex `ui.tsx`).

**One moment of delight.** Fireworks when an analysis lands (CORA `CalloutApp.tsx`). Once, small, tied to a real event.

---

## 12. Data, state, migrations, backups

**Choose storage by shape.** JSON files with caps for small state (Pontifex, Sleipnir); SQLite per tenant for relational or large data (CORA `store.rs`); SwiftData with a separate SQLite for embeddings (Grey Eminence `EmbeddingStore.swift`). Riddle plans rusqlite with FTS5 and AI traces in an attached, compressible database.

**Atomic writes always.** Temp file plus rename with the pid in the temp name; a mutex per shared file; a line-preserving editor for co-managed files that "never rewrites either file wholesale", stated as "a hard requirement, not an aspiration" (Sleipnir `atomic_file.rs`, `aws_config_editor.rs:22-48`, `docs-src/aws-files.md`).

**Migrations.** Versioned schema enums, 23 of them (Grey Eminence `SchemaVersions.swift`), with the house rule "Versioned schemas only" and "No backwards-compat shims, rely on git history". Additive `ALTER TABLE` with ignored errors plus data backfill (CORA `store.rs:129-160`). Serde defaults plus compatibility tests. In-code migrations for keychain layout and refcount backfill (Sleipnir `state.rs:71-86`). The legacy single database is renamed to a permanent `legacy-backup.sqlite` before splitting per org (CORA `orgs.rs:152-162`). Regrets: an explicit SwiftData migration plan removed after a macOS 26 crash, and "default-literal initialization crashes migrations" (Grey Eminence `GreyEminenceApp.swift:63-77`). Gap: no app stamps a schema version into its settings file.

**Backups.** A daily rolling store backup including WAL and SHM with 7-day retention (Grey Eminence `StoreBackupService.swift`); a `DatabaseErrorView` reset path; a recording lock-file sidecar for forensic recovery; re-processing checkpoints; disk-space watermarks with a 10 GB warning. None of the Tauri apps has backups yet.

**Caches.** A generic 30-day `TtlCache` pruned on load and write (Pontifex `ttl_cache.rs`); an LRU with size caps (event samples: 200 per type, 500 types, 32 MB); auto-save with own-save echo suppression (Warpcore `projectIO.ts:48-56`).

**Maintenance.** A 24 h-throttled idempotent `MaintenanceService` (Grey Eminence); "older than N days" clear actions in Storage settings (Warpcore).

**Export and import.** Registry export to a repo file format (Pontifex); Obsidian markdown, PDF and HTML reports (Grey Eminence); a manifest format shared with the CLI (Relevant Reviews); session restore of open items and viewed-state hashes (Relevant Reviews `session.rs`, `viewed_state.rs`).

---

## 13. Testing and quality

**Rust.** Unit tests against an in-memory store (CORA `Store::open_in_memory`, about 100 tests). Live `#[ignore]` integration tests whose header states the claim they prove, such as "Does striping actually widen the sample?" (Pontifex `src-tauri/tests/scan_stripes_live.rs`). Parity tests between a UI list and a Rust enum (Pontifex `tests/format_picker_parity.rs`). A hang-reproduction test for a panic inside a command (Pontifex `tests/infer_hang.rs`). Config-compatibility regression tests. Sentence-style test names (Sleipnir).

**Swift.** 65 XCTest files; a `TestEnvironment.isRunningTests` guard so tests never touch live data; opt-in real-audio diagnostics behind an env var or marker file (Grey Eminence `WhisperConcurrencyBenchmark.swift`). Golden and round-trip protocol tests (NommoBar `Tests/RazerProtocolTests`).

**Python.** 533 pytest functions; moto end-to-end deploys; frozen-binary guard tests that AST-check for PyInstaller pitfalls (Warpcore `tests/test_packaging.py`); a version test that asserts the Python version is printed.

**Fixtures.** A structure-preserving synthetic fixture derived from real data (Sleipnir 166-account `discovery_fixture.txt`); a parity corpus of 292 real schemas (Pontifex); a scenario mock server (gitf-gui).

**Frontend.** vitest for pure utilities and reducers only (Warpcore 117 cases; Pontifex 14 files including a themes token test); `oxlint`.

**Manual.** "Drive the actual flow in the running app — type checks don't catch broken layouts" (Grey Eminence CONTRIBUTING); a `say`-driven fake meeting script (`Scripts/test-conversation.sh`); a manual drive list in the UX plan (Warpcore `UX_PLAN.md §4`).

**Regrets.** Tests run only on tag pushes in every Tauri app; no PR CI; sparse frontend tests; no linter in several.

---

## 14. Docs and process

**README as the manual.** Sections that have worked: What it does, Requirements, Install, First run, Safety, Vocabulary, Architecture, Themes, Developer, Logging, Packaging, Status, Roadmap (Pontifex; Sleipnir). A one-line Status such as "Early but daily-driven. Signed and notarized." A "Why?" section with a quantified hook (Relevant Reviews). An etymology block for the name. Screenshots (Sleipnir); most repos lack them.

**Roadmaps that say no.** Ranked, effort-costed items with rationale and an explicit "Not planned" list (Pontifex `docs/watch-roadmap.md`). Decision docs with a decision table, phases, rules and risks (`docs/jira-integration.md`). Design guidance in numbered sections with status stamps and "[Done since: …]" notes (Warpcore `DESIGN_GUIDANCE.md`). A UX plan shaped verdict → what is wrong → plan → verification → dated follow-ups (`UX_PLAN.md`). `PLAN.md` with completed work first.

**Rationale where it lives.** Module `//!` headers as ADRs (CORA `health.rs`, `orgs.rs`, `activity.rs`, `usage.rs`). Comments that cite dated production incidents. Explained version pins (Warpcore `pyproject.toml:21-25`). "THE single definition" comments on constants (Sleipnir `constants.ts:8`). Troubleshooting tables recorded verbatim because "errors name something other than the cause" (vpb README). Protocol docs with a per-command verification legend and a dated capture log (NommoBar `docs/PROTOCOL.md`).

**House rules in CONTRIBUTING.** "never Combine", "Versioned schemas only", "No backwards-compat shims", "Comments only when the WHY is non-obvious", "One logical change per commit" (Grey Eminence). Flutter-era rules survive in `client_merger/CLAUDE.md`: extract UI into `_build` methods; catch specific exceptions with user-friendly messages.

**Claude Code integration.** A PreToolUse hook for version discipline (Grey Eminence `.claude/hooks/`); `.claude/settings.local.json` permission scoping; project memory notes for tooling gotchas (Sleipnir's `bun --hot` stale-bundle note). Regret: `CLAUDE.md` empty (Warpcore) or absent (most).

**Commits.** Full-sentence subjects (vpb, Warpcore) or lowercase conventional prefixes with narrative subjects (Sleipnir). "Name an error by the domain it came from, not by its number alone" is a commit subject in vpb.

**Public repo hygiene.** `SECURITY.md`, `CONTRIBUTING.md`, issue templates, a PR template with Summary, Test plan and Screenshots (Relevant Reviews).

---

## 15. Stack defaults

### Tauri 2 (Rust core, React UI)

- Rust: `tokio`, `rusqlite` (bundled, FTS5; `bundled-sqlcipher` when encryption at rest matters), `keyring` with `apple-native` and `windows-native`, `reqwest` with `rustls`, `serde`, `ts-rs`, `objc2` for macOS niceties.
- Plugins: `tauri-plugin-log`, `tauri-plugin-window-state` (minus VISIBLE), `tauri-plugin-dialog`, `tauri-plugin-opener`, `tauri-plugin-updater`, `tauri-plugin-autostart`, `tauri-plugin-notification` with the Swift helper fallback on macOS 26.
- Config: strict CSP; minimal capabilities per window; `require_main` on credential commands; close-to-hide; `build.rs` stamp; `bundle.targets` limited to what CI can actually produce.
- Frontend: React 19, Zustand, Radix plus Tailwind v4 plus lucide or a small in-house primitive set, vitest, oxlint; Vite or Bun.
- Release: tag-driven CI as §3, Homebrew tap, updater artifacts with minisign, stable-name asset.

### Swift macOS

- SwiftUI with an AppKit delegate, Swift 6 strict concurrency, SwiftData (versioned schemas) or GRDB, `os.Logger` plus a rotating file log, Sparkle 2, KeychainAccess, `SMAppService`, `MenuBarExtra` or `NSStatusItem` with a manual `NSPanel`, XcodeGen with an entitlement-restore step, XCTest or Swift Testing, `notarytool`, staple, `create-dmg`.

### Companion CLI or sidecar

- Python: Click, Rich, Pydantic v2, hatchling, uv; or Rust: clap. JSONL logs to a project-local dot directory with a home fallback; `--progress` NDJSON with plan first; `--dev`; stderr for humans, stdout for machines; `--yes` implied by `--progress`. PyInstaller one-file sidecar with a stripped-env smoke test, no UPX on macOS. An MCP server exposing read-only actions first.

---

## 16. Checklists

### Day 0, before feature work

- [ ] Name, motif, icon script, README skeleton with a Status line and etymology.
- [ ] Core and adapter split; typed `{kind, message}` errors; one instrumented invoke wrapper; a mirrored command surface in TypeScript.
- [ ] Action allowlist that clicks, CLI and any agent share.
- [ ] Logging plugin, fixed category vocabulary, `Timed` guard, never-log-arguments rule, webview errors forwarded, in-app tail.
- [ ] Settings file with serde defaults, atomic writes, a compatibility test, and a separate file for high-churn state.
- [ ] Dev and prod data split keyed on the build, DEV badge, build stamp in the title.
- [ ] Keychain wrapper with native features on; strict CSP; per-window capabilities; `require_main`.
- [ ] Developer toggle and pane: runtime, storage with reveal and clear, log viewer, in-flight calls, copy diagnostics.
- [ ] Test harness: in-memory store, one fixture derived from real data, vitest for reducers, sentence-style test names.

### Before the first release

- [ ] Tag-driven CI: version equality check, tests, sign and notarize, hard gate, Homebrew cask bump, idempotent release, stable-name asset.
- [ ] Updater wired with a silent check and a banner state machine; updater key custody written down.
- [ ] CHANGELOG bundled and rendered; What's New; version-bump hook.
- [ ] Audit store, soft delete with clash-refusing restore, confirmation tiers re-checked in the backend, preview or diff before every write.
- [ ] Honest progress: named steps, minimum visible time, cancel where the work can stop.
- [ ] First run bootstrapped from existing tool config; a tour or help sheet; troubleshooting section naming the log location and what logs contain.

### Before it becomes a daily driver or goes to another person

- [ ] Backups with retention and a reset path; versioned migrations; disk-space watermarks.
- [ ] Sleep and wake detection, focus-triggered refresh, backoff, circuit breaker, quiet offline behaviour.
- [ ] Launch at login with read-back; tray or menu bar with a reset-position action; notifications with a tested click path and a Test button.
- [ ] Redacted diagnostics bundle export; one-sentence privacy statement of what leaves the device.
- [ ] AI cost ledger with unpriced reported as unpriced; prompt overrides; an experiments or replay tool.
- [ ] PR CI, not only tag CI; lint; parity tests between UI lists and backend enums.

### Before it is public

- [ ] `SECURITY.md`, `CONTRIBUTING.md`, issue and PR templates; docs site with a link checker; screenshots; a "Not planned" list.
- [ ] Any platform verification (Google restricted scopes, Apple entitlements requests) budgeted in time and money.

---

## 17. Recurring regrets

Each of these was recorded in at least one repo. Treat them as pre-emptive fixes.

- **Versions duplicated across files** and releases shipped without notes (Warpcore; Grey Eminence 0.33 to 0.47). Fix: one source, a CI equality check, a bump hook.
- **No updater** leaves users stranded on old builds (Sleipnir FAQ). Fix: wire the updater before the first cask.
- **Logs without rotation** reached 735 MB (Grey Eminence); logs carry account IDs and ARNs with no redaction (Sleipnir). Fix: size cap on day one, a redaction pass before export.
- **`csp: null`** (Sleipnir, Warpcore); Electron `nodeIntegration` and `webSecurity` off with credentials in `localStorage` (evalu8); a committed `.env` (office_time). Fix: strict CSP and keychain from the template.
- **`keyring` default in-memory mock** lost every refresh token (Sleipnir); keychain prompts on every unsigned rebuild (Pontifex); TCC grants reset without a stable signing identity (Grey Eminence). Fix: native features on, sign dev builds.
- **`xcodegen generate` wipes entitlements**; a SwiftData migration plan crashed on macOS 26; default-literal initialisers crash migrations (Grey Eminence). Fix: restore step in CI, versioned schemas, no default literals in models.
- **Fake progress**: "a green animation while nothing happens" (Warpcore UX plan); a notification API that "reports success and shows nothing" on macOS 26 (Pontifex). Fix: progress only from real events, a Test button for every notification path.
- **Tests only on tag pushes, no PR CI, sparse frontend tests** (all Tauri apps).
- **Unwired settings toggles** for launch at login and the menu-bar icon (Grey Eminence). Fix: read back from the system after every write.
- **Stale docs** claiming features that were mocks (Warpcore `docs/task-13-...`); hardcoded default org and profile (CORA `orgs.rs:22`, `bedrock_probe.rs`).
- **Tooling traps**: `bun --hot` serves stale bundles after structural changes (Sleipnir memory note); Tauri v1 environment names "silently produced a minified, sourcemap-less bundle" (Warpcore `vite.config.ts:4-9`); `exit()` raises `NameError` in a frozen binary (`test_packaging.py:32`).
- **A panic inside a command leaves the IPC call pending forever** (Pontifex `tests/infer_hang.rs`). Fix: catch at the command boundary, time out on the TypeScript side.
- **A `.gitignore` entry for `logs` hid a feature directory** from git and Tailwind (Pontifex `README.md:781`).
- **Two automations fighting over one file or mailbox** (Riddle review, SaneBox and Superhuman). Fix: attribute external changes before treating them as user intent.

---

## Appendix: source index

Files worth opening first when lifting a pattern.

| Concern | File |
|---|---|
| Instrumented invoke | Pontifex `src/lib/ipc.ts`; CORA `src/lib/ipc.ts` |
| Typed errors | Pontifex `src-tauri/src/error.rs` |
| Log categories and `Timed` | Pontifex `src-tauri/src/logging.rs` |
| Structured log payloads and viewer | Sleipnir `src-tauri/src/applog.rs`, `src/components/DeveloperTab.tsx` |
| Swift logging with rotation | Grey Eminence `Services/Logging/LogManager.swift` |
| Developer pane | Pontifex `src/features/developer/developer-page.tsx`; Grey Eminence `Features/Settings/DeveloperSettingsView.swift` |
| Context inspector | CORA `src-tauri/src/analysis/types.rs`, `src/components/analysis/ContextDrawer.tsx` |
| Experiments lab | CORA `src-tauri/src/experiments.rs` |
| Audit with undo | CORA `src-tauri/src/store.rs`, `commands.rs`; Warpcore `src/warpcore/audit/` |
| Diff-first confirm, backend re-check | Pontifex `src/features/schemas/confirm-save-dialog.tsx`, `src-tauri/src/commands/schemas.rs` |
| Press and hold | Sleipnir `src/components/HoldButton.tsx` |
| Project lock | Warpcore `visual-builder/src/components/ProjectLock.tsx` |
| Soft delete and restore | Sleipnir `src-tauri/src/config/schema.rs`, `config/mod.rs` |
| Persistence gate | Grey Eminence `Services/Persistence/PersistenceGate.swift` |
| Agent and UI parity | CORA `src-tauri/src/analysis/chat.rs` |
| Bounded tools and caching | CORA `src-tauri/src/analysis/tools.rs`, `engine.rs` |
| Circuit breaker | CORA `src-tauri/src/health.rs` |
| Usage ledger | CORA `src-tauri/src/usage.rs`; Grey Eminence `Services/AI/AIUsage.swift` |
| Prompt overrides | Grey Eminence `Services/AI/PromptStore.swift`; CORA `src/components/PromptEditor.tsx` |
| Keychain wrapper | Pontifex `src-tauri/src/keychain.rs`; Sleipnir `src-tauri/src/secrets.rs` |
| Capabilities and `require_main` | CORA `src-tauri/capabilities/*.json`, `src-tauri/src/commands.rs` |
| Atomic file writes | Sleipnir `src-tauri/src/atomic_file.rs`; Pontifex `src-tauri/src/settings.rs` |
| Co-managed file editor | Sleipnir `src-tauri/src/config/aws_config_editor.rs` |
| Poll loop | CORA `src-tauri/src/github/poller.rs`; Pontifex `src-tauri/src/watch/runtime.rs` |
| Sleep detection | Sleipnir `src-tauri/src/lib.rs`; Grey Eminence `App/AppLifecycleCoordinator.swift` |
| Notifier helper | Pontifex `src-tauri/notifier/main.swift`, `src-tauri/src/watch/notify.rs` |
| Notification click claim | CORA `src-tauri/src/notify.rs` |
| One critical alert | vpb `macos/VpbApp/CriticalAlert.swift` |
| Menu bar restraint | vpb `macos/VpbApp/MenuBarGauge.swift`, `UsageModel.swift` |
| Login item with read-back | vpb `macos/VpbApp/SettingsModel.swift` |
| Hot-plug device handling | NommoBar `Sources/NommoBar/DeviceManager.swift` |
| Off-screen window rescue | CORA `src-tauri/src/lib.rs` |
| Tours | Sleipnir `src/tour/` |
| Docs site | Sleipnir `scripts/build-docs.ts`, `scripts/check-docs-links.ts` |
| In-app help window | Grey Eminence `Features/Help/HelpDocsView.swift` |
| What's New and NEW badges | Grey Eminence `Features/WhatsNew/` |
| Settings cross-links | Grey Eminence `Features/Settings/SettingsNavigation.swift` |
| Token-only themes with test | Pontifex `src/index.css`, `src/theme/themes.test.ts`; CORA `src/lib/theme.ts` |
| Shortcut table as help | CORA `src/windows/MainApp.tsx`; Warpcore `src/hooks/useShortcuts.ts` |
| Blocked reason on buttons | Warpcore `visual-builder/src/utils/deployRunner.ts` |
| Activity bar | Grey Eminence `Services/TransientActivityCoordinator.swift` |
| NDJSON progress | Warpcore `src/warpcore/cli/run_progress.py`, `visual-builder/src/utils/deployEvents.ts` |
| Store backup | Grey Eminence `Services/Persistence/StoreBackupService.swift` |
| Versioned schemas | Grey Eminence `Services/Persistence/SchemaVersions.swift` |
| Per-tenant SQLite | CORA `src-tauri/src/orgs.rs` |
| TTL cache | Pontifex `src-tauri/src/ttl_cache.rs` |
| Release CI (Tauri) | Sleipnir `.github/workflows/release.yml`; Pontifex `.github/workflows/release.yml` |
| Release CI (Swift, Sparkle) | Grey Eminence `.github/workflows/release.yml`, `docs/auto-update-playbook.md` |
| Updater banner | Relevant Reviews `app/src/components/UpdateBanner.tsx` |
| Signing script for extensions | vpb `macos/scripts/sign-and-notarize.sh` |
| Version-bump hook | Grey Eminence `.claude/hooks/check-version-bump.sh` |
| Build stamp | Warpcore `visual-builder/src-tauri/build.rs` |
| Dev/prod path split | Sleipnir `src-tauri/src/paths.rs` |
| Sidecar CLI | Warpcore `visual-builder/src-tauri/src/main.rs`, `packaging/build.py` |
| Frozen-binary guard tests | Warpcore `tests/test_packaging.py` |
| Live tests with claims | Pontifex `src-tauri/tests/*.rs` |
| UI protocol with mock server | gitf-gui `GAME_UI_PROTOCOL.md`, `mock_server/server.py` |
| Roadmap with "Not planned" | Pontifex `docs/watch-roadmap.md` |
| Design guidance with status stamps | Warpcore `visual-builder/DESIGN_GUIDANCE.md`, `UX_PLAN.md` |
| Action layer, provenance, undo, inspectors (design) | Riddle `ARCHITECTURE.md §8, §47-51, §67-90`; `ARCHITECTURE-REVIEW.md` |
