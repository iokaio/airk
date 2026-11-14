// SPDX-License-Identifier: Apache-2.0
// Activity's request/state owners. Session observations are injected and opaque.
import type { CallResult } from './api';
import { body, MAX_INDEX_LIST_LIMIT, OPS, type Operation } from './operations';
import { generatedAction } from './generatedActions';
type Rec = Record<string, unknown>;
const rec = (value: unknown): Rec | null =>
  typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as Rec) : null;
const okValue = (result: CallResult): Rec | null => (result.kind === 'ok' ? rec(result.envelope.result) : null);
const str = (value: unknown): string | null => (typeof value === 'string' ? value : null);
const num = (value: unknown): number | null => (typeof value === 'number' && Number.isFinite(value) ? value : null);
const arr = (value: unknown): unknown[] => (Array.isArray(value) ? value : []);
export interface JobCallbacks {
  runJob: (operation: Operation, body: unknown) => Promise<CallResult>;
  runJobAction: (operation: Operation, body: unknown) => Promise<CallResult>;
  setJobId: (id: string) => void;
  setJobRecord: (record: Rec | null) => void;
  currentContext?: () => unknown;
  currentSelection?: () => unknown;
  observedAt?: () => number;
  transition?: (transition: JobTransition) => void;
  seed?: JobTransition;
  changed?: () => void;
}
/** In-memory handoff only: the complete result is retained, never serialized with context. */
export interface JobTransition {
  origin: string;
  job: Rec;
  result: CallResult;
  context: unknown;
  at: number;
}
export function createJobRequests(callbacks: JobCallbacks) {
  const { runJob, runJobAction, setJobId, setJobRecord } = callbacks;
  let context = { value: callbacks.currentContext?.() }, generation = 0, disposed = false;
  const selection = callbacks.currentSelection?.();
  const seed = callbacks.seed?.context === context.value ? callbacks.seed : undefined;
  let selected = str(seed?.job.id) ?? '', accepted: Rec | null = seed?.job ?? null;
  let detailId: string | null = null, actionId: string | null = null;
  let transition = seed;
  const localIdentity = {};
  let retryOwner: ReturnType<typeof generatedAction> | null = null, retryId = '', retryContext: unknown;
  const releaseRetry = () => { retryOwner?.dispose(); retryOwner = null; retryId = ''; retryContext = undefined; };
  const attachRetry = (id: string) => {
    const identity = context.value !== null && typeof context.value === 'object' ? context.value : localIdentity;
    if (retryOwner && retryId === id && retryContext === identity) return retryOwner;
    releaseRetry(); retryId = id; retryContext = identity;
    retryOwner = generatedAction(identity, `jobs.retry:${id}`, callbacks.changed);
    return retryOwner;
  };
  const current = () => !disposed && context.value === callbacks.currentContext?.() && selection === callbacks.currentSelection?.();
  const live = (ticket: number, captured: typeof context) => current() && generation === ticket && captured === context;
  const bind = (next: unknown) => {
    if (next === context.value || disposed) return context;
    context = { value: next }; generation++;
    releaseRetry();
    selected = ''; accepted = null; detailId = null; actionId = null; transition = undefined;
    return context;
  };
  const openJob = async (id: string, captured = context): Promise<void> => {
    if (!current() || captured !== context) return;
    const ticket = ++generation;
    selected = id; accepted = null; detailId = id; actionId = null; transition = undefined;
    attachRetry(id);
    setJobId(id);
    if (!live(ticket, captured)) return;
    setJobRecord(null);
    if (!live(ticket, captured)) return;
    const value = okValue(await runJob(OPS.jobsGet, body.jobsGet(id)));
    if (!live(ticket, captured)) return;
    const record = rec(value?.job);
    accepted = str(record?.id) === id ? record : null;
    setJobRecord(accepted);
  };
  const jobChange = async (jobId: string, kind: 'retry' | 'cancel', captured = context): Promise<void> => {
    if (!current() || captured !== context || selected !== jobId || str(accepted?.id) !== jobId) return;
    const custody = kind === 'retry' ? attachRetry(jobId) : null;
    const intent = custody?.begin(OPS.jobsRetry, body.jobsRetry(jobId));
    if (custody && !intent) { callbacks.changed?.(); return; }
    const ticket = ++generation;
    actionId = jobId; transition = undefined;
    let result: CallResult;
    try {
      result = await runJobAction(kind === 'retry' ? OPS.jobsRetry : OPS.jobsCancel,
        intent ? intent.method : body.jobsCancel(jobId));
    } catch (error) {
      if (custody && intent) custody.settle(intent, { kind: 'transport', status: null,
        message: 'The original retry did not return a usable outcome. Inspect Activity before another retry.' }, callbacks.observedAt?.() ?? 0);
      throw error;
    }
    if (custody && intent) custody.settle(intent, result, callbacks.observedAt?.() ?? 0);
    if (!live(ticket, captured) || selected !== jobId) return;
    const updated = rec(okValue(result)?.job);
    const updatedId = str(updated?.id);
    if (updated === null || updatedId === null || (kind === 'cancel' && updatedId !== jobId)) return;
    accepted = updated;
    if (kind === 'retry' && updatedId !== jobId) {
      selected = updatedId; actionId = updatedId;
      transition = { origin: jobId, job: updated, result, context: context.value, at: callbacks.observedAt?.() ?? 0 };
      callbacks.transition?.(transition);
      if (!live(ticket, captured)) return;
      setJobId(updatedId);
      if (!live(ticket, captured)) return;
    }
    setJobRecord(updated);
  };
  return { openJob, jobChange, bind,
    dispose: () => { disposed = true; generation++; releaseRetry(); },
    snapshot: () => ({ record: current() ? accepted : null, selected,
      detailCurrent: current() && detailId === selected,
      actionCurrent: current() && actionId === selected,
      retry: current() && retryId === selected ? retryOwner?.snapshot() ?? null : null,
      transition: current() ? transition : undefined }),
  };
}
export type IndexInfo = {
  binding: string;
  name: string | null;
  id: string | null;
  paused: boolean | null;
  retired: boolean;
  /** Registered with the service (only those can be paused or filtered on). */
  registered: boolean;
};
const MAX_NAME_PAGES = 16;
const MAX_PROJECT_PAGES = 16;
export interface IndexCallbacks {
  runCatalog: (operation: Operation, body: unknown) => Promise<CallResult>;
  runProjects: (operation: Operation, body: unknown) => Promise<CallResult>;
  setRegistryKnown: (known: boolean) => void;
  setPartial: (partial: boolean) => void;
  setIndexes: (indexes: IndexInfo[]) => void;
  currentContext?: () => unknown;
  changed?: () => void;
}
export function createIndexRequests(callbacks: IndexCallbacks) {
  const { runCatalog, runProjects, setRegistryKnown, setPartial, setIndexes } = callbacks;
  let context = { value: callbacks.currentContext?.() }, generation = 0, disposed = false;
  let catalogCall: typeof context | null = null, projectsCall: typeof context | null = null;
  let namespace: string | null = null;
  let accepted = { indexes: [] as IndexInfo[], partial: false, registryKnown: false, problem: null as string | null };
  const current = () => !disposed && context.value === callbacks.currentContext?.();
  const bind = (value: unknown) => {
    if (!disposed && context.value !== value) {
      context = { value }; generation++;
      catalogCall = null; projectsCall = null;
      namespace = null;
      accepted = { indexes: [], partial: true, registryKnown: false, problem: null };
    }
    return context;
  };
  const reload = async (captured = context): Promise<void> => {
    if (!current() || captured !== context) return;
    const ticket = ++generation;
    const live = () => current() && captured === context && ticket === generation;
    const inconsistent = (message: string) => {
      if (!live()) return;
      accepted = { ...accepted, partial: true, problem: message };
      setPartial(true); callbacks.changed?.();
    };
    const byBinding = new Map<string, IndexInfo>();
    let after: unknown = null;
    let pages = 0;
    let more = false;
    let catalogIncomplete = false;
    let catalogHeader: { namespace: string; revision: number } | null = null;
    do {
      const expected = after === null ? null : num(rec(after)?.revision);
      catalogCall = context;
      const result = await runCatalog(OPS.catalogPage, body.catalogPage('indexes', {}, after, expected));
      if (!live()) return;
      const page = rec(okValue(result)?.page);
      if (page === null) { catalogIncomplete = true; break; }
      const pageNamespace = str(page.namespace), pageRevision = num(page.revision);
      if (pageNamespace === null || pageRevision === null || (namespace !== null && pageNamespace !== namespace) || (catalogHeader !== null &&
        (catalogHeader.namespace !== pageNamespace || catalogHeader.revision !== pageRevision))) {
        inconsistent('The index catalog changed while its names were being read. Reload indexes.'); return;
      }
      catalogHeader = { namespace: pageNamespace, revision: pageRevision };
      for (const item of arr(page.items)) {
        const value = rec(rec(item)?.value);
        const binding = str(rec(value?.backing)?.binding);
        if (value === null || binding === null) continue;
        byBinding.set(binding, {
          binding,
          name: str(value.name),
          id: str(value.id),
          paused: null,
          retired: value.tombstone === true,
          registered: false,
        });
      }
      after = page.continuation ?? null;
      if (after !== null && (str(rec(after)?.namespace) !== pageNamespace || num(rec(after)?.revision) !== pageRevision)) {
        inconsistent('The index catalog continuation belongs to another observation. Reload indexes.'); return;
      }
      pages++;
      more = after !== null && pages >= MAX_NAME_PAGES;
    } while (after !== null && pages < MAX_NAME_PAGES);
    // Registered indexes in pages of 32 (the registry's bound), continuing
    // with the first page's revision.
    let projectAfter: string | null = null;
    let revision: number | null = null;
    let projectPages = 0;
    let projectsIncomplete = false;
    do {
      projectsCall = context;
      const registered = await runProjects(
        OPS.indexesList,
        body.indexesList(projectAfter, MAX_INDEX_LIST_LIMIT, projectAfter === null ? null : revision),
      );
      if (!live()) return;
      const page = rec(okValue(registered)?.page);
      if (page === null) {
        projectsIncomplete = true;
        break;
      }
      const nextRevision = num(page.revision);
      if (nextRevision === null || (revision !== null && revision !== nextRevision)) {
        inconsistent('The registered index list changed while it was being read. Reload indexes.'); return;
      }
      revision = nextRevision;
      for (const project of arr(page.projects)) {
        const p = rec(project);
        const binding = str(rec(p?.registration)?.binding);
        if (p === null || binding === null) continue;
        const known = byBinding.get(binding);
        const paused = typeof p.paused === 'boolean' ? p.paused : null;
        byBinding.set(binding, known ? { ...known, paused, registered: true } : { binding, name: null, id: null, paused, retired: false, registered: true });
      }
      projectAfter = str(page.after);
      projectPages++;
    } while (projectAfter !== null && projectPages < MAX_PROJECT_PAGES);
    if (projectAfter !== null) projectsIncomplete = true;
    if (!live()) return;
    namespace = catalogHeader?.namespace ?? namespace;
    accepted = { registryKnown: !projectsIncomplete, partial: more || catalogIncomplete || projectsIncomplete,
      indexes: [...byBinding.values()].sort((a, b) => (a.name ?? '~').localeCompare(b.name ?? '~')),
      problem: catalogIncomplete ? 'Index names are incomplete because the catalog could not be read.' : null };
    setRegistryKnown(!projectsIncomplete);
    if (!live()) return;
    setPartial(accepted.partial);
    if (!live()) return;
    setIndexes(accepted.indexes);
    callbacks.changed?.();
  };
  return { reload, bind,
    dispose: () => { disposed = true; generation++; },
    snapshot: () => ({ ...(current() ? accepted : { indexes: [], partial: true, registryKnown: false, problem: null }),
      catalogCurrent: current() && catalogCall === context, projectsCurrent: current() && projectsCall === context }),
  };
}
