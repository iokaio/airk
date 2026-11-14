// SPDX-License-Identifier: Apache-2.0
// Session-local admission for direct generated-key actions. No key is generated here.
import { bindPreparationScope, isUnknownOutcome, type CallResult } from './api';
import type { Operation } from './operations';

// Match the existing native Admin review/pending custody bound, without eviction.
const OUTSTANDING_LIMIT = 32;
export type GeneratedIntent = Readonly<{ operation: Operation; method: unknown }>;
type Cell = { refs: number; pending: GeneratedIntent | null; intent: GeneratedIntent | null; result: CallResult | null; at: number };
type Store = { cells: Map<string, Cell>; listeners: Set<() => void> };
const sessions = new WeakMap<object, Store>();
const outstanding = (cell: Cell): boolean => cell.pending !== null || cell.result !== null && isUnknownOutcome(cell.result);
function immutable(value: unknown): unknown {
  if (value !== null && typeof value === 'object') {
    if (Array.isArray(value)) return Object.freeze(value.map(immutable));
    return Object.freeze(Object.fromEntries(Object.entries(value).map(([key, child]) => [key, immutable(child)])));
  }
  return value;
}
export function generatedAction(context: object, key: string, changed?: () => void) {
  let store = sessions.get(context);
  if (!store) { store = { cells: new Map(), listeners: new Set() }; sessions.set(context, store); }
  const owner = store;
  let cell = owner.cells.get(key);
  if (!cell) { cell = { refs: 0, pending: null, intent: null, result: null, at: 0 }; owner.cells.set(key, cell); }
  const held = cell;
  held.refs++;
  if (changed) owner.listeners.add(changed);
  let disposed = false;
  const release = () => { if (held.refs === 0 && !outstanding(held) && owner.cells.get(key) === held) owner.cells.delete(key); };
  const notify = () => { for (const observer of owner.listeners) observer(); release(); };
  const capacity = () => !outstanding(held) && [...owner.cells.values()].filter(outstanding).length >= OUTSTANDING_LIMIT;
  return {
    begin(operation: Operation, method: unknown): GeneratedIntent | null {
      if (disposed || outstanding(held) || capacity()) return null;
      const intent = Object.freeze({ operation, method: immutable(method) });
      bindPreparationScope(intent.method, held);
      held.pending = intent; held.intent = intent; held.result = null; held.at = 0;
      notify(); return intent;
    },
    settle(intent: GeneratedIntent, result: CallResult, at: number): void {
      if (held.pending !== intent) return;
      held.pending = null; held.result = result; held.at = at;
      notify();
    },
    snapshot: () => ({ pending: held.pending !== null, unknown: held.result !== null && isUnknownOutcome(held.result),
      capacity: capacity(), intent: held.intent, result: held.result, at: held.at }),
    dispose(): void {
      if (disposed) return;
      disposed = true; held.refs--;
      if (changed) owner.listeners.delete(changed);
      release();
    },
  };
}
