# Troubleshooting

## Where things are

| what | prod | dev build |
|---|---|---|
| data (stores) | `~/Library/Application Support/dev.codenaked.riddle/data/` | `~/Library/Application Support/dev.codenaked.riddle.dev/data/` |
| settings | `.../dev.codenaked.riddle/config/settings.toml` | `.../dev.codenaked.riddle.dev/config/settings.toml` |
| logs | `~/Library/Logs/dev.codenaked.riddle/riddle.log` | `~/Library/Logs/dev.codenaked.riddle.dev/riddle-dev.log` |
| keychain service | `dev.codenaked.riddle` | `dev.codenaked.riddle.dev` |

`RIDDLE_DATA_DIR` overrides the root for all of these; `RIDDLE_LOG` sets the
level for one launch (`trace`, `debug`, `info`, `warn`, `error`).

Developer Mode → Storage has "Reveal in Finder" for each of these.

## What logs contain

Identifiers, counts and timings: message ids, thread ids, Dig ids, job ids,
Action names, latencies, HTTP status codes, error kinds. Logs never contain
subjects, bodies, addresses, names, tokens or API keys. Structured payloads
are appended as ` ||PAYLOAD||{json}` and carry the same identifier-only
rule. "Copy diagnostics" in Developer Mode produces a redacted block that is
safe to paste into an issue.

## Common situations

**"Degraded" on a Dig.** The provider, the judge or the LLM is unavailable.
Nothing is hidden while degraded; mail that could not be judged is
Unresolved. Developer Mode → Probes runs each dependency outside the
pipeline and shows the error kind.

**"Catching up" after sleep.** Riddle only processes mail while the machine
is awake. The Task Centre shows the backlog and the summary says whether
processing is complete.

**Store fault banner.** Three consecutive write failures paused Sift. The
log has the call site. Free disk space or repair the file, then clear the
fault from the banner; the persistence gate resumes on the next successful
write.

**Full resync.** Gmail's history cursor expired. Riddle lists messages again
newest first and reconciles labels against its expected projection,
reporting differences in Activity instead of overwriting.

**Connecting a mailbox.** `docs/GMAIL_SETUP.md` covers the Google Cloud
console steps, the Internal-vs-External choice and the sign-in failures worth
recognising.

**Workspace administrator blocks Riddle.** The OAuth error is
`admin_policy_enforced`. Ask the administrator to allow the app's client id
for the `gmail.modify` scope, or use a personal account.
