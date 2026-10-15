// SPDX-License-Identifier: Apache-2.0
import { randomBytes } from 'node:crypto';

export type ReportKind = 'coverage' | 'usage' | 'retention';
type RecordValue = Record<string, unknown>;
const record = (value: unknown): RecordValue => value && typeof value === 'object' && !Array.isArray(value) ? value as RecordValue : {};
const field = (value: unknown, key: string): unknown => record(value)[key];
const shown = (value: unknown): string => value == null ? 'Unknown' : typeof value === 'boolean' ? value ? 'Yes' : 'No'
    : typeof value === 'number' ? Number.isFinite(value) ? value.toLocaleString() : 'Unknown'
    : typeof value === 'string' ? value.slice(0, 512) : 'See JSON';
const escape = (value: unknown): string => shown(value).replace(/[&<>"']/g, character => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[character]!);
const metric = (label: string, value: unknown): string => `<div class="metric"><span>${escape(label)}</span><strong>${escape(value)}</strong></div>`;
const rows = (entries: [string, unknown][]): string => `<table><tbody>${entries.map(([label, value]) => `<tr><th scope="row">${escape(label)}</th><td>${escape(value)}</td></tr>`).join('')}</tbody></table>`;
const section = (title: string, body: string): string => `<section><h2>${escape(title)}</h2>${body}</section>`;

function retention(value: unknown): string {
    const status = record(value), policy = record(field(status, 'policy')), selection = record(field(policy, 'policy'));
    const last = record(field(status, 'last_maintenance')), pressure = record(field(status, 'pressure'));
    const details: [string, unknown][] = [
        ['Policy', field(selection, 'mode')], ['Policy origin', field(policy, 'origin')],
        ['Protected bytes', field(status, 'protected_bytes')], ['Pending intents', field(status, 'pending_intents')],
        ['Pending bytes', field(status, 'pending_bytes')], ['Last pass', field(last, 'state')],
        ['Maintenance pending', field(last, 'pending')], ['Maintenance error', field(last, 'error')],
        ['Quota bytes needed', field(pressure, 'required_bytes')], ['Quota bytes available', field(pressure, 'available_bytes')]
    ];
    return section('Retention', rows(details));
}

function coverage(value: RecordValue): string {
    const report = record(value.coverage);
    const complete = report.scan_complete === true && report.source_configuration_pending !== true;
    const percent = complete && typeof report.percent === 'number' ? `${Math.floor(report.percent)}%` : 'Unknown';
    const summary = `<div class="metrics">${metric('Coverage', percent)}${metric('Current files', report.current)}${metric('Eligible files', report.eligible)}${metric('Generation', report.generation)}</div>`;
    const details: [string, unknown][] = [
        ['Stale', report.stale], ['Missing', report.missing], ['Extraction failed', report.extraction_failed],
        ['Capacity blocked', report.capacity_blocked], ['Unreadable', report.unreadable],
        ['Pending removals', report.pending_removals], ['Excluded', report.excluded], ['Unsupported', report.unsupported],
        ['Inventory complete', report.scan_complete], ['Source configuration pending', report.source_configuration_pending]
    ];
    return summary + section('Inventory', rows(details)) + retention(value.retention);
}

function usage(value: RecordValue): string {
    const report = record(value.report), health = record(value.health), operations = record(report.operations);
    const summary = `<div class="metrics">${metric('Recorded calls', report.calls)}${metric('Evidence calls', report.eligible_evidence_calls)}${metric('Missing baseline', report.missing_baseline_calls)}${metric('Dropped observations', report.dropped_observations)}</div>`;
    const operationRows = Object.entries(operations).slice(0, 64).map(([name, item]) => {
        const operation = record(item);
        return `<tr><th scope="row">${escape(name)}</th><td>${escape(operation.calls)}</td><td>${escape(operation.successful)}</td><td>${escape(operation.failed)}</td><td>${escape(operation.returned_utf8_bytes)}</td></tr>`;
    }).join('');
    const table = `<table><thead><tr><th>Operation</th><th>Calls</th><th>Successful</th><th>Failed</th><th>Returned UTF-8 bytes</th></tr></thead><tbody>${operationRows || '<tr><td colspan="5">No recorded operations</td></tr>'}</tbody></table>`;
    const details: [string, unknown][] = [
        ['Recording status', value.status], ['Recording mode', field(value.policy, 'mode')],
        ['Database bytes', health.database_bytes], ['Complete history', health.complete_history],
        ['Estimated bill savings', report.estimated_bill_savings], ['Provider tokens observed', report.provider_token_usage_observed]
    ];
    return summary + section('Operations', table) + section('Recording and estimates', rows(details));
}

/** Render bounded, escaped report data. The webview sends only one fixed diagnostic request. */
export function renderReport(title: string, kind: ReportKind, value: unknown): string {
    const nonce = randomBytes(24).toString('hex');
    const data = record(value);
    const body = kind === 'coverage' ? coverage(data) : kind === 'retention' ? retention(data.retention) : usage(data);
    const note = data.explanation ?? (kind === 'usage' ? 'Local aggregates and counterfactual estimates are not measured provider tokens or billed savings.' : undefined);
    return `<!doctype html><html lang="en"><head><meta charset="UTF-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'nonce-${nonce}'; script-src 'nonce-${nonce}';"><meta name="viewport" content="width=device-width, initial-scale=1"><title>${escape(title)}</title><style nonce="${nonce}">
        body{font-family:var(--vscode-font-family);color:var(--vscode-foreground);background:var(--vscode-editor-background);padding:1rem 1.5rem;line-height:1.45}
        header{display:flex;align-items:center;justify-content:space-between;gap:1rem;flex-wrap:wrap}h1{font-size:1.55rem}h2{font-size:1.15rem;margin-top:1.5rem}
        button{color:var(--vscode-button-foreground);background:var(--vscode-button-background);border:0;padding:.5rem .8rem;cursor:pointer}button:hover{background:var(--vscode-button-hoverBackground)}
        .metrics{display:grid;grid-template-columns:repeat(auto-fit,minmax(10rem,1fr));gap:.6rem}.metric{border:1px solid var(--vscode-panel-border);padding:.7rem;display:flex;flex-direction:column;gap:.3rem}.metric span{color:var(--vscode-descriptionForeground)}.metric strong{font-size:1.35rem}
        section{overflow-x:auto}table{border-collapse:collapse;width:100%}th,td{text-align:left;border-bottom:1px solid var(--vscode-panel-border);padding:.48rem .6rem}th{font-weight:600}td{font-variant-numeric:tabular-nums}.note{color:var(--vscode-descriptionForeground)}
        </style></head><body><header><h1>${escape(title)}</h1><button id="open-json" type="button">Open as JSON</button></header>${note == null ? '' : `<p class="note">${escape(note)}</p>`}${body}<script nonce="${nonce}">document.getElementById('open-json').addEventListener('click',()=>acquireVsCodeApi().postMessage({type:'openJson'}));</script></body></html>`;
}
