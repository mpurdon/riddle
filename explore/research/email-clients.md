# Email client UX research — triage, categorisation, correction, safety

Compiled 2026-09-25 from fetched vendor docs, changelogs, launch posts and reviews.
Every claim below carries a source number that maps to §5. Items I could not
reach a primary source for are marked **[unverified]** or **[secondary only]**.
Superhuman's help centre (help.superhuman.com) sits behind a bot check, so its
help text is cited from search-engine snippets plus Superhuman's own changelog
(new.superhuman.com), which is fetchable. Quotes are kept under 15 words.

---

## Part A — Client by client

### 1. Superhuman (Superhuman Mail, part of the Superhuman suite since late 2025)

**(a) Triage model.** "Fly through email twice as fast": a full-screen, single-pane
list processed top-down with the keyboard to Inbox Zero. Everything actioned goes
to **Done** (archive), not trash. Split Inbox partitions the inbox into 3–7
streams so each is processed as a batch [S1][S2][S9].

**(b) Grouping, visibility of "why", correction.**
- Default splits: **Important • Other**, Shared, Team, VIP. Important = "person-to-
  person and high-priority"; Other = mailing lists and automated mail (marketing,
  social, updates). Each split shows the *total* count, not unread [S1].
- **Auto Labels** (AI) tag marketing, cold pitches, social updates, "response
  needed", "waiting on", "meetings"; custom labels from short prompts such as "job
  applications" (Business/Enterprise) [S2][S3][S11]. **Auto Archive** removes the
  marketing/cold-pitch/social classes from the inbox; toggled per class under
  Settings → Auto Label Settings [S4].
- Correction UI: `Cmd+K → Move to Important` / `Move to Other`, then choose
  **single message, sender, or domain**; mobile: two-finger tap → Move [S5][S6].
  Whether a per-message move feeds a learned model is **not documented**; the
  documented persistence is the sender/domain rule.
- No "why was this classified" affordance is documented anywhere I could reach
  **[not found]**.
- Custom splits are search queries, so their logic is fully inspectable [S1].

**(c) Keyboard/speed.** 100+ shortcuts; `j/k` move, `e` Done, `h` snooze, `l`
label, `/` search, `Shift+/` AI search, `Cmd+K` palette, `Cmd+Enter` send,
`Cmd+Shift+Enter` send-and-Done. Reviewers measure the palette under 100 ms [S9][S10][S12].

**(d) Layout.** Single pane, full screen, no alternative layouts; Zapier calls it
"text-heavy" and "densely clustered" [S13]. Thread view is a chronological stack
with a one-line **Auto Summarize** header [S10].

**(e) Onboarding.** Concierge flow: wait-list email → 7-question survey →
30-minute screen-share on the user's real inbox (splits, snippets, shortcuts, one
"magic moment") → same-day cheat-sheet + referral credit. Claimed: doubles
activation and referrals; one specialist ≈ $650k ARR/yr [S7]. Reviewers call it
mandatory and 30–45 min [S9][S14]. **Get Me To Zero**: `Cmd+K → Get Me To Zero`
moves everything older than a chosen age (default 1 week) to Done; undoable [S8].

**(f) Automation and guard-rails.** Undo send; Get Me To Zero is reversible;
Auto Labels *tag* rather than move, except Auto Archive which does move to Done
and is per-class opt-out [S4][S11]. Auto Drafts 2.0 writes drafts into the
mailbox before you open the thread; Superhuman claims "60% are sent unedited"
[S6]. Superhuman Go (Nov 2025) adds proactive agents across apps; the launch post
states no email-specific guard-rails [S15][S16].

**(g) Notifications.** Standard push per account; VIP split is the intended way
to get selective attention. No published "quiet by default" stance **[not found]**.

**(h) Praise / complaints.** Praise: speed, Split Inbox ("best-designed feature"),
onboarding, polish [S9][S10][S12]. Complaints: $30–40/month; steep learning curve;
no unified inbox; Auto Archive "missed crucial ones" when the filter was wrong
[S13]; Auto Drafts "can misread context" [S11]; AI features slowed the client
(Trustpilot, mid-2025) [S12 — secondary]; mobile loses the keyboard advantage;
read-receipt ethics [S17].

**(i) Sources.** S1–S17.

---

### 2. HEY (37signals)

**(a) Triage model.** Consent-based email. First-time senders wait in **The
Screener**; you say yes/no. Approved mail lands in the **Imbox** (important),
**The Feed** (newsletters, pre-opened, scrolled like a feed) or **Paper Trail**
(receipts). There is no AI; HEY's founders argue AI drafting erodes authentic
correspondence [S18][S19][S20][S24].

**(b) Grouping and correction.** Routing is a *per-sender rule the user set*.
Change it by tapping the sender avatar → move this email, or all mail from this
contact, to Feed/Paper Trail/Imbox [S19]. Screener decisions are reversible via
**Screener History**; screening a sender back in restores their last 90 days of
mail; rejected senders are never notified [S21]. "Why" is trivially visible
because the user made every decision. **Speakeasy** codes let a stranger bypass the
Screener [S20].

**(c) Keyboard/speed.** Shortcuts exist but are not the pitch; one convert calls
it "a little slow or a little clunky" [S25].

**(d) Layout.** Single column. Imbox splits **New For You** (unread) from
**Previously Seen**; **Reply Later** and **Set Aside** are piles at the bottom of
the screen; **Focus & Reply** shows only Reply Later threads with reply boxes;
**Bundles** collapse prolific senders to one row; **Cover Art** hides seen mail;
**Workflows** (2025–26) are drag-between-stages boards [S18][S20][S22][S23].
One reviewer found the desktop "cramped at default resolution" [S26].

**(e) Onboarding.** New @hey.com address (or HEY for Domains); imported mail must
be screened, which Palmer found tedious [S26]; tapsmart flags the new-address
requirement and (in 2020) no multi-account [S27].

**(f) Guard-rails.** No auto-send, no auto-delete; screening out is
non-destructive and recoverable [S21]. Opened Imbox mail auto-moves to
Previously Seen, which Palmer says fights "keep it visible until actioned"
workflows [S26].

**(g) Notifications.** Off by default for everything; no unread badge; enable
per contact or thread. HEY: unread indicators "get in the way" of focus [S22].

**(h) Praise / complaints.** Praise: control, calm, spy-pixel blocking, the
Screener makes you "feel in control" [S27][S25]. Complaints: $99/yr; lock-in;
Screener adds friction for low-volume inboxes ("solves problems he doesn't
face") [S26]; does not cut time per message [S24].

**(i) Sources.** S18–S27.

---

### 3. Gmail (incl. Priority Inbox, tabs, Inbox by Gmail, Gemini 2025–26)

**(a) Triage model.** A label-based archive with optional machine sorting layered
on top: importance markers, Priority Inbox sections, category tabs, and (2026) an
AI Inbox briefing view.

**(b) Grouping, "why", correction.**
- **Priority Inbox** (beta, 30 Aug 2010): sections "Important and unread",
  "Starred", "Everything else". Signals: people you email most, what you open and
  reply to. Correct with the +/- buttons; filters can force always/never
  important; sections are customisable [S28]. Dec 2010 update added markers in
  every inbox view and a **"why is this important" explanation** [S29].
- **Importance markers** today: hover the yellow marker to see the reason; click
  to flip it; "This also helps Gmail learn". Opt-outs: "Don't use my past actions
  to predict…" or "No markers" [S30].
- **Tabs** (29 May 2013): Primary/Social/Promotions/Updates/Forums; choose which
  show; drag a message to a tab; set senders to always land in a tab; star to
  also show in Primary [S31]. Help: drag → confirmation toast with **Undo**;
  "helps Gmail learn your preferences"; you cannot create categories; filters can
  pin a sender to a category; notifications fire for Primary only [S32].
- **Inbox by Gmail** (2014–2019): topic **Bundles** (Travel, Purchases, Finance…),
  Highlights, Reminders, Snooze, Pin, Sweep. Retired Mar/Apr 2019 to "focus
  solely on Gmail"; snooze, nudges, hover actions and Smart Reply migrated;
  bundles did not (trip bundles the notable loss) [S33][S34].
- **Gemini**: auto summary cards on long threads (May 2025), on by default in
  most regions, off via Settings → Smart features, admin-disableable [S35][S36].
  Jan 8 2026 (Gemini 3): AI Overviews of threads (free), "ask your inbox"
  (Pro/Ultra), Help me write (free), Proofread (Pro/Ultra), and **AI Inbox**
  (trusted testers): a sparkle view above Inbox listing suggested to-dos and
  "topics to catch up on", with VIPs inferred from communication patterns [S37][S38].
  A one-click-approval "inbox cleanup" is reported as coming **[secondary only]**
  [S39]. No per-item "why" is documented for AI Inbox **[not found]**.

**(c) Keyboard.** Off by default; `j/k`, `o`, `u`, `e`, `#`, `r/a/f`, `s`,
`+/-` importance, `l`, `v`, `x`, `z` undo, `g i`, `?` help [S40].

**(d) Layout.** Three-pane optional (reading pane), density presets, conversation
threading; AI Inbox replaces messages with summaries [S37].

**(e) Onboarding.** None of note; Priority Inbox and tabs were opt-in prompts.

**(f) Guard-rails.** Undo toast for tab moves [S32]; **Undo Send** window 5/10/20/30 s
[S41]; importance learning can be disabled [S30]. Gemini summaries are default-on
in most regions [S35]; CNBC reported users must opt out of several Gemini
features **[secondary; page blocked]** [S36].

**(g) Notifications.** New-mail notifications only for Primary when tabs are on [S32].

**(h) Praise / complaints.** Inbox by Gmail: "minimalist and lovely" but low
density; users "give up the control" of their own system [S33]. Gemini: default-on
AI plus a track record of AI Overview mistakes raised reliability concerns [S35].
Importance marker "why" text is praised by third-party guides as the reason the
feature is trusted [S30].

**(i) Sources.** S28–S41.

---

### 4. Outlook (Focused Inbox, Sweep, Rules, Copilot)

**(a) Triage model.** Two-tab inbox (**Focused / Other**) driven by a server-side
model; Sweep and Rules for bulk hygiene; Copilot adds summaries, drafts and a
priority score.

**(b) Grouping, "why", correction.** Focused uses "contacts with whom you
interact" and filters bulk/automated mail; "the more you use it, the better it
gets". Right-click → **Move to Other / Focused** or **Always move to…** (a
sender override). Turn off: View settings → "Don't sort my messages". Replaced
Clutter. **No explanation of why** a message landed in either tab [S42][S43].
Office-Watch: overrides live on a hidden server-side list you cannot view; the
only undo is another "Always move"; Rules cannot target Focused; no drag-and-drop
between tabs; early versions "didn't sensibly distinguish" [S44]. Institutions
publish "turn it off" guides after important mail was missed in Other
**[search snippet; JHU page blocked]** [S45][S46].

**Copilot "Prioritize my inbox"** (Apr 2025+): high/low/normal; the preview line
is replaced by a **headline explaining why**; natural-language instructions
("email about Project Alpha from Joe Smith"); stored as a MAPI property
`_CopilotInboxScoreReason_`; needs a Copilot licence; no admin off switch [S47].
Microsoft's internal rollout found users valued that it "explains why" [S48].
Copilot does not send or move mail; every action needs the user [S49]. The mobile
priority view was **removed Feb 2026** "in response to user feedback and costs" [S50].

**(c) Keyboard.** Conventional Outlook shortcuts; not a differentiator.

**(d) Layout.** Three-pane; Focused/Other tabs above the list.

**(e) Onboarding.** Focused is on by default with a first-run banner; no guided setup.

**(f) Guard-rails.** **Sweep** offers: delete all from sender, keep only the
latest, delete older than 10 days; sweep rules "run once per day"; unavailable in
Junk/Drafts/Sent/Deleted [S51]. Archive, Pin, Snooze, Flag, Rules present [S51].

**(g) Notifications.** Focused-only notification option; Office-Watch notes the
"new mail in Other" toast cannot be disabled [S44].

**(h) Praise / complaints.** Praise: Copilot explanation headline [S47][S48].
Complaints: missed mail in Other, opaque overrides, no rule integration [S44][S45][S46];
Copilot licence gating and cost-driven removals [S50].

**(i) Sources.** S42–S51.

---

### 5. Apple Mail (iOS 18.2+, iPadOS 18.4+, macOS 15.4+; guides now for iOS/macOS 27)

**(a) Triage model.** Four fixed categories across the top of the inbox:
**Primary, Transactions, Updates, Promotions**; Primary is personal mail plus
anything time-sensitive; a List View toggle returns to chronological [S52][S53].

**(b) Grouping, "why", correction.** Time-sensitive Transactions/Updates/
Promotions mail is *also* shown in Primary. Tapping a T/U/P message opens a
**digest of that sender's messages in that category** (Primary is never grouped);
actions apply to the whole digest; "Group by Sender" can be turned off. Correct
via **Categorize Sender** (iPhone: tap sender/list menu; Mac: Control-click):
"All current and future messages from the sender" move. Not available in all
regions [S52][S53]. No per-message "why". (MacRumors' early how-to said there was
no way to correct a category; the shipped guides supersede that [S54].)
**Priority Messages** (Apple Intelligence, Oct 2024) is a section for "the most
urgent emails" such as a same-day invitation; criteria are not published [S55].

**(c) Keyboard.** Standard macOS Mail shortcuts; no triage vocabulary.

**(d) Layout.** Three-column on Mac; category chips above the list; digest view
replaces the sender's row with a stack [S52].

**(e) Onboarding.** Categories on by default after the OS update; a menu switch to
List View; "swipe left on the categories" reaches All Mail [S52].

**(f) Guard-rails.** Categorisation moves nothing on the server; it is a view.
Digest bulk actions are the only new destructive surface [S52].

**(g) Notifications.** Unchanged per-account rules; Apple Intelligence adds
notification summaries and a "Reduce Interruptions" Focus [S55].

**(h) Praise / complaints.** Praise: zero setup, time-sensitive override, sender
digests. Complaints: people who prefer chronological or their own system turn it
off; how-to sites lead with "how to turn off" [S54].

**(i) Sources.** S52–S55.

---

### 6. Spark (Readdle)

**(a) Triage model.** Smart Inbox: personal mail on top, notifications and
newsletters grouped below; **Priority** senders pinned at the top; **Gatekeeper**
screens new senders; Done / Set Aside / Snooze / Pins move things out [S56][S57].

**(b) Grouping, "why", correction.** Three inbox modes: Classic list; **Smart
Inbox** (unread cards: People / Notifications / Newsletters + Pins + Seen); **Smart
Inbox 2.0 / Focused List** (Priority, Pins, Notifications, Newsletters,
Invitations, delegated above the rest, no previews). Correction: open the
message → click the sender address → choose a category; "Spark will sort all
future emails from this sender". Priority is manual per sender/thread; you choose
how many Priority rows show (1–10 or all). No explanation of the automatic
grouping [S56]. Gatekeeper prompts per new sender (on top of the inbox or inside
the email), bulk thumbs-up/down; blocked senders are not notified; off = "Accept
all without asking"; blocks persist after the subscription lapses [S57].

**(c) Keyboard.** Command Center (`Cmd+K`) added in Spark 3 (Oct 2022) [S58].

**(d) Layout.** Three-pane with a Home screen "to reduce inbox visits" [S58].

**(e) Onboarding.** Account add + choice of inbox mode; Gatekeeper is a paid tier [S56][S57].

**(f) Guard-rails.** Categorisation is client-side grouping; AppleInsider forum
reports of deletes not syncing and threads auto-marked read [S59].

**(g) Notifications.** Smart notifications for People only (Smart Inbox); mute threads [S58].

**(h) Praise / complaints.** Praise: multi-account, swipe customisation, Gatekeeper
"like your personal assistant" [S59]. Complaints: Spark 3 "far too playful";
subscription; third-party server privacy; sync bugs; slow support [S58][S59].

**(i) Sources.** S56–S59.

---

### 7. Shortwave

**(a) Triage model.** Inbox Zero via **Splits** (tabs), **Bundles** (one row per
group), **Todos** and an AI assistant that proposes bulk actions. The onboarding
"Shortwave Method" is a single triage pass: not actionable / quick / longer [S60][S61].

**(b) Grouping, "why", correction.** Splits are search queries, importance,
built-in labels, custom labels or AI filters; you can drag a thread onto a label
split. Bundles group by label or sender; a split can auto-bundle per sender [S60].
Built-in Smart Labels: Newsletters, Promotions, Social, Calendar, Updates,
Purchases, Finance, Travel. **Magic Labels** remember how you labelled a sender and
apply it going forward (sender-pattern, not content AI); a toast confirms each
auto-label with **Options** and `Cmd+Z` [S62]. Labelling shows a checkbox "Apply
changes to future messages"; de-selecting a label creates an *always remove*
rule; all such rules are listed and removable under Settings → Filters [S63].
AI filters take natural-language descriptions ("Needs Action", "Cold Outreach",
"FYI") [S64]. Content-based "why" is not shown **[not found]**.

**(c) Keyboard.** `Cmd+J` opens the assistant; Gmail-style shortcuts [S65].

**(d) Layout.** Three-panel; thread opens as split, side panel or full screen [S61][S13].

**(e) Onboarding.** Guided first pass over the whole inbox [S61].

**(f) Guard-rails.** The assistant "recommends actions" and the user "stays fully in
control at each step": bulk archive/label suggestions require confirmation; saved
prompts have an explicit autosubmit toggle; AI Memories are user-edited [S65].

**(g) Notifications.** Per-label push and **delivery schedules** (e.g. newsletters
Saturday 8 am); Do Not Disturb with working hours [S64][S13].

**(h) Praise / complaints.** Praise: bundles "like Inbox", cleaner with less
effort, customisable layout [S13][S61]. Complaints: Gmail-only; AI "slower than
handling tasks manually"; unsubscribe one at a time; cloud AI privacy; free tier
90-day history [S61].

**(i) Sources.** S60–S65, S13.

---

### 8. Notion Mail (launched Apr 2025; **shutting down 22 Sep 2026**)

**(a) Triage model.** Database views over Gmail: filtered, grouped (priority,
label, unread) custom inboxes plus AI auto labels defined in prose [S66][S67].

**(b) Grouping, "why", correction.** Auto label flow: type a prompt ("Calendar
notifications for upcoming meetings"), Notion AI shows **sample matches**, you tick
✅ the relevant ones, and a sidebar view is created; manual tagging/untagging is
said to teach it **[help page returned only navigation; from search snippet]**
[S68][S66]. Reviewers: you cannot test or refine a label after creation and
cannot change its criteria or name without starting over [S69]. No per-message
explanation.

**(c) Keyboard.** `E` archive, `H` reminder, `U` unread, `Cmd+U` unsubscribe,
`Esc` back; `/` commands in compose [S67].

**(d) Layout.** Side peek / centre peek / full page for the open thread [S67].

**(e) Onboarding.** Connect Gmail, start; Gmail-only [S66].

**(f) Guard-rails.** No stated AI guard-rails at launch [S66].

**(g) Notifications.** Standard.

**(h) Praise / complaints.** Praise: views concept. Complaints: drafts "generic",
accepted 4 of 30 [S70]; robotic replies; English-optimised; shallow automation.
Shutdown reason: "more than half of Notion Mail users manage emails without ever
opening their inbox", so Notion is moving to agents; users must export drafts,
snippets and **auto-label instructions** [S71][S72].

**(i) Sources.** S66–S72.

---

### 9. Mimestream (native macOS Gmail client)

**(a) Triage model.** Gmail's own model, rendered natively: labels, categories,
server-side filters, importance/Priority Inbox, unified or separate inboxes [S73][S74][S75].

**(b) Grouping, "why", correction.** Whatever Gmail decides; Mimestream shows
Gmail categories and lets you manage Gmail filters in-app; corrections are
Gmail's [S73][S75]. No smart features of its own [S73].

**(c) Keyboard/speed.** Gmail shortcut set plus macOS conventions; `⇧⌘⏎`
send-and-archive; `⌘Z` undo send; "lightning-fast" via the Gmail API rather than
IMAP [S73][S74][S75].

**(d) Layout.** Apple-Mail-style three columns [S74].

**(e) Onboarding.** Sign in with Google; profiles per account [S73].

**(f) Guard-rails.** Undo send; nothing automated beyond Gmail filters [S73].

**(g) Notifications.** Per-account schedules; Focus Filters [S73][S75].

**(h) Praise / complaints.** Praise: speed, native feel, correct archive/delete
semantics, ex-Apple Mail engineer [S74][S75]. Complaints: $49.99/yr; Gmail-only;
Mac-only (iOS in beta); Cult of Mac noted snooze limits at the time [S75].

**(i) Sources.** S73–S75.

---

### 10. Missive (shared inbox)

**(a) Triage model.** Conversations across channels, assigned to a person or team,
with internal comments and statuses; rules do the routing [S76][S77].

**(b) Rules UX and transparency.** Triggers: incoming, outgoing, user action;
conditions (e.g. "From ends with"); several actions run in order; rules execute
alphabetically (prefix "01."); personal vs organisation scope; up to 1,000 each;
not retroactive; rules never trigger rules; spam skipped unless asked [S77][S78].
**Auditability**: rule events appear in the conversation timeline (hover "Rule"
to see which fired); webhook events can be shown; **AI rules** can "Log prompt
result in conversations" for debugging. AI rules use prompt conditions (fast
sync models) and actions: AI note, draft with AI, tasks with AI, labels with AI;
providers OpenAI/Anthropic/Gemini; content is sent only when a rule runs [S79].

**(c) Keyboard.** Missive and Gmail shortcut sets; `T` new task [S80].

**(d) Layout.** Three-column with in-thread team chat [S76].

**(e) Onboarding.** 30-day trial, no card; requires whole-team adoption [S76].

**(f) Guard-rails.** Draft-with-AI produces drafts; rule logs; BYO API key [S76][S79].

**(h) Praise / complaints.** Praise: "team email actually works", price.
Complaints: overkill for individuals; learning curve [S76].

**(i) Sources.** S76–S80.

---

### 11. Front (shared inbox, customer-ops platform)

**(a) Triage model.** Rules + tags + assignment + SLAs; Autopilot AI agent for
end-to-end workflows with escalation controls [S81][S82][S83].

**(b) Rules UX and transparency.** When → If → Then; a rule library grouped
Classify / Route / Accelerate / Report / Connect; caps 10/20/unlimited by plan;
matching is on "keywords, not meaning" [S81][S83]. **Required tagging** blocks
archiving until a tag is chosen [S83]. Legacy **AI Tagging** required approving 10
example messages per tag (max 50 tags) and is closed to new users; **Topics** and
Autopilot replace it [S83][S84]. AI features are admin toggles with zero data
retention [S82].

**(c)/(d)** Conventional three-pane with comments; polish praised [S81].

**(h) Complaints.** Price ($25–65+/seat), rule caps, keyword-only rules [S81][S83].

**(i) Sources.** S81–S84.

---

### 12. Proton Mail

**(a) Triage model.** Encrypted mailbox with six auto **Categories** (Primary,
Social, Promotions, Newsletters, Transactions, Updates) sorted from **metadata
only** so content stays zero-access encrypted [S85][S86].

**(b) Correction.** Move a message to another category and Proton "automatically
updates the filter for next time"; choose which categories show, toggle
notifications and unread counts per category; default on for new users, opt-in
for existing; disable any time [S85]. The blog page is dated 20 Aug 2026; the
feature was announced in the autumn-2025 roadmap [S85][S86]. No "why".

**(c) Keyboard.** Gmail-style chords (`G` then `I`); `?` shows the list; the 2021
post says off by default, the current support page says on by default
**[discrepancy]** [S87][S88]. Community threads complain shortcuts are unreliable [S89].

**(g) Notifications.** Per-category notification toggle [S85].

**(i) Sources.** S85–S89.

---

### 13. AI-native newcomers (2025–2026)

**Fyxer** (Gmail/Outlook add-on). Sorts into fixed labels: "To respond", "FYI",
"Comment", "Notification", "Meeting update", "To follow up", "Marketing" (its own
page says three buckets); learns from ~300 sent mails; drafts land in Drafts and
are never sent; 30-second setup; three organisation modes incl. "leave my inbox
alone" [S90][S91][S92]. Complaints: fixed categories; important mail stuck in FYI
so nobody replies; drafts "never improved over time"; overage fees [S92][S93].

**Cora** (Every). Screens Gmail so the inbox holds only human mail needing a
reply; everything else is labelled and archived and summarised in a twice-daily
**Brief** with action buttons; drafts in your voice; "will never send"; won't
delete; plain-language rules and in-app re-categorisation [S94][S95].

**Zero / Mail-0** (0.email). Open-source, self-hostable, Gmail first; AI
categorisation, chat-with-inbox, drafting, summaries; early-stage; critics raise
social-feed-style algorithmic control and transparency questions [S96][S97][S98].

**Inbox Zero** (getinboxzero.com). Open-source; plain-English rules; each rule
runs **automated or requires approval** (Pending tab); feedback via "fix" and "why
did this rule run" **[docs excerpt thin — partially unverified]**; cold-email
blocker; Gmail + Outlook [S99][S100].

**Gmail AI Inbox / Copilot Prioritize** — covered above; both lead with a
"why"/to-do surface rather than moving mail [S37][S47].

**Superhuman Go / Suite** — Grammarly rebranded as Superhuman (announced 29 Oct
2025; suite launch post dated 5 Nov 2025 **[dates differ across sources]**);
Go is a cross-app proactive agent; Mail exposes an MCP server so Claude/ChatGPT/
Codex can search and draft across accounts [S15][S16][S6].

---

## Part B — Synthesis

### 1. Comparison table

| Client | Triage model | Categorisation transparency ("why") | Correction loop | Keyboard model | Layout | Undo / safety | Onboarding |
|---|---|---|---|---|---|---|---|
| Superhuman | Split Inbox streams → Done | None documented; custom splits are visible queries | `Cmd+K` Move to Important/Other for message, sender or domain; Auto Label classes toggled in settings | 100+ keys, `Cmd+K`, <100 ms | Single pane, dense | Undo send; Get Me To Zero undoable; Auto Archive moves mail | 30-min concierge on real inbox; Get Me To Zero |
| HEY | Screener consent → Imbox / Feed / Paper Trail | Total: user made every rule | Avatar menu → move this or all from sender; Screener History, 90-day restore | Present, not central | Single column; piles at bottom | Nothing deleted; screened-out recoverable | New address; screen everyone (tedious) |
| Gmail | Markers, Priority Inbox, tabs, AI Inbox | Hover marker shows reason; AI Inbox shows source email per to-do | +/- marker; drag to tab → toast + Undo; filters always/never | Off by default; j/k/e/# vocabulary | 3-pane optional | Undo send 5–30 s; learning opt-out | Opt-in prompts |
| Inbox by Gmail (retired) | Topic bundles, Highlights | Bundle title only | Manual re-bundling "taught" it | Gmail-like | Card list, low density | Undo send | — |
| Outlook | Focused / Other; Copilot priority | Focused: none; Copilot: headline explaining why | Move / Always move (hidden override list) | Standard | 3-pane, tabs | Sweep rules daily; Copilot never sends/moves | Default on |
| Apple Mail | 4 categories + time-sensitive → Primary; Priority Messages | None per message; category definitions only | Categorize Sender (current and future) | Standard | 3-column; sender digests | View-only; digest bulk actions | Default on; List View escape |
| Spark | Smart Inbox groups; Priority; Gatekeeper | None | Sender → category (future mail); Gatekeeper accept/block | `Cmd+K` Command Center | 3-pane + Home | Blocks persist; sync bugs reported | Mode choice; Gatekeeper paid |
| Shortwave | Splits + Bundles + Todos + AI suggestions | Toast per auto-label; rules listed in Settings → Filters | "Apply to future messages" checkbox; always-remove rules; `Cmd+Z` | Gmail-like, `Cmd+J` | 3-panel, choosable | AI proposes, user confirms | "Shortwave Method" pass |
| Notion Mail (retired) | Views + prompt auto labels | Sample-and-tick before creation | Untag/tag said to teach; cannot edit a label's prompt | E/H/U shortcuts | Peek/centre/full | None stated | Connect Gmail |
| Mimestream | Gmail's model, native | Gmail's | Gmail filters in-app | Gmail set + macOS | 3-column | Undo send | Sign in |
| Missive | Assign + rules + comments | Rule name shown on the conversation timeline; AI prompt log | Edit ordered, scoped rules | Missive or Gmail set | 3-column + chat | Not retroactive; rules never chain | Team trial |
| Front | When/If/Then rules, tags, Autopilot | Rule library; keyword rules are explicit | Edit rules; Required tagging | Standard | 3-pane | Escalation controls; admin AI toggles | Templates |
| Proton | 6 metadata categories | None | Move → filter updates for next time | Gmail-like chords | 3-pane | Disable any time | Default on for new users |
| Fyxer | Fixed labels; drafts | None | Whitelist after errors | n/a (add-on) | Inside Gmail/Outlook | Never sends | 30 s |
| Cora | Screen + Brief | Brief cites sources | Chat / in-app recategorise | n/a | Brief email + Gmail | Never sends, never deletes | History analysis |
| Inbox Zero | Plain-English rules | "Why did this rule run" | Fix / approval mode | n/a | Web | Per-rule approval vs automated | Connect account |

### 2. Patterns that consistently work

1. **Sender-scoped corrections that persist and are visible.** Gmail tabs
   ("set senders to always appear"), Apple "Categorize Sender", Spark sender →
   category, Shortwave "Apply changes to future messages" + Settings → Filters,
   Superhuman message/sender/domain, Proton "updates the filter", HEY avatar menu
   [S31][S52][S56][S63][S5][S85][S19]. The user's mental model is "that sender",
   not "that model weight".
2. **Show the reason on the row.** Gmail's hover "why is this important"
   [S29][S30]; Copilot replaces the preview line with a why-headline and Microsoft's
   own users singled that out [S47][S48]; Cora's Brief cites the source mail [S94].
3. **Undo as a toast, on every automated or bulk move.** Gmail tab drag [S32],
   Shortwave auto-label toast with Options/`Cmd+Z` [S62], Get Me To Zero [S8],
   Undo Send everywhere [S41][S73].
4. **Nothing is destroyed by triage.** Done/archive instead of delete
   (Superhuman, Cora); Screener History with a 90-day restore (HEY); Proton and
   Apple categories are views [S8][S94][S21][S52][S85].
5. **AI drafts, humans send.** Fyxer, Cora, Copilot, Superhuman Auto Drafts, Missive
   "Create draft with AI" all stop at Drafts [S90][S94][S49][S6][S79].
6. **Propose, then confirm, for bulk actions.** Shortwave's assistant recommends
   and waits [S65]; Inbox Zero's per-rule approval mode [S99]; Gmail's reported
   one-click-approval cleanup [S39].
7. **Preview matches before creating an auto-rule.** Notion's sample-and-tick
   [S68], Front's legacy 10-example approval [S83], Missive's prompt-result log [S79].
8. **A stable, shared keyboard vocabulary.** `j/k/e/h/l//` and `Cmd+K` recur in
   Gmail, Superhuman, Mimestream, Missive, Notion Mail, Proton [S40][S10][S73][S80][S67][S87].
9. **Few buckets, whole counts.** Superhuman recommends 3–7 splits and shows
   total counts [S1]; Apple 4, Gmail 5, Proton 6 categories [S52][S32][S85].
10. **Quiet by default; notify only the attention tier.** HEY (off, no badge)
    [S22]; Gmail notifies Primary only [S32]; Proton per-category toggle [S85];
    Shortwave delivery schedules [S64].
11. **Time-sensitive overrides the category.** Apple copies time-critical mail
    into Primary [S52]; Gmail stars surface in Primary [S32]; HEY Speakeasy bypasses
    the Screener [S20].
12. **A digest for the non-urgent tier.** Apple sender digests [S52], HEY's Feed
    [S18], Cora's Brief [S94], Gmail AI Inbox to-dos [S37] make hidden mail
    discoverable without notifications.
13. **Start clean, on real mail.** Superhuman's concierge configures the user's
    own inbox and Get Me To Zero clears the backlog reversibly [S7][S8].

### 3. Anti-patterns and known failures

1. **Hidden overrides and no "why" (Outlook Focused).** Overrides are invisible,
   Rules cannot target Focused, and there is no explanation; the outcome is
   missed mail and institutional "turn it off" guides [S44][S45][S46].
2. **Auto-archiving on a classifier the user cannot inspect.** Superhuman Auto
   Archive "missed crucial ones" [S13]; Fyxer left reply-needed mail in FYI
   [S92][S93]. Archive-by-AI needs a visible reason and an easy path back.
3. **Bundles that override the user's own system (Inbox by Gmail).** Praised as
   lovely but low-density; reviewers warned users must "give up the control";
   Google retired it and never ported bundles [S33][S34].
4. **Forced whole-inbox re-screening (HEY).** Screening every existing sender at
   setup is tedious, and the Screener adds friction for people whose inbox is
   already expected mail [S26].
5. **Auto-labels you cannot edit or test (Notion Mail); fixed taxonomies (Fyxer).**
   Users could not refine a label without recreating it [S69]; Fyxer's fixed
   categories cannot match a user's workflow [S92].
6. **AI that slows the core loop.** Superhuman AI slowness complaints [S12];
   Shortwave AI "slower than handling tasks manually" [S61]; icebox: speed "not
   sufficient" but still necessary [S14].
7. **Default-on summaries with known error rates.** Gemini summaries default on
   in most regions against a record of AI Overview mistakes [S35]; Apple's
   notification summaries produced false alerts [S35].
8. **Prioritisation as a paid, removable add-on.** Copilot's mobile priority
   view was withdrawn for "user feedback and costs" [S50]; licence gating makes
   the triage layer inconsistent across devices [S47].
9. **Product/user mismatch.** HEY's Screener for low-volume users [S26];
   Superhuman for under 150 mails/day [S9]; Missive for individuals [S76].
10. **"Learns from you" without evidence.** Fyxer users saw no improvement over
    months [S93]; Focused Inbox "the more you use it" is unmeasurable [S43][S44].
11. **The inbox stops being opened (Notion Mail).** Once agents handle mail,
    users stop opening the client; Notion shut Mail down on that basis [S71].
    A triage app must remain the place where decisions are reviewed, or it dies.
12. **Keyword-only rules presented as smart.** Front's own docs concede rules
    match "keywords, not meaning" [S83]; Proton's metadata-only sort has the same
    ceiling by design [S85].

### 4. Ten transferable lessons for an attention-deciding app (rules + ML + approval-gated agent)

1. **Every disposition carries a human-readable reason, on the row.** Gmail's
   hover text [S30] and Copilot's why-headline [S47] are the two features users
   cite as the basis for trust; Focused Inbox's silence is the cited basis for
   distrust [S44]. Store the reason with the decision (Copilot persists it as a
   MAPI property [S47]) so it survives across surfaces and into logs.
2. **Corrections are scoped and become visible rules.** Offer "this message /
   this sender / this domain" (Superhuman [S5], Apple [S52], Shortwave [S63]) and
   list every resulting rule somewhere editable (Shortwave Settings → Filters
   [S63]; Missive ordered rules [S77]). Never keep an invisible override list [S44].
3. **Rules outrank the model, and the UI says which one fired.** Missive shows
   the rule name on the conversation timeline [S78]; Gmail lets filters force
   always/never important above the classifier [S28]. When a correction and a
   prediction conflict, the correction wins and the row says so.
4. **Triage never destroys.** Archive/Done, not delete (Superhuman [S8], Cora
   [S95]); recoverable screening with a retention window (HEY 90 days [S21]);
   view-only categorisation where possible (Apple, Proton [S52][S85]).
5. **Undo is a toast on every automated move, including bulk ones.** Gmail tab
   drag [S32], Shortwave label toast [S62], Get Me To Zero [S8]. Bulk and agent
   actions get the same affordance as single actions.
6. **A "never hidden" tier sits above every category and every agent.** Apple's
   time-sensitive → Primary [S52], Gmail's star → Primary [S32], HEY's Speakeasy
   [S20], VIP splits [S1]. Make the override visible in the row so users can see
   that the gate ran.
7. **Preview before automate; approve before act.** Show sample matches and ask
   for ticks before a rule goes live (Notion [S68], Front legacy [S83]); let each
   automation be set to "requires approval" vs "automated" (Inbox Zero [S99]);
   have the agent recommend and wait (Shortwave [S65]). Drafts never send
   (Fyxer, Cora, Copilot [S90][S94][S49]).
8. **Speed of the core loop is a trust feature; AI must not block it.**
   Sub-100 ms palette and `j/k/e/h` are why people pay for Superhuman [S10][S12];
   AI that delays opening a message is the top recent complaint [S12][S61].
   Compute classifications asynchronously and render "Unresolved" rather than
   spinners.
9. **Few buckets, whole counts, quiet defaults.** 3–7 streams with total counts
   [S1]; notifications only for the attention tier (HEY [S22], Gmail Primary
   [S32], Proton per-category [S85]); a scheduled digest for the rest (Cora Brief
   [S94], Apple digests [S52]).
10. **Onboard on the user's real mail, start from zero, and keep the review
    surface worth opening.** Superhuman's concierge configures the live inbox and
    Get Me To Zero clears the backlog reversibly [S7][S8]; avoid HEY's mandatory
    re-screen of every sender [S26]. Notion Mail died when users stopped opening
    it [S71]: the app's durable value is being the place where automated decisions
    are reviewed, explained and corrected.

### 5. Sources

Superhuman
- S1 Superhuman Help — Default Split Inbox (snippet; page bot-gated): https://help.superhuman.com/hc/en-us/articles/46005619081101-Default-Split-Inbox
- S2 Superhuman Help — Save Time with Superhuman AI (snippet): https://help.superhuman.com/hc/en-us/articles/46005790702861-Save-Time-with-Superhuman-AI
- S3 Superhuman Mail AI product page: https://superhuman.com/products/mail/ai
- S4 Superhuman changelog — Split Inbox controls: https://new.superhuman.com/split-inbox-controls-321318
- S5 Superhuman Help — Structure Your Inbox (snippet): https://help.superhuman.com/hc/en-us/articles/45271247561107-Structure-Your-Inbox ; changelog: https://new.superhuman.com/more-power-for-your-important-other-splits-216060
- S6 Superhuman Mail updates (changelog index): https://new.superhuman.com/
- S7 Flowjam — Superhuman onboarding teardown: https://www.flowjam.com/blog/superhuman-onboarding-teardown-30-minute-wow-session
- S8 Superhuman changelog — Get Me To Zero: https://new.superhuman.com/get-me-to-zero-260284
- S9 CMDK — Superhuman review 2026: https://cmdk.email/post/superhuman-review/
- S10 Nick Lafferty — six-year review: https://nicklafferty.com/reviews/superhuman/
- S11 Fast.io — Superhuman AI review 2026: https://fast.io/resources/superhuman-ai-review-2026/
- S12 Search summary of Capterra/Trustpilot/Icebox reviews (secondary): https://icebox.cool/blog/superhuman-review-2026-is-30month-worth-it
- S13 Zapier — Shortwave vs Superhuman: https://zapier.com/blog/shortwave-vs-superhuman/
- S14 Icebox — Superhuman review 2026: https://icebox.cool/blog/superhuman-review-2026-is-30month-worth-it
- S15 Grammarly/Superhuman rebrand announcement: https://www.grammarly.com/blog/company/announcing-company-rebrand-to-superhuman/
- S16 9to5Mac — Grammarly becoming Superhuman: https://9to5mac.com/2025/10/29/grammarly-is-becoming-superhuman-as-it-gains-new-powers-at-no-extra-cost-for-now/
- S17 Wikipedia — Superhuman (email client): https://en.wikipedia.org/wiki/Superhuman_(email_client)
- Also: Arman — semi-pro Superhuman guide: https://writing.arman.do/p/superhuman ; SalesRobot review: https://www.salesrobot.co/blogs/superhuman-email-review

HEY
- S18 HEY — How HEY works: https://www.hey.com/how-it-works/
- S19 HEY — The Imbox: https://www.hey.com/features/the-imbox/
- S20 HEY — Features index: https://www.hey.com/features/
- S21 HEY — The Screener: https://www.hey.com/features/the-screener/
- S22 HEY Help — Notifications: https://help.hey.com/article/772-notifications
- S23 HEY — Set Aside / Reply Later / Workflows: https://www.hey.com/features/set-aside/ , https://www.hey.com/features/reply-later/ , https://www.hey.com/features/workflows/
- S24 Agentys — HEY review 2026: https://www.agentys.io/en/blog/hey-email-review
- S25 Khoi's pond — Why I switched to HEY: https://khoipond.substack.com/p/why-i-switched-from-gmail-to-hey
- S26 Oliver Palmer — Why I won't switch to HEY: https://www.oliverpalmer.com/blog/hey-email-review/
- S27 TapSmart — HEY review: https://www.tapsmart.com/apps/hey-email-review/

Gmail
- S28 Official Google Blog — Email overload? Try Priority Inbox (2010): https://googleblog.blogspot.com/2010/08/email-overload-try-priority-inbox.html
- S29 Official Gmail Blog — Priority Inbox update (Dec 2010): https://gmail.googleblog.com/2010/12/priority-inbox-update-time-savings-new.html
- S30 Gmail Help — Importance markers: https://support.google.com/mail/answer/186543?hl=en
- S31 Official Gmail Blog — A new inbox (tabs, 2013): https://gmail.googleblog.com/2013/05/a-new-inbox-that-puts-you-back-in.html
- S32 Gmail Help — Organize your emails into categories: https://support.google.com/mail/answer/3094499?hl=en
- S33 Wikipedia — Inbox by Gmail (reception, shutdown): https://en.wikipedia.org/wiki/Inbox_by_Gmail
- S34 TechCrunch — Say goodbye to Inbox by Gmail (2018): https://techcrunch.com/2018/09/12/say-goodbye-to-inbox-by-gmail/
- S35 TechCrunch — Gemini auto-summaries unless you opt out (May 2025): https://techcrunch.com/2025/05/30/gemini-will-now-automatically-summarize-your-long-emails-unless-you-opt-out
- S36 CNBC — Gemini features in Gmail, opt-out (Jan 2026; page blocked, cited from search summary): https://www.cnbc.com/2026/01/08/google-adds-gemini-features-to-gmail-message-summaries-proofreading-.html
- S37 Google blog — Gmail is entering the Gemini era (8 Jan 2026): https://blog.google/products-and-platforms/products/gmail/gmail-is-entering-the-gemini-era/
- S38 9to5Google — Gmail AI Inbox: https://9to5google.com/2026/01/08/gmail-ai-inbox/
- S39 Carly — Can Gemini manage my inbox? (secondary, Aug 2026): https://www.usecarly.com/blog/can-gemini-manage-my-inbox/
- S40 Gmail Help — Keyboard shortcuts: https://support.google.com/mail/answer/6594?hl=en
- S41 Google blog — How to unsend an email in Gmail: https://blog.google/products/gmail/how-to-unsend-email-gmail/
- Also: Yahoo/Engadget — Gemini summaries rollout: https://tech.yahoo.com/ai/articles/google-giving-gemini-first-look-182736664.html

Outlook
- S42 Microsoft Support — What is Focused Inbox?: https://support.microsoft.com/en-us/outlook/what-is-focused-inbox
- S43 Microsoft Support — Focused Inbox for Outlook: https://support.microsoft.com/en-us/outlook/mail/focused-inbox-for-outlook
- S44 Office-Watch — Outlook Focused Inbox explained (2026): https://office-watch.com/2026/outlook-focused-inbox-explained/
- S45 Johns Hopkins Engineering — Preventing loss of emails with Focused Inbox (page blocked; search snippet): https://support.cmts.jhu.edu/hc/en-us/articles/12674993764237-Preventing-Loss-of-Emails-with-Focused-Inbox
- S46 Microsoft Q&A — Emails going into Other tab: https://learn.microsoft.com/en-us/answers/questions/5756853/emails-going-into-other-tab
- S47 Practical 365 — Prioritize My Inbox: https://practical365.com/prioritize-my-inbox/
- S48 Microsoft Inside Track — Wrangling email with Prioritize My Inbox: https://www.microsoft.com/insidetrack/blog/wrangling-our-email-with-the-prioritize-my-inbox-feature-in-microsoft-outlook/
- S49 Microsoft Support — FAQ about Copilot in Outlook: https://support.microsoft.com/en-us/office/frequently-asked-questions-about-copilot-in-outlook-07420c70-099e-4552-8522-7d426712917b
- S50 Message Center MC1089318 — Copilot priority view on mobile (removed Feb 2026): https://mc.merill.net/message/MC1089318
- S51 Microsoft Support — Organize your inbox with Archive, Sweep and other tools: https://support.microsoft.com/en-us/outlook/organize-your-inbox-with-archive-sweep-and-other-tools-in-outlook-on-the-web

Apple Mail
- S52 Apple Support — Automatically categorize incoming emails in Mail on iPhone: https://support.apple.com/guide/iphone/use-categories-iphfe4a36baf/ios
- S53 Apple Support — Automatically sort incoming emails in Mail on Mac (categories section): https://support.apple.com/guide/mail/automatically-sort-incoming-emails-mlhlp1190/16.0/mac/27
- S54 MacRumors — How to turn off Apple Mail categories: https://www.macrumors.com/how-to/turn-off-apple-mail-categories/
- S55 Apple Newsroom — Apple Intelligence available today (Priority Messages): https://www.apple.com/newsroom/2024/10/apple-intelligence-is-available-today-on-iphone-ipad-and-mac/

Spark
- S56 Spark Help — Customize your Inbox: https://sparkmailapp.com/help/manage-your-inbox/customize-your-inbox
- S57 Readdle Support — Accept or block new senders (Gatekeeper): https://support.readdle.com/spark/spark-onboarding/accept-or-block-new-senders
- S58 9to5Mac — Spark major update (Oct 2022): https://9to5mac.com/2022/10/04/spark-mail-app-major-update/
- S59 AppleInsider — Spark Mail 2.11 review: https://appleinsider.com/articles/23/01/13/spark-mail-211-review-e-mail-organizer-with-gatekeeper-smart-inbox

Shortwave
- S60 Shortwave blog — Splits: https://www.shortwave.com/blog/split-email-inbox-by-importance/
- S61 Clean Email — Shortwave review: https://clean.email/blog/email-clients/shortwave-review
- S62 Shortwave blog — Magic Labels: https://www.shortwave.com/blog/magic-labels-custom-bundles/
- S63 Shortwave docs — Manage auto-apply labels: https://www.shortwave.com/docs/how-tos/manage-auto-apply-labels/
- S64 Shortwave docs — Smart Labels: https://www.shortwave.com/docs/guides/labels/
- S65 Shortwave docs — The AI Assistant: https://www.shortwave.com/docs/guides/ai-assistant/

Notion Mail
- S66 Notion — Introducing Notion Mail: https://www.notion.com/blog/introducing-notion-mail
- S67 Zapier — What is Notion Mail?: https://zapier.com/blog/notion-mail/
- S68 Notion Help — Organize your inbox with auto labeling (page returned navigation; content from search snippet): https://www.notion.com/help/guides/organize-your-inbox-with-notion-ai-auto-labeling
- S69 eesel — Notion Mail reviews (search snippet): https://www.eesel.ai/blog/notion-mail-reviews
- S70 Icebox — Notion Mail review 2026: https://icebox.cool/blog/notion-mail-review-2026-good-idea-real-limitations
- S71 TechCrunch — Notion Mail shuts down amid agent takeover (25 Jun 2026): https://techcrunch.com/2026/06/25/notion-mail-shuts-down-amid-agent-takeover/
- S72 The Verge — Notion is winding down Notion Mail (blocked; from search snippet): https://www.theverge.com/tech/957198/notion-is-winding-down-notion-mail

Mimestream
- S73 Mimestream site: https://mimestream.com/
- S74 Six Colors — Mimestream (2021): https://sixcolors.com/post/2021/10/mimestream-a-native-mac-app-with-proper-gmail-support/
- S75 Cult of Mac — Mimestream review: https://www.cultofmac.com/reviews/mimestream-mac-mail-app-review

Missive / Front
- S76 CMDK — Missive review 2026: https://cmdk.email/post/missive-review/
- S77 Missive docs — Rules: https://missiveapp.com/docs/advanced-features/rules
- S78 Missive docs — Rules FAQ: https://missiveapp.com/docs/advanced-features/rules/faq
- S79 Missive docs — AI rules: https://missiveapp.com/docs/advanced-features/rules/ai-rules
- S80 Missive — Features: https://missiveapp.com/features
- S81 Macha — Front vs Missive: https://www.getmacha.com/blog/front-vs-missive
- S82 Front Help — Front AI features: https://help.front.com/en/articles/1064512
- S83 Macha — Tags and auto-tagging in Front: https://www.getmacha.com/blog/how-to-set-up-tags-in-front
- S84 Front Help — Rule library: https://help.front.com/en/articles/2114 ; AI Tagging [legacy]: https://help.front.com/en/articles/759488

Proton
- S85 Proton blog — Proton Mail Categories: https://proton.me/blog/proton-mail-categories
- S86 Proton blog — 2025 autumn/winter roadmaps: https://proton.me/blog/proton-2025-autumn-roadmaps
- S87 Proton blog — Keyboard shortcuts (2021): https://proton.me/blog/shortcuts
- S88 Proton support — Keyboard shortcuts: https://proton.me/support/keyboard-shortcuts
- S89 Proton UserVoice — Improve keyboard shortcuts: https://protonmail.uservoice.com/forums/284483-proton-mail-calendar/suggestions/38545198-improve-keyboard-shortcuts-gmail-like

AI-native newcomers
- S90 Fyxer — AI email assistant: https://www.fyxer.com/ai-email-assistant
- S91 Unite.AI — Fyxer review: https://www.unite.ai/fyxer-review/
- S92 Gmelius — Fyxer AI review: https://gmelius.com/blog/fyxer-ai-review
- S93 tl;dv — Fyxer AI review: https://tldv.io/blog/fyxer-ai-review/
- S94 Every — Introducing Cora: https://every.to/p/introducing-cora-manage-your-inbox-with-ai
- S95 Cora site: https://cora.computer/
- S96 GitHub — Mail-0/Zero: https://github.com/Mail-0/Zero
- S97 emailexpert — Examining Zero: https://emailexpert.com/examining-zero-an-ai-native-email-client-taking-a-new-approach-to-inbox-management/
- S98 Product Hunt — Zero: https://www.producthunt.com/products/zero-7
- S99 Inbox Zero site: https://www.getinboxzero.com/
- S100 Inbox Zero docs: https://docs.getinboxzero.com/

Method notes
- WebSearch quota was exhausted mid-task; later lookups used direct fetches and
  Brave result pages. Pages that returned 403/blocked: help.superhuman.com,
  appleinsider.com, cnbc.com, theverge.com, arstechnica.com, medium.com, JHU
  support; where used they are marked as snippets or secondary.
- Not verified: whether Superhuman's per-message Important/Other move trains a
  model beyond the sender/domain rule; Gmail's "inbox cleanup" approval flow;
  Inbox Zero's exact feedback UI; the precise Superhuman-suite launch date.
