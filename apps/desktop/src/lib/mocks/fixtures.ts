/**
 * Fixture mailbox for browser mode. Everything here is internally
 * consistent: every Find points at messages that exist, every classified
 * message has a decision trace, the Sift's bulk Activity lists the exact
 * GitHub messages the GitHub Mesh v7 routed to Spoil, and the simulation
 * evaluates the real DSL against these messages' cached Signals.
 */
import type { Dig } from '@/bindings/Dig'
import type { DigGroup } from '@/bindings/DigGroup'
import type { Disposition } from '@/bindings/Disposition'
import type { Find } from '@/bindings/Find'
import type { Message } from '@/bindings/Message'
import type { Mesh } from '@/bindings/Mesh'
import type { Signal } from '@/bindings/Signal'
import type { Stratum } from '@/bindings/Stratum'
import type { Job } from '@/bindings/Job'
import type { Activity } from '@/bindings/Activity'
import type { Autonomy } from '@/bindings/Autonomy'
import type { DecisionTrace } from '@/bindings/DecisionTrace'
import type { Facts } from '@/bindings/Facts'
import type { SignalEvaluation } from '@/bindings/SignalEvaluation'
import type { Conversation } from '@/bindings/Conversation'
import type { Expedition } from '@/bindings/Expedition'
import type { Draft } from '@/bindings/Draft'
import type { UserProfile } from '@/bindings/UserProfile'
import type { Address } from '@/bindings/Address'
import type { FindType } from '@/bindings/FindType'
import type { MeshScope } from '@/bindings/MeshScope'
import type { MeshStatus } from '@/bindings/MeshStatus'
import type { SignalStatus } from '@/bindings/SignalStatus'
import type { Settings, DevInfo, LogLine } from '../types'
import { u64 } from '../format'
import { parseMeshText } from './dsl'

// ---------------------------------------------------------------------------
// Time

export const NOW = new Date()
export const iso = (d: Date) => d.toISOString()
export const ago = (hours: number, minutes = 0) => iso(new Date(NOW.getTime() - hours * 3600_000 - minutes * 60_000))
export const ahead = (hours: number) => iso(new Date(NOW.getTime() + hours * 3600_000))
export const daysAgo = (d: number) => ago(d * 24)

// ---------------------------------------------------------------------------
// Digs and people

export const DIG_WORK = 'dig_work'
export const DIG_PERSONAL = 'dig_personal'
export const DIG_CONSULTING = 'dig_consulting'

export const ME: Record<string, Address> = {
  [DIG_WORK]: { email: 'matthew@northwind.example', name: 'Matthew Purdon' },
  [DIG_PERSONAL]: { email: 'matt.purdon@gmail.example', name: 'Matt Purdon' },
  [DIG_CONSULTING]: { email: 'matthew@purdon-consulting.example', name: 'Matthew Purdon' },
}

export function buildDigs(): Dig[] {
  return [
    {
      id: DIG_WORK,
      name: 'Work',
      email: ME[DIG_WORK].email,
      provider: 'fixture',
      group: 'Work',
      privacy: 'cloud_judgment',
      projection: 'labels_only',
      status: { state: 'catching_up', remaining: u64(1240), total: u64(4981) },
      shadowStartedAt: daysAgo(21),
      disabled: false,
      createdAt: daysAgo(21),
    },
    {
      id: DIG_PERSONAL,
      name: 'Personal',
      email: ME[DIG_PERSONAL].email,
      provider: 'fixture',
      group: 'Personal',
      privacy: 'cloud_reasoning',
      projection: 'full',
      status: { state: 'synced' },
      shadowStartedAt: daysAgo(40),
      disabled: false,
      createdAt: daysAgo(40),
    },
    {
      id: DIG_CONSULTING,
      name: 'Consulting',
      email: ME[DIG_CONSULTING].email,
      provider: 'fixture',
      group: 'Consulting',
      privacy: 'local_only',
      projection: 'none',
      status: { state: 'shadow', day: 3, of: 7 },
      shadowStartedAt: daysAgo(2.4),
      disabled: false,
      createdAt: daysAgo(2.4),
    },
  ]
}

export function buildDigGroups(): DigGroup[] {
  return [
    { name: 'Work', digs: [DIG_WORK], usedBy: [] },
    { name: 'Personal', digs: [DIG_PERSONAL], usedBy: [] },
    { name: 'Consulting', digs: [DIG_CONSULTING], usedBy: [] },
  ]
}

export function buildProfile(): UserProfile {
  return {
    addresses: Object.values(ME).map((a) => a.email),
    aliases: ['mp@northwind.example'],
    groupAddresses: ['platform-team@northwind.example', 'eng-all@northwind.example'],
    name: 'Matthew Purdon',
    role: 'Staff engineer',
    organisationDomains: ['northwind.example'],
    activeProjects: ['Riddle', 'Projection outbox', 'House renovation'],
    vips: ['priya@northwind.example', 'accountant@ledgerly.example'],
    family: ['mum@purdon-family.example', 'sarah@purdon-family.example'],
    neverHidden: ['@northwind.example', 'accountant@ledgerly.example', 'mum@purdon-family.example', 'sarah@purdon-family.example', 'security@github.example'],
    timezone: 'Europe/London',
  }
}

// ---------------------------------------------------------------------------
// Messages

export interface MessageSpec {
  id: string
  dig: string
  thread?: string
  from: Address
  to?: Address[]
  cc?: Address[]
  subject: string
  snippet?: string
  text: string
  html?: string | null
  hoursAgo: number
  labels?: string[]
  disposition: Disposition | null
  strata?: string[]
  findId?: string | null
  sent?: boolean
  attachments?: { filename: string; mimeType: string; sizeBytes: number }[]
  facts?: Partial<Facts>
  /** P(YES) per signal name; versions come from the registry's active version. */
  signals?: Record<string, number>
  /** For choice Signals: the full distribution. */
  choice?: Record<string, Record<string, number>>
  meshId?: string | null
  meshVersion?: number | null
  matchedRule?: string | null
  escalated?: { reason: string; rationale: string } | null
  gate?: string | null
  reason?: string
  shadow?: boolean
  /** Every Signal is a fresh evaluation (an injected message has no cache). */
  fresh?: boolean
  /** Who classified it: defaults to mesh when a mesh is named, else jev/gate/user. */
  mechanism?: DecisionTrace['mechanism']
  language?: string
}

const gh = (name: string): Address => ({ email: 'notifications@github.example', name })
const spoilGh = (i: number, subject: string, snippet: string, hoursAgo: number, relevant = 0.12): MessageSpec => ({
  id: `m_work_gh_${i}`,
  dig: DIG_WORK,
  from: gh('GitHub'),
  subject,
  snippet,
  text: `${snippet}\n\nView it on GitHub: https://github.example/northwind/platform\n\nYou are receiving this because you are subscribed to this repository.`,
  html: `<p>${snippet}</p><p><a href="https://github.example/northwind/platform/actions">View it on GitHub</a></p><p style="color:#666;font-size:12px">You are receiving this because you are subscribed to this repository.<img src="https://github.example/notifications/beacon/${i}.gif" width="1" height="1"></p>`,
  hoursAgo,
  labels: ['INBOX', 'CATEGORY_UPDATES'],
  disposition: { kind: 'spoil' },
  strata: ['Engineering', 'Notifications'],
  facts: { senderDomain: 'github.example', mailingList: false, autoSubmitted: true, precedenceBulk: true, listUnsubscribe: true, gmailCategory: 'updates' },
  signals: { review_requested: 0.03, ci_failure: 0.04, personally_relevant: relevant, routine: 0.95, requires_response: 0.05 },
  meshId: 'mesh_github',
  meshVersion: 7,
  matchedRule: 'ELSE IF routine@v1 > .90\n     AND personally_relevant@v3 < .30\n    → Spoil',
  reason: 'Routine GitHub notification not addressed to you.',
})

const newsletter = (i: number, dig: string, from: Address, subject: string, snippet: string, hoursAgo: number): MessageSpec => ({
  id: `m_${dig.replace('dig_', '')}_nl_${i}`,
  dig,
  from,
  subject,
  snippet,
  text: `${snippet}\n\nRead the full issue online.\n\nUnsubscribe: https://${from.email.split('@')[1]}/unsubscribe`,
  html: `<h2 style="font-family:Georgia">${subject}</h2><p>${snippet}</p><p><a href="https://${from.email.split('@')[1]}/issue/${i}">Read online</a> · <a href="https://${from.email.split('@')[1]}/unsubscribe">Unsubscribe</a></p><img src="https://${from.email.split('@')[1]}/o.gif" width="1" height="1">`,
  hoursAgo,
  labels: ['INBOX', 'CATEGORY_PROMOTIONS'],
  disposition: { kind: 'spoil' },
  strata: ['Newsletters'],
  facts: { senderDomain: from.email.split('@')[1], mailingList: true, listUnsubscribe: true, precedenceBulk: true, gmailCategory: 'promotions' },
  signals: { newsletter: 0.97, personally_relevant: 0.08, routine: 0.9, requires_response: 0.02 },
  meshId: null,
  meshVersion: null,
  matchedRule: null,
  mechanism: 'director',
  reason: 'Director moved it to Spoil at your request: newsletters you have not opened.',
})

export function messageSpecs(): MessageSpec[] {
  const specs: MessageSpec[] = [
    // --- Find: Review (PR #412) ---
    {
      id: 'm_work_pr412',
      dig: DIG_WORK,
      thread: 't_work_pr412',
      from: gh('Jamie Okafor'),
      to: [ME[DIG_WORK]],
      subject: '[northwind/platform] Review requested: Retry budget for the projection outbox (PR #412)',
      snippet: '@matthew requested your review on #412. The outbox retries were unbounded when Gmail rate-limits; this adds a budget and a pause reason.',
      text: '@matthew requested your review on #412.\n\nThe outbox retries were unbounded when Gmail rate-limits; this adds a budget and a pause reason so the Task Centre can say why projection stopped.\n\nFiles changed: 6, +214 −38\n\nhttps://github.example/northwind/platform/pull/412',
      html: '<p><b>@matthew</b> requested your review on <a href="https://github.example/northwind/platform/pull/412">#412</a>.</p><blockquote>The outbox retries were unbounded when Gmail rate-limits; this adds a budget and a pause reason so the Task Centre can say why projection stopped.</blockquote><p>Files changed: 6, <span style="color:green">+214</span> <span style="color:red">−38</span></p>',
      hoursAgo: 1.4,
      labels: ['INBOX', 'CATEGORY_UPDATES', 'IMPORTANT'],
      disposition: { kind: 'find', find_type: 'review' },
      strata: ['Engineering', 'Projects'],
      findId: 'find_review_412',
      facts: { senderDomain: 'github.example', directlyAddressed: true, autoSubmitted: true, gmailImportant: true, gmailCategory: 'updates' },
      signals: { review_requested: 0.96, ci_failure: 0.02, personally_relevant: 0.91, routine: 0.62, requires_response: 0.08 },
      meshId: 'mesh_github',
      meshVersion: 7,
      matchedRule: 'IF review_requested@v3 > .80\n    → Find: Review',
      reason: 'Jamie requested your review directly.',
    },
    {
      id: 'm_work_pr412_2',
      dig: DIG_WORK,
      thread: 't_work_pr412',
      from: gh('Jamie Okafor'),
      to: [ME[DIG_WORK]],
      subject: 'Re: [northwind/platform] Review requested: Retry budget for the projection outbox (PR #412)',
      snippet: 'Pushed a fix for the flaky test. Still needs your eyes on the pause-reason enum before I merge.',
      text: 'Pushed a fix for the flaky test. Still needs your eyes on the pause-reason enum before I merge.\n\n— Jamie',
      hoursAgo: 0.6,
      labels: ['INBOX', 'CATEGORY_UPDATES'],
      disposition: { kind: 'find', find_type: 'review' },
      strata: ['Engineering', 'Projects'],
      findId: 'find_review_412',
      facts: { senderDomain: 'github.example', directlyAddressed: true, autoSubmitted: true, threadLength: 2, gmailCategory: 'updates' },
      signals: { review_requested: 0.88, ci_failure: 0.03, personally_relevant: 0.9, routine: 0.4, requires_response: 0.35 },
      meshId: 'mesh_github',
      meshVersion: 7,
      matchedRule: 'IF review_requested@v3 > .80\n    → Find: Review',
      reason: 'Joined the open Review Find by thread.',
    },
    // --- Find: Do (deploy after review) ---
    {
      id: 'm_work_deploy',
      dig: DIG_WORK,
      from: { email: 'priya@northwind.example', name: 'Priya Raman' },
      to: [ME[DIG_WORK]],
      cc: [{ email: 'platform-team@northwind.example', name: 'Platform team' }],
      subject: 'Deploy the outbox hotfix once #412 lands',
      snippet: 'Can you take the deploy once Jamie merges? Change freeze starts Friday 17:00 so it needs to be out before then.',
      text: 'Matthew,\n\nCan you take the deploy once Jamie merges #412? Change freeze starts Friday 17:00 so it needs to be out before then.\n\nThanks,\nPriya',
      hoursAgo: 3.2,
      labels: ['INBOX', 'IMPORTANT'],
      disposition: { kind: 'find', find_type: 'do' },
      strata: ['Engineering', 'Projects'],
      findId: 'find_do_deploy',
      facts: { senderDomain: 'northwind.example', sameOrganisation: true, senderInContacts: true, senderRepliedToBefore: true, directlyAddressed: true, senderIsVip: true, senderInNeverHidden: true, gmailImportant: true, deadlines: [{ at: ahead(60), text: 'Friday 17:00', daysFromNow: u64(2), past: false }] },
      signals: { requires_response: 0.71, personally_relevant: 0.97, urgency: 0.6, routine: 0.05, commitment_made: 0.2 },
      meshId: null,
      matchedRule: null,
      escalated: { reason: 'No Mesh route matched; requires_response .71 below the .80 auto threshold', rationale: 'A named colleague asks Matthew to carry out a deploy before a stated deadline. This is an action request, not a question: Do.' },
      reason: 'Priya asked you to do the deploy before the change freeze.',
      mechanism: 'llm',
    },
    // --- Find: Decide (Datadog renewal) ---
    {
      id: 'm_work_datadog',
      dig: DIG_WORK,
      thread: 't_work_datadog',
      from: { email: 'renewals@datadog.example', name: 'Datadog Renewals' },
      to: [ME[DIG_WORK]],
      subject: 'Your Datadog contract renews on the 30th — confirm seats',
      snippet: 'Your annual agreement renews on the 30th at 48 seats ($21,600). Reply to adjust seats or cancel before the renewal date.',
      text: 'Hello Matthew,\n\nYour annual agreement renews on the 30th at 48 seats ($21,600). Reply to adjust seats or cancel before the renewal date.\n\nDatadog Renewals',
      hoursAgo: 26,
      labels: ['INBOX'],
      disposition: { kind: 'find', find_type: 'decide' },
      strata: ['Finance', 'Engineering'],
      findId: 'find_decide_renewal',
      facts: { senderDomain: 'datadog.example', directlyAddressed: true, senderRepliedToBefore: true, deadlines: [{ at: ahead(8 * 24), text: 'the 30th', daysFromNow: u64(8), past: false }] },
      signals: { requires_response: 0.83, personally_relevant: 0.8, payment_due: 0.55, routine: 0.2, urgency: 0.4 },
      meshId: null,
      matchedRule: null,
      reason: 'A decision with a date: 48 seats renew on the 30th.',
      mechanism: 'jev',
    },
    {
      id: 'm_work_datadog_2',
      dig: DIG_WORK,
      thread: 't_work_datadog',
      from: { email: 'priya@northwind.example', name: 'Priya Raman' },
      to: [ME[DIG_WORK]],
      subject: 'Fwd: Your Datadog contract renews on the 30th — confirm seats',
      snippet: 'We are only using 31 of the 48 seats. Your call whether we drop to 32 or keep headroom for the hires.',
      text: 'We are only using 31 of the 48 seats. Your call whether we drop to 32 or keep headroom for the hires.\n\nP',
      hoursAgo: 20,
      labels: ['INBOX', 'IMPORTANT'],
      disposition: { kind: 'find', find_type: 'decide' },
      strata: ['Finance', 'Engineering'],
      findId: 'find_decide_renewal',
      facts: { senderDomain: 'northwind.example', sameOrganisation: true, senderInContacts: true, senderRepliedToBefore: true, directlyAddressed: true, senderIsVip: true, senderInNeverHidden: true, threadLength: 2 },
      signals: { requires_response: 0.9, personally_relevant: 0.95, payment_due: 0.3, routine: 0.05, urgency: 0.3 },
      meshId: null,
      matchedRule: null,
      reason: 'Priya is waiting on your decision.',
      mechanism: 'jev',
    },
    // --- AWS billing alert (user corrected Spoil → Find: Know; snoozed) ---
    {
      id: 'm_work_aws',
      dig: DIG_WORK,
      from: { email: 'no-reply@aws.example', name: 'AWS Budgets' },
      to: [ME[DIG_WORK]],
      subject: 'AWS Budgets: northwind-prod is $412 above forecast',
      snippet: 'Your monthly budget northwind-prod has exceeded its forecasted amount. Actual: $6,812. Forecast: $6,400.',
      text: 'Your monthly budget northwind-prod has exceeded its forecasted amount.\n\nActual: $6,812\nForecast: $6,400\n\nView in the AWS Billing console.',
      hoursAgo: 0.9,
      labels: ['INBOX', 'CATEGORY_UPDATES'],
      disposition: { kind: 'find', find_type: 'know' },
      strata: ['Finance', 'Notifications'],
      findId: 'find_know_aws',
      facts: { senderDomain: 'aws.example', autoSubmitted: true, gmailCategory: 'updates' },
      signals: { billing_alert: 0.93, routine: 0.88, personally_relevant: 0.35, requires_response: 0.1 },
      meshId: 'mesh_aws',
      meshVersion: 3,
      matchedRule: 'ELSE IF routine@v1 > .85\n    → Spoil',
      reason: 'You changed it from Spoil to Find: Know. The AWS Notifications Mesh (Shadow) had proposed Spoil.',
      mechanism: 'user',
    },
    // --- Personal: Respond (Adam, Jeep brakes) ---
    {
      id: 'm_pers_adam_1',
      dig: DIG_PERSONAL,
      thread: 't_pers_jeep',
      from: { email: 'adam.k@mechanics.example', name: 'Adam Kowalski' },
      to: [ME[DIG_PERSONAL]],
      subject: 'Jeep brakes — what we found',
      snippet: 'Rear pads are down to 2mm and the offside caliper is sticking. Rotors are borderline. Two options below with prices.',
      text: 'Hi Matt,\n\nRear pads are down to 2mm and the offside caliper is sticking. Rotors are borderline.\n\nOption A: pads + caliper rebuild, £310\nOption B: pads + rotors + new caliper, £540\n\nLet me know which and I can fit it Thursday.\n\nAdam',
      hoursAgo: 30,
      labels: ['INBOX', 'CATEGORY_PERSONAL'],
      disposition: { kind: 'find', find_type: 'respond' },
      strata: ['People'],
      findId: 'find_respond_jeep',
      facts: { senderDomain: 'mechanics.example', senderInContacts: true, senderRepliedToBefore: true, directlyAddressed: true, gmailCategory: 'personal' },
      signals: { requires_response: 0.94, personally_relevant: 0.98, urgency: 0.3, routine: 0.02, payment_due: 0.2 },
      meshId: null,
      matchedRule: null,
      reason: 'Adam asked you to choose between two options.',
      mechanism: 'jev',
    },
    {
      id: 'm_pers_adam_2',
      dig: DIG_PERSONAL,
      thread: 't_pers_jeep',
      from: ME[DIG_PERSONAL],
      to: [{ email: 'adam.k@mechanics.example', name: 'Adam Kowalski' }],
      subject: 'Re: Jeep brakes — what we found',
      snippet: 'Thanks Adam. Is the caliper rebuild likely to hold, or am I paying twice?',
      text: 'Thanks Adam. Is the caliper rebuild likely to hold, or am I paying twice?\n\nMatt',
      hoursAgo: 27,
      labels: ['SENT', 'CATEGORY_PERSONAL'],
      sent: true,
      disposition: { kind: 'filed' },
      strata: ['People'],
      findId: 'find_respond_jeep',
      facts: { senderDomain: 'gmail.example', isSent: true, threadLength: 3 },
      signals: { commitment_made: 0.1, requires_response: 0.05, personally_relevant: 0.9 },
      meshId: null,
      matchedRule: null,
      reason: 'Sent mail: your question keeps the thread waiting on Adam.',
      mechanism: 'deterministic',
    },
    {
      id: 'm_pers_adam_3',
      dig: DIG_PERSONAL,
      thread: 't_pers_jeep',
      from: { email: 'adam.k@mechanics.example', name: 'Adam Kowalski' },
      to: [ME[DIG_PERSONAL]],
      subject: 'Re: Jeep brakes — what we found',
      snippet: 'Honestly the rebuild usually holds 2-3 years. Your call. I need to order parts by tomorrow noon for Thursday.',
      text: 'Honestly the rebuild usually holds 2-3 years. Your call. I need to order parts by tomorrow noon for Thursday.\n\nAdam',
      hoursAgo: 4.5,
      labels: ['INBOX', 'CATEGORY_PERSONAL'],
      disposition: { kind: 'find', find_type: 'respond' },
      strata: ['People'],
      findId: 'find_respond_jeep',
      facts: { senderDomain: 'mechanics.example', senderInContacts: true, senderRepliedToBefore: true, directlyAddressed: true, threadLength: 3, myLastReplyAt: ago(27), deadlines: [{ at: ahead(20), text: 'tomorrow noon', daysFromNow: u64(1), past: false }] },
      signals: { requires_response: 0.97, personally_relevant: 0.98, urgency: 0.7, routine: 0.02, payment_due: 0.2 },
      meshId: null,
      matchedRule: null,
      reason: 'Adam is waiting on your answer by tomorrow noon.',
      mechanism: 'jev',
    },
    // --- Personal: Attend (dentist) ---
    {
      id: 'm_pers_dentist',
      dig: DIG_PERSONAL,
      from: { email: 'reminders@brightsmile.example', name: 'BrightSmile Dental' },
      to: [ME[DIG_PERSONAL]],
      subject: 'Reminder: appointment Thursday 09:30 with Dr Amara',
      snippet: 'Your hygienist appointment is on Thursday at 09:30. Reply C to confirm or call to reschedule.',
      text: 'Your hygienist appointment is on Thursday at 09:30 with Dr Amara.\n\nReply C to confirm or call 020 7946 0000 to reschedule.',
      hoursAgo: 9,
      labels: ['INBOX', 'CATEGORY_UPDATES'],
      disposition: { kind: 'find', find_type: 'attend' },
      strata: ['People'],
      findId: 'find_attend_dentist',
      facts: { senderDomain: 'brightsmile.example', autoSubmitted: true, hasCalendarPart: true, gmailCategory: 'updates', deadlines: [{ at: ahead(2 * 24 + 1), text: 'Thursday 09:30', daysFromNow: u64(2), past: false }] },
      signals: { requires_response: 0.4, personally_relevant: 0.9, routine: 0.5, urgency: 0.35 },
      meshId: null,
      matchedRule: null,
      gate: 'calendar invitation: never hidden',
      reason: 'A dated appointment for you. Calendar mail is never hidden.',
      mechanism: 'gate',
    },
    // --- Consulting: Pay (invoice) ---
    {
      id: 'm_cons_invoice',
      dig: DIG_CONSULTING,
      from: { email: 'accountant@ledgerly.example', name: 'Dana Whitfield (Ledgerly)' },
      to: [ME[DIG_CONSULTING]],
      subject: 'Invoice #2024-117 — Q3 bookkeeping, due Friday',
      snippet: 'Please find attached invoice #2024-117 for Q3 bookkeeping, £1,240.00, due Friday. Bank details unchanged.',
      text: 'Hi Matthew,\n\nPlease find attached invoice #2024-117 for Q3 bookkeeping, £1,240.00, due Friday. Bank details unchanged.\n\nBest,\nDana',
      hoursAgo: 15,
      labels: ['INBOX'],
      attachments: [{ filename: 'invoice-2024-117.pdf', mimeType: 'application/pdf', sizeBytes: 84_322 }],
      disposition: { kind: 'find', find_type: 'pay' },
      strata: ['Finance'],
      findId: 'find_pay_invoice',
      facts: { senderDomain: 'ledgerly.example', senderInContacts: true, senderRepliedToBefore: true, directlyAddressed: true, senderIsVip: true, senderInNeverHidden: true, hasAttachments: true, hasStructuredReceipt: true, deadlines: [{ at: ahead(3 * 24), text: 'Friday', daysFromNow: u64(3), past: false }] },
      signals: { payment_due: 0.96, requires_response: 0.3, personally_relevant: 0.95, receipt: 0.6, routine: 0.1 },
      meshId: null,
      matchedRule: null,
      reason: 'An invoice with a due date from a Never Hidden sender.',
      mechanism: 'jev',
      shadow: true,
    },
    // --- Consulting: Follow Up (Harbor Freight proposal, temporal) ---
    {
      id: 'm_cons_harbor_1',
      dig: DIG_CONSULTING,
      thread: 't_cons_harbor',
      from: ME[DIG_CONSULTING],
      to: [{ email: 'ops@harborfreight.example', name: 'Lena Ortiz' }],
      subject: 'Proposal: warehouse scheduling audit',
      snippet: 'Attached is the proposal we discussed: a four-week audit of the scheduling pipeline with a fixed fee.',
      text: 'Hi Lena,\n\nAttached is the proposal we discussed: a four-week audit of the scheduling pipeline with a fixed fee. Happy to walk through it on a call.\n\nMatthew',
      hoursAgo: 9 * 24,
      labels: ['SENT'],
      sent: true,
      attachments: [{ filename: 'harbor-freight-proposal.pdf', mimeType: 'application/pdf', sizeBytes: 312_044 }],
      disposition: { kind: 'find', find_type: 'follow_up' },
      strata: ['Projects'],
      findId: 'find_followup_harbor',
      facts: { senderDomain: 'purdon-consulting.example', isSent: true, hasAttachments: true },
      signals: { commitment_made: 0.4, requires_response: 0.05, personally_relevant: 0.9 },
      meshId: null,
      matchedRule: null,
      reason: 'You sent a proposal 9 days ago and Lena has not replied.',
      mechanism: 'temporal',
      shadow: true,
    },
    // --- Resolved Find: offsite agenda (resolved by thread state) ---
    {
      id: 'm_work_offsite',
      dig: DIG_WORK,
      thread: 't_work_offsite',
      from: { email: 'sam.l@northwind.example', name: 'Sam Liu' },
      to: [ME[DIG_WORK]],
      subject: 'Offsite agenda — can you own the architecture session?',
      snippet: 'Would you take the 90-minute architecture session on day two? Need a yes by Wednesday so I can print the agenda.',
      text: 'Would you take the 90-minute architecture session on day two? Need a yes by Wednesday so I can print the agenda.\n\nSam',
      hoursAgo: 2 * 24 + 3,
      labels: ['INBOX'],
      disposition: { kind: 'find', find_type: 'respond' },
      strata: ['People', 'Projects'],
      findId: 'find_resolved_offsite',
      facts: { senderDomain: 'northwind.example', sameOrganisation: true, senderInContacts: true, senderRepliedToBefore: true, directlyAddressed: true, senderInNeverHidden: true },
      signals: { requires_response: 0.92, personally_relevant: 0.94, routine: 0.05, urgency: 0.4 },
      meshId: null,
      matchedRule: null,
      reason: 'Sam asked a direct question with a date.',
      mechanism: 'jev',
    },
    {
      id: 'm_work_offsite_2',
      dig: DIG_WORK,
      thread: 't_work_offsite',
      from: ME[DIG_WORK],
      to: [{ email: 'sam.l@northwind.example', name: 'Sam Liu' }],
      subject: 'Re: Offsite agenda — can you own the architecture session?',
      snippet: 'Yes, happy to. Send me the room and I will draft an outline.',
      text: 'Yes, happy to. Send me the room and I will draft an outline.',
      hoursAgo: 5,
      labels: ['SENT'],
      sent: true,
      disposition: { kind: 'filed' },
      strata: ['People', 'Projects'],
      facts: { senderDomain: 'northwind.example', isSent: true, threadLength: 2 },
      signals: { commitment_made: 0.86, requires_response: 0.02, personally_relevant: 0.9 },
      meshId: null,
      matchedRule: null,
      reason: 'Sent from Gmail on the web; it resolved the Respond Find and recorded a commitment.',
      mechanism: 'deterministic',
    },
    // --- Unresolved (5) ---
    {
      id: 'm_pers_lisbon',
      dig: DIG_PERSONAL,
      from: { email: 'ines.c@sapo.example', name: 'Inês Carvalho' },
      to: [ME[DIG_PERSONAL]],
      subject: 'Fotos de Lisboa?',
      snippet: 'Olá Matt! Ainda tens as fotos do jantar em Alfama? A Rita quer imprimir uma para a mãe.',
      text: 'Olá Matt!\n\nAinda tens as fotos do jantar em Alfama? A Rita quer imprimir uma para a mãe.\n\nBeijinhos,\nInês',
      hoursAgo: 7,
      labels: ['INBOX', 'CATEGORY_PERSONAL'],
      disposition: { kind: 'unresolved' },
      strata: [],
      facts: { senderDomain: 'sapo.example', firstTimeSender: true, directlyAddressed: true, gmailCategory: 'personal' },
      language: 'pt',
      signals: { requires_response: 0.55, personally_relevant: 0.6, routine: 0.1 },
      meshId: null,
      matchedRule: null,
      escalated: { reason: 'Language pt outside the Signal Set language scope; requires_response .55 uncertain', rationale: 'The message is Portuguese and the Signals are trusted for English only. It asks for photos; it may be personal. Leaving Unresolved rather than guessing.' },
      reason: 'Portuguese, from a first-time sender. Riddle would rather ask than guess.',
      mechanism: 'llm',
    },
    {
      id: 'm_work_quickone',
      dig: DIG_WORK,
      from: { email: 'tom.b@vantagepartners.example', name: 'Tom Bright' },
      to: [ME[DIG_WORK]],
      subject: 'Quick one',
      snippet: 'Are you the right person for the thing Priya mentioned? If not, who?',
      text: 'Are you the right person for the thing Priya mentioned? If not, who?\n\nTom',
      hoursAgo: 2.2,
      labels: ['INBOX'],
      disposition: { kind: 'unresolved' },
      strata: [],
      facts: { senderDomain: 'vantagepartners.example', firstTimeSender: true, directlyAddressed: true },
      signals: { requires_response: 0.47, personally_relevant: 0.44, routine: 0.09, urgency: 0.2 },
      meshId: null,
      matchedRule: null,
      escalated: { reason: 'requires_response .47 / personally_relevant .44 below the .80 auto threshold', rationale: 'A first-time sender references an unnamed matter. There is not enough context to know whether Matthew is expected to act. Unresolved.' },
      reason: 'Too little context: a first-time sender about an unnamed matter.',
      mechanism: 'llm',
    },
    // --- Unresolved: three alerts from one sender, so the sender cluster has something to group ---
    {
      id: 'm_work_uptime_1',
      dig: DIG_WORK,
      from: { email: 'alerts@uptime.example', name: 'Uptime monitor' },
      to: [ME[DIG_WORK]],
      subject: '[Down] api.example.com is DOWN',
      snippet: 'Monitor api.example.com is DOWN: HTTP 503 from eu-west-1. Retrying every 60 s.',
      text: 'Monitor api.example.com is DOWN: HTTP 503 from eu-west-1. Retrying every 60 s.\n\nThis is an automated alert. Manage monitors at https://uptime.example/dashboard',
      hoursAgo: 1.1,
      labels: ['INBOX', 'CATEGORY_UPDATES'],
      disposition: { kind: 'unresolved' },
      strata: [],
      facts: { senderDomain: 'uptime.example', firstTimeSender: true, directlyAddressed: false },
      signals: { requires_response: 0.22, personally_relevant: 0.38, routine: 0.52, urgency: 0.61 },
      meshId: null,
      matchedRule: null,
      escalated: { reason: 'routine .52 / urgency .61: an alert from a sender no Mesh mentions', rationale: 'Automated monitoring alerts can be noise or an outage that needs someone. No rule names this sender and the Signals disagree, so it stays Unresolved for a decision.' },
      reason: 'An alert from a monitoring sender no rule mentions; the Signals disagree on whether it needs you.',
      mechanism: 'llm',
    },
    {
      id: 'm_work_uptime_2',
      dig: DIG_WORK,
      from: { email: 'alerts@uptime.example', name: 'Uptime monitor' },
      to: [ME[DIG_WORK]],
      subject: '[Up] api.example.com is UP',
      snippet: 'Monitor api.example.com is back UP after 4 minutes: HTTP 200 from eu-west-1.',
      text: 'Monitor api.example.com is back UP after 4 minutes: HTTP 200 from eu-west-1.\n\nThis is an automated alert. Manage monitors at https://uptime.example/dashboard',
      hoursAgo: 1.0,
      labels: ['INBOX', 'CATEGORY_UPDATES'],
      disposition: { kind: 'unresolved' },
      strata: [],
      facts: { senderDomain: 'uptime.example', firstTimeSender: false, directlyAddressed: false },
      signals: { requires_response: 0.22, personally_relevant: 0.38, routine: 0.52, urgency: 0.61 },
      meshId: null,
      matchedRule: null,
      escalated: { reason: 'routine .52 / urgency .61: an alert from a sender no Mesh mentions', rationale: 'Automated monitoring alerts can be noise or an outage that needs someone. No rule names this sender and the Signals disagree, so it stays Unresolved for a decision.' },
      reason: 'An alert from a monitoring sender no rule mentions; the Signals disagree on whether it needs you.',
      mechanism: 'llm',
    },
    {
      id: 'm_work_uptime_3',
      dig: DIG_WORK,
      from: { email: 'alerts@uptime.example', name: 'Uptime monitor' },
      to: [ME[DIG_WORK]],
      subject: '[Down] db-replica-2 is DOWN',
      snippet: 'Monitor db-replica-2 is DOWN: connection refused on port 5432.',
      text: 'Monitor db-replica-2 is DOWN: connection refused on port 5432.\n\nThis is an automated alert. Manage monitors at https://uptime.example/dashboard',
      hoursAgo: 0.6,
      labels: ['INBOX', 'CATEGORY_UPDATES'],
      disposition: { kind: 'unresolved' },
      strata: [],
      facts: { senderDomain: 'uptime.example', firstTimeSender: false, directlyAddressed: false },
      signals: { requires_response: 0.22, personally_relevant: 0.38, routine: 0.52, urgency: 0.61 },
      meshId: null,
      matchedRule: null,
      escalated: { reason: 'routine .52 / urgency .61: an alert from a sender no Mesh mentions', rationale: 'Automated monitoring alerts can be noise or an outage that needs someone. No rule names this sender and the Signals disagree, so it stays Unresolved for a decision.' },
      reason: 'An alert from a monitoring sender no rule mentions; the Signals disagree on whether it needs you.',
      mechanism: 'llm',
    },
    // --- Contamination (3) ---
    {
      id: 'm_pers_phish',
      dig: DIG_PERSONAL,
      from: { email: 'security@paypa1-alerts.example', name: 'PayPal Security' },
      to: [ME[DIG_PERSONAL]],
      subject: 'URGENT: Your account will be suspended in 24 hours',
      snippet: 'We detected unusual activity. Confirm your identity immediately or your account will be permanently suspended.',
      text: 'We detected unusual activity. Confirm your identity immediately or your account will be permanently suspended.\n\nhttp://paypa1-alerts.example/verify',
      html: '<p style="color:red"><b>URGENT:</b> We detected unusual activity.</p><p>Confirm your identity immediately: <a href="http://paypa1-alerts.example/verify">Verify now</a></p><img src="http://paypa1-alerts.example/t.png">',
      hoursAgo: 11,
      labels: ['SPAM'],
      disposition: { kind: 'contamination' },
      strata: [],
      facts: { senderDomain: 'paypa1-alerts.example', firstTimeSender: true, spfPass: false, dkimPass: false, dmarcPass: false, gmailSpam: true, lookalikeDomain: true, urgencyLanguagePresent: true, asksForPaymentOrCredentials: true, replyToMismatch: true },
      signals: {},
      gate: 'contamination: Gmail spam verdict, DMARC fail, lookalike domain, asks for credentials',
      reason: 'Failed authentication from a lookalike domain, asking for credentials, with urgency language. Evidence, not judgment.',
      mechanism: 'gate',
    },
    {
      id: 'm_work_invoice_scam',
      dig: DIG_WORK,
      from: { email: 'ceo.northwind@outlook-mail.example', name: 'Marcus Hale' },
      to: [ME[DIG_WORK]],
      subject: 'Need this paid today',
      snippet: 'Matthew, I am in meetings all day. Please wire the attached invoice before 3pm and confirm. Do not call.',
      text: 'Matthew, I am in meetings all day. Please wire the attached invoice before 3pm and confirm. Do not call.\n\nMarcus',
      hoursAgo: 19,
      labels: ['INBOX'],
      disposition: { kind: 'contamination' },
      strata: [],
      facts: { senderDomain: 'outlook-mail.example', firstTimeSender: true, spfPass: true, dkimPass: true, dmarcPass: false, lookalikeDomain: true, urgencyLanguagePresent: true, asksForPaymentOrCredentials: true },
      signals: {},
      gate: 'contamination: first-time sender impersonating a VIP, asks for payment, urgency language',
      reason: 'Impersonates a colleague from an external address and asks for money today.',
      mechanism: 'gate',
    },
    {
      id: 'm_cons_seo',
      dig: DIG_CONSULTING,
      from: { email: 'growth@rankboost.example', name: 'RankBoost' },
      to: [ME[DIG_CONSULTING]],
      subject: 'I noticed purdon-consulting.example is not ranking',
      snippet: 'I can get you to page one in 30 days guaranteed. Reply YES for a free audit.',
      text: 'I can get you to page one in 30 days guaranteed. Reply YES for a free audit.',
      hoursAgo: 40,
      labels: ['SPAM'],
      disposition: { kind: 'contamination' },
      strata: [],
      facts: { senderDomain: 'rankboost.example', firstTimeSender: true, gmailSpam: true, spfPass: true, dkimPass: false, dmarcPass: false },
      signals: {},
      gate: 'contamination: Gmail spam verdict, DMARC fail, first-time sender',
      reason: "Gmail's spam verdict and a failed DMARC check.",
      mechanism: 'gate',
      shadow: true,
    },
    // --- Filed (receipts, notifications, family, travel) ---
    {
      id: 'm_pers_amazon',
      dig: DIG_PERSONAL,
      from: { email: 'auto-confirm@amazon.example', name: 'Amazon.co.uk' },
      to: [ME[DIG_PERSONAL]],
      subject: 'Your order of "Ubiquiti U6 Lite access point" has been dispatched',
      snippet: 'Order #204-5581923-1120314. Total £89.00. Arriving tomorrow by 9pm.',
      text: 'Order #204-5581923-1120314\n\nUbiquiti U6 Lite access point — £89.00\n\nArriving tomorrow by 9pm.',
      html: '<p>Order <b>#204-5581923-1120314</b></p><table><tr><td>Ubiquiti U6 Lite access point</td><td>£89.00</td></tr></table><p>Arriving tomorrow by 9pm.</p><img src="https://amazon.example/track/px.gif">',
      hoursAgo: 14,
      labels: ['INBOX', 'CATEGORY_UPDATES', 'STARRED'],
      disposition: { kind: 'filed' },
      strata: ['Purchases', 'Receipts'],
      facts: { senderDomain: 'amazon.example', autoSubmitted: true, hasStructuredReceipt: true, gmailCategory: 'updates' },
      signals: { receipt: 0.98, routine: 0.9, personally_relevant: 0.5, requires_response: 0.02 },
      meshId: 'mesh_receipts',
      meshVersion: 2,
      matchedRule: 'IF receipt@v2 > .85\n    → Filed + Stratum: Purchases',
      reason: 'A receipt: Filed into Purchases by the Receipts Mesh.',
    },
    {
      id: 'm_cons_stripe',
      dig: DIG_CONSULTING,
      from: { email: 'receipts@stripe.example', name: 'Stripe' },
      to: [ME[DIG_CONSULTING]],
      subject: 'Your receipt from Purdon Consulting Ltd (#1842-9921)',
      snippet: 'Amount paid £45.00. Notion Team plan, monthly.',
      text: 'Receipt #1842-9921\n\nNotion Team plan, monthly — £45.00\n\nPaid with card ending 4242.',
      hoursAgo: 3 * 24 + 6,
      labels: ['INBOX', 'CATEGORY_UPDATES'],
      disposition: { kind: 'filed' },
      strata: ['Purchases', 'Receipts', 'Finance'],
      facts: { senderDomain: 'stripe.example', autoSubmitted: true, hasStructuredReceipt: true, gmailCategory: 'updates' },
      signals: { receipt: 0.99, routine: 0.92, personally_relevant: 0.4, requires_response: 0.01 },
      meshId: 'mesh_receipts',
      meshVersion: 2,
      matchedRule: 'IF receipt@v2 > .85\n    → Filed + Stratum: Purchases',
      reason: 'A receipt: Filed into Purchases by the Receipts Mesh.',
      shadow: true,
    },
    {
      id: 'm_pers_mum',
      dig: DIG_PERSONAL,
      from: { email: 'mum@purdon-family.example', name: 'Mum' },
      to: [ME[DIG_PERSONAL]],
      subject: 'Sunday',
      snippet: 'Are you still coming Sunday? Sarah is bringing the kids. Bring the good coffee.',
      text: 'Are you still coming Sunday? Sarah is bringing the kids. Bring the good coffee.\n\nMum x',
      hoursAgo: 22,
      labels: ['INBOX', 'CATEGORY_PERSONAL'],
      disposition: { kind: 'filed' },
      strata: ['People'],
      facts: { senderDomain: 'purdon-family.example', senderInContacts: true, senderRepliedToBefore: true, directlyAddressed: true, senderInNeverHidden: true, gmailCategory: 'personal', myLastReplyAt: ago(21) },
      signals: { requires_response: 0.85, personally_relevant: 0.99, routine: 0.02, urgency: 0.2 },
      meshId: null,
      matchedRule: null,
      reason: 'You replied 21 hours ago from your phone; nothing is waiting.',
      mechanism: 'deterministic',
    },
    {
      id: 'm_pers_easyjet',
      dig: DIG_PERSONAL,
      from: { email: 'noreply@easyjet.example', name: 'easyJet' },
      to: [ME[DIG_PERSONAL]],
      subject: 'Booking confirmed: LGW → LIS, 14 Oct',
      snippet: 'Booking reference K7QP2L. Flight EZY8721 departs London Gatwick 07:15, arrives Lisbon 10:05.',
      text: 'Booking reference K7QP2L\n\nEZY8721 London Gatwick 07:15 → Lisbon 10:05, 14 Oct\n\nManage your booking online.',
      hoursAgo: 5 * 24,
      labels: ['INBOX', 'CATEGORY_UPDATES'],
      disposition: { kind: 'filed' },
      strata: ['Travel', 'Purchases'],
      facts: { senderDomain: 'easyjet.example', autoSubmitted: true, hasStructuredReceipt: true, hasCalendarPart: true, gmailCategory: 'updates' },
      signals: { receipt: 0.9, routine: 0.8, personally_relevant: 0.7, requires_response: 0.02 },
      meshId: 'mesh_receipts',
      meshVersion: 2,
      matchedRule: 'IF receipt@v2 > .85\n    → Filed + Stratum: Purchases',
      reason: 'A booking confirmation: Filed into Travel and Purchases.',
    },
    {
      id: 'm_work_pagerduty',
      dig: DIG_WORK,
      from: { email: 'alerts@pagerduty.example', name: 'PagerDuty' },
      to: [{ email: 'platform-team@northwind.example', name: 'Platform team' }],
      subject: '[RESOLVED] #P-1182 projection-outbox latency > 30s',
      snippet: 'Incident resolved by auto-recovery after 6 minutes. No action required.',
      text: 'Incident #P-1182 projection-outbox latency > 30s\n\nResolved by auto-recovery after 6 minutes. No action required.',
      hoursAgo: 6,
      labels: ['INBOX', 'CATEGORY_UPDATES'],
      disposition: { kind: 'filed' },
      strata: ['Engineering', 'Notifications'],
      facts: { senderDomain: 'pagerduty.example', autoSubmitted: true, ccOnly: false, gmailCategory: 'updates' },
      signals: { routine: 0.8, personally_relevant: 0.45, requires_response: 0.05, ci_failure: 0.1 },
      meshId: null,
      matchedRule: null,
      reason: 'A resolved incident for your team: worth having, nothing to do.',
      mechanism: 'jev',
    },
    {
      id: 'm_work_1password',
      dig: DIG_WORK,
      from: { email: 'security@github.example', name: 'GitHub Security' },
      to: [ME[DIG_WORK]],
      subject: '[GitHub] A new SSH key was added to your account',
      snippet: 'A new SSH public key was added to your account. If this was you, no action is needed.',
      text: 'A new SSH public key was added to your account (matthew-nw-laptop). If this was you, no action is needed.',
      hoursAgo: 28,
      labels: ['INBOX', 'CATEGORY_UPDATES'],
      disposition: { kind: 'filed' },
      strata: ['Notifications'],
      facts: { senderDomain: 'github.example', autoSubmitted: true, senderInNeverHidden: true, gmailCategory: 'updates' },
      signals: { routine: 0.7, personally_relevant: 0.85, requires_response: 0.05 },
      meshId: null,
      matchedRule: null,
      gate: 'security alert: never hidden',
      reason: 'A security alert. Never hidden, nothing to do.',
      mechanism: 'gate',
    },
    {
      id: 'm_pers_reno_1',
      dig: DIG_PERSONAL,
      thread: 't_pers_reno',
      from: { email: 'dave@hearthbuilders.example', name: 'Dave Hearth' },
      to: [ME[DIG_PERSONAL]],
      subject: 'Kitchen: revised quote and timeline',
      snippet: 'Revised quote attached with the island moved 40cm. Timeline slips one week because of the electrician.',
      text: 'Revised quote attached with the island moved 40cm. Timeline slips one week because of the electrician.\n\nDave',
      hoursAgo: 2 * 24 + 8,
      labels: ['INBOX', 'CATEGORY_PERSONAL'],
      attachments: [{ filename: 'kitchen-quote-v3.pdf', mimeType: 'application/pdf', sizeBytes: 402_112 }],
      disposition: { kind: 'filed' },
      strata: ['Renovation', 'People'],
      facts: { senderDomain: 'hearthbuilders.example', senderInContacts: true, senderRepliedToBefore: true, directlyAddressed: true, hasAttachments: true, gmailCategory: 'personal', myLastReplyAt: ago(2 * 24) },
      signals: { requires_response: 0.6, personally_relevant: 0.95, routine: 0.05, payment_due: 0.3 },
      meshId: null,
      matchedRule: null,
      reason: 'You replied; Filed into Renovation.',
      mechanism: 'deterministic',
    },
    {
      id: 'm_pers_reno_2',
      dig: DIG_PERSONAL,
      thread: 't_pers_reno',
      from: ME[DIG_PERSONAL],
      to: [{ email: 'dave@hearthbuilders.example', name: 'Dave Hearth' }],
      subject: 'Re: Kitchen: revised quote and timeline',
      snippet: 'Thanks Dave. The island move is fine. Can you confirm the electrician date before I sign?',
      text: 'Thanks Dave. The island move is fine. Can you confirm the electrician date before I sign?\n\nMatt',
      hoursAgo: 2 * 24,
      labels: ['SENT', 'CATEGORY_PERSONAL'],
      sent: true,
      disposition: { kind: 'filed' },
      strata: ['Renovation', 'People'],
      facts: { senderDomain: 'gmail.example', isSent: true, threadLength: 2 },
      signals: { commitment_made: 0.15, requires_response: 0.03, personally_relevant: 0.9 },
      meshId: null,
      matchedRule: null,
      reason: 'Sent mail: you asked Dave a question.',
      mechanism: 'deterministic',
    },
    {
      id: 'm_pers_reno_3',
      dig: DIG_PERSONAL,
      thread: 't_pers_reno',
      from: { email: 'dave@hearthbuilders.example', name: 'Dave Hearth' },
      to: [ME[DIG_PERSONAL]],
      subject: 'Re: Kitchen: revised quote and timeline',
      snippet: 'Electrician is pencilled for the 6th. Will confirm Monday.',
      text: 'Electrician is pencilled for the 6th. Will confirm Monday.\n\nDave',
      hoursAgo: 1.5 * 24,
      labels: ['INBOX', 'CATEGORY_PERSONAL'],
      disposition: { kind: 'filed' },
      strata: ['Renovation', 'People'],
      facts: { senderDomain: 'hearthbuilders.example', senderInContacts: true, senderRepliedToBefore: true, directlyAddressed: true, threadLength: 3, gmailCategory: 'personal' },
      signals: { requires_response: 0.25, personally_relevant: 0.95, routine: 0.05 },
      meshId: null,
      matchedRule: null,
      reason: 'Dave answered; nothing is waiting on you until Monday.',
      mechanism: 'jev',
    },
    // --- Sent, filed ---
    {
      id: 'm_cons_q3',
      dig: DIG_CONSULTING,
      from: ME[DIG_CONSULTING],
      to: [{ email: 'accountant@ledgerly.example', name: 'Dana Whitfield (Ledgerly)' }],
      subject: 'Re: Q3 statements',
      snippet: 'Statements attached. I will send the September receipts by Friday.',
      text: 'Statements attached. I will send the September receipts by Friday.\n\nMatthew',
      hoursAgo: 3 * 24 + 2,
      labels: ['SENT'],
      sent: true,
      disposition: { kind: 'filed' },
      strata: ['Finance'],
      facts: { senderDomain: 'purdon-consulting.example', isSent: true },
      signals: { commitment_made: 0.92, requires_response: 0.02, personally_relevant: 0.9 },
      meshId: null,
      matchedRule: null,
      reason: 'Sent mail with a commitment: "by Friday".',
      mechanism: 'deterministic',
      shadow: true,
    },
    // --- Unsifted (arrived during catch-up) ---
    {
      id: 'm_work_new_1',
      dig: DIG_WORK,
      from: { email: 'hr@northwind.example', name: 'Northwind People' },
      to: [{ email: 'eng-all@northwind.example', name: 'Engineering' }],
      subject: 'Benefits enrolment closes 30 September',
      snippet: 'Open enrolment for 2027 benefits closes at the end of the month. No change is needed to keep your current plan.',
      text: 'Open enrolment for 2027 benefits closes at the end of the month. No change is needed to keep your current plan.',
      hoursAgo: 0.2,
      labels: ['INBOX'],
      disposition: null,
      strata: [],
      facts: { senderDomain: 'northwind.example', sameOrganisation: true, senderInNeverHidden: true },
      signals: {},
    },
    {
      id: 'm_work_new_2',
      dig: DIG_WORK,
      from: gh('GitHub'),
      subject: '[northwind/platform] Run failed: CI on main (a91f2c3)',
      snippet: 'CI failed on main: test_projection_outbox_pauses_on_rate_limit. Triggered by matthew.',
      text: 'CI failed on main: test_projection_outbox_pauses_on_rate_limit.\n\nTriggered by matthew.\n\nhttps://github.example/northwind/platform/actions/runs/88213',
      hoursAgo: 0.1,
      labels: ['INBOX', 'CATEGORY_UPDATES'],
      disposition: null,
      strata: [],
      facts: { senderDomain: 'github.example', autoSubmitted: true, gmailCategory: 'updates' },
      signals: {},
    },
  ]

  // Fourteen routine GitHub notifications the GitHub Mesh v7 routed to Spoil.
  const ghSubjects: [string, string][] = [
    ['[northwind/platform] Run succeeded: CI on feature/outbox-budget', 'All 412 tests passed in 3m 41s.'],
    ['[northwind/platform] Sam Liu commented on #398', 'LGTM, one nit on naming.'],
    ['[northwind/platform] Merged #401: Bump tokio to 1.47', 'Dependabot merged by Jamie Okafor.'],
    ['[northwind/platform] Run succeeded: nightly on main', 'Nightly build green.'],
    ['[northwind/platform] Dependabot: bump serde_json from 1.0.140 to 1.0.141', 'Bumps serde_json to 1.0.141.'],
    ['[northwind/platform] New release: v2.31.0', 'Release notes: outbox retry budget, tray icon fixes.'],
    ['[northwind/platform] Sam Liu assigned #405 to Jamie Okafor', 'Issue #405: pause reason enum naming.'],
    ['[northwind/platform] Run succeeded: CI on dependabot/serde_json', 'All checks passed.'],
    ['[northwind/infra] Terraform plan: no changes', 'Plan for prod: 0 to add, 0 to change, 0 to destroy.'],
    ['[northwind/platform] Priya Raman closed #388', 'Closed as completed.'],
    ['[northwind/platform] Jamie Okafor pushed 3 commits to feature/outbox-budget', 'fix flaky test; rename PauseReason; docs.'],
    ['[northwind/infra] Run succeeded: apply on main', 'Applied 2 changes in staging.'],
    ['[northwind/platform] Sam Liu opened #409: Tray icon blurry on Retina', 'The tray icon renders blurry at 2x.'],
    ['[northwind/platform] Weekly digest: 14 PRs merged', '14 merged, 3 open, 2 stale.'],
  ]
  ghSubjects.forEach(([s, sn], i) => specs.push(spoilGh(i + 1, s, sn, 0.25 + i * 0.35)))

  // Twenty-three newsletters and promotions the Director moved to Spoil at the user's request.
  const nls: [string, Address, string, string][] = [
    [DIG_PERSONAL, { email: 'hello@morningbrew.example', name: 'Morning Brew' }, 'Rates hold, chips rally, and a very expensive banana', 'The Fed held rates again while chip stocks rallied on a surprise order.'],
    [DIG_PERSONAL, { email: 'news@theverge.example', name: 'The Verge' }, 'The best laptops of the year so far', 'We tested 24 laptops. Here is the short list.'],
    [DIG_WORK, { email: 'digest@hackernewsletter.example', name: 'Hacker Newsletter' }, 'Issue #712', 'SQLite as an application file format, again; a new Rust async runtime; why your CI is slow.'],
    [DIG_WORK, { email: 'updates@datadog.example', name: 'Datadog' }, 'What is new in Datadog: September', 'Logs pipelines, a redesigned APM, and cost insights.'],
    [DIG_PERSONAL, { email: 'deals@currys.example', name: 'Currys' }, 'Up to 40% off TVs this weekend only', 'Big screens, small prices. Ends Sunday.'],
    [DIG_PERSONAL, { email: 'noreply@strava.example', name: 'Strava' }, 'Your September in review', 'You ran 42km across 6 activities.'],
    [DIG_WORK, { email: 'newsletter@rustweekly.example', name: 'This Week in Rust' }, 'This Week in Rust 612', 'Rust 1.92 released; async closures stabilised; project spotlight.'],
    [DIG_PERSONAL, { email: 'team@notion.example', name: 'Notion' }, 'Introducing Notion Mail', 'A calmer inbox, built into your workspace.'],
    [DIG_CONSULTING, { email: 'marketing@hubspot.example', name: 'HubSpot' }, 'Q3 state of marketing report', 'What 1,400 marketers told us about AI.'],
    [DIG_PERSONAL, { email: 'noreply@medium.example', name: 'Medium Daily Digest' }, 'Stories for Matt: systems thinking, sourdough', 'Recommended reads based on your history.'],
    [DIG_WORK, { email: 'no-reply@linkedin.example', name: 'LinkedIn' }, 'You appeared in 14 searches this week', 'See who is looking at your profile.'],
    [DIG_PERSONAL, { email: 'offers@ocado.example', name: 'Ocado' }, 'Your weekend favourites are back in stock', 'Plus £15 off your next shop.'],
    [DIG_WORK, { email: 'hello@producthunt.example', name: 'Product Hunt Daily' }, 'Today: an AI that files your email', 'The top launches of the day.'],
    [DIG_PERSONAL, { email: 'newsletter@guardian.example', name: 'The Guardian' }, 'First Edition: the day in five stories', 'Your morning briefing.'],
    [DIG_CONSULTING, { email: 'team@xero.example', name: 'Xero' }, 'New in Xero: bank rules 2.0', 'Automate more of your reconciliation.'],
    [DIG_PERSONAL, { email: 'hello@substack.example', name: 'Substack' }, 'New from Craig Mod: Ridgeline #212', 'Walking the Kumano Kodo, again.'],
    [DIG_WORK, { email: 'noreply@aws-marketing.example', name: 'AWS Events' }, 'Register: re:Invent 2026', 'Early bird pricing ends soon.'],
    [DIG_PERSONAL, { email: 'noreply@spotify.example', name: 'Spotify' }, 'Your weekly discovery is ready', '30 new songs picked for you.'],
    [DIG_PERSONAL, { email: 'no-reply@ebay.example', name: 'eBay' }, 'Items you watched are ending soon', '3 items end today.'],
    [DIG_WORK, { email: 'digest@stackoverflow.example', name: 'Stack Overflow' }, 'The Overflow #241', 'Why do engineers hate meetings? And other questions.'],
    [DIG_PERSONAL, { email: 'hello@headspace.example', name: 'Headspace' }, 'A 3-minute reset for busy mornings', 'Breathe in. Breathe out.'],
    [DIG_CONSULTING, { email: 'news@gartner.example', name: 'Gartner' }, 'Top strategic technology trends', 'Download the report.'],
    [DIG_PERSONAL, { email: 'noreply@uber.example', name: 'Uber Eats' }, '20% off your next three orders', 'Use code HUNGRY20.'],
  ]
  nls.forEach(([dig, from, s, sn], i) => specs.push(newsletter(i + 1, dig, from, s, sn, 1.2 + i * 0.9)))

  return specs
}

// ---------------------------------------------------------------------------
// Signals registry

interface SignalSpec {
  name: string
  versions: number
  status: SignalStatus
  definition: string
  primitive?: 'choice' | 'noul'
  choices?: { name: string; definition: string }[]
  inputs?: { message: boolean; thread: boolean; contextSlices: string[] }
  createdBy?: 'system' | 'user' | 'director'
  notes?: Record<number, string>
  agreement?: number
}

const SIGNAL_SPECS: SignalSpec[] = [
  { name: 'review_requested', versions: 3, status: 'active', definition: 'The message asks the user, by name or by assignment, to review a change, document or proposal.', notes: { 2: 'Exclude "cc for visibility" mentions.', 3: 'Count assignment through a review tool as a request.' }, agreement: 0.972 },
  { name: 'ci_failure', versions: 4, status: 'active', definition: 'The message reports a failed automated build, test run or deployment.', notes: { 2: 'Treat "cancelled" as not a failure.', 3: 'Include flaky retries that ultimately failed.', 4: 'Exclude successes that mention earlier failures.' }, agreement: 0.981 },
  { name: 'personally_relevant', versions: 3, status: 'active', definition: 'The message concerns the user specifically: addressed to them, about their work, their commitments or their money, rather than a broadcast.', inputs: { message: true, thread: false, contextSlices: ['user_profile', 'sender_relationship', 'active_projects'] }, notes: { 2: 'Use the user profile for aliases and group addresses.', 3: 'Active projects count as personally relevant.' }, agreement: 0.931 },
  { name: 'routine', versions: 1, status: 'active', definition: 'The message is an automated, repetitive or informational notification that needs no action.', agreement: 0.958 },
  { name: 'requires_response', versions: 3, status: 'active', definition: 'A person expects the user to reply.', inputs: { message: true, thread: true, contextSlices: ['sender_relationship', 'open_asks', 'thread_state'] }, notes: { 2: 'Consider thread state: an ask already answered does not require a response.', 3: 'A question in a sent message does not require the user to respond.' }, agreement: 0.944 },
  { name: 'urgency', versions: 2, status: 'active', primitive: 'choice', definition: 'How time-critical the message is, from what it asks rather than how it is worded.', choices: [{ name: 'NONE', definition: 'No time constraint.' }, { name: 'SOFT', definition: 'A date or preference, days away.' }, { name: 'HARD', definition: 'A deadline within a day or a consequence stated.' }], notes: { 2: 'Marketing urgency language is not urgency.' }, agreement: 0.902 },
  { name: 'billing_alert', versions: 1, status: 'active', definition: 'An automated alert about cost, budget or a charge that deviates from the expected amount.', createdBy: 'director', agreement: 0.965 },
  { name: 'receipt', versions: 2, status: 'active', definition: 'The message confirms a purchase, payment or booking already made.', notes: { 2: 'Exclude invoices that are still due.' }, agreement: 0.988 },
  { name: 'newsletter', versions: 1, status: 'active', definition: 'A periodical sent to a list, editorial or promotional, not addressed to the user individually.', agreement: 0.976 },
  { name: 'payment_due', versions: 2, status: 'active', definition: 'The user is asked to pay an amount by a date.', notes: { 2: 'Auto-pay confirmations are not due payments.' }, agreement: 0.951 },
  { name: 'commitment_made', versions: 1, status: 'experimental', definition: 'In a sent message, the user promises to do something, with or without a date.', createdBy: 'director', agreement: 0.87 },
  { name: 'waiting_on_me', versions: 1, status: 'proposed', definition: 'Someone has asked the user a question and the user has not answered it.', createdBy: 'director' },
]

export function buildSignals(): Signal[] {
  const out: Signal[] = []
  for (const s of SIGNAL_SPECS) {
    for (let v = 1; v <= s.versions; v++) {
      const latest = v === s.versions
      const status: SignalStatus = latest ? s.status : 'deprecated'
      const choices = s.primitive === 'choice' ? s.choices! : [{ name: 'YES', definition: `Yes: ${s.definition}` }, { name: 'NO', definition: 'No.' }]
      out.push({
        id: `sig_${s.name}`,
        name: s.name,
        version: v,
        primitive: s.primitive ?? 'noul',
        definition: v === s.versions ? s.definition : `${s.definition} (v${v} wording)`,
        choices,
        inputs: s.inputs ?? { message: true, thread: false, contextSlices: [] },
        languageScope: [],
        status,
        calibration:
          status === 'proposed'
            ? null
            : { method: 'platt', labelledCount: 380 + v * 120, engineIdentity: 'jev:jev-1.13.0', calibratedAt: daysAgo(30 - v * 3), plattA: 1.12 + v * 0.05, plattB: -0.08, heldOutAgreement: s.agreement ? s.agreement - (s.versions - v) * 0.02 : null },
        createdAt: daysAgo(60 - v * 10),
        createdBy: s.createdBy ?? 'system',
        changeNote: s.notes?.[v] ?? (v === 1 ? 'Initial definition.' : null),
      })
    }
  }
  return out
}

export function activeVersion(signals: Signal[], name: string): number {
  const vs = signals.filter((s) => s.name === name)
  const active = vs.find((s) => s.status === 'active' || s.status === 'experimental')
  return active?.version ?? Math.max(...vs.map((s) => s.version), 1)
}

// ---------------------------------------------------------------------------
// Meshes, in the §2 DSL

export const MESH_GITHUB_V7 = `Mesh: GitHub Notifications v7

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
`

export const MESH_GITHUB_V6 = `Mesh: GitHub Notifications v6

Scope:
Work Digs

ROUTES

IF review_requested@v2 > .80
    → Find: Review

ELSE IF ci_failure@v3 > .85
    → Find: Do

ELSE IF routine@v1 > .90
    → Spoil

ELSE
    → Escalate

GUARDS

NEVER Spoil IF sender IN never_hidden
`

export const MESH_RECEIPTS_V2 = `Mesh: Receipts v2

Scope:
All Digs

ROUTES

IF receipt@v2 > .85
    → Filed + Stratum: Purchases

ELSE IF fact has_structured_receipt = true
    → Filed + Stratum: Receipts

GUARDS

NEVER Spoil IF fact has_structured_receipt = true
`

export const MESH_FAMILY_V1 = `Mesh: Family Never Spoil v1

Scope:
Dig: Personal

GUARDS

NEVER Spoil IF sender IN family
NEVER Contamination IF sender IN family
`

export const MESH_AWS_V3 = `Mesh: AWS Notifications v3

Scope:
Work Digs

ROUTES

IF billing_alert@v1 > .80
    → Find: Know + Notify

ELSE IF routine@v1 > .85
    → Spoil

ELSE
    → Escalate

GUARDS

NEVER Spoil IF sender IN never_hidden
`

export const MESH_JIRA_V2 = `Mesh: Jira Digest v2

Scope:
Work Digs

ROUTES

IF routine@v1 > .90
    → Spoil

ELSE
    → Escalate
`

interface MeshSpec {
  id: string
  versions: { version: number; text: string; note: string | null; createdAt: string; createdBy: 'system' | 'user' | 'director' }[]
  status: MeshStatus
  priority: number
  shadowStartedAt: string | null
  shadowAgreed: number
  shadowProposed: number
  /** `Autonomy:` header cap, if the fixture text carries one. */
  autonomy?: Autonomy | null
  deletedAt: string | null
}

function olderVersion(text: string, v: number, extra: string): string {
  return text.replace(/ v\d+/, ` v${v}`).replace('GUARDS', `${extra}\nGUARDS`)
}

const MESH_SPECS: MeshSpec[] = [
  {
    id: 'mesh_github',
    status: 'active',
    priority: 10,
    shadowStartedAt: daysAgo(20),
    shadowAgreed: 212,
    shadowProposed: 219,
    deletedAt: null,
    versions: [
      { version: 1, text: MESH_GITHUB_V6.replace(' v6', ' v1').replace('review_requested@v2', 'review_requested@v1').replace('ci_failure@v3', 'ci_failure@v1'), note: 'Created from your first three corrections of GitHub mail.', createdAt: daysAgo(20), createdBy: 'director' },
      { version: 2, text: MESH_GITHUB_V6.replace(' v6', ' v2').replace('review_requested@v2', 'review_requested@v1').replace('ci_failure@v3', 'ci_failure@v2'), note: 'Pin ci_failure v2.', createdAt: daysAgo(18), createdBy: 'system' },
      { version: 3, text: MESH_GITHUB_V6.replace(' v6', ' v3').replace('ci_failure@v3', 'ci_failure@v2'), note: 'Pin review_requested v2.', createdAt: daysAgo(16), createdBy: 'system' },
      { version: 4, text: olderVersion(MESH_GITHUB_V6, 4, '# no guard on Spoil yet\n'), note: 'Added the Escalate fallback.', createdAt: daysAgo(12), createdBy: 'user' },
      { version: 5, text: MESH_GITHUB_V6.replace(' v6', ' v5'), note: 'Guard: never Spoil a never-hidden sender.', createdAt: daysAgo(9), createdBy: 'user' },
      { version: 6, text: MESH_GITHUB_V6, note: 'Pin ci_failure v3.', createdAt: daysAgo(6), createdBy: 'system' },
      { version: 7, text: MESH_GITHUB_V7, note: 'Only Spoil when not personally relevant; require relevance for CI failures.', createdAt: daysAgo(2), createdBy: 'user' },
    ],
  },
  {
    id: 'mesh_receipts',
    status: 'active',
    priority: 20,
    shadowStartedAt: daysAgo(35),
    shadowAgreed: 190,
    shadowProposed: 191,
    deletedAt: null,
    versions: [
      { version: 1, text: MESH_RECEIPTS_V2.replace(' v2', ' v1').replace('receipt@v2', 'receipt@v1'), note: 'Default Mesh.', createdAt: daysAgo(40), createdBy: 'system' },
      { version: 2, text: MESH_RECEIPTS_V2, note: 'Pin receipt v2, which excludes invoices still due.', createdAt: daysAgo(14), createdBy: 'system' },
    ],
  },
  {
    id: 'mesh_family',
    status: 'active',
    priority: 0,
    shadowStartedAt: daysAgo(40),
    shadowAgreed: 44,
    shadowProposed: 44,
    deletedAt: null,
    versions: [{ version: 1, text: MESH_FAMILY_V1, note: 'From your profile: family is never Spoil.', createdAt: daysAgo(40), createdBy: 'system' }],
  },
  {
    id: 'mesh_aws',
    status: 'shadow',
    priority: 15,
    shadowStartedAt: daysAgo(3),
    shadowAgreed: 18,
    shadowProposed: 20,
    deletedAt: null,
    versions: [
      { version: 1, text: MESH_AWS_V3.replace(' v3', ' v1').replace('    → Find: Know + Notify\n\nELSE IF routine@v1 > .85\n    → Spoil', '    → Find: Know\n\nELSE IF routine@v1 > .85\n    → Spoil'), note: 'Proposed by the Director after your AWS corrections.', createdAt: daysAgo(3), createdBy: 'director' },
      { version: 2, text: MESH_AWS_V3.replace(' v3', ' v2').replace(' + Notify', ''), note: 'Escalate the tail instead of Spoil.', createdAt: daysAgo(2), createdBy: 'user' },
      { version: 3, text: MESH_AWS_V3, note: 'Notify on billing alerts.', createdAt: daysAgo(1), createdBy: 'user' },
    ],
  },
  {
    id: 'mesh_jira',
    status: 'deleted',
    priority: 30,
    shadowStartedAt: daysAgo(30),
    shadowAgreed: 61,
    shadowProposed: 70,
    deletedAt: daysAgo(4),
    versions: [
      { version: 1, text: MESH_JIRA_V2.replace(' v2', ' v1'), note: 'Jira digests are routine.', createdAt: daysAgo(30), createdBy: 'user' },
      { version: 2, text: MESH_JIRA_V2, note: 'Escalate instead of Filed.', createdAt: daysAgo(25), createdBy: 'user' },
    ],
  },
]

export function buildMeshes(digIdByName: (n: string) => string | null): Mesh[] {
  const out: Mesh[] = []
  for (const spec of MESH_SPECS) {
    for (const v of spec.versions) {
      const parsed = parseMeshText(v.text, { digIdByName })
      out.push({
        id: spec.id,
        name: parsed.name,
        version: v.version,
        scope: parsed.scope,
        priority: spec.priority,
        routes: parsed.routes,
        guards: parsed.guards,
        status: spec.status,
        sourceText: v.text,
        createdAt: v.createdAt,
        createdBy: v.createdBy,
        changeNote: v.note,
        shadowStartedAt: spec.shadowStartedAt,
        shadowAgreed: spec.shadowAgreed,
        shadowProposed: spec.shadowProposed,
        deletedAt: spec.deletedAt,
        autonomy: spec.autonomy ?? undefined,
        fallback: false,
      })
    }
  }
  return out
}

export const scopeFor = (kind: MeshScope['kind'], value?: string): MeshScope =>
  kind === 'all_digs' ? { kind: 'all_digs' } : kind === 'dig_group' ? { kind: 'dig_group', group: value! } : { kind: 'dig', dig_id: value! }

// ---------------------------------------------------------------------------
// Strata

export function buildStrata(): Stratum[] {
  const mk = (id: string, name: string, scopeGroup: string | null, description: string, count: number, proposed = true): Stratum => ({
    id,
    name,
    version: 1,
    scopeGroup,
    description,
    proposedByRiddle: proposed,
    messageCount: u64(count),
    createdAt: daysAgo(30),
    deletedAt: null,
  })
  return [
    mk('st_purchases', 'Purchases', null, 'Receipts and order confirmations.', 212),
    mk('st_receipts', 'Receipts', null, 'Structured receipts; overlaps Purchases.', 187),
    mk('st_travel', 'Travel', null, 'Bookings, itineraries and changes.', 34),
    mk('st_finance', 'Finance', null, 'Invoices, statements, budgets.', 58),
    mk('st_engineering', 'Engineering', 'Work', 'CI, reviews, incidents, releases.', 421),
    mk('st_notifications', 'Notifications', null, 'Automated notifications from services.', 890),
    mk('st_newsletters', 'Newsletters', null, 'Periodicals sent to a list.', 143),
    mk('st_needs_response', 'Needs Response', null, 'Someone is waiting on you.', 12),
    mk('st_projects', 'Projects', 'Work', 'Active project threads.', 77),
    mk('st_people', 'People', null, 'Personal correspondence.', 305),
    mk('st_renovation', 'Renovation', 'Personal', 'The kitchen and the extension.', 41, false),
  ]
}

// ---------------------------------------------------------------------------
// Finds

interface FindSpec {
  id: string
  type: FindType
  title: string
  summary: string
  priority: number
  status: Find['status']
  dueAt?: string | null
  snoozedUntil?: string | null
  createdAt: string
  resolvedAt?: string | null
  resolvedBy?: Find['resolvedBy']
  reason: string
  confidence: number | null
}

const FIND_SPECS: FindSpec[] = [
  { id: 'find_review_412', type: 'review', title: 'Review requested on PR #412: retry budget for the projection outbox', summary: 'Jamie needs your eyes on the pause-reason enum before merging.', priority: 82, status: 'open', createdAt: ago(1.4), reason: 'Jamie requested your review directly; a follow-up says it is blocking the merge.', confidence: 0.96 },
  { id: 'find_respond_jeep', type: 'respond', title: 'Adam is waiting on you about the Jeep brakes', summary: 'Choose between the £310 rebuild and the £540 replacement; parts must be ordered by tomorrow noon.', priority: 74, status: 'open', dueAt: ahead(20), createdAt: ago(30), reason: 'Adam asked twice and gave a deadline; your last reply was a question back to him.', confidence: 0.97 },
  { id: 'find_pay_invoice', type: 'pay', title: 'Invoice #2024-117 from Ledgerly, £1,240, due Friday', summary: 'Q3 bookkeeping. Bank details unchanged.', priority: 76, status: 'open', dueAt: ahead(3 * 24), createdAt: ago(15), reason: 'An invoice with a due date from Dana, who is on your Never Hidden list.', confidence: 0.96 },
  { id: 'find_attend_dentist', type: 'attend', title: 'Hygienist appointment Thursday 09:30, Dr Amara', summary: 'Reply C to confirm or call to reschedule.', priority: 55, status: 'open', dueAt: ahead(2 * 24 + 1), createdAt: ago(9), reason: 'A dated appointment for you. Calendar mail is never hidden.', confidence: null },
  { id: 'find_decide_renewal', type: 'decide', title: 'Renew Datadog at 48 seats or drop to 32 before the 30th?', summary: 'Priya says 31 of 48 seats are in use. $21,600 at the current count.', priority: 68, status: 'open', dueAt: ahead(8 * 24), createdAt: ago(26), reason: 'A decision with a date, and Priya is waiting on it.', confidence: 0.9 },
  { id: 'find_do_deploy', type: 'do', title: 'Deploy the outbox hotfix before the Friday 17:00 change freeze', summary: 'Priya asked you to take the deploy once #412 merges.', priority: 61, status: 'open', dueAt: ahead(60), createdAt: ago(3.2), reason: 'No Mesh matched; the LLM read it as an action request from Priya with a deadline.', confidence: 0.84 },
  { id: 'find_followup_harbor', type: 'follow_up', title: 'Harbor Freight proposal: no reply for 9 days', summary: 'You sent the scheduling audit proposal to Lena Ortiz. Nothing since.', priority: 48, status: 'open', createdAt: ago(2 * 24), reason: 'Temporal Sift: a sent proposal with no reply after 7 days.', confidence: null },
  { id: 'find_know_aws', type: 'know', title: 'AWS Budgets: northwind-prod is $412 above forecast', summary: 'Actual $6,812 against a $6,400 forecast.', priority: 40, status: 'snoozed', snoozedUntil: ahead(24), createdAt: ago(0.9), reason: 'You changed it from Spoil to Find: Know.', confidence: null },
  { id: 'find_resolved_offsite', type: 'respond', title: 'Sam asked you to own the offsite architecture session', summary: 'Needed a yes by Wednesday.', priority: 66, status: 'resolved', createdAt: ago(2 * 24 + 3), resolvedAt: ago(5), resolvedBy: 'thread_state', reason: 'Resolved by your reply from Gmail on the web.', confidence: 0.92 },
  { id: 'find_dismissed_webinar', type: 'attend', title: 'Webinar: observability at scale, Tuesday 16:00', summary: 'Datadog invited you to a webinar.', priority: 30, status: 'dismissed', createdAt: ago(3 * 24), resolvedAt: ago(2 * 24 + 20), resolvedBy: 'user', reason: 'You said not important.', confidence: 0.7 },
]

export function buildFinds(messages: Message[], specs: MessageSpec[]): Find[] {
  return FIND_SPECS.map((f) => {
    const sources = specs.filter((s) => s.findId === f.id).map((s) => {
      const m = messages.find((x) => x.id === s.id)!
      return { messageId: m.id, threadId: m.threadId, digId: m.digId }
    })
    const digIds = [...new Set(sources.map((s) => s.digId))]
    const first = specs.find((s) => s.findId === f.id)
    return {
      id: f.id,
      findType: f.type,
      title: f.title,
      summary: f.summary,
      priority: f.priority,
      status: f.status,
      dueAt: f.dueAt ?? null,
      snoozedUntil: f.snoozedUntil ?? null,
      createdAt: f.createdAt,
      updatedAt: f.resolvedAt ?? f.createdAt,
      resolvedAt: f.resolvedAt ?? null,
      resolvedBy: f.resolvedBy ?? null,
      sources,
      digIds: digIds.length ? digIds : [DIG_WORK],
      identityKey: sources[0]?.threadId ?? f.id,
      reason: f.reason,
      confidence: f.confidence,
      classificationId: first ? `cls_${first.id}` : null,
      traceId: first ? `tr_${first.id}` : null,
      supersededBy: null, notify: false,
    }
  })
}

// ---------------------------------------------------------------------------
// Messages → Message objects, traces

export function buildMessage(s: MessageSpec): Message {
  const digOwner = ME[s.dig]
  const from = s.sent ? digOwner : s.from
  const to = s.to ?? [digOwner]
  const date = ago(s.hoursAgo)
  const domain = from.email.split('@')[1]
  const auth = s.facts?.spfPass === false || s.facts?.dmarcPass === false ? { spf: s.facts?.spfPass === false ? ('fail' as const) : ('pass' as const), dkim: s.facts?.dkimPass === false ? ('fail' as const) : ('pass' as const), dmarc: s.facts?.dmarcPass === false ? ('fail' as const) : ('pass' as const) } : { spf: 'pass' as const, dkim: 'pass' as const, dmarc: 'pass' as const }
  return {
    id: s.id,
    digId: s.dig,
    threadId: s.thread ?? `t_${s.id}`,
    providerId: `p_${s.id}`,
    subject: s.subject,
    from,
    to,
    cc: s.cc ?? [],
    bcc: [],
    date,
    snippet: s.snippet ?? s.text.split('\n')[0],
    labels: s.labels ?? ['INBOX'],
    headers: {
      extra: {},
      messageId: `<${s.id}@${domain}>`,
      inReplyTo: null,
      references: [],
      listId: s.facts?.mailingList ? `<list.${domain}>` : null,
      listUnsubscribe: s.facts?.listUnsubscribe ? `<https://${domain}/unsubscribe>` : null,
      precedence: s.facts?.precedenceBulk ? 'bulk' : null,
      autoSubmitted: s.facts?.autoSubmitted ? 'auto-generated' : null,
      replyTo: s.facts?.replyToMismatch ? { email: `reply@${domain.replace('alerts', 'collect')}`, name: null } : null,
      authentication: auth,
    },
    attachments: (s.attachments ?? []).map((a, i) => ({ filename: a.filename, mimeType: a.mimeType, sizeBytes: u64(a.sizeBytes), providerId: `att_${s.id}_${i}`, contentId: null })),
    sizeBytes: u64(s.text.length * 3 + 1800),
    isSent: !!s.sent,
    hasBody: true,
    bodyText: s.text,
    bodyHtml: s.html === undefined ? null : s.html,
    syncedAt: date,
  }
}

export function buildFacts(s: MessageSpec, from: Address): Facts {
  const base: Facts = {
    senderCadence: '',
    senderOnSchedule: false,
    governmentSender: false,
    fromMe: false,
    spoofsMe: false,
    isDraft: false,
    isReply: false,
    freeMailSender: false,
    glyphSubstitution: false,
    suspiciousDomain: false,
    senderDomain: from.email.split('@')[1],
    sender: from.email.toLowerCase(),
    senderName: from.name ?? '',
    subject: s.subject ?? '',
    headers: {},
    githubReason: null,
    githubEvent: null,
    listId: '',
    verificationCode: '',
    paymentFailed: false,
    paymentRecovered: false,
    senderInContacts: false,
    senderRepliedToBefore: false,
    firstTimeSender: false,
    sameOrganisation: false,
    mailingList: false,
    listUnsubscribe: false,
    precedenceBulk: false,
    autoSubmitted: false,
    spfPass: true,
    dkimPass: true,
    dmarcPass: true,
    replyToMismatch: false,
    gmailCategory: null,
    gmailImportant: false,
    gmailSpam: false,
    directlyAddressed: false,
    ccOnly: false,
    threadLength: 1,
    myLastReplyAt: null,
    hasCalendarPart: false,
    hasStructuredReceipt: false,
    hasAttachments: (s.attachments?.length ?? 0) > 0,
    language: s.language ?? 'en',
    urgencyLanguagePresent: false,
    asksForPaymentOrCredentials: false,
    deadlines: [],
    isSent: !!s.sent,
    lookalikeDomain: false,
    senderInNeverHidden: false,
    senderIsVip: false,
  }
  return { ...base, ...s.facts }
}

const SIGNAL_SET_FOR_MESH: Record<string, string[]> = {
  mesh_github: ['review_requested', 'ci_failure', 'personally_relevant', 'routine', 'requires_response'],
  mesh_receipts: ['receipt', 'routine', 'personally_relevant', 'requires_response'],
  mesh_aws: ['billing_alert', 'routine', 'personally_relevant', 'requires_response'],
}

export function buildEvaluations(s: MessageSpec, signals: Signal[], evaluatedAt: string): SignalEvaluation[] {
  const out: SignalEvaluation[] = []
  const names = Object.keys(s.signals ?? {})
  names.forEach((name, i) => {
    const p = s.signals![name]
    const sig = signals.find((x) => x.name === name && (x.status === 'active' || x.status === 'experimental')) ?? signals.find((x) => x.name === name)
    if (!sig) return
    let distribution: Record<string, number>
    let selected: string
    if (sig.primitive === 'choice') {
      // urgency: map p to a distribution over NONE/SOFT/HARD.
      const hard = Math.max(0, p - 0.5) * 2 * 0.9
      const soft = p * (1 - hard) * 0.9
      const none = Math.max(0.02, 1 - hard - soft)
      distribution = { NONE: round(none), SOFT: round(soft), HARD: round(hard) }
      selected = Object.entries(distribution).sort((a, b) => b[1] - a[1])[0][0]
    } else {
      distribution = { YES: round(p), NO: round(1 - p) }
      selected = p >= 0.5 ? 'YES' : 'NO'
    }
    const cached = s.fresh ? false : i % 4 !== 3 || (s.meshId === 'mesh_github' && s.id !== 'm_work_gh_9')
    out.push({
      messageId: s.id,
      signalName: name,
      signalVersion: sig.version,
      engineIdentity: 'jev:jev-1.13.0',
      selected,
      distribution,
      calibrated: distribution,
      confidence: Math.max(...Object.values(distribution)),
      latencyMs: u64(cached ? 0 : 140 + (i * 37) % 90),
      inputTokens: u64(cached ? 0 : 1800 + s.text.length),
      cached,
      deterministic: false,
      contextHash: `ctx_${s.id.slice(-6)}`,
      requestHash: `req_${name}_${sig.version}_${s.id.slice(-4)}`,
      evaluatedAt,
    })
  })
  return out
}

const round = (n: number) => Math.round(n * 100) / 100

export function buildTrace(s: MessageSpec, m: Message, signals: Signal[], meshes: Mesh[]): DecisionTrace | null {
  if (!s.disposition) return null
  const facts = buildFacts(s, m.from)
  const evals = buildEvaluations(s, signals, m.date)
  const mesh = s.meshId ? meshes.find((x) => x.id === s.meshId && x.version === (s.meshVersion ?? 1)) ?? null : null
  const judgeMs = evals.filter((e) => !e.cached).reduce((a, e) => a + Number(e.latencyMs), 0)
  const llmMs = s.escalated ? 1240 + (s.text.length % 400) : 0
  const factsMs = 3
  const meshMs = mesh ? 1 : 0
  const syncMs = 17 + (s.text.length % 20)
  const persistMs = 4
  const judgeTokens = evals.filter((e) => !e.cached).reduce((a, e) => a + Number(e.inputTokens), 0)
  const mechanism: DecisionTrace['mechanism'] = s.mechanism ?? (s.gate ? 'gate' : mesh ? 'mesh' : s.escalated ? 'llm' : evals.length ? 'jev' : 'deterministic')
  const setName = s.meshId ? SIGNAL_SET_FOR_MESH[s.meshId]?.join(', ') : evals.map((e) => e.signalName).join(', ')
  return {
    id: `tr_${s.id}`,
    messageId: s.id,
    digId: s.dig,
    objective: 'Determine the disposition of one message and what should happen next.',
    strategy: s.gate ? 'deterministic gate' : mesh ? `Signal Set [${setName}] → Mesh ${mesh.name} v${mesh.version}` : s.escalated ? `Signal Set [${setName}] → confidence policy → LLM escalation` : `Signal Set [${setName}] → confidence policy`,
    facts,
    gate: s.gate ?? null,
    signals: evals,
    meshId: mesh?.id ?? null,
    meshVersion: mesh?.version ?? null,
    meshName: mesh?.name ?? null,
    matchedRule: s.matchedRule ?? null,
    guardVetoes: [],
    escalation: s.escalated
      ? {
          reason: s.escalated.reason,
          modelId: 'bedrock:us.anthropic.claude-haiku-4-5',
          refined: evals.slice(0, 2).map((e) => ({ ...e, engineIdentity: 'bedrock:us.anthropic.claude-haiku-4-5', cached: false, distribution: e.selected === 'YES' ? { YES: 0.91, NO: 0.09 } : { YES: 0.12, NO: 0.88 }, calibrated: e.selected === 'YES' ? { YES: 0.91, NO: 0.09 } : { YES: 0.12, NO: 0.88 }, confidence: 0.9, latencyMs: u64(llmMs), inputTokens: u64(2900) })),
          rationale: s.escalated.rationale,
          latencyMs: u64(llmMs),
        }
      : null,
    conclusion: s.disposition,
    confidence: evals.length ? Math.max(...evals.map((e) => e.confidence)) : null,
    mechanism,
    timing: { syncMs: u64(syncMs), factsMs: u64(factsMs), judgeMs: u64(judgeMs), meshMs: u64(meshMs), llmMs: u64(llmMs), persistMs: u64(persistMs), totalMs: u64(syncMs + factsMs + judgeMs + meshMs + llmMs + persistMs) },
    cost: {
      judgeInputTokens: u64(judgeTokens),
      judgeQuestions: evals.filter((e) => !e.cached).length,
      judgeCostUsd: judgeTokens ? round4((judgeTokens / 1_000_000) * 0.042) : 0,
      llmInputTokens: u64(s.escalated ? 2900 : 0),
      llmOutputTokens: u64(s.escalated ? 180 : 0),
      llmCostUsd: s.escalated ? round4((2900 / 1_000_000) * 1 + (180 / 1_000_000) * 5) : 0,
    },
    judgeIdentity: evals.length ? 'jev:jev-1.13.0' : null,
    appVersion: '0.1.0',
    buildStamp: '0.1.0+3c0df07-dirty',
    shadow: !!s.shadow,
    createdAt: iso(new Date(new Date(m.date).getTime() + 209)),
  }
}

const round4 = (n: number) => Math.round(n * 10000) / 10000

// ---------------------------------------------------------------------------
// Jobs

export function buildJobs(): Job[] {
  return [
    {
      id: 'job_catchup_work',
      kind: 'catch_up',
      priority: 'catch_up',
      status: { state: 'running' },
      digId: DIG_WORK,
      label: 'Catch-up: Work',
      progress: { done: u64(3741), total: u64(4981), unit: 'messages', step: 'Fetching bodies, newest first', etaSeconds: u64(248) },
      checkpoint: { historyId: '8812231' },
      cancellable: true,
      attempts: 1,
      createdAt: ago(0.4),
      startedAt: ago(0.38),
      finishedAt: null,
      updatedAt: ago(0, 1),
    },
    {
      id: 'job_sim_aws',
      kind: 'mesh_simulation',
      priority: 'experiment',
      status: { state: 'queued' },
      digId: null,
      label: 'Simulate AWS Notifications v3 against 90 days',
      progress: { done: u64(0), total: null, unit: 'messages', step: 'Waiting for catch-up to finish', etaSeconds: null },
      checkpoint: null,
      cancellable: true,
      attempts: 0,
      createdAt: ago(0.2),
      startedAt: null,
      finishedAt: null,
      updatedAt: ago(0.2),
    },
    {
      id: 'job_projection_work',
      kind: 'projection',
      priority: 'projection',
      status: { state: 'paused', reason: 'waiting for catch-up' },
      digId: DIG_WORK,
      label: 'Projection: write 41 labels to Gmail',
      progress: { done: u64(0), total: u64(41), unit: 'decisions', step: 'Paused', etaSeconds: null },
      checkpoint: null,
      cancellable: false,
      attempts: 0,
      createdAt: ago(0.3),
      startedAt: null,
      finishedAt: null,
      updatedAt: ago(0.3),
    },
    {
      id: 'job_expedition_subs',
      kind: 'expedition',
      priority: 'interactive',
      status: { state: 'running' },
      digId: null,
      label: 'Expedition: unused subscriptions',
      progress: { done: u64(3), total: u64(6), unit: 'steps', step: 'Finding subsequent receipts', etaSeconds: u64(420) },
      checkpoint: { step: 3 },
      cancellable: true,
      attempts: 1,
      createdAt: ago(0.6),
      startedAt: ago(0.55),
      finishedAt: null,
      updatedAt: ago(0, 3),
    },
    {
      id: 'job_initial_consulting',
      kind: 'initial_sync',
      priority: 'catch_up',
      status: { state: 'completed' },
      digId: DIG_CONSULTING,
      label: 'Initial sync: Consulting',
      progress: { done: u64(2_118), total: u64(2_118), unit: 'messages', step: 'Done', etaSeconds: null },
      checkpoint: null,
      cancellable: false,
      attempts: 1,
      createdAt: daysAgo(2.4),
      startedAt: daysAgo(2.4),
      finishedAt: daysAgo(2.2),
      updatedAt: daysAgo(2.2),
    },
    {
      id: 'job_temporal',
      kind: 'temporal_sift',
      priority: 'backfill',
      status: { state: 'completed' },
      digId: null,
      label: 'Temporal Sift: stalled conversations',
      progress: { done: u64(312), total: u64(312), unit: 'threads', step: 'Done', etaSeconds: null },
      checkpoint: null,
      cancellable: false,
      attempts: 1,
      createdAt: ago(2),
      startedAt: ago(2),
      finishedAt: ago(1.9),
      updatedAt: ago(1.9),
    },
  ]
}

// ---------------------------------------------------------------------------
// Activity

export function buildActivity(specs: MessageSpec[]): Activity[] {
  const gh = specs.filter((s) => s.id.startsWith('m_work_gh_'))
  const nl = specs.filter((s) => s.id.includes('_nl_'))
  const refs = (ss: MessageSpec[]) => ss.map((s) => ({ kind: 'message' as const, id: s.id }))
  const a: Activity[] = [
    {
      id: 'act_sift_gh',
      actor: 'sift',
      action: 'move_to_spoil',
      summary: `Sift moved ${gh.length} messages to Spoil`,
      why: 'Routine GitHub notifications not addressed to you.',
      inputs: { digId: DIG_WORK, count: gh.length },
      affected: refs(gh),
      result: { status: 'ok' },
      provenance: { mechanism: 'mesh', meshId: 'mesh_github', meshVersion: 7, meshName: 'GitHub Notifications', traceId: 'tr_m_work_gh_1', note: `${gh.length - 1} matched the Mesh on cached Signals. 1 needed a fresh evaluation.` },
      undo: { state: 'undoable', restore: { kind: 'dispositions', items: gh.map((s) => ({ digId: s.dig, messageId: s.id, disposition: { kind: 'unresolved' } })) }, label: 'Restore 14 messages to Unresolved' },
      digId: DIG_WORK,
      traceId: 'tr_m_work_gh_1',
      undoes: null,
      createdAt: ago(0, 10),
    },
    {
      id: 'act_user_aws',
      actor: 'user',
      action: 'set_disposition',
      summary: 'You changed "AWS Budgets: northwind-prod is $412 above forecast" from Spoil to Find: Know',
      why: 'Riddle proposed an update to Mesh: AWS Notifications based on your correction.',
      inputs: { digId: DIG_WORK, messageId: 'm_work_aws', from: 'spoil', to: 'find:know' },
      affected: [{ kind: 'message', id: 'm_work_aws' }, { kind: 'find', id: 'find_know_aws' }],
      result: { status: 'ok' },
      provenance: { mechanism: 'user', meshId: 'mesh_aws', meshVersion: 3, meshName: 'AWS Notifications', traceId: 'tr_m_work_aws', note: 'Correction recorded: move (Spoil → Find). The Shadow Mesh disagreed.' },
      undo: { state: 'undoable', restore: { kind: 'dispositions', items: [{ digId: DIG_WORK, messageId: 'm_work_aws', disposition: { kind: 'spoil' } }] }, label: 'Put it back in Spoil and retract the correction' },
      digId: DIG_WORK,
      traceId: 'tr_m_work_aws',
      undoes: null,
      createdAt: ago(0, 25),
    },
    {
      id: 'act_director_nl',
      actor: 'director',
      action: 'move_to_spoil',
      summary: `Director moved ${nl.length} messages to Spoil`,
      why: 'You asked: "Take the newsletters I never open and treat them as Spoil."',
      inputs: { count: nl.length, digIds: [DIG_WORK, DIG_PERSONAL, DIG_CONSULTING] },
      affected: refs(nl),
      result: { status: 'partial', skipped: [{ entity: { kind: 'message', id: 'm_personal_nl_6' }, reason: 'you archived it by hand since' }, { entity: { kind: 'message', id: 'm_work_nl_3' }, reason: 'you opened and starred it since' }] },
      provenance: { mechanism: 'director', meshId: null, meshVersion: null, meshName: null, traceId: null, note: 'Director conversation: "What actually needs my attention this morning?"' },
      undo: { state: 'undoable', restore: { kind: 'dispositions', items: nl.map((s) => ({ digId: s.dig, messageId: s.id, disposition: { kind: 'filed' } })) }, label: `Restore ${nl.length} messages to Filed` },
      digId: null,
      traceId: null,
      undoes: null,
      createdAt: ago(1),
    },
    {
      id: 'act_user_spoil_1',
      actor: 'user',
      action: 'move_to_spoil',
      summary: 'You moved "[RESOLVED] #P-1182 projection-outbox latency > 30s" to Spoil',
      why: null,
      inputs: { digId: DIG_WORK, messageIds: ['m_work_pagerduty'] },
      affected: [{ kind: 'message', id: 'm_work_pagerduty' }],
      result: { status: 'ok' },
      provenance: null,
      undo: { state: 'undone', at: ago(1, 58), by: 'user', undo_activity_id: 'act_user_undo_1' },
      digId: DIG_WORK,
      traceId: null,
      undoes: null,
      createdAt: ago(2),
    },
    {
      id: 'act_user_undo_1',
      actor: 'user',
      action: 'undo_activity',
      summary: 'You undid: moved "[RESOLVED] #P-1182 projection-outbox latency > 30s" to Spoil',
      why: 'Restored to Filed. The correction it recorded was retracted.',
      inputs: { activityId: 'act_user_spoil_1' },
      affected: [{ kind: 'message', id: 'm_work_pagerduty' }],
      result: { status: 'ok' },
      provenance: null,
      undo: { state: 'not_undoable', reason: 'an undo is not undone; redo the action by hand' },
      digId: DIG_WORK,
      traceId: null,
      undoes: 'act_user_spoil_1',
      createdAt: ago(1, 58),
    },
    {
      id: 'act_director_mesh',
      actor: 'director',
      action: 'create_mesh',
      summary: 'Director created Mesh: AWS Notifications v1 (Shadow)',
      why: 'You corrected AWS billing alerts three times.',
      inputs: { meshId: 'mesh_aws', version: 1 },
      affected: [{ kind: 'mesh', id: 'mesh_aws' }],
      result: { status: 'ok' },
      provenance: { mechanism: 'director', meshId: 'mesh_aws', meshVersion: 1, meshName: 'AWS Notifications', traceId: null, note: 'Inducing evidence: 3 corrections across 2 senders; no quarantined mail.' },
      undo: { state: 'undoable', restore: { kind: 'mesh_deleted', meshId: 'mesh_aws' }, label: 'Move the Mesh to Recently Deleted' },
      digId: null,
      traceId: null,
      undoes: null,
      createdAt: daysAgo(3),
    },
    {
      id: 'act_system_resolve',
      actor: 'system',
      action: 'resolve_find',
      summary: 'Find resolved: "Sam asked you to own the offsite architecture session"',
      why: 'You replied from Gmail on the web; Thread State shows your reply.',
      inputs: { findId: 'find_resolved_offsite' },
      affected: [{ kind: 'find', id: 'find_resolved_offsite' }, { kind: 'thread', id: 't_work_offsite' }],
      result: { status: 'ok' },
      provenance: { mechanism: 'provider', meshId: null, meshVersion: null, meshName: null, traceId: null, note: 'Observed from Gmail: a sent message in the thread.' },
      undo: { state: 'not_undoable', reason: 'resolved by your own reply; nothing to restore' },
      digId: DIG_WORK,
      traceId: null,
      undoes: null,
      createdAt: ago(5),
    },
    {
      id: 'act_temporal_harbor',
      actor: 'sift',
      action: 'create_find',
      summary: 'Temporal Sift created Follow Up: "Harbor Freight proposal: no reply for 9 days"',
      why: 'A sent proposal with no reply after 7 days.',
      inputs: { threadId: 't_cons_harbor' },
      affected: [{ kind: 'find', id: 'find_followup_harbor' }, { kind: 'thread', id: 't_cons_harbor' }],
      result: { status: 'ok' },
      provenance: { mechanism: 'temporal', meshId: null, meshVersion: null, meshName: null, traceId: 'tr_m_cons_harbor_1', note: 'Follow Up Finds are made by time, not by mail.' },
      undo: { state: 'not_undoable', reason: 'dismiss the Find instead; that records a correction' },
      digId: DIG_CONSULTING,
      traceId: 'tr_m_cons_harbor_1',
      undoes: null,
      createdAt: ago(2 * 24),
    },
    {
      id: 'act_user_label',
      actor: 'user',
      action: 'label_messages',
      summary: 'You added Stratum: Renovation to 3 messages',
      why: null,
      inputs: { digId: DIG_PERSONAL, count: 3, strata: ['Renovation'] },
      affected: [{ kind: 'message', id: 'm_pers_reno_1' }, { kind: 'message', id: 'm_pers_reno_2' }, { kind: 'message', id: 'm_pers_reno_3' }],
      result: { status: 'ok' },
      provenance: null,
      undo: { state: 'undoable', restore: { kind: 'strata_remove', digId: DIG_PERSONAL, messageIds: ['m_pers_reno_1', 'm_pers_reno_2', 'm_pers_reno_3'], strata: ['Renovation'] }, label: 'Remove Renovation from 3 messages' },
      digId: DIG_PERSONAL,
      traceId: null,
      undoes: null,
      createdAt: daysAgo(1),
    },
    {
      id: 'act_user_mesh_v7',
      actor: 'user',
      action: 'update_mesh',
      summary: 'You updated Mesh: GitHub Notifications v6 → v7',
      why: 'Only Spoil when not personally relevant; require relevance for CI failures.',
      inputs: { meshId: 'mesh_github', fromVersion: 6, toVersion: 7 },
      affected: [{ kind: 'mesh', id: 'mesh_github' }],
      result: { status: 'ok' },
      provenance: { mechanism: 'user', meshId: 'mesh_github', meshVersion: 7, meshName: 'GitHub Notifications', traceId: null, note: 'Simulated against 90 days before applying: 439 changed, 0 new judge calls.' },
      undo: { state: 'undoable', restore: { kind: 'mesh_version', meshId: 'mesh_github', version: 6 }, label: 'Go back to v6' },
      digId: null,
      traceId: null,
      undoes: null,
      createdAt: daysAgo(2),
    },
    {
      id: 'act_projection',
      actor: 'automation',
      action: 'projection',
      summary: 'Projection wrote 41 labels to Gmail (Work)',
      why: 'Decisions made since the last projection pass.',
      inputs: { digId: DIG_WORK, count: 41 },
      affected: [],
      result: { status: 'ok' },
      provenance: { mechanism: 'provider', meshId: null, meshVersion: null, meshName: null, traceId: null, note: 'batchModify, 1 call, 41 ids.' },
      undo: { state: 'not_undoable', reason: 'projection follows decisions; undo the decision and projection follows' },
      digId: DIG_WORK,
      traceId: null,
      undoes: null,
      createdAt: daysAgo(2.1),
    },
    {
      id: 'act_user_never_hidden',
      actor: 'user',
      action: 'add_never_hidden',
      summary: 'You added accountant@ledgerly.example to Never Hidden',
      why: null,
      inputs: { entry: 'accountant@ledgerly.example' },
      affected: [{ kind: 'person', id: 'accountant@ledgerly.example' }],
      result: { status: 'ok' },
      provenance: null,
      undo: { state: 'undoable', restore: { kind: 'never_hidden_remove', entry: 'accountant@ledgerly.example' }, label: 'Remove from Never Hidden' },
      digId: null,
      traceId: null,
      undoes: null,
      createdAt: daysAgo(3),
    },
    {
      id: 'act_user_send',
      actor: 'user',
      action: 'send_message',
      summary: 'You sent "Re: Q3 statements" to Dana Whitfield (Ledgerly)',
      why: null,
      inputs: { digId: DIG_CONSULTING, messageId: 'm_cons_q3' },
      affected: [{ kind: 'message', id: 'm_cons_q3' }],
      result: { status: 'ok' },
      provenance: null,
      undo: { state: 'not_undoable', reason: 'sent mail cannot be unsent' },
      digId: DIG_CONSULTING,
      traceId: null,
      undoes: null,
      createdAt: daysAgo(3.1),
    },
  ]
  return a
}

// ---------------------------------------------------------------------------
// Conversations, Expeditions, Drafts

export function buildConversations(): Conversation[] {
  return [
    {
      id: 'conv_morning',
      title: 'What actually needs my attention this morning?',
      readUntrusted: true,
      digIds: [DIG_WORK, DIG_PERSONAL, DIG_CONSULTING],
      modelId: 'bedrock:us.anthropic.claude-sonnet-5',
      promptVersion: 'director/v3',
      createdAt: ago(1.2),
      updatedAt: ago(1),
      turns: [
        { role: 'user', content: 'What actually needs my attention this morning?', toolCalls: [], createdAt: ago(1.2), inputTokens: u64(0), outputTokens: u64(0), costUsd: null },
        {
          role: 'director',
          content:
            'Seven Finds are open and two messages are Unresolved. Before your first meeting I would handle two:\n\n1. **Adam is waiting on the Jeep brakes** — he needs an answer by tomorrow noon to order parts.\n2. **Review on PR #412** — Jamie says it blocks the merge, and Priya wants the deploy before the Friday change freeze.\n\nThe Ledgerly invoice is due Friday; the Datadog renewal has eight days. Riddle is still catching up on Work (1,240 messages to sift), so this may change.',
          toolCalls: [
            { id: 'tc_1', action: 'list_finds', args: { status: 'open' }, ok: true, summary: '7 open Finds', activityId: null, latencyMs: u64(12), untrusted: false },
            { id: 'tc_2', action: 'task_centre', args: {}, ok: true, summary: 'Work catching up: 1,240 remaining', activityId: null, latencyMs: u64(4), untrusted: false },
            { id: 'tc_3', action: 'get_thread', args: { digId: DIG_PERSONAL, threadId: 't_pers_jeep' }, ok: true, summary: 'Read 3 messages', activityId: null, latencyMs: u64(9), untrusted: true },
          ],
          createdAt: ago(1.15),
          inputTokens: u64(6120),
          outputTokens: u64(310),
          costUsd: 0.0231,
        },
        { role: 'user', content: 'Take the newsletters I never open and treat them as Spoil.', toolCalls: [], createdAt: ago(1.05), inputTokens: u64(0), outputTokens: u64(0), costUsd: null },
        {
          role: 'director',
          content: 'Done: 23 newsletters moved to Spoil across your three Digs; 2 were skipped because you had handled them since. I did not create a Mesh for this because I read mail content earlier in this task, so creating one needs your approval. There is a proposal waiting if you want it.',
          toolCalls: [
            { id: 'tc_4', action: 'search', args: { query: 'is:newsletter never-opened', page: { cursor: null, limit: 100 } }, ok: true, summary: '25 messages', activityId: null, latencyMs: u64(31), untrusted: true },
            { id: 'tc_5', action: 'move_to_spoil', args: { count: 25 }, ok: true, summary: 'Moved 23, skipped 2', activityId: 'act_director_nl', latencyMs: u64(88), untrusted: false },
            { id: 'tc_6', action: 'create_mesh', args: { name: 'Unopened newsletters' }, ok: false, summary: 'needs approval: this task has read untrusted content', activityId: null, latencyMs: u64(2), untrusted: false },
          ],
          createdAt: ago(1),
          inputTokens: u64(7410),
          outputTokens: u64(190),
          costUsd: 0.0251,
        },
      ],
    },
    {
      id: 'conv_aws',
      title: 'Why did the AWS alert go to Spoil?',
      readUntrusted: false,
      digIds: [DIG_WORK],
      modelId: 'bedrock:us.anthropic.claude-sonnet-5',
      promptVersion: 'director/v3',
      createdAt: daysAgo(3),
      updatedAt: daysAgo(3),
      turns: [
        { role: 'user', content: 'Why did the AWS billing alert go to Spoil?', toolCalls: [], createdAt: daysAgo(3), inputTokens: u64(0), outputTokens: u64(0), costUsd: null },
        {
          role: 'director',
          content: 'No Mesh covered AWS mail, so it fell to the Signals: routine .88 against personally_relevant .35, which the confidence policy reads as Spoil. You have corrected AWS billing alerts three times now. I have created a Shadow Mesh, AWS Notifications, that surfaces billing alerts as Find: Know; it will propose rather than act until you have seen it agree.',
          toolCalls: [
            { id: 'tc_7', action: 'explain', args: { digId: DIG_WORK, messageId: 'm_work_aws' }, ok: true, summary: 'jev: routine .88', activityId: null, latencyMs: u64(6), untrusted: false },
            { id: 'tc_8', action: 'create_mesh', args: { name: 'AWS Notifications', status: 'shadow' }, ok: true, summary: 'Created v1 (Shadow)', activityId: 'act_director_mesh', latencyMs: u64(40), untrusted: false },
          ],
          createdAt: daysAgo(3),
          inputTokens: u64(3100),
          outputTokens: u64(160),
          costUsd: 0.0117,
        },
      ],
    },
  ]
}

export function buildExpeditions(): Expedition[] {
  return [
    {
      id: 'exp_subs',
      goal: 'Find every subscription I have paid for during the last year and identify the ones I may no longer need.',
      scope: { digIds: [DIG_PERSONAL, DIG_CONSULTING], since: daysAgo(365), until: null, toolsAllowed: ['search', 'get_message', 'get_thread', 'list_finds', 'evaluate_signal'] },
      budget: { costUsd: 2, seconds: u64(1800), toolCalls: 120 },
      spent: { costUsd: 0.63, seconds: u64(412), toolCalls: 38 },
      status: 'running',
      plan: [
        { n: 1, text: 'Search receipts and payment confirmations since last year.', done: true },
        { n: 2, text: 'Group by merchant and detect recurring amounts.', done: true },
        { n: 3, text: 'For each merchant, find the last time you opened or replied to their mail.', done: true },
        { n: 4, text: 'Find subsequent receipts after any cancellation correspondence.', done: false },
        { n: 5, text: 'Rank by cost and by how long since you engaged.', done: false },
        { n: 6, text: 'Produce a Find per subscription that looks unused.', done: false },
      ],
      checkpoint: { step: 3, merchants: 14 },
      evidence: [
        { kind: 'message', id: 'm_cons_stripe' },
        { kind: 'message', id: 'm_pers_amazon' },
        { kind: 'message', id: 'm_pers_easyjet' },
        { kind: 'thread', id: 't_pers_jeep' },
      ],
      findings: ['14 merchants with recurring charges, £312 per month in total.', 'Notion Team (£45/mo): no message opened since March.'],
      error: null,
      createdAt: ago(0.6),
      updatedAt: ago(0, 3),
      finishedAt: null,
    },
    {
      id: 'exp_reno',
      goal: 'Go through everything related to the house renovation and tell me what we are still waiting for.',
      scope: { digIds: [DIG_PERSONAL], since: daysAgo(120), until: null, toolsAllowed: ['search', 'get_thread'] },
      budget: { costUsd: 1, seconds: u64(900), toolCalls: 60 },
      spent: { costUsd: 0.41, seconds: u64(388), toolCalls: 27 },
      status: 'completed',
      plan: [
        { n: 1, text: 'Search the Renovation Stratum and related threads.', done: true },
        { n: 2, text: 'Extract open asks from each thread.', done: true },
        { n: 3, text: 'Report what is waiting on whom.', done: true },
      ],
      checkpoint: null,
      evidence: [{ kind: 'thread', id: 't_pers_reno' }],
      findings: ['Waiting on Dave: the electrician date. Waiting on you: sign the revised quote (£18,400).'],
      error: null,
      createdAt: daysAgo(4),
      updatedAt: daysAgo(4),
      finishedAt: daysAgo(4),
    },
  ]
}

export function buildDrafts(): Draft[] {
  return [
    {
      id: 'draft_1',
      digId: DIG_WORK,
      providerId: null,
      threadId: 't_work_datadog',
      inReplyTo: 'm_work_datadog_2',
      to: [{ email: 'priya@northwind.example', name: 'Priya Raman' }],
      cc: [],
      bcc: [],
      subject: 'Re: Fwd: Your Datadog contract renews on the 30th — confirm seats',
      bodyText: 'Let us drop to 36: 31 in use plus the two hires and headroom.\n\n',
      updatedAt: ago(19),
    },
  ]
}

// ---------------------------------------------------------------------------
// Settings, dev info, logs

export function buildSettings(): Settings {
  return {
    schemaVersion: 3,
    developerMode: true,
    logLevel: 'debug',
    notifyPriorityThreshold: 70,
    notifications: { mode: 'brief', briefTimes: ['09:00', '16:00'], immediateTypes: ['pay', 'respond'], dueWithinHours: 24, neverHiddenImmediate: true },
    priorityInbox: { strata: [], labels: ['STARRED'], minFindPriority: 70, findTypes: [], minImportance: 'high' },
    shadowPeriodDays: 7,
    judge: { provider: 'jev', model: 'jev-1.13.0', keySource: 'ssm', ssmProfile: 'dev', ssmParameter: '/riddle/jev/api-key', apiKeyPresent: true },
    llm: { provider: 'bedrock', profile: 'dev', region: 'us-east-1', escalationModel: 'us.anthropic.claude-haiku-4-5', directorModel: 'us.anthropic.claude-sonnet-5' },
    budgets: { dailyJudgeUsd: 1.5, dailyLlmUsd: 4, escalationPerDigUsd: 1 },
    gmail: { clientId: '', clientSecretPresent: false },
    autonomy: { default: 'suggest', perDig: { [DIG_WORK]: 'act', [DIG_PERSONAL]: 'act', [DIG_CONSULTING]: 'observe' }, perAction: { send_message: 'suggest' } },
    projectionDefault: 'labels_only',
    privacyDefault: 'cloud_judgment',
    launchAtLogin: false,
    closeToHide: true,
  }
}

export function buildDevInfo(): DevInfo {
  return {
    build: { version: '0.1.0', gitSha: '3c0df07', dirty: true, builtAt: daysAgo(0.1), profile: 'debug' },
    dev: true,
    os: 'macos',
    arch: 'aarch64',
    uptimeSeconds: 4_213,
    dataDir: '/Users/me/Library/Application Support/dev.codenaked.riddle.dev',
    logDir: '/Users/me/Library/Logs/dev.codenaked.riddle.dev',
    configDir: '/Users/me/Library/Application Support/dev.codenaked.riddle.dev/config',
    cacheDir: '/Users/me/Library/Caches/dev.codenaked.riddle.dev',
    stores: [
      { label: 'index', path: '/Users/me/Library/Application Support/dev.codenaked.riddle.dev/index.sqlite', sizeBytes: 18_874_368 },
      { label: 'dig dig_work', path: '/Users/me/Library/Application Support/dev.codenaked.riddle.dev/digs/dig_work.sqlite', sizeBytes: 402_653_184 },
      { label: 'dig dig_personal', path: '/Users/me/Library/Application Support/dev.codenaked.riddle.dev/digs/dig_personal.sqlite', sizeBytes: 221_249_536 },
      { label: 'dig dig_consulting', path: '/Users/me/Library/Application Support/dev.codenaked.riddle.dev/digs/dig_consulting.sqlite', sizeBytes: 61_865_984 },
    ],
    storeFault: null,
    logLevel: 'debug',
  }
}

export function buildLogs(): LogLine[] {
  const lines: [number, string, string, string, unknown][] = [
    [0.5, 'info', 'app', 'riddle 0.1.0 (3c0df07-dirty) started; profile=Dev', null],
    [0.49, 'info', 'store', 'opened index store', { path: 'index.sqlite', schema: 3 }],
    [0.48, 'info', 'store', 'opened dig store dig_work', { schema: 3, sizeBytes: 402653184 }],
    [0.47, 'info', 'jobs', 'scheduler started', { limiters: ['gmail', 'judge', 'llm', 'sqlite'] }],
    [0.46, 'info', 'sync', 'catch-up started for dig_work', { backlog: 4981, historyId: '8812231' }],
    [0.45, 'debug', 'sync', 'fetched 100 headers for dig_work in 412ms', { page: 1 }],
    [0.44, 'debug', 'sift', 'facts extracted for 100 messages', { ms: 31 }],
    [0.43, 'debug', 'judge', 'jev request: 5 questions, 1912 input tokens, 184ms', { requestId: 'jev_01K7Z' }],
    [0.42, 'debug', 'mesh', 'GitHub Notifications v7 matched route 3 for m_work_gh_1', null],
    [0.41, 'info', 'sift', 'classified 100 messages: find 3, filed 21, spoil 68, contamination 2, unresolved 6', { ms: 2210 }],
    [0.4, 'warn', 'judge', 'jev latency 2140ms exceeded budget 1000ms', { requestId: 'jev_01K80' }],
    [0.39, 'debug', 'llm', 'escalation for m_work_deploy: 2900 input tokens, 1240ms', { model: 'us.anthropic.claude-haiku-4-5' }],
    [0.38, 'info', 'actions', 'action move_to_spoil by sift: 14 messages', { activityId: 'act_sift_gh' }],
    [0.37, 'info', 'projection', 'projection paused for dig_work: waiting for catch-up', null],
    [0.36, 'debug', 'ui', 'summary ok in 3ms', null],
    [0.35, 'error', 'judge', 'jev request failed: 429 rate limited; circuit half-open', { requestId: 'jev_01K8A', retryAfterMs: 800 }],
    [0.34, 'info', 'judge', 'jev retry succeeded after 800ms', { requestId: 'jev_01K8A' }],
    [0.3, 'info', 'director', 'conversation conv_morning: 6 tool calls, 13530 input tokens', { costUsd: 0.0482 }],
    [0.28, 'warn', 'director', 'egress gated: create_mesh after untrusted read in conv_morning', null],
    [0.2, 'debug', 'expedition', 'exp_subs checkpoint step 3', { toolCalls: 38, costUsd: 0.63 }],
    [0.1, 'debug', 'jobs', 'job_catchup_work progress 3741/4981', { etaSeconds: 248 }],
    [0.05, 'info', 'credentials', 'jev api key resolved from ssm (dev)', null],
    [0.02, 'debug', 'ui', 'task_centre ok in 2ms', null],
  ]
  return lines.map(([h, level, category, message, payload]) => ({ at: ago(h), level, category, message, payload }))
}
