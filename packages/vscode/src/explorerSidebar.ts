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

export function renderSidebar(state: ExplorerViewState | undefined, folders: number, nonce: string, opening = false): string {
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
</head><body><main id="content">${opening ? '<p role="status">Opening AIRK Explorer…</p>' : sidebarContent(state, folders)}</main>
<script nonce="${nonce}">const vscode=acquireVsCodeApi();
document.addEventListener('click',event=>{const button=event.target.closest('button[data-action="open"]');if(button)vscode.postMessage({type:'sidebar',action:'open'});});
document.addEventListener('submit',event=>{event.preventDefault();const input=document.getElementById('query');if(input&&input.value.trim())vscode.postMessage({type:'sidebar',action:'search',query:input.value});});
window.addEventListener('message',event=>{const value=event.data;if(value&&value.type==='render'&&typeof value.html==='string'){const query=document.getElementById('query');const kept=query?query.value:'';document.getElementById('content').innerHTML=value.html;const next=document.getElementById('query');if(next)next.value=kept;}});</script>
</body></html>`;
}

/** Activity-bar entry point. The old sidebar is exposed only if Explorer cannot open. */
export class ExplorerSidebar implements vscode.WebviewViewProvider, vscode.Disposable {
    private view?: vscode.WebviewView;
    private state?: ExplorerViewState;
    private fallback = false;
    private opening?: Promise<void>;
    private openingView?: vscode.WebviewView;
    private failure?: string;
    private readonly listeners: vscode.Disposable[] = [];

    constructor(private readonly options: {
        commandUris: readonly string[];
        folders(): number;
        attach(listener: (state: ExplorerViewState) => void): Promise<vscode.Disposable>;
        open(): Promise<void>;
        hide(): Promise<void>;
        fallback?(unavailable: boolean): void;
        search(query: string): void;
    }) {}

    resolveWebviewView(view: vscode.WebviewView): void {
        this.view = view;
        view.webview.options = { enableScripts: true, enableCommandUris: [...this.options.commandUris], localResourceRoots: [] };
        this.fallback = false;
        view.webview.html = renderSidebar(this.state, this.options.folders(), randomBytes(24).toString('base64url'), true);
        // Listeners belong to this resolved view; VS Code resolves the view again after it is disposed.
        const owned: vscode.Disposable[] = [];
        const release = () => { for (const listener of owned.splice(0)) { listener.dispose(); this.listeners.splice(this.listeners.indexOf(listener) >>> 0, 1); } };
        const own = (listener: vscode.Disposable) => { owned.push(listener); this.listeners.push(listener); };
        own(view.webview.onDidReceiveMessage(value => {
            const message = sidebarMessage(value);
            if (message?.action === 'open') { void this.launch(view); }
            else if (message?.action === 'search') { this.options.search(message.query); }
        }));
        own(view.onDidDispose(() => { if (this.view === view) { this.view = undefined; } release(); }));
        own(view.onDidChangeVisibility(() => { if (view.visible) { void this.launch(view); } }));
        void this.options.attach(state => this.update(state)).then(listener => { if (this.view === view) { own(listener); } else { listener.dispose(); } }).catch(() => { /* Opening reports the failure; setup remains available without a snapshot. */ });
        if (view.visible) { void this.launch(view); }
    }

    private launch(view: vscode.WebviewView): Promise<void> {
        if (this.opening) {
            return this.openingView === view ? this.opening : this.opening.then(() => {
                if (this.view === view && view.visible) { return this.launch(view); }
            });
        }
        this.openingView = view;
        this.fallback = false;
        this.failure = undefined;
        this.options.fallback?.(false);
        void view.webview.postMessage({ type: 'render', html: '<p role="status">Opening AIRK Explorer…</p>' });
        this.opening = (async () => {
            try { await this.options.open(); }
            catch (error) {
                if (this.view === view) {
                    this.fallback = true;
                    this.options.fallback?.(true);
                    this.failure = error instanceof Error ? error.message : String(error);
                    this.renderFallback();
                }
                return;
            }
            // Never close a different sidebar if this view lost visibility while opening.
            if (this.view === view && view.visible) {
                void view.webview.postMessage({ type: 'render', html: '<p role="status">AIRK Explorer is open in the editor.</p><button data-action="open">Open AIRK Explorer</button>' });
                try { await this.options.hide(); } catch { /* Explorer is still available; do not replace it with the old panel. */ }
            }
        })().finally(() => { this.opening = undefined; this.openingView = undefined; });
        return this.opening;
    }

    private renderFallback(): void {
        if (this.view && this.fallback) {
            void this.view.webview.postMessage({ type: 'render', html: `<p class="note" role="alert">AIRK Explorer could not open. ${esc(this.failure ?? '')}</p><button data-action="open">Retry AIRK Explorer</button>${sidebarContent(this.state, this.options.folders())}` });
        }
    }

    private update(state: ExplorerViewState): void {
        this.state = state;
        this.renderFallback();
    }

    dispose(): void { for (const listener of this.listeners.splice(0)) { listener.dispose(); } this.view = undefined; }
}
