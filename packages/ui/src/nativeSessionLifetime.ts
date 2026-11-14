// SPDX-License-Identifier: Apache-2.0
// Pure native UI lifetime; browser token storage and native authority are separate owners.
import { endsSession } from '../../admin/src/envelope';
import type { Operation } from '../../vscode/src/adminUiContract';
import type { ApiPort, ApiResult, RetainedPreparation } from './apiPort';

export type NativeUiSession = Readonly<{ kind: 'native'; serviceInstance: string }>;
export interface NativeSessionLifetime {
  readonly session: NativeUiSession;
  readonly preparations: readonly RetainedPreparation[];
  isOpen(): boolean;
  invalidate(): void;
  subscribe(listener: () => void): () => void;
  call(operation: Operation, body: unknown): Promise<ApiResult>;
  close(): Promise<void>;
}
const lifetimes = new WeakMap<object, NativeSessionLifetime>();
export function nativeLifetimeOf(session: object): NativeSessionLifetime | undefined { return lifetimes.get(session); }
export function isNativeUiSession(value: unknown): value is NativeUiSession {
  return value !== null && typeof value === 'object' && (value as { kind?: unknown }).kind === 'native';
}
export function createNativeSessionLifetime(port: ApiPort): NativeSessionLifetime {
  if (port.kind !== 'native' || !/^[0-9a-f]{32}$/.test(port.serviceInstance)) throw new Error('invalid_native_ui_session');
  const instance = port.serviceInstance, session: NativeUiSession = Object.freeze({ kind: 'native', serviceInstance: instance });
  let open = true, closing: Promise<void> | null = null;
  const listeners = new Set<() => void>();
  const invalidate = () => {
    if (!open) return;
    open = false;
    for (const listener of listeners) listener();
  };
  const current = () => {
    if (open && (!port.isCurrent() || port.serviceInstance !== instance)) invalidate();
    return open;
  };
  const lifetime: NativeSessionLifetime = {
    session,
    preparations: port.preparations ?? [],
    isOpen: current,
    invalidate,
    subscribe(listener: () => void): () => void { listeners.add(listener); return () => { listeners.delete(listener); }; },
    async call(operation: Operation, body: unknown): Promise<ApiResult> {
      if (!current()) return { kind: 'no_session' };
      let result: ApiResult;
      try { result = await port.call(operation, body); }
      catch { result = { kind: 'transport', status: null, message: 'The native Admin request did not return an outcome. Inspect the original action before trying it again.' }; }
      if (result.kind !== 'no_session' && endsSession(result)) invalidate();
      current();
      // Full late replies remain evidence of the original call; they never reopen this lifetime.
      return result;
    },
    close(): Promise<void> {
      invalidate();
      if (closing === null) {
        // Capture the close promise synchronously, including an immediate thrown error.
        closing = Promise.resolve().then(() => port.close());
      }
      return closing;
    },
  };
  lifetimes.set(session, lifetime);
  return lifetime;
}
