// SPDX-License-Identifier: Apache-2.0
// Pure local C9 read-pilot contract, shared by hosts and presentations.
import type { SessionOutcome } from './generated/i2';
export const PILOT_VERSION = 1;
export type PilotRead =
  | { readonly kind: 'usage-json'; readonly binding: string; readonly days: number }
  | { readonly kind: 'help-topic'; readonly topic: string; readonly startLine: number };

export type PilotRequest = {
  readonly v: typeof PILOT_VERSION;
  readonly type: 'pilot.read';
  readonly requestId: string;
  /** UI generation only: never a lease, grant, token or native request key. */
  readonly epoch: number;
  readonly read: PilotRead;
};

/** Supplied by the owning adapter, never accepted as renderer authority. */
export type PilotContext = {
  host: 'vscode' | 'browser';
  readonly epoch: number;
  ready: boolean;
  trusted: boolean;
  nativeEnabled: boolean;
  usageBinding?: string;
};

export type PilotRefusal = 'pilot_not_ready' | 'pilot_untrusted' | 'pilot_native_disabled'
  | 'pilot_context_changed' | 'pilot_binding_mismatch';

export type PilotI2Invocation = Extract<SessionOutcome, { status: 'invoked' }>;
export type PilotViewContext = { epoch: number; binding?: string; available: boolean };
/** Private host dependency, never a renderer message. Native types stay generated. */
export interface NativePilotHost {
  context(): PilotContext;
  session(): object | null;
  invoke(argv: readonly string[], options: { deadlineMs: number; signal?: AbortSignal }): Promise<PilotI2Invocation>;
}

function object(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : null;
}
function fields(value: Record<string, unknown>, expected: readonly string[]): boolean {
  return Object.keys(value).length === expected.length && expected.every(key => Object.hasOwn(value, key));
}
function integer(value: unknown, min: number, max: number): value is number {
  return typeof value === 'number' && Number.isSafeInteger(value) && value >= min && value <= max;
}

/** Exhaustive, read-only surface: no argv, method, URL, path or command passthrough. */
export function parsePilotRequest(value: unknown): PilotRequest | null {
  const request = object(value);
  if (!request || !fields(request, ['v', 'type', 'requestId', 'epoch', 'read'])
      || request.v !== PILOT_VERSION || request.type !== 'pilot.read'
      || typeof request.requestId !== 'string' || !/^[a-zA-Z0-9_-]{1,64}$/.test(request.requestId)
      || !integer(request.epoch, 0, Number.MAX_SAFE_INTEGER)) return null;
  const read = object(request.read);
  if (!read) return null;
  switch (read.kind) {
    case 'usage-json':
      if (!fields(read, ['kind', 'binding', 'days']) || typeof read.binding !== 'string'
          || !/^[0-9a-f]{64}$/.test(read.binding) || !integer(read.days, 1, 365)) return null;
      return Object.freeze({ v: PILOT_VERSION, type: 'pilot.read', requestId: request.requestId, epoch: request.epoch,
        read: Object.freeze({ kind: 'usage-json', binding: read.binding, days: read.days }) });
    case 'help-topic':
      if (!fields(read, ['kind', 'topic', 'startLine']) || typeof read.topic !== 'string'
          || (read.topic !== '' && !/^[a-z][a-z0-9-]{0,63}$/.test(read.topic)) || !integer(read.startLine, 1, 100000)) return null;
      return Object.freeze({ v: PILOT_VERSION, type: 'pilot.read', requestId: request.requestId, epoch: request.epoch,
        read: Object.freeze({ kind: 'help-topic', topic: read.topic, startLine: read.startLine }) });
    default: return null;
  }
}

/** Repeat after awaiting native work: stale results cannot become current state. */
export function admitPilotRead(request: PilotRequest, context: PilotContext): PilotRefusal | null {
  if (!context.ready) return 'pilot_not_ready';
  if (!context.trusted) return 'pilot_untrusted';
  if (!context.nativeEnabled) return 'pilot_native_disabled';
  if (request.epoch !== context.epoch) return 'pilot_context_changed';
  if (request.read.kind === 'usage-json' && request.read.binding !== context.usageBinding) return 'pilot_binding_mismatch';
  return null;
}
