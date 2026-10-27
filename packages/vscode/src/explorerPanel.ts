// SPDX-License-Identifier: Apache-2.0
import { randomBytes } from 'node:crypto';
import * as vscode from 'vscode';
import { HostMessage, parseWebviewMessage, WebviewMessage } from './explorerContracts';
import { explorerError } from './explorerErrors';

export const EXPLORER_VIEW_TYPE = 'airk.explorer';

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
    generation = 0;

    constructor(private readonly extensionUri: vscode.Uri, private readonly handlers: {
        message(message: WebviewMessage): void;
        visible(visible: boolean): void;
        rejected?(value: unknown): void;
    }) {}

    get open(): boolean { return this.panel !== undefined; }

    reveal(): void {
        if (this.panel) { this.panel.reveal(undefined, false); return; }
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
            panel.onDidDispose(() => { this.panel = undefined; this.ready = false; this.queued = []; this.handlers.visible(false); this.disposeListeners(); }),
        );
        this.handlers.visible(true);
    }

    post(message: HostMessage): void {
        if (!this.panel) { return; }
        if (!this.ready) {
            // Keep only the latest state before the webview script has loaded.
            if (message.type === 'state') { this.queued = this.queued.filter(entry => entry.type !== 'state'); }
            if (this.queued.length < 64) { this.queued.push(message); }
            return;
        }
        void this.panel.webview.postMessage(message);
    }

    private disposeListeners(): void { for (const listener of this.listeners.splice(0)) { listener.dispose(); } }

    dispose(): void { this.disposeListeners(); this.panel?.dispose(); this.panel = undefined; }
}
