# IPC command surface

The contract between the React client and the Rust core. Reads are Tauri
commands; every mutation goes through one generic `invoke_action` command so
the Action layer (`ARCHITECTURE.md` §8) is the only path that changes state.
Types named here are the ts-rs bindings in `apps/desktop/src/bindings/`
unless marked *(spec)*, in which case the shape is given inline and the
frontend mirrors it in `src/lib/types.ts` until the binding exists.

All commands return `T` or throw `RiddleError {kind, message, detail}`.
Arguments are camelCase. Ids are strings.

## Mutations

| command | args | returns |
|---|---|---|
| `invoke_action` | `{ invocation: ActionInvocation }` | `ActionOutcome`; throws `needs_confirmation` with a `PendingAction` in `detail` |
| `list_actions` | – | `ActionMeta[]` |
| `list_pending` | – | `PendingAction[]` |

To confirm: re-invoke with `pendingId` set and the same `action`/`args`.

### Action names and args (all via `invoke_action`)

```
archive_messages        { digId, messageIds[] }
move_to_spoil           { digId, messageIds[] }
mark_contamination      { digId, messageIds[] }
restore_messages        { digId, messageIds[] }            (from Spoil/Contamination to Unresolved, records a correction)
resift                  { digId? }                         (user only; every message Riddle decided is sifted again with the current rules; your decisions are kept, earlier ones stay as history)
decide_again            { digId, messageIds[] }            (sends those messages back to Sift under the current rules; an open Find on one closes as Superseded and returns if it still is one; undo puts the earlier decision and the Find back)
apply_mesh_to_history   { meshId, windowDays? = 30 }       (TwoStep; the active Mesh's changed set from the window is sent back to Sift the same way; your decisions are kept; at most 2,000 messages; windowDays is clamped to 1–365 wherever it is read, including update_mesh's simulation)
set_disposition         { digId, messageId, disposition: Disposition, findType?, dueAt? }   (user correction; dueAt is RFC 3339 and in the future, put on the Find it opens or joins: "follow up by…"; a later call moves the date, undo puts it back)
label_messages          { digId, messageIds[], strata: string[] }
snooze_messages         { digId, messageIds[], until }
resolve_find            { findId }
dismiss_find            { findId }
snooze_find             { findId, until }
retype_find             { findId, findType }
undo_activity           { activityId }
create_mesh             { text, scope?: MeshScope, status?: 'draft'|'shadow'|'active' }
update_mesh             { meshId, text, changeNote? }
set_mesh_status         { meshId, status }
delete_mesh             { meshId }        restore_mesh { meshId }
propose_signal          { signal: Signal (name, definition, primitive, choices) }
test_signal             { name, version?, definition?, sampleSize? }   → SignalTestReport (spec)
promote_signal          { name, version }
evaluate_signal         { digId, messageId, name, version }            → SignalEvaluation
rename_stratum          { stratumId, name }     merge_strata { fromId, intoId }   create_stratum { name, description }
create_draft            { digId, draft: Draft }   update_draft { digId, draft }   delete_draft { digId, draftId }
send_message            { digId, draftId }        (TwoStep; preview shows real recipients)
start_expedition        { goal, scope: ExpeditionScope, budget: ExpeditionBudget }
cancel_expedition       { expeditionId }
add_never_hidden        { entry }                 remove_never_hidden { entry }
save_user_profile       { profile: UserProfile }
save_settings           { settings: Settings }    (applies to the running app at once)
set_secret              { secret: "jev_api_key" | "gmail_client_secret", value }   (user only; stored in the keychain, never recorded)
clear_secret            { secret }                (user only)
import_gmail_client     { text }                  (user only; Google's client JSON: id to settings, secret to the keychain)
add_fixture_dig         { name, email, seed?, count?, group?, privacy?, projection? }
connect_gmail           { name?, group?, privacy?, projection?, loginHint?, digId? }   (with digId: sign an existing Dig in again; it keeps its history and runs a catch-up)
review_shadow           { digId, messageId, agree: bool, disposition? }   (Agree counts for the Shadow Mesh and marks the row; Disagree records your decision as a correction)
dismiss_pending         { pendingId, reason? }              (decline a proposal; the pending action is removed and Activity records it)
remove_dig              { digId }
update_dig              { dig: Dig }
save_dig_group          { group: DigGroup }
arm_dev                 { armed: boolean }
refresh_now             { digId? }                         → { digId, heard }; one account or all; the app turns it into a sync job, then Sift and projection follow
cancel_job              { jobId }    pause_job { jobId }   resume_job { jobId }
inject_fixture_message  { digId, template?: string, from?, subject?, body? }   (Demo Mode)
apply_update_preview    { keep: boolean }
export_meshes           { includeDisabled? }   → { text, count }
import_meshes           { text }               (Drafts, or a new version when the name exists; undo removes them)
unsubscribe             { digId, messageId }   (List-Unsubscribe one-click; external; no undo)
permanent_delete        { digId, messageIds[] } (TypeCount; no undo)
```

## Reads

| command | args | returns |
|---|---|---|
| `summary` | `{ digId? }` | `Summary`; with `digId` the numbers cover that one Dig |
| `task_centre` | – | `TaskCentre`; `judge` and `llm` carry each client's `ServiceHealth` (breaker, last success, last failure), `null` when not configured |
| `list_digs` | – | `Dig[]` |
| `list_dig_groups` | – | `DigGroup[]` |
| `list_messages` | `{ digId?, filter: MessageFilter (spec), page: PageRequest }` | `Page<MessageSummary>` (across Digs when `digId` absent) |
| `get_message` | `{ digId, messageId }` | `Message` (`bodyHtml` sanitized, remote content stripped) |
| `get_thread` | `{ digId, threadId }` | `Thread` |
| `explain` | `{ digId, messageId }` | `Explanation` |
| `search` | `{ query, digId?, page, shadowOnly? }` | `Page<MessageSummary>`; `shadowOnly` lists Shadow proposals (`shadow`, `shadowReview` on each row) |
| `priority_inbox` | `{ digId?, page }` | `Page<MessageSummary>`: the Priority inbox. Messages carrying a chosen Stratum, a chosen provider label (`STARRED`, `IMPORTANT`) or belonging to an open Find at or above the chosen priority, OR-combined; the criteria are `settings.priorityInbox`, read by the backend, so no filter is sent |
| `list_finds` | `{ status?, findType?, digId?, page }` | `Page<Find>` |
| `get_find` | `{ findId }` | `Find` |
| `list_activity` | `{ digId?, actor?, action?, since?, until?, page }` | `Page<Activity>` |
| `get_activity` | `{ activityId }` | `Activity` |
| `list_meshes` | `{ includeDeleted? }` | `Mesh[]` (latest version each) |
| `get_mesh` | `{ meshId, version? }` | `Mesh` |
| `mesh_versions` | `{ meshId }` | `Mesh[]` |
| `parse_mesh` | `{ text }` | `Mesh` (unsaved; throws `invalid` with line info in detail) |
| `simulate_mesh` | `{ text?, meshId?, version?, windowDays? = 90 }` | `SimulationReport` (windowDays clamped to 1–365) |
| `list_signals` | – | `Signal[]` |
| `compare_signal_versions` | `{ name, versionA, versionB }` | `{ compared, agree, disagree, agreement, samples[] }` |
| `signal_versions` | `{ name }` | `Signal[]` |
| `signal_evaluations` | `{ digId, messageId }` | `SignalEvaluation[]` |
| `list_strata` | – | `Stratum[]` |
| `list_conversations` | – | `Conversation[]` (no turns) |
| `get_conversation` | `{ conversationId }` | `Conversation` |
| `director_send` | `{ conversationId?, message, digIds? }` | `{ conversationId }` then `DirectorDelta`/`DirectorToolCall`/`DirectorDone` events |
| `director_retry` | `{ conversationId }` | `{ conversationId }`; drops the failed Director turns at the end and runs your last message again. A failed turn carries `failure { kind, retryAfterSecs? }`; the client counts down and retries by itself when the provider named a wait |
| `list_expeditions` | – | `Expedition[]` |
| `get_expedition` | `{ expeditionId }` | `Expedition` |
| `list_drafts` | `{ digId }` | `Draft[]` |
| `get_settings` | – | `Settings` (spec) |
| `get_user_profile` | – | `UserProfile` |
| `privacy_statement` | `{ digId? }` | `string` |
| `read_trace` | `{ digId, traceId }` | `DecisionTrace` |
| `recent_traces` | `{ digId?, limit? }` | `DecisionTrace[]` |
| `usage_summary` | `{ sinceDays? }` | `UsageSummary` |
| `trust_stats` | `{ digId?, sinceDays? }` | `TrustStats`: decisions, corrections, undos, what would have been archived, per-rule precision, and one offer when an account's last Spoil calls all stood |
| `trace_stats` | `{ digId?, sinceDays? }` | `TraceStats` (spec) |
| `dev_info` | – | `DevInfo` (spec, exists); `swallowed` is the swallowed-failure table: `{ category, site, count, lastAt, lastError }[]` |
| `dev_armed` | – | `boolean` |
| `probe_judge` / `probe_llm` / `probe_provider {digId}` | | `{ ok, latencyMs, message, health? }`; the judge and LLM probes add the client's `ServiceHealth` after the call |
| `read_app_logs` | `{ limit?, category?, level?, text? }` | `LogLine[]` |
| `log_categories` | – | `string[]` |
| `in_flight_calls` | – | `{ command, elapsedMs }[]` |
| `export_diagnostics` | – | the redacted diagnostics bundle, a JSON object: `{ exportedAt, redaction, runtime: DevInfo, taskCentre, traceStats, judge: { identity, health }, llm, digs: [{ id, name, provider, privacy, projection, disabled, status }], actions, settings (OAuth client as present-or-absent), swallowed, warnings }` |
| `update_preview` | – | `UpdatePreview | null` (spec) |
| `test_notification` | – | `{ sent }`; desktop only, shows a system notification |
| `list_shortcuts` | – | not a command; the table lives in the UI |

### Spec-only shapes

```ts
interface MessageFilter {
  disposition?: 'find'|'filed'|'spoil'|'contamination'|'unresolved'
  findType?: FindType; label?: string; query?: string; from?: string
  threadId?: string; since?: string; until?: string; stratum?: string
  unclassifiedOnly?: boolean
}
interface Settings {
  schemaVersion: number
  uiLevel: 'essentials'|'everything'|'developer'  // how much the app shows; the one switch
  developerMode: boolean  // derived: true exactly when uiLevel is 'developer'
  logLevel: 'error'|'warn'|'info'|'debug'|'trace'
  notifyPriorityThreshold: number; shadowPeriodDays: number
  priorityInbox: { strata: string[]; labels: string[]; minFindPriority: number | null; findTypes: FindType[] }
  judge: { provider: 'jev'|'null'|'scripted'; model: string; keySource: 'env'|'keychain'|'ssm'; ssmProfile?: string; ssmParameter?: string; apiKeyPresent: boolean }
  llm: { provider: 'bedrock'|'null'|'scripted'; profile: string; region: string; escalationModel: string; directorModel: string }  // default us.anthropic.claude-sonnet-4-5-20250929-v1:0 (Sonnet 5 is not enabled on the account)
  budgets: { dailyJudgeUsd: number; dailyLlmUsd: number; escalationPerDigUsd: number }
  gmail: { clientId: string; clientSecretPresent: boolean }
  autonomy: { default: Autonomy; perDig: Record<string, Autonomy>; perAction: Record<string, Autonomy> }
  projectionDefault: ProjectionMode; privacyDefault: PrivacyMode
  launchAtLogin: boolean; closeToHide: boolean
}
interface SignalTestReport { name; version; sampleSize; agreement; falsePositives; falseNegatives; uncertain; latencyMs; costUsd; distribution: Record<string, number>; samples: { messageId; digId; expected?; got; confidence }[] }
interface TraceStats { count; escalated; unresolved; avgTotalMs; avgJudgeMs; judgeTokens; judgeCostUsd; llmCostUsd }
interface UpdatePreview { fromVersion; toVersion; changedSignals: number; changedMeshes: number; report: SimulationReport }
interface DevInfo { build: BuildStamp; dev; os; arch; uptimeSeconds; dataDir; logDir; configDir; cacheDir; stores: {label; path; sizeBytes}[]; storeFault: string|null; logLevel }
```

## Events

One Tauri event, `riddle://event`, carrying a `RiddleEvent`. The client
refetches on signals; it never expects data volume in an event.

## Settings additions

`settings.notifications` (`NotificationSettings`): `mode` is `brief` (default) or
`stream`; `briefTimes` up to four local `HH:MM`; `immediateTypes` the Find
types that may interrupt at once when due within `dueWithinHours`;
`neverHiddenImmediate`. In brief mode everything else is said in a brief at
the chosen times. Notifications carry the Find's type and reason, never its
subject. The dock badge counts open Finds plus Unresolved.

`settings.priorityInbox` (`PriorityInboxSettings`): what the `priority_inbox`
read surfaces, OR-combined. `strata` are Stratum names; `labels` are provider
label ids verbatim (default `["STARRED"]`); `minFindPriority` (default 70) is
the floor for an open Find's priority; `null` or `0` means any open Find (TOML
has no null, so it is written and read back as `0`); `findTypes` limits which Find types count, empty
meaning every type. Saved with the rest of the settings through
`save_settings`; the Priority page saves on every change.

## Mesh header additions

`Autonomy: Observe | Suggest | Act` caps what one Mesh's decisions may do in
the mailbox regardless of the account's rung; `Trusted` is refused (an
account setting). An update without the line keeps the previous version's
cap. `Mesh.autonomy` carries it in the binding.

## Typed fields added on 26 September (replacing client-side guesswork)

- `MessageSummary.shadow` / `MessageSummary.shadowReview` (`'agreed' | 'disagreed' | null`):
  with `MessageFilter.shadowOnly: true` the list holds only messages with a
  Shadow proposal, shown as the proposal, each with the user's review.
- `Dig.mailbox` (`MailboxEffect { writes: nothing | labels | labels_and_archive, held: autonomy | dev_unarmed | null }`):
  what Sift does to that mailbox right now, from the projection gate itself.
- `DigGroup.usedBy`: the Meshes and Strata that name the group; while any
  exist, rename and delete are refused.
- `ActionOutcome.summary` / `ActionOutcome.affected`: the Activity sentence
  and the entities the Action touched, for the toast and for refreshing by
  entity kind.
- `Mesh.demoted` (`{ at, reversed, decided }`): set when reversals sent the
  version back to Shadow; cleared by a status the user chooses.
- `RulePrecision.reversedOften`: the demotion rule's verdict.

## Rules in words (`riddle_mesh::humanize`, ARCHITECTURE.md §97)

- `Route.words` / `Guard.words`: the rule as a sentence, written by the
  parser (`parse_mesh`, `create_mesh`, `update_mesh`) and backfilled on every
  Mesh read: "When payment related is very likely and requires action is
  likely, it is something to pay under Finance."
- A Mesh decision's `reason` is the matched route read against that message,
  with its actual scores: "It appears payment related is almost certain and
  requires action is likely, so it is something to pay under Finance." Lists
  and detail headers show it; the Why card shows `matchedRule`, the grammar,
  and the route walk with each clause's actual value. Decisions stored
  before this are rewritten in words once per Dig at boot, from their
  stored trace, with no judge call.
