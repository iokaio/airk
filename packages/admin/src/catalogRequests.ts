// SPDX-License-Identifier: Apache-2.0
// CatalogList's request/state procedure and IndexDetail's retained-detail projection.
// Accepted pages belong to one rendered scope and one native observation.
import type { CallResult } from './api';
import { body, OPS, type CatalogKind, type Operation } from './operations';
type Rec = Record<string, unknown>;
const rec = (value: unknown): Rec | null =>
  typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as Rec) : null;
const str = (value: unknown): string | null => (typeof value === 'string' ? value : null);
const num = (value: unknown): number | null => (typeof value === 'number' && Number.isFinite(value) ? value : null);
const arr = (value: unknown): unknown[] => (Array.isArray(value) ? value : []);
export type Item = { kind: string; value: Rec };
export function itemOf(value: unknown): Item | null {
  const r = rec(value);
  const kind = str(r?.kind);
  const inner = rec(r?.value);
  return kind !== null && inner !== null ? { kind, value: inner } : null;
}
export interface CatalogListProps {
  kind: CatalogKind;
  onRevision: (revision: number, namespace?: string) => void;
  onItems?: (items: Item[], complete: boolean, header?: CatalogHeader) => void;
}
export type CatalogHeader = { namespace: string; revision: number };
export interface CatalogBinding {
  kind: CatalogKind;
  indexId: string | null;
  projectId: string | null;
  reload: number;
  session: unknown;
}
export type CatalogContext = Readonly<CatalogBinding> & { readonly generation: number };
export interface CatalogCallbacks {
  run: (operation: Operation, body: unknown) => Promise<CallResult>;
  setItems: (items: Item[]) => void;
  setCursor: (cursor: unknown) => void;
  setHeader: (header: { namespace: string; revision: number }) => void;
  currentContext?: () => unknown;
  changed?: () => void;
}
export function createCatalogRequests(callbacks: CatalogCallbacks) {
  const { run, setItems, setCursor, setHeader } = callbacks;
  let bound: CatalogContext | null = null, generation = 0, sequence = 0, disposed = false;
  let items: Item[] = [], cursor: unknown = null, header: CatalogHeader | null = null, problem: string | null = null;
  // Filters and reload generations share the same native catalog namespace.
  let pinnedNamespace: string | null = null;
  let lastCall: CatalogContext | null = null;
  const current = (context: CatalogContext) => !disposed && context === bound && context.session === callbacks.currentContext?.();
  const bind = (value: CatalogBinding): CatalogContext => {
    if (bound && value.kind === bound.kind && value.indexId === bound.indexId && value.projectId === bound.projectId
      && value.reload === bound.reload && value.session === bound.session) return bound;
    const context = Object.freeze({ ...value, generation: ++generation });
    if (!disposed) {
      if (bound !== null && value.session !== bound.session) pinnedNamespace = null;
      bound = context; sequence++; items = []; cursor = null; header = null; problem = null; lastCall = null;
    }
    return context;
  };
  const load = async (after: unknown, props: CatalogListProps, context: CatalogContext): Promise<void> => {
    if (!current(context) || props.kind !== context.kind) return;
    const fail = (message: string) => { if (current(context)) { problem = message; callbacks.changed?.(); } };
    if (after !== null && (header === null || cursor === null || JSON.stringify(after) !== JSON.stringify(cursor))) {
      fail('This continuation is out of date. Reload this list.'); return;
    }
    const expected = after === null ? null : header!.revision;
    const ticket = ++sequence;
    const live = () => current(context) && ticket === sequence;
    if (after === null) cursor = null;
    problem = null; lastCall = context;
    const filter = { indexId: context.indexId, projectId: context.projectId };
    const result = await run(OPS.catalogPage, body.catalogPage(context.kind, filter, after, expected));
    if (!live()) return;
    const page = result.kind === 'ok' ? rec(rec(result.envelope.result)?.page) : null;
    if (page === null) return;
    const namespace = str(page.namespace), revision = num(page.revision);
    if (namespace === null || revision === null || (pinnedNamespace !== null && namespace !== pinnedNamespace) || (header !== null && namespace !== header.namespace) || (after !== null &&
      revision !== header?.revision)) {
      fail('The catalog observation changed while loading this list. Reload it.'); return;
    }
    const continuation = page.continuation ?? null;
    if (continuation !== null && (str(rec(continuation)?.namespace) !== namespace || num(rec(continuation)?.revision) !== revision)) {
      fail('The catalog continuation belongs to another observation. Reload this list.'); return;
    }
    const got = arr(page.items)
      .map(itemOf)
      .filter((item): item is Item => item !== null);
    const merged = after === null ? got : [...items, ...got];
    pinnedNamespace = namespace;
    items = merged; cursor = continuation; header = { namespace, revision };
    setItems(merged);
    if (!live()) return;
    props.onItems?.(merged, continuation === null, header);
    if (!live()) return;
    setCursor(continuation);
    if (!live()) return;
    setHeader(header);
    if (!live()) return;
    props.onRevision(revision, namespace);
    callbacks.changed?.();
  };
  return { load, bind,
    dispose: () => { disposed = true; sequence++; },
    view: (context: CatalogContext) => current(context)
      ? { items, cursor, header, problem, callCurrent: lastCall === context }
      : { items: [] as Item[], cursor: null, header: null, problem: null, callCurrent: false },
  };
}
export type IndexDetailCache = { current: { id: string; detail: Rec } | null };
function retainedDetail(kind: 'index' | 'project', props: { id: string; namespace?: string | null },
  detail: { phase: string }, loaded: Rec | null, lastGood: IndexDetailCache): Rec | null {
  const matchesNamespace = (value: Rec) => typeof value.namespace === 'string' && num(value.revision) !== null
    && (props.namespace === undefined || props.namespace === null || value.namespace === props.namespace);
  if (loaded !== null) {
    if (!matchesNamespace(loaded)) return null;
    if (lastGood.current !== null && lastGood.current.detail.namespace !== loaded.namespace) return null;
    if (loaded.item === null) { lastGood.current = null; return loaded; }
    const item = itemOf(loaded.item), id = str(item?.value.id);
    if (item?.kind !== kind || id !== props.id) return null;
    const previous = lastGood.current;
    if (previous?.id === id && previous.detail.namespace === loaded.namespace
      && (num(previous.detail.revision) ?? 0) > (num(loaded.revision) ?? 0)) return null;
    lastGood.current = { id, detail: loaded };
    return loaded;
  }
  return detail.phase === 'pending' && lastGood.current?.id === props.id && matchesNamespace(lastGood.current.detail)
    ? lastGood.current.detail : null;
}
export const indexDetailValue = (props: { id: string; namespace?: string | null }, detail: { phase: string }, loaded: Rec | null, cache: IndexDetailCache): Rec | null =>
  retainedDetail('index', props, detail, loaded, cache);
export const setDetailValue = (props: { id: string; namespace?: string | null }, detail: { phase: string }, loaded: Rec | null, cache: IndexDetailCache): Rec | null =>
  retainedDetail('project', props, detail, loaded, cache);
