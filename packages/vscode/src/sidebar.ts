// SPDX-License-Identifier: Apache-2.0
import { randomBytes } from 'node:crypto';
import { CoverageView, coverageLabel } from './coverage';
import { IndexMode } from './model';
import { VectorStatus } from './vectorModel';

export interface ControlGroup { label: string; commands: [string, string, string][] }
export interface McpVerification {
    phase: 'preparing' | 'checking' | 'verified' | 'failed'; checkedAt?: number; tools?: string[]; error?: string;
    indexState?: 'not_indexed' | 'published';
}
export type IndexDetail = 'agent_lean' | 'essential' | 'balanced' | 'detailed';
export function detailLabel(level: IndexDetail): string {
    return { agent_lean: 'Agent Lean', essential: 'Search Only', balanced: 'Rich Structure', detailed: 'Detailed (advanced)' }[level];
}
export interface DetailStatus {
    configured_detail: IndexDetail; published_detail: IndexDetail | null; pending_reindex: boolean;
}
export interface SidebarCollection {
    collection: string; name: string; root: string; dataDir: string; state: string;
    coverage?: CoverageView; diskBytes?: number; lastIndexed?: string; error?: string;
    mcp?: McpVerification; detail?: DetailStatus; vectors?: VectorStatus; vectorProgress?: string;
    indexActivity?: { phase: string; revision: string | null; processed_artifacts: number; committed_artifacts: number;
        selected_artifacts: number | null; completed_artifacts: number | null; elapsedSeconds: number; quietSeconds: number };
    semanticElapsedSeconds?: number;
    usage?: { calls: number; reduction?: string; compared: number; checkedAt: number; disabled: boolean };
}
export interface SidebarState {
    trusted: boolean; paused: boolean; mode: IndexMode; changingStorage: boolean;
    collections: SidebarCollection[]; groups: ControlGroup[];
    registrationEnabled?: boolean;
    cliStatus?: 'ready' | 'failed' | 'disabled';
}
export type SidebarAction = { type: 'vectors'; collection: string; dataDir: string; enabled: boolean } | { type: 'detail'; collection: string; dataDir: string; level: IndexDetail } | { type: 'ready' } | { type: 'mode'; value: IndexMode }
    | { type: 'registration'; enabled: boolean }
    | { type: 'command'; command: string }
    | { type: 'collection'; action: 'index' | 'reindex' | 'status' | 'usage' | 'verify' | 'mcpDetails' | 'tools' | 'compression' | 'diagnose'; collection: string; dataDir: string };

export function sidebarAction(value: unknown, allowed: Set<string>): SidebarAction | undefined {
    if (!value || typeof value !== 'object') { return; }
    const item = value as Record<string, unknown>;
    if (item.type === 'vectors' && typeof item.collection === 'string' && typeof item.dataDir === 'string' && typeof item.enabled === 'boolean') {
        return { type: 'vectors', collection: item.collection, dataDir: item.dataDir, enabled: item.enabled };
    }
    if (item.type === 'detail' && typeof item.collection === 'string' && typeof item.dataDir === 'string'
        && typeof item.level === 'string' && ['agent_lean', 'essential', 'balanced', 'detailed'].includes(item.level)) {
        return { type: 'detail', collection: item.collection, dataDir: item.dataDir, level: item.level as IndexDetail };
    }
    if (item.type === 'ready') { return { type: 'ready' }; }
    if (item.type === 'registration' && typeof item.enabled === 'boolean') { return { type: 'registration', enabled: item.enabled }; }
    if (item.type === 'mode' && typeof item.value === 'string' && ['manual', 'onSave', 'debounced', 'idle'].includes(item.value)) {
        return { type: 'mode', value: item.value as IndexMode };
    }
    if (item.type === 'command' && typeof item.command === 'string' && allowed.has(item.command)) {
        return { type: 'command', command: item.command };
    }
    if (item.type === 'collection' && typeof item.action === 'string' && ['index', 'reindex', 'status', 'usage', 'verify', 'mcpDetails', 'tools', 'compression', 'diagnose'].includes(item.action)
        && typeof item.collection === 'string' && typeof item.dataDir === 'string') {
        return { type: 'collection', action: item.action as Extract<SidebarAction, { type: 'collection' }>['action'], collection: item.collection, dataDir: item.dataDir };
    }
}

const escape = (value: string | number): string => String(value).replace(/[&<>"']/g,
    character => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[character]!);
const bytes = (value?: number): string => value === undefined ? 'Not measured' : value < 1073741824
    ? `${(value / 1048576).toFixed(1)} MiB` : `${(value / 1073741824).toFixed(2)} GiB`;
const metric = (label: string, value: string | number | undefined): string => `<div class="metric"><strong>${escape(value ?? '—')}</strong><span>${escape(label)}</span></div>`;
const button = (label: string, command: string, disabled = false, secondary = true): string =>
    `<button id="${escape(command)}" data-command="${escape(command)}" class="${secondary ? 'secondary' : 'primary'}"${disabled ? ' disabled' : ''}>${escape(label)}</button>`;

/** Bounded cached observations only; rendering never scans or starts a helper. */
export function sidebarContent(state: SidebarState): string {
    const disabled = !state.trusted;
    const capacityBlocked = state.collections.some(item => item.state === 'capacity-limited');
    const controls = state.trusted ? '' : '<p class="notice">Trust this workspace to enable indexing and agent connections.</p>';
    const modes: [IndexMode, string][] = [['debounced', 'After edits settle'], ['idle', 'When idle'], ['onSave', 'On save'], ['manual', 'Manual']];
    const mode = `<label for="mode">Automatic indexing</label><div class="row"><select id="mode"${disabled ? ' disabled' : ''}>${modes.map(([value, label]) => `<option value="${value}"${state.mode === value ? ' selected' : ''}>${label}</option>`).join('')}</select>${button(state.paused || capacityBlocked ? 'Resume' : 'Pause', state.paused || capacityBlocked ? 'airk.resume' : 'airk.pause', disabled)}</div>`;
    const collections = state.collections.map(item => {
        const view = item.coverage, report = view?.report, label = coverageLabel(view);
        const indexing = item.state === 'indexing' || item.state === 'enriching';
        const status = item.state === 'enriching' ? 'Building semantic index'
            : item.state === 'indexing' ? 'Indexing saved files' : state.paused ? 'Auto-indexing paused'
            : item.state === 'semantic-pending' ? 'Semantic indexing pending'
            : item.state === 'current' && item.vectors?.pending_reindex ? 'Vector change pending'
            : item.state === 'current' && item.detail?.pending_reindex ? 'Detail change pending'
            : item.state === 'current' && (view?.pending || (report && (report.stale || report.missing || report.pending_removals))) ? 'Changes detected'
            : item.state === 'current' && (!report || !report.scan_complete || report.budget_exhausted) ? 'Indexed; coverage unverified' : ({
            queued: 'Changes queued', current: 'Index current', partial: 'Indexed with exclusions', failed: 'Indexing failed',
            'capacity-limited': 'Storage budget reached', paused: 'Auto-indexing paused'
        } as Record<string, string>)[item.state] ?? 'Ready to index';
        const attributes = `data-collection="${escape(item.collection)}" data-storage="${escape(item.dataDir)}"`;
        const actions = `<div class="actions">${(['index', 'reindex', 'status', 'usage', 'compression'] as const).map(action =>
            `<button id="${escape(item.collection)}-${action}" ${attributes} data-action="${action}" class="${action === 'index' ? 'primary' : 'secondary'}"${disabled || (!['status', 'usage'].includes(action) && (indexing || state.changingStorage)) ? ' disabled' : ''}>${action === 'index' ? 'Index now' : action === 'reindex' ? 'Reindex' : action === 'usage' ? 'Usage &amp; Savings' : action === 'compression' ? 'Compression' : 'Details'}</button>`).join('')}</div>`;
        const detail = `<div class="mcp"><h3>Index detail</h3><label for="${escape(item.collection)}-detail">Evidence to retain</label>
            <select id="${escape(item.collection)}-detail" ${attributes} data-detail="true"${disabled || !item.detail || indexing || state.changingStorage ? ' disabled' : ''}>
            ${!item.detail ? '<option>Loading current setting…</option>' : (['agent_lean', 'balanced', 'essential', 'detailed'] as const).map(level => `<option value="${level}"${item.detail?.configured_detail === level ? ' selected' : ''}>${detailLabel(level)}</option>`).join('')}</select>
            <p class="muted">Agent Lean: selected coding and document evidence. Rich Structure: broader facts and declaration tracking. Search Only: text and citations. Vectors are configured separately. Detailed adds call occurrences and document tracking.</p>
            ${item.detail ? `<p role="status">Published: <strong>${escape(item.detail.published_detail ? detailLabel(item.detail.published_detail) : 'Not indexed')}</strong>${item.detail.pending_reindex ? ' · Change pending indexing' : ''}</p>` : '<p class="muted">Current detail is not available yet.</p>'}
            <p class="muted">Changing detail rebuilds evidence. The published index remains available until success. Vector selection is configured separately.</p></div>`;
        const semantic = item.vectors?.semantic_readiness;
        const discovery = item.vectors?.discovery_readiness;
        const discoveryLabel = discovery?.state === 'pending' || discovery?.state === 'partial'
            ? `Discovery ${discovery.state}: ${discovery.committed_artifacts} files committed; total still unknown`
            : discovery?.state === 'failed' ? `Discovery stopped after ${discovery.committed_artifacts} committed files` : '';
        const semanticLabel = semantic?.state === 'pending' || semantic?.state === 'partial'
            ? `Semantic ${semantic.state}: ${semantic.completed_artifacts} of ${semantic.selected_artifacts} selected files complete`
            : semantic?.state === 'failed' ? `Semantic enrichment failed: ${semantic.completed_artifacts} of ${semantic.selected_artifacts} selected files complete`
            : semantic?.state === 'ready' ? (discovery && discovery.state !== 'ready'
                ? 'Semantic evidence ready for committed files; discovery incomplete' : 'Semantic evidence ready') : '';
        const vectors = `<div class="mcp"><h3>Vector search</h3>
            <label class="toggle"><input id="${escape(item.collection)}-vectors" type="checkbox" role="switch" ${attributes} data-vectors="true"${item.vectors?.enabled !== false ? ' checked' : ''}${disabled || !item.vectors || indexing || state.changingStorage ? ' disabled' : ''}> Include meaning-based matches</label>
            <p class="muted">On by default. Combines local vector search with keyword results. First setup downloads about 91 MB of model files; your source files stay on this machine. Uses CPU and additional index storage.</p>
            <p role="status">${escape(item.vectorProgress ?? (!item.vectors ? 'Loading vector setting...' : !item.vectors.enabled ? 'Off: keyword search' : item.vectors.pending_reindex ? 'On: waiting for model setup and indexing' : `On: ${item.vectors.indexed_vectors} indexed vectors`))}</p>
            ${semanticLabel ? `<p class="muted" role="status">${escape(semanticLabel)}</p>` : ''}
            ${item.semanticElapsedSeconds !== undefined ? `<p class="muted" role="status">Semantic batch running for ${escape(Math.floor(item.semanticElapsedSeconds / 60))}m ${escape(item.semanticElapsedSeconds % 60)}s. Published evidence remains available while this batch runs; a batch stops after twenty minutes.</p>` : ''}
            ${discoveryLabel ? `<p class="muted" role="status">${escape(discoveryLabel)}</p>` : ''}
            ${discovery?.failure ? `<p class="error">${escape(discovery.failure.slice(0, 300))}</p>` : ''}
            ${semantic?.failure ? `<p class="error">${escape(semantic.failure.slice(0, 300))}</p>` : ''}
            ${item.vectors?.pending_reindex ? '<p class="muted">Index now to apply the setting. Previous index data remains until a successful rebuild and normal cleanup.</p>' : ''}</div>`;
        const percentage = /^\d+%$/.test(label) ? Number(label.slice(0, -1)) : undefined;
        const progress = percentage === undefined ? '' : `<progress max="100" value="${percentage}" aria-label="Measured index coverage">${percentage}%</progress>`;
        const activity = item.indexActivity;
        const elapsed = activity ? `${Math.floor(activity.elapsedSeconds / 60)}m ${activity.elapsedSeconds % 60}s` : '';
        const published = !!(activity?.revision || item.vectors?.published || item.detail?.published_detail);
        const activeIndex = activity ? `<p class="muted" role="status">Indexing for ${escape(elapsed)}: ${escape(activity.committed_artifacts)} files committed, ${escape(activity.processed_artifacts)} processed. ${published ? 'Published evidence is available to MCP now; discovery remains incomplete.' : 'MCP can start; search awaits the first publication.'}</p>${activity.quietSeconds >= 120 ? `<p class="notice" role="status">No new progress for ${escape(Math.floor(activity.quietSeconds / 60))} minutes. Open Diagnose &amp; Repair if this persists; a stalled scan stops after ten minutes without progress.</p>` : ''}` : '';
        const last = item.lastIndexed ? new Date(item.lastIndexed).toLocaleString() : 'Not indexed in this editor yet';
        const error = item.error || view?.error;
        const mcp = item.mcp, checking = mcp?.phase === 'checking' || mcp?.phase === 'preparing';
        const mcpLabel = !mcp ? 'Not checked' : mcp.phase === 'preparing' ? 'Preparing collection…' : mcp.phase === 'checking' ? 'Checking MCP…'
            : mcp.phase === 'verified' ? `Verified · ${mcp.tools?.length ?? 0} tools available${mcp.indexState === 'not_indexed' ? '; indexing pending' : ''}` : 'Verification failed';
        const connection = `<div class="mcp"><h3>AIRK helper check</h3><p role="status" aria-live="polite">Helper: <strong>${escape(mcpLabel)}</strong></p>
            ${mcp?.checkedAt ? `<p class="muted">Last checked: ${escape(new Date(mcp.checkedAt).toLocaleString())}</p>` : ''}
            ${mcp?.error ? `<p class="error">${escape(mcp.error.slice(0, 600))}</p>` : ''}
            <div class="actions"><button id="${escape(item.collection)}-verify" ${attributes} data-action="verify"${disabled || checking || state.changingStorage ? ' disabled' : ''}>Check AIRK helper</button>
            <button id="${escape(item.collection)}-mcpDetails" class="secondary" ${attributes} data-action="mcpDetails"${disabled || !mcp || checking ? ' disabled' : ''}>View details</button></div>
            <p class="muted">Checks this collection’s helper without waiting for indexing, not whether an agent is connected.</p>
            <button id="${escape(item.collection)}-tools" class="secondary" ${attributes} data-action="tools"${disabled || state.changingStorage ? ' disabled' : ''}>Open AIRK Tools</button>
            <p class="muted">Call tools directly in the Terminal panel. Includes help, examples and JSON results for this collection.</p></div>`;
        return `<section class="collection"><div class="heading"><h2>${escape(item.name)}</h2><span class="badge${indexing ? ' working' : ''}">${escape(status)}</span></div>
            <p class="path" title="${escape(item.root)}">${escape(item.root)}</p>
            <div class="coverage"><strong>${escape(label)}</strong><span>saved-file coverage</span></div>${progress}${activeIndex}
            <div class="metrics">${metric('Current', report?.current)}${metric('Eligible', report?.eligible)}${metric('Stale', report?.stale)}${metric('Missing', report?.missing)}</div>
            ${report ? `<p class="muted">${escape(report.extraction_failed)} extraction failures · ${escape(report.unsupported)} unsupported · ${escape(report.capacity_blocked)} capacity blocked</p>` : '<p class="muted">Coverage appears after the first inventory scan.</p>'}
            ${view?.pending && report ? '<p class="muted">Files changed; these counts are from the previous scan.</p>' : ''}
            ${error ? `<p class="error" role="status">${escape(error.slice(0, 600))}</p>` : ''}${actions}
            <div class="actions"><button class="secondary" data-action="diagnose" ${attributes}${disabled || state.changingStorage ? ' disabled' : ''}>Diagnose &amp; Repair</button></div>
            <p class="muted">${item.usage ? `Last 30 days: ${escape(item.usage.calls)} recorded calls · ${escape(item.usage.reduction ?? 'Not measured')} estimated tokens avoided versus complete cited files (${escape(item.usage.compared)} comparisons). ${item.usage.disabled ? 'Recording is off. ' : ''}Updated ${escape(new Date(item.usage.checkedAt).toLocaleTimeString())}.` : 'Open Usage &amp; Savings to see activity and estimated savings.'}</p>${vectors}${detail}
            <details id="location-${escape(item.collection)}"><summary>Storage and last index</summary><dl><dt>Index size</dt><dd>${bytes(item.diskBytes)}</dd><dt>Eligible source size</dt><dd>${bytes(report?.eligible_source_bytes)}</dd><dt>Last indexed</dt><dd>${escape(last)}</dd><dt>Index folder</dt><dd class="path">${escape(item.dataDir)}</dd></dl></details>${connection}</section>`;
    }).join('');
    const primaryCommands = new Set(['airk.index', 'airk.reindex', 'airk.indexMode', 'airk.pause', 'airk.resume', 'airk.chooseStorage', 'airk.status', 'airk.verifyMcp', 'airk.setupAgent', 'airk.manageMcp', 'airk.manageVscodeMcp', 'airk.storageLimits', 'airk.currentMaintenance']);
    const groups = state.groups.filter(group => group.label !== 'Indexing').map(group => {
        const rows = group.commands.filter(([, command]) => !primaryCommands.has(command) && command !== 'airk.installCli').map(([label, command, help]) =>
            `<div class="control">${button(label, command, disabled && command !== 'airk.logs')}<p class="muted">${escape(help)}</p></div>`).join('');
        return `<details id="group-${escape(group.label)}"${group.label === 'Setup and reports' ? ' open' : ''}><summary>${escape(group.label)}</summary>${rows}</details>`;
    }).join('');
    return `<header><h1>AIRK</h1><span class="muted">Local workspace evidence</span></header>${controls}
        <section><h2>Use AIRK in your terminal</h2>
        <p>Start with <code>airk --help</code> in a new terminal.</p>
        <p class="muted">${disabled ? 'Trust this workspace to install the bundled CLI.' : state.cliStatus === 'failed' ? 'CLI setup needs attention. Install or repair it below; see the AIRK output log for details.' : state.cliStatus === 'disabled' ? 'Automatic CLI installation is off. Install it below when you are ready.' : state.cliStatus === 'ready' ? 'CLI installed. Open a new VS Code terminal. Restart external terminal apps after setup to refresh PATH.' : 'The bundled CLI is installed during trusted startup. If the command is unavailable, install or repair it below.'}</p>
        ${button('Install or repair CLI', 'airk.installCli', disabled)}</section>
        <section>${mode}${state.paused ? '<p class="notice">Automatic jobs are paused. A current pass finishes safely; Index now still works.</p>' : ''}
        ${capacityBlocked && !state.paused ? '<p class="notice">A collection reached its storage limit. Reclaim eligible space or adjust its quota, then Resume.</p>' : ''}
        ${state.changingStorage ? '<p class="notice" role="status">Choosing storage. New indexing jobs are held until this finishes.</p>' : ''}</section>
        ${collections || '<section><h2>No workspace collection</h2><p>Open a folder and trust the workspace to start indexing.</p></section>'}
        <section><h2>Agent connections</h2><label class="toggle"><input id="registration" type="checkbox" role="switch"${state.registrationEnabled !== false ? ' checked' : ''}${disabled ? ' disabled' : ''}> VS Code registration: ${state.registrationEnabled !== false ? 'On' : 'Off'}</label>
        <p class="muted">Automatically offer AIRK collections to VS Code on this extension host. This is not a connection status or a global MCP kill switch.</p>
        ${button('Connect coding agent…', 'airk.manageMcp', disabled, false)}
        ${button('VS Code / Copilot MCP controls', 'airk.manageVscodeMcp', disabled)}
        <p class="muted">Choose Codex, Claude Code or another agent to configure it. VS Code / Copilot controls only manage VS Code connections.</p></section>
        <section><h2>Index storage</h2><p class="muted">Choose a drive for new indexes. Existing indexes stay where they are.</p>${button('Choose storage folder…', 'airk.chooseStorage', disabled || state.changingStorage)}<div class="actions">${button('Storage limits…', 'airk.storageLimits', disabled)}</div><div class="actions">${button('Reclaim unreferenced objects…', 'airk.currentMaintenance', disabled)}</div><p class="muted">Defaults: 100 GiB per collection, 500 GiB total; one current index per collection. Pending updates and cleanup can temporarily use additional space.</p></section>${groups}`;
}

export function renderSidebar(state: SidebarState): string {
    const nonce = randomBytes(24).toString('hex');
    return `<!doctype html><html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'nonce-${nonce}'; script-src 'nonce-${nonce}';"><title>AIRK dashboard</title>
    <style nonce="${nonce}">
    *{box-sizing:border-box}body{margin:0;padding:12px;font-family:var(--vscode-font-family);font-size:var(--vscode-font-size);color:var(--vscode-foreground);background:var(--vscode-sideBar-background);line-height:1.45}
    header{margin:0 0 16px}h1{font-size:18px;margin:0}h2,h3{font-size:13px;margin:0 0 6px;font-weight:600}.mcp{margin-top:14px;padding-top:12px;border-top:1px solid var(--vscode-panel-border)}.mcp p{margin:6px 0}.toggle{display:flex;align-items:center;gap:6px}.toggle input{accent-color:var(--vscode-button-background)}section,details{margin:0 0 12px;padding:12px;border:1px solid var(--vscode-widget-border,var(--vscode-panel-border));border-radius:5px}section details{padding:0;border:0;margin:12px 0 0}
    .heading{display:flex;flex-wrap:wrap;gap:6px;justify-content:space-between}.badge{font-size:11px;color:var(--vscode-descriptionForeground)}.working{color:var(--vscode-progressBar-background)}.working::before{content:'●';margin-right:4px}
    .path{overflow-wrap:anywhere;color:var(--vscode-descriptionForeground);font-size:11px;margin:4px 0 12px}.muted{color:var(--vscode-descriptionForeground);font-size:11px;margin:6px 0}.coverage{display:flex;align-items:baseline;gap:7px;flex-wrap:wrap}.coverage strong{font-size:18px}.coverage span{font-size:11px;color:var(--vscode-descriptionForeground)}progress{width:100%;height:5px;accent-color:var(--vscode-progressBar-background)}
    .metrics{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:6px;margin:12px 0}.metric strong,.metric span{display:block}.metric strong{font-size:16px}.metric span{font-size:11px;color:var(--vscode-descriptionForeground)}.row,.actions{display:flex;gap:6px;flex-wrap:wrap;margin-top:6px}.actions button{flex:1}.row select{flex:1;min-width:100px}button,select{font:inherit;font-size:12px;border:1px solid transparent;border-radius:2px;padding:6px 9px;min-height:28px}button{cursor:pointer;background:var(--vscode-button-background);color:var(--vscode-button-foreground)}button.secondary{background:var(--vscode-button-secondaryBackground);color:var(--vscode-button-secondaryForeground)}button:hover{filter:brightness(1.12)}button:disabled{opacity:.5;cursor:default}button:focus-visible,select:focus-visible,summary:focus-visible{outline:1px solid var(--vscode-focusBorder);outline-offset:2px}select{background:var(--vscode-dropdown-background);color:var(--vscode-dropdown-foreground);border-color:var(--vscode-dropdown-border)}
    summary{cursor:pointer;font-weight:600}.control{margin-top:12px}.control button{text-align:left;width:100%}.notice{padding:8px;border-left:2px solid var(--vscode-focusBorder);background:var(--vscode-textBlockQuote-background)}.error{color:var(--vscode-errorForeground);overflow-wrap:anywhere}dl{margin:8px 0}dt{font-size:11px;color:var(--vscode-descriptionForeground)}dd{margin:0 0 6px;overflow-wrap:anywhere}@media(max-width:240px){body{padding:6px}section,details{padding:8px}.metrics{grid-template-columns:repeat(2,minmax(0,1fr))}}
    </style></head><body><main id="dashboard">${sidebarContent(state)}</main><script nonce="${nonce}">
    const vscode=acquireVsCodeApi(), dashboard=document.getElementById('dashboard');
    function remember(){const details={};document.querySelectorAll('details').forEach(d=>details[d.id]=d.open);vscode.setState({details,scroll:window.scrollY});}
    function restore(state){if(!state)return;Object.entries(state.details||{}).forEach(([id,open])=>{const el=document.getElementById(id);if(el)el.open=open;});window.scrollTo(0,state.scroll||0);}
    restore(vscode.getState());
    dashboard.addEventListener('toggle',remember,true);window.addEventListener('scroll',remember);
    dashboard.addEventListener('click',event=>{const button=event.target.closest('button');if(!button||button.disabled)return;if(button.dataset.action){vscode.postMessage({type:'collection',action:button.dataset.action,collection:button.dataset.collection,dataDir:button.dataset.storage});}else if(button.dataset.command){vscode.postMessage({type:'command',command:button.dataset.command});}});
    dashboard.addEventListener('change',event=>{if(event.target.dataset.vectors)vscode.postMessage({type:'vectors',collection:event.target.dataset.collection,dataDir:event.target.dataset.storage,enabled:event.target.checked});if(event.target.dataset.detail)vscode.postMessage({type:'detail',collection:event.target.dataset.collection,dataDir:event.target.dataset.storage,level:event.target.value});if(event.target.id==='mode')vscode.postMessage({type:'mode',value:event.target.value});if(event.target.id==='registration')vscode.postMessage({type:'registration',enabled:event.target.checked});});
    window.addEventListener('message',event=>{if(event.data.type!=='render')return;remember();const state=vscode.getState(),focus=document.activeElement?.id;dashboard.innerHTML=event.data.html;restore(state);if(focus)document.getElementById(focus)?.focus({preventScroll:true});});
    vscode.postMessage({type:'ready'});
    </script></body></html>`;
}

export function sidebarControlGroups(): ControlGroup[] {
    // One command catalog serves the sidebar and status-bar menu.
    return [
        { label: 'Indexing', commands: [
            ['Index Now', 'airk.index', 'Incremental update of this folder'],
            ['Reindex Collection (Full Re-extraction)', 'airk.reindex', 'Re-extract every eligible file'],
            ['Choose Indexing Mode…', 'airk.indexMode', 'After edits settle · When idle · On save · Manual'],
            ['Pause Auto-indexing', 'airk.pause', 'Stop automatic jobs; keep queued changes'],
            ['Resume Auto-indexing', 'airk.resume', 'Restart automatic jobs after a pause or budget stop']] },
        { label: 'Setup and reports', commands: [
            ['Diagnose & Repair', 'airk.diagnose', 'Inspect current progress and saved failures; retry or rebuild from here'],
            ['Install or Repair CLI', 'airk.installCli', 'Make airk available in new terminals'],
            ['Open AIRK Tools', 'airk.openTools', 'Call MCP tools in a collection terminal with help and examples'],
            ['Connect Coding Agent…', 'airk.setupAgent', 'Choose an agent and follow its setup guide'],
            ['Check AIRK MCP Helper', 'airk.verifyMcp', 'Handshake with the helper and list its tools'],
            ['Manage Agent Connections', 'airk.manageMcp', 'Choose the coding agent to configure'],
            ['VS Code / Copilot MCP Controls', 'airk.manageVscodeMcp', 'Start, stop or enable VS Code connections only'],
            ['Show Collection Status and Coverage', 'airk.status', 'Current, stale, missing and failed files'],
            ['Usage & Savings', 'airk.usage', 'Content-free local aggregates and estimates'],
            ['Show Output Log', 'airk.logs', 'Skipped files and failures']] },
        { label: 'Collections and storage', commands: [
            ['Choose Index Storage Folder…', 'airk.chooseStorage', 'Build new indexes on another drive'],
            ['Configure Storage Limits…', 'airk.storageLimits', 'Collection and shared quotas in GiB'],
            ['Manage Collections and Disk Space', 'airk.collections', 'Sizes, roots and free space'],
            ['Remove Unneeded Collection…', 'airk.removeCollection', 'Delete a derived index; source files stay'],
            ['Reclaim Unreferenced Index Objects', 'airk.currentMaintenance', 'One bounded cleanup pass; current evidence stays']] },
        { label: 'Extraction', commands: [
            ['Show Ingestion Limits', 'airk.ingestionLimits', 'Machine sizing, measured source sizes, quotas and storage path'],
            ['Configure Ingestion Limits…', 'airk.configureIngestionLimits', 'Restore automatic sizing or import explicit limits'],
            ['Show Extraction Policy', 'airk.extractionPolicy', 'Configured versus applied policy'],
            ['Configure Extraction Policy…', 'airk.configureExtractionPolicy', 'Validate a JSON policy for the next index']] }
    ];
}
