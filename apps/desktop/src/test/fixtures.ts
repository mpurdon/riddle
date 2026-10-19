/**
 * Well-formed binding values for unit tests that care about one or two
 * fields: a `MessageSummary` row and a `TaskCentre` snapshot, each with
 * every field the binding requires and `over` on top. Page tests drive the
 * mocks instead; these are for the pure helpers around the query cache.
 */
import type { MessageSummary } from '@/bindings/MessageSummary'
import type { TaskCentre } from '@/bindings/TaskCentre'

export const messageSummary = (over: Partial<MessageSummary> = {}): MessageSummary => ({
  id: 'm_1',
  digId: 'dig-1',
  threadId: 't_1',
  subject: 'Subject',
  from: { email: 'x@example.com', name: null },
  date: '2026-09-25T09:00:00Z',
  snippet: '',
  labels: [],
  isSent: false,
  hasAttachments: false,
  disposition: null,
  strata: [],
  findId: null,
  shadow: false,
  shadowReview: null,
  importance: 'normal',
  importanceSetByYou: false,
  ...over,
})

export const taskCentre = (over: Partial<TaskCentre> = {}): TaskCentre => ({ overall: 'caught_up', digs: [], jobs: [], storeFault: null, judge: null, llm: null, asOf: '2026-10-19T07:00:00.000Z', ...over })
