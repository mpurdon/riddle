# Riddle — Architecture

## 1. Purpose

Riddle is an **agent-native email client** designed around two complementary goals:

1. Build an excellent email client that dramatically reduces the amount of email a user must manually process.
2. Provide a real-world environment for experimenting with **Jev**, LLM reasoning, semantic classification, agent architecture, and human/AI collaboration.

These goals must reinforce each other.

Riddle must remain a useful product even if Jev ultimately proves unsuitable for some classes of problems.

Likewise, experimentation must happen against real product outcomes rather than artificial demonstrations.

The architectural north star is:

> **Riddle remains the product. Jev is an important experiment inside Riddle. Riddle is not a Jev demo.**

Jev is TypeSafe AI's hosted decision model. Its contract, limits and documented weaknesses are recorded in 18 and treated as design constraints.

Companion documents:

- `README.md` — the product.
- `ARCHITECTURE-REVIEW.md` — the review this revision responds to.
- `APPLICATION_FEATURES.md` — the desktop application conventions this project follows.

Sections were renumbered to plain integers on 2026-09-22 after the review revision; `ARCHITECTURE-REVIEW.md` and `APPLICATION_FEATURES.md` reference the new numbers.

---

# 2. Product Model

Riddle treats email as an unstructured stream of information that should be continuously understood rather than manually organized.

The primary product goal is not Inbox Zero.

It is:

> **Attention Zero**

When Riddle says there is nothing requiring attention, the user should be able to trust it.

The archaeology metaphor provides the domain language.

## Dig

A connected email account.

Examples:

- Work Gmail
- Personal Gmail
- Consulting Gmail

Digs may operate independently or participate in a unified Riddle.

A Dig can be disabled without being removed (`set_dig_enabled`). A disabled Dig is not synced, sifted or projected, and it is left out of everything that spans Digs: lists, counts, Finds (a Find is hidden when every source is on a disabled Dig), notifications, rule previews and apply-to-history, Temporal Sift and the Director. Its store, decisions and sign-in are kept, and enabling it resumes from where it stopped. Work reads `list_enabled` / `App::list_digs`; only account management reads every Dig. Spend already recorded still counts in usage. A test account beside a real one was the case: removing it was the only way to stop it cluttering Finds and rule previews.

## Dig Group

A collection of related Digs.

Examples:

- Work
- Personal
- Consulting

Dig Groups also provide a policy scope.

A Dig belongs to at most one Dig Group.

## Sift

The continuous processing pipeline that understands incoming and sent mail.

Sent mail is in scope because waiting-for detection, commitments, and Follow Up Finds depend on what the user asked for and promised.

Sift considers:

- provider metadata
- deterministic facts
- sender and recipient information
- conversation history
- Thread State (who spoke last, open asks, my commitments)
- Semantic Signals
- Meshes
- Strata
- Context
- previous user behaviour
- previous corrections
- related messages

Its job is to determine:

- what the message is
- where it belongs
- whether it deserves attention
- what should happen next

Most mail should pass through Sift without user involvement.

## Mesh

A policy describing how particular kinds of mail should be handled.

Meshes combine:

- deterministic facts
- Semantic Signals, pinned to a version
- scope
- thresholds
- exceptions
- actions

A Mesh contains two kinds of rules:

- **routes** decide where mail goes
- **guards** veto outcomes; they run after every route, from every scope

Example:

```text
Mesh: GitHub Notifications v8

Scope:
Work Digs

ROUTES

IF review_requested@v3 > .80
    → Find: Review

ELSE IF ci_failure@v4 > .85
     AND personally_relevant@v3 > .70
    → Find: Do

ELSE IF routine@v1 > .90
     AND personally_relevant@v3 < .30
    → Spoil

ELSE
    → Escalate

GUARDS

NEVER Spoil IF sender IN never_hidden
```

A Mesh may output:

- a disposition: Find (with a type), Filed, Spoil, Contamination
- one or more Strata to add
- Notify
- Escalate

Escalate hands the message to the LLM step of Sift (39). If the LLM cannot resolve it either, the disposition is Unresolved.

Thresholds are written against calibrated scores (27). A Mesh references a Signal by name and version. Promoting a new Signal version never changes a Mesh silently; it triggers re-simulation (38) and a proposal.

Meshes are policy.

They should not become arbitrary collections of hidden prompts.

## Disposition

Every message has exactly one disposition:

```text
Find            deserves attention; produces or joins a Find
Filed           understood, nothing needed; organized into Strata
Spoil           legitimate, low value
Contamination   untrusted
Unresolved      Riddle could not decide
```

Strata are independent labels. A message in Spoil can still carry the Purchases Stratum.

Finds are entities derived from one or more messages, not a per-message flag.

Unresolved is a real state with its own tray and its own count in the summary. It is never hidden and never inflated into a Find.

## Strata

Overlapping semantic categories.

Examples:

- People
- Projects
- Purchases
- Travel
- Finance
- Engineering
- Notifications
- Newsletters
- Receipts
- Needs Response

Unlike folders, a message may belong to multiple Strata.

Strata are governed:

- Riddle proposes Strata; the user renames, merges and splits them
- a Stratum is scoped to a Dig Group or global
- Strata are versioned, because Meshes route to them
- near-duplicate names are flagged before a new Stratum is created

## Context

Information that helps Riddle interpret mail.

Examples:

- sender relationship
- previous communication
- project membership
- conversation state
- who is waiting for whom
- deadlines
- previous classifications
- user corrections
- related messages
- sender behaviour

Riddle prefers context over keyword matching.

## User Profile

What Riddle knows about the user.

```text
addresses and aliases
group addresses the user receives through
name, role, organisation
active projects
VIPs, family, Never Hidden senders
```

`personally_relevant` and `directly_addressed` are undefined without it. It is seeded on first run and edited in settings.

## People

A model of every sender and recipient.

```text
identity across Digs
relationship strength
first contact date
replied to by the user?
organisation
mailing list or automated sender?
reputation and authentication history
```

## Thread State

Per thread:

```text
last speaker
open asks (from them, from me)
my commitments
last reply by me, from any client
```

Thread State is recomputed whenever the thread changes, from any source, not only on new inbound mail.

## Find

A first-class representation of something deserving attention.

Find types may include:

- Respond
- Decide
- Review
- Do
- Pay
- Attend
- Follow Up
- Know

A Find does not have to correspond to one email.

Twenty messages describing the same production problem may become one Find.

A Find has a lifecycle:

```text
OPEN → SNOOZED → OPEN
OPEN → RESOLVED     user acted, or Thread State shows a reply from any client
OPEN → DISMISSED    user said not important; recorded as a correction
OPEN → EXPIRED      dated Finds (Attend, Pay) after their date, or any Find
                    past its route's `+ Expires:` (open or snoozed)
OPEN → SUPERSEDED   merged into another Find, or replaced by a newer Find
                    of its `+ Replaces` series
```

Messages join an existing Find by thread, or by topic when the Director merges them. The same thread arriving in two Digs produces one Find, not two.

Two route outputs let a Mesh say a Find stops mattering, for mail where only the latest one counts (a daily reminder, a status nag). `+ Replaces` puts the Find in a series (the Mesh and the sender). The Find from the newest message stays open and the others become Superseded by it. `+ Replaces: same subject` keys the series by the subject too, read past its numbers and reply prefixes (`riddle_core::series_subject`), for a sender that runs several notices at once: a registrar's one reminder per domain, keyed by the sender alone, replaced the other domains' (16 Oct 2026). Order is by message date, not by when Sift saw the message, so re-deciding history never closes today's Find. `+ Expires:` sets `expires_at`: `end of day` is midnight in the user's timezone on the day the message arrived, `next morning` is 9 am the day after, and a span (`18h`, `2d`) counts from the message's date. The timezone is the system's, recorded into the profile at every boot; daylight saving moves the instant, not the local hour. Temporal Sift closes the Find as Expired then. Mail decided after its expiry makes a Find that is born Expired. Neither output changes a message's disposition, so neither interacts with Never Hidden. `+ Replaces` also works on Spoil and Filed, for a store's sale mail where only the newest matters: the newest message of the series keeps its decision and the older ones are superseded (`message_series` in the Dig store). A superseded message keeps its disposition and stays searchable; it reads as read, sinks out of the Spoil list unless asked for, and is recorded in Activity as `supersede_messages`, whose undo keeps it. Both are recorded in Activity as `close_finds`. Undo reopens the Find and keeps it (`kept_open_at`): an undo is the user choosing that Find, so no rule closes it again, whether a newer one in the series, its expiry or a passed Attend or Pay date. A Mesh in Shadow proposes no Finds, so neither acts there. `+ Expires:` also takes minutes under an hour (`15m`), for what is useful only briefly, such as a sign-in code.

Importance is how much a message matters: Low, Normal, High or Critical, on every message. `Priority:` was already the order Meshes run in, so a route sets the level with `+ Importance: high`. When the route that decided says nothing (or a gate left its match Unresolved), Sift derives it: Spoil, Filed and Contamination are Low, Unresolved is Normal, a Find is the band its 0–100 priority falls in (under 35 Low, under 70 Normal, under 90 High, else Critical), and `+ Notify` is Critical, so it still sorts first. A Find's priority stays the order within its level; an explicit level moves the number into its band rather than replacing it. The level is a column on `classifications` so lists filter on it (`minImportance`, and the Priority inbox's `minImportance`, High and above by default). The user's own level for a message (`set_importance`, Riddle-only, reversible) lives in `importance_overrides`, apart from the classification, so a rule re-deciding the message never overwrites it; lists and the explanation read it first. "Always…" (`remember_for_sender`) carries the level the user set into the rule it writes.

A sign-in code is read out of the message, not guessed from the subject: `verification_code` (`riddle_sift::facts`) is 4 to 10 digits beside a phrase that introduces a code, a strong one ("verification code", "single-use code", "passcode") on either side and a weak one ("your FreshBooks code", "the code below") only before it, nearest first, with links, addresses, years, prices, phone numbers, US ZIPs, "confirmation code" and "promo code" ruled out; `has_verification_code` is the rule's test. On the mailbox that prompted it, the subject rule caught half the code mail and Security and account alerts took the rest as plain notices; the fact found 22 codes in 6,247 messages and nothing else. The Accounts pack (v2) runs before Security and account alerts: `Find: Know + Stratum: Sign-in + Notify + Expires: 15m + Importance: Critical`. The Find carries the code (`Find::code`, the newest when a thread brings another), the notification's title is the code, and the Finds list and the message page show it with a copy button.

A payment that did not go through is its own fact, not a bill: `payment_failed` (failed, declined, "was unsuccessful", "add a different payment method", from the subject or the start of a message whose subject is about a payment; a subject saying it succeeded wins) and `payment_recovered` (charged to a backup method after the main one failed). The Money pack's Payment problems rule (v2, priority -2, ahead of Security alerts, Receipts and Payments) makes a failure `Find: Pay + Notify + Importance: High` and a recovery a Know Find, only from a sender with DKIM or DMARC passing, since "your payment failed" is the commonest phishing opener; past-due and overdue bills notify at High too. On the mailbox that prompted it, ten failures (Google Workspace, Stripe's "was unsuccessful", Uber) were Pay Finds at a bill's priority, and the probe found exactly those plus one scam that a safety rule already holds. The same pass found `lookalike_domain` treating any embedded brand as imitation: amazonaws.com read as Amazon and four genuine AWS past-due notices went to Contamination through the deterministic gate. An embedded brand now counts only as its own word (`paypal-secure`, `amazon-billing`), brands' other domains are named (`OFFICIAL_DOMAINS`), and the flag is recomputed by the facts refresh.

Mail from a community the user belongs to is neither a social network's notice nor marketing. `community_platform` names where a post comes from (`nextdoor`, `facebook_group`, `google_group`, `groups_io`, `meetup`, or `creator` for Patreon posts from creators the user backs) and is empty for a platform's own mail (Nextdoor markets from `ms.` hosts, Patreon from `updates.patreon.com`); `from_community` is true for the groups, not the creators. The Reading and social pack (v3) runs Community ahead of Payments and Social: creators are Filed under Creators, communities and the user's own `communities` list under Community, never Spoiled. On the mailbox that prompted it, Nextdoor's neighbourhood posts were filed with LinkedIn and Pinterest, two were Pay Finds ("Jon owes me $2500"), and Patreon creator posts were spoiled as promotions.

Model comparison (Developer → Models, `riddle_app::comparison`) measures which LLM suits which job on the person's own mail. Two jobs use a model. Escalation is replayed per candidate through the real pipeline with `SiftOptions::force_escalation`, so every candidate is asked even where the judge was sure, the Mesh routes decide on each candidate's Signals as Sift would, and nothing is persisted; it is scored against the person's own decisions (corrections, and decisions graded right in Rule check) and against what Riddle decided. The Director is replayed with read-only tools on the person's past questions (`Director::replay`), and scored by which answer the person picks. A roster (`Settings.comparison.roster`) lists candidates; each is probed with a five-token call at boot and every six hours and recorded as Usable, NotEnabled or Failing, because Bedrock listed Claude 5.5 and GPT-6 Luna weeks before this account could call them, even with their agreements accepted: model access can need account-level enablement from AWS. With `challengers` on, every usable roster model answers each real escalation beside the production model (`BenchSource::Shadow`), recorded and never applied, so scores accumulate as the person corrects mail. Benchmarks and challengers share one daily cap (`daily_usd`, $3 by default) and record their calls as `UsageTask::Comparison`, off the accounts' ledgers, so they never spend the Sift's escalation budget. Outputs live in `comparison_outputs` (index store); access checks and Director picks in kv.

An account's labels are kept by name. Messages carry the provider's label ids verbatim (`Label_4452336792128208639` was on 2,200 messages of a real mailbox) and nothing named a user's own labels, so the app could not show what labels an account has. Every sync now replaces the account's label list (`provider_labels` in the Dig store, one provider call; a failure is logged and the sync goes on), and `list_labels` returns each label with its kind (Gmail's own, a category tab, the user's, Riddle's), name and message and unread counts. The Labels page lists them per account, nested by "/", and opens the Inbox on one; the message page names them.

The Inbox chooses its mail from a column of places (`riddle_core::places`, `mail_places`; option D of the mail-navigation study, 11 Oct 2026), which replaced a Label dropdown of every Gmail label beside a separate Stratum dropdown. A place is a mailbox (`mailbox:INBOX`), one of the user's labels by full name (`label:Work/Receipts`, so one place covers every account), a Topic (`topic:Receipts`: a Stratum together with the user's label of the same name, compared without case, one row and counted once per message), or all mail. `list_messages` takes `filter.place` and resolves it per account into that account's label ids and Stratum, OR-combined (`App::list_messages_at`). The column shows the mailboxes, the user's pins (`pin_place` / `unpin_place`, kept in the index store under `mail.pinned`), Topics by size, then the remaining labels folded as a tree with empty ones folded again; `g l` goes to any place by name and ⌘\ collapses the column to icons.

An account that is not syncing says so. The sync job treated `provider_auth` and transient provider errors as deferrals: it logged a warning and completed, so a real Gmail account failed every 45 seconds for two weeks (twenty thousand times) on an expired sign-in while the Task Centre said "Caught up" and Sync now answered "Syncing now". Every sync's outcome is now recorded in the account's sync state (`sync_failing_since`, `sync_failure`; cleared by the next sync that gets through, and by signing in again), and the account's status becomes `SyncFailing { failure, since, last_synced_at }`: at once for a refused sign-in or an administrator's block, which retrying cannot fix, and after fifteen minutes for a rate limit or an unreachable provider, which usually clear on their own. Becoming `SyncFailing` notifies once; a banner stays until it clears, with Sign in again or Retry now; the Task Centre reads Degraded; and `refresh_now` refuses an account whose sign-in has expired, saying why, rather than waking a sync that will fail. A refused Gmail sign-in with no OAuth client configured is `NotSetUp`, not `SignIn`: an offline CLI run that used to save its no-network settings wiped a real profile's client id, and every token refresh after it failed as an "expired sign-in" that signing in again could not fix; saving a client clears it. A Sift pass holds a per-account lock (`SiftLocks`): a sync, a "decide again" and a catch-up each run one, and side by side they read the same unclassified ids, so after a two-week gap 455 of 924 messages were judged twice. Signing in (`connect_gmail`) is not gated by the dev build's mailbox writes: it changes nothing in the mailbox, and a dev build on Hold could not renew its own sign-in from the banner. Google expires an OAuth client's refresh tokens after seven days while its consent screen is in Testing; publishing it to Production stops the weekly expiry.

Follow Up Finds are created by Temporal Sift (19), not by an arriving message.

Topic is not who sent it (13 Oct 2026). A CPA's advice on a corporation's medical benefits was filed under People, or left Unresolved, because Signals said what mail wants from the reader and none said what it is about, and the only rule for a person's mail added People. Three changes:
- A route whose output is only Strata (`→ Stratum: Finance`) is a tag (`MeshOutput::is_tag`): it adds its Strata and evaluation goes on, so the rule that decides keeps deciding and the message is also under the topic. Tags that match before the deciding route are added to its Strata, and Unresolved mail keeps them. A tag Mesh runs early (the money pack's Money talk, personal band).
- `finance_topic@v1` reads a money matter (taxes, accounting, payroll, benefits, banking, investments, insurance, a business's finances), not money owed; it is in the human correspondence Signal set, so only a person's mail is scored for it. Money talk tags Finance on that, and on anything from `list:financial_advisors`, an adviser's newsletter included.
- `known_thread` (the thread has earlier mail from the user or someone they know) and `known_domain` (the user knows someone at the sender's domain, where the domain is one organisation's, `organisation_domain`: not a consumer or ISP mailbox, not a shared help desk) widen Human correspondence: the CPA had replied in a thread the household was in, at a firm where the user knew a colleague, and ended Unresolved as a stranger.

Unresolved mail that had a home (14 Oct 2026). Of 91 Unresolved messages in a real mailbox, most were kinds of mail the packs could already name but missed by a clause:
- Core v2. The Security and account alerts guard now reads what its route reads (`security_alert > .80 AND marketing < .70 AND newsletter < .80`): security gear launches, security newsletters and a bank's cashback promo scored .81-.96 as alerts, and 13 were kept from Spoil. Account notices files terms, privacy and agreement notices from automated senders by subject first, so they need no judge; an account notice that wants something done (`requires_action > .75`) is a Do Find; routine needs only be `> .50` (policy notices scored .55-.75). Cold outreach no longer makes a recruiter Contamination: its `claims_prior_contact` route skips `recruiting > .80`, since "I came across your profile" is how recruiters open; glyph substitution still counts.
- Career pack (new). `recruiting@v1` (a job, a role or a hiring process for the reader; human correspondence set) tags Career in Career talk (personal band), and Job alerts and recruiters files `list:job_boards` (LinkedIn's job-alert addresses, Indeed, Glassdoor…), `list:recruiters` and a stranger's recruiting mail under Career, ahead of Social and the promotion rules. A recruiter the user knows is still decided by Human correspondence, with Career added.
- `event_ticket@v1` (tickets, mobile entry or a parking pass the reader holds) makes Event tickets in Reading and social v4 a Know Find under Entertainment, at the end of the specific band because ticket sellers' mail lands in the Promotions tab. `shipment_update@v1` (where a package is or when it arrives) makes Deliveries in Shopping v3 Filed under Purchases. Both are in the commerce Signal set. Travel v2 decides from `travel_reservation > .75`: a hotel stay at .88 had been passed over on margin at `> .85`.
- Documents v2 tags Finance on signature-service mail about money (`finance_topic > .80`): an accountant's T183 e-file request was under Documents only, and Money talk reads only a person's own mail. The tag lives in the documents pack because a pack's Meshes may read only the lists it declares; `finance_topic` on automated mail is scored in the baseline set.

Four engine changes came from the same review of 91 Unresolved (14 Oct 2026):
- A Mesh changed while it runs keeps deciding with its last Active version until the new version is made Active (`repo::meshes::list_live`); the new version, in Shadow, is evaluated beside it and, where it would decide differently, records a Shadow proposal next to the live decision. Changing Payments had put the whole Mesh in Shadow and an "access turned off" notice went a day with no decision at all. A message with only a Shadow proposal is undecided again, except in an account's Shadow Period, where proposals are all it records.
- `noreply_sender` (`noreply@`, `account-noreply@`, `donotreply@`, a bare `notifications@`) counts as `automated_sender`: 33 of the 91 came from such addresses without bulk headers, so the Updates rule never caught them.
- Unresolved mail Riddle decided under rules that have changed since (the runtime records when an Action marked `changes_rules` last succeeded: Meshes, packs, lists, Never Hidden, Signals, an account's or the app's settings, an undo), or that ran out of escalation budget before today, goes back to Sift, 25 a pass, recorded in Activity under Sift; the user's own decisions never do.
- A reply the user sent takes its thread's Strata.

## Spoil

Legitimate but low-value mail.

Examples:

- marketing
- routine notifications
- repetitive automated reports
- low-value newsletters
- social notifications

Spoil is not deleted.

It remains searchable, recoverable, and available for summaries.

## Contamination

Untrusted mail.

Examples:

- spam
- phishing
- scams
- impersonation
- malicious content

Spoil and Contamination are intentionally distinct:

> Unimportant does not mean untrustworthy.

Contamination is decided by deterministic evidence first: Gmail's own spam verdict, authentication results, sender history, lookalike domains, a first-time sender asking for money or credentials. It is not a semantic judgment, because the judge does not treat adversarial content as hostile (18).

Contamination never enters Director or Expedition context unless the user opens it deliberately. Mail that argues for its own importance is treated as evidence against it.

## Director

The Director is Riddle's central agent.

It operates at a higher level than Sift.

The Director can:

- investigate
- search
- reason
- plan
- invoke Actions
- create Expeditions
- inspect classifications
- propose Meshes
- propose Semantic Signals
- test Semantic Signals
- explain decisions
- perform authorized work

Examples:

> What actually needs my attention this morning?

> Did anything important arrive while I was away?

> Why did you think this was a Find?

> Find the hotel reservation from my Portugal trip.

> What am I waiting for other people to respond to?

> Which subscriptions am I paying for that I don't seem to use?

The Director asks before any external side effect and shows the Action layer's real arguments, not its own description of them.

Once the Director has read untrusted content in a task, egress Actions (send, reply, forward, create or update a Mesh, add to Never Hidden) require approval, and the UI states why they are disabled (58).

What the Director did can be read back in full (12 Oct 2026). A Payments fix it proposed had dropped a route and two guards, and finding out why took its redacted tool calls, a pending list that forgot declined proposals, and an Activity ledger with no Director in it. Now:
- Each tool call in the conversation record keeps `input`, the arguments exactly as sent (a Mesh's text, a search's words), and `output`, a compact result: ids and counts for a read that carries mail, the result cut at 4,000 characters otherwise. The redacted `args` are still what events, logs and Activity carry.
- Every proposal is kept with what became of it (`proposals`, `ProposalRecord`: applied with its Activity, failed, declined with the reason, replaced, expired). A Director proposal replaces its conversation's pending proposal about the same thing (`Action::proposal_subject`: the Mesh, the Signal).
- Activity names the conversation behind a row (`conversation_id`): the Director's proposals and Mesh previews are rows under its name, and a change the user applies from a proposal says "Proposed by the Director".
- `director_audit` returns one conversation's turns, proposals and Activity together, through dispatch (the dev bridge) and `riddle-mcp`, which also reads a copied `riddle://director/<id>` link.
- A message's page and a Find's page carry the message's link beside the account name (16 Oct 2026): `riddle://message/<digId>/<messageId> (Sender · Subject)`, the ids every read takes. `get_message` and `explain` on the bridge and in `riddle-mcp` accept a pasted one whole as `messageId` (`riddle_core::parse_message_link`). Before it, naming a message to another session meant searching the store by subject. A Copy subject button sits beside the link: the link names the message to a tool, the subject to a person.
- Money pack v6: a transfer that arrived (`sender_domain IN list:money_transfers`) is Find: Know under Finance at Importance: High, since it is often the money the user was waiting for; it had sat at P65 level with any account notice. v7 moves Bank and transfers ahead of Security and account alerts (priority −3): the judge read 39 of 41 Interac deposits as security alerts at .81–.85, and only the Find margin kept them under Finance. A sender list is a fact and decides before a Signal.
- A route passed over as too close to call is one sentence after the decision, "Security and account alerts v4 looked first and nearly took it: …, but the margin 0.04 is below the 0.05 required"; the earlier "Passed over first, too close to call: …" read as if the message had been filed there.
- Travel pack v4: Bookings takes an event ticket from .75, not .85. The Telpark parking bookings it was written for scored .82 and .84, so the rule only filed them when escalation happened to re-score the reservation Signal, and each apply to history saw the stored decision as drift and sent them back. A decision that stands only on an escalation re-score is drift by construction.
- Core pack v3: the Account notices Do route carries Replaces, so a notice that repeats from one address (a lock's low battery every few days, a registrar's renewal reminder) is one Find, the newest, and the earlier ones close as Superseded. A series is keyed by Mesh and sender address, so a different notice from the same address is replaced too.
- Series suggestions (16 Oct 2026): `riddle_agent::series::suggest_series` reads the open Finds (sender, subject, the Mesh and route that raised each) and names a sender whose open Finds repeat one subject, numbers aside, three or more to a run, under a route that keeps no series. The app assembles the rows (`App::series_suggestions`); `mesh_corrections` lists them to the Director as `seriesSuggestions` beside the correction-based ones, `list_suggestions` to the Director page, whose empty state offers each as a question that has the Director propose `+ Replaces: same subject` and simulate it. The pattern had been noticed by hand twice in one day; now the application sees it for any sender.
- Engineering vendors (GitHub pack v3, 17 Oct 2026): a registrar's renewal is engineering and a bill at once. A Find has one type but any number of Strata, so a tag Mesh in the personal band adds Engineering to mail from the vendors on `engineering_vendors`, and the money rules still decide the type; Moniker's Pay Finds read Finance alone until then.
- Sender suggestions (17 Oct 2026): a sender with three or more messages in four months that is on no list is offered the list it looks like it belongs on. Evidence comes from the store (a sender's newest six messages scored as invoices, promotions the user restored from Spoil) and from a judgment per domain, `riddle_agent::senders::sender_kind_question`, a choice question on the sender's newest message (cloud or hosting, registrar, developer tooling, payments service, accountant or adviser, retailer, newsletter, government, a person or other), on two messages for a sender that writes often; a verdict is kept in the index store under `sender_kind:v4:<domain>` with the judge's identity, so a disagreement is kept (and offers nothing) rather than judged again, and a model change judges afresh. Which kinds belong on a list is the list's own declaration, `kinds` on a pack's `[[lists]]` entry, copied onto the `NamedListDef`. `App::review_senders` runs at the start of each Sift pass from one grouped query (`messages::sender_counts_since`) and a few index reads (the verdicts, the lists, the installed packs' `[detect]` domains through `packs::recognised_domains`, which parses no Mesh), none per sender, and only on a pass whose backlog has mail or after a rule change, with a floor of every tenth pass or ten minutes (the stale-Unresolved retry follows the same rule), so an idle pass costs one probe of the backlog index and one key-value read; never for an account kept off the cloud, never for a consumer mailbox (`organisation_domain`), never for a domain an installed pack already recognises in its `[detect]` block (GitHub's mail is the GitHub pack's business), the one skip the rows are built with, so the detector never sees such a sender. The invoice count reads each sender's newest six messages (`messages::sender_recent_ids`) and the invoice Signal's evaluations for those ids alone (`signals::evaluations_of_signal_for_ids`). Sibling subdomains whose kinds agree are offered as one registrable domain (`riddle_core::registrable_domain`); when they disagree each is offered on its own. `riddle_agent::senders::suggest_lists` maps the rows to the lists that exist; `mesh_corrections` lists them to the Director as `senderSuggestions` when the read is not about one Mesh, `list_suggestions` returns `Suggestions { series, senders }` to the Director page with each suggestion's `label` and `prompt` worded by the backend, and the Director acts with `save_list` and an apply to history. Lists had been filled by hand twice in a day.
- The Director reads Meshes as an index or one at a time (`list_meshes` with `meshId`): the whole list overran the tool loop's 30,000-character limit, and the Mesh it was editing was in the part cut off.
- A Mesh preview counts what the edit changes against today's rules re-run (`editChanged`, `editTransitions`) apart from `drift`, mail today's rules already decide differently from how it was stored; `changed` is still what applying to history re-decides.

## Expedition

Durable delegated work spanning multiple searches, judgments, and Actions.

Examples:

> Find every subscription I've paid for during the last year and identify ones I may no longer need.

> Go through everything related to the renovation and tell me what we're waiting for.

> Find all travel reservations for Portugal and organize them.

Expeditions persist independently from a chat conversation.

An Expedition declares its scope (Digs, date range, tools) and its budget (cost, time, tool calls) before it starts, reports progress in the Task Centre, can be cancelled, and resumes from its last checkpoint after restart. Its results become Finds.

---

# 3. Core Architectural Principle: One Riddle

Riddle does not have a normal application and a separate AI application.

The UI and Director are two interfaces to the same system.

> **The UI and the Director are two ways of interacting with the same Riddle.**

Conceptually:

```text
                    RIDDLE

                Action Layer
                     │
       ┌─────────────┼─────────────┐
       │             │             │
      UI          Director      Automation
       │             │             │
       └─────────────┼─────────────┘
                     │
              Domain Services
                     │
                 Riddle Store
```

If a human can reasonably perform an operation through Riddle, the Director should generally be capable of performing the same operation through an Action when authorized.

---

# 4. Technology Architecture

Riddle begins as a modular desktop application using Tauri.

```text
riddle/
├── apps/
│   └── desktop/
│
├── crates/
│   ├── riddle-core/
│   ├── riddle-actions/
│   ├── riddle-errors/
│   ├── riddle-mail/
│   ├── riddle-gmail/
│   ├── riddle-store/
│   ├── riddle-sync/
│   ├── riddle-projection/
│   ├── riddle-search/
│   ├── riddle-embeddings/
│   ├── riddle-context/
│   ├── riddle-people/
│   ├── riddle-signals/
│   ├── riddle-judge/
│   ├── riddle-mesh/
│   ├── riddle-sift/
│   ├── riddle-scheduler/
│   ├── riddle-finds/
│   ├── riddle-activity/
│   ├── riddle-agent/
│   ├── riddle-expeditions/
│   ├── riddle-ai/
│   ├── riddle-jobs/
│   ├── riddle-settings/
│   ├── riddle-credentials/
│   ├── riddle-privacy/
│   ├── riddle-observability/
│   ├── riddle-cli/
│   └── riddle-mcp/
│
└── src-tauri/
    └── commands/
```

These are **logical boundaries**, not network services.

Riddle begins as a modular application rather than a collection of microservices.

Stack defaults, from `APPLICATION_FEATURES.md` §15:

- Rust: tokio, rusqlite (bundled, FTS5, `bundled-sqlcipher` for encryption at rest), keyring with the `apple-native` and `windows-native` features, reqwest with rustls, serde, ts-rs.
- Tauri plugins: log, window-state (minus VISIBLE), dialog, opener, updater, autostart, notification with a Swift helper on macOS 26.
- Frontend: React 19, Zustand, Radix with Tailwind v4 and lucide, vitest, oxlint.
- Strict CSP; minimal capabilities per window; credential commands guarded by `require_main`.

---

# 5. Execution Model

Riddle is a desktop process. Nothing runs while the machine sleeps or the app is not running.

```text
Machine awake, app running      Sift, sync and jobs run
Window closed                   process stays resident (tray, close-to-hide)
Machine asleep                  nothing runs
App not running                 nothing runs
```

Consequences:

- The morning summary is a catch-up that starts on wake. Its latency is proportional to the backlog and capped by Gmail's fetch quota, about 300 messages per minute per account.
- Catch-up order: newest first, headers before bodies, Signals for likely Finds before Signals for likely Spoil.
- The summary states whether processing is complete.
- Sleep is detected from a wall-clock gap on a 30 s tick. A wake, a window focus, or a visibility change triggers a refresh pass.
- macOS App Nap is held off with an activity assertion while jobs run.

Process presence:

- the window can be moved, resized, minimised and zoomed before anything in it is judged. The title bar is the app's own (`titleBarStyle: Overlay`), so the top bar carries `data-tauri-drag-region="deep"`; `-webkit-app-region` is a Chromium property that WKWebView ignores, and the first build shipped an immovable window because of it. The app icon is generated by `scripts/make-icon.py` and checked at 16 px, because an unfindable Dock icon is the same class of defect.
- tray or menu-bar item with Open, Refresh now, Reset window position, Quit
- close hides the window while work is armed; Dock reopen restores it; the Dock badge shows the Find count
- launch at login through `SMAppService` with `.requiresApproval` handled and the setting read back from the system after every write
- off-screen window rescue after monitor changes

Provider events:

- Gmail push requires a Cloud Pub/Sub endpoint and a server. Riddle polls `history.list` every 30 to 60 seconds per Dig, at 2 quota units per call, and on focus.
- The history cursor can expire in as little as hours. Sync keeps a full-resync path (53).

Multi-device:

- v1 is single-device. Riddle's understanding lives in one machine's store.
- Policy travels: Meshes and Signals export as files (12). Not built beyond the Mesh Actions yet; the steps (export in Settings, automatic backup, sync through a private Git repository) are in `docs/ROADMAP.md`.
- The purely deterministic subset of Meshes may be compiled into Gmail server-side filters, so that subset applies on every device while Riddle is closed. Existing Gmail filters are imported as seed Meshes on first run so the two rule systems do not fight.

Notifications:

- On macOS 26 the notification plugin's API reports success and shows nothing. Riddle ships a small Swift `UNUserNotificationCenter` helper, versioned by a hash of its inputs, with a Test Notification button in settings.
- macOS gives no click callback; the next app activation within a short window claims the notification.
- Only Finds above a priority threshold interrupt. Everything else waits in the Task Centre and the summary.

Per-Dig status separates intent from delivery:

```text
Synced          cursor current
Catching up     backlog being fetched
Sifting         N remaining
Projection      N decisions not yet written to Gmail
Degraded        provider, judge or LLM unavailable; nothing hidden
```

---

# 6. Distribution and Updates

Release pipeline, on a `v*` tag:

1. Verify the tag equals the version in `tauri.conf.json`.
2. Run tests.
3. Build, sign and notarize.
4. Hard gate: `codesign --verify`, `xcrun stapler validate`, `spctl`. Unnotarized output is refused.
5. Create the GitHub release idempotently; upload a stable-name asset with `--clobber`.
6. Bump the Homebrew cask.

Updates:

- `tauri-plugin-updater` with a silent check shortly after launch and every few hours, and a banner state machine from checking to Relaunch.
- The updater signing key lives in the password manager and as a CI secret. Losing it strands every install.

Version discipline:

- one source of truth for the version, checked by CI
- a pre-commit hook denies tags that do not match and code changes without a bump
- a build stamp (git sha, dirty flag, build time) is injected at build and recorded in every decision trace, so replay comparisons know which build produced each result

Update Preview:

An update that ships new system Signal versions or changed default Meshes must not change behaviour silently. On first launch after such an update Riddle runs Mesh simulation (38) against history and shows:

```text
This update changes 2 system Signals and 1 default Mesh.

Against your last 90 days:

Unchanged     18,102
Spoil → Find      14
Find → Spoil       3
Filed → Spoil     22

[Preview] [Apply] [Keep previous behaviour]
```

Nothing is reclassified until the user chooses.

A bundled changelog is rendered in-app.

---

# 7. Tauri Is an Adapter

The Rust application is Riddle itself.

The React application is a client of Riddle.

Tauri commands must therefore remain thin adapters.

```text
React
   │
   ▼
Tauri Command
   │
   ▼
Riddle Action
```

The Director follows the same architecture:

```text
Director
   │
   ▼
Agent Tool Adapter
   │
   ▼
Riddle Action
```

Tauri commands are not the canonical application API.

Actions are.

Mechanics:

- One instrumented invoke wrapper in the webview logs command names and timings, keeps an in-flight registry, warns on slow calls, and never logs argument values. For Riddle that means message identifiers, never subjects, bodies or addresses.
- Commands return typed errors (8). A panic inside a command is caught at the boundary; the webview side times out rather than waiting forever.
- Types shared across the boundary are generated from Rust with ts-rs.
- Lists are paged. The webview never runs SQL. Progress flows over Channels; events carry signals, not volume.

---

# 8. Actions Are the Capability Boundary

Actions are the single source of truth for capabilities.

Examples:

```text
list_messages
search_messages
get_message
get_thread

create_draft
update_draft
send_message

archive_messages
snooze_messages
label_messages

create_mesh
update_mesh
simulate_mesh

evaluate_signal
evaluate_signal_set

propose_signal
test_signal
compare_signal_versions

list_finds
resolve_find

move_to_spoil
mark_contamination

start_expedition
cancel_expedition

unsubscribe
import_gmail_filters
export_meshes
import_meshes

undo_activity
```

An Action owns:

- input contract
- output contract
- validation
- authorization
- execution
- risk metadata
- idempotency
- audit behaviour
- undo behaviour

The Action Layer—not prompts—enforces permissions.

Every Action returns a typed error `{kind, message, detail}` so the UI and the Director can respond correctly:

```text
provider_auth             sign in
provider_admin_blocked    a Workspace administrator blocks Riddle; explain what to ask for
provider_rate_limited     back off; park the job
provider_history_expired  full resync
judge_unavailable         degrade to Unresolved; nothing hidden
llm_unavailable           degrade to Unresolved; nothing hidden
budget_exhausted          pause the job; show it in the Task Centre
policy_denied             not permitted at this autonomy level
needs_confirmation        return a pending action for the user
conflict                  provider state changed underneath; reconcile
```

Confirmation tiers are declared per Action (51).

---

# 9. Director Is a Client, Not the Backend

The Director must not directly mutate application state.

It invokes Actions.

This is prohibited:

```text
Director
   ↓
SQLite UPDATE
```

This is required:

```text
Director
   ↓
update_mesh()
   ↓
Action Layer
   ↓
Domain
   ↓
Store
```

The same applies to Jev.

Neither Jev nor an LLM receives arbitrary authority over Riddle state.

---

# 10. CLI and MCP

A `riddle` CLI shares the core and the Action layer:

```text
riddle sync --dig work
riddle sift replay --since 30d --mesh github@v8
riddle simulate mesh.toml
riddle export traces --redacted
riddle expedition run "find unused subscriptions" --budget 2.00
```

- `--progress` emits NDJSON with the plan before the first side effect.
- `--progress` implies `--yes`: a program driving the CLI confirms on its own side.
- stderr is for humans; stdout is for machines.

An MCP server exposes Actions to external agents. Read-only Actions first; mutating Actions carry the same risk metadata and confirmation tiers as everywhere else. An external agent is a client like the UI and the Director, never a back door.

---

# 11. Local-First State

Riddle is local-first.

SQLite is the persistence layer.

There is one store per Dig, plus a small index store for cross-Dig views. A Dig's privacy policy, backup, cloud exclusion and removal are therefore file operations, and data never crosses Dig boundaries by accident.

Likely domain state includes:

```text
digs
dig_groups

messages
threads
participants
contacts

strata
message_strata

signals
signal_versions
signal_evaluations

meshes
mesh_versions

classifications

finds
find_sources

context

expeditions

activities
action_effects

decision_traces
reasoning_runs

experiments
experiment_runs

jobs
job_checkpoints
outbox

sync_state

user_profile
people
thread_state

dispositions
corrections

calibrations

settings
```

Gmail remains authoritative for Gmail-owned state.

Riddle remains authoritative for Riddle intelligence.

Schema changes are versioned migrations. Before any structural migration the previous store is copied to a permanent `legacy-backup`. Every write goes through a persistence gate: it is logged with its call site, and three consecutive failures flip a fault flag that pauses Sift and shows in the Task Centre.

---

# 12. Settings and Configuration

Settings are split by ownership:

```text
config        declarative, shareable, hand-editable
              Digs, Dig Groups, per-Dig privacy and autonomy,
              the Never Hidden list, Mesh and Signal references

state         runtime
              sync cursors, job state, update state

high-churn    Activity, traces
              separate files or tables, so a settings save
              can never clobber live data
```

Rules:

- settings are owned by the Rust core and written atomically (temp file, then rename)
- every field has a serde default; a test proves that a file written before a feature still loads
- a schema version is stamped into each file
- display preferences take a light path that never invalidates credentials
- dev builds use a separate data directory, keychain service and log file (59)

Meshes and Signals export and import as files. This gives version control of policy and a way to carry policy between machines without a server.

First run seeds from what already exists: Gmail labels, filters and categories, the user's send-as aliases, and contacts. Guesses are ranked by how likely they are to be right now. A bad guess is not a neutral default.

---

# 13. Data Lifecycle

- Riddle state (Meshes, Signals, Finds, Activity, traces, settings) is backed up daily with a retention window. The mail cache is not backed up; it is re-syncable.
- A reset path exists for a corrupt store.
- Disk-space watermarks warn before the mail cache fills the disk. The cache evicts bodies of old, Filed and Spoil mail first.
- Traces are compressed, kept in an attached database, and aged out by policy. Raw judge and LLM requests are stored by reference to message and prompt-template version, not as copies of content.
- Expensive lookups use a TTL cache.
- A throttled, idempotent maintenance job prunes and compacts.
- Removing a Dig purges its store, its traces, its exports and its keychain items.

---

# 14. Provider State and Riddle State Are Separate

Provider state includes:

- messages
- threads
- provider labels
- drafts
- spam
- trash
- archive state
- provider change history

Riddle state includes:

- Digs
- Dig Groups
- Semantic Signals
- Meshes
- Strata
- Context
- Finds
- Spoil classifications
- Contamination classifications
- Filed and Unresolved dispositions
- Snooze state (Gmail has no snooze)
- Expeditions
- Activity
- reasoning/evaluation history

Provider concepts should not leak into Riddle's semantic model unnecessarily.

---

# 15. Provider Projection and Reconciliation

Riddle's decisions are projected into Gmail so that other clients see them.

```text
Riddle state          Gmail projection
---------------------------------------------------
Spoil                 archive + label Riddle/Spoil
Contamination         label Riddle/Contamination; Gmail spam untouched
Stratum               label Riddle/<Stratum>
Find                  none; Finds are Riddle entities
Filed                 archive, optional label
Unresolved            none; stays in the inbox
Snooze                Riddle-only
```

Projection is a per-Dig setting: full, labels only, or none. With none, Riddle's value exists only inside Riddle, and the summary says so.

Every projection write goes through an outbox:

- queued locally, applied when online, idempotent, retried with backoff
- ordered per thread
- shown in the per-Dig status as "Projection: N pending"

Reconciliation:

- Gmail is authoritative for Gmail-owned state. A provider-side change wins over a queued projection for the same message, and the conflict is recorded as Activity.
- Provider-side changes are attributed before the learning layer sees them: a human action in another client, a Gmail filter, another automation, or Riddle's own projection echoing back. Only human actions count as corrections.
- A change to Gmail's `SPAM` label after Riddle decided a message sends it back to Sift, which recomputes its facts: marked spam meets the Contamination gate, "Not spam" lifts it. Riddle never writes that label, so the change is Gmail's or the user's. Before, sync stored the label and nothing read it, and a scam Gmail moved to Spam days after it arrived stayed Unresolved. A decision the user made is kept.
- When the history cursor has expired, sync performs a full resync and reconciles labels against Riddle's expected projection, reporting differences rather than overwriting.

Labels and filters Riddle creates carry a managed-by marker so the user can tell Riddle's from their own.

---

# 16. Gmail First

Riddle should initially use the Gmail API directly.

The mail abstraction should preserve Gmail capabilities rather than reducing Gmail to an IMAP-like lowest common denominator.

Important Gmail concepts include:

- messages
- threads
- labels
- drafts
- spam
- trash
- archive semantics
- search
- change history

Provider abstraction may come later.

Gmail constraints that shape the design (verified 2026-09-21; sources in `ARCHITECTURE-REVIEW.md` §5.2):

- `history.list` cursors are valid for at least a week but sometimes only hours. Expiry returns 404 and requires a full sync.
- Push notifications need Cloud Pub/Sub and a server. A desktop app polls.
- Quota: 6,000 units per user per minute. `messages.get` costs 20, so about 300 messages per minute per account. `batchModify` costs 50 for up to 1,000 ids. Batch HTTP is 100 calls and does not reduce units. The project shares 80M units per day.
- `gmail.modify` is a restricted scope. App verification is required to exceed 100 users. In Testing status, refresh tokens expire after seven days. Transmitting mail content to a cloud model may require an annual security assessment.
- Google's Limited Use policy allows transfer to a third party only for a prominent, consented user-facing feature, with a vendor that does not train on the data, and now requires prompt-injection protection.
- A Google Workspace administrator can block Riddle. The OAuth error is `admin_policy_enforced` and is explained plainly.
- Not available through the API: snooze, schedule send, unsubscribe. Available as labels: IMPORTANT, SPAM, CATEGORY_PERSONAL, CATEGORY_SOCIAL, CATEGORY_PROMOTIONS, CATEGORY_UPDATES, CATEGORY_FORUMS. These are free deterministic facts.
- Desktop apps cannot keep a client secret. Loopback redirect with PKCE is the flow.

---

# 17. Sift Architecture

Sift is a pipeline.

It is not itself a general-purpose agent.

```text
              MESSAGE (inbound or sent)
                        │
                        ▼
                  Normalization
                        │
                        ▼
                Deterministic Facts
                        │
                        ▼
          Never Hidden / Contamination gates
                        │
                        ▼
        Signal Selection (from Meshes in scope)
                        │
                        ▼
            Semantic Judgment Engine (18)
                        │
                        ▼
                 Semantic Signals
                        │
                        ▼
                  Meshes: routes
                        │
                        ▼
                Confidence Policy
                        │
             ┌──────────┴──────────┐
             │                     │
        confident              uncertain/
         result                   novel
             │                     │
             │                     ▼
             │            LLM refines Signals
             │                     │
             │                     ▼
             │            Meshes: routes again
             │                     │
             │             still uncertain?
             │                     │
             │                     ▼
             │                 Unresolved
             │                     │
             └──────────┬──────────┘
                        ▼
                  Meshes: guards
                        │
                        ▼
                   Disposition
                        │
                        ▼
                     Actions
```

Stages:

**Normalization.** HTML to text; quoted-reply and signature stripping; size caps; attachment metadata only; language detection; calendar and structured-data parts extracted. Quoted-reply stripping decides whether `requires_response` works on long threads.

**Deterministic Facts.** A fixed vocabulary computed without any model:

```text
sender_domain, sender_in_contacts, sender_replied_to_before,
first_time_sender, same_organisation, mailing_list (List-Id),
list_unsubscribe, precedence_bulk, auto_submitted,
authentication_results (SPF, DKIM, DMARC), reply_to_mismatch,
gmail_category, gmail_important, gmail_spam,
directly_addressed, cc_only, thread_length, my_last_reply_at,
has_calendar_part, has_structured_receipt, language,
urgency_language_present, asks_for_payment_or_credentials,
dates_and_deadlines (parsed, compared here, never by the judge),
sender_cadence (daily | weekdays | every_few_days | weekly | monthly | irregular),
sender_on_schedule (about one a day, at the same local time),
government_sender (a government domain whose mail passed DMARC),
from_me (an own address, and sent, a draft or authenticated), spoofs_me,
free_mail_sender (gmail.com, outlook.com, icloud.com…),
glyph_substitution ("| look forward", Latin mixed with Cyrillic in one word),
cold_outreach (one-to-one mail from a stranger's free mailbox, not a reply),
github_event (review_requested_you | review_requested_team | approved | commented | merged | closed | pushed | run_failed | opened | changes_requested),
list_id (the List-Id identifier, e.g. platform.northwind.github.com)
```

One sender, several kinds of mail: rules group a sender under `WHEN` and route by facts first (a header, the subject, `is_reply`, `list_id`, the sender's name), then by the account Signals (`security_alert`, `payment_related`, `account_notification`) for what only reading tells, then a default for the rest. GitHub is the worked case. `X-GitHub-Reason` says why the user is on a thread and sticks to it: once a team is asked to review a PR, every approval, bot comment and merge arrives as `review_requested`, and 1,437 of them were open Review Finds. `github_event` reads the first line of the notification ("requested your review", "requested review from @team", "approved this pull request", "Run failed:" in the subject), so personal review requests become Review Finds, team requests and new PRs are Filed, follow-ups are Spoil, and failed Actions runs, which arrive as `subscribed`, become Do Finds that expire after two days.

`government_sender` is true for Canadian federal, provincial and territorial domains (`canada.ca`, `gc.ca`, `ontario.ca`, `gov.bc.ca`…, matched as whole labels so `petro-canada.ca` is not one), `.gov` and `.mil`, and a government second level under a country code (`gov.uk`, `gouv.fr`), and only when DMARC passed: the Canada Revenue Agency and the RCMP are the names scams use most, and a forged From line or a domain that merely ends in "canada.ca" must not become the most urgent Find. A route's `+ Notify` makes its Find interrupt at once in any notification mode and gives it a priority of at least 95; before, the flag was recorded and read by nothing.

`sender_cadence` is classified by People from when the sender's mail arrived over 120 days, by the user's local day. It says whether only the newest message of a sender is likely to matter, not whether any does: on the mailbox that prompted it, eighteen senders wrote daily and one was a to-do. `sender_on_schedule` separates mail on a timer from mail the user's own actions set off: purchase confirmations and LinkedIn were daily and personally relevant too, but came several times a day at any hour. The rule that picked out only the reminder was `IF sender_cadence = daily AND sender_on_schedule AND personally_relevant@v1 > .8 → Find: Do + Replaces + Expires: end of day`; with cadence and the Signal alone it also caught 137 purchase confirmations.

`cold_outreach` came from a scam opener Gmail flagged only days after it arrived: an investor's name on a numbered Gmail address, "I noticed your comment on my YouTube channel", SPF, DKIM and DMARC all passing. It is a free-mail sender nobody knows or protects, with none of the headers bulk mail carries, not a reply (a shop answering a website form has no thread in the mailbox but is not an opener), on a thread the user has not answered. It describes how mail arrived, not what it says, and real strangers write that way too, so a rule pairs it with a Signal, and never with `human_authored` alone, which scored that shop's reply 0.15 and a fake recruiter 0.18 (on that mailbox it never scored any of 6,198 messages above 0.6, and its partner's mail sat near 0.2, so the default Human correspondence Mesh, gated on `human_authored > .70`, decided nothing; it now decides people by `known_sender` and non-automated mail, and uses Signals only for the Find type): `claims_prior_contact` (the author says they already know the reader) contradicts a history Riddle can check, and `glyph_substitution` is a filter-evasion trick few honest writers use. At a free mailbox, authentication proves only that someone with an account there sent the mail, so it no longer counts as support for a Find from a first-time sender, and a free-mail domain is never seeded as the user's organisation. Drafts are the user's own and are Filed without a judge; mail from an own address counts as known; an own address on mail that failed DMARC outright is Contamination evidence ("I have access to your account"), though an organisation or Never Hidden match still leaves it Unresolved for the user.

**Gates.** The Never Hidden list and Contamination evidence are applied before any Signal is evaluated. A gate outcome cannot be overridden by a Signal or a route.

**Signal Selection.** The union of Signals required by the Meshes in scope for the Dig, plus the system baseline. A message never pays for a Signal no Mesh will read. Each Signal Set builds its own state within a token budget (25).

**Sent mail.** Sent messages are evaluated for `asks_question`, `makes_commitment` and `requests_action`, and the results update Thread State.

**Guards run last.** Every guard from every scope runs on the routed result. Any veto wins.

The guiding principle is:

> **Use AI for judgment, not plumbing.**

---

# 18. Semantic Judgment Engine

Sift depends on a trait, not on a vendor.

```text
trait SemanticJudge

  evaluate(state, questions[]) -> answers[]

  state       text or JSON, size-bounded
  question    id, primitive (choice | noul), definition, options
  answer      selected option, full distribution,
              model id and version, prompt/schema hash,
              token usage, latency, deterministic flag

  limits()    max state tokens, max questions, rate limits
  identity()  provider, model id, model version
```

Jev, by TypeSafe AI, is the initial implementation.

Facts recorded 2026-09-21 (sources in `ARCHITECTURE-REVIEW.md` §5.1):

```text
model             jev-1.13.0, pinned; never an alias
primitives        Choice, Score, Noul
state window      32k tokens for state plus the longest question; 64k per request
rate limits       1,200 requests per minute; 250k tokens per second
latency           70 to 500 ms claimed
price             $0.042 per million input tokens
language          English primary; others less reliable
training          not trained on customer requests
```

Documented weaknesses, treated as constraints:

- reads dates as text: date comparisons are computed as deterministic facts, never asked
- does not treat adversarial content as hostile: gates run before judgment, and Contamination never reaches it
- accuracy falls as irrelevant state grows: each Signal Set has a state builder with a token budget
- literal reading, and worse with contradictory instructions: Signal definitions follow authoring rules (23)
- Score levels are weakly calibrated: Signals use Choice and Noul only
- weaker outside English: a language fact lowers autonomy or routes to the LLM

Cost sanity: 2,000-token states for 100,000 historical messages cost about $8 at list price. The LLM escalation tail dominates initial cost, so the escalation budget is the number to design around.

Replacement path: an open model behind the same endpoint shape (next-token logits with a shared-prefix cache), an encoder classifier, or an LLM with verbalised probabilities. Anthropic's API exposes no log-probabilities. Each replacement carries its own calibration map (27).

Fallback when no judge is available: deterministic facts plus Gmail categories route the obvious cases; everything else is Unresolved; nothing is hidden. Invariant 37 depends on this path existing.

---

# 19. Temporal Sift

Some dispositions change because time passes or because a thread changes, not because a message arrives.

A scheduler owns:

- Follow Up Finds: no reply after N days to an open ask of mine
- Respond Finds: resolved when Thread State shows my reply from any client
- Attend and Pay Finds: expire or escalate at their date
- Finds past their `+ Expires:` time: expire
- snooze expiry
- re-evaluation of Context-dependent Signals whose Context slice changed (26)
- the Shadow Period clock per Mesh (51)

Temporal Sift runs as durable jobs (55). It is message-free: it reads Thread State and Finds, and it writes Activity like everything else.

---

# 20. Jev Is the Preferred Initial Semantic Decision Engine

Jev should initially be treated as Riddle's preferred high-volume semantic judgment engine.

The working hypothesis is:

> Jev can perform many bounded semantic judgments quickly and cheaply, while an LLM handles uncertainty, novelty, synthesis, planning, and open-ended reasoning.

This is an architectural hypothesis to be measured rather than assumed.

Jev must remain replaceable.

The interface it is replaceable behind, its documented limits, and the fallback when it is unavailable are specified in 18.

---

# 21. System 1 / System 2

Riddle conceptually uses:

```text
Jev
Fast / bounded / cheap judgment
              │
              ▼
        confidence gate
              │
     ┌────────┴────────┐
     │                 │
 confident         uncertain
     │                 │
     ▼                 ▼
  policy              LLM
                       │
                deeper reasoning
```

Jev acts as a semantic judgment primitive.

The LLM acts as the deeper reasoning system.

---

# 22. Semantic Signals

Jev should generally not be asked giant questions such as:

> Is this email important?

Instead it evaluates smaller semantic properties.

Example:

```text
requires_response

YES       .91
NO        .07
UNCLEAR   .02


review_requested

YES       .96
NO        .04


ci_failure

YES       .02
NO        .98


personally_relevant

YES       .91
NO        .09


routine

YES       .62
NO        .38
```

These results become **Semantic Signals**.

---

# 23. Signal Registry

Semantic Signals are managed domain objects.

Riddle maintains a Signal Registry containing reusable semantic primitives such as:

```text
requires_response
requires_action
direct_request

personally_relevant
human_authored

routine
novel

marketing
newsletter
transactional

payment_related
meeting_related

review_requested
ci_failure
deployment_failed
```

A signal has:

```text
id
name
version

primitive          choice | noul

definition

choices
choice definitions

inputs             message | thread | named Context slices
language scope

status

calibration        labelled set, method, model version, date

created_at
created_by

evaluation metadata
```

Signals are versioned.

Authoring rules, because the definition is the prompt:

- one property per Signal
- no negations, no "and/or", no double indirection
- no dates, counts or arithmetic; those are facts
- English definitions; other languages take the LLM path until measured

---

# 24. Signals Should Be Atomic-ish

Signals represent semantic judgments.

Meshes compose those judgments into behaviour.

For example:

```text
review_requested
ci_failure
personally_relevant
requires_response
```

are Signals.

This:

```text
review_requested
OR
(ci_failure AND personally_relevant)
OR
requires_response
```

is Mesh policy.

This separation prevents the Signal Registry from becoming thousands of highly specific natural-language rules.

---

# 25. Signal Sets

Signals concerning the same input should be evaluated together when practical.

Example GitHub Signal Set:

```text
requires_response
review_requested
ci_failure
personally_relevant
routine
```

Rather than making five independent evaluations, Riddle may evaluate the Signal Set against the same message/context in one Jev operation where supported.

The important optimization principle is:

> **Minimize repeated state transmission and evaluation rather than minimizing the number of semantic questions.**

Signal Sets may be domain-specific.

Examples:

```text
GitHub Signal Set

Commerce Signal Set

Human Correspondence Signal Set

Calendar Signal Set

Financial Signal Set
```

Deterministic routing determines which Signal Sets are relevant.

Signal Selection is derived: the union of Signals required by the Meshes in scope for the Dig, plus the system baseline.

Each Signal Set has a state builder with a token budget. It includes the normalized message, the Thread State summary, and only the Context slices the Set's Signals declare. The judge's accuracy falls as irrelevant state grows (18), so the budget is a quality control, not only a cost control.

---

# 26. Semantic Signal Cache

Signal evaluations should be persisted.

Example:

```text
message: msg_123

requires_response:v3
    YES .94

review_requested:v2
    NO .98

ci_failure:v4
    YES .99

routine:v1
    YES .87

personally_relevant:v3
    YES .91
```

This produces a semantic feature store for email.

Changing a Mesh usually should not require rerunning Jev.

The Mesh can simply reevaluate existing signals.

The cache key is:

```text
message id
Signal name and version
engine identity and model version
hash of the Context slice consumed, for Context-dependent Signals
```

A Signal that declares message-only inputs is cached indefinitely. A Context-dependent Signal is stale when its Context slice changes and is re-evaluated by Temporal Sift (19). A model change invalidates every cached evaluation for that engine until re-calibration passes (27).

---

# 27. Signal Versioning

Changing the definition of a Signal creates a new version.

Example:

```text
requires_response:v3
requires_response:v4
```

Historical Signal evaluations retain their version.

This allows:

- comparison
- replay
- regression testing
- shadow evaluation
- historical simulation

Every Signal version carries a calibration artifact: the labelled set, the method (Platt scaling or conformal prediction), the model version, and the date. Thresholds in Meshes are written against calibrated scores; the raw distribution is stored alongside. A raw `.80` is an operating point on a ranking score, not an 80% chance, until the calibration says otherwise.

Meshes reference `signal@version`. Promoting a new version never rewrites a Mesh silently. It triggers re-simulation of every dependent Mesh (38) and a proposal to the user.

A model change invalidates every calibration for that engine until re-calibration passes a held-out gate.

---

# 28. How Signals Are Created

Signals may originate from three sources.

## System Signals

Riddle ships with a curated initial semantic vocabulary.

Examples:

```text
requires_response
personally_relevant
human_authored
routine
marketing
transactional
```

## User Intent

The user may express behaviour that requires a semantic concept.

Example:

> GitHub only matters if someone asks me for a review or something I own fails.

The Director searches the Signal Registry.

If existing Signals express the intent, they are reused.

If an important primitive is missing, the Director may propose one.

## Behavioural Discovery

Repeated user corrections may reveal a missing semantic distinction.

The Director may identify the pattern and propose a Signal.

---

# 29. The Director Acts as Semantic Compiler

The Director translates human intent into Riddle's semantic vocabulary.

Example:

```text
"I don't care about GitHub unless someone
asks me for a review or something I own fails."
```

Director:

```text
Existing:
    review_requested
    personally_relevant

Missing:
    owned_work_failed

Action:
    propose owned_work_failed:v1
```

The Director should prefer existing Signals over inventing redundant ones.

The Signal Registry is effectively:

> **Riddle's semantic instruction set.**

Meshes are programs written against that instruction set.

The Director acts partly as a compiler from human intent to semantic policy.

---

# 30. Signal Lifecycle

Signals move through a lifecycle.

```text
PROPOSED
    │
    ▼
CANDIDATE
    │
    ▼
EVALUATING
    │
    ├────► REJECTED
    │
    ▼
EXPERIMENTAL
    │
    ▼
ACTIVE
    │
    ▼
DEPRECATED
```

The Director may freely inspect, propose, and test Signals within its permissions.

Production promotion is a separate Action.

Initially, promotion should require explicit user approval.

---

# 31. Signal Testing

Candidate Signals should be evaluated before promotion.

Riddle can construct historical evaluation corpora containing:

- positive examples
- negative examples
- ambiguous examples
- user-corrected examples
- disagreement examples

Example:

```text
Candidate:
owned_deployment_failed:v1

Positive examples          112
Negative examples          500
Ambiguous examples          73
User-corrected examples     41
```

Jev evaluates the candidate against the corpus.

Riddle records:

- observed agreement
- false positives
- false negatives
- uncertainty
- latency
- cost
- confidence distributions

These measurements must clearly identify the dataset and methodology.

Corpora built from corrections are biased: users only correct what they see, so missed Finds hidden in Spoil are under-represented. Every Signal also keeps a random audited sample of Spoil and Filed mail, labelled by the user in small batches, from which recall is measured.

Each recorded evaluation stores the model snapshot. Comparisons across time are valid only within a snapshot. Near-threshold disagreements are treated as noise, because inference is not deterministic even at temperature zero.

---

# 32. Adversarial Signal Testing

The Director/LLM should attempt to find counterexamples before recommending Signal promotion.

For:

```text
owned_deployment_failed
```

examples might include:

- another person's deployment failed
- warning followed by successful deployment
- historical failure quoted in a reply
- CI failure unrelated to deployment
- intentionally cancelled deployment
- marketing content discussing deployment failures
- ambiguous project ownership

These become permanent semantic regression tests.

---

# 33. Agent-Native Jev Capabilities

Jev capabilities must be exposed through Riddle Actions.

The Director should not have a private Jev integration.

Examples:

```text
evaluate_signal
evaluate_signal_set

get_signal
list_signals

propose_signal
test_signal

compare_signal_versions
simulate_signal

get_signal_evaluation
get_signal_metrics

find_signal_disagreements
find_signal_failures
```

This follows the core agent-native invariant:

> If Riddle can do it, the Director can do it through an Action when authorized.

---

# 34. Production Evaluation vs Experimental Evaluation

Two capabilities must remain distinct.

## evaluate_signal

Runs an existing versioned Signal exactly as production would.

```text
evaluate_signal(
    signal = requires_response:v3,
    subject = msg_123
)
```

## test_signal

Runs a candidate definition in an isolated experimental environment.

It cannot silently change production behaviour.

This allows the Director to experiment without introducing arbitrary semantic prompts into Sift.

---

# 35. Semantic Test-Driven Development

The Director should be capable of iteratively developing Signals.

```text
User Intent
    │
    ▼
Director
    │
    ▼
Search Signal Registry
    │
    ▼
Missing primitive
    │
    ▼
propose_signal()
    │
    ▼
Candidate v1
    │
    ▼
test_signal()
    │
    ▼
Inspect failures
    │
    ▼
Revise
    │
    ▼
Candidate v2
    │
    ▼
test_signal()
    │
    ▼
Compare
    │
    ▼
Recommend promotion
```

This is effectively semantic test-driven development against Jev.

---

# 36. Mesh Scope

Meshes use hierarchical scope.

```text
All Digs
   ↓
Dig Group
   ↓
Dig
   ↓
Exceptions
```

Examples:

```text
All Digs:
Receipts → Purchases

Work:
GitHub CI failures → Find

Personal:
Never classify family as Spoil
```

More specific policy may refine or override broader policy.

Global Meshes should not be physically duplicated into every Dig.

Resolution order for one message:

1. Collect the Meshes in scope: All Digs, then the Dig Group, then the Dig. A Dig belongs to at most one group.
2. Evaluate routes from the most specific scope to the least. The first matching route wins. Within one scope, Meshes are ordered by an explicit priority, then by name.
3. Evaluate every guard from every scope. Any guard veto overrides any route.
4. Escalate if no route matched and the message is not Filed by a default rule.

"Refine" means a more specific Mesh adds outputs, such as a Stratum or Notify. "Override" means it produces a different disposition. Both are visible in the Mesh Inspector.

Order is one list and is kept visible, because rules in an ordered list fail quietly: a generic "account notification → Filed" rule at Priority 0 filed GitHub push notifications before the GitHub Mesh at 10 saw them, and only a trace showed it.

- Two running Meshes in one scope may not share a Priority; a tie used to be broken by name. Creating, updating or activating a Mesh that would tie is refused with the free numbers either side. A new Mesh without a `Priority:` line takes the first free one from 0; an update without one keeps its version's.
- Every simulation reports `taken_first_by` (mail the candidate would decide on its own that an earlier Mesh decides, by Mesh) and `takes_from` (mail it would take from later Meshes). "Would decide" means a route matched, stood under the confidence policy, and no guard vetoed it (`simulate::decider`).
- The Meshes page opens on the run order (`mesh_order`): every running Mesh in position, what it decided over a window, and the same "taken first by" counts, replayed over stored facts and cached Signals without the judge.
- A Mesh can be marked a fallback (`set_mesh_fallback`): a catch-all such as Updates or Automated notifications that exists to decide what no specific Mesh did. The mark changes no decision and makes no version; the run order and previews stop flagging the mail earlier Meshes take from it, so the warnings that remain are specific rules competing. Newsletters and Automated notifications are seeded as fallbacks.
- Apply to history re-decides any message whose recorded effect would differ, not only its disposition: another Find type, other Strata, Notify, or joining or leaving a `+ Replaces` series.

## Named lists and rule packs

Rules that one person wrote carry that person's senders: `NOT sender_domain CONTAINS "uber.com"` in a promotions rule, a registrar's domain in a bank rule. Named lists move those values out of the text. `sender IN list:keep_promotions` (or `sender_domain IN list:…`) reads a list the person keeps (`save_list`, Settings and the Packs page): an address matches exactly, a domain matches it and its subdomains by whole labels. A Mesh may only name a list that exists.

A rule pack is a TOML file: an id, a version, an area, a summary, the lists its Meshes read (each with the question onboarding asks and generic defaults, never one person's values), the Meshes (text without `Priority:`), and how onboarding detects that it applies. Packs are built in (`crates/riddle-mesh/packs/`) or imported from a file (`import_pack`), and any set of Meshes can be written as one (`create_pack`, entries left out unless asked). `install_pack` creates the lists from the person's answers (merging into lists that exist), then places each Mesh by band and rank: safety (-100…-61), personal (-60…-21), specific (-20…19), general (20…39), fallback (40…69), at band start plus the Mesh's rank or the next free slot. Ranks, not install order, decide the order, so installing Shopping before Money still runs payments before promotions. New Meshes start in Shadow unless the person chooses Active; a Mesh of the same name that is not the pack's is refused unless the install may replace it, and one whose rules are unchanged keeps its status. Each Mesh records the pack and version it came from; installing a newer version replaces the pack's own Meshes and leaves the lists. Undo puts back every Mesh and list the install changed. Removing a pack sends its Meshes to Recently Deleted and keeps the lists.

The rule check (`rule_check`, `rule_samples`, `grade_decision`) shows each running Mesh with what it decided and ten recent decisions, one per sender before repeats, to mark right or wrong; a wrong one is corrected on the spot (`set_disposition`). It is how a pack earns trust for someone with no Director conversation behind it.

---

# 37. Mesh Versioning

Every Mesh is versioned.

Historical decisions retain the exact Mesh version responsible for them.

This allows Riddle to answer:

> Why did this message go to Spoil?

with:

> GitHub Notifications Mesh v7 classified it based on these Signals.

---

# 38. Mesh Simulation

A proposed Mesh change should be executable against historical data before activation.

Example:

```text
GitHub Notifications

v7 → proposed v8

Historical messages       18,421

Unchanged                  17,982
Changed                       439

Spoil → Find                   81
Find → Spoil                  119
Escalate → Spoil              201
Escalate → Find                38

New Jev calls required          0
```

Cached Semantic Signals make many Mesh simulations deterministic and inexpensive.

Simulation is free only when the Signals it needs are cached. A new Signal has no cache. Backfilling a new Signal over history runs under a policy: a bounded window (default 90 days), sampling beyond it, on-demand evaluation for older mail, and a per-Dig budget shown before the run starts.

---

# 39. LLM Escalation

The LLM handles the tail that bounded semantic judgment cannot safely resolve.

Escalation reasons include:

- low confidence
- conflicting Signals
- novel situations
- insufficient Context
- high-risk decisions
- complex reasoning

Thresholds should vary by decision.

There is no universal confidence threshold.

Moving a routine newsletter to Spoil may tolerate a lower confidence requirement than identifying phishing or performing an external side effect.

The LLM's output contract keeps Meshes as the only policy authority:

1. The LLM receives the normalized message, Thread State, the relevant Context, and the Signal distributions.
2. It returns refined Signal values with a rationale. It never returns a disposition.
3. The Mesh routes run again on the refined Signals.
4. If the result is still below threshold, the disposition is Unresolved.

Escalated decisions are therefore explained by the same Mesh version as everything else, with the LLM's refinement recorded in the trace.

Escalation has a budget per Dig (55). When the budget is exhausted, the message is Unresolved rather than silently routed.

---

# 40. Director Is LLM-First

The Director handles open-ended tasks.

Example:

> Figure out whether Adobe continued charging me after I cancelled.

The Director may plan:

```text
1. Find cancellation correspondence.
2. Establish cancellation date.
3. Locate subsequent receipts.
4. Determine whether they represent charges.
5. Compare dates.
6. Produce conclusion.
```

The Director can invoke Jev Signals as cheap semantic primitives during the investigation.

```text
Director
    │
    ├── search_messages()
    │
    ├── evaluate_signal(
    │      acknowledges_cancellation
    │   )
    │
    ├── search_messages()
    │
    ├── evaluate_signal(
    │      represents_charge
    │   )
    │
    └── synthesize conclusion
```

The architectural pattern is:

> **The LLM decides which questions need answering. Jev cheaply answers bounded questions. Riddle Actions gather evidence and perform work.**

---

# 41. Finds Are First-Class Entities

A Find is not merely a message flag.

Suggested structure:

```text
id
type

title
summary

priority
status            OPEN | SNOOZED | RESOLVED | DISMISSED | EXPIRED | SUPERSEDED
due_at            for Attend and Pay
snoozed_until
expires_at        from a route's `+ Expires:`; not a deadline
series            key (Mesh + sender, + subject with `Replaces: same subject`) and message date, from `+ Replaces`
kept_open_at      set when the user undoes a rule's closing; rules leave it open

created_at
resolved_at
resolved_by       USER | THREAD_STATE | TEMPORAL | DIRECTOR

sources           messages and threads, possibly across Digs
identity_key      for cross-Dig deduplication

reason
confidence

classification_id
decision_trace_id
```

Multiple source messages may support one Find.

Find creation from an unknown or low-reputation sender requires deterministic support (a fact, not only a Signal), because senders can steer Signals by writing "urgent" (58).

---

# 42. Expeditions Are Durable Agent Work

An Expedition contains:

```text
goal
scope             Digs, date range, tools allowed

budget            cost, time, tool calls
spent

status
plan

actions
checkpoints

evidence
findings

artifacts

activity
```

An Expedition persists independently from the Director conversation that created it.

It reports progress in the Task Centre, can be cancelled, and resumes from its last checkpoint after restart. Tools are withdrawn when the budget is exhausted, and a partial result is reported as partial.

---

# 43. Conversations Are Not State

Director conversation history may provide useful context.

It is not the source of truth.

Important outcomes become domain objects.

```text
Decision → state

Rule → Mesh

Semantic primitive → Signal

Delegated work → Expedition

Important result → Find

Mutation → Activity
```

---

# 44. Context Is Retrieved, Not Stuffed

The mailbox should not be placed wholesale into an LLM context window.

Riddle retrieves task-specific Context using:

- structured queries
- full-text search
- semantic retrieval
- relationship data
- Signal evaluations
- historical Activity
- Meshes
- relevant threads/messages

Principle:

> **The mailbox is the Director's workspace, not the model's context window.**

---

# 45. Search Is Hybrid

Riddle supports:

- exact search
- structured search
- full-text search
- semantic retrieval
- agentic investigation

Examples:

> Where's that receipt for the access point?

> What commitments have I made over email this month?

> What invoices haven't been dealt with?

> What did we decide about the deadline?

Exact search has operator parity with Gmail search and works across Digs. The semantic index covers subjects, normalized bodies and attachment names, and is built locally.

---

# 46. Corrections Are Structured Feedback

When a user corrects Riddle, the correction becomes structured data.

A correction may indicate:

- incorrect Signal
- missing Signal
- poor Signal definition
- bad Mesh
- missing Context
- changed preference
- incorrect LLM reasoning

Riddle should not blindly rewrite policy after every correction.

Repeated corrections can accumulate evidence.

The Director may eventually say:

> You've corrected GitHub review requests three times. I can update the Mesh.

Corrections are typed:

```text
move              changed disposition
retype            changed Find type
dismiss           not important
restore           from Spoil or Contamination
never_hide        added a sender to Never Hidden
external_move     a human action in another client (15)
```

Moves made by Gmail filters, other automations, or Riddle's own projection are not corrections and never train anything.

Corrections retract with undo (50): undoing an action also removes the evidence it generated.

---

# 47. Activity Ledger

Every meaningful mutation produces Activity.

Actor types include:

```text
USER
DIRECTOR
SIFT
MESH
AUTOMATION
SYSTEM
```

Activity records:

```text
who
what
when
why

inputs

affected entities

Action

result

provenance

undo information

trace_id
```

Product Activity is separate from conventional software logs.

---

# 48. Logging

Software logs are separate from Activity and from traces.

- `tauri-plugin-log` writes to the log directory and stdout in every build, with a size cap and rotation from day one, in `[rfc3339][LEVEL][category]` form. Noisy crates are pinned to warn.
- Categories are a fixed vocabulary: sync, projection, sift, judge, llm, mesh, director, expedition, store, jobs, ui. A `Timed` guard warns when an operation exceeds its budget.
- Structured payloads are appended as ` ||PAYLOAD||<json>` and expanded in the log viewer.
- Webview errors are forwarded to the backend log through a `ui_log` command and per-route error boundaries.
- The level is set in settings and applies on the next launch. Writes to Gmail always log at info.
- Helper processes log to their own files.

Redaction is a hard rule: logs carry identifiers only. No subjects, bodies, addresses, names or tokens. Developer Mode resolves identifiers from the store when displaying. The troubleshooting docs state what logs may still contain.

Logging never fails the work it describes. A failure to open the log directory does not stop Sift.

---

# 49. Provenance Is Data

Important decisions must retain provenance at execution time.

Do not attempt to reconstruct it afterward.

Classification provenance may include:

```text
classification
confidence

decision mechanism

signal evaluations

Mesh ID
Mesh version

Context references

reason

decision_trace_id

timestamp
```

Decision mechanisms should be distinguishable:

```text
DETERMINISTIC
GATE
JEV
MESH
LLM
TEMPORAL
DIRECTOR
USER
PROVIDER
```

---

# 50. Undo Is Architectural

Undo is not a UI trick.

Reversible Actions must define inverse behaviour or restoration data.

Bulk Actions should produce one logical Activity that can be undone together.

Example:

```text
Director moved 127 messages to Spoil

[Undo All]
```

Actions that cannot safely be undone must explicitly declare that property before execution.

Undo semantics:

- Undo is non-linear. Undoing an Activity restores only the messages that no later Activity touched; skipped messages are listed.
- Undo All on a bulk Activity skips messages the user has since handled, and says so.
- Undoing a Mesh change reverts the Mesh version, its reclassifications and their projections (15), and reopens Finds it resolved.
- Undo retracts the correction the action generated (46).
- Undo checks provider state first. If a message was moved or deleted in another client since, the undo reports a conflict rather than guessing.
- Sending has no undo beyond a local send delay. The UI says "Sent mail cannot be unsent" before sending.

---

# 51. Action Risk Metadata

Actions declare metadata such as:

```text
mutating
reversible
external_side_effect
risk
confirmation_policy
```

Conceptual autonomy levels:

```text
OBSERVE
SUGGEST
ACT
TRUSTED
```

Example policies:

```text
move_to_spoil
    ACT after a Shadow Period;
    confirm in place with Undo All for bulk

archive
    ACT

create_mesh / update_mesh
    SUGGEST by default;
    per-Mesh ACT after a Shadow Period with an observed agreement threshold

send_message / reply / forward
    two-step confirmation showing the Action layer's real recipients

permanent_delete / empty Contamination
    type the exact count

disable all Meshes / raise a Dig to TRUSTED
    press and hold, with coaching text

Director egress after untrusted content in the same task
    approval, with the reason shown on the disabled control
```

Prompts never replace authorization.

Every confirmation is re-validated in the Action layer, never only in the dialog. Policy objects (Meshes, Signals, Strata) are soft-deleted into Recently Deleted; restore refuses a name clash; purge is the only lossy path.

Autonomy is per Dig and per Action, unlocked from OBSERVE through SUGGEST to ACT as the Shadow Period (19) shows agreement. TRUSTED is opt-in per Dig.

Dev builds start in OBSERVE, and their mailbox writes start on Hold (59).

---

# 52. Idempotency

Mutating Actions must support idempotency.

Retries can arise from:

- agents
- provider APIs
- network failures
- background jobs
- application restart

Repeated execution must not accidentally duplicate side effects.

---

# 53. Incremental Sync

Initial Gmail ingestion is followed by provider change tracking.

```text
Initial Sync
     │
     ▼
Provider Change Cursor
     │
     ▼
Incremental Changes
     │
     ▼
Sift
```

Sync state is maintained per Dig.

The poller is a `tokio::select!` on a timer and a `Notify` trigger, with exponential backoff, slowdown under rate limit, and parking on auth failure. A credential-change subscriber wakes it on sign-in.

When the cursor has expired, sync performs a full resync: list messages newest first, reconcile labels against the expected projection (15), and report differences.

Initial sync fetches metadata first and bodies lazily, newest first, and shows its progress in the Task Centre because it takes hours for a large mailbox.

---

# 54. Durable Background Work

Long-running work should be modeled as durable jobs.

Examples:

- initial synchronization
- historical Sift
- Signal evaluation
- Signal re-evaluation
- Mesh simulation
- Expeditions
- embeddings
- bulk actions
- experimental replay

Where appropriate, jobs should resume after Riddle closes and reopens.

---

# 55. Jobs and Scheduling

Every job competes for four limited resources:

```text
Gmail API      about 300 message fetches per minute per Dig
Judge          1,200 requests per minute, shared by all Digs
LLM            provider rate limits and the cost budget
SQLite         one writer at a time
```

Priority classes, highest first:

```text
INTERACTIVE     Director requests, user-triggered Actions
INCOMING        Sift of newly arrived mail
CATCH_UP        backlog after wake, newest first
PROJECTION      outbox writes
BACKFILL        historical Sift, new-Signal backfill, embeddings
EXPERIMENT      shadow evaluation, replay
```

Rules:

- one rate limiter per resource, shared across Digs, fair between Digs
- back-pressure when a resource saturates; lower classes yield
- jobs checkpoint progress, retry with backoff, declare dependencies, cancel cleanly, and pause on sleep, network loss or provider outage
- job steps are idempotent (52)
- a daily budget per Dig for judge and LLM spend; exhaustion pauses BACKFILL and EXPERIMENT first
- CPU-bound work (HTML parsing, quoted-reply stripping, local embeddings) runs on blocking threads; SQLite has one writer with pooled readers and short transactions

The Task Centre is a product surface, not a Developer Mode panel:

- a status indicator: Caught up, Catching up, Degraded
- a list of running and queued jobs with progress, estimated time, pause and cancel
- completion notifications for long jobs
- an onboarding progress view for the multi-hour initial sync

The morning summary reads from it and says whether processing is complete.

---

# 56. Model Providers Are Replaceable

Riddle's AI boundary must support multiple providers/models.

Different models may be selected for:

- Jev-style semantic judgment
- classification
- embeddings
- summarization
- Director reasoning
- verification
- Signal generation
- adversarial test generation

Selection may consider:

- task
- quality
- latency
- cost
- privacy
- context requirements
- local availability

No single model provider should become architectural infrastructure.

Operations:

- the model catalog is imported from a shared settings file or listed live; no model id is hard-coded; the judge model is pinned to an exact version
- model tiers (judge, escalation, Director, verification) record the evidence for their selection in code
- prompt caching with a rolling checkpoint and a cache-free retry for Director and Expedition calls
- a circuit breaker per provider (Gmail, judge, LLM) with a probe slot that cannot wedge; retrying a single call is the right answer to a blip and the wrong answer to an outage
- a usage ledger with task-local attribution to Sift, Director, Expedition or Simulation; anything that cannot be priced is reported as unpriced, never as free; a usage row never fails the work
- the model provider credential is a separate principal from the mailbox credential
- the Director's system prompt is a versioned, runtime-editable file with placeholders, like a Signal
- when a provider is unavailable Riddle degrades to Unresolved; it never hides mail because a classifier was down

---

# 57. Privacy Boundaries

Riddle should minimize mailbox content sent externally.

Configurations available in v1, per Dig:

- local-only: deterministic facts, Gmail categories and local embeddings; no cloud calls
- cloud judgment, everything else local
- cloud judgment and cloud reasoning, with redacted Context

A Work Dig on an employer account defaults to local-only until the user chooses otherwise.

Privacy policy differs by Dig, and the per-Dig store (11) makes the boundary physical.

What leaves the device is stated in one sentence in the app, with the vendor named. Vendors must not train on the data. The Limited Use disclosure required by Google is shown in the app.

Credentials never enter agent context.

The Director receives capabilities, not provider authentication tokens.

---

# 58. Security Threat Model

Mail is untrusted input, and the Director holds tools that send mail and write policy. The incidents that shaped this section are collected in `ARCHITECTURE-REVIEW.md` §5.3.

Controls:

1. Mail, attachments and tool results enter model context delimited and marked untrusted, never as user or system turns.
2. Contamination and injection-flagged mail never enter Director or Expedition context unless the user opens them deliberately.
3. Within one task, once untrusted content has been read, egress Actions (send, reply, forward, web fetch, create or update a Mesh, add to Never Hidden) are approval-gated. The disabled control states the reason.
4. Approval prompts show the Action layer's real arguments: actual recipients, actual rule text.
5. The Action layer enforces deterministic recipient policy independent of the model. No forwarding to a never-before-seen external address without approval.
6. Durable state (Meshes, Signals, trust facts, Never Hidden) is created only from explicit user actions. Director proposals show the inducing evidence and are rejected when it traces to a single sender or to quarantined mail.
7. Agent output never renders remote images; links are allow-listed, including reference-style markdown.
8. Never Hidden and never-Find gates run before any Signal and cannot be overridden by one.
9. An injection classifier is defence in depth, not the boundary.
10. An adversarial regression suite covering known incidents gates releases.

Client hardening:

- HTML mail renders in a sandboxed `srcdoc` iframe with scripts and same-origin disabled, remote content blocked by default, and `cid:` images served through a custom protocol; a Rust-side sanitizer runs first
- strict CSP; capabilities minimal and per window; credential commands guarded by `require_main`
- OAuth refresh tokens in the OS keychain through `keyring` with native features enabled; the webview receives only `*_present` booleans
- the store is encrypted at rest with SQLCipher, key in the keychain
- files holding secrets are written 0600 through an atomic private write
- exports and diagnostics default to redaction
- crash reports never include minidumps unless scrubbed

---

# 59. Developer Safety

A dev build must never touch the real mailbox by accident.

- Dev and prod are split at build time: separate data directory, keychain service suffix, log file, and a DEV badge in the UI.
- Dev builds start in OBSERVE, with mailbox writes on Hold, and a separate OAuth client and test account are the default. What a dev build may write is `ProviderWrites` (`set_mailbox_writes`, under Developer → Mailbox writes): Hold writes nothing; Ask leaves queued writes for one "Send" (`send_held_changes`, a single confirmation for the batch, or `discard_held_changes`) and asks before any Action that calls the provider; Allow writes as a release build does and needs press and hold. Actions split by how they touch the mailbox. `external` ones call the provider while they run (send, permanent delete, unsubscribe, sign-in) and are what Hold refuses and Ask asks about. `projected` ones change Riddle and queue the matching write in the outbox (corrections, archive, Strata, Shadow review); they always run, and the outbox holds their writes until Allow or a Send. It replaced one armed switch, under which corrections shared the external flag and a batch of seven was refused seven times, though none of them would have touched the mailbox while the account's projection was off.
- Dev builds are signed with a stable Developer ID so keychain ACLs survive rebuilds.
- Tests never open the real store or call Gmail: a test-host guard plus an in-memory store.
- A panic inside a command is caught at the boundary and the webview times out (7).
- The persistence gate (11) makes silent write failure impossible.

---

# 60. Jev Decision Traces

Whenever Jev participates in a meaningful decision, Riddle should persist an inspectable structured trace.

This is not unrestricted hidden model chain-of-thought.

It is a deliberate reasoning artifact.

A trace may include:

```text
objective

strategy
model
version

Signals evaluated

questions
choices

evidence references

probability distributions

conclusion

confidence

Mesh

resulting Actions

latency

token usage

cost

app version
build stamp

timestamp
```

Principle:

> **A meaningful Jev decision must never be reduced to “Jev said so.”**

---

# 61. Traceability

Actions caused by intelligent decisions must link to those decisions.

```text
Decision Trace
      │
      ▼
Classification
      │
      ▼
Action
      │
      ▼
Activity
```

Navigation must work in both directions.

From an Activity:

```text
What happened?
      ↓
Why?
      ↓
Decision
      ↓
Signals
      ↓
Evidence
      ↓
Mesh
```

---

# 62. Reasoning Experiments

Riddle should support comparing reasoning strategies.

Potential strategies include:

```text
Jev
LLM direct classification
Jev + Mesh
Jev + LLM verification
future strategies
```

Experimental execution must use the same authorized Action boundary as production.

---

# 63. Shadow Evaluation

Experimental strategies should be capable of operating in shadow mode.

```text
Incoming Message
       │
       ├──────────────► Production Strategy
       │                       │
       │                       ▼
       │                 mailbox result
       │
       └──────────────► Shadow Strategy
                               │
                               ▼
                         recorded result
                         NO MUTATION
```

This allows safe real-world evaluation.

---

# 64. Historical Replay

Historical messages can be replayed through:

- different Signal versions
- different Signal Sets
- different Jev configurations
- different Mesh versions
- different thresholds
- different LLMs
- different reasoning strategies

Replay never mutates the real mailbox.

Production history therefore becomes an evaluation corpus.

---

# 65. Evaluation Metrics

Reasoning approaches are evaluated against product outcomes.

Potential metrics include:

- correction rate
- observed user agreement
- Find precision
- Find recall
- false positive rate
- false negative rate
- reversal rate
- evidence efficiency
- LLM escalation rate
- user acceptance
- latency
- cost

The central question is:

> **Did this approach make Riddle meaningfully better?**

Not:

> Did the model appear to reason more?

Targets that define Attention Zero, measured on audited samples and not on corrections alone:

```text
Find recall               ≥ 0.98 on the audited sample
false Spoil rate          ≤ 0.5% of human-authored mail
Unresolved rate           ≤ 5% after the Shadow Period
LLM escalation rate       ≤ 10%
cost per 1,000 messages   stated per Dig, with a cap
```

Promotion of a Signal or Mesh version is gated on these, per Dig.

---

# 66. Testing and CI

- Rust unit tests run against an in-memory store.
- Live tests against Gmail and the judge are `#[ignore]`, and each states in its header the claim it proves.
- Parity tests keep UI enumerations (dispositions, Find types, Mesh outputs) equal to the Rust enums.
- A structure-preserving synthetic mailbox fixture is derived from real mail with content replaced.
- Config-compatibility tests load files written before each feature.
- A hang test proves a panicking command cannot wedge the UI.
- Frontend tests cover pure reducers (progress, Task Centre state) with vitest; `oxlint` and `clippy` run in CI.
- CI runs on pull requests, not only on tags.
- The adversarial suite (58) and the Signal regression corpora (32) run before release.

---

# 67. Developer Mode

Developer Mode is a first-class product capability.

It exists to:

1. Debug Riddle.
2. Evaluate Jev, Signals, Meshes, Sift, and Director behaviour.
3. Demonstrate Riddle's architecture to engineers.
4. Make probabilistic AI behaviour understandable.

Developer Mode is an **interactive observability environment**, not merely verbose logging.

Principle:

> **If Riddle made a decision, Developer Mode should make it possible to understand what participated in that decision, what information was used, what the systems returned, what policy was applied, what it cost, how long it took, and what happened afterward.**

Developer Mode opens with an operations pane, ahead of the inspectors below:

```text
Runtime      version, build stamp, debug or release, log level, OS
Storage      per-Dig store paths and sizes, cache and log directories,
             reveal in Finder, open config, clear caches
Logs         live tail; filter by category, level and text; payload expansion
IPC          in-flight commands with elapsed time
Diagnostics  copy a redacted diagnostics block; export a redacted bundle
Probes       run the Gmail and judge probes outside the pipeline
```

Developer Mode grants no authority (91). The inspectors from 68 onward are phased after the product surfaces in 5 and 55.

---

# 68. Message Inspector

Every message should expose its complete processing path.

Example:

```text
MESSAGE

Gmail ID
Thread
Dig
Received

────────────────────────

DETERMINISTIC FACTS

sender_domain
automated_sender
known_sender
directly_addressed
thread_length
provider_labels

────────────────────────

JEV SIGNALS

requires_response:v3
YES .08
NO  .89
UNCLEAR .03

review_requested:v2
YES .96
NO  .04

────────────────────────

MESH

GitHub Notifications v7

review_requested > .80 ✓

→ FIND: REVIEW

────────────────────────

ESCALATION

LLM required: NO

Reason:
High-confidence Mesh result

────────────────────────

TIMING

Sync       17ms
Facts       3ms
Jev       184ms
Mesh        1ms
Persist     4ms

Total     209ms

────────────────────────

COST

Jev input tokens
Jev questions
Jev cost
LLM cost
Total
```

---

# 69. Raw Jev Inspector

Every Jev evaluation should expose, where available and safe:

## Request

```text
model
Signal Set
Signal versions

input state
questions
choices
configuration

input tokens
timestamp
```

## Response

```text
raw structured response

probability distributions

selected results

latency
token usage
cost

provider request identifier
```

Developer Mode should support:

```text
[Pretty]
[Raw]
```

Secrets and credentials are always removed.

---

# 70. Signal Inspector

Every Signal should expose:

```text
name
version

definition

choice definitions

required Context

current evaluation

Jev probabilities

producing evaluation

Meshes using it

historical metrics
```

This allows engineers to understand Riddle's semantic vocabulary directly.

---

# 71. Signal Registry Inspector

Developer Mode should expose the complete Signal Registry.

Useful fields include:

```text
Signal
Version
Status
Usage count
Meshes using it
Observed agreement
Correction rate
Average confidence
Average latency
Average cost
```

Metrics must always identify their dataset and methodology.

---

# 72. Signal Playground

Developer Mode should allow an engineer to select a message and manually evaluate Signals.

```text
MESSAGE

"John requested your review on PR #412"

Signals

☑ requires_response:v3
☑ review_requested:v2
☑ personally_relevant:v3
☐ urgency:v2
☐ routine:v1

[Run Jev]
```

The results should expose:

- probabilities
- latency
- tokens
- cost
- request
- response

---

# 73. Signal Workshop

Developer Mode should provide a workspace for creating and evolving Signals.

The engineer can watch:

```text
Director
   │
   ├── searches Signal Registry
   │
   ├── identifies missing concept
   │
   ├── proposes Signal v1
   │
   ├── selects historical corpus
   │
   ├── generates adversarial tests
   │
   ├── invokes Jev
   │
   ├── analyzes failures
   │
   ├── proposes v2
   │
   ├── reruns evaluation
   │
   └── recommends promotion
```

Every stage should be inspectable.

---

# 74. Signal Version Comparison

Developer Mode should compare versions against the same examples.

Example:

```text
requires_response

               v3        v4

YES            .62       .91
NO             .31       .07
UNCLEAR        .07       .02

Expected:
YES
```

Dataset-level comparison may show:

```text
Messages             10,000

Observed agreement

v3                    93.1%
v4                    97.2%

False positives
v3                     5.2%
v4                     1.8%

Average latency
...

Average cost
...
```

---

# 75. Mesh Inspector

Developer Mode should expose actual Mesh execution.

```text
GITHUB NOTIFICATIONS v7

INPUT FACTS

source = GitHub

INPUT SIGNALS

review_requested       .96
ci_failure             .02
requires_response      .08
personally_relevant    .91
routine                .62

POLICY

IF review_requested > .80
    → Find: Review             ✓ MATCH

RESULT

Find: Review
```

This clearly demonstrates the boundary between:

- Jev judgment
- deterministic policy

---

# 76. Mesh Simulator

An engineer should be able to:

1. Modify a Mesh.
2. Select historical data.
3. Execute the proposed version.
4. Compare outcomes.
5. Inspect changed decisions.

Cached Signals should be reused whenever their definitions have not changed.

---

# 77. Escalation Inspector

When Jev escalates to an LLM, the transition should be visible.

Example:

```text
requires_response

YES       .47
NO        .44
UNCLEAR   .09

Auto threshold:
.80

RESULT
ESCALATE
```

Then:

```text
LLM VERIFICATION

Model
Context supplied
Signal distributions
Applicable Mesh
Decision
Result
Cost
Latency
```

This makes the fast-judgment/deep-reasoning boundary concrete.

---

# 78. Director Inspector

Developer Mode should expose the Director's structured observable execution.

Example:

```text
GOAL

Determine whether Adobe continued
charging after cancellation.

PLAN

1. Find cancellation correspondence.
2. Establish cancellation date.
3. Find subsequent receipts.
4. Determine whether receipts are charges.
5. Compare dates.
6. Report conclusion.

ACTION
search_messages(...)

JEV
acknowledges_cancellation
YES .97

ACTION
search_messages(...)

...
```

Riddle exposes structured plans, Actions, evidence, and decisions—not hidden model scratch reasoning.

---

# 79. Action Inspector

Every Action should expose:

```text
Action
Actor

Input

Started
Completed
Duration

Risk
Reversible

Triggered By

Provider operations

Result

Activity

Undo information
```

---

# 80. Activity Timeline

Developer Mode should expose a system-wide timeline.

Example:

```text
10:42:17.502 Gmail change received
10:42:17.519 Message persisted
10:42:17.523 Sift started
10:42:17.526 Facts extracted
10:42:17.529 Jev started
10:42:17.713 Jev completed
10:42:17.714 Signals persisted
10:42:17.715 Mesh v7 evaluated
10:42:17.716 Find created
10:42:17.718 Activity persisted
10:42:17.721 UI notified
```

Each entry should be expandable.

---

# 81. Correlation and Tracing

Every processing chain receives a trace identifier.

Example:

```text
trace_id = sift_01K...
```

The trace links:

```text
provider event
message
Sift
Jev evaluation
Signals
Mesh
LLM escalation
classification
Action
Activity
Find
```

Where useful, structured telemetry should contain:

```text
trace_id
span_id
parent_span_id

component
operation

input references
output references

strategy
model
version

duration
token usage
cost

decision
confidence

provenance

error
fallback

timestamp
```

---

# 82. Live Pipeline

Developer Mode should include a Live Pipeline.

Example:

```text
10:42:17  Message received
             │
10:42:17  Facts extracted
             │
10:42:17  Jev evaluating...
             │
10:42:17  Jev 184ms
             │
             ├ review_requested .96
             ├ ci_failure       .02
             ├ requires_reply   .08
             ├ relevance        .91
             └ routine          .62
             │
10:42:17  Mesh v7 matched
             │
10:42:17  Find: Review
```

This should use real telemetry.

---

# 83. Developer Overlay

Normal Riddle UI may optionally expose compact diagnostic overlays.

Example Find:

```text
Review requested on PR #412

[JEV .96] [MESH v7] [NO LLM] [209ms]
```

Example Spoil:

```text
GitHub workflow succeeded

[SPOIL .94] [JEV] [MESH v7]
```

Clicking opens the corresponding inspector.

---

# 84. Cost Observatory

Developer Mode should aggregate AI usage.

Example:

```text
TODAY

Messages sifted        1,842

Jev
Requests                 711
Questions              3,481
Input tokens         921,441
Cost                    $...
Average latency         181ms

LLM verification
Requests                  37
Cost                    $...

Director
Requests                  14
Cost                    $...

Cost / 1,000 messages    $...

Jev-only decisions       94.8%
LLM escalations           5.2%
```

This validates whether the architecture actually achieves its economic goals.

---

# 85. Decision Quality Observatory

Developer Mode should also expose observed quality.

Example:

```text
LAST 30 DAYS

Sift decisions         42,819
Accepted               41,706
Corrected               1,113

Observed agreement       97.4%

Jev-only                 ...
Jev → LLM                ...
LLM-only                 ...

Most corrected Signals

routine:v1               ...
personally_relevant:v3   ...
requires_response:v3     ...
```

These are observed product outcomes, not claims of universal model accuracy.

---

# 86. Jev vs. LLM Observatory

Experimental runs should support side-by-side comparisons.

Example:

```text
EXPERIMENT

Jev + Mesh vs LLM

Dataset:
10,000 messages

                 Jev      LLM

Observed result    ...
Avg latency        ...
Total cost         ...
Corrections        ...

Disagreement:
612 messages

[Inspect Disagreements]
```

Disagreements are especially valuable evaluation material.

---

# 87. Performance Observatory

Developer Mode should expose operational performance such as:

```text
Sync latency

Sift queue depth

Jev concurrency
Jev rate-limit utilization

LLM escalation queue

SQLite latency
FTS latency

Embedding queue

Background jobs

Gmail API calls

Signal cache hit rate
```

This allows engineers to distinguish model performance from application performance.

---

# 88. Failure Inspector

Failures are first-class pipeline events.

Example:

```text
JEV EVALUATION FAILED

Signal Set:
GitHub:v4

Attempt:
2 / 4

Fallback:
Queued retry

Disposition:
UNRESOLVED (retry queued)

Mailbox mutation:
NONE
```

Fallback behaviour must itself be observable.

---

# 89. Exportable Traces

Developer Mode should allow sanitized trace export for:

- bug reports
- architecture discussions
- Jev experiments
- benchmarks
- demonstrations

Potential export content:

```text
message metadata
redacted content

deterministic facts

Jev request
Jev response

Signal evaluations

Mesh evaluation

LLM escalation

Actions

timings
costs

outcome
```

Redaction is the default.

---

# 90. Demo Mode

Developer Mode should support a presentation-oriented Demo Mode.

It uses real telemetry but presents the pipeline visually:

```text
INPUT
  ↓
DETERMINISTIC FACTS
  ↓
JEV
  ↓
SEMANTIC SIGNALS
  ↓
MESH
  ↓
ESCALATION?
  ↓
RESULT
  ↓
ACTION
```

A compelling engineering demonstration should be possible:

1. Send Riddle an email.
2. Watch it arrive.
3. Watch deterministic facts appear.
4. Watch Jev evaluate the Signal Set.
5. Inspect probabilities.
6. Watch the Mesh consume those Signals.
7. See whether an LLM is required.
8. Watch the resulting Action.
9. Inspect Activity and provenance.
10. Correct the result.
11. Modify or create a Signal/Mesh.
12. Replay against historical mail.
13. Compare outcomes.

Demo Mode must never fabricate telemetry.

---

# 91. Developer Mode Security

Developer Mode exposes more information but does not gain more authority.

Requirements:

- explicitly enabled
- credentials never displayed
- authentication tokens always redacted
- provider secrets never enter traces
- sensitive content clearly distinguished from metadata
- exports default to redaction
- enabling Developer Mode grants no additional Action permissions

Developer Mode observes security boundaries.

It does not bypass them.

---

# 92. No Intelligent Decision Without Structured Telemetry

Repository-level engineering rule:

> **No intelligent decision without structured telemetry.**

A classifier buried inside the application that simply returns `true` or `false` without provenance violates the architecture.

Every intelligent subsystem must expose enough structured information to evaluate its behaviour.

---

# 93. Architectural North Star

Consider:

> These GitHub messages are mostly junk. I only care when someone asks me to review something or one of my builds fails.

The Director:

1. Parses the intent.
2. Searches the Signal Registry.
3. Finds `review_requested`.
4. Finds `ci_failure`.
5. Determines whether existing `personally_relevant` adequately represents “one of my builds.”
6. If necessary, proposes a new Signal.
7. Tests it against historical email.
8. Generates adversarial examples.
9. Invokes Jev through Riddle Actions.
10. Compares Signal versions.
11. Builds a proposed Mesh.
12. Simulates the Mesh against historical Signals.
13. Shows the user the impact.

For example:

```text
Proposed GitHub Mesh

Historical messages:
143

121 → Spoil
16  → Review Finds
6   → CI Failure Finds

Signal evaluations:
141 cached, 2 need the new Signal

Backfill:
owned_work_failed@v1 over 90 days, 143 messages

Expected additional judge cost:
$0.01

[Inspect]
[Apply]
```

After approval:

1. Mesh version is created.
2. Activity records the change.
3. Affected mail is reclassified.
4. Finds are created.
5. UI updates.

Later:

> Why is this GitHub message a Find?

Riddle can show:

```text
GitHub Notifications v8

review_requested        .04
ci_failure              .97
personally_relevant     .91

Matched:

ci_failure > .85
AND
personally_relevant > .70

→ Find: CI Failure
```

The user can:

```text
[Inspect decision]   (Developer Mode only)
[View Signal]        (Developer Mode only)
[View Mesh]
[Mark as Filed]
[Undo]
```

The card leads with the one-sentence `reason` the pipeline wrote for a person; the route walk above sits under a "How it was decided" disclosure.

If corrected, the correction becomes structured feedback available for Signal and Mesh evaluation.

At every point Riddle knows:

- what happened
- which system did it
- which version did it
- what facts existed
- what Jev was asked
- what Jev returned
- which Signals resulted
- which Mesh consumed them
- why policy matched
- whether an LLM was used
- which Action occurred
- what it cost
- how long it took
- whether the user accepted or corrected it
- how to reverse it

---

# 94. Non-Functional Requirements

Performance:

```text
Sift latency, one message, judge only     ≤ 1 s from arrival
Catch-up throughput                       quota-bound; 300 messages per minute per Dig
Initial sync, 100k messages               hours; shown in the Task Centre
UI                                        smooth lists at 100k rows, paged
Idle CPU                                  near zero; App Nap allowed when idle
Disk                                      watermarks; bodies evicted first
```

Resilience:

- offline: reading, search, drafting and queued Actions work; sync resumes on reconnect
- provider or model outage: degrade to Unresolved; nothing is hidden because a classifier was unavailable
- crash-safe: every mutation is a transaction; jobs resume from checkpoints
- schema migrations are versioned, with a backup first

Internationalisation and accessibility:

- a language fact per message; non-English mail lowers autonomy or takes the LLM path until measured
- timezone-aware deadlines
- keyboard-first triage; screen-reader labels on every control

Cost governance:

- a per-Dig daily budget with a hard cap; BACKFILL and EXPERIMENT pause first
- the Cost Observatory (84) reads from the usage ledger (56)

Operability:

- the window obeys the platform: move, resize, minimise, zoom, remembered position, off-screen rescue; a build is not runnable until it does
- signed releases with an updater (6)
- a redacted diagnostics bundle
- troubleshooting docs naming the log location and what logs may contain

Process:

- the README carries a status line; a roadmap lists what is not planned
- CONTRIBUTING records house rules; CLAUDE.md records agent rules
- module headers record decisions and the incidents behind them

---

# 95. Architectural Invariants

The following are non-negotiable unless explicitly superseded by a later architecture decision.

1. **Actions are Riddle's capability boundary.**
2. **The UI and Director operate on the same Riddle.**
3. **The Director never directly mutates application state.**
4. **Jev never directly mutates application state.**
5. **Prompts are not authorization.**
6. **Conversations are not state.**
7. **Every meaningful mutation leaves an Activity trail.**
8. **Important automated decisions retain provenance.**
9. **Reversible Actions are reversible by design.**
10. **Meshes are versioned.**
11. **Mesh changes are historically simulatable.**
12. **Semantic Signals are managed, versioned domain objects.**
13. **Signals represent semantic judgments; Meshes represent policy.**
14. **Existing Signals should be reused before new Signals are created.**
15. **New production Signals must be evaluated before promotion unless explicitly overridden by the user.**
16. **The Director can invoke Jev only through Actions.**
17. **Experimental Signal evaluation cannot silently alter production behaviour.**
18. **Jev is the preferred initial high-volume semantic decision engine, but remains replaceable.**
19. **LLMs handle uncertainty, novelty, synthesis, planning, and open-ended reasoning.**
20. **Experimental reasoning cannot bypass Actions or authorization.**
21. **Shadow before autonomy where practical.**
22. **Hypotheses, evidence, Signals, and decisions are structured artifacts where useful.**
23. **Context is retrieved, not stuffed.**
24. **Credentials never enter model context.**
25. **Provider concepts and Riddle concepts remain distinct.**
26. **Finds are first-class entities.**
27. **Expeditions are durable.**
28. **Corrections are structured feedback.**
29. **Agent autonomy is explicit and permissioned.**
30. **Models and providers are replaceable.**
31. **Mutations are idempotent.**
32. **Riddle begins as a modular application, not distributed microservices.**
33. **Developer Mode is an architectural capability, not an afterthought.**
34. **No intelligent decision without structured telemetry.**
35. **Developer Mode exposes behaviour but never bypasses security.**
36. **Raw hidden model chain-of-thought is not application state; structured reasoning artifacts are.**
37. **Riddle remains useful independently of Jev.**
38. **Jev's value is established through measured product outcomes, not assumed.**
39. **Bookkeeping, logging and telemetry never fail the work they describe.**
40. **Progress and status derive from real events; a disabled control states its reason.**
41. **Nothing changes silently on update; behaviour changes are previewed and approved.**
42. **Never Hidden gates run before any Signal, and no Signal can override them.**
43. **Untrusted content never becomes durable policy without an explicit user action.**
44. **Meshes pin Signal versions; thresholds are written against calibrated scores.**
45. **Every message has exactly one disposition, and Unresolved is one of them.**
46. **Dev builds cannot mutate a real mailbox until the user allows it, or sends what waits.**
47. **Logs carry identifiers, never content.**
48. **Provider-side changes are attributed before they are treated as corrections.**
49. **Sent mail is sifted.**
50. **The LLM refines Signals; Meshes remain the only policy authority.**

---

# 96. Trust on Evidence

The UX study (`explore/`, September 2026) asked how a user comes to let Riddle
act, and the answer was evidence the user can inspect, never a survey. Five
mechanisms carry it:

- **Trust stats** (`trust_stats { digId?, sinceDays? }`): Sift decisions in
  the window, corrections that still stand, Activity entries the user undid,
  Spoil decisions held below Act on accounts that would archive, and each
  rule's precision (`decisions` vs `reversed`, plus Shadow `proposed` vs
  `agreed`). The Summary's trust card renders it; nothing is estimated.
- **The seducible moment**: when an account below Act with Gmail changes on
  has forty consecutive Sift Spoil calls that stood, `trust_stats.offer`
  names it once. The offer states the rung change and the undo guarantee,
  and "Not now" waits for the next clean run. It never nags.
- **Shadow review** (`review_shadow`): Agree counts for the proposing Mesh
  (`shadow_agreed`) and marks the row; Disagree records the user's own
  decision as a correction held against the proposing Mesh, not against
  whatever decided the message for real. Until this Action nothing recorded
  agreement; the Shadow Period could never be earned.
- **Demotion on evidence**: when the user has reversed a quarter or more of
  an Active Mesh version's last twenty-plus decisions on an account, the
  Mesh steps back to Shadow and a `notify` event says so. Promotion is earned
  in the Shadow Period; demotion is earned the same way.
- **Per-rule autonomy**: a Mesh may carry `Autonomy: Observe | Suggest | Act`
  in its header, a cap below the account's rung. The projection gate holds
  decisions of a capped Mesh whatever the account allows; `Trusted` is
  refused as a Mesh cap because sending is an account decision. An update
  without the line keeps the cap.

Each of these is answered by the store, not guessed by the client. A
message row says whether it is a Shadow proposal and how the user reviewed
it (`MessageSummary.shadow`, `shadow_review`; `search { shadowOnly }` lists
the proposals, and a disagreement keeps the proposal beside the decision).
A Dig says what Sift would do to its mailbox and what holds it
(`Dig.mailbox { writes, held: autonomy | dev_unarmed }`), a Dig group names
the Meshes that use it (`used_by`), an `ActionOutcome` carries its Activity
sentence and the entities it touched (`summary`, `affected`, which the client
uses to invalidate exactly what changed), a demoted Mesh records when and on
what counts (`Mesh.demoted { at, reversed, decided }`, cleared by any status
the user chooses), and each rule's precision row says whether the engine's
demotion rule fired (`reversed_often`, `riddle_core::reversed_often`). The
client had derived all five from dates, settings and string matches and got
each of them wrong in some state.

Two smaller rules follow from the same study. A **guard-only Mesh** (the
user's "Your corrections", one `NEVER … IF sender = …` line per one-click
rule) keeps its Active status across updates: guards veto and never hide
mail, so there is nothing to earn. **Notifications** default to a brief
(`settings.notifications`): only a Never Hidden sender, or a Pay or Respond
Find due within a day, interrupts at once; everything else is said at the
chosen times, the text carries the Find's type and reason and never its
subject, and the dock badge counts decisions waiting (open Finds plus
Unresolved), never messages.

---

# 97. Rules in Words

A Mesh route is written in the evaluator's grammar (`ELSE IF
payment_related@v1 > .80 AND requires_action@v1 > .70 → Find: Pay + Stratum:
Finance`), and until this section that grammar was also the reason shown on
the Why card. `riddle_mesh::humanize` reads it as a sentence, deterministically
and without a model call, in two voices:

- **The rule as written** (`Route.words`, `Guard.words`, filled by the parser
  and backfilled on read for Meshes stored earlier): "When payment related
  is very likely and requires action is likely, it is something to pay under
  Finance." The Meshes page shows it under each route and guard.
- **The rule against one message** (the classification's `reason` for a
  Mesh decision): "It appears payment related is almost certain and requires
  action is likely, so it is something to pay under Finance." The decision
  reads the message's actual scores, facts and thread length, says "was not
  scored" for a Signal the judge did not answer, and names the guard when
  one vetoed the route. Lists and detail headers lead with it. The Why card
  does not: its job is to show exactly what happened, so it shows
  `matched_rule`, the grammar, and the route walk with each clause's actual
  value.

The words for a probability are calibrated language, not invention: Sherman
Kent's "Words of Estimative Probability" as standardised in ICD 203, with
the IPCC scale's top band. The bands are Riddle's, cut where Mesh thresholds
sit: virtually certain (≥ .99), almost certain (≥ .95), extremely likely
(≥ .90), very likely (≥ .80), likely (≥ .65), more likely than not (> .50),
about as likely as not (.45 to .50), unlikely (.20 to .45), very unlikely
(.05 to .20), almost certainly not (below .05). A rule reads its threshold as
a floor, so `> .80` is "very likely" and a score of .96 that cleared it is
"almost certain"; the two never contradict. Facts read as statements about
the message, each with its negation ("you were only copied", "the sender's
domain imitates one you know"); an unknown fact or Signal falls back to its
name in words, so a new Signal reads acceptably before anyone writes a gloss.

A reason is written when the decision is made. Decisions made before there
were words are rewritten once per Dig at boot (`App::rewrite_reasons_in_words`):
each current Mesh decision is read against its stored trace, exactly as the
Why card replays it, and the classification and the Finds that carry it get
the sentence, with no judge call. `resift { digId? }` is for a change of
rules, not of wording: it retires every decision Riddle made (never the
user's), as superseded rows the inspector still shows, and queues a Sift job
per account; cached Signal scores mean most of it costs nothing.

Activating a Mesh decides new mail only. Two entry points send past mail
back to Sift through one mechanism (`send_back_to_sift` in riddle-app):
"Apply to past mail" on the Meshes page (`apply_mesh_to_history`, the
Mesh's changed set from a window, previewed and confirmed) and "Decide
again" in a Find's Why drawer (`decide_again`, that one message). Both
retire the current decision, close an open Find on the message as
Superseded (a fresh one appears if it still is one), and queue the pass.
Undo puts the earlier decision and the Find back, and the Sift pass the
action itself caused does not count as "handled since".

The changed set is every message the current rules decide differently,
every message a `+ Replaces` route would pull into its series, and every
message never scored on a Signal the Mesh reads. The last matters after a
Signal changes: a Mesh pins `signal@vN`, the simulation cannot score a
message against a version the judge has not evaluated, and such mail used
to count as unchanged, so history kept decisions made without the Signal
the rule now reads. The preview says how many need a fresh judge call and
what that costs; the cap is 2,000 messages per apply.

---

# 98. Facts Before Signals

The first real mailbox Riddle sifted end to end came back three quarters
Unresolved. The judge had scored every Signal; what failed was the step after
routing. The default Meshes ended in `Escalate`, or in Signal routes whose
weakest margin fell under the confidence policy (§37), and the account was
local-only at the time, so the escalation the routes asked for was refused.
Two thousand GitHub notifications sat behind a single `ELSE → Escalate`.

Three decisions follow:

- **A header is a fact, not a judgement.** GitHub states why it sent a
  notification in `X-GitHub-Reason`; mail synced before that header was kept
  says the same in its footer. `github_reason` is a deterministic fact
  (`riddle_sift::facts::github_reason`), the default GitHub Mesh routes on it
  (`review_requested`, `assign`, `mention`, `ci_activity`, and a Filed floor
  instead of `Escalate`), and the stored facts of older mail are filled once
  per Dig at boot (`App::backfill_github_reason`). The same shape applies to
  `gmail_category` and `list_unsubscribe` for promotions, social and update
  mail, and to sender domains for banks, crowdfunding and entertainment:
  when a fact says it, no Signal is asked, no margin can fail, and no
  escalation is needed.
- **The simulation applies Sift's confidence policy.** `ConfidencePolicy`
  in riddle-mesh holds the margins and the non-English rule; Sift's defaults
  come from it. A preview used to promise "1,289 to Spoil" where Sift would
  have held every one for escalation; now it says Unresolved, which is what
  the person would have got.
- **A match too close to call lets a later Mesh decide.** A route that
  matches on a Signal whose margin is short of the policy's for its
  disposition no longer ends the search (`evaluate_with`): the next Mesh gets
  its turn, and the thin match is the decision, held as before, only when
  nothing after it matches. The reason names what was passed over. Without
  it an early Signal rule stranded borderline mail a later rule would have
  decided with confidence: a promotions rule at priority 27 would have sent
  77 Spoil messages to Unresolved. It falls through to the next Mesh, never
  to its own Mesh's ELSE. Only a short margin falls through: the non-English hold does not,
  because its remedy is escalation (the refiner reads the message), not a
  later Mesh deciding on the same unreliable Signals. And falling through never
  hides: a borderline Find would have surfaced as Unresolved, so only a later
  Find or Filed may take it over; a later Spoil or Contamination is skipped,
  and if nothing else decides, the Find is held Unresolved as before.
- **Spoil earns its place; Filed may start Active.** A new Mesh that hides
  mail starts in Shadow and is activated by hand; one that only files can
  start Active, since nothing it decides leaves the Inbox view.

---

# 97. Final Principle

Most email clients ask:

> How can we help you manage your inbox?

Riddle asks:

> **Why are you managing your inbox at all?**

The Sift processes the material.

Jev makes fast semantic judgments.

Meshes turn those judgments into policy.

The LLM handles ambiguity and deeper reasoning.

The Director manages the excavation.

Actions provide controlled capabilities.

Activity records what happened.

Developer Mode lets us see behind the curtain.

Corrections make the system better.

Everything leaves a trail.

**You keep the Finds.**