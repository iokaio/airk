// SPDX-License-Identifier: Apache-2.0
// `airk-admin/1` envelope parsing (contracts §4.4). Generated types come from the
// Rust owner through packages/contracts-gen/i3.ts; no hand-written mirror here.
import type { ErrorBody, FailureEnvelope, SuccessEnvelope } from '../../contracts-gen/i3';

export const ENVELOPE_SCHEMA = 'airk-admin/1';

export type ParsedResponse =
  | { kind: 'ok'; status: number; envelope: SuccessEnvelope }
  | { kind: 'failed'; status: number; envelope: FailureEnvelope }
  /** No envelope: network failure, or hyper's bare 431/400 (review L5). */
  | { kind: 'transport'; status: number | null; message: string };

function isErrorBody(value: unknown): value is ErrorBody {
  if (typeof value !== 'object' || value === null) return false;
  const v = value as Record<string, unknown>;
  return typeof v.code === 'string' && typeof v.class === 'string' && typeof v.retryable === 'boolean';
}

export function parseEnvelope(status: number, text: string): ParsedResponse {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    return { kind: 'transport', status, message: `HTTP ${status} without an admin envelope.` };
  }
  if (typeof value !== 'object' || value === null) {
    return { kind: 'transport', status, message: `HTTP ${status} without an admin envelope.` };
  }
  const v = value as Record<string, unknown>;
  if (v.schema !== ENVELOPE_SCHEMA) {
    return { kind: 'transport', status, message: `HTTP ${status}: unexpected envelope schema.` };
  }
  if (v.ok === true && typeof v.operation === 'string' && 'result' in v) {
    return { kind: 'ok', status, envelope: value as SuccessEnvelope };
  }
  if (v.ok === false && isErrorBody(v.error)) {
    return { kind: 'failed', status, envelope: value as FailureEnvelope };
  }
  return { kind: 'transport', status, message: `HTTP ${status}: malformed admin envelope.` };
}

/** 401 and every `unauthenticated` class end the session (M2: one uniform 401). */
export function endsSession(parsed: ParsedResponse): boolean {
  if (parsed.status === 401) return true;
  return parsed.kind === 'failed' && parsed.envelope.error.class === 'unauthenticated';
}
