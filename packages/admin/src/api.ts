// SPDX-License-Identifier: Apache-2.0
// The browser I3 adapter: POST /admin/api/v1/call/<operation> with X-AIRK-CSRF.
import { endsSession, parseEnvelope, type ParsedResponse } from './envelope';
import type { Operation } from './operations';
import { currentSession, dropSession } from './session';

const CALL_PREFIX = '/admin/api/v1/call/';

export const NOT_RUNNING =
  'The service did not answer. If it is stopped, start it with `airk` or `airkctl service start`, then run `airkctl admin open` for a new session.';

export type CallResult = ParsedResponse | { kind: 'no_session' };

/**
 * The body of a review-mode or destructive row (contracts §4.3b, U98): the
 * method without the rule's fields, the session-held review token and the
 * typed confirmation text where the row needs one.
 */
export type ReviewedCall = { method: unknown; review?: string; confirm_text?: string };

export function reviewedCall(method: unknown, review: string | null, confirmText?: string | null): ReviewedCall {
  const reviewed: ReviewedCall = { method };
  if (review !== null) reviewed.review = review;
  if (confirmText !== undefined && confirmText !== null) reviewed.confirm_text = confirmText;
  return reviewed;
}

const REVIEW_TOKEN = /^[0-9a-f]{32}$/;

/**
 * The success envelope's `review` token, issued by a generate-mode row (a
 * draft, preview or keyed mutation). Read defensively: the generated
 * SuccessEnvelope type may predate the field.
 */
export function reviewOf(result: CallResult): string | null {
  if (result.kind !== 'ok') return null;
  const review = (result.envelope as { review?: unknown }).review;
  return typeof review === 'string' && REVIEW_TOKEN.test(review) ? review : null;
}

/** Sends a ReviewedCall for a review-mode or destructive row. */
export function callReviewed(
  operation: Operation,
  method: unknown,
  review: string | null,
  confirmText?: string | null,
): Promise<CallResult> {
  return call(operation, reviewedCall(method, review, confirmText));
}

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
