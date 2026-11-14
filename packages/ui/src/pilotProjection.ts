// SPDX-License-Identifier: Apache-2.0
// Operation-specific projections pinned to Rust-emitted UI-pilot native cases.
// Complete original outcomes remain in PilotObservation; nothing is stripped.
import type { PilotObservation } from './pilotContract';

export type PilotProjection =
  | { kind: 'usage'; payload: unknown }
  | { kind: 'help'; payload: unknown; article: Record<string, unknown>; content: string }
  | { kind: 'failure'; code: string; payload?: unknown };
const record = (value: unknown): Record<string, unknown> | null => value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : null;
function json(text: unknown): unknown {
  if (typeof text !== 'string') return undefined;
  try { return JSON.parse(text); } catch { return undefined; }
}

export function projectPilotObservation(observation: PilotObservation): PilotProjection {
  const { result, request } = observation;
  let payload: unknown, failed = false;
  if (result.kind === 'refused') return { kind: 'failure', code: result.code };
  if (result.kind === 'transport') return { kind: 'failure', code: 'transport' };
  if (result.kind === 'i3') {
    if (!result.envelope.ok) return { kind: 'failure', code: result.envelope.error.code, payload: result.envelope.error.native };
    const response = record(result.envelope.result);
    if (request.read.kind === 'usage-json') {
      if (response?.status !== 'usage_document' || !Object.hasOwn(response, 'document')) return { kind: 'failure', code: 'unexpected_usage_reply', payload: response };
      payload = response.document;
    } else {
      if (response?.status !== 'tool_result' || !Object.hasOwn(response, 'result')) return { kind: 'failure', code: 'unexpected_help_reply', payload: response };
      payload = response.result;
    }
  } else {
    payload = json(result.outcome.stdout);
    failed = result.outcome.class !== 'success' || result.outcome.exit_code !== 0;
    if (request.read.kind === 'usage-json' && failed) return { kind: 'failure', code: result.outcome.error?.code ?? result.outcome.class, payload };
  }
  if (request.read.kind === 'usage-json') {
    return payload === undefined ? { kind: 'failure', code: 'unexpected_usage_reply' } : { kind: 'usage', payload };
  }
  const mcp = record(payload);
  const blocks = Array.isArray(mcp?.content) ? mcp.content : [];
  const text = blocks.map(record).find(block => block?.type === 'text');
  const article = record(mcp?.structuredContent) ?? record(json(text?.text));
  const error = record(article?.error);
  if (failed || mcp?.isError === true || error !== null) return { kind: 'failure', code: typeof error?.code === 'string' ? error.code : 'help_error', payload };
  return article && typeof article.content === 'string'
    ? { kind: 'help', payload, article, content: article.content }
    : { kind: 'failure', code: 'unexpected_help_reply', payload };
}

export function pilotNextLine(observation: PilotObservation): number | null {
  const projection = projectPilotObservation(observation);
  const value = projection.kind === 'help' ? projection.article.next_line : null;
  return typeof value === 'number' && Number.isSafeInteger(value) && value > 0 ? value : null;
}
