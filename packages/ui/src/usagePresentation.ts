// SPDX-License-Identifier: Apache-2.0
// A reading aid over the actual native Usage JSON document, never a DTO mirror.
export type UsageTab = 'summary' | 'breakdown' | 'recording';
export type UsageNode = { tag: 'section' | 'h3' | 'p' | 'dl' | 'dt' | 'dd' | 'table' | 'thead' | 'tbody' | 'tr' | 'th' | 'td' | 'details' | 'summary' | 'pre'; children: readonly (UsageNode | string)[] };
const node = (tag: UsageNode['tag'], ...children: (UsageNode | string)[]): UsageNode => ({ tag, children });
const rec = (value: unknown): Record<string, unknown> => value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};
function text(value: unknown): string {
  if (value === null || value === undefined) return 'not available';
  return typeof value === 'string' ? value : JSON.stringify(value);
}
function utc(value: unknown): string {
  return typeof value === 'number' && Number.isSafeInteger(value) && Math.abs(value) <= 8640000000000000
    ? new Date(value).toISOString() : 'not reported';
}
const facts = (values: readonly [string, unknown][]): UsageNode => node('dl', ...values.flatMap(([label, value]) => [node('dt', label), node('dd', text(value))]));
function table(headers: readonly string[], values: readonly (readonly unknown[])[]): UsageNode {
  return node('table', node('thead', node('tr', ...headers.map(header => node('th', header)))),
    node('tbody', ...values.map(row => node('tr', ...row.map(value => node('td', text(value)))))));
}
const complete = (label: string, value: unknown): UsageNode => node('details', node('summary', label), node('pre', JSON.stringify(value, null, 2) ?? 'not available'));

/** Known scalar summaries are explicitly a subset; the full document is retained. */
export function usagePresentation(document: unknown, context: { index: string; days: number }, tab: UsageTab): UsageNode {
  const doc = rec(document), report = rec(doc.report), health = rec(doc.health), policy = rec(doc.policy);
  const header = [node('h3', tab === 'summary' ? 'Usage summary' : tab === 'breakdown' ? 'Usage breakdown' : 'Usage recording'),
    node('p', `Index ${context.index} · requested ${context.days} days · UTC report interval ${utc(report.start_unix_ms)} to ${utc(report.end_unix_ms_exclusive)} (end exclusive).`),
    node('p', `Native status: ${text(doc.status)}. Complete history: ${text(health.complete_history)}. These are recorded observations, not proof of every client call or source freshness.`)];
  const contents: UsageNode[] = [];
  if (typeof doc.notice === 'string') contents.push(node('p', doc.notice));
  if (tab === 'summary') {
    contents.push(facts([
      ['Recorded calls', report.calls], ['Eligible evidence calls', report.eligible_evidence_calls],
      ['Calls with missing baseline', report.missing_baseline_calls], ['Calls with mismatched baseline', report.mismatched_baseline_calls],
      ['Excluded from estimates', report.excluded_from_estimation_calls], ['Native estimated bill savings', report.estimated_bill_savings],
      ['Provider token usage observed', report.provider_token_usage_observed], ['Estimator version', report.estimator_version],
    ]), node('p', 'Savings are native estimates under the reported assumptions, not measured provider bills. A missing measurement stays unavailable; no savings percentage is inferred.'),
    complete('Baseline availability and estimate assumptions', { baseline_unavailable: report.baseline_unavailable, heuristic: report.heuristic,
      comparisons: report.comparisons, price_scenario: doc.price_scenario }));
  } else if (tab === 'breakdown') {
    contents.push(node('h3', 'Operations'), table(['Operation', 'Calls', 'Successful', 'Failed', 'Truncated', 'Returned UTF-8 bytes'],
      Object.entries(rec(report.operations)).map(([name, value]) => { const operation = rec(value); return [name, operation.calls, operation.successful, operation.failed, operation.truncated, operation.returned_utf8_bytes]; })),
    node('h3', 'Clients'), table(['Client', 'Calls'], Object.entries(rec(report.clients))),
    node('h3', 'UTC daily observations'), table(['Start (UTC)', 'End exclusive (UTC)', 'Calls', 'Eligible evidence calls'],
      (Array.isArray(doc.history) ? doc.history : []).map(value => { const day = rec(value); return [utc(day.start_unix_ms), utc(day.end_unix_ms_exclusive), day.calls, day.eligible_evidence_calls]; })));
  } else {
    contents.push(facts([
      ['Recording mode', policy.mode], ['Aggregate retention days', policy.aggregate_retention_days],
      ['Diagnostic retention hours', policy.diagnostic_retention_hours], ['Collection byte limit', policy.max_collection_bytes],
      ['Data-root byte limit', policy.max_data_root_bytes], ['Retained days', health.retained_days],
      ['Dropped observations', health.dropped_observations], ['Process dropped observations', doc.process_dropped_observations],
      ['Duplicate observations', health.duplicate_observations], ['Earliest retained (UTC)', utc(health.earliest_retained_unix_ms)],
      ['Latest retained (UTC)', utc(health.latest_retained_unix_ms)],
    ]), complete('Complete recording health', doc.health), complete('Complete resource observations', doc.resources));
  }
  return node('section', ...header, ...contents, node('p', 'The summaries select fields for reading. Complete operations, provenance, assumptions, history, resources and future fields remain in the native JSON below.'),
    complete('Complete native Usage document (JSON)', document));
}
export function renderUsageNode<T>(create: (tag: UsageNode['tag'], children: readonly (T | string)[]) => T, tree: UsageNode): T {
  return create(tree.tag, tree.children.map(child => typeof child === 'string' ? child : renderUsageNode(create, child)));
}
