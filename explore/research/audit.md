# Riddle desktop — usability audit from source

Scope: `apps/desktop/src` read in full (every page under `features/`, the shell, `components/`, `lib/shortcuts.ts`, `lib/useShortcuts.ts`, `lib/useListNav.ts`, `lib/actionHelp.ts`, `lib/selectors.ts`, `lib/useAction.ts`, `lib/stores.ts`, `lib/events.ts`, `index.css`), plus `README.md`, `ARCHITECTURE.md` (§2 product model, §38–42, §46–51), `APPLICATION_FEATURES.md` (identical to `~/Projects/application_features.md`, verified with `diff`), and `docs/GMAIL_SETUP.md`. No files in the repository were modified. Nothing here was verified by running the app; every claim is traced to a line in the source. Paths are relative to `apps/desktop/src/` unless they start with `crates/`, `docs/` or a top-level file. Where a statement is an opinion rather than a fact from the code it is marked **[opinion]**.

Severity uses Nielsen's scale: 1 cosmetic, 2 minor, 3 major, 4 catastrophic (blocks a task or destroys trust).

---

## 1. Information architecture

### 1.1 Rail and routes

The rail (`app/shell/Rail.tsx:21-53`) has four groups; only two have headings.

| Group heading | Rail label | Route(s) (`app/routes.tsx:28-48`) | What the page is for | Count badge |
|---|---|---|---|---|
| *(none)* | Summary | `/` | Morning card: caught up or catching up, counts as links, Director note, Shadow card, Attention Zero proof | — |
| | Finds | `/finds`, `/finds/:findId` | Master/detail of Finds grouped by type; Resolve / Snooze / Retype / Not important; Why? card; sources | open Finds (moss) |
| | Unresolved | `/unresolved` | Master/detail of mail Riddle could not decide; Find… / Filed / Spoil / Contamination / Never hide | unresolved (ochre) |
| | Inbox | `/inbox`, `/inbox/:digId/:messageId` | Chronological list with 7 filters; bulk Archive / Spoil / Stratum / Snooze / Contamination; message page | — |
| **Excavation** | Spoil | `/spoil` | Review list: Not Spoil, Never hide sender | — |
| | Contamination | `/contamination` | Review list: Restore, Never hide sender, Delete permanently… | — |
| | Strata | `/strata` | Table of categories: rename, merge, create | — |
| | Meshes | `/meshes`, `/meshes/:meshId` | Policy editor: DSL text, structure, history, preview, status | — |
| | Signals | `/signals`, `/signals/:name` | ML judge vocabulary registry: definition, calibration, compare, test, promote, propose | — |
| | Activity | `/activity` | Audit ledger with provenance and Undo | — |
| **Director** | Director | `/director`, `/director/:conversationId` | Chat with tool-call cards and proposals | pending proposals (ochre) |
| | Expeditions | `/expeditions`, `/expeditions/:expeditionId` | Durable agent tasks with budget | — |
| *(none)* | Settings | `/settings` (+ `#digs`, `#update` …) | Four tabs: General, Accounts, Providers, Mail rules | — |
| | Developer | `/developer/*` (only when `developerMode`) | 17 sub-panes | — |

Also in the top bar (`app/shell/TopBar.tsx:24-39`): Dig switcher, "Ask Riddle" search, DEV chip, Compose, Task Centre popover, `?`. Two overlays are not in the rail: the composer slide-over (`features/messages/composer.tsx:143`) and the tour (`components/Tour.tsx:71`).

Observations on the grouping:

- "Excavation" (`Rail.tsx:32`) is a grab-bag: two review trays (Spoil, Contamination), a taxonomy (Strata), two policy editors (Meshes, Signals) and the audit log (Activity). Nothing else in the app uses the word "Excavation". The grouping does not answer the question a new user has ("where do I look when Riddle got it wrong?" → Spoil/Contamination/Activity; "where do I change the rules?" → Meshes/Signals/Strata).
- Signals sits in primary navigation for every user although its page speaks of "Noul", "Platt scaling", "engineIdentity" and "held-out agreement" (`features/signals/SignalsPage.tsx:36,148-161`). **[opinion]** It belongs under Meshes or Developer.
- Accounts are called "Dig" in the switcher (`DigSwitcher.tsx:14,25` "All Digs"), the Summary ("across your 3 Digs", `lib/selectors.ts:152`), the Inbox empty state ("Connect a Dig in Settings", `InboxPage.tsx:156`) and the Compose warning ("Add a Dig before composing", `composer.tsx:54`), but "account" in Settings (tab "Accounts (N)", `SettingsPage.tsx:85`; "Add an account", `AddAccountWizard.tsx:95`; "Each account is its own Dig", `:100`). The wizard sentence is the only place the two are equated.

### 1.2 Same concept, different names

| Concept | Names the user meets | Where |
|---|---|---|
| Connected mailbox | Dig, account, Test mailbox, Fixture Dig | `Rail`/`Summary` vs `SettingsPage.tsx:85`; `AddAccountWizard.tsx:103`; mock summary "You added Fixture Dig" |
| Riddle writing to Gmail | "Changes in Gmail", Projection, "decisions not yet written to Gmail (Projection)", "Labels / Labels and archiving" | `accounts/model.ts:30-34`; `SummaryPage.tsx:90`; `selectors.ts:37` chip "Projection 41" |
| Undoing a classification | Not Spoil, Restore, Correct…, Decide, "Corrected" toast, set_disposition | `ReviewPage.tsx:74`; `MessagePage.tsx:105`; `UnresolvedPage.tsx:79-88`; `useAction.ts:188` |
| Telling Riddle a Find was wrong | "Not important" (dismisses the Find) vs "Not Important" (re-files the message as Filed) | `FindsPage.tsx:219` (`dismiss_find`) vs `MessagePage.tsx:137` + `Bits.tsx:281-285` (`set_disposition filed`) |
| The ML judge | Judge, Jev, "View Jev", "Decided on Signals", "Jev (TypeSafe)" | `SettingsPage.tsx:249`; `Bits.tsx:269`; `selectors.ts:200` |
| Unclassified mail | Unsifted, "Not yet sifted", "Not sifted yet" | `selectors.ts:241`; `InboxPage.tsx:33`; `MessagePage.tsx:141` |
| Category | Stratum, Strata, Label (Gmail), "Riddle/<Stratum> label" | Inbox has separate "Stratum" and "Label" filters `InboxPage.tsx:128-129`; help `actionHelp.ts:85` |
| Filed | Filed, Archive ("Records it as Filed"), "Filed into your Strata" | `actionHelp.ts:83`; `SummaryPage.tsx:77` |
| Escalation | Escalate, Unresolved (`SimulationTable.tsx:10` relabels `unresolved` as "Escalate"), "LLM second opinions", "Escalate to LLM" | `MeshesPage.tsx:219`; `SettingsPage.tsx:151` |
| Shadow | Shadow, Shadow Period, "Shadow proposals", "Would be Spoil", "Shadow agreement", status chip `shadow` | `SummaryPage.tsx:106-111`; `InboxPage.tsx:99`; `Bits.tsx:28`; `MeshesPage.tsx:103` |
| Mesh soft delete | Delete, "Recently Deleted", status `deleted`, "Mesh moved to Recently Deleted" | `MeshesPage.tsx:116,174`; `useAction.ts:208` |

### 1.3 Vocabulary load

Distinct domain nouns a user meets in the primary UI (not Developer Mode): Dig, Dig Group, Sift, Find, the eight Find types (Respond, Decide, Review, Do, Pay, Attend, Follow Up, Know), Disposition, Filed, Spoil, Contamination, Unresolved, Unsifted, Stratum, Label, Mesh, Route, Guard, Scope, Shadow / Shadow Period, Signal, Signal version, Noul / Choice, Calibration, Judge / Jev, LLM / Escalation, Director, Proposal, Expedition, Never Hidden, Autonomy (Observe, Suggest, Act, Trusted), Changes in Gmail / Projection, Privacy mode (Local only, Cloud judgment, Cloud reasoning), Task Centre, Activity, actor (You, Director, Sift, Mesh, Automation, System), Correction, Context slice, User Profile, Attention Zero, Dig status (Synced, Catching up, Sifting, Projection, Degraded, Shadow day N, Not signed in), Arming, Excavation, Recently Deleted, Temporal Sift, Priority (P82), Trace.

That is **about 45 distinct nouns**, roughly **16 of which are needed before the daily loop makes sense** (Dig, Sift, Find + types, Unresolved, Filed, Spoil, Contamination, Stratum, Mesh, Never Hidden, Director, Task Centre, Autonomy, Changes in Gmail, Activity, Shadow). Nothing in the app defines them in one place; definitions are scattered across page subtitles (`ReviewPage.tsx:69`, `MeshesPage.tsx:92`, `SignalsPage.tsx:53`) and the three-step tour.

### 1.4 Where the metaphor helps and where it hides meaning

- **Find** works: "something we found that deserves attention" reads naturally, and the eight types are plain verbs.
- **Dig** works after one sentence; the wizard provides it (`AddAccountWizard.tsx:100`).
- **Sift** works ("Sifting 41" chip, `selectors.ts:35`).
- **Spoil** does not carry "low value, kept, searchable" on its own. In archaeology spoil is the excavated waste heap; to most users "spoil" reads as spoiled/rotten or spoiler. The rail icon is `Trash2` (`Rail.tsx:34`) and the Inbox "Spoil" button also uses `Trash2` (`InboxPage.tsx:108`), which says "deleted" — the opposite of what the copy insists ("Spoil is not deleted", `ReviewPage.tsx:89`). Severity 2.
- **Contamination** reads as "toxic"; it covers ordinary spam too. The `AlertTriangle` icon is reasonable. The distinction from Spoil is explained well on the Contamination page (`ReviewPage.tsx:91`).
- **Strata** actively misleads. Geological strata are ordered, non-overlapping layers; Riddle's Strata are overlapping tags ("A message can belong to several Strata", `Bits.tsx:98`). Users will expect one Stratum per message. Severity 2.
- **Mesh** is meant as a sieve mesh, but the rail uses a `Network` icon (`Rail.tsx:37`), so the word reads as "network mesh". Nothing in the app says "mesh as in sieve". Severity 2.
- **Expeditions** is guessable ("a longer trip"), and the subtitle "Delegated work with a budget. Results become Finds." (`ExpeditionsPage.tsx:37`) closes the gap.
- **Director** is not archaeology-specific but is fine; the risk is that it sounds like a person who directs the user.
- **Signals**, **Escalate**, **Projection**, **Arming**, **Judge** are not part of the metaphor at all and read as engineering terms.

---

## 2. User flows, step by step

### (a) First run, no accounts

1. App opens on `/` (Summary). `useSummary` (`lib/queries.ts:9`) is global; with no Digs the backend reports caught-up and zero counts, so `summaryCopy` (`selectors.ts:146-157`) returns `attentionZero = true`. The headline is **"Nothing needs your attention."**, the evidence line is "Synced just now. 0 messages Filed, 0 in Spoil since 9:41", and the only buttons are **Review Spoil** and **Undo** (disabled: "Nothing to undo yet") (`SummaryPage.tsx:56-65`). There is no "Add an account" call to action anywhere on the Summary; the only zero-account gate in the app is inside Settings → Accounts (`accounts/AccountsTab.tsx:80-88`) and the Inbox empty state (`InboxPage.tsx:156-157`). Severity 4 for a first-run screen that asserts the product's core promise on an empty database.
2. Simultaneously the tour appears bottom-centre (`components/Tour.tsx:45-70`): "Tour · 1 of 3", title **Finds**, body "Riddle does not show you an inbox of 847 unread messages…", buttons **Show me** / **Next** / **Skip (Esc)**. "Show me" navigates to `/finds`, which on first run is the empty state "No open Finds." Step 2 (Spoil) navigates to an empty Spoil; step 3 (Meshes) to "No Meshes yet." The tour demonstrates three empty pages.
3. Tour step 2 promises "a daily digest catches anything misfiled" (`Tour.tsx:19`). No digest exists in the UI; the Settings header comment records that the digest control was removed because it "saved nothing" (`SettingsPage.tsx:11-14`). Severity 3 (honesty).
4. The task-centre trigger reads "Caught up" with a green dot (`TaskCentrePopover.tsx:33-37`, `selectors.ts:79`) — again asserting completion with nothing connected.
5. Nothing tells the user that the tour can be replayed from Settings → General ("Show the tour again", `SettingsPage.tsx:95`).

Friction: the user has to guess that Settings → Accounts is the start. `Escape` closes the tour, but `Escape` is also the global close for help and Task Centre (`Shell.tsx:35-38`); harmless.

### (b) Adding a Gmail account

Entry: rail **Settings** → tab **Accounts (0)** → **Add account** (`AccountsTab.tsx:69,84`) or the Inbox empty-state **Open Settings** (`InboxPage.tsx:157`, deep link `#digs` → Accounts tab via `SettingsPage.tsx:41-53`).

Wizard (`AddAccountWizard.tsx`), a 720px dialog with a stepper (`:208-221`):

1. **Account type** — two radio cards, Gmail / Test mailbox (`:101-104`), **Continue**.
2. **Google client** — if no client id: paragraph "Riddle signs in to Gmail through an OAuth client you create once in Google Cloud: a Desktop app client with the Gmail API enabled. `docs/GMAIL_SETUP.md` walks through it." (`:121-123`), then **Import from Google's JSON…** (file picker, `GoogleClient.tsx:29-52`) and a collapsed "Enter the client id by hand instead" (`:125-128`). **Continue** is disabled with reason "Import the client file or enter its id first" (`:167`).
   - The hardest step of onboarding (create a Cloud project, enable the API, configure Google Auth Platform, choose Internal/External, add test users, add the `gmail.modify` scope, create a Desktop client, download JSON — `docs/GMAIL_SETUP.md:39-80`) is delegated to a repository path rendered in monospace, not a link, not openable from the app. Severity 3.
   - The client secret is optional for a Desktop client (`GMAIL_SETUP.md:11-15`); the import result panel says so plainly and names the keychain service (`GoogleClient.tsx:65-79`). This part is good.
   - Feedback on import: a `role="status"` card "Imported client_secret_….json at 10:42" plus a success toast (`GoogleClient.tsx:44-47,55-81`). Good.
3. **Behaviour** — Name, Group, then three rows of `OptionCards` (Autonomy without Trusted, Changes in Gmail, Privacy) and a computed sentence "With these settings: Riddle classifies only. Gmail is never changed." (`:233-270`, `model.ts:56-67`). Defaults are Observe / None / the privacy default (`:77`). This is the best screen in the app for explaining consequences.
4. **Sign in** — in a dev build that is not armed, the step is replaced by "This is a development build. It refuses to talk to a real mailbox until you arm it…" and a **Hold to arm this dev build** button (1.5 s, `:320-334`). After arming, the normal step: "Your browser opens Google's sign-in… Riddle asks only to read and label mail (gmail.modify)." → **Sign in with Google** (`:338-345`). The Action `connect_gmail` returns an auth URL, the browser opens, and the wizard polls `listDigs` every 1.5 s and listens for `notify` events titled "Gmail sign-in failed" / "Gmail connected" (`:282-292`). Waiting state has a spinner, "Waiting for Google…", and **Open the sign-in page again** (`:346-357`). Five-minute timeout with a clear failure card and **Try again** (`:301-305,358-366`).
   - Failure detection depends on matching an event *title string* (`:289-290`). Any wording change in Rust silently breaks the wizard's failure state.
   - The arming coaching says "Arming lets this build sign in and, at Act, change Gmail" (`:326`), conflating sign-in with mailbox writes; the Ops pane says the opposite ("Provider mutations … reach the mailbox only once armed", `developer/OpsPane.tsx:83`). Minor.
5. **Done** — "Personal is connected", the address, and two bullets including the effect sentence (`:371-388`). Footer: **Add another** / **Open Personal** (`:195-201`). "Open Personal" selects the account in the Settings pane (`AccountsTab.tsx:119`), not the mailbox. Severity 2 (label promises the inbox).

What can go wrong afterwards, and the gap: `docs/GMAIL_SETUP.md:146-149` documents that an External client in Testing status expires **seven days after consent**, and that a missing refresh token requires the user to "sign in again". The UI has **no sign-in-again control**. `errorAffordance('provider_auth')` produces a **Sign in** button (`selectors.ts:302`) that navigates to `/settings#digs` (`components/ErrorCard.tsx:22`), where `AccountDetail` offers Test connection, Sync now, autonomy, projection, privacy, group and Remove (`accounts/AccountDetail.tsx:56-152`) — nothing reconnects. The `not_connected` chip tooltip says "remove it and connect the account again" (`selectors.ts:43`); Remove is the type-the-count tier and deletes classifications, traces and the keychain entry (`GMAIL_SETUP.md:178-181`). A personal `@gmail.com` user therefore loses all learned state weekly. Severity 4.

### (c) The daily loop: Summary → Finds → act

1. Open app → Summary. If catching up: "Catching up." + "4 Finds so far. 1,240 of 4,981 messages still to sift." + a **Task Centre** link (`SummaryPage.tsx:40-48`). If caught up: "Good morning." then five count lines as links, with Finds and (if > 0) Unresolved in bold (`:67-91`). Optional Director note (`:94-102`) and Shadow card (`:104-114`). Footer "Last synced 2 minutes ago. Riddle processes mail while this machine is awake; nothing runs while it sleeps." (`:116-118`).
   - The Summary ignores the Dig switcher: `useSummary()` takes no Dig (`lib/api.ts:96`) and the header prints `digName(null)` = "All Digs" always (`SummaryPage.tsx:38`). The rail badges are also global (`Rail.tsx:26-27`), while Finds/Unresolved/Inbox pages filter by the selected Dig (`FindsPage.tsx:54-55`). With "Work" selected, the rail says "Finds 7" and the page says "3 open". Severity 2.
2. Click **7 Finds need your attention** (or `g f`) → `/finds`. Left pane 420px: filters Status (Open default) and Type; rows grouped by type with sticky headers; each row: title, summary, "P82 · 2 sources · due tomorrow", and a 32px priority bar (`FindsPage.tsx:113-156`). Right pane: "Select a Find. j and k move through the list; Enter opens; e resolves, s snoozes, d dismisses, r retypes, w toggles Why?." (`:174`). The first row is the cursor by default, but the detail pane only shows it when `findId` is absent and `nav_.current` exists — which it does, so the first Find's detail is visible at once. Good.
3. Detail (`FindDetail`, `:181-292`): header with type chip, "P82", Dig names, age, due; actions **Resolve** (title "Mark as dealt with (e). Nothing changes in Gmail."), **Snooze** (menu), **Retype** (menu), **Not important** (title "Close it and note that Riddle should not have raised it (d). Nothing changes in Gmail."), **?** (`:211-221`). Body: summary, **Talk to the Director about this**, **Reply**, the **Why?** card (open by default), then "Sources · 2 messages" as a thread tree with the source message outlined in ochre and the latest expanded (`:227-288`).
   - The Find's own plain-English `reason` ("Jamie requested your review directly; a follow-up says it is blocking the merge." in the fixture, `lib/mocks/fixtures.ts:1249`) is **never rendered** — `FindsPage.tsx` contains no reference to `find.reason`. The Why? card instead shows the Mesh route walk with monospace clauses like `review_requested@v3 > .80 is .93` (`Bits.tsx:150-158,166-208`). The friendliest explanation the backend produces is discarded. Severity 3.
   - "Resolved by thread state" is rendered from the raw enum (`:205`, `resolvedBy.replace('_',' ')`) instead of "resolved by your reply".
4. Press `e`: optimistic patch to `resolved` (`:74-84`), toast "Resolved" with **Undo** (8 s, `useAction.ts:156-168`; `stores.ts:38`). With the Open filter the Find leaves the list on refetch.
   - **Keyboard dead end.** If the Find was opened with Enter or a click, the URL is `/finds/:findId` (`:65,132`). After `e`, the resolved Find disappears from the `open` list and `current = ordered.find(f => f.id === findId) ?? null` is `null` (`:66`), so the detail pane shows "Select a Find" while `j`/`k` move an invisible cursor: `current` stays `null` because `findId` is still in the URL. Only a mouse click on a row (which navigates) recovers. Every keyboard-first session hits this on the first resolved Find. Severity 3.
5. Press `s`: a small overlay "Snooze until" with 1–4 numbered options (Later today 18:00 / Tomorrow 8:00 / Next Monday / In a week), navigable with j/k/Enter/digits (`:295-330`). Good. `r` opens the equivalent "Change type to" overlay.
6. Press `d` ("Not important"): dismisses with Undo; help says "undoing withdraws the correction" (`actionHelp.ts:75`). Good.
7. **Open message** (inside an expanded source) → `/inbox/:digId/:messageId`; **Back** is `nav(-1)` (`MessagePage.tsx:79`).
8. **Talk to the Director about this** → `setPrefill(directorAbout(...))` then `nav('/director?send=1')` (`FindsPage.tsx:235-239`). The Director page sends immediately (`DirectorPage.tsx:75-84`). The user's own bubble contains the templated prompt including raw ids: `Let's talk about this Review Find: "…" (Dig dig_work, message m_work_aws). Why did Riddle decide it this way…` (`selectors.ts:344-347`). It costs an LLM turn with no chance to edit, and shows internal identifiers to the user. Severity 2.

### (d) Unresolved: deciding a message

`/unresolved` (`features/finds/UnresolvedPage.tsx`). Header: "Unresolved — 5 Riddle could not decide. Deciding here is the fastest way to teach it." (`:49`). Rows: sender, subject, time, snippet, Dig + Strata chips (`:54-68`). Detail: subject, `Name <email> · Dig · time`; buttons **Find…** (primary, opens a menu of the eight types), **Filed**, **Spoil**, **Contamination** (danger), **Never hide** (ghost), **?** (`:79-105`); copy "Decide what this is. Your answer is recorded as a correction; when the same kind keeps coming up, the Director can show you the pattern and propose a Mesh change." (`:110`); the Why? card; **Open the message** (`:117-119`).

- Keyboard `f` silently decides **Find: Respond** (`:39`, `find_type: 'respond'` hard-coded) while the button opens a type menu. A user who presses `f` on an invoice creates a Respond Find. Severity 2.
- `n` (documented as "Not Spoil / Restore") maps to Filed here (`:43`), duplicating `i`.
- No bulk decide: `multi: false` (`:32`); with 200 Unresolved after a week away, each is one decision plus one keypress. `x`/`shift+x` do nothing though the help sheet lists them under "Any list". Severity 2.
- The list is capped at 200 (`page(null, 200)`, `:30`) with no paging; the header total may exceed what is listed.
- After a decision the item leaves the list and the cursor lands on the next — the flow is quick. Empty state: "Nothing is Unresolved. Attention Zero includes this tray…" (`:52`). Good.

### (e) Inbox, Review pages, message page, and `?`

**Inbox** (`features/messages/InboxPage.tsx`). Header: title "Inbox" / "Search: q" / "Shadow proposals"; subtitle "1,204 messages · All Digs" (`:99-100`). Filter bar: a search input (placeholder "Search (from:, is:, stratum:, label:)", commits on Enter, `:125`), Disposition select, Stratum select, Label select with a hard-coded list (`INBOX`, `SENT`, `IMPORTANT`, `SNOOZED`, Promotions, Updates, Personal — `:129`), Sender input, Since/Until dates, **Clear filters**. Action buttons appear only when a row is current or selected: "current row"/"N selected", **Archive**, **Spoil**, **Stratum**, **Snooze** (menu), **Contamination** (danger), **?** (`:102-120`).

- Rows (`MessageList.tsx:57-89`): checkbox, sender (bold if Find), subject, paperclip, time; second line snippet, Dig, up to two Strata chips, disposition chip. **Single click only moves the cursor; opening needs a double-click or Enter** (`:65-66`). Finds and Unresolved open on single click. Nothing in the row hints at double-click. Severity 3.
- No preview pane: reading a message means the full-page `MessagePage`, then Back. The message page has no next/previous. Triage of the inbox is open → act → back → j. Severity 3.
- Keyboard `s` snoozes **24 hours, silently** (`:89`, `Date.now() + 86_400_000`) while the toolbar Snooze offers named times; on Finds `s` opens a picker. Severity 2.
- `w` navigates to `/inbox/:dig/:id#why` (`:92`) but `MessagePage` never reads `location.hash`, so it does not scroll to Why. Severity 1.
- Label filter values are Gmail system ids shown raw (`INBOX`, `SENT`, `CATEGORY_UPDATES` → "Updates" but `IMPORTANT` unchanged), and user-created Gmail labels are not offered.
- Empty states are instructions: "No messages match. Try a sender, a subject word, or from:, is:, stratum: and label: operators. Or ask the Director a question." / "Connect a Dig in Settings to start the excavation." with **Open Settings** (`:152-158`). Good.

**Spoil / Contamination** (`features/messages/ReviewPage.tsx`, one component). Header subtitle "312 messages · All Digs · legitimate, low value; not deleted, still searchable" or "… untrusted; decided by evidence, never read by the Director" (`:69`). Buttons: **Not Spoil** / **Restore**, **Never hide sender**, and on Contamination **Delete permanently…** (`:73-83`), plus `?`. Explanatory line under the header (`:89,91`) and on Contamination a warning Note "Mail that argues for its own importance is treated as evidence against it. Check the sender and authentication before restoring." (`:94-100`). Empty states explain the Shadow Period and the evidence checks (`:114-119`).

- Buttons are enabled whenever the list has items (`:73,76,80`); with nothing checked they act on the cursor row, indicated only by the small "current row" text (`:72`). A user who clicks **Delete permanently…** thinking nothing is selected gets a type-the-count dialog for one message — the count check saves them, but the affordance is unclear. Severity 2.
- The `messages` scope is not bound here, so `e`, `#`, `!`, `l` do nothing on Spoil; only `n` and `h` (`:61`). The `?` sheet lists `#`/`!` under "Inbox and message", so the omission is documented but surprising.
- **Never hide sender** on a selection runs `add_never_hidden` per sender without `digId`/`messageId` (`:52`) and then restores; the help promises "Recorded as a correction on this message" (`actionHelp.ts:97`), which this call cannot do. Severity 1.
- "Delete permanently…" (button) vs help "Moves them to Gmail's Trash, which Gmail empties after 30 days" (`actionHelp.ts:96`) vs dialog "This cannot be undone" (`TypeCountDialog.tsx:18`). The three statements are reconcilable but the button and dialog overstate. Severity 1.

**Message page** (`features/messages/MessagePage.tsx`). Header: Back, subject; subtitle Dig, "thread of N", disposition chip, dev overlay; actions **Reply**, **Reply all**, **Forward**, **Correct…** (menu: eight Find types, Filed, Spoil, Contamination), **Snooze**, **Stratum**, **Archive** (ghost), **?** (`:76-116`). Body: Contamination Note when applicable (`:118-124`), the thread as collapsible cards with avatar initial, sender, email, "auth failed" chip on SPF/DKIM/DMARC failure (`:161`), a Reply-To mismatch line in rust (`:178-183`), a List line, and **Labels** as raw Gmail ids (`:190-191`); the body in a sandboxed iframe with "Remote images blocked: the sender cannot see that you opened this." + **Load remote images (N)** (`components/MessageBody.tsx:86-100`) and a collapsible "Links in this message (N)" list (`:109-130`). Then the Why? card or "Not sifted yet. This message has no decision to explain yet. It is in the queue; the Task Centre shows progress." (`:132-143`).

- The disposition chip depends on the `explain` query (`:63`); until it loads the chip reads "Unsifted". `<StrataChips strata={ex.data ? [] : []} />` (`:90`) always renders nothing — Strata are never shown on the message page header. Severity 1.
- The **Correct…** menu does not mark the current disposition (`:65-71`).
- The header holds eight controls plus a truncating title; at narrower widths the title vanishes first (`Bits.tsx:53-58`, `shrink-0` on actions).
- `s` snoozes 24 h silently here too (`:56`).

**The `?` beside a row of buttons** (`components/ActionHelp.tsx:56-82`): opens a wide dialog titled e.g. "Acting on a message" with the intro and a five-column table Button / In Riddle / In Gmail / Teaches Riddle / Undo rendered from `lib/actionHelp.ts`. The Gmail column is computed for the account being viewed ("Adds the Riddle/Spoil label and archives it." vs "Nothing: this account's 'Changes in Gmail' is set to None.", `actionHelp.ts:42-48`); an unarmed dev build shows a "Needs arming" tag and a banner (`ActionHelp.tsx:43,64-68`); the footer names the account's setting and points to Settings → Accounts (`:73-77`). This is excellent and is the only place in the app that says what a click does to Gmail. On the Inbox the context is the Dig filter, so with "All Digs" selected it falls back to the generic sentence (`:76`).

### (f) Correcting Riddle and what the user learns

Corrections available: Finds → **Not important** / **Retype**; Unresolved → decide; Message → **Correct…**; Spoil/Contamination → **Not Spoil** / **Restore**; anywhere → **Never hide** (Unresolved, Review, Settings → Mail rules); Why? card on the message page → **Not Important** (= Filed, `MessagePage.tsx:137`).

What the user is told about the effect:
- At the point of action: only a toast ("Corrected", "Dismissed", "Restored 3", `useAction.ts:179-198`) with Undo. The toast never says whether Gmail changed. To know, the user must open `?`. Severity 3 for trust (see §6).
- The one-time explanation that a correction "never changes a rule by itself" appears in the `?` dialog and help sheet (`actionHelp.ts:53`) and on the Unresolved page (`UnresolvedPage.tsx:110`). Nothing tells the user how many corrections the Director needs before proposing, or shows the count of corrections against a Mesh anywhere in the normal UI (the Developer "Decision Quality" pane has "Most corrected Signals", `developer/Observatories.tsx:221`).
- Undo semantics are documented per button ("Yes, and undoing withdraws the correction.", `actionHelp.ts:75,95`) and in Activity ("Cannot undo: …", `ActivityPage.tsx:143`). Good.
- Naming conflict: "Not important" on a Find dismisses the Find and leaves the message classified (`actionHelp.ts:75`); "Not Important" on the Why? card re-files the message as Filed, which is a different correction with a different Gmail effect. Same label, two Actions. Severity 3.

### (g) Talking to the Director, proposals, applying a Mesh to history

`/director` (`features/director/DirectorPage.tsx`). Left: conversation list (title, age, "read mail" chip) with **New**; bottom: Proposals panel when any exist. Right: empty state "Ask the Director." with six clickable suggestions (`:29-36,136-153`) — clicking one sends it immediately. Input: two-row textarea, **Send** (title "Send (⌘↵)"), scope line "Scope: all Digs. Contamination never enters the Director's context unless you open it." (`:205-227`).

- **Enter sends** (`:211-216`); Shift+Enter is the undocumented newline. The shortcut table documents only `mod+enter` (`lib/shortcuts.ts:74`). A user typing a multi-line question sends half of it. Severity 2.
- Streaming: a "Director · working" bubble with tool-call cards (action name, summary, latency, a "read untrusted content" chip with tooltip explaining egress gating, an **Activity** link) and "Thinking…"/"Reading results…" (`:185-201,233-257`). Each finished turn shows cost and tokens (`:171`). Good transparency. There is **no Stop/Cancel** for a running turn. Severity 2.
- A banner appears once the conversation has read mail: "The Director read mail content in this conversation. Sending, forwarding, creating or updating a Mesh, and adding to Never Hidden now need your approval, and each control says so." (`:156-163`). Good.
- Director answers are rendered by a tiny Markdown renderer (`:38-55`) inside a body with `user-select: none` (`index.css:70`) and no `.selectable` class: **the user cannot select or copy the Director's answer** with the mouse. Same for Find titles/summaries and the Why? card. Severity 3.
- **Proposals** arrive as `proposal_created` events and live only in a Zustand store (`lib/stores.ts:158-162`; `lib/events.ts:114-117`). `listPending` is fetched only while that in-memory list is non-empty (`DirectorPage.tsx:262`). After a restart, pending proposals and the rail badge (`Rail.tsx:19,45`) disappear even though the backend keeps `PendingAction`s "until confirmed" (`bindings/PendingAction.ts:6`). Severity 3.
- A proposal card: summary, age, reason; **Preview** (dialog with the Mesh text as `<pre>` and the rest as JSON, "The real arguments the Action layer will run with.", `:307-328`), **Apply** (routes through the confirmation tier — for `create_mesh`/`update_mesh` after untrusted reads this is the `ApprovalDialog`), **Dismiss** (local only; "Proposal dismissed — Nothing changed.", `:293-302`). Dismiss does not tell the backend, so the pending action remains stored server-side.
- `ApprovalDialog` (`components/confirm/ApprovalDialog.tsx`): warning Note when the task read untrusted content, "Real arguments" as text + JSON, **Reject** / **Approve**. **Approve is `autoFocus`** (`:32`): pressing Enter approves an egress action the dialog just asked the user to read carefully. Severity 3.
- Applying to history: nowhere in the Director flow does the UI show the simulation report (unchanged/changed/transitions) before Apply. The Mesh editor's "Preview impact" (`MeshesPage.tsx:386,399-409`) is the only place a report is shown, and it is not reachable from a proposal card. The README's promise "This change would have affected 17 messages in the last 90 days. Preview · Apply" (`README.md:697-701`) is not realised in the proposal UI. Severity 3.

### (h) Editing a Mesh by hand, simulating, activating

`/meshes` (`features/meshes/MeshesPage.tsx`). Left 360px: **New**, rows with name, `vN`, status chip (raw enum: `shadow`, `active`, `draft`, `disabled`, `deleted`), scope, "agreement 87%" (`:95-112`), and a footer toggle "Show Recently Deleted (N)" (`:114-118`). Right: **Editor / Structure / History (N)** tabs (`:190`).

Editor (`:289-437`): a textarea with a line-number gutter, live parse after 250 ms (`:303-318`) showing "line 7: …" issues in red or "Valid: 3 routes, 1 guard, scope All Digs." (`:360-374`); a **Change note (why this version exists)** input; for a new Mesh a "Start as Shadow (proposes) / Draft (off)" select; **Preview impact** (⌘↵; disabled "Fix the errors first"), **Apply as v8** / **Create** (⌘S; disabled "Fix the errors first" or "No changes"), **Keep v7** (`:375-397`). Right aside: the `SimulationTable` after preview, then a static **Syntax** box (`:399-434`).

- The gutter is a sibling `div` of the textarea inside a container without `overflow-auto` (`:343-359`); once the Mesh is taller than the pane the textarea scrolls internally and the line numbers stay put, so error markers point at the wrong lines. Severity 2.
- The DSL is learnt from a 20-line syntax box and the `Facts:` list ending in "…" (`:428-429`); there is no autocomplete, no hover on a Signal name, no link from `routine@v1` to the Signal page. Recall-heavy. Severity 2 **[opinion: acceptable for the current single developer-user]**.
- Preview: "Run Preview impact to execute this text against your history: how many messages would change, which way, and whether any Signal needs a fresh judge call. Nothing changes until you Apply." (`:408`). The report lists unchanged/changed/transitions/new judge calls/estimated cost and warns when the backfill costs (`SimulationTable.tsx:18-29`). Good. After Apply the report is shown but the editor remounts on the new version (`:192` keyed by version) — the report survives only because `setReport` runs before `onSaved`.
- Status buttons (`:159-176`): **Shadow**, **Activate** (hidden when the Mesh proposes Spoil; disabled for drafts with "A draft starts in Shadow; it earns Active by agreeing with you"), **Disable**, **Delete** (soft, undo toast — the only destructive-looking button without an ellipsis, `:174`). When a Shadow Mesh proposes Spoil, a banner "This Mesh proposes Spoil. Activating lets it hide mail on its own." with agreement stats and **Hold to activate** (1.5 s) appears (`:181-189`). The hold coaching says "the mail it hides stays reviewable in Spoil". Good.
- History: version list with change note and author; diff against the previous version; **Restore as vN+1** rewrites the version token in the text (`:238-287`). Good.
- Structure tab labels routes "first match wins" and guards "run after every route, from every scope" (`:208,225`) — but nothing in the app shows precedence **between Meshes** (README promises "More specific behaviour can override broader", `README.md:151`). With 10 Meshes the user cannot see which wins. Severity 2.
- Nothing on this page says which Meshes are Riddle defaults vs the user's, except `created by riddle` in the subtitle (`:148`).

### (i) Settings: all four tabs

`features/settings/SettingsPage.tsx`. Header subtitle "Everything here is saved through the Action layer and appears in Activity." (`:84`) — system-centric.

- **General**: App (Launch at login, Close hides the window switches; **Show the tour again**), Notifications ("Priority threshold" 0–100 number + **Test Notification** with a helpful failure hint naming System Settings → Notifications, `:101-121`), Update (either "You are on the latest version…" or an Update Preview card with a SimulationTable, **Apply…** / **Keep previous behaviour**, `:358-381`), Developer Mode (switch + Log level). The threshold number is unexplained: users see "P82" on Finds but nothing says what 82 means or what typical values are. Severity 2. The update banner's button says **Review and relaunch** (`Banners.tsx:32`) but the section has no relaunch. Severity 2.
- **Accounts (N)**: list/detail (`accounts/AccountsTab.tsx`). List rows: avatar, name, test-mailbox flask, status dot, autonomy + "· Labels/Archive" (`:124-146`); search appears at six accounts (`:35`); a **Setup** section with "Google sign-in — Client set up / Not set up yet" and "Defaults and groups — Default: Observe · 1 group" (`:104-108`). Detail (`AccountDetail.tsx`): inline-editable name, email, provider chip, status chip, "Added 3 days ago"; **Test connection**, **Sync now**; a "Right now:" sentence in a highlighted box (`:83-90`); Autonomy option cards with "Use the default (Observe)" link; Trusted requires a hold with coaching "Trusted acts without asking, including sending mail." (`:111-126`); Changes in Gmail cards with a "Held because autonomy is observe. Your own moves in Riddle still apply." line (`:128-139`); Privacy cards; Group picker with inline "New group…"; **Remove…** confirmed inline by typing the message count (`:260-330`). Defaults panel: default autonomy (Trusted excluded; a Trusted default is displayed as Act, `:201`), "Sending mail" autonomy select, new-account defaults, Groups with rename/delete locked while rules use them (`Groups.tsx:46`).
  - This tab is the clearest surface in the app. Its only gap is the missing reconnect (§2b).
- **Providers**: Judge card (provider select "Jev (TypeSafe)" / "None: every uncertain message is Unresolved" / "Scripted (development)"; free-text **Model** "Pinned to an exact version."; Key source; SSM fields; **Save judge**, **Probe**; a keychain `SecretField` that only appears after saving, `:248-301`), LLM card (Bedrock profile, region, two free-text model ids, `:303-344`), Budgets (three USD/day fields with a description that uses "Escalation", "second opinions", `:151-159`), Privacy (a per-Dig privacy statement as text, `:346-356`). Free-text model ids with no picker and no validation beyond "Name the model" (`:262`) invite typos that only Probe catches. **[opinion]** Acceptable for the developer audience; not for anyone else.
- **Mail rules**: Never Hidden chips with remove ×, an add field validated as address or `@domain`, and a paragraph listing the built-in rules (`:189-215`). Description "You edit this list; Riddle cannot." (`:167`) contradicts the Director's `add_never_hidden` egress Action (`DirectorPage.tsx:160`; `actionHelp.ts:65`). User Profile: ten fields as comma-separated lists, **Save profile** (`:217-242`); description "personally_relevant and directly_addressed are undefined without it." exposes Signal identifiers (`:171`). Severity 2 each.
- Deep links work via `#/settings#section` (`:41-53,66-77`). There is no `⌘,`; only the chord `g ,` (`shortcuts.ts:41`).

### (j) Activity and undo

`/activity` (`features/activity/ActivityPage.tsx`). Filters: Dig (Current / All / each), Actor (You, Director, Sift, Mesh, Automation, System), Action (raw action names like `move_to_spoil`, `:67`), Since/Until. Entries grouped by day ("Today"), each: clock, bold summary (struck through when undone), provenance line with a clickable "Mesh: GitHub Notifications v7", why, note, failure line, a collapsible "1 skipped: it was handled by hand since." for partial results, "Undone by you at 10:42", "Cannot undo: …", actor chip, Dig, and buttons **View Messages (N)**, **View Reasoning**, **Edit Mesh**, **View Find**, **Undo** / **Undo All** (`:95-180`). This matches the README's design closely.

- **View Reasoning** navigates to `/developer/messages/...` (`:155`); with Developer Mode off the destination is "Developer Mode is off. Turn it on in Settings." (`developer/DeveloperPage.tsx:37-43`). The same dead end is reached from the Why? card's **View Jev** (`Bits.tsx:268-270`). Severity 3.
- **View Messages** for a bulk entry goes to `/inbox?disposition=spoil` (only for `move_to_spoil`) or plain `/inbox` (`:150`), not to the affected messages. Severity 2.
- Undo elsewhere: toast **Undo** (8 s); global `u` undoes the last reversible Action of the session and says "Nothing to undo" otherwise (`Shell.tsx:48-54`); Summary Attention Zero has an **Undo** button (`SummaryPage.tsx:61`); the Why? card on the message page has **Undo** for the last undoable (`MessagePage.tsx:138`). Consistent and honest ("Undone. Skipped what you handled by hand since.", `ActivityPage.tsx:170`).

### (k) Search (SearchBox) and the Task Centre

**SearchBox** (`app/shell/SearchBox.tsx`): placeholder "Ask Riddle: search, or a question for the Director"; while typing, a right-hand hint says "Director ↵" or "Search ↵" (`:52`) from `looksLikeQuestion` (`:9-14`): a trailing `?` or a leading `what|why|who|when|where|which|how|did|do|does|is|are|can|could|should|find (the|my|all|every)|show me|draft|summari[sz]e|tell me|go through`. A question sends **immediately** (`nav('/director?send=1')`, `:36`).

- Verified with the regex: `is:unread`, `is:starred from:adam`, `do:something`, `find the receipt`, `show me everything from adam` all route to the Director and are sent without review; `from:adam` and `portugal hotel` search. The Inbox placeholder advertises `is:` as a search operator (`InboxPage.tsx:125`). Severity 3 (wrong destination, real cost, no edit).
- There is no way to force a mode (no prefix, no modifier) and the hint is small 11px text.
- `/` focuses the box; Escape clears and blurs (`:47`).

**Task Centre** (`app/shell/TaskCentrePopover.tsx`, `t` to open): trigger shows a dot + "Caught up / Catching up / Degraded" + active job count (`:33-37`); popover: label + detail ("as of 10:42" / "312 messages to go"), store-fault line, per-Dig chips with tooltips ("Catching up 1,240" → "1,240 of 4,981 messages still to fetch"), a "First sync in progress" card with an honest disclaimer, "Background work" with job rows (label, kind · priority, Pause/Resume/Cancel with a disabled reason, progress bar, "Fetching headers · 1,204 of 4,981 messages (24%)", "~3 min left"), and "Recently finished (N)" (`:40-95`). Empty: "Nothing running. Riddle polls Gmail every 30 to 60 seconds while awake." This is a strong implementation of §55.

- The `busy` and `warn` chip tones share one colour (`components/ui/Chip.tsx:8-9`), so "Catching up" and a warning look the same.
- Job rows show `priority` raw (`catch_up`, `interactive`, `:112`).

### (l) Spoil, Contamination, Strata, Signals, Expeditions

Spoil/Contamination: see (e). **Strata** (`features/strata/StrataPage.tsx`): subtitle promises "you rename, merge and retire them" (`:52`) but only Rename and Merge exist (`:80-85`); no retire/delete. Origin chip "proposed by Riddle" / "yours" (`:77`); clicking a name filters the Inbox by that Stratum (`:70`). The create dialog warns on near-duplicates (bigram similarity ≥ 0.6, `:22-38,146-150`) and lets the user scope to a Dig Group. Merge dialog explains the consequence (`:166`). Good. Severity 2 for the unfulfilled "retire".

**Signals** (`features/signals/SignalsPage.tsx`): rows show `snake_case` name, version, raw status chip (`proposed`, `candidate`, `evaluating`, `experimental`…), definition, "noul · calibrated on 1,200 · agreement 91.2% · riddle" (`:56-68`). Detail: **Test** and **Promote…**; tabs Definition (choices, inputs, calibration with Platt a/b), Compare versions (diff + a table), Test ("Run test on 40 messages" — spends money with no cost shown until after, `:202-249`). Propose dialog with seven authoring rules (`:33-41,256-316`). Everything here assumes the reader knows what calibration and held-out agreement are.

**Expeditions** (`features/expeditions/ExpeditionsPage.tsx`): list with goal, status chip, a plan progress bar, "2/5 steps · $0.42 of $1.00"; detail with three budget cards (Cost/Time/Tool calls with bars), "Approaching its budget. It stops at the limit and reports what it has." (`:96`), plan checklist, Evidence chips (`message m_work_aws` → navigates to `/inbox?q=m_work_aws`, a search for a raw id, `:119-129`), Findings, error, "Checkpointed; resumes from step N after a restart." Start dialog: goal, three budget numbers, Dig checkboxes with "(local-only: no LLM)" (`:147-195`); scope date range and tools are not offered although the type supports them (`:183` hard-codes `toolsAllowed`, `since: null`). Evidence should link to the message page, not a search. Severity 2.

---

## 3. Keyboard and interaction model

### 3.1 Every shortcut (`lib/shortcuts.ts:27-75`) and where it is bound

| Keys | Scope (sheet heading) | Action | Bound in | Discoverable |
|---|---|---|---|---|
| `?` | Everywhere | Help sheet | `Shell.tsx:31` | Top-bar `?` button |
| `/` | Everywhere | Focus Ask Riddle | `Shell.tsx:32` | `kbd` in the search box |
| `c` | Everywhere | Compose | `Shell.tsx:33` | Compose button title |
| `t` | Everywhere | Task Centre | `Shell.tsx:34` | Sheet only |
| `Esc` | Everywhere | Close help / Task Centre / tour / dialogs | `Shell.tsx:35`, `Tour.tsx:53`, Radix | universal |
| `g s` `g f` `g u` `g i` `g p` `g a` `g m` `g d` `g ,` | Everywhere | Go to Summary/Finds/Unresolved/Inbox/Spoil/Activity/Meshes/Director/Settings | `Shell.tsx:39-47` | Sheet only; no hint in the rail |
| `u` | Everywhere | Undo last undoable | `Shell.tsx:48-54` | Sheet; Summary Undo button |
| `j` `k` `Enter` `x` `⇧x` | "Any list" | next/prev/open/select/select all | `useListNav` in Finds, Unresolved, Inbox, Spoil, Contamination only | Finds/Unresolved empty-detail hints |
| `e` `s` `d` `r` `w` | Finds | resolve, snooze picker, dismiss, retype picker, toggle Why | `FindsPage.tsx:88-94` | Button titles; empty-detail hint |
| `e` `#` `!` `s` `l` `r` `f` `w` | Inbox and message | archive, spoil, contamination, snooze 24 h, Stratum picker, reply, forward, why | `InboxPage.tsx:84-93`, `MessagePage.tsx:51-60` | Button titles (partly) |
| `n` `h` `f` `i` `#` | Spoil, Contamination and Unresolved | not spoil/restore, never hide, decide Find, decide Filed, decide Spoil | `ReviewPage.tsx:61` (n, h only), `UnresolvedPage.tsx:38-44` (all) | Button titles; Unresolved empty-detail hint |
| `⌘↵` `⌘S` | Meshes | preview, apply | `MeshesPage.tsx:338` | Button titles |
| `⌘↵` | Director | send | `DirectorPage.tsx:111` (plain Enter also sends, undocumented) | Send button title |

Not in the table but active: `1`–`9`, `j`/`k`/arrows/Enter inside the Finds snooze/retype overlay (`FindsPage.tsx:303-318`); Enter/Escape in inline name fields; Space/Enter hold on `HoldButton` (`HoldButton.tsx:63-74`); Escape in the composer saves and closes (`composer.tsx:115-125`).

### 3.2 Conflicts and inconsistencies

- Same key, different meaning by page, with no on-screen mode indicator: `e` = Resolve (Finds) / Archive (messages); `s` = picker (Finds) / silent 24 h (messages); `r` = Retype / Reply; `f` = Forward (messages) / Decide Find→Respond (Unresolved); `#` = Spoil (messages) / Decide Spoil (Unresolved); `n` = Filed on Unresolved (`UnresolvedPage.tsx:43`) but Restore on Spoil.
- "Any list" is an overclaim: Meshes, Signals, Expeditions, Director conversations, Activity and Strata do not use `useListNav`; `j`/`k`/Enter do nothing there. `x`/`⇧x` no-op on Finds and Unresolved (`multi: false`).
- Chords wait one second for the second key (`useShortcuts.ts:28`); the first key is swallowed with `preventDefault` even when the second never comes. Typing `g` then clicking elsewhere leaves `pendingChord` set until the next keypress.
- A chord prefix `g` and a single-key `g` cannot coexist; fine today.
- `isEditableTarget` guards inputs except Escape and modifier combos (`useShortcuts.ts:42`); good.
- No arrow-key navigation in lists, no Home/End/PageDown; `j`/`k` only.

### 3.3 Focus, selection and what needs the mouse

- List rows are `role="option"` with `tabIndex={-1}` (`MessageList.tsx:59-62`, `FindsPage.tsx:126-129`, `UnresolvedPage.tsx:55`): they are never in the Tab order, and keyboard selection is a global listener, not DOM focus. `:focus-visible` rings exist (`index.css:86-89`) and the selected row has `.row-selected` (`:92-95`), but a screen reader is never told the cursor moved (no `aria-activedescendant`, no live region). Severity 3 for accessibility.
- Needs the mouse: opening a Mesh, Signal, Expedition or conversation; the Inbox filters (Selects/date inputs are Tab-reachable but not shortcut-reachable); the Task Centre Pause/Cancel buttons (Tab-reachable inside the popover); the Correct… and Snooze menus on the message page (no shortcut opens Correct…); Load remote images; the Proposals Apply/Preview; Strata rename/merge; everything in Settings except via Tab.
- Buttons with only an icon get `aria-label` from `title` (`Button.tsx:50`); `ActionHelpButton` sets one explicitly (`ActionHelp.tsx:62`). Disabled buttons expose their reason via tooltip and `aria-label` on a wrapper span (`Button.tsx:71-78`).
- `body { user-select: none }` (`index.css:70`) with an allow-list (`:74-77`): text is not selectable unless marked `.selectable`. Not marked: Director answers, Find titles/summaries, Why? card text, page headers, Activity summaries, Expedition findings. Severity 3.
- Dialogs: Radix traps focus and closes on Escape. `TwoStepDialog` autofocuses **Send** (`:51`) and `ApprovalDialog` autofocuses **Approve** (`:32`), so Enter completes the second step immediately (see §5 H5).
- Single-click vs double-click: Inbox/Spoil/Contamination rows open on double-click or Enter (`MessageList.tsx:65-66`); Finds and Unresolved rows open on single click. Severity 3.

---

## 4. Feedback, states and copy

### 4.1 Empty states (all are instructions or proofs — the README rule holds)

- Finds: "No open Finds. When the Sift surfaces something that deserves attention it appears here, with a Why? you can inspect. Corrections in Unresolved are the fastest way to teach Riddle what counts." + **Open Unresolved** (`FindsPage.tsx:105-111`). With Status = All the title renders "No  Finds." (double space, `:108`).
- Unresolved: "Nothing is Unresolved. Attention Zero includes this tray…" (`UnresolvedPage.tsx:52`).
- Inbox: "No messages here. Connect a Dig in Settings to start the excavation." + **Open Settings**; search variant lists operators (`InboxPage.tsx:152-158`).
- Spoil: "Spoil is empty. Nothing has been moved to Spoil for this Dig yet. During the Shadow Period, Riddle only proposes; see the Summary for what it would have done." (`ReviewPage.tsx:118`).
- Contamination: "No Contamination. No mail failed the evidence checks: Gmail spam verdict, authentication, lookalike domains, first-time senders asking for money or credentials."
- Strata, Meshes, Signals, Expeditions, Activity, Director conversations: each has one (`StrataPage.tsx:54`, `MeshesPage.tsx:94,121`, `SignalsPage.tsx:55,71`, `ExpeditionsPage.tsx:39,60`, `ActivityPage.tsx:72`, `DirectorPage.tsx:129,136`).
- Summary Attention Zero: evidence line + **Review Spoil** / **Undo** (`SummaryPage.tsx:56-65`) — but it is shown with zero accounts (§2a).
- Missing: no empty state for the Inbox while a first sync is running beyond the generic text (the Task Centre has it, `TaskCentrePopover.tsx:64-72`).

### 4.2 Loading

Plain text: "Loading…" (route suspense, `routes.tsx:21`), "Reading the summary…" (`SummaryPage.tsx:25`), "Loading message…" (`MessagePage.tsx:62`), "Loading Mesh…", "Loading settings…", "Checking…" (Mesh parse, update check). Finds/Unresolved lists show nothing in the list area while loading; only the header subtitle says "Loading…". Buttons show "Working…" while busy (`Button.tsx:68`) — which changes width and hides the label; Probe results appear inline. No skeletons. Acceptable; **[opinion]** the Finds list should show a row-shaped placeholder so the first keypress does not land on nothing.

### 4.3 Errors

- Query errors → `ErrorCard` with a typed affordance: Sign in, Open Task Centre, Retry, Reconcile, Open Developer Mode, and a "Degraded · nothing hidden" chip for judge/LLM outages (`components/ErrorCard.tsx`, `selectors.ts:299-336`). The copy is good ("Gmail history expired — A full resync is queued. Nothing is lost."). The "Sign in" affordance leads nowhere useful (§2b).
- Action errors → an 8-second error toast (`useAction.ts:232-238`) and nothing else; failed Actions do appear in Activity with "Failed: …" (`ActivityPage.tsx:127`), but the user is not pointed there.
- `policy_denied` affordance sniffs `e.message.includes('armed')` (`selectors.ts:322`) despite `ipc.ts:8` promising "nothing here sniffs strings".
- Route crashes → "This page failed to render. … The rest of Riddle keeps working." + **Try again** (`components/ErrorBoundary.tsx:21-27`). Good.
- Store fault → a persistent red banner with **Open Developer Mode** (`Banners.tsx:14-21`) — for a normal user, Developer Mode is not the fix; the banner offers nothing else.
- Mesh parse issues → per-line red gutter markers and a `role="alert"` list (`MeshesPage.tsx:346,362`). Good.

### 4.4 Toasts and progress

- Toasts: bottom-right, up to five, 4 s (8 s with an action or error), `role="status" aria-live="polite"`, an **Undo** button when the Action is reversible (`stores.ts:38-39`, `ConfirmInPlace.tsx:14-35`). Titles come from a fixed map ("Corrected", "Stratum added", "Mesh status changed", `useAction.ts:178-229`) rather than the richer Activity summary the backend produces ("You changed "AWS Billing Alert" from Spoil to Find"), although the doc comment says it should default to that summary (`useAction.ts:30`).
- Progress bars only move on real events; unknown totals are striped, not animated (`components/ui/Progress.tsx`, `index.css:102-105`). `HoldButton` uses a timer so a hold completes even when unpainted (`HoldButton.tsx:24-26`). Honest, as the conventions require.

### 4.5 Confirmations by tier (`lib/confirm.ts:14-29`)

| Tier | Control | Example | Notes |
|---|---|---|---|
| none / in_place_undo | runs, Undo toast | Spoil, Archive, Resolve, Correct… | Consistent |
| two_step | dialog with real recipients; "Sent mail cannot be unsent." (`TwoStepDialog.tsx:17-53`) | Send | Send is autofocused |
| type_count | dialog or inline; count from the Action layer (`TypeCountDialog.tsx`, `AccountDetail.tsx:302-328`) | Delete permanently…, Remove… | Good; disabled reason "The Action layer gave no count" is system-centric |
| press_hold | 1.5 s hold with coaching (`HoldButton.tsx`) | Trusted, Activate a Spoil Mesh, Arm dev build, generic hold dialog with raw JSON (`ConfirmHost.tsx:29-38`) | Good; JSON in the generic dialog is unreadable |
| approval | real arguments + Reject/Approve (`ApprovalDialog.tsx`) | Director egress after untrusted reads | Approve is autofocused |

Ellipsis convention ("Send…", "Remove…", "Delete permanently…", "Promote…", "Merge…", "Start…", "Apply…") is applied consistently except Mesh **Delete** (`MeshesPage.tsx:174`, soft delete with undo — defensible).

### 4.6 Which pages explain "why"

Explain: Finds (Why? card, open by default), Unresolved (Why? card), Message page (Why? card or "Not sifted yet"), Activity (provenance + why + note), Meshes (Structure tab, agreement), Signals (calibration), Task Centre (per-Dig tooltips), Settings → Accounts ("Right now:" sentence, "Held because…"). Do not explain: Inbox and Spoil rows (only a disposition chip; to learn why, open the message and scroll to the bottom), Strata ("proposed by Riddle" with no reason), Expeditions (evidence as ids), the rail badges.

### 4.7 System-centric copy (quoted)

- "Everything here is saved through the Action layer and appears in Activity." (`SettingsPage.tsx:84`)
- "The Action layer gave no count" (`TypeCountDialog.tsx:28`, `AccountDetail.tsx:324`)
- "personally_relevant and directly_addressed are undefined without it." (`SettingsPage.tsx:171`)
- "Judge and LLM cover all spend with that provider; Escalation caps the LLM second opinions Sift asks for." (`SettingsPage.tsx:151`)
- "Conversations are not state: everything the Director does lands in Finds, drafts, Meshes and Activity." (`DirectorPage.tsx:129`)
- "View Jev" (`Bits.tsx:269`); "Decided on Signals" (`selectors.ts:200`); "Cursor current" (`selectors.ts:30`)
- "Product Activity, not software logs." (`ActivityPage.tsx:64`); "Metrics always name their dataset." (`SignalsPage.tsx:198`)
- "(Dig dig_work, message m_work_aws)" inside the user's own Director message (`selectors.ts:346`)
- "This job cannot be cancelled mid-step; it stops at its next checkpoint" (`TaskCentrePopover.tsx:127`)
- "N decisions are not yet written to Gmail (Projection)." (`SummaryPage.tsx:90`)
- Raw enums as labels: Mesh/Find/Signal/Expedition status chips (`MeshesPage.tsx:103`, `FindsPage.tsx:144`, `SignalsPage.tsx:61`, `ExpeditionsPage.tsx:46`); job priority (`TaskCentrePopover.tsx:112`); Gmail label ids (`MessagePage.tsx:191`, `InboxPage.tsx:129`); action names in the Activity filter (`ActivityPage.tsx:67`); "resolved by thread_state" (`FindsPage.tsx:205`).
- Raw JSON as the "real arguments" in hold, two-step (non-send) and approval dialogs (`ConfirmHost.tsx:31`, `TwoStepDialog.tsx:44`, `ApprovalDialog.tsx:26`).

### 4.8 Tone

Consistent: calm, declarative, first-person-free, honest about limits ("Riddle only works while the machine is awake; Attention Zero cannot be promised for mail it has not seen yet.", `TaskCentrePopover.tsx:67`; "Sent mail cannot be unsent.", `composer.tsx:188`). Occasionally it lectures where a control would do (Activity subtitle, Signals compare footnote). Sentence case throughout, with the domain nouns capitalised (Find, Spoil, Mesh) — a reasonable house rule, but it makes "Filed" and "Find" look like proper nouns next to lowercase raw enums.

---

## 5. Heuristic evaluation (Nielsen)

**H1 Visibility of system status**
- The autonomy level and Gmail write mode of the Dig you are looking at are invisible outside Settings → Accounts. The top bar shows sync status only (`DigSwitcher.tsx:25-35`); toasts never mention Gmail. — `useAction.ts:174-229`, `DigSwitcher.tsx` — **3**
- Pending Director proposals and the rail badge exist only in session memory (`stores.ts:158-162`, `DirectorPage.tsx:262`) — **3**
- Rail badges and the Summary ignore the Dig filter while pages honour it (`Rail.tsx:26-27`, `SummaryPage.tsx:38`, `api.ts:96`) — **2**
- Message-page disposition chip depends on the `explain` query and reads "Unsifted" until it loads (`MessagePage.tsx:63,89`) — **1**
- No running-turn indicator for the Director outside the page; no Stop (`DirectorPage.tsx:185-201`) — **2**

**H2 Match between system and the real world**
- Spoil with a trash icon; Mesh with a network icon; Strata for overlapping tags (`Rail.tsx:34,37`) — **2**
- Raw enum values and Gmail label ids as labels (§4.7) — **2**
- Vendor and ML vocabulary in the primary UI: "View Jev", "Noul", "Platt scaling", "engineIdentity" (`Bits.tsx:269`, `SignalsPage.tsx:36,148-161`) — **3**
- "Projection" as a user-facing word (`SummaryPage.tsx:90`, `selectors.ts:37`) — **2**
- Snooze keyboard = 24 h, unlabeled (`InboxPage.tsx:89`, `MessagePage.tsx:56`) — **2**

**H3 User control and freedom**
- No way to sign in again to an expired Dig; the "Sign in" affordance leads to a pane without a sign-in (`selectors.ts:302`, `ErrorCard.tsx:22`, `AccountDetail.tsx`) — **4**
- Questions typed in Ask Riddle and "Talk to the Director about this" send without a chance to edit (`SearchBox.tsx:34-36`, `FindsPage.tsx:238`) — **2**
- Proposal **Dismiss** is local only; the pending action stays server-side (`DirectorPage.tsx:293-302`) — **2**
- Undo is broad and honest (toast, `u`, Activity, Undo All with skips) — strength.

**H4 Consistency and standards**
- Single-click opens Finds/Unresolved but not Inbox/Spoil/Contamination (`MessageList.tsx:65-66` vs `FindsPage.tsx:130-133`) — **3**
- "Not important" (dismiss Find) vs "Not Important" (file the message) (`FindsPage.tsx:219`, `Bits.tsx:283`) — **3**
- `f`, `s`, `e`, `r`, `n` mean different things on different pages with no mode indicator (§3.2) — **2**
- "Dig" vs "account" (§1.1) — **2**
- Mesh **Delete** lacks the ellipsis every other destructive button has (`MeshesPage.tsx:174`) — **1**
- No `⌘,` for Settings (`shortcuts.ts:41`) contrary to `APPLICATION_FEATURES.md` §10 — **1**

**H5 Error prevention**
- **Send** and **Approve** are autofocused so Enter completes the confirmation the dialog asks the user to read (`TwoStepDialog.tsx:51`, `ApprovalDialog.tsx:32`) — **3**
- `is:unread` and similar operator queries are routed to the Director and sent (`SearchBox.tsx:13`) — **3**
- `f` on Unresolved creates a Respond Find with no type choice (`UnresolvedPage.tsx:39`) — **2**
- Signal **Test** spends money with no cost estimate shown first (`SignalsPage.tsx:93-101,206-208`) — **2**
- Review-page buttons act on the cursor row when nothing is checked (`ReviewPage.tsx:32,73-83`) — **2**
- Strengths: type-the-count with the count from the Action layer; hold with coaching; "Add at least one recipient"; Contamination warning before restore.

**H6 Recognition rather than recall**
- `g`-chords and `t`/`u` are discoverable only from the `?` sheet; the rail shows no key hints (`Rail.tsx:61-71`) — **2**
- The Mesh DSL is learnt from a static syntax box; no completion, no Signal links (`MeshesPage.tsx:410-433`) — **2**
- The **Correct…** menu does not show the current disposition (`MessagePage.tsx:65-71`) — **2**
- Inbox operators exist only in a placeholder (`InboxPage.tsx:125`) — **1**
- The Find's plain-English `reason` is never shown (`FindsPage.tsx`, none) — **3**

**H7 Flexibility and efficiency of use**
- No next/previous on the message page; no preview pane in the Inbox (`MessagePage.tsx`) — **3**
- No multi-select or bulk decide on Finds/Unresolved (`FindsPage.tsx:65`, `UnresolvedPage.tsx:32`) — **2**
- No arrow keys, Home/End in lists (`useListNav.ts:26-45`) — **2**
- No search or sort on Finds; 200-item cap (`FindsPage.tsx:55`) — **2**

**H8 Aesthetic and minimalist design**
- Why? card leads with a route walk in monospace instead of one sentence (`Bits.tsx:236-309`) — **3** for non-developers, **1** for the intended user
- Eight header controls on the message page; seven filter controls on the Inbox (`MessagePage.tsx:94-115`, `InboxPage.tsx:123-137`) — **2**
- 10–11px type for chips, rail group titles, row metadata; 13px body (`index.css:71`, `Rail.tsx:59`, `Chip.tsx:15`) — **2**
- Help sheet is one long dialog (keyboard + four tables) with no search (`HelpSheet.tsx`) — **1**

**H9 Help users recognise, diagnose, and recover from errors**
- "Sign in" button that cannot sign in (`ErrorCard.tsx:22`) — **4**
- Action failures are 8-second toasts; no error centre; failed Activity entries exist but are not linked from the toast (`useAction.ts:237`) — **2**
- Store-fault banner offers only Developer Mode (`Banners.tsx:18-20`) — **2**
- Mesh parse errors with line markers — strength.

**H10 Help and documentation**
- The Google Cloud setup is a repo file path in monospace, not opened by the app (`AddAccountWizard.tsx:122`, `AccountsTab.tsx:176`) — **3**
- The tour promises a Spoil digest that does not exist (`Tour.tsx:19`) — **3**
- Strata "retire" (`StrataPage.tsx:52`), "Review and relaunch" (`Banners.tsx:32`), "You edit this list; Riddle cannot." (`SettingsPage.tsx:167`) promise or deny things the code contradicts — **2**
- `?` sheet and per-row `?` tables driven from one source — strength.

---

## 6. Trust and autonomy

What the UI does well:
- One sentence per account states the current effect: "Right now: Riddle classifies only. At Act it would add labels in Gmail." and "Held because autonomy is observe." (`AccountDetail.tsx:83-90,130-139`, `model.ts:56-67`). New accounts start Observe/None by construction (`AddAccountWizard.tsx:77`).
- The `?` table's Gmail column is computed per account and says exactly what a button writes (`actionHelp.ts:41-48`).
- Every Sift/Mesh/Director change lands in Activity with actor, provenance, why and Undo/Undo All; undone entries are struck through; partial undos list what was skipped (`ActivityPage.tsx:95-180`).
- Send shows the Action layer's real recipients and "Sent mail cannot be unsent." (`TwoStepDialog.tsx`); egress after untrusted reads is approval-gated with the reason on screen (`DirectorPage.tsx:156-163`, `ApprovalDialog.tsx:18-22`).
- The Shadow card and "Would be Spoil" chips distinguish proposals from actions (`SummaryPage.tsx:104-114`, `Bits.tsx:25-32`).
- Remote images blocked with the reason "the sender cannot see that you opened this"; links neutralised and listed (`MessageBody.tsx:93,109-130`).
- Degraded states say "Nothing is hidden" everywhere (`selectors.ts:39,70`, `ErrorCard.tsx:49`).

Where a user could be surprised:
1. **At the moment of acting**, nothing says whether Gmail changed. "Moved 3 to Spoil" at Act + "Labels and archiving" archived three messages in Gmail; the toast is identical at Observe/None. The Summary's "41 decisions are not yet written to Gmail (Projection)" is the only ambient hint, and it uses a word the user has not been taught. Severity 3.
2. **The Dig switcher shows sync status but not autonomy.** A user with Work at Act and Personal at Observe sees the same chrome for both.
3. **"Review Proposals"** on the Shadow card leads to the Inbox titled "Shadow proposals — what Riddle would have done; nothing applied" (`SummaryPage.tsx:110`, `InboxPage.tsx:99-100`). There is no accept/reject per row; the toolbar buttons act for real (Archive, Spoil, Contamination). The Summary reports "You disagreed with 6 of its choices" without saying how disagreement is expressed. Severity 3.
4. **Activation of a Spoil-proposing Mesh** is guarded by a hold (`MeshesPage.tsx:181-189`), but the rail, Summary and Inbox never show "3 Meshes are active and hiding mail". After activation the only trace is Activity.
5. **Never Hidden** is described as user-only (`SettingsPage.tsx:167`) yet the Director can add to it after approval (`actionHelp.ts:65`, `DirectorPage.tsx:160`).
6. **Corrections' effect is opaque**: the user is told a correction never changes a rule by itself, but never sees corrections accumulate or which Mesh they count against; the Director proposal appears "when the same kind keeps coming up" (`UnresolvedPage.tsx:110`) with no visible threshold.
7. **Proposals vanish on restart** (§2g), so an approved-later proposal may be forgotten while its pending action persists in the backend.
8. **Removing an account** is the only recovery from an expired sign-in and wipes learned state (§2b). The inline dialog says "This cannot be undone" honestly (`AccountDetail.tsx:305`), which makes the absence of a reconnect path more painful, not less.
9. **Director costs** are visible per turn (`DirectorPage.tsx:171`), but the Signal **Test** button and simulation backfills only show cost afterwards or in the report; daily budgets live three tabs away.
10. **Reversibility feels good** wherever Undo appears (most places). The exceptions are labelled ("Cannot undo: sent mail cannot be unsent"), which is the right behaviour.

---

## 7. Scalability

**10 accounts.** The Dig switcher becomes a 12-line menu with status per line (`DigSwitcher.tsx:13-17`) — fine. The Accounts list adds a search at six (`AccountsTab.tsx:35`) and groups by Dig Group — fine. The Task Centre lists every Dig chip in a 440px popover (`TaskCentrePopover.tsx:49-63`) — fine to ~15. Inbox rows carry the Dig name per row (`MessageList.tsx:83`) — fine. What breaks: the rail has no per-Dig counts, the Summary is always "All Digs", and the Dig filter resets to All on every launch (`stores.ts:82`, not persisted). Meshes scoped "Dig: Work" and Strata scoped per group are listed flat with no filter by Dig (`MeshesPage.tsx:95`, `StrataPage.tsx:67`). Expedition scope is a row of ten checkboxes (`ExpeditionsPage.tsx:168-175`).

**500 Finds.** `listFinds` requests one page of 200 (`FindsPage.tsx:55`) and there is no "Load more"; the header total will say 500 while 200 are shown, and the rail badge says 500. Grouping into up to eight sticky sections with no collapse means `j` walks 200 rows linearly; there is no search, no sort by due/priority within a group, no multi-select. Every `find_changed` event invalidates the whole list (`events.ts:59-63`). The same 200 cap applies to Unresolved (`UnresolvedPage.tsx:30`). The notification threshold is a single global number (`SettingsPage.tsx:105`), so at scale either everything or nothing notifies. Severity 3 at this scale.

**10 Meshes.** The list is fine; the problems are cognitive: no display of precedence between Meshes (§2h), no per-Mesh count of decisions or corrections in the normal UI, and each Why? card names one Mesh version without a link back to the version's diff (it links to the Mesh by name, `Bits.tsx:277`). The editor gutter drift (§2h) starts to matter once a Mesh exceeds ~40 lines.

**Long threads.** The message page renders every message in the thread (`MessagePage.tsx:125-131`); only the opened one is expanded, each expanded body is a sandboxed iframe sized from line count and capped at 1200px with an internal scrollbar (`MessageBody.tsx:82-83,101-108`). Reply-tree rails stop indenting at depth 6 and print "depth N" (`ThreadRow.tsx:18,52`). A 100-message thread is 100 collapsed cards — workable; there is no "expand all"/"collapse all" and no jump to the latest. Find detail expands the latest source by default (`FindsPage.tsx:193`). Director conversations render all turns and scroll to bottom on each delta (`DirectorPage.tsx:90-92`); no virtualisation, but fine to hundreds of turns.

**Activity.** Paged at 40 with Load more and day grouping (`ActivityPage.tsx:47,83-89`) — scales. The Action filter lists raw action names, which grows with the registry.

---

## 8. Top 20 problems, ranked by impact × frequency

1. **No way to sign in again to an expired or revoked Dig**; "Sign in" leads to a pane without a sign-in; recovery is Remove (wipes learned state) + re-add. Weekly for External/Testing Gmail clients. → Add **Sign in again** to `AccountDetail` (reuse `connect_gmail` for an existing Dig) and make `provider_auth` land on it. (`selectors.ts:302`, `ErrorCard.tsx:22`, `AccountDetail.tsx`)
2. **First run asserts "Nothing needs your attention."** with no accounts and offers Review Spoil/Undo; the tour tours empty pages and promises a digest. → Gate the Summary on `digCount === 0` with an Add-account card; run the tour after the first sync; drop the digest sentence. (`SummaryPage.tsx:56-65`, `Tour.tsx:19`)
3. **Inbox/Spoil/Contamination need double-click or Enter to open; no preview pane; no next/prev on the message page.** → Single-click opens into a right-hand preview pane (as Finds/Unresolved already do) with `j`/`k` continuing to work. (`MessageList.tsx:65-66`, `MessagePage.tsx`)
4. **Gmail effect invisible at the point of action.** → Toast suffix computed from `gmailEffect`: "Moved to Spoil · archived in Gmail" / "· Riddle only"; an autonomy/projection chip in the Dig switcher. (`useAction.ts:174-229`, `DigSwitcher.tsx`)
5. **Keyboard dead end on Finds after acting on a URL-selected Find.** → When `findId` is no longer in the list, fall back to the cursor and replace the URL. (`FindsPage.tsx:66`)
6. **Ask Riddle misroutes operator queries (`is:unread`) to the Director and sends at once.** → Treat anything containing `:` operators as search; require a confirm step or explicit prefix for Director sends. (`SearchBox.tsx:13,34-36`)
7. **"View Jev" / "View Reasoning" dead-end in Developer Mode for normal users**, and expose a vendor name. → Hide or relabel ("Inspect decision") when `developerMode` is off. (`Bits.tsx:268-270`, `ActivityPage.tsx:155`)
8. **Approve/Send autofocused; Enter completes the second step.** → Autofocus Cancel/Reject, or require a click on the primary. (`ApprovalDialog.tsx:32`, `TwoStepDialog.tsx:51`)
9. **Shadow "Review Proposals" is a plain inbox with no accept/reject.** → A proposal review list with Agree/Disagree per row that feeds the agreement count. (`SummaryPage.tsx:110`, `InboxPage.tsx:99-100`)
10. **Proposals and their badge are session-only.** → Fetch `listPending` on mount; make Dismiss a backend call. (`stores.ts:158-162`, `DirectorPage.tsx:262,293-302`)
11. **The Find's plain-English `reason` is never shown; the Why? card leads with a monospace route walk.** → Show `find.reason` as the first line of the detail and of the Why? card; collapse the route walk. (`FindsPage.tsx:181-292`, `Bits.tsx:240-251`)
12. **Director answers, Find text and Why? cards cannot be selected or copied.** → Add `.selectable` to content regions or drop `user-select: none` from `body`. (`index.css:70`, `DirectorPage.tsx:38-55`)
13. **"Not important" means two different Actions.** → Rename the Why?-card button "File as done" or make it dismiss the Find. (`FindsPage.tsx:219`, `MessagePage.tsx:137`)
14. **Rail badges and Summary ignore the Dig filter.** → Pass `digId` to `summary`, or label the badges "all Digs". (`Rail.tsx:26-27`, `api.ts:96`)
15. **"Any list" shortcuts are not on any list; `x` no-ops on Finds/Unresolved; `f` hard-codes Respond; `s` hard-codes 24 h.** → Bind `useListNav` on every list, open the type picker on `f`, open the snooze picker on `s`. (`UnresolvedPage.tsx:39`, `InboxPage.tsx:89`, `MessagePage.tsx:56`)
16. **Metaphor/icon mismatches** (Spoil = trash icon, Mesh = network icon, "Excavation" heading, Strata as layers). → Sieve/heap icons; rename the group "Review" + "Rules"; one-line definitions in the `?` sheet. (`Rail.tsx:32-40`)
17. **Google client setup is a repo file path.** → Open the guide in the browser from the wizard; show the six console steps inline. (`AddAccountWizard.tsx:122`)
18. **Finds and Unresolved capped at 200 with no paging, search, sort or bulk.** → Infinite paging as MessageList; multi-select; sort within groups. (`FindsPage.tsx:55`, `UnresolvedPage.tsx:30`)
19. **Raw enum and system strings as UI labels** (status chips, Gmail label ids, `thread_state`, action names, JSON arguments). → Label maps like `findTypeLabel`; render real arguments as key/value rows. (§4.7)
20. **Director Enter sends; no Stop; "Talk to the Director" auto-sends a prompt containing raw ids.** → ⌘↵ to send, Enter for newline; a Stop button; prefill without `send=1` and keep ids out of the visible text. (`DirectorPage.tsx:211-216`, `selectors.ts:346`)

Also worth fixing, below the cut: Mesh editor gutter drift (`MeshesPage.tsx:343-359`); "Open {name}" in the wizard opens Settings (`AddAccountWizard.tsx:198`); Strata "retire" absent (`StrataPage.tsx:52`); "Review and relaunch" without a relaunch (`Banners.tsx:32`); Never Hidden "Riddle cannot" contradiction (`SettingsPage.tsx:167`); "No  Finds." double space (`FindsPage.tsx:108`); Expedition evidence as id searches (`ExpeditionsPage.tsx:123`); notification threshold unexplained (`SettingsPage.tsx:101-105`); composer lacks attachments and contacts the README calls table stakes (`composer.tsx`, `README.md:776`).

---

## 9. What is already good

- **One shortcut table drives keys and help** (`lib/shortcuts.ts:1-5`, `HelpSheet.tsx`), and **one action-help table drives the per-row `?` and the sheet** (`lib/actionHelp.ts:1-12`). The `?` dialog's Riddle / Gmail / Teaches / Undo columns, computed for the account in view, are the best explanation of consequences I have seen in a mail client.
- **Honest status everywhere**: "Catching up" instead of a false Attention Zero (`selectors.ts:133-144`); the Task Centre's onboarding disclaimer; progress bars that only move on events; "Nothing is hidden" on every degraded path; "Not sifted yet" instead of a fake explanation.
- **The account model**: option cards with consequences, the "Right now:" sentence, "Held because…", Observe/None defaults, Trusted behind a hold with coaching, inline type-the-count removal, groups locked while rules use them (`accounts/*`).
- **Confirmation tiers are structural and re-validated**: real recipients, the exact count from the Action layer, hold with keyboard support and coaching, approval showing real arguments, and an in-place Undo tier for everything else (`components/confirm/*`, `lib/confirm.ts`).
- **Undo is real**: toast Undo, global `u`, Activity Undo/Undo All with skipped lists, undone entries struck through, "Cannot undo: <reason>", and help text that says when undo withdraws a correction.
- **Activity reads like the README promised**: actor chips, "Triggered by Mesh: … v7" as a link, why lines, View Messages / Edit Mesh / View Find (`ActivityPage.tsx:95-180`).
- **Explanations exist for every decision** (Why? card on Finds, Unresolved and messages; route walk with actual values; LLM rationale as a blockquote; "Shadow: proposed, not applied") — the content is there, it only needs a plain-English layer on top.
- **Security-minded mail rendering**: sandboxed `srcdoc`, remote images off with a reason, links neutralised and listed with scheme allow-listing, "auth failed" and Reply-To mismatch chips, a Contamination note that says exactly what is disabled (`MessageBody.tsx`, `MessagePage.tsx:118-124,161,178-183`).
- **Director transparency**: tool-call cards with latency and an Activity link, per-turn cost and tokens, the untrusted-content banner and chip with a tooltip that explains egress gating, proposals previewed on real arguments.
- **Mesh editing** is text with live validation, line markers, a change note, version diffs, Preview impact with cost, Keep vs Apply, and a hold before a Spoil-proposing Mesh may act.
- **Empty states are instructions or proofs** without exception; **disabled controls always say why** (`Button.tsx:71-78`, used consistently).
- **Wizard**: one decision per step, effect sentence, arming explained in place, sign-in that waits with a retry link and a five-minute timeout with a clear failure card.
- **Dark mode via tokens**, a distinctive archaeology palette, thin scrollbars, focus-visible rings — the visual system is coherent even where the metaphor is not.
