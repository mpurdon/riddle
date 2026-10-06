# Triage, inbox and notification UX in communication and work-tracking clients

Research date: 2026-09-25. Method: web search to locate primary sources, then direct fetches of vendor help centres, changelogs, design blogs, developer docs, one peer-reviewed paper (PDF) and named reviews. Anything not confirmed by a fetched page is marked **[unverified]**. Blocked hosts (HTTP 403 or fetch refused): support.discord.com, help.superhuman.com, resources.arc.net, theverge.com, bbc.com, web.archive.org, medium.com, academia.edu, dl.acm.org. For those, official blogs, an official PDF, Wikipedia and named third-party guides are used and labelled as such.

Quotes are kept under 15 words.

---

## Part A. Per-client records

Each record: (a) mental model, (b) prioritisation and "why", (c) tuning, (d) keyboard,
(e) dismiss/done and reversibility, (f) threading, (g) praise/complaints, (h) sources.

### 1. Slack

(a) Three parallel inboxes: **Unreads** (conversations grouped, one preview each), **Activity** (a feed of notifications: DMs, mentions, thread replies, reactions, app notifications, reminders, invitations, VIP items) and **Later** (self-saved items with In progress / Archived / Completed tabs). Mobile adds **Catch up**, a one-card-at-a-time pass over unread conversations (Jan 2024; AI summary by pinch, Aug 2025).

(b) No explicit ranking inside Activity; the Unreads view can be sorted by "recommended order", newest or oldest activity, sidebar order or alphabetically. The only stated priority mechanism is **VIPs** (paid): up to 300 people, apps and workflows whose DMs and mentions appear in a "VIP unreads" sidebar section and can bypass Do Not Disturb. Activity does show the *type* of each item (mention, reaction, thread reply) and is filterable by type, which is the closest thing to a "why".

(c) Global default "Everything" vs "Mentions and direct messages" with per-channel exceptions; keyword alerts (exact match, case-insensitive, and **not** matched inside threads); a notification schedule (every day / weekdays / custom hours); a mobile delay (1 minute after locking the desktop, 10 minutes after cursor inactivity); toggles for thread-reply notifications, huddle starts and whether thread replies count in the badge. Dec 2025 simplified preferences to "all new messages" vs "only mentions". Custom Activity views are saveable filter combinations shown as tabs (desktop; channel filters on paid plans).

(d) Cmd+K jump; Cmd+Shift+A Unreads; Cmd+Shift+T Threads; Cmd+Shift+M Activity; Esc mark conversation read; Shift+Esc mark all read; Option+click mark unread; Option+Shift+Up/Down previous/next unread; Cmd+/ lists all shortcuts. In Unreads, arrows move between messages and Left/Right collapse or expand a conversation group.

(e) Mark read is per conversation or "Mark All Messages Read" with an **Undo** button that appears immediately afterwards. Activity items can be cleared one at a time or in bulk and remain visible under a "Cleared notifications" filter. Later items can be marked Complete or Archived and remain browsable. Catch up on mobile: swipe right marks read, swipe left keeps unread.

(f) Threads are one level deep and live in a right-hand panel; the channel shows only a reply-count affordance. Slack's designer records that hiding replies from the channel was "the single most meaningful change" and that unlimited nesting made the design "extremely complex". An "also send to channel" release valve exists because hiding replies hurt a minority. A dedicated **All Threads** view was modelled on All Unreads.

(g) Praise: the Unreads/Threads views are widely used as a keyboard triage loop (Esc/Shift+Esc). Complaints: broadcast replies appear twice in the channel (case study); Doist claims Slack users stay connected "9+ hours per workday" and that presence and dots drive always-on pressure; VIPs and custom Activity filters are paid-only.

(h) Activity: https://slack.com/help/articles/19693583638803-Triage-notifications-in-the-Activity-tab · Unreads and Catch up: https://slack.com/help/articles/226410907-View-all-your-unread-messages · Catch up gestures: https://slack.com/help/articles/201457107-Send-and-read-messages · VIPs: https://slack.com/help/articles/34963579361683-Add-contacts-as-VIPs · Preferences: https://slack.com/help/articles/201355156-Configure-your-Slack-notifications · Later: https://slack.com/help/articles/360042650274-Save-messages-and-files-for-later · Shortcuts: https://slack.com/help/articles/201374536-Slack-keyboard-shortcuts · Release history: https://slack.com/help/articles/115004846068-Slack-updates-and-changes · Threads design: https://slack.design/articles/threads-in-slack-a-long-design-journey-part-2-of-2/ · Case study: https://raw.githubusercontent.com/hatefrad/product-engineer/main/case-studies/slack-threads.md

### 2. Discord

Official help-centre pages returned 403 and Wayback was blocked; the notification and Inbox details below come from search snippets of those pages plus third-party guides and are marked accordingly. The threads material is from Discord's own blog.

(a) The unit is the server; each server has a notification level and each channel can override it. An **Inbox** (Ctrl+I) collects Mentions and Unreads across servers, with a "For You" tab [official Inbox FAQ via snippet; unverified].

(b) No ranking. The Inbox distinguishes direct @mentions from @here/@everyone; @here is listed under Unreads rather than Mentions [snippet; unverified]. The item type (mention vs unread) is the "why".

(c) Per server: All Messages / Only @mentions / Nothing; mute for 15 min, 1 h, 8 h, 24 h or until re-enabled; suppress @everyone and @here; suppress role mentions; per-channel overrides; mobile quiet hours [discords.ai guide; official page unverified]. Private threads reach only people @mentioned or added (Discord blog).

(d) Ctrl+K quick switcher; Ctrl+I inbox; Ctrl+/ shortcut list; Alt+Up/Down channels; Alt+Shift+Up/Down unread channels only; Esc marks channel read; Shift+Esc marks server read [defkey.com, third-party].

(e) Marking read is by channel or whole server; there is no "done" state separate from read. Muting is time-boxed, which is a form of reversible snooze for a source.

(f) Threads open in a split-view panel on the right and also appear in the channel list; they auto-archive after 24 hours of inactivity into an Archived tab and can be reopened (Discord blog, July 2021). Wikipedia describes them as temporary channels that "disappear".

(g) Community feature-request titles ask for an all-unreads view and call the Unreads tab "a mess" [titles only; bodies unverified]. Praise: the three-level per-source model is simple and copied by other tools.

(h) Threads: https://discord.com/blog/connect-the-conversation-with-threads-on-discord · Notification levels (third-party): https://www.discords.ai/wiki/discord-notifications-management · Shortcuts (third-party): https://defkey.com/discord-shortcuts · Official (403, titles only): https://support.discord.com/hc/en-us/articles/215253258-Notifications-Settings-101 , https://support.discord.com/hc/en-us/articles/360045027712-Inbox-FAQ · Wikipedia: https://en.wikipedia.org/wiki/Discord

### 3. Microsoft Teams

(a) A single **Activity feed** of typed events: chat and channel @mentions, replies, reactions, meeting invites and cancellations, membership changes, trending and suggested posts. Items expire after 30 days.

(b) No ranking. Every row carries a type icon (@, team, channel, reply, like, crown, trending, suggested, calendar), so the reason is visible per item. Filters were reduced in 2024 to two pills, **@mentions** and **Unread**, plus a free-text filter (Ctrl+Shift+F); replies, reactions, apps and voicemail filters were retired.

(c) Per event type: Banner and feed / Show only in feed / Off. Per channel: All activity / Mentions & replies / Off, or custom. Missed-activity email frequency is selectable. Urgent messages and "priority contacts" can have distinct sounds; a Windows option mutes everything except calls and meetings.

(d) Ctrl+1 Activity; Ctrl+E search; Ctrl+Shift+F filter; Ctrl+Alt+K mark all read; Ctrl+Alt+U unread; Ctrl+Alt+M @mentions; Ctrl+Alt+C missed calls; Ctrl+. opens the shortcut list (customisable).

(e) Read/unread only, plus "mark all as read". No done or saved state; items age out.

(f) Channel posts have flat replies under the post; the feed links to the reply.

(g) Complaint (Tony Redmond): calendar notifications cluttered the feed because Outlook already alerts, and Microsoft's rationale for removing filters was unclear. Praise: per-type icons and the Banner/feed/Off matrix are easy to reason about.

(h) Feed: https://support.microsoft.com/en-us/teams/notifications-settings/explore-the-activity-feed-in-microsoft-teams · Settings: https://support.microsoft.com/en-us/office/manage-notifications-in-microsoft-teams-1cc31834-5fe5-412b-8edb-43fecc78413d · Shortcuts: https://support.microsoft.com/en-us/office/keyboard-shortcuts-for-microsoft-teams-2e8e2a70-e8d8-4a19-949b-4c36dd5292d2 · 2024 changes: https://office365itpros.com/2024/05/29/teams-activity-feed-changes/

### 4. Linear

(a) Two queues. **Inbox** is personal: notifications for issues you created, are assigned to, are mentioned in, or subscribe to. **Triage** is a team inbox for issues arriving from integrations, other teams, or created directly in Triage; triage issues are excluded from normal views until accepted.

(b) Inbox has a **Priority** tab that "separates notifications that need your attention" from other updates, and a "show unread first" option. Each notification names its type (assigned, mentioned, status change), which is the "why". Triage Intelligence (Business/Enterprise) suggests assignee and labels and surfaces likely duplicates.

(c) Channels: desktop, mobile, email, Slack, each with a green/grey enabled dot. Email defaults to digests "grouped ... based on urgency" and is only sent if the Inbox item is still unread. Per-issue subscribe (Shift+S) and unsubscribe (Cmd+Shift+S). Docs state you cannot customise which notification types appear in Inbox, there is no archive, and at most 2,000 open notifications are kept.

(d) Inbox: J/K or arrows; U read/unread; Alt+U all read; Backspace delete; Shift+Backspace delete all read; H snooze; G then I. Triage: 1 accept, 2 duplicate (or M M), 3 decline, H snooze, G then T. Global: Cmd+K command menu, ? shortcut sheet, C new issue, G+letter navigation, A/S/P/L set assignee/status/priority/label, X select. Cmd/Ctrl+F filters the Inbox by title, id, type, assignee, team, project, priority.

(e) Snooze hides until a chosen time **or new activity, whichever first**. Accept moves to the team's default status; decline sets Canceled with an optional explanatory comment; duplicate merges attachments and customer requests into the canonical issue and cancels the duplicate (merge only works from the duplicate side). Inbox items are deleted rather than archived. Undo for deletes is not documented [unverified].

(f) Issue comments are flat with reply-threads inside the issue; Inbox rows group by issue.

(g) Praise: speed. A technical teardown attributes it to an IndexedDB local database, optimistic writes reconciled by a sync engine, per-field MobX observables, modulepreload batching, a service worker precaching about 1,200 assets, an inlined app shell, and animations limited to transform/opacity under 150 ms (author disclaims insider access; percentages are Linear's own). Lenny's Newsletter describes triage as "an inbox for your team" with a rotating goalie engineer. Complaints from docs: no archive, 2,000-item cap, fixed notification types. Reviewer complaints not fetched [unverified].

(h) Inbox: https://linear.app/docs/inbox · Triage: https://linear.app/docs/triage · Notifications: https://linear.app/docs/notifications · Snooze/merge: https://linear.app/changelog/2021-06-17-inbox-snooze-and-easier-issue-merge · Triage launch: https://linear.app/changelog/2021-06-29-linear-release-and-issue-triage · Method: https://linear.app/method/introduction · Shortcuts (third-party): https://keycombiner.com/collections/linear/ · Speed: https://performance.dev/how-is-linear-so-fast-a-technical-breakdown · Lenny's: https://www.lennysnewsletter.com/p/how-linear-builds-product

### 5. GitHub Notifications inbox

(a) A subscription-driven inbox at github.com/notifications, grouped by repository or date, with sidebar filters (Assigned, Participating, Mentioned, Review requested) and up to 15 custom filters written in a query language.

(b) Every row carries a **reason label** (mention, subscribed, review requested and so on) and the full reason vocabulary is filterable: assign, author, comment, participating, invitation, manual, mention, review-requested, security-alert, state-change, team-mention, ci-activity. The same reason ships in email as an X-GitHub-Reason header so mail clients can route on it. This is the most explicit "why you received this" in the set.

(c) Per repository: All activity / Participating and @mentions / Ignore / Custom (issues, PRs, releases, discussions, security alerts). Delivery to web, email, mobile in any combination; per-organisation email routing.

(d) e Done; Shift+I read; Shift+U unread; Shift+M unsubscribe; ? opens the page's shortcut dialog. Limits: no full-text search, no NOT/exclusion operators, cannot distinguish is:issue from is:pr, default filters cannot be edited.

(e) Three exits with different strengths. **Done** removes from inbox but stays under is:done for 3 months. **Saved** keeps indefinitely. **Unsubscribe** stops notifications until you are @mentioned, your team is mentioned, or a review is requested; the item disappears. Unsaved notifications expire after 3 months.

(f) Issue/PR conversations are flat with quoted replies; the inbox links to the latest comment.

(g) Ben Balter: triage in a blocked-off first hour, "watch liberally, unsubscribe often", route by reason with mail filters, highlight @mentions; complaint that GitHub's own search "is not always perfect" and the setup feels overengineered.

(h) Inbox: https://docs.github.com/en/subscriptions-and-notifications/how-tos/viewing-and-triaging-notifications/managing-notifications-from-your-inbox · Filters and reasons: https://docs.github.com/en/subscriptions-and-notifications/reference/inbox-filters · Concepts: https://docs.github.com/en/subscriptions-and-notifications/concepts/about-notifications · Configuring: https://docs.github.com/en/subscriptions-and-notifications/get-started/configuring-notifications · Shortcuts: https://docs.github.com/en/get-started/accessibility/keyboard-shortcuts · Workflow: https://ben.balter.com/2020/08/25/how-i-manage-github-notifications/

### 6. Things 3 (Cultured Code)

(a) Capture to **Inbox** ("temporary staging ground"), then process into Today (with a This Evening sub-section), Upcoming (next seven days emphasised), Anytime, Someday, or Logbook. Today is the daily commitment; Someday is deliberately visually recessive.

(b) No automatic ranking. Guidance is a single on/off "Important" tag and manual drag order in Today; deadlines and start dates pull items into Today automatically.

(c) Not a notification product; the tuning is which list an item sits in. Cultured Code suggests reviewing Today each morning and Someday every couple of months.

(d) Quick Entry Ctrl+Space from any app; Quick Entry with Autofill Ctrl+Opt+Space (pulls the current Mail message or Safari page in); Cmd+1..6 lists; Cmd+T Today, Cmd+E Evening, Cmd+R Anytime, Cmd+O Someday, Cmd+S When; Shift+Cmd+D deadline; Cmd+K complete; Opt+Cmd+K cancel; Shift+Cmd+M move; Cmd+F Quick Find, which also navigates by typing a list name ("Type Travel"). Help → Keyboard Shortcuts opens the sheet.

(e) Complete and Cancel both go to the Logbook, which is a browsable permanent history.

(f) Not applicable; checklists inside to-dos only.

(g) Third-party guides praise the daily Inbox-to-zero ritual [Vanja Petreski, unverified detail]; the natural-language dates and Type Travel are marketed as the speed features.

(h) Lists: https://culturedcode.com/things/support/articles/4001304/ · Shortcuts: https://culturedcode.com/things/support/articles/2785159/ · Prioritising: https://culturedcode.com/things/support/articles/3289315/ · Quick Find: https://culturedcode.com/things/support/articles/2803584/ · Features: https://culturedcode.com/things/features/

### 7. Todoist

(a) Inbox is the "default place for ideas or tasks until they're added to a project"; Today and Upcoming are the working views; Filters are saved searches.

(b) Manual p1..p4 priorities typed inline; no automatic ranking.

(c) Reminders (automatic, custom, recurring, location); filter queries such as "(today | overdue) & #Work" to make review views.

(d) q opens Quick Add anywhere in the app; Ctrl+Cmd+A opens it from any Mac app; syntax parses dates ("next Sunday at 9am"), #project, @label, p1, and assignees.

(e) Completing a recurring task advances it; Karma gamifies completion streaks.

(f) Not applicable.

(g) Reviewer praise/complaints not fetched [unverified].

(h) Guide: https://www.todoist.com/inspiration/how-to-use-todoist-effectively · Glossary: https://www.todoist.com/help/articles/todoist-glossary-cA60laWMH · Hidden features: https://www.todoist.com/inspiration/hidden-features-todoist

### 8. Superlist

(a) Inbox captures everything; the user distributes items to lists or Today. One reviewer called this a "little morning ritual".

(b) Manual. (c) Not a notification product. (d) A global quick-launcher shortcut (Option+Shift+Space) that the reviewer found conflicts with typing a non-breaking space and cannot be changed. (e) Bulk multiselect actions arrived in 1.56/1.57: move, add or remove from Inbox, duplicate, delete, drag as a group. (f) Notes nest inside tasks.

(g) Praise: sync, nested notes-in-tasks, task filters. Complaints (part 2): clunky editor, awkward task creation, un-customisable shortcut, many small rough edges; reviewer returned to Google Tasks.

(h) Updates: https://www.superlist.com/updates · Review 1: https://johannesippen.com/2024/superlist-test/ · Review 2: https://johannesippen.com/2024/superlist-test-2/

### 9. Twist (Doist)

(a) Channels hold titled **threads**; the personal **Inbox** shows what you missed in threads you follow; marking a thread Done moves it to a Done view. A 2021 redesign moved from three columns to two so the open thread fills the screen.

(b) No ranking; the inbox is your own follow list. (c) No presence indicators and no "stress-inducing" dots by design; Doist frames presence as "hijacking attention" and runs a 70/25/5 async/sync/in-person model. (d) Keyboard shortcuts for thread navigation are advertised; specifics not fetched [unverified]. (e) Done is a per-thread state and the thread can be reopened from the Done view. (f) One level: thread title plus flat comments; no nested replies. (g) TechCrunch: "an opinionated tool" that helps focus; implicit complaint is that it rejects real-time chat entirely.

(h) https://twist.com/ · https://techcrunch.com/2021/10/12/doist-redesigns-twist-the-slack-alternative-focused-on-async-work/ · https://async.twist.com/asynchronous-communication/

### 10. Arc browser: Little Arc and the sidebar as tab triage

Help-centre pages returned 403; figures below come from search snippets of those pages plus Wikipedia.

(a) The sidebar separates pinned tabs (kept) from today tabs (ephemeral). **Little Arc** is a small window that opens links from other apps; you read and close, or promote the page into the sidebar. Unpinned tabs auto-archive on a cadence (default 12 hours; Little Arc windows every 6 hours) [snippets; unverified]. Viewing a tab resets its timer.

(b) Recency is the ranking: what you have not touched leaves. (c) Cadence per user; Spaces and "Air Traffic Control" route links to a Space. (d) Cmd+T Command Bar for tabs, history and commands. (e) Archive is browsable and restorable from the Library; nothing is deleted. (f) Not applicable. (g) The Verge (via Wikipedia): "right about most of them" but adjustment is needed. Arc went into maintenance mode in May 2025 and The Browser Company was acquired by Atlassian in September 2025.

(h) Wikipedia: https://en.wikipedia.org/wiki/Arc_(web_browser) · Official (403): https://resources.arc.net/hc/en-us/articles/19235387524503-Little-Arc-Quick-Lookups-Instant-Triaging , https://resources.arc.net/hc/en-us/articles/19228855311127-Auto-Archive-Clean-as-you-go

### 11. Notion Inbox

(a) An Inbox at the top of the sidebar, grouped by page and by comment thread, fed by @mentions, replies to your comments, pages set to "All comments", Person-property assignments, reminders and invitations.

(b) No ranking in the help page; Sept 2026 release notes say Notion Agent can group notifications and "show what needs attention". A "Prioritize with AI" button was reported in search snippets only [unverified].

(c) Per page on hover: All updates / All comments / Replies & mentions. Push is suppressed while the app is open; email goes out after five minutes of inactivity unless "Always send" is set; Slack delivery is optional.

(d) Not documented in the fetched pages [unverified]. (e) Mark read/unread, Archive, "Archive read", "Archive all"; an Archived filter keeps them reachable. Mobile: swipe right read, swipe left archive. (f) Comment threads per block, flat within a thread.

(g) Not fetched [unverified].

(h) https://www.notion.com/help/updates-and-notifications · https://www.notion.com/help/notification-settings · https://www.notion.com/releases

### 12. Apple: Focus, Scheduled Summary, Apple Intelligence summaries and Priority

(a) Notifications are a stream with per-app delivery policy; **Focus** modes are named contexts that allow-list people and apps and silence everything else into Notification Center; **Scheduled Summary** batches low-priority apps into set times (iOS 15; link present on the support page, details not fetched); **Priority Notifications** (iOS 18.4) places items judged important in a separate section at the top.

(b) Developers declare an interruption level: passive (no sound, never breaks Focus), active (default), time-sensitive (may break Focus and summaries), critical (bypasses everything; needs Apple approval and separate user opt-in), plus a 0–1 relevance score that orders items inside a summary [OneSignal developer doc; Apple's page did not render]. Apple Intelligence summaries are rendered in italics and later carried a "Summarized by Apple Intelligence" label; Priority is on-device, default off, per-app disable-able.

(c) Focus: Allow Notifications From / Silence Notifications From for people and apps, Time Sensitive override toggle, repeated calls within three minutes, Focus filters, scheduling and automatic triggers (sleep, driving, workout, controller). Display As: Count / Stack / List.

(d) Not keyboard-driven.

(e) Silenced items are not lost; they wait in Notification Center. Summaries can be turned off per app.

(f) Messages are grouped per app/thread; the summary condenses a group into a line.

(g) Praise (9to5Mac): Priority solves "we want certain notifications ... only the important ones". Failure: in Dec 2024–Jan 2025 summaries fabricated news (Mangione, Littler, Nadal); Apple called the feature beta, disabled it for news apps in 18.3, added a "may contain errors" note, then re-enabled with a mandatory label (Wikipedia, citing BBC).

(h) Focus: https://support.apple.com/guide/ipad/allow-or-silence-notifications-for-a-focus-ipad85aa12a1/ipados · Notifications: https://support.apple.com/en-us/108781 · Levels: https://documentation.onesignal.com/docs/en/ios-focus-modes-and-interruption-levels · Priority: https://www.macrumors.com/2025/02/21/ios-18-4-priority-notifications/ , https://www.9to5mac.com/2025/03/27/ios-184s-new-notification-feature-is-easily-among-apple-intelligences-best/ · Incident: https://en.wikipedia.org/wiki/Apple_Intelligence · Focus in practice: https://www.engadget.com/2237551/iphone-focus-modes-better-productivity/

### 13. Android notification channels

(a) Since Android 8.0 every notification belongs to a **channel** the app declares with a user-visible name; the user tunes each channel in system settings.

(b) Importance is per channel: Urgent (HIGH: sound and heads-up), High (DEFAULT: sound), Medium (LOW: silent), Low (MIN: silent, no status-bar icon), None. The app sets the initial importance and **cannot raise it later**; the user can change it at will.

(c) Channel groups (for multi-account apps); a deep link intent opens a channel's settings page. Android 13 adds a runtime POST_NOTIFICATIONS permission that is off by default for new installs; guidance is to ask in context, after the user has seen value, and to expect users to see per-app notification frequency and revoke.

(d) Not applicable. (e) Dismiss is per notification; channels can be silenced. (f) Group summaries stack notifications per app.

(g) The model is praised for giving users the last word; developer complaint is that a wrong initial importance cannot be corrected by the app.

(h) https://developer.android.com/develop/ui/views/notifications/channels · https://developer.android.com/develop/ui/views/notifications/notification-permission

### 14. Superhuman (command palette and split inbox)

Help-centre pages returned 403; sources are Superhuman's own shortcut PDF and blog, a design analysis, a third-party shortcut list and Wikipedia.

(a) A **Split Inbox**: presorted streams (Important, Other, and splits such as VIP, Team, Calendar, News) so the user batch-processes one kind of mail at a time; H/L move between splits.

(b) Splits are the prioritisation; the "why" is the split name. Custom split criteria (sender, domain, label, keywords) are described by third parties [unverified]. Adding a sender to a split is a Cmd+K command.

(c) Split membership and mute/unsubscribe (Shift+M mute, Cmd+U unsubscribe per PDF).

(d) Official PDF v8: J/K next/previous conversation; Enter open; E mark done; Shift+E mark not done; H remind me (snooze); S star; U read/unread; Z undo; X select; Shift+Esc select all; G-then-letter folder navigation; ? shortcut help; Cmd+K opens Superhuman Command. The Cmd+K blog: fuzzy match with a 0–1 command-score, synonyms shown as "title (alias)", context-aware boosting, last-used ordering for lists such as labels. Design analysis: Cmd+K shows the shortcut beside every action so users graduate to the key; a 100 ms public target (50–60 ms internal) [analysis; unverified]; 30-minute 1:1 onboarding drills on synthetic mail [unverified adoption figures].

(e) Done is archive; "Mark Not Done" restores; Z undoes the last action; snooze returns the thread at a time.

(f) Conversation view with expand-one / expand-all messages (O, Shift+O).

(g) Praise: speed and the shortcut-teaching palette. Complaints (Wikipedia): $30/month, "overhyped and overpriced"; a 2019 read-receipt controversy is widely reported but not fetched here [unverified]; Grammarly acquired it in 2025.

(h) PDF: https://download.superhuman.com/Superhuman%20Keyboard%20Shortcuts.pdf · Palette: https://blog.superhuman.com/how-to-build-a-remarkable-command-palette/ · Analysis: https://blakecrosley.com/guides/design/superhuman · Shortcuts (third-party): https://usethekeyboard.com/superhuman/ · Wikipedia: https://en.wikipedia.org/wiki/Superhuman_(email_client) · Official (403): https://help.superhuman.com/hc/en-us/articles/46005636204941-Custom-Split-Inbox

### 15. Raycast and Spotlight-type launchers

(a) One global hotkey (Cmd+Space) opens a search; Enter runs the primary action on the selected item; **Cmd+K opens an Action Panel** listing every action for that item with its shortcut shown on the right, searchable with fuzzy matching; Esc backs out.

(b) Aliases use strict prefix matching (exact beats prefix) and are more predictable than fuzzy search; hotkeys bind a command globally.

(c) Cmd+, configures a command inline; Emacs and Vim binding sets are offered.

(d) Enter / Cmd+Enter / Opt+Enter for primary, secondary, tertiary actions; Tab between fields. Maggie Appleton traces the pattern to Spotlight (2005) and Quicksilver and lists the components: hotkey, fuzzy search, tabbed filters, chained selections, extensibility.

(e) Not applicable. (f) Not applicable. (g) Praise: hands stay on the keyboard, rarely used commands leave the screen; criticism (Appleton): only useful if the search finds things by the words users actually use.

(h) https://manual.raycast.com/action-panel · https://manual.raycast.com/command-aliases-and-hotkeys · https://manual.raycast.com/keyboard-shortcuts · https://maggieappleton.com/command-bar

### 16. Gmail importance markers (short aside for the "why" pattern)

A yellow marker flags important mail. Signals listed by Google: who you email and how often, what you open, what you reply to, keywords in mail you read, what you star, archive or delete. Hovering the marker shows why the message was marked; clicking it toggles the mark **and** trains future predictions; is:important searches them. Source: https://support.google.com/mail/answer/186543

---

## Part B. Synthesis

### B1. Comparison table

| Client | Queue model | Shown "why" | Per-source tuning | Snooze | Done and reversibility | Keyboard core |
|---|---|---|---|---|---|---|
| Slack | Unreads + Activity feed + Later | item type; VIP section | All / mentions per channel; keywords; schedule; VIPs | Later reminders | Mark read (Undo), cleared items filter | Esc, Shift+Esc, Cmd+Shift+A/T/M, Cmd+/ |
| Discord | Inbox: Mentions / Unreads / For You | mention vs unread | All / @mentions / Nothing per server and channel; timed mute | timed mute | Read only; Esc / Shift+Esc | Ctrl+K, Ctrl+I, Ctrl+/ |
| Teams | Activity feed (30 days) | type icon per row | Banner+feed / feed / Off per event; per channel | none | Read only; mark all | Ctrl+Alt+K/U/M, Ctrl+. |
| Linear | Inbox (personal) + Triage (team) | notification type; Priority tab | channels, digest, subscribe per issue | H; returns on activity | Delete; accept/decline/merge; no archive | 1/2/3, H, U, J/K, G+I/T, Cmd+K, ? |
| GitHub | Inbox with reason labels | reason label + reason: filter | All / participating / ignore / custom per repo | none | Done (3 months), Saved, Unsubscribe | e, Shift+I/U/M, ? |
| Things 3 | Inbox to lists | list membership | n/a | date or Someday | Logbook keeps all | Ctrl+Space, Cmd+1..6, Cmd+K |
| Todoist | Inbox to projects | p1..p4 | reminders, filters | reschedule | complete, recurring advance | q, Ctrl+Cmd+A |
| Superlist | Inbox to lists | none | n/a | none | bulk actions | global launcher |
| Twist | Inbox of followed threads | followed | no presence, no dots | none | Done view, reopen | thread navigation |
| Arc | Sidebar; Little Arc | recency | archive cadence | auto-archive | Archive restorable | Cmd+T |
| Notion | Inbox by page/thread | trigger type | per page comment level | none | Archive, Archive read/all | (not documented) |
| Apple | Focus + Summary + Priority | Priority section; italics label | allow-lists; interruption level; per-app summary | Scheduled Summary | silenced items kept | n/a |
| Android | channels | channel name | importance per channel; user overrides app | none | dismiss | n/a |
| Superhuman | Split Inbox | split name | splits, mute, unsubscribe | H | E done, Shift+E not done, Z undo | J/K, E, H, Cmd+K, ? |
| Raycast | launcher | n/a | aliases, hotkeys | n/a | n/a | Cmd+Space, Cmd+K, Enter tiers |

### B2. Prioritisation and "why" patterns

1. **Label the reason on the row, then make the reason a filter.** GitHub's reason vocabulary appears as a label on every notification, is queryable (reason:mention) and ships in email headers so users can route elsewhere. Teams uses a per-type icon. Linear names the notification type. Users cannot correct a ranking they cannot see.
2. **One-action correction that also trains.** Gmail's marker is both the explanation (hover) and the correction (click), and Google states the click improves prediction. Apple's Priority section is per-app disable-able. Neither requires opening settings.
3. **Explicit allow-lists beat inferred importance for the top tier.** Slack VIPs, Apple Focus allow-lists and Teams priority contacts are user-declared and bypass quiet modes. The inference layer sits below them, never above.
4. **Separate "needs you" from "FYI" as a tab, not a sort.** Linear's Priority tab, Superhuman's Important/Other split, Apple's Priority section and Discord's Mentions vs Unreads all present two lists rather than one interleaved ranking, so a wrong guess lands in the other list rather than vanishing.
5. **Summaries need a visible marker and a kill switch.** Apple's incident ended with italics, a "Summarized by" label, per-app disable and a temporary category-wide pause. Slack's mobile summary is opt-in per view (pinch).
6. **Keyword rules must state their limits.** Slack documents that keywords are exact matches and ignore threads; unstated limits produce silent misses.

### B3. Triage queue patterns

- **One-at-a-time vs list scanning.** Slack ships both: Unreads (grouped list, Esc per group) on desktop and Catch up (card, swipe right/left) on mobile. Linear Triage and GitHub are lists with single-key exits; Superhuman is a list with a keyboard cursor. Card mode suits touch and small counts; list mode suits counts over a few dozen.
- **Shared vocabulary.** J/K next/previous (Linear, Superhuman, GitHub-style); E done or archive (GitHub, Superhuman); H snooze (Linear, Superhuman); X select; U read/unread (Linear, Superhuman); Shift+I/Shift+U read/unread (GitHub); Esc and Shift+Esc for "this" and "all" (Slack, Discord); Cmd+K palette (Slack jump, Linear, Superhuman, Raycast); ? for the shortcut sheet (GitHub, Linear, Superhuman); G-then-letter for navigation (Linear, Superhuman); numeric keys for triage verdicts (Linear 1/2/3).
- **Snooze that returns on activity.** Linear's snooze ends at the chosen time or on new activity, whichever first. GitHub Unsubscribe reverses on mention or review request. These prevent snooze from hiding a change of state.
- **Done with a retention window plus immediate undo.** GitHub keeps Done for 3 months and Saved forever; Slack shows Undo after mark-all-read and keeps cleared items in a filter; Superhuman has Z and Mark Not Done; Things and Arc keep everything in a log or archive. Nothing in the set deletes on first action except Linear's inbox delete.
- **Batch operations.** Mark all read (Slack, Teams, Discord per server), Archive read (Notion), Shift+Backspace delete all read (Linear), select-all-from-here (Superhuman), multiselect bulk move (Superlist).
- **Three exits, not one.** GitHub's Done / Saved / Unsubscribe distinguishes "handled", "later" and "stop telling me"; Slack's Later has Complete / Archive; Linear's Triage has Accept / Decline / Duplicate / Snooze. A single "dismiss" conflates these.

### B4. Notification design

- **Levels per source, with three settings.** Discord (All / @mentions / Nothing), Teams (All activity / Mentions & replies / Off), GitHub (All / Participating / Ignore / Custom), Slack (Everything / Mentions and DMs plus exceptions). Android turns the same idea into OS policy: the app proposes an importance, the user owns it.
- **Schedules and contexts.** Slack notification schedule; Apple Focus with allow-lists, Time Sensitive override and automatic triggers; Discord timed mutes; Teams mute-except-calls. Silenced items are retained (Apple Notification Center, Slack cleared filter).
- **Digest only what is still unread.** Linear emails digests grouped by urgency and only if the Inbox item was not read; Notion emails after five minutes of inactivity; Slack delays mobile push while the desktop is active. Delivery adapts to presence without showing presence to others (Twist removes presence entirely).
- **Escalation is declared, not inferred.** Apple interruption levels and Android importance are set by the sender's category; only a user allow-list (VIP, Focus people) lets something through a quiet period.
- **Evidence on fatigue.** Pielot et al. (MobileHCI 2014, 15 users, one week): 63.5 notifications per day, viewed within minutes even on silent, more notifications associated with more negative emotion but also more connectedness; the authors conclude suppressing interruptions is viable for professional communication. Fournier et al. (Computers in Human Behavior, via Hendrick): a single notification slows processing for about 7 seconds, d = 0.62, participants over 150 per day. Microsoft's 2025 telemetry (top 20 % of users by ping volume): an interruption every 2 minutes, 275 per day, 117 emails and 153 Teams messages daily. Doist cites HBR: about 80 % of the workday spent communicating.

### B5. Command palettes and keyboard discoverability

- **The palette is the tutorial.** Superhuman, Raycast and Linear print the shortcut beside every palette entry; Superhuman's blog and the Raycast manual both say users graduate to the key over time.
- **Two entry points, one sheet.** Cmd+K for "do or go" and ? (Linear, GitHub, Superhuman) or Cmd+/ (Slack) or Ctrl+. (Teams) or Ctrl+/ (Discord) for the full list. Things puts it in Help → Keyboard Shortcuts.
- **Context first.** Superhuman boosts commands valid in the current state and hides invalid ones; Raycast's Action Panel is per selected item with primary/secondary/ tertiary Enter tiers.
- **Forgiving search.** Fuzzy scoring with synonyms (Superhuman), but strict prefix for user-defined aliases (Raycast) because predictability matters once a habit forms.
- **Chords for navigation.** G-then-letter (Linear, Superhuman) keeps single keys free for actions on the focused item.
- **Global capture.** Things Ctrl+Space and Todoist Ctrl+Cmd+A open capture from any app; Things' Autofill variant pulls the current Mail or Safari item in.

### B6. Threading

- **Slack**: one level, replies hidden in a side panel, channel shows a reply count; a broadcast option because hiding hurt some users; an All Threads view. The design team says unlimited nesting made early prototypes "extremely complex".
- **Discord**: threads spawn from a message, open in a split panel, appear in the channel list, auto-archive after 24 hours idle and can be reopened.
- **GitHub and Linear**: flat comment streams per issue; the inbox row points at the latest event and the reason.
- **Twist**: a titled thread is the unit; comments are flat.
- **HN / Reddit-style trees**: Coding Horror argues nesting breaks reading order, causes horizontal scrolling and lets new replies appear anywhere, and proposes one level at most (Stack Exchange's question/answer/comment). HN users asked for collapse because a large first-comment subtree buries the second top-level comment and mobile lacks indent guides; extensions filled the gap until native collapse.
- **What is legible**: one visible level with a count and "last reply" affordance; a side panel rather than inline expansion; a cross-conversation "threads I follow" view; collapse controls when depth is unavoidable; and never showing the same reply twice.

### B7. The ten most transferable lessons for Riddle

1. **Put the reason on the row and make it a filter and a correction.** Evidence: GitHub reason labels and reason: queries; Gmail's hover-why plus click-to-train; Teams per-type icons. For Riddle: every disposition shows the rule or Signal that produced it, the row is filterable by that reason, and flipping it feeds the ML.
2. **Two lists beat one ranking.** Linear's Priority tab, Superhuman's Important/Other, Apple's Priority section. A wrong call lands in the other list, which is visible, rather than disappearing. Riddle's Unresolved disposition already fits this; keep it a tab, not a sort.
3. **User allow-lists sit above all inference and quiet modes.** Slack VIPs (bypass DND), Apple Focus people/apps, Teams priority contacts, Android user-owned importance the app cannot raise. This is the Never Hidden gate; expose it as an editable list with one-key add.
4. **Snooze returns on new activity, not only on the clock.** Linear's inbox and triage snooze; GitHub unsubscribe reverses on mention. A snoozed thread that gets a new reply should resurface.
5. **Done is a state with a retention window and an undo, never a delete.** GitHub Done (3 months, is:done), Saved (forever), Slack Undo after mark-all-read and the cleared filter, Superhuman Z and Mark Not Done, Things Logbook, Arc Archive.
6. **Offer three exits: handled, later, stop-telling-me.** GitHub Done/Saved/Unsubscribe, Slack Later Complete/Archive, Linear Accept/Decline/Duplicate/Snooze. A single dismiss loses the signal Riddle needs to learn from.
7. **Single-key vocabulary with a palette that prints the keys.** J/K, E, H, U, X, ?, Cmd+K, G-then-letter, numeric verdicts (Linear 1/2/3). Superhuman and Raycast show the shortcut beside every palette entry; users learn by doing.
8. **Latency is a feature and comes from local-first data.** Linear reads from IndexedDB, writes optimistically and syncs; Superhuman targets about 100 ms. For a desktop client over a local store, render from the store and reconcile with the mailbox later.
9. **AI summaries and verdicts must be labelled, per-source disable-able and pausable.** Apple's summary incident ended with italics, a label, per-app off and a category pause; Slack's summary is opt-in per view. Riddle's agent output should carry provenance and an approval gate, which matches its approval-gated design.
10. **Digest what is still unread and adapt delivery to presence without broadcasting presence.** Linear emails only unread items in urgency-grouped digests; Slack delays mobile push while desktop is active; Twist removes presence entirely. Combined with the fatigue data (63.5 to 150+ notifications per day, 7-second cost each, 275 workplace interruptions daily), a daily digest of Unresolved plus immediate alerts only for allow-listed senders is the defensible default.

Secondary lessons: state rule limits in the UI (Slack's keyword caveats); keep threads one level with counts and a side panel (Slack, Discord); give bulk actions on the read set (Notion Archive read, Linear delete all read); mobile can be one-card-at-a-time while desktop stays a list (Slack Catch up vs Unreads).

### B8. Sources

Primary (vendor docs, changelogs, design and engineering posts, official PDFs)
- https://slack.com/help/articles/19693583638803-Triage-notifications-in-the-Activity-tab
- https://slack.com/help/articles/226410907-View-all-your-unread-messages
- https://slack.com/help/articles/201457107-Send-and-read-messages
- https://slack.com/help/articles/34963579361683-Add-contacts-as-VIPs
- https://slack.com/help/articles/201355156-Configure-your-Slack-notifications
- https://slack.com/help/articles/360042650274-Save-messages-and-files-for-later
- https://slack.com/help/articles/201374536-Slack-keyboard-shortcuts
- https://slack.com/help/articles/115004846068-Slack-updates-and-changes
- https://slack.design/articles/threads-in-slack-a-long-design-journey-part-2-of-2/
- https://discord.com/blog/connect-the-conversation-with-threads-on-discord
- https://support.microsoft.com/en-us/teams/notifications-settings/explore-the-activity-feed-in-microsoft-teams
- https://support.microsoft.com/en-us/office/manage-notifications-in-microsoft-teams-1cc31834-5fe5-412b-8edb-43fecc78413d
- https://support.microsoft.com/en-us/office/keyboard-shortcuts-for-microsoft-teams-2e8e2a70-e8d8-4a19-949b-4c36dd5292d2
- https://linear.app/docs/inbox
- https://linear.app/docs/triage
- https://linear.app/docs/notifications
- https://linear.app/changelog/2021-06-17-inbox-snooze-and-easier-issue-merge
- https://linear.app/changelog/2021-06-29-linear-release-and-issue-triage
- https://linear.app/method/introduction
- https://docs.github.com/en/subscriptions-and-notifications/how-tos/viewing-and-triaging-notifications/managing-notifications-from-your-inbox
- https://docs.github.com/en/subscriptions-and-notifications/reference/inbox-filters
- https://docs.github.com/en/subscriptions-and-notifications/concepts/about-notifications
- https://docs.github.com/en/subscriptions-and-notifications/get-started/configuring-notifications
- https://docs.github.com/en/get-started/accessibility/keyboard-shortcuts
- https://culturedcode.com/things/support/articles/4001304/
- https://culturedcode.com/things/support/articles/2785159/
- https://culturedcode.com/things/support/articles/3289315/
- https://culturedcode.com/things/support/articles/2803584/
- https://culturedcode.com/things/features/
- https://www.todoist.com/inspiration/how-to-use-todoist-effectively
- https://www.todoist.com/help/articles/todoist-glossary-cA60laWMH
- https://www.todoist.com/inspiration/hidden-features-todoist
- https://www.superlist.com/updates
- https://twist.com/
- https://async.twist.com/asynchronous-communication/
- https://www.notion.com/help/updates-and-notifications
- https://www.notion.com/help/notification-settings
- https://www.notion.com/releases
- https://support.apple.com/guide/ipad/allow-or-silence-notifications-for-a-focus-ipad85aa12a1/ipados
- https://support.apple.com/en-us/108781
- https://developer.android.com/develop/ui/views/notifications/channels
- https://developer.android.com/develop/ui/views/notifications/notification-permission
- https://download.superhuman.com/Superhuman%20Keyboard%20Shortcuts.pdf
- https://blog.superhuman.com/how-to-build-a-remarkable-command-palette/
- https://manual.raycast.com/action-panel
- https://manual.raycast.com/command-aliases-and-hotkeys
- https://manual.raycast.com/keyboard-shortcuts
- https://support.google.com/mail/answer/186543
- https://www.microsoft.com/en-us/worklab/work-trend-index/breaking-down-infinite-workday
- http://www.pielot.org/pubs/Pielot2014-MobileHCI-Notifications.pdf (Pielot, Church, de Oliveira, MobileHCI 2014, doi:10.1145/2628363.2628364)

Reviews, teardowns and secondary sources
- https://techcrunch.com/2021/10/12/doist-redesigns-twist-the-slack-alternative-focused-on-async-work/
- https://office365itpros.com/2024/05/29/teams-activity-feed-changes/
- https://ben.balter.com/2020/08/25/how-i-manage-github-notifications/
- https://performance.dev/how-is-linear-so-fast-a-technical-breakdown
- https://www.lennysnewsletter.com/p/how-linear-builds-product
- https://keycombiner.com/collections/linear/
- https://raw.githubusercontent.com/hatefrad/product-engineer/main/case-studies/slack-threads.md
- https://westleydang.com/an-argument-for-threading-in-slack/
- https://blog.codinghorror.com/web-discussions-flat-by-design/
- https://alex.kirk.at/2010/02/16/collapsible-threads-for-hacker-news/
- https://news.ycombinator.com/item?id=11355038
- https://maggieappleton.com/command-bar
- https://blakecrosley.com/guides/design/superhuman
- https://usethekeyboard.com/superhuman/
- https://en.wikipedia.org/wiki/Superhuman_(email_client)
- https://en.wikipedia.org/wiki/Arc_(web_browser)
- https://en.wikipedia.org/wiki/Apple_Intelligence
- https://en.wikipedia.org/wiki/Discord
- https://www.macrumors.com/2025/02/21/ios-18-4-priority-notifications/
- https://www.9to5mac.com/2025/03/27/ios-184s-new-notification-feature-is-easily-among-apple-intelligences-best/
- https://www.engadget.com/2237551/iphone-focus-modes-better-productivity/
- https://documentation.onesignal.com/docs/en/ios-focus-modes-and-interruption-levels
- https://carlhendrick.substack.com/p/how-much-cognitive-damage-does-a (cites Fournier et al., Computers in Human Behavior)
- https://johannesippen.com/2024/superlist-test/ and https://johannesippen.com/2024/superlist-test-2/
- https://www.discords.ai/wiki/discord-notifications-management (third-party)
- https://defkey.com/discord-shortcuts (third-party)

Attempted but blocked (claims relying on them are marked unverified)
- https://support.discord.com/hc/en-us/articles/215253258-Notifications-Settings-101
- https://support.discord.com/hc/en-us/articles/360045027712-Inbox-FAQ
- https://help.superhuman.com/hc/en-us/articles/46005636204941-Custom-Split-Inbox
- https://help.superhuman.com/hc/en-us/articles/45191759067411-Speed-Up-With-Shortcuts
- https://resources.arc.net/hc/en-us/articles/19235387524503-Little-Arc-Quick-Lookups-Instant-Triaging
- https://resources.arc.net/hc/en-us/articles/19228855311127-Auto-Archive-Clean-as-you-go
- https://developer.apple.com/documentation/usernotifications/unnotificationinterruptionlevel (JS-rendered, no text)
- https://support.apple.com/guide/iphone/summarize-notifications-reduce-interruptions-iph1fbe7d2b9/ios (navigation only)
- https://dl.acm.org/doi/10.1145/2628363.2628364 (403; PDF fetched from author's site instead)
