// SPDX-License-Identifier: Apache-2.0
import * as nodePath from 'node:path';
import {randomUUID} from 'node:crypto';
import * as vscode from 'vscode';
import { ABOUT_LINKS, EngineHelpView, ExplorerViewState, HostMessage, MESSAGE_VERSION, SearchSessionView, StatusSummary, WebviewMessage, WebviewMessageOf } from './explorerContracts';
import { ControlSource, ExplorerControl } from './explorerControl';
import { explorerError, ExplorerError } from './explorerErrors';
import { EvidenceClient, EvidenceFailure } from './explorerEvidence';
import { FEATURES } from './explorerFeatures';
import { ExplorerPanel } from './explorerPanel';
import { formatRoute, parseRoute, WORKSPACES } from './explorerRoutes';
import { continueFacts, FactHandles, runFacts, scoped } from './explorerFacts';
import { changesSince, checkCitations, inspect } from './explorerInspect';
import { managementView, panelDetails } from './explorerManagement';
import { originArgs, originatingResult, passageView, RefRegistry, searchArgs, searchView } from './explorerSearch';
import { headline, statusSummary, scalarWithRetainedDetails } from './explorerStatus';
import { displayPath } from './explorerSource';
import { exportStrategy, importPlan, importRecipe, importStrategy, listWorkspaceFiles, loadWorkspaceFile, dirtyAt, openPlanDraft, openSource, savePlanInWorkspace } from './explorerWorkspaceIO';
import { renderPlan, RunResult, SearchRecipe, validatePlan, validateRecipe, validateStrategy } from './explorerStrategies';
import { runStrategy } from './explorerStrategyRunner';
import { SuggestionStore, suggestionsView } from './explorerSuggestions';
import { ReviewContext, reviewChanges, reviewImpact, reviewTests } from './explorerReview';
import { LaunchSpec } from './model';
import { saveStudioInWorkspace } from './explorerWorkspaceIO';
import { openAgentInstructions } from './explorerWorkspaceIO';
import { validateStudio } from './explorerStrategies';
import { usageAction } from './usageReport';
import { HandoffStore } from './explorerHandoffs';
import { IndexingCatalogController } from './indexingCatalogController';
import { catalogExport, CatalogObject, WindowContextDescriptor, validateNativeHostContext, validateWindowContext } from './indexingCatalog';
import { MANAGE_WORKSPACE_TRUST_COMMAND } from './explorerCommands';
import { NativeExecutionDisabled } from './nativeGate';
import type { AssistanceWindowStatus } from './explorerContracts';
import type { SearchDraft } from './explorerDrafts';
import { nativeSavedSourceState, readResultBody, verificationLabel } from './sourceVerification';
import { ServiceError } from './serviceClient';

/** The narrow surface of the service extension that the Explorer may use. */
export interface ExplorerHost {
    /** Resolve an already reviewed selection against protected native HTTPS setup. */
    prepareEvidenceConnection?(spec:LaunchSpec,client?:'airk-explorer'|'airk-workbench'):Promise<LaunchSpec>;
    /** Validate the active frozen combined selection and prefill an inert form. */
    prefillCombinedSearch?(search: SearchDraft): Promise<void>;
    prefillRecommendedSearch?(search: SearchDraft): Promise<void>;
    /** Trusted owning adapter supplies canonical identity; absence keeps window association controls gated. */
    windowContext?(): WindowContextDescriptor | undefined;
    windowContextObservation?(): CatalogObject | undefined;
    /** Stable window/workspace producer generation, independent of observed metadata. */
    windowContextIdentity?(): string;
    prepareWindowContext?(): Promise<void>;
    useGrant?(choice:CatalogObject,namespace:string,revision:number,valid:()=>boolean):Promise<CatalogObject>;
    readonly onDidChangeWindowContext?: vscode.Event<void>;
    usage?(spec: LaunchSpec, valid: () => boolean): Promise<EmbeddedUsage>;
    readonly extensionUri: vscode.Uri;
    readonly version: string;
    extensionUpdate?(): ExplorerViewState['extensionUpdate'];
    connectionCheck?(): ExplorerViewState['connectionCheck'];
    workbenchVisible?(): boolean;
    toggleWorkbench?(): Promise<void>;
    readonly onDidChangeWorkbenchVisibility?: vscode.Event<void>;
    startup?(): ExplorerViewState['startup'];
    runtimeUpgrade?(): ExplorerViewState['runtimeUpgrade'];
    searchScopeSelection?():{index_ids:string[];project_ids:string[];managedProjectIds:string[];active:boolean};
    applySearchScope?(selection:CatalogObject):Promise<CatalogObject>;
    saveIndexSet?(selection:CatalogObject):Promise<CatalogObject>;
    reloadUpdatedExtension?(): Promise<void>;
    trusted(): boolean;
    workspaceTrusted?(): boolean;
    nativeExecutionEnabled?(): boolean;
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
const DISMISSED_NOTICES_KEY = 'airk.explorer.dismissedNotices';
const DISMISSED_NOTICES_LIMIT = 512;
function validNoticeKey(key: unknown): key is string {
    return typeof key === 'string' && key.length > 0 && key.length <= 8192
        && !/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(key) && new TextEncoder().encode(key).length <= 8192;
}
export interface EmbeddedUsage {
    view(): { html: string };
    action(message: unknown): Promise<{ html: string }>;
    dispose(): void;
}
const RECIPE_LIMIT = 50;
interface ContextObservation {
    owner: string; attempts: number; deadline: number;
    state: 'idle' | 'reading' | 'waiting' | 'complete' | 'stopped'; retryAt?: number;
}
const CONTEXT_OBSERVATION_ATTEMPTS = 8;
const CONTEXT_OBSERVATION_BUDGET_MS = 30000;

export interface ExplorerSnapshotListener { (state: ExplorerViewState): void }

/**
 * Owns the Explorer panel, its evidence session and the published view state. All
 * AIRK work goes through the existing control facade or the evidence session; the
 * webview only renders state and sends validated, typed requests.
 */
export class ExplorerController implements vscode.Disposable {
    private readonly control: ExplorerControl;
    private readonly indexing: IndexingCatalogController;
    private indexingDelivery?: { panel: number; tag: string };
    private readonly panel: ExplorerPanel;
    private readonly listeners: vscode.Disposable[] = [];
    private readonly snapshotListeners = new Set<ExplorerSnapshotListener>();
    private readonly refs = new RefRegistry();
    private readonly facts = new FactHandles(this.refs);
    private lastStrategyRun?:{project:string;selection:number;result:RunResult};
    private evidence?: EvidenceClient;
    private selected?: string;
    private selectedSpecIdentity?:string;
    private readonly sourceFiles = new Map<string, string>();
    private selectionEpoch = 0;
    private searchState?: SearchSessionView;
    private usageSession?: { epoch: number; value: Promise<EmbeddedUsage> };
    private searchSequence = 0;
    private status?: StatusSummary;
    private statusFor?: string;
    private statusIdentity?: string;
    private observedIdentity?: string;
    private contextObservation?: ContextObservation;
    private contextObservationRead?: Promise<void>;
    private contextObservationTimer?: ReturnType<typeof setTimeout>;
    private panelVisible = false;
    private statusLoading?: Promise<void>;
    private statusLoadingKind?: 'scalar' | 'details';
    private retainedDetails?: { project: string; binding: string; identity?: string; summary: StatusSummary; settings?: string };
    private notice?: ExplorerError;
    private noticeWrites: Promise<void> = Promise.resolve();
    private revision = 0;
    private route = '#overview';
    private retained?: { dispose(): void };
    private managementOpen = false;
    private lastAutomaticStatusCheck = 0;
    private disposed = false;

    constructor(private readonly host: ExplorerHost) {
        this.control = new ExplorerControl(host.control, host.version);
        const control=this.control;
        this.indexing = new IndexingCatalogController({ get control(){return control.catalog;}, identity: () => `${this.control.observationIdentity}:${this.selectionEpoch}`,
            authorized: () => !this.disposed && this.host.trusted() && !this.host.extensionUpdate?.(), windowContext:()=>this.host.windowContext?.(),windowContextObservation:()=>this.host.windowContextObservation?.(),canReadWindowContext:()=>!!this.host.prepareWindowContext&&this.host.nativeExecutionEnabled?.()!==false,...(host.useGrant?{useGrant:host.useGrant.bind(host)}:{}),searchScopeSelection:host.searchScopeSelection?.bind(host),applySearchScope:host.applySearchScope?.bind(host),saveIndexSet:host.saveIndexSet?.bind(host),changed: () => this.publish() });
        this.panel = new ExplorerPanel(host.extensionUri, {
            message: message => { void this.handle(message); },
            visible: visible => this.visibility(visible),
            rejected: () => host.log('AIRK Explorer ignored an unrecognized webview message.'),
        });
        this.listeners.push(this.control.onDidChange(() => this.observedControl()), host.onDidChangeSpecs(() => this.specsChanged()), host.suggestions.onDidChange(() => this.publish()));
        if(host.onDidChangeWorkbenchVisibility)this.listeners.push(host.onDidChangeWorkbenchVisibility(()=>this.publish()));
        if(host.onDidChangeWindowContext)this.listeners.push(host.onDidChangeWindowContext(()=>{this.publish();this.observeConnectedContext();}));
        this.selectDefault();
        this.rememberSource(vscode.window.activeTextEditor);
        const editorChanges = vscode.window.onDidChangeActiveTextEditor?.(editor => { if (this.rememberSource(editor)) { this.publish(); } });
        if (editorChanges) { this.listeners.push(editorChanges); }
        this.publish();
        this.observeConnectedContext();
    }

    /** Reveal the Explorer, optionally at a route from the manifest. */
    open(route?: string): void {
        this.rememberSource(vscode.window.activeTextEditor);
        if (route && parseRoute(route)) { this.route = formatRoute(parseRoute(route)!); }
        this.panel.reveal();
        this.panel.post({ v: MESSAGE_VERSION, type: 'navigate', route: this.route });
        this.publish();
        void this.loadStatus();
        this.observeConnectedContext(true);
    }

    /** Studio and Planner draft revisions last reported by the webview (for agent draft conflicts). */
    draftRevisions: { studio?: number; planner?: number } = {};
    /** Publication of the selected project's last status, if observed. */
    get publication(): string | undefined {
        return this.host.trusted() && this.statusFor===this.selected
            && this.statusIdentity===this.control.observationIdentity ? this.status?.publication : undefined;
    }
    get assistanceStatus(): Pick<AssistanceWindowStatus, 'active_route' | 'binding_generation' | 'blockers' | 'selected_project'> {
        return { active_route: this.route.replace(/^#/, ''), binding_generation: this.selectionEpoch, selected_project: this.selected,
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
            workbenchVisible: this.host.workbenchVisible?.()??false,
            ...(this.host.extensionUpdate?.() ? { extensionUpdate: this.host.extensionUpdate() } : {}),
            connectionCheck: this.host.connectionCheck?.(),
            startup: this.host.startup?.(),
            runtimeUpgrade: this.host.trusted() ? this.host.runtimeUpgrade?.() : undefined,
            trusted: this.host.trusted(),
            workspaceTrusted: this.host.workspaceTrusted?.(),
            nativeExecutionEnabled: this.host.nativeExecutionEnabled?.(),
            connection: this.host.trusted()?this.control.connection():this.host.workspaceTrusted?.()===false?'untrusted':'disconnected',
            projects,
            ...(this.control.observedAt() !== undefined ? { projectsObservedAt: this.control.observedAt() } : {}),
            ...(this.selected ? { selectedProject: this.selected } : {}),
            ...(this.selected && this.host.trusted() && this.sourceFiles.get(this.selected) ? { sourceFile: this.sourceFiles.get(this.selected) } : {}),
            selectionEpoch: this.selectionEpoch,
            ...(this.host.trusted() && this.status && this.statusFor === this.selected && this.statusIdentity === this.control.observationIdentity ? { status: this.status } : {}),
            features: FEATURES,
            indexing: this.indexing.snapshot(),
            ...(this.notice ? { notice: this.notice } : {}),
            dismissedNotices: this.dismissedNotices(),
            ...(this.control.message() ? { controlMessage: this.control.message()!.slice(0, 600) } : {}),
            ...(this.control.morePages() ? { moreProjects: true } : {}),
            ...(this.managementOpen && this.host.trusted() ? { management: managementView(this.control.state) } : {}),
            ...(this.host.trusted() ? { suggestions: suggestionsView(this.host.suggestions.list()) } : {}),
        };
    }

    private dismissedNotices(): string[] {
        const stored = this.host.workspaceState.get<unknown>(DISMISSED_NOTICES_KEY);
        return Array.isArray(stored) ? [...new Set(stored.filter(validNoticeKey))] : [];
    }

    /** Serialize preference writes so rapid dismissals cannot overwrite each other. */
    private dismissNotice(key: string): Promise<void> {
        const write = this.noticeWrites.then(async () => {
            if (this.disposed) { throw new EvidenceFailure(explorerError('CLIENT_NOT_AUTHORIZED')); }
            if (!validNoticeKey(key)) { throw new Error('Notice dismissal key must contain 1–8192 UTF-8 bytes.'); }
            const keys = this.dismissedNotices();
            if (!keys.includes(key)) {
                if (keys.length >= DISMISSED_NOTICES_LIMIT) { throw new Error('Explorer has reached its limit of 512 dismissed notices. This notice was not dismissed.'); }
                await this.host.workspaceState.update(DISMISSED_NOTICES_KEY, [...keys, key]);
            }
            this.publish();
        });
        this.noticeWrites = write.catch(() => undefined);
        return write;
    }

    private publish(): void {
        if (this.disposed) { return; }
        this.revision++;
        const state = this.snapshot();
        // Routine scalar console updates do not resend retained catalog pages or previews.
        const {indexing,...scalarState}=state;
        this.panel.post({ v: MESSAGE_VERSION, type: 'state', state: scalarState });
        if(indexing){
            const tag=JSON.stringify([indexing.generation,indexing.busy,indexing.error,indexing.pending,indexing.review?.previewDigest]);
            if(this.indexingDelivery?.panel!==this.panel.generation||this.indexingDelivery.tag!==tag){
                this.indexingDelivery={panel:this.panel.generation,tag};
                this.panel.post({v:MESSAGE_VERSION,type:'indexing.session',session:indexing});
            }
        }
        for (const listener of this.snapshotListeners) { listener(state); }
        const { word } = headline(state.connection, state.status, state.projects.length);
        const project = state.projects.find(entry => entry.project === state.selectedProject);
        this.host.setStatusBar(`AIRK: ${word}`, project ? `AIRK Explorer · ${project.name} · ${word}` : `AIRK Explorer · ${word}`);
    }

    private visibility(visible: boolean): void {
        this.panelVisible = visible;
        if (!visible) { this.clearContextObservationTimer(); }
        if (!visible && !this.panel.open) { this.managementOpen = false; }
        if (visible && !this.retained) { this.retained = this.control.retain(); void this.loadStatus(); }
        else if (!visible && this.retained) { this.retained.dispose(); this.retained = undefined; }
        if (visible) { this.observeConnectedContext(); }
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
        this.selectedSpecIdentity=JSON.stringify(this.host.specs().find(spec=>spec.service?.project===project));
        this.selectionEpoch++;
        this.searchState = undefined;
        this.evidence?.dispose(); this.evidence = undefined;
        this.status = undefined; this.statusFor = undefined; this.notice = undefined;
        this.refs.clear(); this.facts.clear();
    }

    private specsChanged(): void {
        const spec = this.selected ? this.host.specs().find(entry => entry.service?.project === this.selected) : undefined;
        // A changed launcher binding (runtime upgrade, storage change) invalidates the session.
        const identity=JSON.stringify(spec);
        if (identity!==this.selectedSpecIdentity) {
            this.selectedSpecIdentity=identity;
            this.evidence?.dispose(); this.evidence = undefined;
            this.selectionEpoch++; this.status = undefined; this.statusFor = undefined;
            this.retainedDetails=undefined;this.statusIdentity=undefined;this.usageSession?.value.then(value=>value.dispose(),()=>{});this.usageSession=undefined;
            this.searchState = undefined;
            this.refs.clear(); this.facts.clear();
        }
        this.selectDefault();
        this.publish();
        if (this.panel.open) { void this.loadStatus(); }
        this.observeConnectedContext();
    }

    private observedControl(): void {
        if(this.observedIdentity!==this.control.observationIdentity){
            this.observedIdentity=this.control.observationIdentity;
            this.status=undefined;this.statusFor=undefined;this.statusIdentity=undefined;this.notice=undefined;
            this.lastAutomaticStatusCheck=0;
        }
        // Refresh published status when the cached console reports a different publication.
        const row = this.control.projects().find(entry => entry.project === this.selected);
        if (this.retained&&this.panel.open&&(Date.now()-this.lastAutomaticStatusCheck>=5000
            ||row?.publication&&this.status?.publication&&row.publication!==this.status.publication)) {
            this.lastAutomaticStatusCheck=Date.now();void this.loadStatus();
        }
        this.publish();
        this.observeConnectedContext();
    }

    private contextObservationOwner(): string | undefined {
        const identity = this.control.observationIdentity;
        return this.disposed || !this.host.trusted() || this.host.extensionUpdate?.() || this.host.nativeExecutionEnabled?.() === false
            || !this.control.state.connected || !identity || !this.host.prepareWindowContext
            // Native window ownership is independent of the selected evidence
            // view. Use Grant can change that view without replacing its host.
            ? undefined : JSON.stringify([identity, this.host.windowContextIdentity?.() ?? this.selectionEpoch]);
    }
    private completeWindowContextObservation():boolean {
        try{
            const descriptor=validateWindowContext(this.host.windowContext?.()),observation=this.host.windowContextObservation?.();
            if(observation?.status!=='catalog_host_context')return false;
            const native=validateNativeHostContext(observation.descriptor),capabilities=observation.capabilities;
            if(!capabilities||typeof capabilities!=='object'||Array.isArray(capabilities))return false;
            const contract=(capabilities as CatalogObject).policy_contract;
            return native.host===descriptor.host&&native.key===descriptor.key&&typeof contract==='number'&&Number.isSafeInteger(contract)&&contract>=0;
        }catch{return false;}
    }
    private clearContextObservationTimer(): void {
        if (this.contextObservationTimer !== undefined) { clearTimeout(this.contextObservationTimer); this.contextObservationTimer = undefined; }
    }
    private scheduleContextObservation(observation: ContextObservation): void {
        if (!this.panelVisible || this.contextObservation !== observation || this.contextObservationTimer !== undefined) { return; }
        const delay = Math.max(0, (observation.retryAt ?? Date.now()) - Date.now());
        if (observation.attempts >= CONTEXT_OBSERVATION_ATTEMPTS || Date.now() + delay >= observation.deadline) {
            observation.state = 'stopped';
            this.host.log('AIRK connected-runtime capability observation exhausted its bounded retry budget.');
            return;
        }
        this.contextObservationTimer = setTimeout(() => {
            this.contextObservationTimer = undefined;
            if (this.panelVisible && this.contextObservation === observation && observation.owner === this.contextObservationOwner()) { this.observeConnectedContext(); }
        }, delay);
        this.contextObservationTimer.unref?.();
    }

    /** Observe native capabilities; only known read-only/pre-query refusals may retry. */
    private observeConnectedContext(retry=false):void {
        const owner = this.contextObservationOwner();
        if (!owner) { this.clearContextObservationTimer(); this.contextObservation = undefined; return; }
        if (this.contextObservation?.owner !== owner) {
            this.clearContextObservationTimer();
            this.contextObservation = { owner, attempts: 0, deadline: Date.now() + CONTEXT_OBSERVATION_BUDGET_MS, state: 'idle' };
        }
        let observation = this.contextObservation;
        // An explicit read can finish while the automatic observer is backing
        // off. Its current authenticated result satisfies the same owner; do
        // not later reread a newer catalog clock and retire an active review.
        if(!this.contextObservationRead&&this.completeWindowContextObservation()){
            this.clearContextObservationTimer();observation.state='complete';return;
        }
        if (retry && !this.contextObservationRead && !this.host.windowContextObservation?.()
            && ['complete', 'stopped'].includes(observation.state)) {
            observation = this.contextObservation = { owner, attempts: 0, deadline: Date.now() + CONTEXT_OBSERVATION_BUDGET_MS, state: 'idle' };
        }
        if (this.contextObservationRead || observation.state === 'complete' || observation.state === 'stopped') { return; }
        if (observation.state === 'waiting') {
            if (!this.panelVisible) { return; }
            if (Date.now() >= observation.deadline || observation.attempts >= CONTEXT_OBSERVATION_ATTEMPTS) { observation.state = 'stopped'; return; }
            if (Date.now() < (observation.retryAt ?? 0)) { this.scheduleContextObservation(observation); return; }
        }
        this.clearContextObservationTimer();
        observation.state = 'reading'; observation.attempts++;
        const current = () => this.contextObservation === observation && owner === this.contextObservationOwner();
        const read = Promise.resolve().then(() => current() ? this.host.prepareWindowContext!() : undefined).then(() => {
            if (current()) { observation.state = 'complete'; this.publish(); }
        }, error => {
            if (!current()) { return; }
            this.host.log(`AIRK connected-runtime capability observation failed: ${String(error)}`);
            if (error instanceof ServiceError && !error.outcomeUnknown
                && ['registry_busy', 'console_queue_full', 'catalog_control_connection_unavailable'].includes(error.code)) {
                observation.state = 'waiting';
                observation.retryAt = Date.now() + Math.min(4000, 250 * 2 ** (observation.attempts - 1));
                this.scheduleContextObservation(observation);
            } else { observation.state = 'stopped'; }
        }).finally(() => {
            if (this.contextObservationRead === read) { this.contextObservationRead = undefined; }
            // An old owner finishing cannot hydrate the replacement window; its
            // release may admit one fresh observation for the current owner.
            if (this.contextObservation !== observation || owner !== this.contextObservationOwner()) { this.observeConnectedContext(); }
        });
        this.contextObservationRead = read;
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
    private requireAuthorization(native=false): void {
        if(this.disposed)throw new EvidenceFailure(explorerError('CLIENT_NOT_AUTHORIZED'));
        if(!this.host.trusted())throw new EvidenceFailure(explorerError(this.host.workspaceTrusted?.()===false?'WORKSPACE_UNTRUSTED':'CLIENT_NOT_AUTHORIZED'));
        if(native&&this.host.nativeExecutionEnabled?.()===false)throw new NativeExecutionDisabled();
    }

    /** The evidence session for the selected project, created lazily. */
    protected session(): EvidenceClient {
        this.requireAuthorization(true);
        const spec = this.host.specs().find(entry => entry.service?.project === this.selected);
        if (!spec) { throw new EvidenceFailure(explorerError('NO_PROJECT')); }
        if (!this.evidence) { this.evidence = new EvidenceClient(spec, this.host.version,{resolveConnection:this.host.prepareEvidenceConnection?.bind(this.host)}); }
        return this.evidence;
    }

    private detailSettingsKey(project: string): string | undefined {
        const object = (value: unknown): Record<string, unknown> => value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};
        const state = this.control.state;
        const page = state.inactiveProjects ? state.activePage : state.page;
        const row = page?.projects.find(entry => object(object(entry.project).registration).binding === project);
        const snapshot = object(object(row?.inspection).snapshot);
        return typeof snapshot.incarnation === 'string' && typeof snapshot.settings_digest === 'string'
            ? JSON.stringify([snapshot.incarnation, snapshot.settings_digest, snapshot.vectors_enabled]) : undefined;
    }

    private loadStatus(kind: 'scalar' | 'details' = 'scalar'): Promise<void> {
        if (this.disposed) { return Promise.resolve(); }
        if (this.statusLoading) {
            if (kind !== 'details' || this.statusLoadingKind !== 'scalar') { return this.statusLoading; }
            const project = this.selected, epoch = this.selectionEpoch, identity = this.control.observationIdentity;
            const generation = this.panel.generation;
            const spec = JSON.stringify(this.host.specs().find(entry => entry.service?.project === project));
            return this.statusLoading.then(() => {
                if (!this.disposed && this.host.trusted() && this.panel.open && generation === this.panel.generation
                    && project === this.selected && epoch === this.selectionEpoch && identity === this.control.observationIdentity
                    && spec === JSON.stringify(this.host.specs().find(entry => entry.service?.project === project))) {
                    return this.loadStatus('details');
                }
            });
        }
        const project = this.selected;
        const epoch = this.selectionEpoch;
        const spec = this.host.specs().find(entry => entry.service?.project === project);
        if (!project || !this.host.trusted() || !spec) { return Promise.resolve(); }
        const binding = JSON.stringify(spec), generation = this.panel.generation;
        const identity = this.control.observationIdentity;
        const settings = this.detailSettingsKey(project);
        const bound = () => !this.disposed && this.host.trusted() && project === this.selected
            && identity === this.control.observationIdentity && epoch === this.selectionEpoch
            && (kind !== 'details' || settings === this.detailSettingsKey(project))
            && JSON.stringify(this.host.specs().find(entry => entry.service?.project === project)) === binding;
        const current = () => bound() && this.panel.open && generation === this.panel.generation;
        this.statusLoadingKind = kind;
        this.statusLoading = (async () => {
            try {
                const result = await this.session().call(kind === 'details' ? 'airk_diagnostics' : 'airk_status',
                    { response_mode: 'full' }, { timeout: kind === 'details' ? 90000 : 15000, supersede: 'status' });
                if (current()) {
                    const raw = result.payload.observed_unix_ms;
                    const received = Date.now();
                    const observed = kind === 'scalar' && typeof raw === 'number'
                        && Number.isSafeInteger(raw) && raw >= 0 ? Math.min(raw, received) : received;
                    const summary = statusSummary(result.payload, observed, result.rawPayload);
                    if (kind === 'details') { this.retainedDetails = { project, binding, identity, summary: { ...summary, detailsObservedAt: observed }, settings }; }
                    const prior = this.retainedDetails;
                    const detail = prior?.project === project && prior.binding === binding && prior.identity === identity
                        ? prior.summary : undefined;
                    this.status = kind === 'details' ? { ...summary, detailsObservedAt: observed } : scalarWithRetainedDetails(summary, detail, prior?.settings === this.detailSettingsKey(project));
                    this.statusFor = project; this.statusIdentity = identity; this.notice = undefined;
                }
            } catch (error) {
                if (current() && error instanceof EvidenceFailure && error.nativeResponse) {
                    const body = error.nativePayload ?? readResultBody(error.nativeResponse);
                    if (body.schema === 'airk.combined-read/1') {
                        this.status = statusSummary(body, Date.now(), error.nativeResponse);
                        this.statusFor = project; this.statusIdentity = identity;
                    } else if (error.nativePayload && !Object.keys(body).length) {
                        this.status = { ...statusSummary({}, Date.now()), nativeFailure: error.nativeResponse };
                        this.statusFor = project; this.statusIdentity = identity;
                    }
                }
                if (current() && failure(error).code !== 'NO_PROJECT') { this.notice = failure(error); }
            } finally {
                this.statusLoading = undefined; this.statusLoadingKind = undefined;
                if (current()) { this.publish(); }
                // A new visible recipient/binding gets its OWN observation. Do
                // not publish the old response into a recreated panel or trust scope.
                if (!this.disposed && this.host.trusted() && this.retained && this.panel.open
                    && (!bound() || generation !== this.panel.generation)) { void this.loadStatus(); }
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
        const panelGeneration = this.panel.generation;
        try {
            const data = await this.dispatch(message);
            if (epoch !== this.selectionEpoch && !['project.select', 'status.refresh', 'source.choose','indexing.grant.use'].includes(message.type)) { throw new EvidenceFailure(explorerError('REQUEST_SUPERSEDED')); }
            if (message.type !== 'ready' && panelGeneration === this.panel.generation) { this.respond(message.requestId, { data: data ?? null }); }
        } catch (error) {
            if (panelGeneration === this.panel.generation) { this.respond(message.requestId, { error: failure(error) }); }
        }
    }

    protected async dispatch(message: WebviewMessage): Promise<unknown> {
        if(this.disposed)throw new EvidenceFailure(explorerError('CLIENT_NOT_AUTHORIZED'));
        if(message.type.startsWith('indexing.'))this.requireAuthorization(message.type!=='indexing.export');
        switch (message.type) {
            case 'notice.dismiss': return this.dismissNotice(message.key);
            case 'indexing.recommendations':
                return this.indexing.recommendations(async()=> (await this.session().call('airk_query_scope',{...message.arguments,view:'recommendations',max_bytes:65536},{supersede:'recommendations'})).payload);
            case 'indexing.recommendations.dismiss': return this.indexing.dismissRecommendations();
            case 'indexing.recommendations.copy':
            case 'indexing.recommendations.use': {
                const prior=this.indexing.snapshot().recommendations;
                const scopeEpoch=this.selectionEpoch;
                if(prior?.schema!=='airk.recommendations/1')throw new Error('Read current recommendations first.');
                const selection=prior.selected as CatalogObject;
                const choice={project_ids:selection.project_ids,index_ids:selection.index_ids,member_availability:selection.member_availability};
                // Revalidate the exact selection and current grants before an action.
                const current=(await this.session().call('airk_query_scope',{view:'recommendations',selection:choice,max_bytes:65536},{supersede:'recommendations'})).payload;
                this.requireAuthorization(true);
                if(scopeEpoch!==this.selectionEpoch||this.indexing.snapshot().recommendations!==prior)throw new Error('Recommendations changed; refresh before acting.');
                if(current.scope_identity!==prior.scope_identity||current.catalog_revision!==prior.catalog_revision)throw new Error('Recommendations changed; refresh before acting.');
                if(message.type==='indexing.recommendations.copy'){
                    const instructions=current.instructions as CatalogObject;
                    if(instructions.status!=='available'||typeof instructions.text!=='string')throw new Error('Scope instructions unavailable.');
                    await vscode.env.clipboard.writeText(instructions.text);return {copied:true};
                }
                if(!this.host.prefillRecommendedSearch)throw new Error('A reviewed combined Workbench connection is required.');
                await this.host.prefillRecommendedSearch({query:'',mode:'lexical',limit:12,maxBytes:8192,fullChunks:false,...choice} as SearchDraft);
                return {prefilled:true};
            }
            case 'workspace.manageTrust':
                if(this.disposed)throw new EvidenceFailure(explorerError('CLIENT_NOT_AUTHORIZED'));
                if(this.host.workspaceTrusted?.()!==false&&!this.host.trusted())throw new EvidenceFailure(explorerError('CLIENT_NOT_AUTHORIZED'));
                await vscode.commands.executeCommand(MANAGE_WORKSPACE_TRUST_COMMAND);return;
            case 'indexing.page': {
                this.requireAuthorization(true);
                // Establish the authenticated window owner before admitting a
                // page that can seed a native review. The producer shares its
                // in-flight read with automatic observation; this explicit read
                // has no retry loop and creates no grant or indexing work. A
                // saved explicit local choice can be revalidated on reconnect.
                if(this.host.prepareWindowContext&&(!this.host.windowContext?.()||this.host.windowContextObservation?.()?.status!=='catalog_host_context')){
                    const identity=this.control.observationIdentity,windowIdentity=this.host.windowContextIdentity?.();
                    await this.host.prepareWindowContext();this.requireAuthorization(true);
                    if(identity!==this.control.observationIdentity||windowIdentity!==this.host.windowContextIdentity?.())throw new ServiceError('window_context_changed');
                    if(!this.host.windowContext?.()||this.host.windowContextObservation?.()?.status!=='catalog_host_context')throw new ServiceError('catalog_control_connection_unavailable');
                }
                return this.indexing.page(message.kind,message.next,message.index,message.project);
            }
            case 'indexing.scope.read':
                this.requireAuthorization(true);
                if(this.host.prepareWindowContext)await this.host.prepareWindowContext();
                this.requireAuthorization(true);return this.indexing.scopeRead(message.kind,message.next);
            case 'indexing.scope.apply':return this.indexing.scopeApply(message.selection,message.generation);
            case 'indexing.set.read':return this.indexing.setRead(message.id);
            case 'indexing.set.save':return this.indexing.scopeApply(message.selection,message.generation,true);
            case 'indexing.detail': return this.indexing.detail(message.kind,message.id);
            case 'indexing.preview': return this.indexing.preview(message.operation,message.generation);
            case 'indexing.create': return this.indexing.create(message.name,message.generation);
            case 'indexing.apply': return this.indexing.apply(message.generation);
            case 'indexing.reconcile': return this.indexing.reconcile();
            case 'indexing.context.read':
                this.requireAuthorization();
                if(this.host.prepareWindowContext){this.indexing.invalidate();this.publish();await this.host.prepareWindowContext();this.requireAuthorization();return this.indexing.adoptHostContext();}
                return this.indexing.readContext();
            case 'indexing.context.preview': return this.indexing.previewContext(message.selection,message.generation);
            case 'indexing.context.delete': return this.indexing.previewContext({project_ids:[],index_ids:[]},message.generation,true);
            case 'indexing.grant.draft': return this.indexing.grantDraft(message.selection,message.generation);
            case 'indexing.grant.use': return this.indexing.useGrant(message.choice,message.generation);
            case 'indexing.grant.revoke': return this.indexing.revokeGrant(message.generation);
            case 'indexing.setup.draft': return this.indexing.setupDraft(message.selection,message.generation);
            case 'indexing.setup.apply': return this.indexing.setupApply(message.generation);
            case 'indexing.setup.resume':return this.indexing.setupResume(message.id,message.generation);
            case 'indexing.source.draft':return this.indexing.sourceDraft(message.change,message.generation);
            case 'indexing.source.read':return this.indexing.sourceRead(message.begin,message.generation);
            case 'indexing.source.pending':return this.indexing.sourcePending(message.generation);
            case 'indexing.split.open':return this.indexing.splitOpen(message.index,message.generation);
            case 'indexing.split.sourceRead':return this.indexing.splitSourceRead(message.generation);
            case 'indexing.split.draft':return this.indexing.splitDraft(message.selection,message.generation);
            case 'indexing.split.apply':return this.indexing.splitApply(message.generation);
            case 'indexing.split.read':return this.indexing.splitRead(message.operation,message.generation);
            case 'indexing.split.recover':return this.indexing.splitRecover(message.action,message.generation);
            case 'indexing.policy.read':return this.indexing.policyRead(message.generation);
            case 'indexing.policy.resume':return this.indexing.policyResume(message.generation);
            case 'indexing.retirement.read':return this.indexing.retirementRead(message.id,message.generation);
            case 'indexing.retirement.stopTracking':return this.indexing.stopTracking(message.generation);
            case 'indexing.source.resolve':return this.indexing.sourceResolve(message.generation);
            case 'indexing.export': {
                this.requireAuthorization();
                const markdown=message.format==='copy_markdown'||message.format==='markdown',text=catalogExport(this.indexing.snapshot(),markdown);
                if(message.format==='copy'||message.format==='copy_markdown'){await vscode.env.clipboard.writeText(text);return {copied:true};}
                const uri=await vscode.window.showSaveDialog({defaultUri:vscode.Uri.file(markdown?'airk-indexing.md':'airk-indexing.json'),filters:markdown?{Markdown:['md']}:{JSON:['json']}});
                if(uri){await vscode.workspace.fs.writeFile(uri,new TextEncoder().encode(text));return {saved:true};}return {saved:false};
            }
            case 'instructions.open': {
                this.requireAuthorization();
                if (message.project !== this.requireProject()) { throw new EvidenceFailure(explorerError('REQUEST_SUPERSEDED')); }
                const spec = this.host.specs().find(entry => entry.service?.project === message.project), epoch = this.selectionEpoch;
                if (!spec) { throw new EvidenceFailure(explorerError('NO_PROJECT')); }
                return openAgentInstructions(spec.root, message.agent, () => !this.disposed && this.host.trusted() && epoch === this.selectionEpoch);
            }
            case 'details.copy': {
                this.requireAuthorization();
                const status=message.project===this.selected&&this.statusFor===this.selected&&this.statusIdentity===this.control.observationIdentity?this.status:undefined;
                const text=panelDetails(managementView(this.control.state),message.section,this.host.version,message.project,status);
                await vscode.env.clipboard.writeText(text);return {copied:true};
            }
            case 'workbench.toggle':
                await this.host.toggleWorkbench?.();
                this.publish();return;
            case 'extension.reload':
                if(this.host.extensionUpdate?.())await this.host.reloadUpdatedExtension?.();
                return;
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
                this.requireAuthorization();
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
            case 'ready':
                this.indexingDelivery=undefined;
                this.publish(); this.panel.post({ v: MESSAGE_VERSION, type: 'navigate', route: this.route });
                if (this.searchState) { this.panel.post({ v: MESSAGE_VERSION, type: 'search.session', session: this.searchState }); }
                return;
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
            case 'status.refresh': await Promise.all([this.loadStatus('details'), this.control.refresh()]); return;
            case 'manage.refresh': await this.control.refresh(); return;
            case 'manage.observe': {
                this.managementOpen = (message as WebviewMessageOf<'manage.observe'>).open;
                this.publish(); return;
            }
            case 'manage.action': {
                const { action, expectedPlan, expectedPreview, v: _v, type: _type, requestId: _id, ...rest } = message as WebviewMessageOf<'manage.action'>;
                this.requireAuthorization();
                if(action==='approvePlan'&&(!expectedPlan||!expectedPreview||this.control.state.plan?.value.id!==expectedPlan||this.control.state.plan?.value.preview_digest!==expectedPreview)){
                    return {message:'Repair preview changed. Review the current proposed actions before starting repair.'};
                }
                const fields = Object.fromEntries(Object.entries(rest).filter(([, value]) => value !== undefined));
                const fingerprint=()=>JSON.stringify(managementView(this.control.state).projects.map(entry=>[entry.summary.project,entry.health,entry.jobId,entry.jobState,entry.pendingJobId,entry.work.phase,entry.work.captured,entry.work.extracted,entry.work.publications,entry.configuration]));
                const before=action==='refresh'?fingerprint():undefined;
                // The shared console validates the action, its fields and page membership again before acting.
                await this.control.run(action, fields);
                if(action==='refresh')return {message:this.control.state.connected
                    ?`Status checked at ${new Date().toLocaleTimeString()}. ${before===fingerprint()?'No change in project work or settings.':'Project observations updated.'} This checks service observations; it does not restart or repair indexing.`
                    :'Could not refresh the service connection. Retained observations are shown; current project progress is unconfirmed.'};
                return { message: this.control.message() ?? null };
            }
            case 'manage.command': {
                this.requireAuthorization();
                const command=(message as WebviewMessageOf<'manage.command'>).command;
                const result=await vscode.commands.executeCommand(command, ...(command==='airk.verifyMcp'?[this.selected]:[]));
                return command==='airk.httpIssuerRotationStatus'||command==='airk.rotateHttpIssuer'?result:undefined;
            }
            case 'search.form': {
                const { project, selectionEpoch, query, mode, pathPrefix, limit, maxBytes, fullChunks } = message;
                if (project !== this.requireProject() || selectionEpoch !== this.selectionEpoch) { throw new EvidenceFailure(explorerError('REQUEST_SUPERSEDED')); }
                this.searchState = { ...this.searchState, project, selectionEpoch, form: { query, mode, pathPrefix, limit, maxBytes, fullChunks }, busy: this.searchState?.busy ?? false, page: this.searchState?.page ?? 0 };
                return;
            }
            case 'search.page': {
                if (message.project !== this.requireProject() || message.selectionEpoch !== this.selectionEpoch || !this.searchState) { throw new EvidenceFailure(explorerError('REQUEST_SUPERSEDED')); }
                const last = Math.max(0, Math.ceil((this.searchState.result?.hits.length ?? 0) / 20) - 1);
                this.searchState = { ...this.searchState, page: Math.min(message.page, last) };
                return;
            }
            case 'search.run': {
                const typed = message as WebviewMessageOf<'search.run'>;
                const project = this.requireProject();
                const args = searchArgs(typed);
                const selectionEpoch = this.selectionEpoch, sequence = ++this.searchSequence;
                const binding = JSON.stringify(this.host.specs().find(entry => entry.service?.project === project));
                const { query, mode, limit, maxBytes, fullChunks } = typed;
                this.searchState = { project, selectionEpoch, form: { query, mode, limit, maxBytes, fullChunks, pathPrefix: typed.pathPrefix ?? '' }, busy: true, page: 0 };
                const publish = () => { this.panel.post({ v: MESSAGE_VERSION, type: 'search.session', session: this.searchState! }); };
                const current = () => !this.disposed && this.host.trusted() && project === this.selected
                    && selectionEpoch === this.selectionEpoch && sequence === this.searchSequence
                    && JSON.stringify(this.host.specs().find(entry => entry.service?.project === project)) === binding;
                publish();
                try {
                    const result = await this.session().call('airk_search', args, { supersede: 'search', timeout: 30000 });
                    if (!current()) { throw new EvidenceFailure(explorerError('REQUEST_SUPERSEDED')); }
                    const status = this.statusFor === project && this.statusIdentity === this.control.observationIdentity ? this.status : undefined;
                    const view = searchView(project, typed, result.payload, result.publication, this.refs,
                        { ...(status?.semantic ? { semantic: status.semantic } : {}), ...(status?.vectorsEnabled !== undefined ? { vectorsEnabled: status.vectorsEnabled } : {}) }, result.rawPayload);
                    this.searchState = { ...this.searchState!, result: view, busy: false }; publish();
                    // Search evidence travels once through search.session. The
                    // correlated response only releases the requesting waiter.
                    return { completed: true };
                } catch (error) {
                    if (current()) { this.searchState = { ...this.searchState!, result: undefined, busy: false, error: failure(error).message,
                        ...(error instanceof EvidenceFailure && error.nativeResponse ? { nativeFailure: error.nativeResponse } : {}) }; publish(); }
                    throw error;
                }
            }
            case 'evidence.expand': {
                const typed = message as WebviewMessageOf<'evidence.expand'>;
                const project = this.requireProject();
                const ref = this.refs.get(typed.ref, project);
                if (!ref.publication) { throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH')); }
                const infoResult = await this.session().call('airk_file_info', { path: ref.path, revision: ref.publication, known_hash: ref.contentHash, ...originArgs(ref.origin) });
                const info = originatingResult(infoResult.payload, ref.origin);
                if ((info.publication ?? infoResult.publication) !== ref.publication) { throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH')); }
                // Extracted coordinates have no host-known page boundary: keep one cited line.
                const sourceLines = info.payload.line_basis === 'source';
                const result = await this.session().call('airk_context', { path: ref.path, line: ref.startLine,
                    before: sourceLines ? typed.before : 0, after: sourceLines ? typed.after : 0, revision: ref.publication, known_hash: ref.contentHash, max_bytes: 32768, ...originArgs(ref.origin) }, { timeout: 30000 });
                const selected = originatingResult(result.payload, ref.origin);
                if ((selected.publication ?? result.publication) !== ref.publication) { throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH')); }
                return passageView(project, result.payload, result.publication, this.refs);
            }
            case 'source.open': {
                const typed = message as WebviewMessageOf<'source.open'>;
                const project = this.requireProject();
                const ref = this.refs.get(typed.ref, project);
                // The location on disk comes from AIRK's own file record, never from the webview.
                const native=await this.session().call('airk_file_info', { path: ref.path, ...originArgs(ref.origin) }, { timeout: 30000 });
                const selected=originatingResult(native.payload,ref.origin),info=selected.payload;
                const source = info.source as Record<string, unknown> | undefined;
                if (!source || typeof source.root !== 'string' || typeof source.relative_path !== 'string') {
                    throw new EvidenceFailure(explorerError('TOOL_ERROR', 'AIRK did not report a source location for this file.'));
                }
                return openSource({ root: source.root, relative: source.relative_path, contentHash: ref.contentHash, startLine: ref.startLine,
                    savedState:nativeSavedSourceState(info,ref.contentHash,selected.publication??native.publication),nativeResult:native.rawPayload??native.payload,
                    nativeMessage:info.content_hash!==ref.contentHash?'The indexed hash changed since this citation; a saved-source comparison for that older hash is unavailable.':verificationLabel(info.source_verification),
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
                    args: { symbol: symbol.symbolId, direction: typed.direction, hops: typed.hops, limit: typed.limit, max_bytes: 16384, ...originArgs(symbol.origin) } });
            }
            case 'code.outline': {
                const typed = message as WebviewMessageOf<'code.outline'>, project = this.requireProject();
                const ref = typed.ref ? this.refs.get(typed.ref, project) : undefined;
                const path = ref?.path ?? this.activeRelativePath();
                if (!path) { throw new EvidenceFailure(explorerError('NO_SOURCE_FILE')); }
                return runFacts(this.session(), this.facts, project, { tool: 'airk_outline', limit: 100, args: { path, limit: 100, max_bytes: 32768, ...originArgs(ref?.origin) } });
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
                const typed = message as WebviewMessageOf<'strategy.run'>, project = this.requireProject(),selection=this.selectionEpoch;
                const checked = validateStrategy(typed.strategy);
                if (!checked.ok) { throw new EvidenceFailure(explorerError('INVALID_SCHEMA', checked.errors.slice(0, 5).join('; '))); }
                const result=await runStrategy({ strategy: checked.strategy, params: textRecord(typed.params, 8), scope: textRecord(typed.scope, 1) },
                    { session: this.session(), facts: this.facts, project, extensionVersion: this.host.version, dirty: dirtyAt });
                if(!this.host.trusted()||selection!==this.selectionEpoch||this.requireProject()!==project)throw new EvidenceFailure(explorerError('STALE_BINDING'));
                const captured={...result,hostRunId:randomUUID().replaceAll('-','')};this.lastStrategyRun={project,selection,result:structuredClone(captured)};return captured;
            }
            case 'strategy.run.export':{
                const saved=this.lastStrategyRun,typed=message as WebviewMessageOf<'strategy.run.export'>;
                const current=()=>{if(!this.host.trusted()||!saved||saved.project!==this.requireProject()||saved.selection!==this.selectionEpoch||saved.result.hostRunId!==typed.runId||saved.result.manifest.evidenceDigest!==typed.digest)throw new EvidenceFailure(explorerError('STALE_BINDING'));};current();
                const text=JSON.stringify(saved!.result,null,2)+'\n';
                if(typed.format==='copy'){await vscode.env.clipboard.writeText(text);current();return {copied:true};}
                const target=await vscode.window.showSaveDialog({title:'Export complete strategy run and native source checks',filters:{JSON:['json']}});if(!target)return {saved:false};current();
                await vscode.workspace.fs.writeFile(target,new TextEncoder().encode(text));return {saved:true};
            }
            case 'strategy.export': {
                const checked = validateStrategy((message as WebviewMessageOf<'strategy.export'>).strategy);
                if (!checked.ok) { throw new EvidenceFailure(explorerError('INVALID_SCHEMA', checked.errors.slice(0, 5).join('; '))); }
                return exportStrategy(checked.strategy);
            }
            case 'strategy.import': return (await importStrategy()) ?? null;
            case 'usage.read': case 'usage.action': {
                const project = this.requireProject(), epoch = this.selectionEpoch;
                const spec = this.host.specs().find(entry => entry.service?.project === project);
                if (!spec || !this.host.usage) { throw new EvidenceFailure(explorerError('CAPABILITY_DISABLED', 'Usage is unavailable for this project.')); }
                if (this.usageSession?.epoch !== epoch) {
                    void this.usageSession?.value.then(session => session.dispose(), () => undefined);
                    const value = this.host.usage(spec, () => !this.disposed && this.host.trusted() && epoch === this.selectionEpoch);
                    this.usageSession = { epoch, value };
                    void value.catch(() => { if (this.usageSession?.value === value) { this.usageSession = undefined; } });
                }
                const session = await this.usageSession.value;
                if (message.type === 'usage.read') { return session.view(); }
                const typed = message as WebviewMessageOf<'usage.action'>;
                const action = usageAction({ type: 'usage', action: typed.action, ...(typed.days !== undefined ? { days: typed.days } : {}) });
                if (!action) { throw new EvidenceFailure(explorerError('INVALID_MESSAGE')); }
                return session.action(action);
            }
            case 'studio.save': {
                const typed = message as WebviewMessageOf<'studio.save'>, epoch = this.selectionEpoch;
                const document = validateStudio(typed.document);
                if (!document) { throw new EvidenceFailure(explorerError('INVALID_SCHEMA', 'Invalid Studio document.')); }
                const root = this.host.specs().find(spec => spec.service?.project === this.selected)?.root;
                if (!root) { throw new EvidenceFailure(explorerError('STALE_BINDING')); }
                return saveStudioInWorkspace(document, typed.name, root, typed.expectedHash,
                    () => this.host.trusted() && !this.disposed && epoch === this.selectionEpoch && this.draftRevisions.studio === typed.revision);
            }
            case 'clipboard.write': await vscode.env.clipboard.writeText((message as WebviewMessageOf<'clipboard.write'>).text); return;
            case 'suggestions.get': {
                const item = this.host.trusted() ? this.host.suggestions.get((message as WebviewMessageOf<'suggestions.get'>).id) : undefined;
                if (!item) { throw new EvidenceFailure(explorerError('PROPOSAL_EXPIRED', 'This suggestion is no longer available.')); }
                return { id: item.id, title: item.draft.title, status: item.status, client: item.client.name, warnings: item.warnings,
                    ...(item.targetRevision !== undefined ? { targetRevision: item.targetRevision } : {}), ...(item.draft.evidence ? { evidence: item.draft.evidence } : {}), body: item.draft.body };
            }
            case 'suggestions.resolve': {
                const typed = message as WebviewMessageOf<'suggestions.resolve'>;
                this.requireAuthorization();
                const item = this.host.suggestions.get(typed.id);
                const current = item?.kind === 'strategy' ? this.draftRevisions.studio : item?.kind === 'plan' ? this.draftRevisions.planner : undefined;
                if (typed.status === 'applied' && current !== undefined && typed.targetRevision !== current) { throw new EvidenceFailure(explorerError('PROPOSAL_CONFLICT')); }
                if (typed.status === 'applied' && item?.draft.body.kind === 'search' && item.draft.body.search.project_ids) {
                    const status = this.host.suggestions.status(typed.id)?.status;
                    if (status !== 'pending_review' && status !== 'deferred') { throw new EvidenceFailure(explorerError('PROPOSAL_EXPIRED')); }
                    if (!this.host.prefillCombinedSearch) { throw new EvidenceFailure(explorerError('CAPABILITY_DISABLED', 'A matching reviewed combined Workbench session is required to apply this draft.')); }
                    if (!this.host.suggestions.resolve(typed.id, 'applied')) { throw new EvidenceFailure(explorerError('PROPOSAL_EXPIRED')); }
                    try { await this.host.prefillCombinedSearch(item.draft.body.search); }
                    catch (error) { this.host.suggestions.resolve(typed.id, 'conflicted'); throw error; }
                    return;
                }
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
            case 'review.changes': return reviewChanges(this.reviewContext(), message);
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
                this.requireAuthorization();
                const typed = message as WebviewMessageOf<'files.list'>;
                return listWorkspaceFiles(typed.kind, typed.directory, typed.workspace, typed.offset,
                    this.host.specs().find(spec => spec.service?.project === this.selected)?.root);
            }
            case 'files.load': {
                this.requireAuthorization();
                const typed = message as WebviewMessageOf<'files.load'>;
                return loadWorkspaceFile(typed.kind, typed.workspace, typed.path);
            }
            case 'recipes.import': {
                this.requireAuthorization();
                return (await importRecipe()) ?? null;
            }
            case 'recipes.export': {
                this.requireAuthorization();
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
                this.requireAuthorization();
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
        if (spec.combinedSession) { throw new EvidenceFailure(explorerError('CAPABILITY_DISABLED', 'Git Review needs one explicit source/index context. Use qualified combined evidence in Workbench for member reads.')); }
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
            ...(this.publication ? { currentPublication: this.publication } : {}) });
    }

    /** Pending UI preferences finish before a replacement controller reads them. */
    async finishPendingNoticeWrites(): Promise<void> { this.dispose(); await this.noticeWrites; }
    dispose(): void {
        void this.usageSession?.value.then(session => session.dispose(), () => undefined);
        if (this.disposed) { return; }
        this.disposed = true;
        this.clearContextObservationTimer(); this.contextObservation = undefined;
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
