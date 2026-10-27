// SPDX-License-Identifier: Apache-2.0
import { randomBytes } from 'node:crypto';
import * as vscode from 'vscode';
import type { ExplorerViewState } from './explorerContracts';
import { headline } from './explorerStatus';
import { SERVICE_DASHBOARD_STYLE, serviceDashboardBody } from './serviceDashboard';

const esc = (value: unknown): string => String(value).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);
const age = (ms: number | undefined): string => ms === undefined ? 'not observed' : `${Math.max(0, Math.round((Date.now() - ms) / 1000))} s ago`;

export type SidebarMessage = { type: 'sidebar'; action: 'open' } | { type: 'sidebar'; action: 'search'; query: string };

/** Only the two sidebar actions are accepted; a query is bounded text, never a path or command. */
export function sidebarMessage(input: unknown): SidebarMessage | undefined {
    if (!input || typeof input !== 'object' || Array.isArray(input)) { return undefined; }
    const value = input as Record<string, unknown>, keys = Object.keys(value).sort().join(',');
    if (value.type !== 'sidebar') { return undefined; }
    if (value.action === 'open' && keys === 'action,type') { return { type: 'sidebar', action: 'open' }; }
    if (value.action === 'search' && keys === 'action,query,type' && typeof value.query === 'string'
        && value.query.trim().length > 0 && value.query.length <= 4096 && !/[\u0000-\u001f\u007f]/.test(value.query)) {
        return { type: 'sidebar', action: 'search', query: value.query };
    }
    return undefined;
}

/** Compact live status, quick search and first-run guidance for the AIRK activity bar view. */
export function sidebarContent(state: ExplorerViewState | undefined, folders: number): string {
    const registered = state?.projects.length ?? 0;
    const trusted = state?.trusted ?? false;
    if (!state || !trusted || registered === 0) { return serviceDashboardBody(trusted, folders, registered); }
    const project = state.projects.find(entry => entry.project === state.selectedProject);
    const status = state.status;
    const { word, severity } = headline(state.connection, status, registered);
    const facts: [string, unknown][] = [
        ['Project', project ? project.name : 'None selected'],
        ['Service', state.connection],
        ['Published index', status ? status.indexState : 'not observed'],
        ['Indexed files', status?.files ?? 'not observed'],
        ['Semantic coverage', status ? status.semantic : 'not observed'],
        ['Source work', status ? status.freshness : 'not observed'],
        ['Observed', age(status?.observedAt)],
    ];
    return `<h1>AIRK</h1>
<p role="status"><span class="pill ${severity}">${esc(word)}</span> ${registered} registered project${registered === 1 ? '' : 's'}</p>
<dl>${facts.map(([name, value]) => `<dt>${esc(name)}</dt><dd>${esc(value)}</dd>`).join('')}</dl>
<p class="note">Readiness is the published index; it is not proof that live files match. Counts come from AIRK; missing values stay unobserved.</p>
<form id="search" role="search"><label for="query">Search indexed evidence</label><input id="query" name="query" maxlength="4096" placeholder="Words that must all appear" required>
<button type="submit">Search</button></form>
<button type="button" class="primary" data-action="open">Open AIRK Explorer</button>
${state.suggestions?.open ? `<p role="status"><a href="command:airk.explorer.showSuggestions">${state.suggestions.open} agent suggestion${state.suggestions.open === 1 ? '' : 's'} waiting for review</a></p>` : ''}
${state.notice ? `<p class="note" role="alert">${esc(state.notice.message)}</p>` : ''}
<p><a href="command:airk.setupAgent">Connect a coding agent</a> · <a href="command:airk.collections">Management</a></p>`;
}

export function renderSidebar(state: ExplorerViewState | undefined, folders: number, nonce: string): string {
    return `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'nonce-${nonce}'; script-src 'nonce-${nonce}';">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<style nonce="${nonce}">${SERVICE_DASHBOARD_STYLE}
dl{display:grid;grid-template-columns:auto 1fr;gap:2px 10px}dt{color:var(--vscode-descriptionForeground)}dd{margin:0}
.pill{border-radius:10px;padding:1px 8px;border:1px solid var(--vscode-panel-border);font-weight:600}
.pill.ok{border-color:var(--vscode-testing-iconPassed,green)}.pill.warn{border-color:var(--vscode-editorWarning-foreground,orange)}.pill.error{border-color:var(--vscode-editorError-foreground,red)}
form{display:flex;flex-direction:column;gap:6px;margin:12px 0}input{padding:6px;color:var(--vscode-input-foreground);background:var(--vscode-input-background);border:1px solid var(--vscode-input-border,transparent)}
button{font:inherit;padding:6px 10px;color:var(--vscode-button-foreground);background:var(--vscode-button-background);border:0;cursor:pointer}
button.primary{width:100%}button:focus-visible,input:focus-visible{outline:2px solid var(--vscode-focusBorder);outline-offset:2px}</style>
</head><body><main id="content">${sidebarContent(state, folders)}</main>
<script nonce="${nonce}">const vscode=acquireVsCodeApi();
document.addEventListener('click',event=>{const button=event.target.closest('button[data-action="open"]');if(button)vscode.postMessage({type:'sidebar',action:'open'});});
document.addEventListener('submit',event=>{event.preventDefault();const input=document.getElementById('query');if(input&&input.value.trim())vscode.postMessage({type:'sidebar',action:'search',query:input.value});});
window.addEventListener('message',event=>{const value=event.data;if(value&&value.type==='render'&&typeof value.html==='string'){const query=document.getElementById('query');const kept=query?query.value:'';document.getElementById('content').innerHTML=value.html;const next=document.getElementById('query');if(next)next.value=kept;}});</script>
</body></html>`;
}

/** The AIRK activity bar view (id airk.dashboard): status card, quick search and setup guidance. */
export class ExplorerSidebar implements vscode.WebviewViewProvider, vscode.Disposable {
    private view?: vscode.WebviewView;
    private state?: ExplorerViewState;
    private readonly listeners: vscode.Disposable[] = [];

    constructor(private readonly options: {
        commandUris: readonly string[];
        folders(): number;
        attach(listener: (state: ExplorerViewState) => void): Promise<vscode.Disposable>;
        open(): void;
        search(query: string): void;
    }) {}

    resolveWebviewView(view: vscode.WebviewView): void {
        this.view = view;
        view.webview.options = { enableScripts: true, enableCommandUris: [...this.options.commandUris], localResourceRoots: [] };
        view.webview.html = renderSidebar(this.state, this.options.folders(), randomBytes(24).toString('base64url'));
        this.listeners.push(
            view.webview.onDidReceiveMessage(value => {
                const message = sidebarMessage(value);
                if (message?.action === 'open') { this.options.open(); }
                else if (message?.action === 'search') { this.options.search(message.query); }
            }),
            view.onDidDispose(() => { if (this.view === view) { this.view = undefined; } }),
        );
        void this.options.attach(state => this.update(state)).then(listener => this.listeners.push(listener));
    }

    private update(state: ExplorerViewState): void {
        this.state = state;
        if (this.view) { void this.view.webview.postMessage({ type: 'render', html: sidebarContent(state, this.options.folders()) }); }
    }

    dispose(): void { for (const listener of this.listeners.splice(0)) { listener.dispose(); } this.view = undefined; }
}
