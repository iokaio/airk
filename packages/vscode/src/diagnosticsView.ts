// SPDX-License-Identifier: Apache-2.0
import { randomBytes } from 'node:crypto';
import type { DiagnosticReport } from './diagnostics';

export type DiagnosticsAction = 'refresh' | 'verify' | 'copy' | 'retry' | 'resume' | 'rebuild' | 'showOutput';
const actions: readonly DiagnosticsAction[] = ['refresh', 'verify', 'copy', 'retry', 'resume', 'rebuild', 'showOutput'];

/** Messages select a fixed operation; collection bindings and commands stay in the host. */
export function diagnosticsAction(value: unknown): DiagnosticsAction | undefined {
    if (!value || typeof value !== 'object' || Array.isArray(value)) { return undefined; }
    const message = value as Record<string, unknown>;
    if (Object.keys(message).length !== 2 || !Object.hasOwn(message, 'type') || !Object.hasOwn(message, 'action')
        || message.type !== 'diagnostics' || typeof message.action !== 'string') { return undefined; }
    return actions.find(action => action === message.action);
}

const text = (value: unknown): string => value == null ? 'Not available' : typeof value === 'boolean' ? value ? 'Yes' : 'No'
    : typeof value === 'number' ? Number.isFinite(value) ? value.toLocaleString() : 'Not available'
    : typeof value === 'string' ? value.slice(0, 4096) : 'See copied report';
const escape = (value: unknown): string => text(value).replace(/[&<>"']/g, character =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[character]!);
const label = (value: string): string => value.replace(/([a-z])([A-Z])/g, '$1 $2').replace(/_/g, ' ').replace(/^./, first => first.toUpperCase());
const rows = (entries: [string, unknown][]): string => `<dl>${entries.map(([key, value]) => `<dt>${escape(key)}</dt><dd>${escape(value)}</dd>`).join('')}</dl>`;

function facts(value: unknown, depth = 0): string {
    if (!value || typeof value !== 'object' || Array.isArray(value)) { return `<p>${escape(value)}</p>`; }
    const entries = Object.entries(value).slice(0, 24);
    if (!entries.length) { return '<p class="muted">No additional measurements.</p>'; }
    const scalar: [string, unknown][] = [], nested: string[] = [];
    for (const [key, item] of entries) {
        if (Array.isArray(item) && item.length && depth < 2) {
            nested.push(`<details open><summary>${escape(label(key))}</summary><ol>${item.slice(0, 8)
                .map(entry => `<li>${facts(entry, depth + 1)}</li>`).join('')}</ol></details>`);
        } else if (item && typeof item === 'object' && !Array.isArray(item) && depth < 2) {
            nested.push(`<details${key === 'semantic_readiness' || key === 'discovery_readiness' ? ' open' : ''}><summary>${escape(label(key))}</summary>${facts(item, depth + 1)}</details>`);
        } else { scalar.push([label(key), item]); }
    }
    return rows(scalar) + nested.join('');
}

const probeLabels: Record<keyof DiagnosticReport['probes'], string> = {
    helper: 'Native helper', status: 'Index and semantic progress', schedule: 'Indexing queue',
    vectors: 'Vector model', extraction: 'Index detail', storage: 'Collection storage', integrity: 'Retained data verification'
};
const recommendations: Record<DiagnosticReport['assessment']['recommendedAction'], string> = {
    none: 'No repair is currently indicated.',
    wait: 'Work is active. Refresh diagnostics to check its progress.',
    resume: 'Resume window indexing to continue work that was paused.',
    retry: 'Retry indexing to continue from committed work and check saved files.',
    rebuild: 'Rebuild the derived index if retry cannot recover the retained evidence.',
    review: 'Review the current observations and helper errors before choosing a repair.'
};
const observedWorkActive = (report?: DiagnosticReport): boolean => !!report?.editor?.busy
    || ['indexing', 'enriching'].includes(report?.editor?.state ?? '');

export function renderDiagnosticsReport(name: string, report?: DiagnosticReport,
    state: { busy?: boolean; activity?: string; error?: string; canVerify?: boolean; indexing?: boolean } = {}): string {
    const nonce = randomBytes(24).toString('hex');
    const button = (action: DiagnosticsAction, title: string, unavailable = false) =>
        `<button type="button" data-action="${action}"${state.busy || unavailable ? ' disabled' : ''}>${title}</button>`;
    const active = report?.assessment.state === 'busy'
        || (report?.assessment.state === 'building' && report.assessment.recommendedAction === 'wait');
    const verificationBusy = state.indexing ?? observedWorkActive(report);
    const current = report ? `<section aria-labelledby="current-heading"><h2 id="current-heading">Current observations</h2>
        <p class="muted">Checked ${escape(report.capturedAt)}. Refresh to observe changes.</p>
        ${rows([['Collection', report.binding.name], ['Collection ID', report.binding.collection], ['Workspace', report.binding.root], ['Index storage', report.binding.dataDir], ['Helper', report.binding.helperPath]])}
        ${report.editor ? `<details open><summary>Editor scheduling</summary>${rows([
            ['Window indexing paused', report.editor.paused], ['Collection state', report.editor.state],
            ['Collection work pending', report.editor.pending], ['Collection operation active', report.editor.busy]
        ])}</details>` : ''}
        ${Object.entries(report.probes).map(([key, probe]) => `<details${key === 'status' || key === 'helper' || key === 'integrity' || probe.state === 'failed' ? ' open' : ''}>
        <summary>${escape(probeLabels[key as keyof typeof probeLabels])}: ${escape(probe.state)}</summary>
        <p class="muted">Checked ${escape(probe.checkedAt)} · ${escape(probe.durationMs)} ms</p>
        ${probe.error ? `<p class="error">${escape(probe.error.message)}</p>${rows([
            ['Exit code', probe.error.code], ['Signal', probe.error.signal], ['Timed out', probe.error.timedOut]
        ])}` : ''}${probe.value == null ? '' : facts(probe.value)}</details>`).join('')}</section>` : '';
    const history = report ? `<section aria-labelledby="history-heading"><h2 id="history-heading">Previous attempts</h2>
        <p class="muted">These are earlier attempts. Current observations above describe the latest checks.</p>
        ${report.historyError ? `<p class="error">Previous attempt history is unavailable: ${escape(report.historyError.message)}</p><p class="muted">Current diagnostics and repair actions remain available.</p>` : ''}
        ${report.recentEvents.length ? `<ol>${report.recentEvents.slice(-12).reverse().map(event => `<li><details>
        <summary>${escape(event.at)} · ${escape(label(event.kind))}</summary>
        ${rows([['Helper', event.helperPath], ['Helper version', event.helperVersion], ['Action', event.action]])}
        ${event.error ? `<p class="error">${escape(event.error.message)}</p>` : ''}</details></li>`).join('')}</ol>`
            : '<p>No previous attempts have been recorded for this collection.</p>'}</section>` : '';
    const configuration = report?.agentConfiguration;
    const entryStatus = { matches: 'Matches this helper and collection', different_helper: 'Different helper path',
        different_binding: 'Different collection binding', unresolved: 'Could not compare' };
    const agentConfiguration = configuration ? `<section aria-labelledby="agent-heading"><h2 id="agent-heading">Agent configuration</h2>
        <p class="muted">These observations cover known project configuration files and the Codex user configuration. They do not verify an agent connection or an already running helper.</p>
        ${rows([['Checked', configuration.checkedAt], ['Configuration checks', configuration.state], ['Files checked', configuration.filesChecked],
            ['Files present', configuration.filesPresent], ['Current extension helper', report.binding.helperPath]])}
        ${configuration.entries.length ? `<ol>${configuration.entries.slice(0, 32).map(entry => `<li><details open><summary>${escape(entry.client)} · ${escape(entryStatus[entry.status])}</summary>
            ${rows([['Scope', entry.scope], ['Configuration', entry.configPath], ['Server', entry.serverName], ['Configured helper', entry.helperPath],
                ['Version hint from path', entry.helperVersion]])}${entry.issue ? `<p>${escape(entry.issue)}</p>` : ''}</details></li>`).join('')}</ol>`
            : '<p>No comparable AIRK entries were found in the checked configuration files.</p>'}
        ${configuration.issues.length ? `<ul>${configuration.issues.slice(0, 16).map(issue => `<li>${escape(issue.client)} · ${escape(issue.configPath)}: ${escape(issue.reason)}</li>`).join('')}</ul>` : ''}
        ${configuration.truncated ? '<p class="muted">Some configuration entries were omitted from this bounded report.</p>' : ''}
        <p>For a different helper path, review the current configuration in Connect Coding Agent, update the saved entry, then reload that agent.</p></section>` : '';
    return `<!doctype html><html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1">
    <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'nonce-${nonce}'; script-src 'nonce-${nonce}';">
    <title>AIRK Diagnose &amp; Repair</title><style nonce="${nonce}">
    body{font-family:var(--vscode-font-family);color:var(--vscode-foreground);background:var(--vscode-editor-background);padding:1rem 1.5rem;line-height:1.5;max-width:68rem;margin:auto}
    h1{font-size:1.6rem}h2{font-size:1.2rem}section{margin-top:1.5rem;padding-top:1rem;border-top:1px solid var(--vscode-panel-border)}
    p,dd,summary{overflow-wrap:anywhere}.muted{color:var(--vscode-descriptionForeground)}.error{color:var(--vscode-errorForeground);white-space:pre-wrap}
    nav{display:flex;gap:.5rem;flex-wrap:wrap}button{font:inherit;color:var(--vscode-button-foreground);background:var(--vscode-button-background);border:1px solid transparent;padding:.5rem .8rem;cursor:pointer}
    button:hover{background:var(--vscode-button-hoverBackground)}button:disabled{opacity:.55;cursor:default}button:focus-visible,summary:focus-visible{outline:2px solid var(--vscode-focusBorder);outline-offset:2px}
    dl{display:grid;grid-template-columns:minmax(8rem,1fr) minmax(0,3fr);gap:.4rem 1rem}dt{font-weight:600}dd{margin:0}details{margin:.7rem 0}summary{cursor:pointer}ol{padding-left:1.4rem}
    @media(max-width:450px){dl{grid-template-columns:1fr}dd{margin-bottom:.5rem}}
    </style></head><body><header><h1>Diagnose &amp; Repair</h1><p>${escape(name)}</p></header>
    <div role="status" aria-live="polite">${state.busy ? `<p>${escape(state.activity ?? 'Checking diagnostics…')}</p>` : ''}
    ${report ? `<h2>${state.busy ? 'Last observed: ' : ''}${escape(label(report.assessment.state))}</h2><p>${escape(report.assessment.summary)}</p><p>${escape(recommendations[report.assessment.recommendedAction])}</p>` : '<p>Collect diagnostics for this collection to see its current state.</p>'}</div>
    ${state.error ? `<p class="error" role="alert">${escape(state.error)}</p>` : ''}
    <nav aria-label="Diagnostics">${button('refresh', 'Refresh diagnostics')}${button('verify', 'Verify retained data', !report || state.canVerify === false || verificationBusy)}${button('copy', 'Copy report', !report)}${button('showOutput', 'Show Output')}</nav>
    <p class="muted">Refresh checks a bounded sample of retained data. Verify retained data continues through bounded pages of the same indexed revision; you can cancel it.</p>
    ${verificationBusy ? '<p class="muted">Retained data verification is unavailable while this collection is indexing. Refresh after the current work finishes.</p>' : ''}
    ${current}<section aria-labelledby="repair-heading"><h2 id="repair-heading">Repair choices</h2>
    <p><strong>Retry indexing</strong> checks saved files and continues committed work for this collection.</p>
    <p><strong>Resume window indexing</strong> resumes automatic indexing for the open collections in this window.</p>
    <p><strong>Rebuild index</strong> recalculates the derived index from saved files. Source files are preserved. Large collections can take time to rebuild.</p>
    ${active ? '<p class="muted">Indexing is active or a publication is busy. A retry or rebuild may wait for the current batch.</p>' : ''}
    <nav aria-label="Repair">${button('retry', 'Retry indexing', !report)}${button('resume', 'Resume window indexing', !report)}${button('rebuild', 'Rebuild index', !report)}</nav></section>
    ${agentConfiguration}${history}<p class="muted">The copied report includes local paths, measurements and helper errors for troubleshooting.</p>
    <script nonce="${nonce}">const api=acquireVsCodeApi();const allowed=['refresh','verify','copy','retry','resume','rebuild','showOutput'];document.addEventListener('click',event=>{if(!(event.target instanceof Element))return;const button=event.target.closest('button[data-action]');if(!button||button.disabled||!allowed.includes(button.dataset.action))return;api.postMessage({type:'diagnostics',action:button.dataset.action});});</script></body></html>`;
}

export interface DiagnosticsPanelOptions {
    name: string;
    valid: () => boolean;
    canVerify?: () => boolean;
    diagnose: () => Promise<DiagnosticReport>;
    act: (action: Exclude<DiagnosticsAction, 'refresh' | 'verify' | 'copy'>, report: DiagnosticReport | undefined) => Promise<void>;
    verify?: (report: DiagnosticReport, onProgress: (report: DiagnosticReport) => void,
        shouldContinue: () => boolean) => Promise<DiagnosticReport>;
}

/** Each panel captures one collection. Revalidate the binding around every asynchronous operation. */
export async function openDiagnosticsPanel(options: DiagnosticsPanelOptions): Promise<void> {
    const vscode = await import('vscode');
    const panel = vscode.window.createWebviewPanel('airk.diagnostics', `AIRK Diagnose & Repair: ${options.name}`,
        vscode.ViewColumn.Active, { enableScripts: true, localResourceRoots: [] });
    let report: DiagnosticReport | undefined, busy = false, disposed = false, error: string | undefined, activity: string | undefined;
    const check = () => {
        if (disposed || !vscode.workspace.isTrusted || !options.valid()) {
            throw new Error('The collection, storage selection or workspace trust changed. Reopen Diagnose & Repair for the current collection.');
        }
    };
    const canVerify = () => options.canVerify ? options.canVerify() : !observedWorkActive(report);
    const checkVerification = () => {
        check();
        if (!canVerify()) { throw new Error('This collection is indexing. Refresh diagnostics and verify retained data after the current work finishes.'); }
    };
    const render = () => { if (!disposed) { panel.webview.html = renderDiagnosticsReport(options.name, report, { busy, activity, error, canVerify: !!options.verify, indexing: !canVerify() }); } };
    const refresh = async () => { check(); const observed = await options.diagnose(); check(); report = observed; };
    const run = async (action: DiagnosticsAction) => {
        if (busy || disposed) { return; }
        busy = true; error = undefined;
        activity = { refresh: 'Checking diagnostics…', verify: 'Verifying retained data…', copy: 'Copying report…', retry: 'Retrying indexing…',
            resume: 'Resuming window indexing…', rebuild: 'Rebuilding the derived index…', showOutput: 'Opening AIRK Output…' }[action];
        render();
        try {
            check();
            if (action === 'refresh') { await refresh(); }
            else if (action === 'verify') {
                if (!report || !options.verify) { return; }
                checkVerification();
                const initial = report;
                const verified = await vscode.window.withProgress({ location: vscode.ProgressLocation.Notification,
                    title: `Verifying AIRK retained data: ${options.name}`, cancellable: true }, async (progress, token) => {
                    checkVerification();
                    return options.verify!(initial, observed => {
                        checkVerification(); report = observed;
                        const integrity = observed.probes.integrity.value;
                        activity = `Verifying retained data: ${text(integrity?.checked_files)} of ${text(integrity?.total_files)} files checked…`;
                        progress.report({ message: activity }); render();
                    }, () => !disposed && vscode.workspace.isTrusted && options.valid() && canVerify() && !token.isCancellationRequested);
                });
                checkVerification(); report = verified;
            }
            else if (action === 'copy') {
                if (report) { await vscode.env.clipboard.writeText(JSON.stringify(report, null, 2)); check(); }
            } else {
                if (action !== 'showOutput' && !report) { return; }
                await options.act(action, report); check();
                if (action !== 'showOutput') { await refresh(); }
            }
        } catch (failure) { error = failure instanceof Error ? failure.message : String(failure); }
        finally { busy = false; render(); }
    };
    const listener = panel.webview.onDidReceiveMessage(async value => {
        const action = diagnosticsAction(value);
        if (action) { await run(action); }
    });
    panel.onDidDispose(() => { disposed = true; listener.dispose(); });
    await run('refresh');
}
