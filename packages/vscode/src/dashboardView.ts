// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import { SidebarState, renderSidebar, sidebarContent } from './sidebar';

export interface DashboardHost {
    readonly output: Pick<vscode.OutputChannel, 'appendLine'>;
    sidebarState(): SidebarState;
    sidebarMessage(message: unknown): Promise<void>;
}

export class DashboardView implements vscode.WebviewViewProvider, vscode.Disposable {
    private view?: vscode.WebviewView;
    private timer?: NodeJS.Timeout;
    private last = '';
    private listeners: vscode.Disposable[] = [];
    constructor(private readonly controller: DashboardHost) {}
    get visible(): boolean { return this.view?.visible ?? false; }
    resolveWebviewView(view: vscode.WebviewView): void {
        this.listeners.forEach(listener => listener.dispose());
        this.view = view; this.last = '';
        view.webview.options = { enableScripts: true, localResourceRoots: [] };
        view.webview.html = renderSidebar(this.controller.sidebarState());
        this.listeners = [view.webview.onDidReceiveMessage(message =>
            this.controller.sidebarMessage(message).catch(error => {
                this.controller.output.appendLine(String(error));
                void vscode.window.showErrorMessage(`AIRK: ${error instanceof Error ? error.message : String(error)}`);
            })), view.onDidChangeVisibility(() => { if (view.visible) { this.refresh(); } }),
        view.onDidDispose(() => { if (this.view === view) { this.view = undefined; } })];
    }
    refresh(): void {
        if (!this.visible || this.timer) { return; }
        this.timer = setTimeout(() => {
            this.timer = undefined;
            if (!this.visible) { return; }
            const html = sidebarContent(this.controller.sidebarState());
            if (html !== this.last) { this.last = html; void this.view!.webview.postMessage({ type: 'render', html }); }
        }, 100);
    }
    dispose(): void { clearTimeout(this.timer); this.listeners.forEach(listener => listener.dispose()); this.view = undefined; }
}
