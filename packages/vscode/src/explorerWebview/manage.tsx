// SPDX-License-Identifier: Apache-2.0
import { ComponentChildren } from 'preact';
import { useEffect, useState } from 'preact/hooks';
import { FactRow, MANAGE_ACTIONS, MANAGE_COMMANDS, ManagedProject, ManagementView } from '../explorerContracts';
import { request } from './api';
import { Facts } from './components';
import { ago, observed, shortToken } from './format';
import type { ScreenProps } from './workspaces';

type Action = typeof MANAGE_ACTIONS[number];
type Command = typeof MANAGE_COMMANDS[number];
interface ActionFields { project?: string; id?: string; scope?: 'one' | 'selected' | 'all'; operation?: 'auto' | 'verify' | 'recover' | 'reconcile' | 'vectors' | 'rebuild'; selected?: boolean; includePaths?: boolean; allowFallback?: boolean }

/** Management tabs ask the host to include the shared console view while they are shown. */
function useManagement(state: ScreenProps['state']): ManagementView | undefined {
    useEffect(() => {
        void request('manage.observe', { open: true }).catch(() => undefined);
        return () => { void request('manage.observe', { open: false }).catch(() => undefined); };
    }, []);
    return state.management;
}

function useActions() {
    const [busy, setBusy] = useState<string>();
    const [outcome, setOutcome] = useState<string>();
    const act = async (action: Action, fields: ActionFields = {}, label = action) => {
        setBusy(label); setOutcome(undefined);
        try {
            const result = await request<{ message: string | null }>('manage.action', { action, ...fields });
            setOutcome(result.message ? `AIRK reported: ${result.message}` : undefined);
        } catch (failure) { setOutcome(failure instanceof Error ? failure.message : String(failure)); }
        finally { setBusy(undefined); }
    };
    const command = async (name: Command) => {
        try { await request('manage.command', { command: name }); } catch (failure) { setOutcome(failure instanceof Error ? failure.message : String(failure)); }
    };
    return { busy, outcome, act, command };
}

function Act(props: { actions: ReturnType<typeof useActions>; action: Action; fields?: ActionFields; enabled?: boolean; ghost?: boolean; children: ComponentChildren }) {
    const disabled = props.enabled === false || props.actions.busy !== undefined;
    return <button type="button" class={props.ghost ? 'ghost' : ''} disabled={disabled} onClick={() => { void props.actions.act(props.action, props.fields); }}>{props.children}</button>;
}

function Frame(props: { view?: ManagementView; trusted: boolean; actions: ReturnType<typeof useActions>; intro: string; children: ComponentChildren }) {
    if (!props.trusted || props.view?.trusted === false) { return <p class="callout">Trust this workspace before AIRK connects or accepts management requests.</p>; }
    if (!props.view) { return <p class="muted">Reading the AIRK service state…</p>; }
    return (
        <div class="manage">
            <p class="muted">{props.intro} {props.view.authority} · {props.view.connected ? 'connected' : 'disconnected'}{props.view.busy ? ' · working' : ''}</p>
            {props.actions.outcome && <p class="callout" role="status">{props.actions.outcome}</p>}
            {props.view.message && !props.actions.outcome && <p class="muted" role="status">Last control outcome: <code>{props.view.message}</code></p>}
            {props.children}
        </div>
    );
}

function Rows(props: { rows: readonly FactRow[] }) { return <Facts rows={props.rows.map(([name, value]) => [name, value])} />; }

function ProjectPicker(props: { view: ManagementView; value?: string; onChange(project: string): void }) {
    return (
        <select value={props.value ?? ''} aria-label="Project" onChange={event => props.onChange((event.target as HTMLSelectElement).value)}>
            {props.view.projects.map(entry => <option key={entry.summary.project} value={entry.summary.project} title={entry.summary.collection}>{entry.summary.name}</option>)}
        </select>
    );
}

function Paging(props: { view: ManagementView; actions: ReturnType<typeof useActions> }) {
    return (
        <div class="actions">
            <Act actions={props.actions} action="registeredProjects" enabled={props.view.enabled.projectActions} ghost>Registered projects</Act>
            <Act actions={props.actions} action="inactiveProjects" enabled={props.view.enabled.projectActions} ghost>Unregistered projects / cleanup</Act>
            <Act actions={props.actions} action="firstPage" ghost>First page</Act>
            <Act actions={props.actions} action="nextPage" enabled={props.view.morePages} ghost>Next page</Act>
            <span class="muted">{props.view.pageNote}</span>
        </div>
    );
}

export function ProjectsScreen(props: ScreenProps) {
    const view = useManagement(props.state), actions = useActions();
    return (
        <Frame view={view} trusted={props.state.trusted} actions={actions} intro="Registered projects from the service's cached project page.">
            {view && <>
                <div class="actions">
                    <Act actions={actions} action="registerWorkspace" enabled={view.enabled.registerWorkspace}>Review workspace registration</Act>
                    <Act actions={actions} action="refresh" enabled={view.enabled.refresh} ghost>Refresh</Act>
                </div>
                <Paging view={view} actions={actions} />
                {!view.projects.length && <p class="callout">No project is visible on this page.</p>}
                <div class="table-wrap"><table class="grid">
                    <thead><tr><th scope="col"><span class="sr-only">Select for repair</span></th><th scope="col">Project</th><th scope="col">Source root</th>
                        <th scope="col">Watching</th><th scope="col">Paused</th><th scope="col">Publication (cached)</th><th scope="col">Files (cached)</th>
                        <th scope="col">Semantic (cached)</th><th scope="col">Inspected</th><th scope="col"><span class="sr-only">Actions</span></th></tr></thead>
                    <tbody>{view.projects.map(entry => {
                        const project = entry.summary;
                        return (
                            <tr key={project.project} class={project.project === props.state.selectedProject ? 'selected' : ''}>
                                <td><input type="checkbox" aria-label={`Select ${project.collection} for repair`} checked={entry.selected} disabled={view.inactive || !view.enabled.projectActions}
                                    onChange={event => { void actions.act('select', { project: project.project, selected: (event.target as HTMLInputElement).checked }); }} /></td>
                                <th scope="row" title={project.collection}>{project.name}{project.current && <span class="tag">open here</span>}</th>
                                <td><code class="path">{project.root || 'not observed'}</code></td>
                                <td>{observed(project.watching)}</td><td>{observed(project.paused)}</td>
                                <td title={project.publication}>{shortToken(project.publication)}</td><td>{observed(project.indexedFiles)}</td>
                                <td>{project.semantic ?? 'not observed'}</td><td>{ago(project.observedAt)}</td>
                                <td class="actions">{!view.inactive && <>
                                    {project.project !== props.state.selectedProject && <button type="button" class="ghost" onClick={() => { void request('project.select', { project: project.project }).catch(() => undefined); }}>Select</button>}
                                    <Act actions={actions} action="index" fields={{ project: project.project }} enabled={view.enabled.projectActions}>Index now</Act>
                                </>}</td>
                            </tr>
                        );
                    })}</tbody>
                </table></div>
                <p class="muted">{view.selectedCount} selected across pages for repair. Cached values update after inspect or configuration jobs.</p>
            </>}
        </Frame>
    );
}

export function ActivityScreen(props: ScreenProps) {
    const view = useManagement(props.state), actions = useActions();
    return (
        <Frame view={view} trusted={props.state.trusted} actions={actions} intro="Current and queued native work. Ages are real advancement observations; there is no ETA.">
            {view && <>
                <Paging view={view} actions={actions} />
                {view.projects.map(entry => (
                    <article class="card" key={entry.summary.project}>
                        <h2 title={entry.summary.collection}>{entry.summary.name}</h2>
                        <Rows rows={entry.activity} />
                        <div class="actions">
                            {entry.jobId && (entry.jobState === 'failed' || entry.jobState === 'canceled') && <Act actions={actions} action="retryJob" fields={{ id: entry.jobId }}>Retry job</Act>}
                            {entry.jobId && (entry.jobState === 'running' || entry.jobState === 'pending') && <Act actions={actions} action="cancelJob" fields={{ id: entry.jobId }} ghost>Cancel job</Act>}
                            {entry.pendingJobId && <Act actions={actions} action="cancelJob" fields={{ id: entry.pendingJobId }} ghost>Cancel queued job</Act>}
                        </div>
                    </article>
                ))}
                <h2>Recent transitions</h2>
                {view.historyOmitted && <p class="muted">Older transitions are omitted from this recent-history view.</p>}
                <ol class="log">{view.events.map((line, index) => <li key={index}><code>{line}</code></li>)}</ol>
                <h2>Recent process transitions</h2>
                <ol class="log">{view.processEvents.map((line, index) => <li key={index}><code>{line}</code></li>)}</ol>
            </>}
        </Frame>
    );
}

const OPERATIONS: [NonNullable<ActionFields['operation']>, string][] = [['auto', 'Automatic (inspect, then choose)'], ['verify', 'Verify retained data'],
    ['recover', 'Recover'], ['reconcile', 'Reconcile with sources'], ['vectors', 'Regenerate vectors'], ['rebuild', 'Full rebuild']];

export function RepairScreen(props: ScreenProps) {
    const view = useManagement(props.state), actions = useActions();
    const [operation, setOperation] = useState<NonNullable<ActionFields['operation']>>('auto');
    const [scope, setScope] = useState<'one' | 'selected' | 'all'>('one');
    const [project, setProject] = useState<string | undefined>(props.state.selectedProject);
    const [fallback, setFallback] = useState(false);
    const plan = view?.plan;
    return (
        <Frame view={view} trusted={props.state.trusted} actions={actions} intro="Repair plans inspect state before choosing actions. Nothing runs until you approve the exact preview.">
            {view && <>
                <div class="inline-form">
                    <label>Operation <select value={operation} onChange={event => setOperation((event.target as HTMLSelectElement).value as typeof operation)}>
                        {OPERATIONS.map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
                    <label>Scope <select value={scope} onChange={event => setScope((event.target as HTMLSelectElement).value as typeof scope)}>
                        <option value="one">One project</option><option value="selected">Selected projects ({view.selectedCount})</option><option value="all">All registered projects</option></select></label>
                    {scope === 'one' && <ProjectPicker view={view} value={project} onChange={setProject} />}
                    <Act actions={actions} action="repair" enabled={view.enabled.projectActions && !view.inactive && (scope !== 'one' || !!project) && (scope !== 'selected' || view.selectedCount > 0)}
                        fields={{ scope, operation, ...(scope === 'one' && project ? { project } : {}) }}>Create repair preview</Act>
                </div>
                {!plan && <p class="muted">Create a plan for one project, the selected projects, or all registered projects.</p>}
                {plan && <article class="card">
                    <h2>Repair preview and receipts</h2>
                    <Rows rows={plan.rows} />
                    <div class="table-wrap"><table class="grid">
                        <thead><tr><th>Project</th><th>Phase</th><th>Action</th><th>Availability impact</th><th>Temporary disk estimate (bytes)</th><th>Source</th><th>Fallback approval required</th><th>Result</th><th>Cleanup pending</th></tr></thead>
                        <tbody>{plan.items.map((item, index) => <tr key={index}><td><code>{item.project}</code></td><td>{item.phase}</td><td>{item.action}</td><td>{item.availability}</td>
                            <td>{item.estimate}</td><td>{item.source}</td><td>{item.fallback}</td><td>{item.result}</td><td>{item.cleanup}</td></tr>)}</tbody>
                    </table></div>
                    <p class="muted">Plan rows are cached observations. Read every preview page before approving; disk estimates are observations and native admission reserves space before work.</p>
                    <div class="actions">
                        <Act actions={actions} action="morePlan" enabled={plan.morePages && view.enabled.projectActions} ghost>More plan projects</Act>
                        <Act actions={actions} action="refreshPlan" enabled={view.enabled.projectActions} ghost>Refresh cached plan rows</Act>
                        <label class="check"><input type="checkbox" checked={fallback} onChange={event => setFallback((event.target as HTMLInputElement).checked)} /> Approve any explicitly identified fresh/offline fallback in this exact plan</label>
                        <Act actions={actions} action="approvePlan" fields={{ allowFallback: fallback }} enabled={plan.approvable && view.enabled.projectActions}>Approve this exact preview</Act>
                        <Act actions={actions} action={plan.paused ? 'resumePlan' : 'pausePlan'} enabled={view.enabled.projectActions} ghost>{plan.paused ? 'Resume plan' : 'Pause plan'}</Act>
                        <Act actions={actions} action="cancelPlan" enabled={view.enabled.projectActions} ghost>Cancel plan</Act>
                    </div>
                    {!plan.approvable && <p class="muted">Approval becomes available when the plan is ready and every preview page has been read.</p>}
                </article>}
                <h2>Requests awaiting confirmation</h2>
                <p class="muted">An unknown outcome keeps its original request key. Rechecking sends that exact request; forgetting it does not cancel native work.</p>
                {!view.pending.length && <p class="muted">No saved request is awaiting confirmation.</p>}
                <ul class="hits">{view.pending.map(intent => (
                    <li key={intent.id} class="hit card">{intent.title} · {intent.unknown ? 'Outcome unknown' : 'Prepared'}
                        <div class="actions"><Act actions={actions} action="retryIntent" fields={{ id: intent.id }}>Check / retry exact request</Act>
                            {intent.lifecycle && intent.forceAvailable && <Act actions={actions} action="forceIntent" fields={{ id: intent.id }} ghost>Approve owned manager force stop</Act>}
                            <Act actions={actions} action="forgetIntent" fields={{ id: intent.id }} ghost>Forget local receipt request</Act></div></li>
                ))}</ul>
            </>}
        </Frame>
    );
}

function ProjectCards(props: { view: ManagementView; actions: ReturnType<typeof useActions>; children(entry: ManagedProject): ComponentChildren; showRows?: boolean }) {
    return <>{props.view.projects.map(entry => (
        <article class="card" key={entry.summary.project}>
            <h2 title={entry.summary.collection}>{entry.summary.name} <span class="muted">{entry.summary.root}</span></h2>
            {props.showRows && <Rows rows={entry.rows} />}
            {entry.uncertainConfiguration && <p class="callout">The configuration effect is uncertain. Inspect the current settings before requesting another change.</p>}
            <div class="actions">{props.children(entry)}</div>
        </article>
    ))}</>;
}

export function IndexingScreen(props: ScreenProps) {
    const view = useManagement(props.state), actions = useActions();
    return (
        <Frame view={view} trusted={props.state.trusted} actions={actions} intro="Per-project indexing controls. Project settings open the native settings picker (vectors, detail, models, extraction policy, scan limits and more).">
            {view && <>
                <div class="actions"><button type="button" class="ghost" onClick={() => { void actions.command('airk.indexMode'); }}>Choose default automatic watching…</button></div>
                <Paging view={view} actions={actions} />
                {view.inactive ? <p class="callout">Unregistered projects are listed in Storage.</p> : <ProjectCards view={view} actions={actions} showRows>{entry => {
                    const project = entry.summary.project, on = view.enabled.projectActions;
                    return <>
                        <Act actions={actions} action="inspect" fields={{ project }} enabled={on}>Inspect</Act>
                        <Act actions={actions} action="index" fields={{ project }} enabled={on}>Index now</Act>
                        <Act actions={actions} action={entry.summary.paused ? 'resume' : 'pause'} fields={{ project }} enabled={on} ghost>{entry.summary.paused ? 'Resume project' : 'Pause project'}</Act>
                        <Act actions={actions} action="watch" fields={{ project, selected: !entry.summary.watching }} enabled={on} ghost>{entry.summary.watching ? 'Stop automatic watching' : 'Watch automatically'}</Act>
                        <Act actions={actions} action="restartProject" fields={{ project }} enabled={on} ghost>Restart project worker</Act>
                        <Act actions={actions} action="modelSetup" fields={{ project }} enabled={on} ghost>Install bundled model</Act>
                        <Act actions={actions} action="configureProject" fields={{ project }} enabled={on}>Project settings…</Act>
                        {entry.configurationResult && <Act actions={actions} action="copyConfiguration" fields={{ project }} ghost>Copy configuration result</Act>}
                    </>;
                }}</ProjectCards>}
            </>}
        </Frame>
    );
}

export function StorageScreen(props: ScreenProps) {
    const view = useManagement(props.state), actions = useActions();
    return (
        <Frame view={view} trusted={props.state.trusted} actions={actions} intro="Storage location, project registration and reviewed cleanup. Sources and saved settings are always preserved.">
            {view && <>
                <div class="actions">
                    <button type="button" onClick={() => { void actions.command('airk.chooseStorage'); }}>Choose index storage folder…</button>
                    <Act actions={actions} action="inactiveProjects" enabled={view.enabled.projectActions} ghost>Show unregistered projects</Act>
                    <Act actions={actions} action="registeredProjects" enabled={view.enabled.projectActions} ghost>Show registered projects</Act>
                </div>
                <p class="muted">Quotas, extra source roots and unregistering a project are in each project's settings (Indexing › Project settings…).</p>
                {view.inactive && <ProjectCards view={view} actions={actions}>{entry => <>
                    <Act actions={actions} action="previewCleanup" fields={{ project: entry.summary.project }} enabled={view.enabled.projectActions}>Preview / resume index cleanup</Act>
                    <Act actions={actions} action="deleteDerived" fields={{ project: entry.summary.project }} enabled={view.enabled.projectActions && entry.canDeleteDerived}>Delete reviewed index state</Act>
                    {entry.configurationResult && <Act actions={actions} action="copyConfiguration" fields={{ project: entry.summary.project }} ghost>Copy cleanup preview</Act>}
                </>}</ProjectCards>}
                {view.inactive && <p class="muted">Unregistered projects keep sources and saved settings. Deletion needs a separate exact preview and confirmation.</p>}
                {view.legacy && <article class="card">
                    <h2>Legacy 1.3 startup review</h2>
                    <p class="muted">Review the old executable path, project and startup ownership separately from the 1.4 service. Review reads metadata only and never runs the old command.</p>
                    <Rows rows={view.legacy.rows} />
                    <div class="actions">
                        <Act actions={actions} action="reviewLegacyStartup">Review old startup paths</Act>
                        <Act actions={actions} action="legacyStartupReceipts" ghost>Saved legacy removal receipts</Act>
                        {view.legacy.reviewed && <Act actions={actions} action="removeLegacyStartup" enabled={view.legacy.canRemove}>Remove reviewed old startup</Act>}
                        {view.legacy.reviewed && <Act actions={actions} action="copyLegacyStartup" ghost>Copy this legacy review</Act>}
                    </div>
                    <ul>{view.legacy.receipts.map(receipt => <li key={receipt.id}>{receipt.service} · {receipt.phase}
                        <Act actions={actions} action="resumeLegacyStartup" fields={{ id: receipt.id }} ghost>{receipt.phase === 'complete' ? 'Show completed receipt' : 'Review / resume saved removal'}</Act></li>)}</ul>
                </article>}
            </>}
        </Frame>
    );
}

export function ServiceScreen(props: ScreenProps) {
    const view = useManagement(props.state), actions = useActions();
    const on = (name: string) => view?.enabled[name] === true;
    return (
        <Frame view={view} trusted={props.state.trusted} actions={actions} intro="The per-user native service on">
            {view && <>
                <div class="actions">
                    <Act actions={actions} action="connect" enabled={on('connect')}>Connect / reconnect</Act>
                    <Act actions={actions} action="start" enabled={on('start')} ghost>Start service</Act>
                    <Act actions={actions} action="restart" enabled={on('restart')} ghost>Restart service</Act>
                    <Act actions={actions} action="stop" enabled={on('stop')} ghost>Stop service</Act>
                    <Act actions={actions} action="resources" enabled={on('resources')} ghost>Resource settings…</Act>
                </div>
                {view.lifecycle === 'external' && <p class="muted">Startup, stop, restart, upgrades and rollback are controlled by the external service manager.</p>}
                <div class="cards two">
                    <article class="card"><h2>Service</h2><Rows rows={view.service} />
                        <p class="muted">Health confirms responsiveness. It does not prove indexing progress, source freshness or complete semantic coverage.</p></article>
                    <article class="card"><h2>Runtime</h2><Rows rows={view.runtime} />
                        <div class="actions">
                            <Act actions={actions} action="upgradeRuntime" enabled={on('upgradeRuntime')} ghost>Review bundled runtime upgrade</Act>
                            <Act actions={actions} action="rollbackRuntime" enabled={on('rollbackRuntime')} ghost>Review previous runtime</Act>
                            <Act actions={actions} action="repairRuntime" enabled={on('repairRuntime')} ghost>Review runtime payload repair</Act>
                            <Act actions={actions} action="resumeActivation" enabled={on('resumeActivation')} ghost>Review saved runtime activation</Act>
                            <Act actions={actions} action="recoverLifecycle" enabled={on('recoverLifecycle')} ghost>Resume saved lifecycle request</Act>
                            <Act actions={actions} action="abandonLifecycle" enabled={on('abandonLifecycle')} ghost>Abandon saved lifecycle request</Act>
                        </div></article>
                    <article class="card"><h2>Resources</h2><Rows rows={view.resources} /></article>
                    <article class="card"><h2>Inference helper</h2><Rows rows={view.inference} />
                        <div class="actions">
                            <Act actions={actions} action="startEmbedding" enabled={on('startEmbedding')} ghost>Start inference helper</Act>
                            <Act actions={actions} action="stopEmbedding" enabled={on('stopEmbedding')} ghost>Stop this inference helper</Act>
                            <Act actions={actions} action="forceEmbedding" enabled={on('forceEmbedding')} ghost>Force-stop owned inference helper</Act>
                        </div></article>
                </div>
            </>}
        </Frame>
    );
}

export function AgentsScreen(props: ScreenProps) {
    const view = useManagement(props.state), actions = useActions();
    return (
        <Frame view={view} trusted={props.state.trusted} actions={actions} intro="Connect coding agents to AIRK Retrieval (cited evidence) on">
            {view && <>
                <div class="cards two">
                    <article class="card"><h2>AIRK Retrieval</h2>
                        <p>The 16 read-only <code>airk_*</code> tools, scoped to one project. Agents use them for cited evidence.</p>
                        <div class="actions">
                            <button type="button" onClick={() => { void actions.command('airk.setupAgent'); }}>Connect a coding agent…</button>
                            <button type="button" class="ghost" onClick={() => { void actions.command('airk.verifyMcp'); }}>Check the AIRK MCP helper</button>
                            <button type="button" class="ghost" onClick={() => { void actions.command('airk.manageVscodeMcp'); }}>VS Code / Copilot MCP controls</button>
                        </div>
                        <p class="muted">Helper verification proves the launcher starts and lists tools. Whether an agent loaded it is checked in that agent.</p></article>
                    <article class="card"><h2>Responses and terminal</h2>
                        <p>MCP response mode: <strong>{view.responseMode}</strong>. Full adds provenance, readiness and coverage; agents can request it per call.</p>
                        <div class="actions">
                            <Act actions={actions} action="responseMode" enabled={view.enabled.responseMode} ghost>Choose terse / full responses</Act>
                            <button type="button" class="ghost" onClick={() => { void actions.command('airk.installCli'); }}>Install or repair CLI</button>
                            <button type="button" class="ghost" onClick={() => { void actions.command('airk.openTools'); }}>Open AIRK Tools terminal</button>
                        </div></article>
                </div>
            </>}
        </Frame>
    );
}

export function UsageScreen(props: ScreenProps) {
    const view = useManagement(props.state), actions = useActions();
    return (
        <Frame view={view} trusted={props.state.trusted} actions={actions} intro="Local usage records on">
            {view && <article class="card">
                <h2>Usage & Savings</h2>
                <p>The usage report covers the selected project for 7 or 30 days: recorded calls by client (including the AIRK Explorer's own searches), operations and estimated context comparisons. Recording mode, feedback, import, clearing and calibration are in the report and in Indexing › Project settings.</p>
                <p class="muted">Estimates describe counterfactual context, not provider tokens or billing.</p>
                <button type="button" onClick={() => { void actions.command('airk.usage'); }}>Open the Usage & Savings report</button>
            </article>}
        </Frame>
    );
}

export function LogsScreen(props: ScreenProps) {
    const view = useManagement(props.state), actions = useActions();
    const [paths, setPaths] = useState(false);
    return (
        <Frame view={view} trusted={props.state.trusted} actions={actions} intro="Bounded native logs and a metadata-only diagnostic report from">
            {view && <>
                <div class="actions">
                    <Act actions={actions} action="logs" ghost>Open service logs</Act>
                    <button type="button" class="ghost" onClick={() => { void actions.command('airk.logs'); }}>Show extension output</button>
                </div>
                <article class="card">
                    <h2>Diagnostic preview</h2>
                    <p class="muted">Reports contain metadata only and are never uploaded. Paths and user names are redacted unless you include them. Copy keeps this exact captured preview.</p>
                    <label class="check"><input type="checkbox" checked={paths} onChange={event => setPaths((event.target as HTMLInputElement).checked)} /> Include paths and user/machine names</label>
                    <div class="actions">
                        <Act actions={actions} action="previewReport" fields={{ includePaths: paths }}>Preview report</Act>
                        <Act actions={actions} action="copyReport" enabled={!!view.preview} ghost>Copy this preview</Act>
                    </div>
                    {view.preview && <pre class="excerpt" tabIndex={0}>{view.preview}</pre>}
                </article>
            </>}
        </Frame>
    );
}
