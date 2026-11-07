// SPDX-License-Identifier: Apache-2.0
import { randomBytes } from 'node:crypto';
import * as vscode from 'vscode';
import { HostMessage, parseWebviewMessage, WebviewMessage } from './explorerContracts';
import { explorerError } from './explorerErrors';

export const EXPLORER_VIEW_TYPE = 'airk.explorer';
// A stable bootstrap can replace the CommonJS graph without replacing this
// window. Keep ownership outside that graph so its successor cannot duplicate it.
const OWNER_KEY = Symbol.for('iokaio.airk.explorerPanelOwner');
const owners = globalThis as unknown as Record<symbol, ExplorerPanel | undefined>;
function explorerTab(tab: vscode.Tab): boolean {
    const input = tab.input as { viewType?: unknown } | undefined;
    return input?.viewType === EXPLORER_VIEW_TYPE || input?.viewType === `mainThreadWebview-${EXPLORER_VIEW_TYPE}`;
}

/** Static shell: the bundled webview renders everything; no inline data or remote resources. */
export function explorerHtml(webview: { cspSource: string; asWebviewUri(uri: vscode.Uri): vscode.Uri }, extensionUri: vscode.Uri, nonce: string): string {
    const script = webview.asWebviewUri(vscode.Uri.joinPath(extensionUri, 'out', 'explorerWebview.js'));
    const style = webview.asWebviewUri(vscode.Uri.joinPath(extensionUri, 'out', 'explorerWebview.css'));
    return `<!DOCTYPE html><html lang="en"><head><meta charset="UTF-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src ${webview.cspSource} data:; style-src ${webview.cspSource}; script-src 'nonce-${nonce}'; font-src ${webview.cspSource};">
<meta name="viewport" content="width=device-width, initial-scale=1.0"><title>AIRK Explorer</title>
<link rel="stylesheet" href="${style.toString()}"></head>
<body><div id="root" aria-live="polite"></div><script nonce="${nonce}" src="${script.toString()}"></script></body></html>`;
}

/**
 * One AIRK Explorer editor panel per window. Every inbound message is validated by
 * the shared contract before the controller sees it; unknown messages are dropped.
 */
export class ExplorerPanel implements vscode.Disposable {
    private panel?: vscode.WebviewPanel;
    private readonly listeners: vscode.Disposable[] = [];
    private ready = false;
    private queued: HostMessage[] = [];
    private opening = false;
    private disposed = false;
    generation = 0;

    constructor(private readonly extensionUri: vscode.Uri, private readonly handlers: {
        message(message: WebviewMessage): void;
        visible(visible: boolean): void;
        rejected?(value: unknown): void;
    }) {}

    get open(): boolean { return this.panel !== undefined || this.opening; }

    reveal(): void {
        if (this.disposed) { return; }
        if (this.panel) { this.panel.reveal(undefined, false); return; }
        if (this.opening) { return; }
        const previous = owners[OWNER_KEY];
        if (previous !== this) { previous?.dispose(); }
        owners[OWNER_KEY] = this;
        // A crashed/replaced extension host can leave a restored editor tab
        // without a live ExplorerPanel. Close only AIRK Explorer tabs before
        // creating the new owner; all other editor tabs remain untouched.
        const groups = vscode.window.tabGroups;
        const orphaned = groups?.all.flatMap(group => group.tabs).filter(explorerTab) ?? [];
        if (orphaned.length && groups.close) {
            this.opening = true;
            void Promise.resolve(groups.close(orphaned, true)).then(closed => {
                if (this.disposed || owners[OWNER_KEY] !== this) { return; }
                this.opening = false;
                if (!closed) { throw new Error('VS Code did not close the previous AIRK Explorer tab.'); }
                this.createPanel();
            }).catch(error => {
                if (this.disposed || owners[OWNER_KEY] !== this) { return; }
                this.opening = false; this.queued = [];
                owners[OWNER_KEY] = undefined;
                void vscode.window.showErrorMessage?.(`AIRK Explorer could not reopen: ${String(error)}`);
            });
            return;
        }
        this.createPanel();
    }

    private createPanel(): void {
        if (this.disposed || owners[OWNER_KEY] !== this) { return; }
        const panel = vscode.window.createWebviewPanel(EXPLORER_VIEW_TYPE, 'AIRK Explorer', { viewColumn: vscode.ViewColumn.Active, preserveFocus: false }, {
            enableScripts: true, enableCommandUris: false, retainContextWhenHidden: true,
            localResourceRoots: [vscode.Uri.joinPath(this.extensionUri, 'out')],
        });
        this.panel = panel; this.ready = false;
        this.generation++;
        panel.iconPath = vscode.Uri.joinPath(this.extensionUri, 'images', 'airk.svg');
        panel.webview.html = explorerHtml(panel.webview, this.extensionUri, randomBytes(24).toString('base64url'));
        this.listeners.push(
            panel.webview.onDidReceiveMessage(value => {
                const message = parseWebviewMessage(value);
                if (!message) {
                    this.handlers.rejected?.(value);
                    if (value?.v === 1 && typeof value.requestId === 'string' && /^[A-Za-z0-9_-]{1,64}$/.test(value.requestId)) {
                        void panel.webview.postMessage({ v: 1, type: 'result', requestId: value.requestId, ok: false, error: explorerError('INVALID_MESSAGE', 'Fields are invalid or exceed their UTF-8 byte budget.') });
                    }
                    return;
                }
                if (message.type === 'ready') { this.ready = true; for (const queued of this.queued.splice(0)) { void panel.webview.postMessage(queued); } }
                this.handlers.message(message);
            }),
            panel.onDidChangeViewState(event => this.handlers.visible(event.webviewPanel.visible)),
            panel.onDidDispose(() => this.closed(panel)),
        );
        // Tab closure is authoritative even when VS Code retains the webview
        // handle and delays onDidDispose. Match the exact Tab, so a hidden editor
        // or a delayed close from the previous owner cannot close this panel.
        // createWebviewPanel sends a main-thread RPC; tab events cannot interleave
        // this synchronous setup before these listeners have been registered.
        const groups = vscode.window.tabGroups;
        let ownedTab = groups?.all.flatMap(group => group.tabs).find(explorerTab);
        const tabChanges = groups?.onDidChangeTabs?.(event => {
            if (this.panel !== panel) { return; }
            ownedTab ??= event.opened.find(explorerTab);
            if (ownedTab && event.closed.includes(ownedTab)) {
                this.closed(panel); panel.dispose();
            }
        });
        if (tabChanges) { this.listeners.push(tabChanges); }
        const groupChanges = groups?.onDidChangeTabGroups?.(event => {
            if (this.panel !== panel || !ownedTab) { return; }
            if (event.closed.includes(ownedTab.group) && !groups.all.some(group => group.tabs.includes(ownedTab!))) {
                this.closed(panel); panel.dispose();
            }
        });
        if (groupChanges) { this.listeners.push(groupChanges); }
        this.handlers.visible(true);
    }

    post(message: HostMessage): void {
        if (!this.panel && !this.opening) { return; }
        if (!this.ready) {
            // Keep only the latest state before the webview script has loaded.
            if (message.type === 'state') { this.queued = this.queued.filter(entry => entry.type !== 'state'); }
            if (this.queued.length < 64) { this.queued.push(message); }
            return;
        }
        void this.panel?.webview.postMessage(message);
    }

    private disposeListeners(): void { for (const listener of this.listeners.splice(0)) { listener.dispose(); } }

    private closed(panel: vscode.WebviewPanel): void {
        if (this.panel !== panel) { return; }
        this.panel = undefined; this.ready = false; this.queued = [];
        if (owners[OWNER_KEY] === this) { owners[OWNER_KEY] = undefined; }
        this.disposeListeners(); this.handlers.visible(false);
    }

    dispose(): void {
        if (this.disposed) { return; }
        this.disposed = true;
        const panel = this.panel, wasOpen = this.open;
        this.panel = undefined; this.opening = false; this.ready = false; this.queued = [];
        if (owners[OWNER_KEY] === this) { owners[OWNER_KEY] = undefined; }
        this.disposeListeners(); panel?.dispose();
        if (wasOpen) { this.handlers.visible(false); }
    }
}
