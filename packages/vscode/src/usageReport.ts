// SPDX-License-Identifier: Apache-2.0
/** Presentation only: native reports remain the accounting authority. */
type Data = Record<string, unknown>;
const obj = (v: unknown): Data => v && typeof v === 'object' && !Array.isArray(v) ? v as Data : {};
const array = (v: unknown): unknown[] => Array.isArray(v) ? v.slice(0, 512) : [];
const integer = (v: unknown): bigint | undefined => typeof v === 'string' && /^-?\d{1,40}$/.test(v) ? BigInt(v)
    : typeof v === 'number' && Number.isSafeInteger(v) ? BigInt(v) : undefined;
const display = (v: unknown): string => v == null ? 'Not measured' : integer(v)?.toLocaleString('en-US')
    ?? (typeof v === 'boolean' ? v ? 'Yes' : 'No' : typeof v === 'string' ? v.slice(0, 512) : 'Not measured');
const esc = (v: unknown): string => display(v).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);
const card = (label: string, value: unknown, help: string): string => `<div class="metric"><span>${esc(label)}</span><strong>${esc(value)}</strong><small>${esc(help)}</small></div>`;
const table = (headers: string[], rows: unknown[][]): string => `<table><thead><tr>${headers.map(h => `<th>${esc(h)}</th>`).join('')}</tr></thead><tbody>${rows.length ? rows.map(row => `<tr>${row.map(v => `<td>${esc(v)}</td>`).join('')}</tr>`).join('') : `<tr><td colspan="${headers.length}">No measurements available yet.</td></tr>`}</tbody></table>`;
const section = (title: string, body: string): string => `<section><h2>${esc(title)}</h2>${body}</section>`;
const date = (v: unknown): string => typeof v === 'number' && Number.isSafeInteger(v) && v >= 0 && v < 8640000000000000 ? new Date(v).toISOString().slice(0, 10) : 'Unknown date';
const button = (action: string, label: string): string => `<button type="button" data-usage="${action}">${label}</button>`;
const clientName = (v: unknown): string => ({ claude_code: 'Claude Code', codex: 'Codex', vscode: 'VS Code', copilot_cli: 'Copilot CLI', open_code: 'OpenCode', unknown: 'Unspecified client' })[String(v)] ?? display(v);
const recordingName = (v: unknown): string => ({ aggregates_only: 'Daily totals', diagnostic_events: 'Daily totals and short-lived diagnostics', disabled: 'Off' })[String(v)] ?? 'Not available';
const sessionCoverage = (count: unknown, total: unknown): string => `${display(count)} of ${display(total)} sessions measured`;
const comparison = (report: Data): Data => obj(obj(report.comparisons).full_returned_sources);
const baselineReason = (key: string): string => ({
    not_recorded: 'Reason not recorded by the earlier recorder', missing_citation: 'No usable source citation',
    too_many_sources: 'Too many cited sources', missing_revision: 'Response has no indexed revision',
    revision_unavailable: 'The cited revision is no longer available', revision_mismatch: 'Revision or response format differs',
    source_unavailable: 'Cited source is unavailable', source_mismatch: 'Cited source hash differs',
    inventory_limit: 'Older index exceeds the comparison inventory limit', source_payload_limit: 'Older source payload exceeds the comparison limit',
    source_text_limit: 'Older source text exceeds the comparison limit', measurement_error: 'Comparison could not be verified'
})[key] ?? 'Unrecognized comparison reason';
const evidenceOperations = new Set(['search', 'read_file', 'outline', 'symbols', 'imports', 'document_facts', 'references', 'context', 'code_facts', 'relationships']);
export function reductionPercent(baseline: unknown, returned: unknown): string {
    const b = integer(baseline), r = integer(returned);
    if (b === undefined || r === undefined || b <= 0n) { return 'Not measured'; }
    const tenths = (b - r) * 1000n / b;
    return `${tenths < 0n ? '-' : ''}${(tenths < 0n ? -tenths : tenths) / 10n}.${(tenths < 0n ? -tenths : tenths) % 10n}%`;
}
const measured = (s: Data, key: string, count: string): unknown => integer(s[count]) === 0n || integer(s[count]) === undefined ? undefined : s[key];
const seconds = (value: unknown): string => {
    const ms = integer(value); if (ms === undefined) { return 'Not measured'; }
    const magnitude = ms < 0n ? -ms : ms;
    return `${ms < 0n ? '-' : ''}${(magnitude / 1000n).toLocaleString('en-US')}.${String(magnitude % 1000n).padStart(3, '0')} s`;
};
function pairedDifference(a: Data, b: Data, key: string, count: string): string {
    const n = integer(a.sessions);
    if (!n || n !== integer(b.sessions) || n !== integer(a[count]) || n !== integer(b[count])) { return 'Not enough comparable measurements'; }
    const av = integer(a[key]), bv = integer(b[key]);
    return av === undefined || bv === undefined ? 'Not measured' : key === 'elapsed_ms' ? seconds((bv - av).toString()) : (bv - av).toLocaleString('en-US');
}

export type UsageAction = { type: 'usage'; action: string; days?: number };
export function usageAction(value: unknown): UsageAction | undefined {
    const v = obj(value);
    if (v.type !== 'usage' || typeof v.action !== 'string') { return; }
    if (v.action === 'days' && Object.keys(v).length === 3 && [7, 30].includes(v.days as number)) {
        return { type: 'usage', action: 'days', days: v.days as number };
    }
    if (Object.keys(v).length === 2 && ['refresh', 'json', 'csv', 'markdown', 'recording', 'clear', 'import', 'prices', 'removePrices', 'template', 'feedback'].includes(v.action)) {
        return { type: 'usage', action: v.action };
    }
}

export function renderUsage(value: unknown): string {
    const v = obj(value), report = obj(v.report), health = obj(v.health), c = comparison(report), estimate = obj(c.token_estimate);
    const hasComparison = (integer(c.compared_calls) ?? 0n) > 0n;
    const compared = integer(c.compared_calls) ?? 0n, eligible = integer(report.eligible_evidence_calls);
    const unavailable = eligible === undefined ? undefined : eligible - compared;
    const reasons: unknown[][] = Object.entries(obj(report.baseline_unavailable)).map(([reason, count]) => [baselineReason(reason), count]);
    const reasonCount = Object.values(obj(report.baseline_unavailable)).reduce<bigint>((sum, count) => sum + (integer(count) ?? 0n), 0n);
    const unclassified = (integer(report.missing_baseline_calls) ?? 0n) + (integer(report.mismatched_baseline_calls) ?? 0n) - reasonCount;
    if (unclassified > 0n) { reasons.push([baselineReason('not_recorded'), unclassified.toString()]); }
    const operationEntries = Object.entries(obj(report.operations));
    const evidenceCalls = operationEntries.length ? operationEntries.filter(([name]) => evidenceOperations.has(name)).reduce((sum, [, raw]) => sum + (integer(obj(raw).calls) ?? 0n), 0n) : undefined;
    const otherCalls = evidenceCalls === undefined || integer(report.calls) === undefined ? undefined : integer(report.calls)! - evidenceCalls;
    const days = v.days === 7 ? 7 : 30;
    const controls = `<nav aria-label="Usage report controls"><label for="usage-days">Show <select id="usage-days"><option value="7"${days === 7 ? ' selected' : ''}>Last 7 days</option><option value="30"${days === 30 ? ' selected' : ''}>Last 30 days</option></select></label> ${button('refresh', 'Refresh')} ${button('csv', 'Export spreadsheet')} ${button('markdown', 'Export summary')}</nav>`;
    const status = v.status === 'disabled' ? 'Recording is off. Earlier records can still appear below.' : v.status === 'empty' ? 'No usage recorded yet. Connect an agent and ask it to use AIRK to find something in this collection.'
        : 'This report covers recorded activity. Some requests may be missing; it is not a complete account of your agent sessions.';
    const headline = `<p>${esc(obj(report.scope).collection)} · ${date(report.start_unix_ms)} to ${date(typeof report.end_unix_ms_exclusive === 'number' ? report.end_unix_ms_exclusive - 1 : undefined)} (UTC)</p><p role="status">${esc(status)}</p>`;
    const cards = `<div class="metrics">${card('Estimated tokens avoided', hasComparison ? estimate.nominal_reduction : undefined, 'Estimated tokens avoided compared with sending the complete cited files.')}${card('Smaller results', hasComparison ? reductionPercent(c.baseline_utf8_bytes, c.returned_utf8_bytes) : undefined, 'Only requests with a usable comparison are included. Negative means larger results.')}${card('Requests compared', c.compared_calls ?? 0, `${display(report.eligible_evidence_calls)} evidence requests were eligible for comparison.`)}${card('Recorded AIRK calls', report.calls, 'Includes searches, reads and maintenance. A call does not prove a useful result.')}</div>`;
    const coverage = `<p><strong>${esc(compared.toString())} of ${esc(eligible?.toString())} eligible requests compared; ${esc(unavailable?.toString())} unavailable.</strong> Evidence requests: ${esc(evidenceCalls?.toString())}. Background and other activity: ${esc(otherCalls?.toString())}. Maintenance and coverage checks receive no token-savings credit.</p>`;
    const explain = `<p>A token is a small piece of text an AI reads. AIRK returns selected passages instead of entire files. We compare those passages, including response formatting, with the complete files cited by the result. Your agent might otherwise have read only part of a file, so this is an estimate, not measured token or bill savings.</p>
        <p>${hasComparison ? `Using different text-to-token assumptions gives ${esc(estimate.assumption_low)} to ${esc(estimate.assumption_high)} tokens of reduction. This range is not a guarantee.` : 'There are no usable file comparisons yet. Check the reasons below. After updating AIRK, index this collection to create compact source measurements and restart connected agent MCP sessions. Verify that your agent uses this collection and storage directory.'}</p>
        ${reasons.length ? table(['Why comparisons were unavailable', 'Requests'], reasons) + '<p>Older missing comparisons cannot be reconstructed from daily totals. New measurements do not fill in earlier history.</p>' : ''}
        <p>Time saved and actual money saved: <strong>not established by AIRK activity alone</strong>. Import matched task results below to compare completed work.</p>`;
    const history = array(v.history).map(raw => { const day = obj(raw), comp = comparison(day); return [date(day.start_unix_ms), day.calls, comp.compared_calls ?? 0, obj(comp.token_estimate).nominal_reduction]; });
    const lifetime = obj(v.lifetime), lc = comparison(lifetime);
    const longer = Object.keys(lifetime).length ? `<p>Since ${date(lifetime.start_unix_ms)}: <strong>${esc(lifetime.calls)} recorded calls</strong> and <strong>${esc(obj(lc.token_estimate).nominal_reduction)} estimated tokens of context reduction</strong> across ${esc(lc.compared_calls ?? 0)} comparisons.</p><p>These partial totals survive the 30-day detail window. They start when this version first records a call, exclude earlier activity and imported sessions, and reset when you clear history.</p>`
        : '<p>Longer-term totals start with the first recorded AIRK call in this version. Earlier activity is not reconstructed.</p>';
    const sessions: unknown[][] = [], pairs: unknown[][] = [], calibrations: unknown[][] = [];
    for (const raw of array(v.supplemental)) {
        const bucket = obj(raw);
        for (const rawSummary of array(bucket.summaries)) {
            const s = obj(rawSummary), stratum = obj(s.stratum), session = obj(s.sessions), a = obj(s.paired_with_airk), b = obj(s.paired_without_airk);
            const label = `${clientName(stratum.client)} · ${date(typeof bucket.day === 'number' ? bucket.day * 86400000 : undefined)}${typeof stratum.session_model_revision_sha256 === 'string' ? ` · model revision ${stratum.session_model_revision_sha256.slice(0, 8)}` : ''}`;
            if ((integer(session.sessions) ?? 0n) > 0n) { sessions.push([label, session.sessions, measured(session, 'input_tokens', 'input_measured_sessions'), measured(session, 'cached_input_tokens', 'cached_input_measured_sessions'), measured(session, 'output_tokens', 'output_measured_sessions'), seconds(measured(session, 'elapsed_ms', 'elapsed_measured_sessions')), s.linked_airk_requests ?? 0]); }
            if ((integer(a.sessions) ?? 0n) > 0n) { pairs.push([label, a.sessions, pairedDifference(a, b, 'input_tokens', 'input_measured_sessions'), pairedDifference(a, b, 'elapsed_ms', 'elapsed_measured_sessions'), pairedDifference(a, b, 'tool_round_trips', 'round_trip_measured_sessions'), array(s.paired_quality).map(display).join(' / ')]); }
            if ((integer(s.context_pairs) ?? 0n) > 0n) { calibrations.push([label, stratum.provenance, s.context_pairs, s.tokenizer_pairs, (integer(s.tokenizer_pairs) ?? 0n) > 0n ? s.token_reduction : obj(s.byte_heuristic).nominal_reduction]); }
        }
    }
    const supplied = `<p>Optional measurements supplied by you or your agent. They are separate from AIRK’s own counters and are not verified bills. Missing counters stay “Not measured”. Session totals can include activity beyond AIRK.</p>${button('import', 'Import measurements…')} ${button('template', 'Create measurement template')} ${button('feedback', 'Rate an AIRK result…')}
        ${table(['Client / day', 'Sessions', 'Input tokens', 'Cached input tokens', 'Output tokens', 'Measured time', 'Linked AIRK calls'], sessions)}
        <h3>Same-task comparisons</h3><p>Positive differences mean less input, time or tool work with AIRK; negative differences mean more. A difference is shown only when both sides have all measurements. Check task quality before judging savings. Quality counts: both passed / only AIRK passed / only alternative passed / both failed / unknown. Matching conditions are supplied by the evaluator, not verified here.</p>${table(['Client / day', 'Pairs', 'Fewer input tokens', 'Less time', 'Fewer tool rounds', 'Quality counts'], pairs)}
        <details><summary>Tokenizer and alternative-context measurements</summary>${table(['Client / day', 'Evidence source', 'Comparisons', 'Tokenizer-counted pairs', 'Signed reduction'], calibrations)}</details>`;
    const price = obj(v.price_scenario), scenario = obj(price.scenario);
    const money = (amount: unknown): string => {
        if (typeof amount !== 'string' || !/^-?\d{1,40}\.\d{1,12}$/.test(amount)) { return 'Not measured'; }
        const [whole, fraction] = amount.split('.');
        return `${display(scenario.currency)} ${whole}.${fraction.replace(/0+$/, '').padEnd(2, '0')}`;
    };
    const costRows = array(price.context_equivalents).map(raw => { const item = obj(raw); return [item.baseline_kind ?? obj(item.stratum).baseline_kind, item.compared_pairs, money(item.nominal_input_cost_equivalent), money(item.assumption_low), money(item.assumption_high)]; });
    const sessionCosts = array(price.session_input_equivalents).map(raw => { const s = obj(raw); return [`${clientName(obj(s.stratum).client)} · ${display(s.kind)}`, s.sessions, `${money(s.uncached_input_equivalent)} (${sessionCoverage(s.cache_classified_sessions, s.sessions)})`, money(s.cached_input_equivalent), `${money(s.output_equivalent)} (${sessionCoverage(s.output_measured_sessions, s.sessions)})`, s.unpriced_input_tokens_unknown_cache_split]; });
    const pairedCosts = array(price.paired_cost_equivalents).map(raw => { const s = obj(raw); return [s.pairs, money(s.uncached_input_reduction), money(s.cached_input_reduction), money(s.output_reduction), array(s.quality).map(display).join(' / ')]; });
    const costs = `<p>See what the measured or estimated tokens would cost at rates you supply. This does not establish a smaller bill, and a subscription may cost the same regardless of usage. Prices apply to this report only; check that the selected model and rates match your measurements.</p>${button('prices', 'Set cost assumptions…')} ${button('removePrices', 'Remove cost assumptions')}
        ${price.scenario ? `<p>Model: ${esc(scenario.model)} · ${esc(scenario.currency)} · rates dated ${esc(scenario.rate_date)}. Input: ${esc(scenario.input_price_per_million)} / million tokens; output: ${esc(scenario.output_price_per_million)} / million tokens. Cached input: ${esc(scenario.cached_input_treatment)} (${esc(scenario.cached_input_price_per_million)} / million). Context estimate assumes uncached input: ${esc(scenario.context_assumes_uncached)}.</p>${table(['Comparison', 'Requests', 'Estimated input-cost reduction', 'Lower assumption', 'Upper assumption'], costRows)}<h3>Supplied session cost equivalents</h3>${table(['Session type', 'Sessions', 'Uncached input cost', 'Cached input cost', 'Output cost', 'Input tokens with unknown cache split'], sessionCosts)}<h3>Matched-task cost differences</h3><p>Positive amounts mean lower cost with AIRK at these rates; negative amounts mean higher cost. Missing measurements are not priced. Review the task quality counts above before interpreting a reduction.</p>${table(['Pairs', 'Less uncached-input cost', 'Less cached-input cost', 'Less output cost', 'Quality counts'], pairedCosts)}` : '<p>No cost assumptions selected.</p>'}`;
    const operations = Object.entries(obj(report.operations)).slice(0, 64).map(([name, raw]) => { const s = obj(raw); return [name, s.calls, s.successful, s.empty, s.failed, s.truncated, s.returned_utf8_bytes, s.total_elapsed_us]; });
    const index = Object.entries(obj(report.operations)).filter(([name]) => ['index', 'reindex'].includes(name)).map(([name, raw]) => { const s = obj(obj(raw).index_work); return [name, s.indexed, s.reused, s.removed, s.processed_source_bytes, s.scan_extract_us, s.publish_us, s.disk_growth_bytes]; });
    const embeddingLabels: Record<string, string> = {
        embedding_inputs: 'Embedding inputs', embedding_blank_inputs: 'Blank inputs skipped',
        embedding_duplicate_inputs: 'Duplicate inputs reused', embedding_actual_forwards: 'Neural model executions',
        embedding_tokenized_inputs: 'Tokenized cache miss inputs', embedding_input_tokens: 'Tokens before truncation',
        embedding_embedded_tokens: 'Embedded tokens', embedding_padding_tokens: 'Padding slots',
        embedding_truncated_inputs: 'Truncated inputs', embedding_plan_compilations: 'Compiled inference plans',
        embedding_plan_compile_us: 'Plan compilation µs', embedding_cache_hits: 'Cached inputs',
        vector_units: 'Semantic inputs', vector_body_bytes: 'Semantic body bytes', vector_planning_us: 'Semantic input planning µs',
        embedding_cache_misses: 'Cache miss inputs', embedding_model_load_us: 'Model loading µs',
        embedding_cache_lookup_us: 'Cache lookup µs', embedding_cache_store_us: 'Cache persistence µs',
        embedding_inference_us: 'Inference µs', vector_graph_insert_us: 'Graph insertion µs'
    };
    const embedding = Object.entries(obj(report.operations)).filter(([name]) => ['index', 'reindex'].includes(name)).flatMap(([name, raw]) => {
        const metrics = obj(obj(obj(raw).index_work).metrics);
        return Object.entries(embeddingLabels).map(([key, label]) => { const metric = obj(metrics[key]); return [name, label, measured(metric, 'value', 'samples'), metric.samples]; });
    });
    const resources = obj(v.resources);
    const technical = `<details><summary>Measurement details for developers</summary>
        <p>Compared full-source bytes: ${esc(c.baseline_utf8_bytes)}. Returned bytes for those same requests: ${esc(c.returned_utf8_bytes)}. Missing baselines: ${esc(report.missing_baseline_calls)}; mismatched: ${esc(report.mismatched_baseline_calls)}; excluded calls: ${esc(report.excluded_from_estimation_calls)}. Estimates use four bytes per nominal token and two-to-six-byte sensitivity assumptions.</p>
        ${table(['Operation', 'Calls', 'Successful', 'Empty', 'Failed', 'Truncated', 'Response bytes', 'Total microseconds'], operations)}
        <h3>Indexing cost</h3>${table(['Operation', 'Changed files', 'Reused files', 'Removed files', 'Source bytes', 'Scan/extract µs', 'Publish µs', 'Disk growth bytes'], index)}<p>Reuse avoids extraction work; it does not prove avoided disk reads. Zero stage totals without samples are not measured durations; inspect JSON for sample counts.</p>
        <h3>Embedding work</h3>${table(['Operation', 'Measurement', 'Total', 'Measured jobs'], embedding)}<p>Blank and duplicate inputs do not each require a neural model execution. Token counts cover executed cache misses and include special tokens. Semantic input planning includes boundary selection and token checks, excluding embedding, cache and graph work. Inference time includes worker scheduling and any plan compilation. These phase timings are inside publication time; do not add them to publication time.</p>
        <h3>Process resources</h3>${table(['Counter', 'Total', 'Measured intervals'], Object.entries(obj(resources.counters)).map(([key, raw]) => [key, obj(raw).value, obj(raw).samples]))}<p>Peak process memory: ${esc(resources.peak_rss_bytes)} bytes. Overlapping intervals excluded: ${esc(resources.overlapped_intervals)}. Process counters may include background work; transfer bytes are not physical disk I/O.</p>
        <h3>Recording health</h3><p>History completeness: not certified. Missing observations across processes: ${esc(report.dropped_observations)}. Drops in this report process: ${esc(v.process_dropped_observations)}. Ledger size: ${esc(health.database_bytes)} bytes. Duplicate requests ignored: ${esc(health.duplicate_observations)}.</p>
        ${table(['Client', 'Recorded calls'], Object.entries(obj(report.clients)))}${table(['Content type', 'Recorded calls'], Object.entries(obj(report.content_kinds)))}
        <p>Explicit feedback: ${esc(health.useful_feedback)} useful, ${esc(health.partly_useful_feedback)} partly useful, ${esc(health.not_useful_feedback)} not useful. Unrated results have unknown usefulness.</p></details>`;
    return controls + headline + cards + coverage + section('What do these numbers mean?', explain) + section('Daily activity', table(['UTC date', 'Recorded calls', 'Compared requests', 'Estimated context reduction (tokens)'], history))
        + section('Longer-term progress', longer) + section('Agent sessions and task results', supplied) + section('What might this be worth?', costs) + technical
        + section('Your local usage history', `<p>Recording: ${esc(recordingName(obj(v.policy).mode))}. Details are retained for ${esc(obj(v.policy).aggregate_retention_days)} days. Queries, file contents and prompts are not stored in the usage ledger.</p>${button('recording', 'Change recording…')} ${button('clear', 'Clear usage history…')}`);
}
