// SPDX-License-Identifier: Apache-2.0
// The browser I3 adapter: POST /admin/api/v1/call/<operation> with X-AIRK-CSRF.
import { endsSession, parseEnvelope } from './envelope';
import type { ApiResult } from '../../ui/src/apiPort';
import { isNativeUiSession, nativeLifetimeOf } from '../../ui/src/nativeSessionLifetime';
import type { Operation } from './operations';
import { currentSession, dropSession, type Session } from './session';
import { createPreparedMutations, preparationAdmissionRefused, preparationOf, preparationScope, preparedUnresolved, preparesMutation } from './preparedMutations';
export { bindPreparationScope, preparationAdmissionRefused, preparationOf, recoverPreparedResult } from './preparedMutations';

const CALL_PREFIX = '/admin/api/v1/call/';

export const NOT_RUNNING =
  'The service did not answer. If it is stopped, start it with `airk` or `airkctl service start`, then run `airkctl admin open` for a new session.';

export type CallResult = ApiResult;

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

/** Exact native unknowns and transport loss, which cannot establish no effects. */
export function isUnknownOutcome(result: CallResult): boolean {
  if (preparationOf(result)) return preparedUnresolved(result);
  if (preparationAdmissionRefused(result)) return false;
  if (result.kind === 'transport') return true;
  if (result.kind !== 'failed' || result.status !== 503
      || result.envelope.error.class !== 'unavailable' || result.envelope.error.retryable) return false;
  switch (result.envelope.error.code) {
    case 'service_request_outcome_unknown':
    case 'admin_prepared_reply_unavailable':
    case 'catalog_apply_outcome_unknown':
    case 'catalog_index_setup_outcome_unknown':
    case 'catalog_split_apply_outcome_unknown':
    case 'catalog_metadata_saved_outcome_unknown':
    case 'source_change_resolution_outcome_unknown': return true;
    default: return false;
  }
}

/** Failure handles authorize only the matching receipt read, never a generic apply. */
export function recoveryOf(result: CallResult): string | null {
  if (preparationOf(result)) return null;
  if (!isUnknownOutcome(result) || result.kind !== 'failed') return null;
  const review = result.envelope.review;
  return typeof review === 'string' && REVIEW_TOKEN.test(review) ? review : null;
}

const record = (value: unknown): Record<string, unknown> | null =>
  value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : null;
const digest = (value: unknown): boolean => typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);

/** A matching immutable receipt acknowledges a submitted action, not job completion. */
export function receiptAcknowledges(result: CallResult, operation: Operation, method: unknown): boolean {
  if (result.kind !== 'ok') return false;
  const native = record(result.envelope.result);
  const receipt = record(native?.receipt);
  if (!native || !receipt || native.expired !== false) return false;
  if (operation === 'catalog.policy_resume') {
    const submitted = record(record(method)?.catalog_policy_resume);
    const job = record(receipt.job);
    const configuration = record(record(record(receipt.configuration)?.request)?.configuration);
    const policy = record(configuration?.policy);
    if (!submitted || !job || !configuration || !policy) return false;
    return native.status === 'catalog_policy_resume_receipt'
      && native.index_id === submitted.index_id && native.operation_id === submitted.operation_id
      && digest(receipt.source_job) && digest(job.id) && job.kind === 'configure'
      && configuration.operation === 'apply_source_policy' && configuration.operation_id === submitted.operation_id
      && policy.index_id === submitted.index_id && job.project === policy.owner_binding;
  }
  if (operation === 'catalog.split_recover') {
    const submitted = record(record(record(method)?.catalog_split_recover)?.request);
    const acknowledged = record(receipt.request);
    const outcome = record(receipt.outcome);
    if (!submitted || !acknowledged || !outcome || native.status !== 'catalog_split_recovery_receipt'
        || native.operation_id !== submitted.operation_id || !digest(receipt.request_digest)) return false;
    for (const key of ['operation_id', 'request_digest', 'expected_receipt_digest', 'action']) {
      if (acknowledged[key] !== submitted[key]) return false;
    }
    if ((acknowledged.expected_finalization_revision ?? null) !== (submitted.expected_finalization_revision ?? null)) return false;
    switch (outcome.outcome) {
      case 'resume_accepted':
      case 'continuation_accepted':
      case 'finalization_accepted':
      case 'cancel_before_fence_accepted':
      case 'restrictive_completion_required':
      case 'already_complete': return digest(outcome.receipt_digest);
      default: return false;
    }
  }
  return false;
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
  if (preparesMutation(operation)) return preparations(session).run(operation, body, preparationScope(body));
  return send(session, operation, body);
}

const preparedSessions = new WeakMap<object, ReturnType<typeof createPreparedMutations>>();
function preparations(session: Session) {
  let owner = preparedSessions.get(session);
  if (!owner) {
    let storage: Pick<Storage, 'getItem' | 'setItem' | 'removeItem'> | undefined;
    if (!isNativeUiSession(session)) {
      // Failure to access storage is an admission refusal, never a silent
      // downgrade to an unretained browser mutation.
      try { storage = window.sessionStorage; }
      catch { storage = { getItem() { throw Error('storage_unavailable'); }, setItem() { throw Error('storage_unavailable'); }, removeItem() { throw Error('storage_unavailable'); } }; }
    }
    owner = createPreparedMutations({ serviceInstance: session.serviceInstance, storage,
      ...(isNativeUiSession(session) ? { initial: nativeLifetimeOf(session)?.preparations } : {}),
      current: () => currentSession() === session,
      send: (operation, body, acknowledgment) => send(session, operation, body, acknowledgment),
    });
    preparedSessions.set(session, owner);
  }
  return owner;
}
export function pendingPreparations() { const session = currentSession(); return session ? preparations(session).list() : []; }
export function onPreparationChange(listener: () => void): () => void {
  const session = currentSession(); return session ? preparations(session).subscribe(listener) : () => {};
}
export function recoverPreparation(handle: string): Promise<CallResult> {
  const session = currentSession(); return session ? preparations(session).recover(handle) : Promise.resolve({ kind: 'no_session' });
}
export function acknowledgePreparation(handle: string): Promise<CallResult> {
  const session = currentSession(); return session ? preparations(session).acknowledge(handle) : Promise.resolve({ kind: 'no_session' });
}

async function send(session: Session, operation: Operation, body: unknown, acknowledgment = false): Promise<CallResult> {
  if (currentSession() !== session) return { kind: 'no_session' };
  if (isNativeUiSession(session)) {
    const owner = nativeLifetimeOf(session);
    if (!owner) return { kind: 'no_session' };
    const result = await owner.call(operation, body);
    if (result.kind !== 'no_session' && endsSession(result) && currentSession() === session) dropSession();
    return result;
  }
  let response: Response;
  const controller = acknowledgment ? new AbortController() : null;
  const timer = controller ? setTimeout(() => controller.abort(), 15_000) : null;
  try {
    response = await fetch(`${CALL_PREFIX}${operation}`, {
      method: 'POST',
      credentials: 'same-origin',
      cache: 'no-store',
      redirect: 'error',
      referrerPolicy: 'no-referrer',
      headers: { 'Content-Type': 'application/json', 'X-AIRK-CSRF': session.csrf },
      body: JSON.stringify(body),
      ...(controller ? { signal: controller.signal } : {}),
    });
  } catch {
    if (timer !== null) clearTimeout(timer);
    return { kind: 'transport', status: null, message: NOT_RUNNING };
  }
  // Headers already establish authorization loss even if the body never arrives.
  if (response.status === 401 && currentSession() === session) dropSession();
  let text: string;
  try {
    text = await response.text();
  } catch {
    return { kind: 'transport', status: response.status,
      message: `HTTP ${response.status}: the response body could not be read. The request outcome is unconfirmed.` };
  } finally { if (timer !== null) clearTimeout(timer); }
  const parsed = parseEnvelope(response.status, text);
  // Review M1: the token is removed on any 401.
  if (endsSession(parsed) && currentSession() === session) dropSession();
  return parsed;
}
