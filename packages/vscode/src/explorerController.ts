// SPDX-License-Identifier: Apache-2.0
import * as nodePath from 'node:path';
import * as vscode from 'vscode';
import { EngineHelpView, ExplorerViewState, HostMessage, MESSAGE_VERSION, StatusSummary, WebviewMessage, WebviewMessageOf } from './explorerContracts';
import { ControlSource, ExplorerControl } from './explorerControl';
import { explorerError, ExplorerError } from './explorerErrors';
import { EvidenceClient, EvidenceFailure } from './explorerEvidence';
import { FEATURES } from './explorerFeatures';
import { ExplorerPanel } from './explorerPanel';
import { formatRoute, parseRoute } from './explorerRoutes';
import { continueFacts, FactHandles, runFacts, scoped } from './explorerFacts';
import { changesSince, checkCitations, inspect } from './explorerInspect';
import { managementView } from './explorerManagement';
import { passageView, RefRegistry, searchArgs, searchView } from './explorerSearch';
import { headline, statusSummary } from './explorerStatus';
import { displayPath } from './explorerSource';
import { openSource } from './explorerWorkspaceIO';
import { LaunchSpec } from './model';

/** The narrow surface of the service extension that the Explorer may use. */
export interface ExplorerHost {
    readonly extensionUri: vscode.Uri;
    readonly version: string;
    trusted(): boolean;
    specs(): readonly LaunchSpec[];
    /** Registered project for the active editor's workspace folder, if any. */
    activeProject(): string | undefined;
    readonly onDidChangeSpecs: vscode.Event<void>;
    readonly control: ControlSource;
    log(line: string): void;
    setStatusBar(text: string, tooltip: string): void;
}

export interface ExplorerSnapshotListener { (state: ExplorerViewState): void }

/**
 * Owns the Explorer panel, its evidence session and the published view state. All
 * AIRK work goes through the existing control facade or the evidence session; the
 * webview only renders state and sends validated, typed requests.
 */
export class ExplorerController implements vscode.Disposable {
    private readonly control: ExplorerControl;
    private readonly panel: ExplorerPanel;
    private readonly listeners: vscode.Disposable[] = [];
    private readonly snapshotListeners = new Set<ExplorerSnapshotListener>();
    private readonly refs = new RefRegistry();
    private readonly facts = new FactHandles(this.refs);
    private evidence?: EvidenceClient;
    private selected?: string;
    private status?: StatusSummary;
    private statusFor?: string;
    private statusLoading?: Promise<void>;
    private notice?: ExplorerError;
    private revision = 0;
    private route = '#overview';
    private retained?: { dispose(): void };
    private managementOpen = false;
    private disposed = false;

    constructor(private readonly host: ExplorerHost) {
        this.control = new ExplorerControl(host.control, host.version);
        this.panel = new ExplorerPanel(host.extensionUri, {
            message: message => { void this.handle(message); },
            visible: visible => this.visibility(visible),
            rejected: () => host.log('AIRK Explorer ignored an unrecognized webview message.'),
        });
        this.listeners.push(this.control.onDidChange(() => this.observedControl()), host.onDidChangeSpecs(() => this.specsChanged()));
        this.selectDefault();
        this.publish();
    }

    /** Reveal the Explorer, optionally at a route from the manifest. */
    open(route?: string): void {
        if (route && parseRoute(route)) { this.route = formatRoute(parseRoute(route)!); }
        this.panel.reveal();
        this.panel.post({ v: MESSAGE_VERSION, type: 'navigate', route: this.route });
        this.publish();
        void this.loadStatus();
    }

    /** Open Search with a query typed outside the panel; the webview runs it like any search. */
    search(query: string): void {
        this.open('#search/query');
        if (query.trim() && query.length <= 4096) { this.panel.post({ v: MESSAGE_VERSION, type: 'search', query }); }
    }

    onSnapshot(listener: ExplorerSnapshotListener): vscode.Disposable {
        this.snapshotListeners.add(listener); listener(this.snapshot());
        return { dispose: () => { this.snapshotListeners.delete(listener); } };
    }

    snapshot(): ExplorerViewState {
        const projects = this.host.trusted() ? this.control.projects() : [];
        return {
            revision: this.revision,
            extensionVersion: this.host.version,
            trusted: this.host.trusted(),
            connection: this.control.connection(),
            projects,
            ...(this.control.observedAt() !== undefined ? { projectsObservedAt: this.control.observedAt() } : {}),
            ...(this.selected ? { selectedProject: this.selected } : {}),
            ...(this.status && this.statusFor === this.selected ? { status: this.status } : {}),
            features: FEATURES,
            ...(this.notice ? { notice: this.notice } : {}),
            ...(this.control.message() ? { controlMessage: this.control.message()!.slice(0, 600) } : {}),
            ...(this.control.morePages() ? { moreProjects: true } : {}),
            ...(this.managementOpen && this.host.trusted() ? { management: managementView(this.control.state) } : {}),
        };
    }

    private publish(): void {
        if (this.disposed) { return; }
        this.revision++;
        const state = this.snapshot();
        this.panel.post({ v: MESSAGE_VERSION, type: 'state', state });
        for (const listener of this.snapshotListeners) { listener(state); }
        const { word } = headline(state.connection, state.status, this.host.specs().length);
        const project = state.projects.find(entry => entry.project === state.selectedProject);
        this.host.setStatusBar(`AIRK: ${word}`, project ? `AIRK Explorer · ${project.collection} · ${word}` : `AIRK Explorer · ${word}`);
    }

    private visibility(visible: boolean): void {
        if (visible && !this.retained) { this.retained = this.control.retain(); void this.loadStatus(); }
        else if (!visible && this.retained) { this.retained.dispose(); this.retained = undefined; }
    }

    private selectDefault(): void {
        const specs = this.host.specs();
        if (this.selected && specs.some(spec => spec.service?.project === this.selected)) { return; }
        const active = this.host.activeProject();
        this.select(active && specs.some(spec => spec.service?.project === active) ? active : specs[0]?.service?.project);
    }

    private select(project: string | undefined): void {
        if (project === this.selected) { return; }
        this.selected = project;
        this.evidence?.dispose(); this.evidence = undefined;
        this.status = undefined; this.statusFor = undefined;
        this.refs.clear(); this.facts.clear();
    }

    private specsChanged(): void {
        const spec = this.selected ? this.host.specs().find(entry => entry.service?.project === this.selected) : undefined;
        // A changed launcher binding (runtime upgrade, storage change) invalidates the session.
        if (this.evidence && (!spec || spec.command !== this.evidence.spec.command || JSON.stringify(spec) !== JSON.stringify(this.evidence.spec))) {
            this.evidence.dispose(); this.evidence = undefined;
        }
        this.selectDefault();
        this.publish();
        if (this.panel.open) { void this.loadStatus(); }
    }

    private observedControl(): void {
        // Refresh published status when the cached console reports a different publication.
        const row = this.control.projects().find(entry => entry.project === this.selected);
        if (this.panel.open && row?.publication && this.status?.publication && row.publication !== this.status.publication) { void this.loadStatus(); }
        this.publish();
    }

    /** The active editor's file as a collection-relative path within the selected project's root. */
    private activeRelativePath(): string | undefined {
        const editor = vscode.window.activeTextEditor;
        const spec = this.host.specs().find(entry => entry.service?.project === this.selected);
        if (!editor || editor.document.uri.scheme !== 'file' || !spec) { return undefined; }
        const relative = nodePath.relative(displayPath(spec.root), editor.document.uri.fsPath);
        if (!relative || relative.startsWith('..') || nodePath.isAbsolute(relative)) { return undefined; }
        return relative.split(nodePath.sep).join('/');
    }

    private requireProject(): string {
        if (!this.selected) { throw new EvidenceFailure(explorerError('NO_PROJECT')); }
        return this.selected;
    }

    /** The evidence session for the selected project, created lazily. */
    protected session(): EvidenceClient {
        if (!this.host.trusted()) { throw new EvidenceFailure(explorerError('WORKSPACE_UNTRUSTED')); }
        const spec = this.host.specs().find(entry => entry.service?.project === this.selected);
        if (!spec) { throw new EvidenceFailure(explorerError('NO_PROJECT')); }
        if (!this.evidence) { this.evidence = new EvidenceClient(spec, this.host.version); }
        return this.evidence;
    }

    private loadStatus(): Promise<void> {
        if (this.statusLoading || this.disposed) { return this.statusLoading ?? Promise.resolve(); }
        const project = this.selected;
        if (!project || !this.host.trusted()) { return Promise.resolve(); }
        this.statusLoading = (async () => {
            try {
                const result = await this.session().call('airk_status', {}, { timeout: 90000, supersede: 'status' });
                if (project === this.selected) { this.status = statusSummary(result.payload); this.statusFor = project; this.notice = undefined; }
            } catch (error) {
                if (project === this.selected) { this.notice = failure(error); }
            } finally { this.statusLoading = undefined; this.publish(); }
        })();
        return this.statusLoading;
    }

    private respond(requestId: string, outcome: { data: unknown } | { error: ExplorerError }): void {
        const message: HostMessage = 'data' in outcome
            ? { v: MESSAGE_VERSION, type: 'result', requestId, ok: true, data: outcome.data }
            : { v: MESSAGE_VERSION, type: 'result', requestId, ok: false, error: outcome.error };
        this.panel.post(message);
    }

    private async handle(message: WebviewMessage): Promise<void> {
        if (this.disposed) { return; }
        try {
            const data = await this.dispatch(message);
            if (message.type !== 'ready') { this.respond(message.requestId, { data: data ?? null }); }
        } catch (error) {
            this.respond(message.requestId, { error: failure(error) });
        }
    }

    protected async dispatch(message: WebviewMessage): Promise<unknown> {
        switch (message.type) {
            case 'ready': this.publish(); this.panel.post({ v: MESSAGE_VERSION, type: 'navigate', route: this.route }); return;
            case 'nav.go': {
                const route = parseRoute(message.route);
                if (!route) { throw new EvidenceFailure(explorerError('INVALID_MESSAGE')); }
                this.route = formatRoute(route); return;
            }
            case 'project.select': {
                const typed = message as WebviewMessageOf<'project.select'>;
                if (!this.host.specs().some(spec => spec.service?.project === typed.project)) { throw new EvidenceFailure(explorerError('NO_PROJECT')); }
                this.select(typed.project); this.publish(); await this.loadStatus(); return;
            }
            case 'status.refresh': await Promise.all([this.loadStatus(), this.control.refresh()]); return;
            case 'manage.refresh': await this.control.refresh(); return;
            case 'manage.observe': {
                this.managementOpen = (message as WebviewMessageOf<'manage.observe'>).open;
                this.publish(); return;
            }
            case 'manage.action': {
                const { action, v: _v, type: _type, requestId: _id, ...rest } = message as WebviewMessageOf<'manage.action'>;
                if (!this.host.trusted()) { throw new EvidenceFailure(explorerError('WORKSPACE_UNTRUSTED')); }
                const fields = Object.fromEntries(Object.entries(rest).filter(([, value]) => value !== undefined));
                // The shared console validates the action, its fields and page membership again before acting.
                await this.control.run(action, fields);
                return { message: this.control.message() ?? null };
            }
            case 'manage.command': {
                if (!this.host.trusted()) { throw new EvidenceFailure(explorerError('WORKSPACE_UNTRUSTED')); }
                await vscode.commands.executeCommand((message as WebviewMessageOf<'manage.command'>).command);
                return;
            }
            case 'search.run': {
                const typed = message as WebviewMessageOf<'search.run'>;
                const project = this.requireProject();
                const result = await this.session().call('airk_search', searchArgs(typed), { supersede: 'search', timeout: 30000 });
                const status = this.statusFor === project ? this.status : undefined;
                return searchView(project, typed, result.payload, result.publication, this.refs,
                    { ...(status?.semantic ? { semantic: status.semantic } : {}), ...(status?.vectorsEnabled !== undefined ? { vectorsEnabled: status.vectorsEnabled } : {}) });
            }
            case 'evidence.expand': {
                const typed = message as WebviewMessageOf<'evidence.expand'>;
                const project = this.requireProject();
                const ref = this.refs.get(typed.ref, project);
                const result = await this.session().call('airk_read_file', { path: ref.path, start_line: Math.max(1, ref.startLine - typed.before),
                    end_line: ref.endLine + typed.after, max_bytes: 32768 }, { timeout: 30000 });
                return passageView(project, result.payload, result.publication, this.refs);
            }
            case 'source.open': {
                const typed = message as WebviewMessageOf<'source.open'>;
                const project = this.requireProject();
                const ref = this.refs.get(typed.ref, project);
                // The location on disk comes from AIRK's own file record, never from the webview.
                const info = (await this.session().call('airk_file_info', { path: ref.path }, { timeout: 30000 })).payload;
                const source = info.source as Record<string, unknown> | undefined;
                if (!source || typeof source.root !== 'string' || typeof source.relative_path !== 'string') {
                    throw new EvidenceFailure(explorerError('TOOL_ERROR', 'AIRK did not report a source location for this file.'));
                }
                return openSource({ root: source.root, relative: source.relative_path, contentHash: ref.contentHash, startLine: ref.startLine,
                    ...(typeof info.line_basis === 'string' ? { lineBasis: info.line_basis } : {}) });
            }
            case 'code.symbols': {
                const typed = message as WebviewMessageOf<'code.symbols'>, project = this.requireProject();
                return runFacts(this.session(), this.facts, project, { tool: 'airk_symbols', limit: typed.limit,
                    args: scoped({ limit: typed.limit, max_bytes: 16384, ...(typed.name?.trim() ? { name: typed.name.trim() } : {}) }, typed.pathPrefix) });
            }
            case 'code.facts': {
                const typed = message as WebviewMessageOf<'code.facts'>, project = this.requireProject();
                return runFacts(this.session(), this.facts, project, { tool: 'airk_code_facts', limit: typed.limit,
                    args: scoped({ name: typed.name.trim(), limit: typed.limit, max_bytes: 32768 }, typed.pathPrefix) });
            }
            case 'code.references': {
                const typed = message as WebviewMessageOf<'code.references'>, project = this.requireProject();
                return runFacts(this.session(), this.facts, project, { tool: 'airk_references', limit: typed.limit,
                    args: scoped({ name: typed.name.trim(), limit: typed.limit, max_bytes: 16384 }, typed.pathPrefix) });
            }
            case 'code.relationships': {
                const typed = message as WebviewMessageOf<'code.relationships'>, project = this.requireProject();
                const symbol = this.facts.symbols.get(typed.symbol, project);
                return runFacts(this.session(), this.facts, project, { tool: 'airk_relationships', limit: typed.limit,
                    args: { symbol: symbol.symbolId, direction: typed.direction, hops: typed.hops, limit: typed.limit, max_bytes: 16384 } });
            }
            case 'code.outline': {
                const typed = message as WebviewMessageOf<'code.outline'>, project = this.requireProject();
                const path = typed.ref ? this.refs.get(typed.ref, project).path : this.activeRelativePath();
                if (!path) { throw new EvidenceFailure(explorerError('INVALID_MESSAGE', 'Open a file from the selected project, or choose a result, to see its outline.')); }
                return runFacts(this.session(), this.facts, project, { tool: 'airk_outline', limit: 100, args: { path, limit: 100, max_bytes: 32768 } });
            }
            case 'docs.facts': {
                const typed = message as WebviewMessageOf<'docs.facts'>, project = this.requireProject();
                return runFacts(this.session(), this.facts, project, { tool: 'airk_document_facts', limit: typed.limit,
                    args: scoped({ limit: typed.limit, max_bytes: 32768, ...(typed.kind !== 'any' ? { kind: typed.kind } : {}),
                        ...(typed.name?.trim() ? { name: typed.name.trim() } : {}) }, typed.pathPrefix) });
            }
            case 'evidence.inspect': {
                const typed = message as WebviewMessageOf<'evidence.inspect'>, project = this.requireProject();
                return inspect(this.session(), this.refs, project, typed.ref, dirtyFile);
            }
            case 'evidence.check': {
                const typed = message as WebviewMessageOf<'evidence.check'>, project = this.requireProject();
                return checkCitations(this.session(), this.refs, project, typed.text, dirtyFile);
            }
            case 'evidence.changes': {
                const typed = message as WebviewMessageOf<'evidence.changes'>;
                this.requireProject();
                return changesSince(this.session(), typed.since, typed.limit);
            }
            case 'help.engine': {
                const typed = message as WebviewMessageOf<'help.engine'>;
                this.requireProject();
                const result = await this.session().call('airk_help', { ...(typed.topic ? { topic: typed.topic } : {}), max_bytes: 16384 }, { timeout: 30000 });
                return engineHelp(result.payload);
            }
            case 'list.more': {
                const typed = message as WebviewMessageOf<'list.more'>;
                return continueFacts(this.session(), this.facts, this.requireProject(), typed.more);
            }
            default: throw new EvidenceFailure(explorerError('CAPABILITY_DISABLED', `${(message as WebviewMessage).type} is not available in this build.`));
        }
    }

    dispose(): void {
        if (this.disposed) { return; }
        this.disposed = true;
        this.retained?.dispose();
        this.evidence?.dispose();
        this.panel.dispose();
        for (const listener of this.listeners.splice(0)) { listener.dispose(); }
        this.snapshotListeners.clear();
    }
}

/** airk_help reply → Explorer view; the guidance text is shown verbatim as text. */
function engineHelp(payload: Record<string, unknown>): EngineHelpView {
    const topics = (Array.isArray(payload.topics) ? payload.topics : []).flatMap(raw => {
        const entry = raw && typeof raw === 'object' ? raw as Record<string, unknown> : {};
        return typeof entry.id === 'string' ? [{ id: entry.id.slice(0, 64), title: typeof entry.title === 'string' ? entry.title.slice(0, 160) : entry.id.slice(0, 64) }] : [];
    });
    return { topic: typeof payload.topic === 'string' ? payload.topic : 'overview', title: typeof payload.title === 'string' ? payload.title.slice(0, 200) : 'AIRK help',
        content: typeof payload.content === 'string' ? payload.content.slice(0, 32768) : '', ...(typeof payload.version === 'string' ? { version: payload.version } : {}),
        topics, truncated: payload.truncated === true || payload.next_line != null };
}

/** True when an open editor holds unsaved edits for this file. */
function dirtyFile(file: string): boolean {
    const fold = (value: string) => process.platform === 'win32' ? value.toLowerCase() : value;
    return vscode.workspace.textDocuments.some(document => document.uri.scheme === 'file' && fold(document.uri.fsPath) === fold(file) && document.isDirty);
}

export function failure(error: unknown): ExplorerError {
    if (error instanceof EvidenceFailure) { return error.error; }
    const code = error && typeof error === 'object' && typeof (error as { code?: unknown }).code === 'string' ? (error as { code: string }).code : 'TOOL_ERROR';
    return explorerError(code, error instanceof Error ? error.message : undefined);
}
