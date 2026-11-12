// SPDX-License-Identifier: Apache-2.0
// The browser I3 adapter: POST /admin/api/v1/call/<operation> with X-AIRK-CSRF.
import { endsSession, parseEnvelope, type ParsedResponse } from './envelope';
import type { Operation } from './operations';
import { currentSession, dropSession } from './session';

const CALL_PREFIX = '/admin/api/v1/call/';

export const NOT_RUNNING =
  'The service did not answer. If it is stopped, start it with `airk` or `airkctl service start`, then run `airkctl admin open` for a new session.';

export type CallResult = ParsedResponse | { kind: 'no_session' };

export async function call(operation: Operation, body: unknown): Promise<CallResult> {
  const session = currentSession();
  if (session === null) return { kind: 'no_session' };
  let response: Response;
  try {
    response = await fetch(`${CALL_PREFIX}${operation}`, {
      method: 'POST',
      credentials: 'same-origin',
      cache: 'no-store',
      redirect: 'error',
      referrerPolicy: 'no-referrer',
      headers: { 'Content-Type': 'application/json', 'X-AIRK-CSRF': session.csrf },
      body: JSON.stringify(body),
    });
  } catch {
    return { kind: 'transport', status: null, message: NOT_RUNNING };
  }
  const parsed = parseEnvelope(response.status, await response.text());
  // Review M1: the token is removed on any 401.
  if (endsSession(parsed) && currentSession() === session) dropSession();
  return parsed;
}
