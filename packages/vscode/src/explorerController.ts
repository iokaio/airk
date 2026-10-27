// SPDX-License-Identifier: Apache-2.0
import * as nodePath from 'node:path';
import * as vscode from 'vscode';
import { ABOUT_LINKS, EngineHelpView, ExplorerViewState, HostMessage, MESSAGE_VERSION, StatusSummary, WebviewMessage, WebviewMessageOf } from './explorerContracts';
import { ControlSource, ExplorerControl } from './explorerControl';
import { explorerError, ExplorerError } from './explorerErrors';
import { EvidenceClient, EvidenceFailure } from './explorerEvidence';
import { FEATURES } from './explorerFeatures';
import { ExplorerPanel } from './explorerPanel';
import { formatRoute, parseRoute, WORKSPACES } from './explorerRoutes';
import { continueFacts, FactHandles, runFacts, scoped } from './explorerFacts';
import { changesSince, checkCitations, inspect } from './explorerInspect';
import { managementView } from './explorerManagement';
import { passageView, RefRegistry, searchArgs, searchView } from './explorerSearch';
import { headline, statusSummary } from './explorerStatus';
import { displayPath } from './explorerSource';
import { exportStrategy, importPlan, importRecipe, importStrategy, listWorkspaceFiles, loadWorkspaceFile, liveAt, openPlanDraft, openSource, savePlanInWorkspace } from './explorerWorkspaceIO';
import { renderPlan, SearchRecipe, validatePlan, validateRecipe, validateStrategy } from './explorerStrategies';
import { runStrategy } from './explorerStrategyRunner';
import { SuggestionStore, suggestionsView } from './explorerSuggestions';
import { ReviewContext, reviewChanges, reviewImpact, reviewTests } from './explorerReview';
import { LaunchSpec } from './model';
import { HandoffStore } from './explorerHandoffs';
import type { AssistanceWindowStatus } from './explorerContracts';

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
    /** Per-workspace storage for search recipes. */
    readonly workspaceState: Pick<vscode.Memento, 'get' | 'update'>;
    /** This window's agent drafts (Suggestions Inbox). */
    readonly suggestions: SuggestionStore;
    readonly handoffs?: HandoffStore;
}

const RECIPES_KEY = 'airk.explorer.searchRecipes';
const RECIPE_LIMIT = 50;

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
    private readonly sourceFiles = new Map<string, string>();
    private selectionEpoch = 0;
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
        this.listeners.push(this.control.onDidChange(() => this.observedControl()), host.onDidChangeSpecs(() => this.specsChanged()), host.suggestions.onDidChange(() => this.publish()));
        this.selectDefault();
        this.rememberSource(vscode.window.activeTextEditor);
        const editorChanges = vscode.window.onDidChangeActiveTextEditor?.(editor => { if (this.rememberSource(editor)) { this.publish(); } });
        if (editorChanges) { this.listeners.push(editorChanges); }
        this.publish();
    }

    /** Reveal the Explorer, optionally at a route from the manifest. */
    open(route?: string): void {
        this.rememberSource(vscode.window.activeTextEditor);
        if (route && parseRoute(route)) { this.route = formatRoute(parseRoute(route)!); }
        this.panel.reveal();
        this.panel.post({ v: MESSAGE_VERSION, type: 'navigate', route: this.route });
        this.publish();
        void this.loadStatus();
    }

    /** Studio and Planner draft revisions last reported by the webview (for agent draft conflicts). */
    draftRevisions: { studio?: number; planner?: number } = {};
    /** Publication of the selected project's last status, if observed. */
    get publication(): string | undefined { return this.status?.publication; }
    get assistanceStatus(): Pick<AssistanceWindowStatus, 'active_route' | 'binding_generation' | 'blockers'> {
        return { active_route: this.route.replace(/^#/, ''), binding_generation: this.selectionEpoch,
            blockers: [...(!this.host.trusted() ? ['WORKSPACE_UNTRUSTED'] : []), ...(!this.selected ? ['NO_PROJECT'] : []),
                ...(!this.panelOpen ? ['EXPLORER_CLOSED'] : []), ...(this.notice ? [this.notice.code] : [])] };
    }

    /** Open the Explorer with the Suggestions Inbox showing. */
    showInbox(): void {
        this.panel.reveal();
        this.panel.post({ v: MESSAGE_VERSION, type: 'inbox' });
    }

    /** Whether the Explorer panel is open in this window (reported to Explorer Assistance). */
    get panelOpen(): boolean { return this.panel.open; }

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
            ...(this.selected && this.host.trusted() && this.sourceFiles.get(this.selected) ? { sourceFile: this.sourceFiles.get(this.selected) } : {}),
            selectionEpoch: this.selectionEpoch,
            ...(this.status && this.statusFor === this.selected ? { status: this.status } : {}),
            features: FEATURES,
            ...(this.notice ? { notice: this.notice } : {}),
            ...(this.control.message() ? { controlMessage: this.control.message()!.slice(0, 600) } : {}),
            ...(this.control.morePages() ? { moreProjects: true } : {}),
            ...(this.managementOpen && this.host.trusted() ? { management: managementView(this.control.state) } : {}),
            ...(this.host.trusted() ? { suggestions: suggestionsView(this.host.suggestions.list()) } : {}),
        };
    }

    private publish(): void {
        if (this.disposed) { return; }
        this.revision++;
        const state = this.snapshot();
        this.panel.post({ v: MESSAGE_VERSION, type: 'state', state });
        for (const listener of this.snapshotListeners) { listener(state); }
        const { word } = headline(state.connection, state.status, state.projects.length);
        const project = state.projects.find(entry => entry.project === state.selectedProject);
        this.host.setStatusBar(`AIRK: ${word}`, project ? `AIRK Explorer · ${project.name} · ${word}` : `AIRK Explorer · ${word}`);
    }

    private visibility(visible: boolean): void {
        if (!visible && !this.panel.open) { this.managementOpen = false; }
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
        this.selectionEpoch++;
        this.evidence?.dispose(); this.evidence = undefined;
        this.status = undefined; this.statusFor = undefined; this.notice = undefined;
        this.refs.clear(); this.facts.clear();
    }

    private specsChanged(): void {
        const spec = this.selected ? this.host.specs().find(entry => entry.service?.project === this.selected) : undefined;
        // A changed launcher binding (runtime upgrade, storage change) invalidates the session.
        if (this.evidence && (!spec || spec.command !== this.evidence.spec.command || JSON.stringify(spec) !== JSON.stringify(this.evidence.spec))) {
            this.evidence.dispose(); this.evidence = undefined;
            this.selectionEpoch++; this.status = undefined; this.statusFor = undefined;
            this.refs.clear(); this.facts.clear();
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

    /** Observe source editors, ignoring webview focus loss and foreign projects. */
    private rememberSource(editor: vscode.TextEditor | undefined): boolean {
        if (!editor || !['file', 'vscode-remote'].includes(editor.document.uri.scheme) || !this.host.trusted()) { return false; }
        let changed = false;
        for (const spec of this.host.specs()) {
            if (!spec.service) { continue; }
            const relative = this.relativeSource(spec, editor.document.uri.fsPath);
            if (relative && this.sourceFiles.get(spec.service.project) !== relative) {
                this.sourceFiles.set(spec.service.project, relative); changed = true;
            }
        }
        while (this.sourceFiles.size > 64) { this.sourceFiles.delete(this.sourceFiles.keys().next().value!); }
        return changed;
    }

    private relativeSource(spec: LaunchSpec, file: string): string | undefined {
        const relative = nodePath.relative(displayPath(spec.root), displayPath(file));
        if (!relative || relative === '..' || relative.startsWith(`..${nodePath.sep}`) || nodePath.isAbsolute(relative)) { return undefined; }
        return relative.split(nodePath.sep).join('/');
    }

    /** Retain a per-project target while the Explorer occupies the active editor group. */
    private activeRelativePath(): string | undefined {
        return this.selected ? this.sourceFiles.get(this.selected) : undefined;
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
        const epoch = this.selectionEpoch;
        if (!project || !this.host.trusted()) { return Promise.resolve(); }
        this.statusLoading = (async () => {
            try {
                const result = await this.session().call('airk_status', {}, { timeout: 90000, supersede: 'status' });
                if (epoch === this.selectionEpoch) { this.status = statusSummary(result.payload); this.statusFor = project; this.notice = undefined; }
            } catch (error) {
                // A transient missing registration is not a user-facing failure; the next refresh retries.
                if (epoch === this.selectionEpoch && failure(error).code !== 'NO_PROJECT') { this.notice = failure(error); }
            } finally {
                this.statusLoading = undefined; this.publish();
                if (!this.disposed && epoch !== this.selectionEpoch) { void this.loadStatus(); }
            }
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
        const epoch = this.selectionEpoch;
        try {
            const data = await this.dispatch(message);
            if (epoch !== this.selectionEpoch && !['project.select', 'status.refresh', 'source.choose'].includes(message.type)) { throw new EvidenceFailure(explorerError('REQUEST_SUPERSEDED')); }
            if (message.type !== 'ready') { this.respond(message.requestId, { data: data ?? null }); }
        } catch (error) {
            this.respond(message.requestId, { error: failure(error) });
        }
    }

    protected async dispatch(message: WebviewMessage): Promise<unknown> {
        switch (message.type) {
            case 'help.choose': {
                const route = parseRoute(message.route)!;
                const workspace = WORKSPACES.find(entry => entry.id === route.workspace)!;
                const tab = workspace.tabs.find(entry => entry.id === route.tab);
                const screen = `${workspace.title}${tab ? ` › ${tab.title}` : ''}`;
                const choice = await vscode.window.showQuickPick([
                    { label: `Help for ${screen}`, description: 'Guidance for the screen you are on', destination: 'screen' as const },
                    { label: 'Browse all help', description: 'Search all Explorer and AIRK engine guidance', destination: 'general' as const },
                ], { title: 'AIRK Help', placeHolder: 'Choose screen guidance or general help' });
                return choice?.destination ?? null;
            }
            case 'source.choose': {
                if (!this.host.trusted()) { throw new EvidenceFailure(explorerError('WORKSPACE_UNTRUSTED')); }
                const project = this.requireProject(), epoch = this.selectionEpoch;
                const spec = this.host.specs().find(entry => entry.service?.project === project);
                if (!spec) { throw new EvidenceFailure(explorerError('NO_PROJECT')); }
                const tabs = vscode.window.tabGroups?.all.flatMap(group => group.tabs.flatMap(tab => {
                    const input = tab.input as { uri?: vscode.Uri; modified?: vscode.Uri } | undefined;
                    return input?.uri ?? input?.modified ?? [];
                })) ?? vscode.workspace.textDocuments.map(document => document.uri);
                const openFiles = [...new Map(tabs.filter(uri => ['file', 'vscode-remote'].includes(uri.scheme)).map(uri => [`${uri.scheme}:${uri.fsPath}`, uri])).values()];
                const items = openFiles.map(uri => {
                    const owner = this.host.specs().find(entry => entry.service && this.relativeSource(entry, uri.fsPath));
                    return { label: nodePath.basename(uri.fsPath), description: uri.fsPath,
                        detail: owner ? `Project: ${owner.name ?? owner.collection}${owner.service?.project !== project ? ' · switches the selected project' : ''}` : 'Outside the open AIRK projects; choose its project before examining indexed evidence', uri, owner };
                });
                const picked = await vscode.window.showQuickPick([...items, { label: 'Choose another file…', description: 'Browse files in the selected project', uri: undefined, owner: spec }],
                    { title: 'AIRK source file', placeHolder: 'Choose an open editor file, or browse for another file', matchOnDescription: true });
                if (!picked) { return; }
                const [file] = picked.uri ? [picked.uri] : await vscode.window.showOpenDialog({ title: 'Choose source file for AIRK Explorer', canSelectMany: false, canSelectFiles: true, canSelectFolders: false, defaultUri: vscode.Uri.file(displayPath(spec.root)) }) ?? [];
                if (!file) { return; }
                if (this.disposed || !this.host.trusted() || epoch !== this.selectionEpoch) { throw new EvidenceFailure(explorerError('STALE_BINDING')); }
                const owner = picked.uri ? picked.owner : spec;
                const relative = owner && ['file', 'vscode-remote'].includes(file.scheme) ? this.relativeSource(owner, file.fsPath) : undefined;
                if (!relative) { throw new EvidenceFailure(explorerError('NO_SOURCE_FILE', 'Choose a file inside the selected project.')); }
                if (owner?.service?.project !== project) { this.select(owner!.service!.project); }
                this.sourceFiles.set(owner!.service!.project, relative); this.publish();
                return;
            }
            case 'help.openExternal': {
                const key = (message as WebviewMessageOf<'help.openExternal'>).destination as keyof typeof ABOUT_LINKS;
                return { opened: await vscode.env.openExternal(vscode.Uri.parse(ABOUT_LINKS[key].url)) };
            }
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
                if (!ref.publication) { throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH')); }
                const info = await this.session().call('airk_file_info', { path: ref.path, revision: ref.publication, known_hash: ref.contentHash });
                if (info.publication !== ref.publication) { throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH')); }
                // Extracted coordinates have no host-known page boundary: keep one cited line.
                const sourceLines = info.payload.line_basis === 'source';
                const result = await this.session().call('airk_context', { path: ref.path, line: ref.startLine,
                    before: sourceLines ? typed.before : 0, after: sourceLines ? typed.after : 0, revision: ref.publication, known_hash: ref.contentHash, max_bytes: 32768 }, { timeout: 30000 });
                if (result.publication !== ref.publication) { throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH')); }
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
                if (!path) { throw new EvidenceFailure(explorerError('NO_SOURCE_FILE')); }
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
                const path = typed.ref ? undefined : this.activeRelativePath();
                if (!typed.ref && !path) { throw new EvidenceFailure(explorerError('NO_SOURCE_FILE')); }
                return inspect(this.session(), this.refs, project, typed.ref, dirtyFile, path);
            }
            case 'evidence.check': {
                const typed = message as WebviewMessageOf<'evidence.check'>, project = this.requireProject();
                return checkCitations(this.session(), this.refs, project, typed.text, dirtyFile);
            }
            case 'evidence.changes': {
                const typed = message as WebviewMessageOf<'evidence.changes'>;
                this.requireProject();
                return changesSince(this.session(), typed.since, typed.limit, this.facts, this.requireProject());
            }
            case 'evidence.changesMore': return changesSince(this.session(), '', 50, this.facts, this.requireProject(), message.more);
            case 'help.engine': {
                const typed = message as WebviewMessageOf<'help.engine'>;
                this.requireProject();
                if ([typed.topic, typed.command, typed.tool].filter(Boolean).length > 1) { throw new EvidenceFailure(explorerError('INVALID_MESSAGE', 'Choose one help selector.')); }
                const result = await this.session().call('airk_help', { ...(typed.topic ? { topic: typed.topic } : {}), ...(typed.command ? { command: typed.command } : {}),
                    ...(typed.tool ? { tool: typed.tool } : {}), ...(typed.startLine ? { start_line: typed.startLine } : {}), max_bytes: 16384 }, { timeout: 30000 });
                return engineHelp(result.payload);
            }
            case 'list.more': {
                const typed = message as WebviewMessageOf<'list.more'>;
                return continueFacts(this.session(), this.facts, this.requireProject(), typed.more);
            }
            case 'strategy.run': {
                const typed = message as WebviewMessageOf<'strategy.run'>, project = this.requireProject();
                const checked = validateStrategy(typed.strategy);
                if (!checked.ok) { throw new EvidenceFailure(explorerError('INVALID_SCHEMA', checked.errors.slice(0, 5).join('; '))); }
                return runStrategy({ strategy: checked.strategy, params: textRecord(typed.params, 8), scope: textRecord(typed.scope, 1) },
                    { session: this.session(), facts: this.facts, project, extensionVersion: this.host.version, live: liveAt });
            }
            case 'strategy.export': {
                const checked = validateStrategy((message as WebviewMessageOf<'strategy.export'>).strategy);
                if (!checked.ok) { throw new EvidenceFailure(explorerError('INVALID_SCHEMA', checked.errors.slice(0, 5).join('; '))); }
                return exportStrategy(checked.strategy);
            }
            case 'strategy.import': return (await importStrategy()) ?? null;
            case 'clipboard.write': await vscode.env.clipboard.writeText((message as WebviewMessageOf<'clipboard.write'>).text); return;
            case 'suggestions.get': {
                const item = this.host.trusted() ? this.host.suggestions.get((message as WebviewMessageOf<'suggestions.get'>).id) : undefined;
                if (!item) { throw new EvidenceFailure(explorerError('PROPOSAL_EXPIRED', 'This suggestion is no longer available.')); }
                return { id: item.id, title: item.draft.title, status: item.status, client: item.client.name, warnings: item.warnings,
                    ...(item.targetRevision !== undefined ? { targetRevision: item.targetRevision } : {}), ...(item.draft.evidence ? { evidence: item.draft.evidence } : {}), body: item.draft.body };
            }
            case 'suggestions.resolve': {
                const typed = message as WebviewMessageOf<'suggestions.resolve'>;
                if (!this.host.trusted()) { throw new EvidenceFailure(explorerError('WORKSPACE_UNTRUSTED')); }
                const item = this.host.suggestions.get(typed.id);
                const current = item?.kind === 'strategy' ? this.draftRevisions.studio : item?.kind === 'plan' ? this.draftRevisions.planner : undefined;
                if (typed.status === 'applied' && current !== undefined && typed.targetRevision !== current) { throw new EvidenceFailure(explorerError('PROPOSAL_CONFLICT')); }
                if (!this.host.suggestions.resolve(typed.id, typed.status)) { throw new EvidenceFailure(explorerError('PROPOSAL_EXPIRED', 'This suggestion was already resolved or has expired.')); }
                return;
            }
            case 'suggestions.discardAll': return { discarded: this.host.suggestions.discardAll() };
            case 'suggestions.applied': {
                const typed = message as WebviewMessageOf<'suggestions.applied'>;
                if (!this.host.trusted() || !this.host.suggestions.acknowledge(typed.id, typed.revision)) { throw new EvidenceFailure(explorerError('PROPOSAL_CONFLICT')); }
                return;
            }
            case 'handoff.share': {
                const typed = message as WebviewMessageOf<'handoff.share'>;
                if (!this.host.trusted() || !this.host.handoffs) { throw new EvidenceFailure(explorerError('CAPABILITY_DISABLED')); }
                const owner = this.selectionEpoch;
                const approved = await vscode.window.showWarningMessage('Share this Explorer text with a coding agent?', { modal: true,
                    detail: 'Only this complete snapshot will be available through an expiring token for ten minutes. Review the displayed text before sharing; the agent cannot read other drafts with it.' }, 'Share snapshot');
                if (approved !== 'Share snapshot') { return { shared: false }; }
                if (owner !== this.selectionEpoch) { throw new EvidenceFailure(explorerError('STALE_BINDING')); }
                const value = this.host.handoffs.share(typed.kind as 'studio' | 'planner', typed.revision, typed.content, typed.publication);
                return { shared: true, token: value.token, content_sha256: value.content_sha256, expires_unix_ms: value.expires_unix_ms };
            }
            case 'suggestions.revisions': {
                const typed = message as WebviewMessageOf<'suggestions.revisions'>;
                this.draftRevisions = { studio: typed.studio, planner: typed.planner }; return;
            }
            case 'review.changes': return reviewChanges(this.reviewContext());
            case 'review.impact': return reviewImpact(this.reviewContext());
            case 'review.tests': return reviewTests(this.reviewContext());
            case 'recipes.list': return this.recipes();
            case 'recipes.save': {
                const recipe = validateRecipe((message as WebviewMessageOf<'recipes.save'>).recipe);
                if (!recipe) { throw new EvidenceFailure(explorerError('INVALID_SCHEMA', 'The recipe has unknown fields or invalid values.')); }
                const expected = (message as WebviewMessageOf<'recipes.save'>).expected;
                if (expected !== undefined && expected !== JSON.stringify(this.recipes().find(entry => entry.name === recipe.name) ?? null)) {
                    throw new EvidenceFailure(explorerError('PROPOSAL_CONFLICT', 'The saved recipe changed while importing. Refresh and review it again.'));
                }
                const others = this.recipes().filter(entry => entry.name !== recipe.name);
                if (others.length >= RECIPE_LIMIT) { throw new EvidenceFailure(explorerError('INVALID_SCHEMA', `At most ${RECIPE_LIMIT} recipes are kept; delete one first.`)); }
                await this.host.workspaceState.update(RECIPES_KEY, [...others, recipe].sort((a, b) => a.name.localeCompare(b.name)));
                return this.recipes();
            }
            case 'recipes.delete': {
                const name = (message as WebviewMessageOf<'recipes.delete'>).name;
                await this.host.workspaceState.update(RECIPES_KEY, this.recipes().filter(entry => entry.name !== name));
                return this.recipes();
            }
            case 'planner.openDraft': return openPlanDraft(this.planMarkdown((message as WebviewMessageOf<'planner.openDraft'>).plan));
            case 'planner.import': return (await importPlan()) ?? null;
            case 'files.list': {
                if (!this.host.trusted()) { throw new EvidenceFailure(explorerError('WORKSPACE_UNTRUSTED')); }
                const typed = message as WebviewMessageOf<'files.list'>;
                return listWorkspaceFiles(typed.kind, typed.directory, typed.workspace, typed.offset,
                    this.host.specs().find(spec => spec.service?.project === this.selected)?.root);
            }
            case 'files.load': {
                if (!this.host.trusted()) { throw new EvidenceFailure(explorerError('WORKSPACE_UNTRUSTED')); }
                const typed = message as WebviewMessageOf<'files.load'>;
                return loadWorkspaceFile(typed.kind, typed.workspace, typed.path);
            }
            case 'recipes.import': {
                if (!this.host.trusted()) { throw new EvidenceFailure(explorerError('WORKSPACE_UNTRUSTED')); }
                return (await importRecipe()) ?? null;
            }
            case 'recipes.export': {
                if (!this.host.trusted()) { throw new EvidenceFailure(explorerError('WORKSPACE_UNTRUSTED')); }
                const recipe = validateRecipe(message.recipe), epoch = this.selectionEpoch;
                if (!recipe) { throw new EvidenceFailure(explorerError('INVALID_SCHEMA')); }
                const name = recipe.name.toLowerCase().replace(/[^a-z0-9]+/g, '-').slice(0, 80) || 'search';
                return savePlanInWorkspace(JSON.stringify(recipe, null, 2) + '\n', `${name}.recipe.airk.json`,
                    () => this.host.trusted() && !this.disposed && epoch === this.selectionEpoch,
                    { title: 'Export AIRK search recipe', filters: { 'AIRK recipe JSON': ['json'] }, directory: '.airk/recipes' });
            }
            case 'planner.exportJSON': {
                const typed = message as WebviewMessageOf<'planner.exportJSON'>, checked = validatePlan(typed.plan), epoch = this.selectionEpoch;
                if (!checked.ok) { throw new EvidenceFailure(explorerError('INVALID_SCHEMA', checked.errors.slice(0, 5).join('; '))); }
                if (checked.plan.project && checked.plan.project !== this.selected) { throw new EvidenceFailure(explorerError('STALE_BINDING')); }
                return savePlanInWorkspace(JSON.stringify(checked.plan, null, 2) + '\n', typed.fileName,
                    () => this.host.trusted() && !this.disposed && epoch === this.selectionEpoch && this.draftRevisions.planner === typed.revision,
                    { title: 'Export AIRK plan JSON', filters: { 'AIRK plan JSON': ['json'] } });
            }
            case 'planner.saveInWorkspace': {
                const typed = message as WebviewMessageOf<'planner.saveInWorkspace'>;
                if (!this.host.trusted()) { throw new EvidenceFailure(explorerError('WORKSPACE_UNTRUSTED')); }
                if (typed.plan.project && typed.plan.project !== this.selected) { throw new EvidenceFailure(explorerError('STALE_BINDING')); }
                const epoch = this.selectionEpoch;
                // The revision comes back unchanged so the webview can ignore a result for an older draft.
                return { ...(await savePlanInWorkspace(this.planMarkdown(typed.plan), typed.fileName,
                    () => this.host.trusted() && !this.disposed && epoch === this.selectionEpoch && this.draftRevisions.planner === typed.revision)), revision: typed.revision };
            }
            default: throw new EvidenceFailure(explorerError('CAPABILITY_DISABLED', `${(message as WebviewMessage).type} is not available in this build.`));
        }
    }

    private reviewContext(): ReviewContext {
        const project = this.requireProject();
        const spec = this.host.specs().find(entry => entry.service?.project === project)!;
        return { session: this.session(), handles: this.facts, project, root: displayPath(spec.root) };
    }

    /** Stored recipes, revalidated on every read so a damaged entry is dropped rather than run. */
    private recipes(): SearchRecipe[] {
        const stored = this.host.workspaceState.get<unknown[]>(RECIPES_KEY, []);
        return (Array.isArray(stored) ? stored : []).flatMap(entry => { const recipe = validateRecipe(entry); return recipe ? [recipe] : []; }).slice(0, RECIPE_LIMIT);
    }

    /** Plans are rendered on the host from a validated plan, never from webview-supplied Markdown. */
    private planMarkdown(value: Record<string, unknown>): string {
        const checked = validatePlan(value);
        if (!checked.ok) { throw new EvidenceFailure(explorerError('INVALID_SCHEMA', checked.errors.slice(0, 5).join('; '))); }
        return renderPlan(checked.plan, { workspace: vscode.workspace.name ?? 'workspace', extensionVersion: this.host.version,
            ...(this.status?.publication ? { currentPublication: this.status.publication } : {}) });
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
        topics, truncated: payload.truncated === true || payload.next_line != null,
        ...(Number.isSafeInteger(payload.start_line) && Number(payload.start_line) > 0 ? { startLine: Number(payload.start_line) } : {}),
        ...(Number.isSafeInteger(payload.next_line) && Number(payload.next_line) > 0 ? { nextLine: Number(payload.next_line) } : {}),
        ...(typeof payload.command === 'string' ? { command: payload.command } : {}), ...(typeof payload.tool === 'string' ? { tool: payload.tool } : {}) };
}

/** A flat record of short strings (strategy parameters or scope); anything else is refused. */
function textRecord(value: Record<string, unknown>, limit: number): Record<string, string> {
    const entries = Object.entries(value);
    if (entries.length > limit || entries.some(([key, item]) => !/^[A-Za-z][A-Za-z0-9]{0,31}$/.test(key) || typeof item !== 'string' || item.length > 1024)) {
        throw new EvidenceFailure(explorerError('INVALID_SCHEMA', 'Parameters must be short text values.'));
    }
    return Object.fromEntries(entries) as Record<string, string>;
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
