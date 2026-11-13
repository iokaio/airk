// SPDX-License-Identifier: Apache-2.0
// Activity's request/state procedures, relocated without changing acceptance.
import type { CallResult } from './api';
import { body, MAX_INDEX_LIST_LIMIT, OPS, type Operation } from './operations';
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
}
export function createJobRequests(callbacks: JobCallbacks) {
  const { runJob, runJobAction, setJobId, setJobRecord } = callbacks;
  const openJob = async (id: string): Promise<void> => {
    setJobId(id);
    setJobRecord(null);
    const value = okValue(await runJob(OPS.jobsGet, body.jobsGet(id)));
    setJobRecord(rec(value?.job));
  };
  const jobChange = async (jobId: string, kind: 'retry' | 'cancel'): Promise<void> => {
    const result = await runJobAction(
      kind === 'retry' ? OPS.jobsRetry : OPS.jobsCancel,
      kind === 'retry' ? body.jobsRetry(jobId) : body.jobsCancel(jobId),
    );
    const updated = rec(okValue(result)?.job);
    if (updated !== null) setJobRecord(updated);
  };
  return { openJob, jobChange };
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
}
export function createIndexRequests(callbacks: IndexCallbacks) {
  const { runCatalog, runProjects, setRegistryKnown, setPartial, setIndexes } = callbacks;
  const reload = async (): Promise<void> => {
    const byBinding = new Map<string, IndexInfo>();
    let after: unknown = null;
    let pages = 0;
    let more = false;
    do {
      const expected = after === null ? null : num(rec(after)?.revision);
      const result = await runCatalog(OPS.catalogPage, body.catalogPage('indexes', {}, after, expected));
      const page = rec(okValue(result)?.page);
      if (page === null) break;
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
      const registered = await runProjects(
        OPS.indexesList,
        body.indexesList(projectAfter, MAX_INDEX_LIST_LIMIT, projectAfter === null ? null : revision),
      );
      const page = rec(okValue(registered)?.page);
      if (page === null) {
        projectsIncomplete = true;
        break;
      }
      revision = num(page.revision);
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
    setRegistryKnown(!projectsIncomplete);
    setPartial(more || projectsIncomplete);
    setIndexes([...byBinding.values()].sort((a, b) => (a.name ?? '~').localeCompare(b.name ?? '~')));
  };
  return { reload };
}
