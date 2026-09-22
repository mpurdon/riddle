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
set_disposition         { digId, messageId, disposition: Disposition, findType? }   (user correction)
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
save_settings           { settings: Settings }
add_fixture_dig         { name, email, seed?, count?, group?, privacy?, projection? }
remove_dig              { digId }
update_dig              { dig: Dig }
save_dig_group          { group: DigGroup }
arm_dev                 { armed: boolean }
refresh_now             { digId? }
cancel_job              { jobId }    pause_job { jobId }   resume_job { jobId }
inject_fixture_message  { digId, template?: string, from?, subject?, body? }   (Demo Mode)
apply_update_preview    { keep: boolean }
```

## Reads

| command | args | returns |
|---|---|---|
| `summary` | – | `Summary` |
| `task_centre` | – | `TaskCentre` |
| `list_digs` | – | `Dig[]` |
| `list_dig_groups` | – | `DigGroup[]` |
| `list_messages` | `{ digId?, filter: MessageFilter (spec), page: PageRequest }` | `Page<MessageSummary>` (across Digs when `digId` absent) |
| `get_message` | `{ digId, messageId }` | `Message` (`bodyHtml` sanitized, remote content stripped) |
| `get_thread` | `{ digId, threadId }` | `Thread` |
| `explain` | `{ digId, messageId }` | `Explanation` |
| `search` | `{ query, digId?, page }` | `Page<MessageSummary>` |
| `list_finds` | `{ status?, findType?, digId?, page }` | `Page<Find>` |
| `get_find` | `{ findId }` | `Find` |
| `list_activity` | `{ digId?, actor?, action?, since?, until?, page }` | `Page<Activity>` |
| `get_activity` | `{ activityId }` | `Activity` |
| `list_meshes` | `{ includeDeleted? }` | `Mesh[]` (latest version each) |
| `get_mesh` | `{ meshId, version? }` | `Mesh` |
| `mesh_versions` | `{ meshId }` | `Mesh[]` |
| `parse_mesh` | `{ text }` | `Mesh` (unsaved; throws `invalid` with line info in detail) |
| `simulate_mesh` | `{ text?, meshId?, version?, windowDays? }` | `SimulationReport` |
| `list_signals` | – | `Signal[]` |
| `signal_versions` | `{ name }` | `Signal[]` |
| `signal_evaluations` | `{ digId, messageId }` | `SignalEvaluation[]` |
| `list_strata` | – | `Stratum[]` |
| `list_conversations` | – | `Conversation[]` (no turns) |
| `get_conversation` | `{ conversationId }` | `Conversation` |
| `director_send` | `{ conversationId?, message, digIds? }` | `{ conversationId }` then `DirectorDelta`/`DirectorToolCall`/`DirectorDone` events |
| `list_expeditions` | – | `Expedition[]` |
| `get_expedition` | `{ expeditionId }` | `Expedition` |
| `list_drafts` | `{ digId }` | `Draft[]` |
| `get_settings` | – | `Settings` (spec) |
| `get_user_profile` | – | `UserProfile` |
| `privacy_statement` | `{ digId? }` | `string` |
| `read_trace` | `{ digId, traceId }` | `DecisionTrace` |
| `recent_traces` | `{ digId?, limit? }` | `DecisionTrace[]` |
| `usage_summary` | `{ sinceDays? }` | `UsageSummary` |
| `trace_stats` | `{ digId?, sinceDays? }` | `TraceStats` (spec) |
| `dev_info` | – | `DevInfo` (spec, exists) |
| `dev_armed` | – | `boolean` |
| `probe_judge` / `probe_llm` / `probe_provider {digId}` | | `{ ok, latencyMs, message }` |
| `read_app_logs` | `{ limit?, category?, level?, text? }` | `LogLine[]` |
| `log_categories` | – | `string[]` |
| `in_flight_calls` | – | `{ command, elapsedMs }[]` |
| `export_diagnostics` | – | `string` (redacted block) |
| `update_preview` | – | `UpdatePreview | null` (spec) |
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
  developerMode: boolean; logLevel: 'error'|'warn'|'info'|'debug'|'trace'
  notifyPriorityThreshold: number; shadowPeriodDays: number
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
