# Contributing to Riddle

House rules, in the order they usually matter.

## Where things go

- `crates/riddle-*` is the application. `apps/desktop` is a client of it.
  Tauri commands are thin; if a command has an `if`, it probably belongs in
  a crate.
- Every user-facing or agent-facing mutation is an Action in the crate that
  owns the domain, registered into `riddle-actions`. The UI, the CLI, the
  Director and MCP all go through `Runtime::invoke`.
- Types that cross the IPC boundary live in `riddle-core` and are exported
  with ts-rs. `cargo test -p riddle-core` regenerates
  `apps/desktop/src/bindings/`; CI fails if the bindings are stale.
- SQL lives in `riddle-store/src/repo/*`. Nothing else writes SQL.

## Rules that bite

- Logs carry identifiers and counts. Never subjects, bodies, addresses,
  names, tokens or keys. `riddle_observability::redact` exists for the cases
  in between.
- Bookkeeping never fails the work: Activity, usage rows, logs and events
  are best-effort around the operation they describe.
- Every message has exactly one disposition, and `Unresolved` is one of
  them. A missing judge or LLM degrades to `Unresolved`; it never hides mail.
- Never Hidden runs before any Signal and nothing overrides it.
- Tests never touch the real mailbox, keychain or network. Live tests are
  `#[ignore]` and their doc comment names the claim they prove.
- Secrets are resolved at runtime (keychain, SSM) and never written into
  this repository, printed, or logged.

## Workflow

- `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace --exclude riddle-desktop`; in `apps/desktop`:
  `pnpm lint`, `pnpm test`, `pnpm build`.
- Sentence-style test names: `fn a_never_hidden_sender_cannot_become_spoil()`.
- Module headers explain the decision and the incident behind it, briefly.
- Version lives in the workspace `Cargo.toml`; `scripts/version-check.sh`
  keeps `tauri.conf.json` and `package.json` equal to it, and a release tag
  must equal it too. Bump the version with every user-visible change.
- Commit messages describe the change; the build ledger trailer is added by
  `scripts/commit.sh` during the assisted build and is optional afterwards.
