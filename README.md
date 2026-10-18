# Riddle

> **An agent-native email client that sifts through the noise and surfaces what matters.**

**Status:** first build (v0.1.0, 2026-09-22): the Rust core, the Tauri desktop client, a CLI twin and an MCP server exist and run end to end on a synthetic Fixture Dig with Jev judging and Bedrock escalating. Gmail signs in through a Google OAuth client: one baked into the build (`RIDDLE_GOOGLE_CLIENT_ID` and `RIDDLE_GOOGLE_CLIENT_SECRET` at build time; the repository ships none) or your own, imported under Settings → Accounts; see `docs/GMAIL_SETUP.md`. See `ARCHITECTURE.md` for the technical design, `ARCHITECTURE-REVIEW.md` for the review, `APPLICATION_FEATURES.md` for the desktop conventions this project follows, `docs/GMAIL_SETUP.md` for connecting a real mailbox, `docs/BUILD_PLAN.md` for what was built and what is deferred, and `docs/BUILD_LEDGER.md` for what the build cost.

**Run it:** `cd apps/desktop && pnpm install && pnpm tauri dev` (the dev build uses its own data directory and starts unarmed). On macOS the dev binary is signed on launch with your first Developer ID or Apple Development identity (`scripts/run-signed.sh`, set `RIDDLE_CODESIGN_IDENTITY` to choose), so the keychain's "Always Allow" for the Gmail token survives rebuilds; without an identity it runs unsigned and asks after every rebuild. `tauri dev` runs a bare binary, so the Dock shows a generic icon there; for a double-clickable app with the real icon, `pnpm tauri build --debug --bundles app` writes `target/debug/bundle/macos/Riddle.app`. Headless: `cargo run -p riddle-cli -- serve` then open `http://127.0.0.1:1430/?bridge=<port>:<token>` from `pnpm dev`, or drive it with `scripts/bridge.py`. Offline, no spend: `riddle serve --offline`. Any command takes `--offline` to boot with the scripted judge, no model and no keychain, which is how to simulate or create Meshes from a script without a keychain prompt on a fresh build.

## Product Vision

Email has become less of a mailbox and more of an unstructured event stream.

Personal conversations, work, receipts, notifications, newsletters, account alerts, marketing, automated systems, and spam all arrive through the same channel and compete equally for attention.

Traditional email clients largely leave organization to the user. Folders, labels, filters, rules, searches, and increasingly AI features help—but the user is still ultimately responsible for managing the inbox.

**Riddle changes that model.**

Riddle is an agent-native email client built around continuous understanding and classification. It understands incoming mail, determines what it is, organizes it appropriately, identifies what deserves attention, and can actively do work on the user's behalf.

The goal isn't **Inbox Zero**.

The goal is **Attention Zero**:

> When Riddle says there's nothing you need to deal with, you should be able to trust it.

---

# The Archaeology Metaphor

Riddle's language comes from archaeological excavation.

An archaeologist doesn't treat every bucket of soil as equally important. Material is excavated, sifted, classified, placed into context, and separated from debris.

The objective is to uncover the small number of meaningful finds hidden within a much larger volume of material.

Email has the same problem.

Riddle turns the inbox into an excavation.

---

## Digs

A **Dig** is a connected email account.

A user might have:

- Personal
- Work
- Consulting
- Business
- Project-specific

Digs can operate independently while Riddle provides a unified view across all of them.

Each Dig develops its own understanding of the people, conversations, organizations, projects, and types of mail that pass through it.

Digs can also be grouped—for example, **Work**, **Personal**, or **Consulting**—allowing behaviour and Meshes to be shared across related accounts.

A Dig belongs to at most one Dig Group, so there is never a question of which group's behaviour wins.

Riddle can understand relationships across Digs when appropriate without requiring every Dig to behave identically.

A Work Dig on Google Workspace may need an administrator to allow Riddle before it can connect. When that happens, Riddle says so plainly—and says what to ask for—rather than failing quietly.

---

## Sift

**Sift** is the continuous process of understanding mail—both what arrives and what the user sends.

Sent mail matters as much as received mail. Knowing what the user asked for and what they promised is how Riddle knows who is waiting for whom, and that is where **Follow Up** Finds come from.

Messages are examined using signals such as:

- Sender and recipient
- Message content
- Conversation history
- Previous interactions
- Existing organization
- User behaviour
- Related messages
- Meshes
- Known people and projects
- Other contextual information

The Sift determines what a message is, where it belongs, whether it deserves attention, and what—if anything—should happen next.

Most mail should pass through the Sift without requiring the user's involvement.

---

## Mesh

**Meshes** describe what Riddle should look for and how particular kinds of mail should be handled.

They can range from simple deterministic rules:

> Messages from GitHub reporting a failed workflow → Engineering Alerts

to semantic instructions:

> Surface GitHub messages when I'm explicitly requested as a reviewer or when CI fails on one of my pull requests. Everything else can go to Spoil.

A Mesh is made of **routes** and **guards**.

A route decides where mail goes. A guard is a veto that runs after routing—"never classify messages from family as Spoil" is a guard, and no route can override it.

A Mesh can produce:

- A disposition—**Find** (with a type), **Filed**, **Spoil**, or **Contamination**
- A Stratum
- A notification
- Escalate—hand the message to deeper reasoning

Meshes may be explicitly created by the user or proposed by Riddle. When the user corrects the same kind of mistake repeatedly, Riddle proposes a Mesh change and applies it only with the user's approval.

A new or changed Mesh starts by **suggesting** what it would do. It earns the right to **act** on its own after a Shadow Period in which its suggestions match what the user actually does.

Every Mesh is versioned. Every change can be previewed against the mail already in the Dig before it takes effect, and every decision remembers which version made it.

A user should be able to create or modify a Mesh conversationally:

> "I don't care about these unless the build fails."

Riddle should understand what that means.

### Mesh Scope

A Mesh can operate at different scopes:

- **All Digs** — behaviour that should apply everywhere
- **Dig Group** — shared behaviour across related Digs
- **Individual Dig** — behaviour specific to one account

More specific behaviour can override broader behaviour when necessary.

For example:

> **All Digs:** Receipts → Purchases  
>
> **Work Digs:** GitHub CI failures → Find  
>
> **Personal Dig:** Never classify messages from family as Spoil

The Director should understand natural-language scope:

> "Do this for all my Digs."

> "Only do this at work."

> "Apply this everywhere except Personal."

The user should not have to duplicate the same Mesh across every account simply because multiple Digs happen to share the same behaviour.

---

## Strata

**Strata** represent meaningful categories and relationships within mail.

Rather than forcing every message into a traditional folder hierarchy, Riddle can organize messages into overlapping categories such as:

- People
- Projects
- Purchases
- Travel
- Finance
- Notifications
- Newsletters
- Receipts
- Engineering
- Needs Response

A message can belong to multiple Strata because meaning is rarely hierarchical.

Strata should emerge naturally from how the user actually works rather than requiring someone to design a filing system before it becomes useful.

Riddle proposes Strata as patterns appear. The user can rename, merge, and split them, and retire the ones that stopped being useful.

A Stratum belongs to a Dig Group or to all Digs, so a Stratum that makes sense at work does not leak into Personal.

---

## Context

**Context** is everything Riddle knows that helps it understand mail.

A message rarely makes sense in isolation.

Riddle should understand things such as:

- Who is this person?
- Have you communicated before?
- How important are their messages normally?
- Is this part of an ongoing conversation?
- Who is waiting for whom?
- Does it relate to an active project?
- Is there a deadline?
- Have similar messages mattered before?
- How have you previously handled mail from this sender?
- Does information elsewhere in your mail change the meaning of this message?

Context allows Riddle to reason about the meaning of mail rather than simply matching keywords.

---

# Finds

A **Find** is something worth surfacing.

Instead of asking:

> "Which emails haven't I read?"

Riddle asks:

> **"What did we find that deserves your attention?"**

A Find might represent:

- **Respond** — someone expects an answer
- **Decide** — a decision is required
- **Review** — something needs examination or approval
- **Do** — an action has been requested
- **Pay** — an invoice or payment requires attention
- **Attend** — an event or meeting matters
- **Follow Up** — a conversation has stalled
- **Know** — important information with no immediate action

A Find doesn't necessarily need to correspond one-to-one with an email.

Twenty messages about the same problem may represent **one Find**.

The same conversation arriving in two Digs—copied to both a work and a personal address—is one Find, not two.

A Find has a life of its own. It is **open**, **snoozed**, **resolved**, **dismissed**, or **expired**.

A Find resolves itself when the user deals with it, including a reply sent from a phone or from Gmail on the web. Riddle should not ask the user to close what they already handled.

Dated Finds—**Attend**, **Pay**—expire when their moment passes.

**Follow Up** Finds are made by time, not by mail: a conversation with no reply after long enough becomes a Find without any new message arriving.

The purpose of Riddle is ultimately to uncover Finds—not maximize the number of messages the user processes.

---

# Unresolved

Sometimes Riddle cannot decide.

A message may be new, ambiguous, in a language Riddle handles poorly, or simply unlike anything it has seen before.

That mail is **Unresolved**.

Unresolved mail is not hidden, and it is not inflated into a Find to be safe. It has its own tray and its own count in the morning summary, so the user can see exactly what Riddle was unsure about and decide in seconds.

Attention Zero includes Unresolved. When Riddle says there is nothing to deal with, it means there are no Finds *and* nothing Unresolved.

Correcting Unresolved mail is the fastest way to teach Riddle.

---

# Spoil

**Spoil** is legitimate mail that has little or no immediate value.

Examples might include:

- Marketing
- Routine notifications
- Automated reports
- Low-value newsletters
- Social notifications
- Repetitive system messages

Spoil isn't deleted.

It remains searchable, recoverable, and available when needed. It simply stops competing for attention.

Riddle can summarize Spoil rather than presenting every message individually:

> **238 messages moved to Spoil**
>
> 82 promotions  
> 61 automated notifications  
> 48 newsletters  
> 31 social notifications  
> 16 miscellaneous

The user can always ask:

> "Was there anything interesting in my Spoil this week?"

Riddle sends a **Spoil digest** on a schedule the user chooses—daily or weekly—so a misfiled message is caught within a day rather than a month.

Spoil is also where subscriptions go to be noticed. When the user never opens a sender's mail, Riddle offers **Unsubscribe** as a first-class action, and the user can say "never again" for a sender without touching a filter.

Some mail can never become Spoil no matter what a Mesh says. See **Never Hidden**.

---

# Contamination

**Contamination** is mail that should not be trusted.

This includes:

- Spam
- Phishing
- Scams
- Suspicious senders
- Malicious content
- Spoofing or impersonation attempts

Contamination is deliberately distinct from Spoil.

**Unimportant** and **untrustworthy** are fundamentally different classifications.

Contamination is decided by evidence before judgment: Gmail's own spam verdict, whether the message passed authentication, whether the sender has ever been seen, whether links and reply addresses match who the message claims to be.

The Director does not read Contamination. Untrusted mail stays out of the Director's context unless the user opens it deliberately.

Mail that tries to talk its way into attention—"URGENT: respond immediately"—is treated as a signal against it, not for it. Riddle pays attention to what a message *is*, not to what it says it is.

---

# The Director

Riddle isn't simply an email client with an AI chatbot attached.

At the center of Riddle is the **Director**.

The Director is the user's agent within Riddle: responsible for understanding the excavation as a whole, helping determine what matters, investigating when asked, and carrying out work on the user's behalf.

The Director doesn't need to inspect every piece of mail manually. The Sift continuously processes incoming material.

Instead, the Director operates at a higher level.

The user might ask:

> "What actually needs my attention this morning?"

> "Did anything important arrive while I was away?"

> "Why did you think this was a Find?"

> "Find the hotel reservation from my Portugal trip."

> "What am I waiting for other people to respond to?"

> "Which subscriptions am I paying for that I don't seem to use?"

> "Draft replies to everything that's waiting on me."

> "These GitHub notifications aren't useful unless something fails. Handle those automatically from now on."

The Director can investigate, organize, explain, recommend actions, and—when authorized—perform those actions.

Before any action that leaves the machine—send, reply, forward—the Director asks, and shows the user the real recipients and the real message rather than a description of them.

When the Director has read untrusted mail during a task, it cannot send, forward, or create a Mesh in that task without the user's approval, and the control says why:

> **Send disabled** — the Director read untrusted content in this task.

The Director should feel like part of Riddle itself rather than a separate AI experience.

There should be no concept of an "AI version" of the user's mailbox.

The Director and the user work with the **same Digs, Finds, Strata, Meshes, drafts, messages, and state**.

---

# Expeditions

Some questions require more than finding a single message.

An **Expedition** is a task the Director undertakes across the user's mail.

For example:

> "Find every subscription I've paid for during the last year and identify the ones I may no longer need."

Or:

> "Go through everything related to the house renovation and tell me what we're still waiting for."

Or:

> "Find all of my travel reservations for Portugal and organize them."

An Expedition may involve searching across Digs, examining many conversations, connecting related information, and producing one or more Finds.

An Expedition has a budget—in cost, in time, and in how much mail it may read—and says when it is approaching it. It shows its progress, can be cancelled, and picks up where it left off if Riddle is closed in the middle.

This gives Riddle a natural distinction between **searching for mail** and **delegating work involving mail**.

---

# Ask Riddle

Traditional search remains important.

Users should always be able to find messages using familiar sender, date, keyword, label, and other exact searches.

But Riddle should also support questions where the user knows what they're looking for without knowing how to construct the query.

For example:

> "Where's that receipt for the access point?"

> "Find the conversation where Adam told me what was wrong with the Jeep brakes."

> "What commitments have I made over email this month?"

> "What invoices haven't been dealt with?"

> "What did we decide about the project deadline?"

> "Show me everything related to Riddle from the last two weeks."

The user shouldn't need to remember which Dig, sender, subject line, or keyword contains the answer.

---

# The Core Experience

Opening Riddle shouldn't feel like opening an inbox containing 847 unread messages.

Instead:

> **Good morning.**
>
> Caught up as of 7:42.
>
> 312 messages arrived across your 3 Digs.
>
> **7 Finds need your attention.**
>
> **2 are Unresolved.**
>
> 41 were Filed into your Strata.
>
> 236 went to Spoil.
>
> 26 were Contamination.
>
> **Director:** Two of the Finds are probably worth handling before your first meeting.

The summary says whether Riddle has finished. Riddle processes mail while the machine is awake, so opening it after a night or a week away means it is **Catching up**—and it says so:

> **Catching up.**
>
> 4 Finds so far. 1,240 of 4,981 messages still to sift.

A count that implies completion while Riddle is still working would be a lie. Attention Zero cannot be promised for mail Riddle has not yet seen.

The **Task Centre** is always one click away: a status for each Dig—**Synced**, **Catching up**, **Sifting**, or **Degraded**—and a list of background work such as initial sync, catch-up, historical replay, Mesh simulation, and Expeditions, each with progress and a cancel button.

The user deals with the things that matter.

Everything else remains available without demanding attention.

The traditional chronological inbox still exists when the user wants it.

It simply isn't the center of the product.

---

# Agent-Native by Design

Riddle treats direct interaction and delegated interaction as equally important ways of using the product.

Anything the user can reasonably do manually should also be something they can ask the Director to do.

A user might manually:

- Find a message
- Apply a Stratum
- Archive mail
- Create a Mesh
- Draft a response
- Review Spoil

Or simply say:

> "Take all of these and treat messages like them as Spoil from now on."

Likewise, when the Director does something, the result shouldn't disappear into an AI conversation.

If the Director drafts an email, it's a normal Riddle draft.

If it creates a Mesh, that Mesh is visible and editable.

If it produces a Find, that Find appears alongside other Finds.

If it organizes mail into Strata, the user can inspect and change those classifications.

**The UI and the Director are two ways of interacting with the same Riddle.**

---

# Activity & Provenance

Riddle should provide a complete, human-readable history of meaningful activity.

The user should always be able to answer:

> **What happened?**

> **Who did it?**

> **Why did it happen?**

> **What information or Mesh caused the decision?**

> **Can I undo it?**

Activity should include actions performed by:

- The user
- The Director
- The Sift
- Meshes
- Automated processes

Rather than hiding AI activity behind a generic "AI organized this" indicator, Riddle should expose the provenance of important decisions.

For example:

> **10:42 AM — Sift moved 14 messages to Spoil**
>
> Triggered by **Mesh: Routine GitHub Notifications** v7
>
> 13 matched the Mesh on cached Signals.
>
> 1 needed a fresh evaluation.
>
> **View Messages · View Reasoning · Edit Mesh · Undo**

Or:

> **11:07 AM — You changed "AWS Billing Alert" from Spoil to Find**
>
> Riddle proposed an update to **Mesh: AWS Notifications** based on your correction.
>
> **Preview · Apply · Undo**

Activity should be filterable by Dig, actor, action type, Mesh, and time.

The objective isn't merely an audit log.

It is **full traceability into what Riddle and its intelligence are doing on the user's behalf.**

---

# Undo

Automation becomes much easier to trust when it is reversible.

Where technically possible, meaningful actions performed by either the user or Director should support **Undo**.

This includes actions such as:

- Archive
- Move to Spoil
- Apply or remove Strata
- Change Find status
- Bulk organization
- Mesh creation or modification
- Classification changes

Undo should restore both the affected mail and Riddle's internal understanding when appropriate.

For bulk operations:

> **Director moved 127 messages to Spoil.**
>
> **Undo All**

The user shouldn't have to manually locate and restore 127 messages because an automated decision was wrong.

Actions that cannot safely be undone should be clearly distinguished before execution.

Undo is honest about what it can and cannot do.

Undoing an action also retracts what Riddle learned from it. A correction that was itself a mistake should not keep teaching.

**Undo All** skips messages the user has since handled by hand, and says which ones it skipped.

When Riddle has written its decisions into Gmail as labels or archive state (Projection), undo restores Gmail too—not just Riddle's view of it.

Sending cannot be undone. Riddle says so before the user sends, not after.

---

# Mesh Feedback & Repair

When automation behaves unexpectedly, correcting the individual message isn't enough.

The user should be able to investigate **why** it happened and correct the underlying behaviour.

For example:

> **Why did this go to Spoil?**
>
> Mesh **GitHub Notifications** classified it as a routine GitHub notification.
>
> **Correct Message · Edit Mesh · Disable Mesh**

The user might then say:

> "PR review requests shouldn't go to Spoil."

Riddle can propose a change:

> **Update GitHub Notifications**
>
> Current:
>
> `Routine GitHub notifications → Spoil`
>
> Proposed:
>
> `Routine GitHub notifications → Spoil`
>
> `Except review requests addressed to you → Find: Review`
>
> This change would have affected **17 messages in the last 90 days**.
>
> **Preview · Apply**

Whenever practical, Riddle should allow the user to **preview the historical impact of a Mesh change before applying it**.

This turns an unexpected AI decision into an opportunity to improve the system rather than simply correct one mistake.

---

# The Shadow Period

Riddle does not start hiding mail on day one.

When a Dig is connected, Riddle syncs recent mail first and watches.

During the **Shadow Period**—the first seven days, or until the user has reviewed what Riddle proposes—Riddle records what it *would* have done with every message and shows it. The inbox is untouched.

> **Shadow Period — day 3**
>
> Riddle would have moved 412 messages to Spoil and surfaced 19 Finds.
>
> You disagreed with 6 of its choices.
>
> **Review Proposals**

A Mesh starts acting on its own only once its proposals match what the user actually does. Autonomy is earned per Mesh, not switched on for the whole Dig.

New and changed Meshes go through the same period.

---

# Never Hidden

Some mail cannot become Spoil or Contamination, whatever a Mesh or a Signal says:

- People the user has written to
- The user's contacts
- People at the user's own organisation
- Anyone the user has marked as important
- Security and account alerts
- Financial and legal mail
- Calendar invitations and changes
- Mail Riddle is unsure about

These rules are deterministic. They run after every Mesh and win every time.

The user can edit the list. Riddle cannot.

---

# Trust Is the Product

Riddle only works if users trust the Sift and the Director.

Trust comes from more than model accuracy.

It comes from:

**Visibility.** The user can see what happened.

**Provenance.** Riddle can explain what caused it.

**Control.** The user can change the underlying behaviour.

**Reversibility.** Mistakes can be undone.

**Learning.** Corrections improve future behaviour.

A user should never have to wonder:

> "What the hell did the AI just do to my email?"

Riddle should be able to show them.

---

# Privacy

The user's mail stays on the user's machine.

Classification may use a cloud judgment model, and deeper reasoning may use a cloud LLM. Riddle states exactly what leaves the device, to whom, and why, in one place the user can read.

Each Dig can be excluded from cloud AI. A Work Dig can be local-only: deterministic rules and on-device processing, nothing else.

Nothing in the user's mail is used to train anyone's models—not Riddle's, not a provider's.

Riddle uses Gmail under Google's Limited Use rules and says so in the app.

Developer Mode shows more, but exports and logs are redacted by default and never contain credentials.

---

# Keyboard, Help and Empty States

Triage is keyboard-first. Moving through Finds, filing, spoiling, snoozing, and undoing should not need the mouse.

One table drives both the shortcuts and the help sheet, so a shortcut cannot exist without being documented. `?` opens the sheet.

Three pull tips introduce Finds, Unresolved and the Director, each once, on the first visit to that page, and never again unless asked from Settings. Nothing tours an empty page: before an account is connected the Summary offers to connect Gmail or to start with sample mail.

Every empty state is an instruction or a proof. A new Dig says what to do next. The Attention Zero screen shows its evidence:

> **Nothing needs your attention.**
>
> Synced 2 minutes ago. 41 messages Filed, 236 in Spoil since yesterday.
>
> **Review Spoil · Undo**

A disabled control always says why.

---

# Scope

Riddle v1 is:

- **Gmail only.** Other providers may come later; the mail model will not be reduced to their lowest common denominator to get there.
- **macOS desktop.** Riddle processes mail while the machine is awake. Nothing is processed while it sleeps, and Riddle says so.
- **One machine.** Riddle's understanding—Finds, Strata, Activity, Context—lives on that machine. Meshes can be exported and carried to another.
- **An email client first.** Compose with attachments, reply and forward with quoting, threads, signatures, send-as aliases, contacts autocomplete, calendar invitations, search, keyboard shortcuts, offline reading, an undo-send window, snooze, and print are table stakes, not extras.

A Work account on Google Workspace may be blocked by its administrator. Riddle tells the user that plainly, with what to ask the administrator for.

Not planned for v1:

- Other mail providers
- Mobile
- Multi-device sync of Riddle's understanding
- Shared team Meshes

---

# Product Principles

## Attention, not unread counts

Riddle optimizes for what deserves attention rather than how many messages have been marked as read.

## Understand before presenting

The client should understand mail before deciding how prominently to display it.

## Context over keywords

The relationship between the user, sender, conversation, history, and surrounding activity is more meaningful than keyword matching alone.

## Delegate, don't just search

Sometimes the user wants an email.

Sometimes the user wants something **done**.

Riddle should understand the difference.

## AI is part of the product, not a destination

Users shouldn't need to open a special AI mode to benefit from intelligence.

The Sift, Director, Meshes, Finds, Context, search, and organization should all benefit from it naturally.

## Explainable automation

Users should be able to understand why Riddle made an important decision and what caused it.

## Correct the system, not just the symptom

When automation behaves incorrectly, users should be able to fix the underlying Mesh or behaviour—not merely move one email back.

## Corrections are learning

Fixing a classification should improve future classifications rather than simply moving one message.

## Automation earns autonomy

Riddle should earn the right to act through predictable behaviour, transparency, reversibility, and user control.

## Everything leaves a trail

Meaningful actions by users, the Director, Meshes, and automation should be attributable and inspectable.

## Reversible by default

Wherever possible, actions should be designed so they can be safely undone.

## Nothing disappears

Aggressive classification is only useful when mistakes are recoverable.

Spoil, Strata, Finds, and other classifications remain searchable and inspectable.

## Bookkeeping never fails the work

Logging, cost tracking, and telemetry degrade silently. They never block the Sift or the Director.

## Honest status

Progress and status come from real events. Nothing animates while nothing happens. A disabled control says why.

## Nothing changes silently on update

When an update changes a default Mesh or Signal, Riddle previews the effect on the user's mail and asks before applying it. This is the Update Preview.

## One Riddle

The user and Director work with the same information and the same objects.

There is no separate "AI mailbox."

---

# The Product Principle

Most email clients ask:

> **How can we help you manage your inbox?**

Riddle asks:

> **Why are you managing your inbox at all?**

Humans should spend their time communicating, understanding, and making decisions—not sorting messages produced by other humans and machines.

The Sift processes the material.

The Director manages the excavation.

Everything leaves a trail.

**You keep the Finds.**