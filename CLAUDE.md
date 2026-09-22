# Riddle — rules for agents working in this repository

Read `ARCHITECTURE.md` for the design and `APPLICATION_FEATURES.md` for the
house conventions. `ARCHITECTURE-REVIEW.md` records why the design is the way
it is. The rules below are the ones that bite when ignored.

## Shape

- The Rust workspace in `crates/` is the application. `apps/desktop` (Tauri +
  React) is a client of it, as are `riddle-cli` and `riddle-mcp`.
- **Actions are the capability boundary** (`ARCHITECTURE.md` §8). UI commands,
  CLI verbs, Director tools and MCP tools all call `riddle-actions`. Nothing
  else mutates the store on behalf of a user or an agent.
- Tauri commands are thin: deserialize, call an Action or a read service,
  serialize. No business logic in `src-tauri`.
- Types that cross the IPC boundary live in `riddle-core` and derive `ts_rs::TS`.
  Run `cargo test -p riddle-core` to regenerate `apps/desktop/src/bindings/`.
- One crate per bounded concern, named `riddle-<concern>`. A crate may depend
  on `riddle-core`, `riddle-errors`, `riddle-store` and `riddle-observability`
  freely. Cross-domain dependencies go through traits defined in `riddle-core`.

## Non-negotiables

- Every fallible function returns `riddle_errors::Result<T>`; the error kinds
  are the taxonomy in `ARCHITECTURE.md` §8. Do not add string-only errors.
- Logs carry identifiers only: message ids, thread ids, Dig ids, counts,
  timings. Never subjects, bodies, addresses, names, tokens or API keys.
  Use the `riddle_observability` macros with a category from its vocabulary.
- Bookkeeping never fails the work. A failed log line, usage row or Activity
  write is reported and swallowed; the operation it describes still succeeds.
- Every message has exactly one disposition; `Unresolved` is one of them.
  When a judge or LLM is unavailable, the answer is `Unresolved`, never a
  hidden message.
- The Never Hidden gate runs before any Signal, and no Signal or route
  overrides it.
- Tests never touch the real mailbox, the real keychain or the network. Use
  the in-memory store (`Store::in_memory()`), the fixture provider and the
  `NullJudge` / `ScriptedJudge` / `ScriptedLlm` doubles. Live tests are
  `#[ignore]` and say in a doc comment which claim they prove.
- Secrets are resolved at runtime (keychain, SSM) and never written to disk in
  this repository, never printed, never logged.

## Style

- Module headers explain the decision and the incident behind it, briefly.
- Sentence-style test names: `fn a_message_from_a_never_hidden_sender_cannot_be_spoil()`.
- `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt` must
  pass. Frontend: `pnpm lint` (oxlint) and `pnpm test` (vitest).
- Commit messages are written by the build coordinator with `scripts/commit.sh`;
  subagents do not commit.
