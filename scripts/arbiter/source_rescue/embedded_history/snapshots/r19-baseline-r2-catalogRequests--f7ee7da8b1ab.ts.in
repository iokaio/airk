// SPDX-License-Identifier: Apache-2.0
// CatalogList's request/state procedure and IndexDetail's retained-detail projection.
// R19a preserves captured-row merging and the original detail-cache behavior.
import type { CallResult } from './api';
import { body, OPS, type CatalogKind, type Operation } from './operations';
type Rec = Record<string, unknown>;
const rec = (value: unknown): Rec | null =>
  typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as Rec) : null;
const str = (value: unknown): string | null => (typeof value === 'string' ? value : null);
const num = (value: unknown): number | null => (typeof value === 'number' && Number.isFinite(value) ? value : null);
const arr = (value: unknown): unknown[] => (Array.isArray(value) ? value : []);
const text = (value: unknown): string => (typeof value === 'string' ? value : JSON.stringify(value) ?? 'undefined');
export type Item = { kind: string; value: Rec };
export function itemOf(value: unknown): Item | null {
  const r = rec(value);
  const kind = str(r?.kind);
  const inner = rec(r?.value);
  return kind !== null && inner !== null ? { kind, value: inner } : null;
}
export interface CatalogListProps {
  kind: CatalogKind;
  onRevision: (revision: number) => void;
  onItems?: (items: Item[], complete: boolean) => void;
}
export interface CatalogCallbacks {
  run: (operation: Operation, body: unknown) => Promise<CallResult>;
  setItems: (items: Item[]) => void;
  setCursor: (cursor: unknown) => void;
  setHeader: (header: { namespace: string; revision: number }) => void;
}
export function createCatalogRequests(callbacks: CatalogCallbacks) {
  const { run, setItems, setCursor, setHeader } = callbacks;
  const load = async (after: unknown, props: CatalogListProps,
    filter: { indexId: string | null; projectId: string | null }, items: Item[]): Promise<void> => {
    const expected = after === null ? null : num(rec(after)?.revision);
    const result = await run(OPS.catalogPage, body.catalogPage(props.kind, filter, after, expected));
    const page = result.kind === 'ok' ? rec(rec(result.envelope.result)?.page) : null;
    if (page === null) return;
    const got = arr(page.items)
      .map(itemOf)
      .filter((item): item is Item => item !== null);
    const merged = after === null ? got : [...items, ...got];
    setItems(merged);
    props.onItems?.(merged, (page.continuation ?? null) === null);
    setCursor(page.continuation ?? null);
    const revision = num(page.revision);
    if (revision !== null) {
      setHeader({ namespace: text(page.namespace), revision });
      props.onRevision(revision);
    }
  };
  return { load };
}
export type IndexDetailCache = { current: { id: string; detail: Rec } | null };
export function indexDetailValue(props: { id: string }, detail: { phase: string }, loaded: Rec | null, lastGood: IndexDetailCache): Rec | null {
  if (loaded !== null) lastGood.current = { id: props.id, detail: loaded };
  const d =
    loaded ?? (detail.phase === 'pending' && lastGood.current?.id === props.id ? lastGood.current.detail : null);
  return d;
}
