// SPDX-License-Identifier: Apache-2.0
// Answer alone runs to its own transport ceiling (final plan 12.4); the values are Rust-emitted.
import { ANSWER_TRANSPORT_DEADLINE_MS, READ_TRANSPORT_DEADLINE_MS } from './generated/arbiterBudget';

/** Whether a public or native tool name is Answer (airk-core `read_attempt::is_answer_tool`). */
export function isAnswerTool(name: unknown): boolean {
    return name === 'airk_answer' || name === 'answer';
}

/** The transport deadline of one tool call: Answer's 605000 ms, 125000 ms for every other tool. */
export function transportDeadlineFor(tool: unknown): number {
    return isAnswerTool(tool) ? ANSWER_TRANSPORT_DEADLINE_MS : READ_TRANSPORT_DEADLINE_MS;
}

/** The Invoke deadline ceiling of one airkctl argv: Answer's for `tools call airk_answer`. */
export function invokeDeadlineLimitFor(argv: readonly string[]): number {
    return argv[0] === 'tools' && argv[1] === 'call' && isAnswerTool(argv[2]) ? ANSWER_TRANSPORT_DEADLINE_MS : READ_TRANSPORT_DEADLINE_MS;
}
