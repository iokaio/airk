// SPDX-License-Identifier: Apache-2.0
// I3 prepare/submit/recover custody. Only the native owner can dispatch or
// acknowledge a mutation; restoring this tab's journal never submits anything.
import type { AcknowledgedMutationCall, MutationPreparation, PreparedMutationCall, RecoverMutationCall } from '../../contracts-gen/i3';
import type { ApiResult, RetainedPreparation } from '../../ui/src/apiPort';
import type { Operation } from './operations';

export const PREPARATION_STORAGE_PREFIX = 'airk-admin-preparations:';
const LIMIT = 32, BODY_BYTES = 64 * 1024;
const TOKEN = /^[0-9a-f]{32}$/;
function freezeJSON(value: unknown): unknown {
  if (value && typeof value === 'object') { for (const child of Object.values(value)) freezeJSON(child); Object.freeze(value); }
  return value;
}
const PREPARED_OPERATIONS = new Set<Operation>([
  'jobs.retry', 'catalog.policy_resume', 'catalog.split_recover',
  'recovery.plan_create', 'service.stop', 'service.restart',
]);
export const preparesMutation = (operation: Operation): boolean => PREPARED_OPERATIONS.has(operation);
export type PreparationState = 'ready' | 'unknown' | 'known';
type Entry = { operation: Operation; body: unknown; acknowledgment: ApiResult; preparation: MutationPreparation;
  state: PreparationState; result: ApiResult; pending: boolean; restored: boolean; scope?: object };
type JournalEntry = Pick<Entry, 'operation' | 'body' | 'acknowledgment' | 'preparation'>;
export type PreparationView = Readonly<{ operation: Operation; body: unknown; acknowledgment: ApiResult; preparation: MutationPreparation;
  state: PreparationState; result: ApiResult; pending: boolean; restored: boolean }>;
const metadata = new WeakMap<object, { owner: ReturnType<typeof createPreparedMutations>; entry: Entry }>();
// Local concurrency identity only. It carries no native authority/key and is
// deliberately absent from persisted records, results and transport bodies.
const scopes = new WeakMap<object, object>();
export function bindPreparationScope(body: unknown, scope: object): void { if (body && typeof body === 'object') scopes.set(body, scope); }
export function preparationScope(body: unknown): object | undefined { return body && typeof body === 'object' ? scopes.get(body) : undefined; }
const view = (entry: Entry): PreparationView => ({ operation: entry.operation, body: entry.body, acknowledgment: entry.acknowledgment,
  preparation: entry.preparation, state: entry.state, result: entry.result, pending: entry.pending, restored: entry.restored });
const admissionRefusals = new WeakSet<object>();
const failure = (message: string): ApiResult => {
  const result: ApiResult = { kind: 'transport', status: null, message };
  admissionRefusals.add(result); return result;
};
export function preparationAdmissionRefused(result: ApiResult): boolean { return admissionRefusals.has(result); }
export function preparationOf(result: ApiResult): PreparationView | null {
  const held = metadata.get(result);
  return held ? view(held.entry) : null;
}
export function preparedUnresolved(result: ApiResult): boolean { const value = metadata.get(result); return !!value && (value.entry.state !== 'known' || value.entry.pending); }
export function recoverPreparedResult(result: ApiResult): Promise<ApiResult> {
  const held = metadata.get(result);
  return held ? held.owner.recover(held.entry.preparation.preparation) : Promise.resolve(result);
}
export function mutationPreparation(result: ApiResult): MutationPreparation | null {
  if (result.kind !== 'ok' || !result.envelope.result || typeof result.envelope.result !== 'object') return null;
  const value = result.envelope.result as Record<string, unknown>;
  return Object.keys(value).length === 2 && typeof value.preparation === 'string' && TOKEN.test(value.preparation)
    && (value.receipt_operation === null || typeof value.receipt_operation === 'string')
    ? value as MutationPreparation : null;
}

/** A session's storage adapter is absent in the editor. It contains public
 * immutable input and opaque handles only, never browser/native credentials. */
export function createPreparedMutations(options: {
  serviceInstance: string; current(): boolean;
  send(operation: Operation, body: unknown, acknowledgment?: boolean): Promise<ApiResult>;
  storage?: Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;
  initial?: readonly RetainedPreparation[];
}) {
  const entries = new Map<string, Entry>(), preparing = new Set<{ operation: Operation; scope: object | undefined }>();
  const overlaps = (first: object | undefined, second: object | undefined): boolean => !first || !second || first === second;
  const listeners = new Set<() => void>();
  const key = PREPARATION_STORAGE_PREFIX + options.serviceInstance;
  let journalBlocked = false;
  const publish = () => { for (const listener of listeners) listener(); };
  const persist = () => {
    if (!options.storage) return;
    const records: JournalEntry[] = [...entries.values()].filter(entry => entry.state !== 'known').map(entry => ({
      operation: entry.operation, body: entry.body, acknowledgment: entry.acknowledgment, preparation: entry.preparation,
    }));
    if (records.length) options.storage.setItem(key, JSON.stringify({ version: 1, serviceInstance: options.serviceInstance, records }));
    else options.storage.removeItem(key);
  };
  try {
    const stored = options.initial ? JSON.stringify({ version: 1, serviceInstance: options.serviceInstance, records: options.initial }) : options.storage?.getItem(key);
    if (stored) {
      const value = JSON.parse(stored);
      if (value.version !== 1 || value.serviceInstance !== options.serviceInstance || !Array.isArray(value.records)
          || value.records.length > LIMIT) throw Error('invalid_preparation_journal');
      for (const item of value.records) {
        if (!item || !preparesMutation(item.operation) || new TextEncoder().encode(JSON.stringify(item.body)).length > BODY_BYTES
            || item.acknowledgment?.envelope?.operation !== item.operation) throw Error('invalid_preparation_record');
        const preparation = mutationPreparation(item.acknowledgment);
        if (!preparation || JSON.stringify(preparation) !== JSON.stringify(item.preparation)
            || entries.has(preparation.preparation)) throw Error('invalid_preparation_acknowledgment');
        entries.set(preparation.preparation, { ...item, body: freezeJSON(item.body), preparation: Object.freeze({ ...preparation }), state: 'unknown', result: item.acknowledgment,
          pending: false, restored: true });
      }
    }
  } catch { journalBlocked = true; }

  const remember = (entry: Entry, result: ApiResult): ApiResult => {
    entry.result = result; metadata.set(result, { owner, entry }); publish(); return result;
  };
  const acknowledgeKnown = async (entry: Entry): Promise<ApiResult> => {
    const original = entry.result;
    if (entry.state !== 'known' || !options.current()) return original;
    try { persist(); } catch { journalBlocked = true; return original; }
    const body: AcknowledgedMutationCall = { acknowledged: entry.preparation.preparation };
    const ack = await options.send(entry.operation, body, true);
    if (ack.kind === 'failed' && ack.status === 400) {
      entry.state = 'unknown';
      if (options.current()) { try { persist(); } catch { journalBlocked = true; } }
    } else if (ack.kind === 'ok' && ack.envelope.operation === entry.operation
      && ack.envelope.result && typeof ack.envelope.result === 'object'
      && (ack.envelope.result as { acknowledged?: unknown }).acknowledged === entry.preparation.preparation)
      entries.delete(entry.preparation.preparation);
    publish();
    return original;
  };
  const finish = async (entry: Entry, result: ApiResult): Promise<ApiResult> => {
    // A complete explicit Refused or success (excluding READY) is final. Internal,
    // unavailable, malformed and missing replies do not establish no effects.
    const known = !mutationPreparation(result) && (result.kind === 'ok' && result.envelope.operation === entry.operation
      || result.kind === 'failed' && result.envelope.error.class === 'refused');
    entry.state = known ? 'known' : mutationPreparation(result) ? 'ready' : 'unknown';
    remember(entry, result);
    // An old session's late reply is evidence only. It cannot overwrite or
    // remove the replacement cookie lifetime's same-tab journal.
    if (!options.current()) return result;
    if (!known) { try { persist(); } catch { journalBlocked = true; } return result; }
    // Retain the complete native result before releasing native custody. Storage
    // cleanup must succeed before ACK; a failed ACK never replaces this result.
    return acknowledgeKnown(entry);
  };
  const owner = {
    subscribe(listener: () => void): () => void { listeners.add(listener); return () => { listeners.delete(listener); }; },
    list(): PreparationView[] { return [...entries.values()].map(view); },
    blocked(): boolean { return journalBlocked || [...entries.values()].some(entry => entry.restored && (entry.state !== 'known' || entry.pending)); },
    async run(operation: Operation, body: unknown, scope?: object): Promise<ApiResult> {
      if (!options.current()) return { kind: 'no_session' };
      if (owner.blocked()) return failure('Recover the original prepared action before starting another generated action. If this tab’s journal is unavailable, open a fresh Admin session.');
      if ([...preparing].some(entry => entry.operation === operation && overlaps(entry.scope, scope))
          || [...entries.values()].some(entry => entry.operation === operation && overlaps(entry.scope, scope) && (entry.state !== 'known' || entry.pending)))
        return failure('The original action is pending or unresolved. Read its preparation outcome before starting another action.');
      if (entries.size + preparing.size >= LIMIT) return failure('This session retains 32 prepared actions. Recover their outcomes or open a fresh Admin session.');
      let immutableBody: unknown;
      try {
        const text = JSON.stringify(body);
        if (new TextEncoder().encode(text).length > BODY_BYTES) throw Error('preparation_body_too_large');
        immutableBody = freezeJSON(JSON.parse(text));
      } catch { return failure('The action body cannot be captured within the 64 KiB preparation limit. Nothing was submitted.'); }
      const admission = { operation, scope }; preparing.add(admission);
      try {
        const acknowledgment = await options.send(operation, immutableBody);
        const observedPreparation = mutationPreparation(acknowledgment);
        if (!observedPreparation || acknowledgment.kind !== 'ok' || acknowledgment.envelope.operation !== operation) return acknowledgment;
        const preparation = Object.freeze({ ...observedPreparation });
        const entry: Entry = { operation, body: immutableBody, acknowledgment, preparation, state: 'ready',
          result: acknowledgment, pending: false, restored: false, scope };
        entries.set(preparation.preparation, entry);
        preparing.delete(admission);
        remember(entry, acknowledgment);
        if (!options.current()) return acknowledgment;
        try { persist(); } catch {
          journalBlocked = true;
          return remember(entry, failure('The preparation was received, but this tab could not retain its recovery handle. Nothing was submitted. Recover the original preparation or open a fresh Admin session.'));
        }
        if (!options.current()) return acknowledgment;
        const submit: PreparedMutationCall = { prepared: preparation.preparation };
        entry.pending = true; publish();
        try { return await finish(entry, await options.send(operation, submit)); }
        finally { entry.pending = false; publish(); }
      } finally { preparing.delete(admission); }
    },
    async recover(handle: string): Promise<ApiResult> {
      const entry = entries.get(handle);
      if (!entry || !options.current()) return { kind: 'no_session' };
      if (entry.pending) return entry.result;
      entry.pending = true; publish();
      const body: RecoverMutationCall = { recover: handle };
      try { return await finish(entry, await options.send(entry.operation, body)); }
      finally { entry.pending = false; publish(); }
    },
    async acknowledge(handle: string): Promise<ApiResult> {
      const entry = entries.get(handle);
      if (!entry || !options.current()) return { kind: 'no_session' };
      if (entry.pending || entry.state !== 'known') return entry.result;
      entry.pending = true; publish();
      try { return await acknowledgeKnown(entry); }
      finally { entry.pending = false; publish(); }
    },
  };
  for (const entry of entries.values()) metadata.set(entry.result, { owner, entry });
  return owner;
}
