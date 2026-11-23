// SPDX-License-Identifier: Apache-2.0
// The extension-side Answer client: one request builder and one call path for
// Workbench and Admin. Phase 1 is the ordinary one-call path. The job path
// (plan 12.4.2) adds a handle (`continuation`), partial replies and
// cancellation here without changing callers: they observe `AnswerObservation`s
// and receive the final reply.
import type { BudgetV3, ContextRequestV3 } from './generated/arbiter';
import { ANSWER_TRANSPORT_DEADLINE_MS, READ_TRANSPORT_OVERHEAD_MS } from './generated/arbiterBudget';

/** Checked against the Rust-emitted `answer_loop_cases.json` request schema. */
export const ANSWER_REQUEST_SCHEMA = 'airk.arbiter.context.request/3';
export function answerRequest(question: string, budget: BudgetV3, continuation?: string): ContextRequestV3 {
    return { schema: ANSWER_REQUEST_SCHEMA, question, budget: structuredClone(budget), ...(continuation ? { continuation } : {}) };
}
/** One Answer call waits for its own deadline plus the delivery margin, never past Answer's transport ceiling. */
export function answerTransportTimeout(budget: Pick<BudgetV3, 'deadline_ms'>): number {
    const deadline = Number.isSafeInteger(budget.deadline_ms) && budget.deadline_ms > 0 ? budget.deadline_ms : ANSWER_TRANSPORT_DEADLINE_MS;
    return Math.min(ANSWER_TRANSPORT_DEADLINE_MS, deadline + READ_TRANSPORT_OVERHEAD_MS);
}

/** What a caller is told while it waits. `stage` is reserved for relayed loop progress, which no extension transport carries yet. */
export type AnswerObservation =
    | { kind: 'sent'; at: number; timeoutMs: number }
    | { kind: 'stage'; at: number; stage: string; part?: number; round?: number };
export interface AnswerTransport<Result> { call(request: ContextRequestV3, timeoutMs: number): Promise<Result> }
/** Run one Answer to its reply. A job handle in the reply is the job path's to follow; this client returns it as delivered. */
export async function runAnswer<Result>(transport: AnswerTransport<Result>, request: ContextRequestV3,
    observe: (observation: AnswerObservation) => void = () => {}, now: () => number = Date.now): Promise<Result> {
    const timeoutMs = answerTransportTimeout(request.budget);
    observe({ kind: 'sent', at: now(), timeoutMs });
    return transport.call(request, timeoutMs);
}
