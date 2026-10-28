// SPDX-License-Identifier: Apache-2.0
import { ComponentChildren } from 'preact';
import { useState } from 'preact/hooks';
import { FactRow, MANAGE_ACTIONS, MANAGE_COMMANDS, ManagedProject, ManagementView } from '../explorerContracts';
import { request } from './api';
import { CopyDetails, Facts } from './components';
import type { Route } from '../explorerRoutes';
import { ago, byteSize, duration, observed, shortToken } from './format';
import type { ScreenProps } from './workspaces';
import { UsageReport } from './usage';
import { semanticExplanation } from '../explorerStatus';

type Action = typeof MANAGE_ACTIONS[number];
type Command = typeof MANAGE_COMMANDS[number];
interface ActionFields { project?: string; id?: string; scope?: 'one' | 'selected' | 'all'; operation?: 'auto' | 'verify' | 'recover' | 'reconcile' | 'vectors' | 'rebuild'; selected?: boolean; includePaths?: boolean; allowFallback?: boolean; expectedPlan?:string; expectedPreview?:string }

/** App owns the single observation subscription, including nested project views. */
function useManagement(state: ScreenProps['state']): ManagementView | undefined { return state.management; }

export function useActions() {
    const [busy, setBusy] = useState<string>();
    const [outcome, setOutcome] = useState<string>();
    const act = async (action: Action, fields: ActionFields = {}, label = action) => {
        setBusy(label); setOutcome(undefined);
        try {
            const result = await request<{ message: string | null }>('manage.action', { action, ...fields });
            setOutcome(result.message ? result.message : undefined);
        } catch (failure) { setOutcome(failure instanceof Error ? failure.message : String(failure)); }
        finally { setBusy(undefined); }
    };
    const command = async (name: Command) => {
        try { await request('manage.command', { command: name }); } catch (failure) { setOutcome(failure instanceof Error ? failure.message : String(failure)); }
    };
    return { busy, outcome, act, command };
}

export function Act(props: { actions: ReturnType<typeof useActions>; action: Action; fields?: ActionFields; enabled?: boolean; ghost?: boolean; children: ComponentChildren }) {
    const disabled = props.enabled === false || props.actions.busy !== undefined;
    return <button type="button" class={props.ghost ? 'ghost' : ''} disabled={disabled} onClick={() => { void props.actions.act(props.action, props.fields); }}>{props.actions.busy===props.action?props.action==='refresh'?'Refreshing status…':'Working…':props.children}</button>;
}

function Frame(props: { view?: ManagementView; trusted: boolean; actions: ReturnType<typeof useActions>; intro: string; children: ComponentChildren }) {
    if (!props.trusted || props.view?.trusted === false) { return <p class="callout">Trust this workspace before AIRK connects or accepts management requests.</p>; }
    if (!props.view) { return <p class="muted">Reading the AIRK service state…</p>; }
    return (
        <div class="manage">
            <p class="muted">{props.intro} {props.view.authority} · {props.view.connected ? 'connected' : props.view.busy?'connecting':'disconnected'}{props.view.busy&&props.view.connected ? ' · checking status' : ''} · Last service check: {ago(props.view.checkedAt)}</p>
            {props.actions.outcome && <p class="callout" role="status">{props.actions.outcome}</p>}
            {props.view.message && !props.actions.outcome && <p class="muted" role="status">Last control outcome: <code>{props.view.message}</code></p>}
            {props.children}
        </div>
    );
}

function Rows(props: { rows: readonly FactRow[] }) { return <Facts rows={props.rows.map(([name, value]) => [name, value])} />; }

function RuntimeVersions(props: { state: ScreenProps['state']; view: ManagementView }) {
    return <section class="card runtime-versions" aria-label="AIRK executable versions">
        <h2>AIRK executable versions</h2>
        <Rows rows={[
            ['VS Code extension', props.state.extensionVersion], ...props.view.runtimeVersions,
            ...(props.state.status?.runtimeVersions ?? [['MCP frontend (airk)', 'not observed'], ['Index worker (airk)', 'not observed']] as FactRow[]),
        ]} />
        <p class="muted">CLI and helper versions describe the selected installation. Process observations describe the service and the selected project’s MCP connection and worker; they may differ during an upgrade. {props.state.status ? `Project runtime observed ${ago(props.state.status.observedAt)}.` : 'Connect to the selected project to observe its processes.'} {!props.view.connected && 'Disconnected: observations may be stale.'}</p>
        {props.state.status?.versionsMatch === false && <p class="callout" role="alert">The observed runtime versions do not match. Review Service before relying on the new version’s behavior.</p>}
    </section>;
}

function WatchDiagnostics(props: { entry: ManagedProject }) {
    return <details><summary>Unreadable directories</summary>
        {props.entry.watchDiagnostics ? <Rows rows={props.entry.watchDiagnostics} /> : <p class="muted">Traversal diagnostics have not been observed.</p>}
        <p class="muted">The watcher retries its directory plan about every 10 seconds. Each root walk stops at its first failure, so this is an observed list, not a complete inventory. Entries clear after that root is fully traversed. Logs record unavailable/recovered transitions without repeating the same failure on every retry. Indexing completion is separate.</p>
    </details>;
}

function ProjectPicker(props: { view: ManagementView; value?: string; onChange(project: string): void }) {
    return (
        <select value={props.value ?? ''} aria-label="Project" onChange={event => props.onChange((event.target as HTMLSelectElement).value)}>
            {props.view.projects.map(entry => <option key={entry.summary.project} value={entry.summary.project} title={entry.summary.collection}>{entry.summary.name}</option>)}
        </select>
    );
}

function Paging(props: { view: ManagementView; actions: ReturnType<typeof useActions> }) {
    return <div class="actions project-paging">
        <div class="project-filters" role="group" aria-label="Project list filter">
            <button type="button" class={!props.view.inactive ? 'selected' : 'ghost'} aria-pressed={!props.view.inactive}
                disabled={!props.view.enabled.projectActions || props.actions.busy !== undefined || !props.view.inactive}
                onClick={() => void props.actions.act('registeredProjects')}>Active projects</button>
            <button type="button" class={props.view.inactive ? 'selected' : 'ghost'} aria-pressed={props.view.inactive}
                disabled={!props.view.enabled.projectActions || props.actions.busy !== undefined || props.view.inactive}
                onClick={() => void props.actions.act('inactiveProjects')}>Removed projects</button>
        </div>
        <Act actions={props.actions} action="firstPage" enabled={props.view.enabled.refresh} ghost>First page</Act>
        <Act actions={props.actions} action="nextPage" enabled={props.view.morePages && props.view.enabled.refresh} ghost>Next page</Act>
        <span class="muted">{props.view.pageNote}</span>
    </div>;
}

export function ProjectHealth(props: { entry: ManagedProject; view: ManagementView; actions: ReturnType<typeof useActions>; go?(route:Route):void }) {
    const { entry, view, actions } = props, health = entry.health;
    return <section class={`project-health health-${health.severity}`} aria-label={`${entry.summary.name} health`}>
        <div class="section-head"><h3>{health.title}</h3><CopyDetails section="health" project={entry.summary.project} /></div><p>{health.detail}</p>
        {health.reviewTab&&props.go&&<button type="button" class="ghost" onClick={()=>props.go!({workspace:'manage',tab:health.reviewTab})}>{health.reviewTab==='repair'?'Review recovery plan':'Review diagnostics'}</button>}
        {health.action && <div class="actions"><Act actions={actions} action={health.action}
            fields={health.action === 'retryJob' ? { id: entry.jobId } : { project: entry.summary.project }}
            enabled={view.enabled.projectActions && !view.inactive && (health.action !== 'retryJob' || !!entry.jobId)}>{health.actionLabel}</Act></div>}
        {health.action === 'restartProject' && <p class="muted">After restart, confirm that a worker connects and new progress is observed. The original failure cause may be unavailable.</p>}
    </section>;
}

/** Shared dashboard for one exact native attempt. Previous-worker counters remain labeled. */
export function WorkProgress(props: { entry: ManagedProject }) {
    const work = props.entry.work;
    const phases: Record<string, string> = { starting: 'Starting worker', discovering: 'Discovering source files', extracting: 'Extracting source files',
        preparing: 'Preparing index changes', publishing: 'Publishing index changes', embedding: 'Building semantic index', verifying: 'Verifying retained data',
        recovering: 'Recovering captured work', retiring: 'Cleaning up owned index state', finished: 'Work finished' };
    const sample=<section class="card work-progress" aria-label="Indexing progress">
        <div class="section-head"><h2>Indexing progress</h2><span class="muted">Sample observed {ago(work.observedAt)}</span><CopyDetails section="progress" project={props.entry.summary.project} /></div>
        <h3>{props.entry.summary.jobKind === 'model_setup' && props.entry.jobState === 'running' ? 'Preparing offline model' : work.phase ? phases[work.phase] ?? work.phase : 'Progress not observed'}</h3>
        <p class="muted">{work.current ? 'Worker connected · matching job sample' : 'Retained sample · current progress unconfirmed'}.
            {' '}Last advancement: {duration(work.lastAdvanceMs)} ago at sample · Elapsed: {duration(work.elapsedMs)}.</p>
        <div class="progress-stages" aria-label="Indexing stages">{[['discovering','Discover'],['extracting','Extract'],['publishing','Publish'],['embedding','Semantic']].map(([phase,label]) => <span key={phase} class={work.current && work.phase === phase ? 'active' : ''}>{label}</span>)}</div>
        {work.wait !== 'No matching native wait observation' && <p class="callout">{work.wait}</p>}
        <dl class="project-metrics progress-metrics">
            <div><dt>Captured files</dt><dd>{observed(work.captured)}</dd></div>
            <div><dt>Newly extracted</dt><dd>{observed(work.extracted)}</dd></div>
            <div><dt>Index transactions published</dt><dd>{observed(work.publications)}</dd></div>
            <div><dt>Total eligible files</dt><dd>{work.total === undefined ? 'Unknown' : observed(work.total)}</dd></div>
        </dl>
        <details><summary>More progress details</summary>
            <p class="muted">Captured includes reused results. Newly extracted counts new payloads, not a completion fraction. Counters belong to this attempt. Stages can repeat by batch; semantic work follows source publication. No overall percentage or ETA is inferred.</p>
            <Facts rows={[
                ['Current phase at sample', duration(work.phaseElapsedMs)],
                ['Prepared / committed work items', `${observed(work.prepared)} / ${observed(work.committed)}`],
                ['Verified retained objects', observed(work.verified)], ['Completed semantic units', observed(work.semanticUnits)],
            ]} />
            <details><summary>Native progress observations</summary><Rows rows={props.entry.activity} /><Rows rows={work.phaseTimes} /></details>
        </details>
    </section>;
    return props.entry.pendingJobId&&!['running','pending'].includes(props.entry.jobState??'')?<section class="card work-progress" aria-label="Queued indexing request">
        <div class="section-head"><h2>Waiting for the queued request to start</h2><CopyDetails section="progress" project={props.entry.summary.project} /></div>
        <p>No progress for this queued request has been observed. The previous job's counters do not describe the queued work.</p>
        {work.wait!=='No matching native wait observation'&&<p class="callout">{work.wait}</p>}
        <details><summary>Previous {props.entry.summary.jobKind??'job'} result and counters</summary>{sample}</details>
    </section>:sample;
}

/** Each Management tab mounts only its section; project selection survives tab changes. */
export function ProjectsScreen(props: ScreenProps) {
    const view = useManagement(props.state), actions = useActions();
    const [opened, setOpened] = useState<string | undefined>(props.state.selectedProject);
    const entry = view?.projects.find(item => item.summary.project === opened) ?? view?.projects[0];
    const scoped = entry && view ? { ...props, manageEmbedded: true, state: { ...props.state, selectedProject: entry.summary.project, management: { ...view, projects: [entry] } } } : undefined;
    const tab = props.route.tab ?? 'overview';
    if (tab === 'service') return <><ServiceScreen {...props} />{view && props.state.trusted && <RuntimeVersions state={props.state} view={view} />}</>;
    if (tab === 'agents') return <AgentsScreen {...props} />;
    if (tab === 'usage') return <UsageScreen {...props} />;
    if (tab === 'logs') return <LogsScreen {...props} />;
    return <Frame view={view} trusted={props.state.trusted} actions={actions} intro="Manage indexing on">
        {view && <>
            <div class="section-head"><div class="inline-form"><label>Project <ProjectPicker view={view} value={entry?.summary.project} onChange={setOpened} /></label>
                <Act actions={actions} action="registerWorkspace" enabled={view.enabled.registerWorkspace} ghost>Add project…</Act></div>
                <span class="muted">Updates automatically</span></div>
            <details><summary>Browse active or removed projects</summary><Paging view={view} actions={actions} /></details>
            {!view.connected && !view.busy && <p class="callout">The service connection is unavailable. Automatic checks will retry; view connection details in the Service tab.</p>}
            {view.pending.length > 0 && <section class="project-health health-warn" role="status"><h3>Requests need confirmation</h3>
                <p>A connection ended before an outcome was confirmed. Check the saved request before submitting another.</p>
                {view.pending.map(intent => <div class="actions" key={intent.id}><span>{intent.title}</span><Act actions={actions} action="retryIntent" fields={{ id: intent.id }} enabled={view.enabled.projectActions}>Check exact request</Act></div>)}
            </section>}
            {!entry && <p class="callout" role="status">{!view.connected?view.busy?'Connecting to AIRK and loading saved project details…':'Project registrations could not be checked. Retained work has not been reported as removed.':`No ${view.inactive?'removed':'active'} projects on this page. Add project reviews an open folder.`}</p>}
            {entry && scoped && <>
                <h2>{entry.summary.name}</h2><p class="path">{entry.summary.root}</p>
                <ProjectHealth entry={entry} view={view} actions={actions} go={props.go} />
                {tab === 'overview' && !view.inactive && !entry.health.action && <div class="actions">
                    <Act actions={actions} action={entry.jobState === 'running' || entry.jobState === 'pending' || entry.pendingJobId ? 'pause' : 'index'} fields={{ project: entry.summary.project }} enabled={view.enabled.projectActions}>
                        {entry.jobState === 'running' || entry.jobState === 'pending' || entry.pendingJobId ? 'Pause indexing' : 'Update index'}</Act>
                </div>}
                {view.inactive && <p class="callout">{entry.lifecycle}. Removal stops indexing and retains settings and index data. Add project reviews registration again; pending cleanup must finish first.</p>}
                {tab === 'overview' && <><section class="card"><h3>Available for search</h3>
                    <p>Text index: <strong>{entry.summary.indexState ?? 'not checked'}</strong> · {observed(entry.summary.indexedFiles)} published files · observed {ago(entry.summary.observedAt)}</p>
                    <p>Semantic coverage: <strong>{semanticExplanation({ semantic: entry.summary.semantic ?? 'unknown', vectorsEnabled: entry.configuration.vectorsEnabled }).title}</strong></p>
                    <p class="muted">{semanticExplanation({ semantic: entry.summary.semantic ?? 'unknown', vectorsEnabled: entry.configuration.vectorsEnabled }).detail}</p>
                    <p class="muted">Published counts describe saved evidence, separately from new work. They do not prove all saved files are up to date.</p>
                </section>
                {!view.inactive && <>
                    {(entry.jobId || entry.pendingJobId || entry.configuration.pending) && <WorkProgress entry={entry} />}
                    <ProjectSettings entry={entry} view={view} actions={actions} />
                </>}
                </>}
                {tab === 'activity' && <section class="card" aria-label="Recent activity and requests"><div class="section-head"><h3>Recent activity and requests</h3><CopyDetails section="activity" project={entry.summary.project} /></div>
                    {(entry.jobId || entry.pendingJobId || entry.configuration.pending) && <WorkProgress entry={entry} />}
                    <p class="muted">Native outcomes survive closing this window. Applied settings and search coverage are separate steps. Recent history is bounded.</p>
                    <Rows rows={entry.requests} />
                    <ol class="log">{entry.events.map((event, index) => <li key={index}>{event}</li>)}</ol>
                    {view.historyOmitted && <p class="muted">Earlier events are omitted.</p>}
                    {entry.jobId && ['running','pending'].includes(entry.jobState ?? '') && <Act actions={actions} action="cancelJob" fields={{ id: entry.jobId }} enabled={view.enabled.projectActions} ghost>Cancel current request</Act>}
                    {entry.pendingJobId && <Act actions={actions} action="cancelJob" fields={{ id: entry.pendingJobId }} enabled={view.enabled.projectActions} ghost>Cancel queued request</Act>}
                </section>}
                {tab === 'storage' && <section aria-label="Storage and removal"><h3>Storage and removal</h3>
                    <StorageScreen {...scoped} />
                    {!view.inactive && <Act actions={actions} action="unregisterProject" fields={{ project: entry.summary.project }} enabled={view.enabled.projectActions} ghost>Remove project from AIRK…</Act>}
                </section>}
                {tab === 'repair' && <section class="card" aria-label="Advanced diagnostics and recovery"><div class="section-head"><h3>Advanced diagnostics and recovery</h3><CopyDetails section="recovery" project={entry.summary.project} /></div>
                    <div class="actions"><Act actions={actions} action="inspect" fields={{ project: entry.summary.project }} enabled={view.enabled.projectActions && !view.inactive} ghost>Check index and settings</Act>
                        <Act actions={actions} action="restartProject" fields={{ project: entry.summary.project }} enabled={view.enabled.projectActions && !view.inactive} ghost>Restart project worker</Act>
                        <Act actions={actions} action="configureProject" fields={{ project: entry.summary.project }} enabled={view.enabled.projectActions && !view.inactive && !entry.configuration.pending} ghost>Advanced settings…</Act>
                        {entry.configurationResult && <Act actions={actions} action="copyConfiguration" fields={{ project: entry.summary.project }} ghost>Copy settings result</Act>}</div>
                    <Rows rows={entry.rows} /><WatchDiagnostics entry={entry} />
                    {!view.inactive && <RepairScreen key={entry.summary.project} {...scoped} />}
                </section>}
            </>}
        </>}
    </Frame>;
}
function ProjectSettings({ entry, view, actions }: { entry: ManagedProject; view: ManagementView; actions: ReturnType<typeof useActions> }) {
    const project = entry.summary.project, enabled = view.enabled.projectActions && !entry.configuration.pending && actions.busy === undefined;
    const semantic = entry.configuration.requestedVectors ?? entry.configuration.vectorsEnabled;
    const detailNames: Record<string,string> = { agent_lean: 'Agent Lean', essential: 'Search Only', balanced: 'Rich Structure', detailed: 'Detailed' };
    return <section class="card project-settings"><h3>Index settings</h3>
        <label>Search mode <select aria-label="Search mode" value={semantic === undefined ? 'unknown' : semantic ? 'semantic' : 'text'} disabled={!enabled}
            onChange={event => void actions.act((event.target as HTMLSelectElement).value === 'semantic' ? 'enableSemantic' : 'disableSemantic', { project })}>
            {semantic === undefined && <option value="unknown" disabled>Not checked</option>}<option value="text">Text only</option><option value="semantic">Text + semantic</option></select></label>
        <p class="muted">Semantic mode prepares the offline model and schedules indexing. The saved mode changes after it applies; search becomes ready after publication.</p>
        {entry.configuration.requestedVectors!==undefined&&<p role="status">Requested: {semantic?'Text + semantic':'Text only'}. Saved mode: {entry.configuration.vectorsEnabled===undefined?'not observed':entry.configuration.vectorsEnabled?'Text + semantic':'Text only'}. AIRK will finish applying the request.</p>}
        {entry.configuration.requestedDetail&&<p role="status">Requested detail: {detailNames[entry.configuration.requestedDetail]??entry.configuration.requestedDetail}.</p>}

        {semantic === true && entry.summary.semantic === 'disabled' && entry.health.action !== 'enableSemantic' && !['running','pending'].includes(entry.jobState ?? '') && !entry.pendingJobId && <Act actions={actions} action="enableSemantic" fields={{ project }} enabled={enabled}>Complete semantic setup</Act>}
        <label>Updates <select aria-label="Updates" value={entry.summary.watching === undefined ? 'unknown' : entry.summary.watching ? 'automatic' : 'manual'} disabled={!view.enabled.projectActions || actions.busy !== undefined}
            onChange={event => void actions.act('watch', { project, selected: (event.target as HTMLSelectElement).value === 'automatic' })}>
            {entry.summary.watching === undefined && <option value="unknown" disabled>Not checked</option>}<option value="automatic">Automatic</option><option value="manual">Manual</option></select></label>
        <p class="muted">Automatic watches saved files even after VS Code closes. Pause temporarily stops work and preserves this preference.</p>
        <div class="inline-form"><span>Evidence detail: {detailNames[entry.configuration.detail ?? ''] ?? entry.configuration.detail ?? 'not checked'}</span><Act actions={actions} action="configureDetail" fields={{ project }} enabled={enabled} ghost>Change detail…</Act></div>
        <p class="muted">Evidence detail controls supported source structure, independently of semantic search.</p>
        {entry.configuration.pending && <p role="status">Settings change {entry.configuration.state}. Current work finishes first; its outcome appears in Recent activity.</p>}
    </section>;
}

const OPERATIONS: [NonNullable<ActionFields['operation']>, string][] = [['auto', 'Automatic (inspect, then choose)'], ['verify', 'Verify retained data'],
    ['recover', 'Recover'], ['reconcile', 'Reconcile with sources'], ['vectors', 'Regenerate vectors'], ['rebuild', 'Full rebuild']];

export function RepairScreen(props: ScreenProps) {
    const view = useManagement(props.state), actions = useActions();
    const [operation, setOperation] = useState<NonNullable<ActionFields['operation']>>('auto');
    const [scope, setScope] = useState<'one' | 'selected' | 'all'>('one');
    const [project, setProject] = useState<string | undefined>(props.state.selectedProject);
    const [fallbackReview, setFallbackReview] = useState<{plan?:string;preview?:string;checked:boolean}>({checked:false});
    const plan = view?.plan;
    const fallback=!!plan?.preview&&fallbackReview.plan===plan.id&&fallbackReview.preview===plan.preview&&fallbackReview.checked;
    const needsFallback=plan?.items.some(item=>item.fallback==='Yes')??false;
    const planTitle=plan?.paused?'Repair plan paused':plan?.phase==='ready'?'Review and start repair':plan?.phase==='planning'?'Checking the project before repair'
        :plan?.phase==='running'?plan.items.some(item=>item.phase==='running')?'Repair running':'Repair queued':plan?.phase==='finished'?'Repair finished':plan?.phase==='canceling'?'Canceling repair':'Repair status';
    return (
        <Frame view={view} trusted={props.state.trusted} actions={actions} intro="Repair plans inspect state before choosing actions. Nothing runs until you approve the exact preview.">
            {view && <>
                <div class="inline-form">
                    <label>Operation <select value={operation} onChange={event => setOperation((event.target as HTMLSelectElement).value as typeof operation)}>
                        {OPERATIONS.map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
                    <label>Scope <select value={scope} onChange={event => setScope((event.target as HTMLSelectElement).value as typeof scope)}>
                        <option value="one">One project</option>{!props.manageEmbedded && <><option value="selected">Selected projects ({view.selectedCount})</option><option value="all">All registered projects</option></>}</select></label>
                    {scope === 'one' && <ProjectPicker view={view} value={project} onChange={setProject} />}
                    <Act actions={actions} action="repair" enabled={view.enabled.projectActions && !view.inactive && (scope !== 'one' || !!project) && (scope !== 'selected' || view.selectedCount > 0)}
                        fields={{ scope, operation, ...(scope === 'one' && project ? { project } : {}) }}>Create repair preview</Act>
                </div>
                {!plan && <p class="muted">Create a plan for one project, the selected projects, or all registered projects.</p>}
                {plan && <article class="card">
                    <div class="section-head"><h2>{planTitle}</h2>{project&&<CopyDetails section="recovery" project={project} />}</div>
                    <p>{plan.phase==='ready'?'The check is complete. Review the proposed work below, then choose Start repair. No repair starts until you confirm.'
                        :plan.phase==='planning'?'AIRK is inspecting the selected project to choose a repair. This is the preview stage.'
                        :plan.phase==='running'?'Your repair was accepted. Project progress and the current plan rows show whether it is queued or running.'
                        :plan.phase==='finished'?'This repair has finished. Review its results below. Published source freshness and semantic coverage are checked separately.'
                        :'The latest native repair state is shown below.'}</p>
                    <ul>{plan.items.map((item,index)=><li key={index}><strong>{view.projects.find(entry=>entry.summary.project===item.project)?.summary.name??`Project ${item.project.slice(0,12)}`}</strong>: {OPERATIONS.find(([value])=>value===item.action)?.[1]??item.action} · {item.phase}.
                        {item.action==='reconcile'&&' Update indexed evidence from saved source files while keeping the published evidence available.'}
                        {item.action==='recover'&&' Recover captured index changes; this can temporarily affect search availability.'}
                        {item.action==='rebuild'&&' Rebuild the index from sources. Review availability, temporary storage and any required fallback approval.'}
                    </li>)}</ul>
                    <div class="actions">
                        {plan.morePages&&<Act actions={actions} action="morePlan" enabled={view.enabled.projectActions}>Review remaining projects</Act>}
                        {plan.phase==='ready'&&needsFallback&&<label class="check"><input type="checkbox" checked={fallback} onChange={event=>setFallbackReview({plan:plan.id,preview:plan.preview,checked:(event.target as HTMLInputElement).checked})} /> Allow the fresh/offline fallback identified in this exact plan</label>}
                        {plan.phase==='ready'&&<Act actions={actions} action="approvePlan" fields={{allowFallback:fallback,expectedPlan:plan.id,expectedPreview:plan.preview}}
                            enabled={plan.approvable&&view.enabled.projectActions&&(!needsFallback||fallback)}>Start repair</Act>}
                        {plan.paused&&plan.phase!=='finished'&&<Act actions={actions} action="resumePlan" enabled={view.enabled.projectActions}>Resume repair</Act>}
                        {!plan.paused&&plan.phase==='running'&&<Act actions={actions} action="pausePlan" enabled={view.enabled.projectActions} ghost>Pause repair</Act>}
                        {plan.phase!=='finished'&&plan.phase!=='canceling'&&<Act actions={actions} action="cancelPlan" enabled={view.enabled.projectActions} ghost>Cancel repair</Act>}
                    </div>
                    {plan.phase==='ready'&&!plan.approvable&&<p class="callout">Review all remaining project pages before starting this repair.</p>}
                    <details><summary>Technical preview, results and storage estimates</summary><Rows rows={plan.rows} />
                    <div class="table-wrap"><table class="grid">
                        <thead><tr><th>Project</th><th>Phase</th><th>Action</th><th>Availability impact</th><th>Temporary disk estimate (bytes)</th><th>Source</th><th>Fallback approval required</th><th>Result</th><th>Cleanup pending</th></tr></thead>
                        <tbody>{plan.items.map((item, index) => <tr key={index}><td><code>{item.project}</code></td><td>{item.phase}</td><td>{item.action}</td><td>{item.availability}</td>
                            <td>{item.estimate}</td><td>{item.source}</td><td>{item.fallback}</td><td>{item.result}</td><td>{item.cleanup}</td></tr>)}</tbody>
                    </table></div>
                    <p class="muted">Plan rows are cached observations. Read every preview page before approving; disk estimates are observations and native admission reserves space before work.</p>
                    <div class="actions">
                        <Act actions={actions} action="morePlan" enabled={plan.morePages && view.enabled.projectActions} ghost>More plan projects</Act>
                        <Act actions={actions} action="refreshPlan" enabled={view.enabled.projectActions} ghost>Refresh cached plan rows</Act>
                    </div>
                    </details>
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

export function StorageScreen(props: ScreenProps) {
    const view = useManagement(props.state), actions = useActions();
    return (
        <Frame view={view} trusted={props.state.trusted} actions={actions} intro="Storage location, project registration and reviewed cleanup. Sources and saved settings are always preserved.">
            {view && <>
                <div class="actions">
                    <Act actions={actions} action="refresh" enabled={view.enabled.refresh} ghost>Refresh</Act>
                    <button type="button" onClick={() => { void actions.command('airk.chooseStorage'); }}>Choose index storage folder…</button>
                </div>
                <p class="muted">Storage below belongs to the listed projects. Choosing a folder affects new registrations; it does not move existing indexes. Sizes are cached native quota accounting, not a scan of the source folder or a promise of free disk space.</p>
                {!props.manageEmbedded && <Paging view={view} actions={actions} />}
                {!view.projects.length && <p class="callout">No projects are visible on this page. Choose Registered projects or refresh the service state.</p>}
                {view.projects.map(entry => <article class="card storage-card" key={entry.summary.project}>
                    <div class="section-head"><h2>{entry.summary.name} {view.inactive && <span class="tag">unregistered · retained index</span>}</h2><CopyDetails section="storage" project={entry.summary.project} /></div>
                    <p class="storage-location">Index storage root <code class="path">{entry.storage.root ?? 'not observed'}</code></p>
                    <p class="muted">Collection <code class="path">{entry.summary.collection}</code> · Source <code class="path">{entry.summary.root || 'not observed'}</code></p>
                    <dl class="project-metrics">
                        <div><dt>Accounted index size</dt><dd>{byteSize(entry.storage.usedBytes)}</dd></div>
                        <div><dt>Reserved</dt><dd>{byteSize(entry.storage.reservedBytes)}</dd></div>
                        <div><dt>Collection quota</dt><dd>{byteSize(entry.storage.limitBytes)}</dd></div>
                        <div><dt>Quota remaining</dt><dd>{byteSize(entry.storage.availableBytes)}</dd></div>
                    </dl>
                    <p class="muted">Inspected {ago(entry.storage.observedAt)}{!view.connected ? ' · disconnected; cached values only' : ''}. {entry.storage.usedBytes === undefined ? view.inactive ? 'No retained storage measurement is available. Review a cleanup preview for the retained index.' : 'No storage measurement is available. Inspect the project to collect one.' : 'Index accounting excludes original source files. Reserved space is committed to admitted work.'}</p>
                    <details><summary>Capacity and index details</summary><Rows rows={entry.storage.rows} />
                        <p class="muted">Root quota remaining can be shared by several projects. Physical growth allowance respects the native free-space reserve. The replacement estimate includes the existing accounted size and a fresh SQLite/WAL allowance; future source growth is unknown. These are observations, not reservations.</p>
                    </details>
                    {!view.inactive && <div class="actions">
                        <Act actions={actions} action="inspect" fields={{ project: entry.summary.project }} enabled={view.enabled.projectActions}>Inspect index &amp; storage</Act>
                        <Act actions={actions} action="configureProject" fields={{ project: entry.summary.project }} enabled={view.enabled.projectActions} ghost>Project settings…</Act>
                    </div>}
                    {view.inactive && <div class="actions">
                        <Act actions={actions} action="previewCleanup" fields={{ project: entry.summary.project }} enabled={view.enabled.projectActions}>Preview / resume index cleanup</Act>
                        <Act actions={actions} action="deleteDerived" fields={{ project: entry.summary.project }} enabled={view.enabled.projectActions && entry.canDeleteDerived}>Delete reviewed index state</Act>
                        {entry.configurationResult && <Act actions={actions} action="copyConfiguration" fields={{ project: entry.summary.project }} ghost>Copy cleanup preview</Act>}
                    </div>}
                </article>)}
                <p class="muted">Remove a project from Storage and removal, even if its source folder is gone. Then preview and delete its retained index here. Quotas and extra roots are in Advanced settings.</p>
                {view.inactive && <p class="muted">Unregistered projects keep sources and saved settings. Deletion needs a separate exact preview and confirmation.</p>}
                {view.legacy && <details class="card legacy-startup">
                    <summary>Legacy 1.3 startup review</summary>
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
                </details>}
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
                    <Act actions={actions} action="refresh" enabled={on('refresh')} ghost>Check status now</Act>
                    <Act actions={actions} action="start" enabled={on('start')} ghost>Start service</Act>
                    <Act actions={actions} action="restart" enabled={on('restart')} ghost>Restart service</Act>
                    <Act actions={actions} action="stop" enabled={on('stop')} ghost>Stop service</Act>
                    <Act actions={actions} action="resources" enabled={on('resources')} ghost>Resource settings…</Act>
                </div>
                {view.lifecycle === 'external' && <p class="muted">Startup, stop, restart, upgrades and rollback are controlled by the external service manager.</p>}
                <div class="cards two">
                    <article class="card"><div class="section-head"><h2>Service</h2><CopyDetails section="service" /></div><Rows rows={view.service} />
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
                        <p>MCP response mode: <strong>{view.responseMode}</strong>.</p>
                        <p><strong>Terse (default)</strong> suits routine code searches and navigation. It keeps focused evidence and citations while reducing repeated metadata, leaving more of the agent’s context budget for the task.</p>
                        <p><strong>Full</strong> adds detailed provenance, readiness and coverage information. Choose it to investigate missing or surprising results, freshness problems or parser limitations, or to audit where evidence came from. Responses are larger; Full does not make the index more complete.</p>
                        <p class="muted">Agents can request Full for an individual call with <code>response_mode: full</code>. Changing this default requires reconnecting the agent’s MCP session.</p>
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
            <p class="muted">Usage follows the project selected in the Explorer header.</p>
            <UsageReport available={props.state.trusted && !!props.state.selectedProject} />
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
                    <Act actions={actions} action="exportLogs" ghost>Save retained service logs…</Act>
                    <button type="button" class="ghost" onClick={() => { void actions.command('airk.showExtensionOutput'); }}>Show extension output</button>
                    <Act actions={actions} action="refresh" enabled={view.enabled.refresh} ghost>Refresh recent events</Act>
                </div>
                <article class="card">
                    <h2>Recent service events</h2>
                    <p class="muted">{view.processLogStatus}{!view.connected ? ' · disconnected; retained observations only' : ''}</p>
                    {view.processEvents.length ? <ol class="log" tabIndex={0} aria-label="Recent service events">{view.processEvents.map((event, index) => <li key={index}><code class="path">{event}</code></li>)}</ol>
                        : <p class="muted">No service events are present in this observation. This does not prove the service has never logged an event.</p>}
                    <p class="muted">Up to 32 recent process events are shown. Open service logs writes the bounded native log page to the AIRK Service output channel; Show extension output opens AIRK’s separate extension output channel.</p>
                </article>
                <article class="card">
                    <h2>Diagnostic report</h2>
                    <p class="muted">Capture the observed project state, failure outcomes and recent job history. Reports contain metadata only and are never uploaded. Paths and user names are redacted unless you include them. Open, Copy and Save use the same captured report; generate again to refresh it. Missing history and size limits are marked in the report. Save retained service logs exports the longer process history separately.</p>
                    <label class="check"><input type="checkbox" checked={paths} onChange={event => setPaths((event.target as HTMLInputElement).checked)} /> Include paths and user/machine names</label>
                    <div class="actions">
                        <Act actions={actions} action="previewReport" fields={{ includePaths: paths }}>Generate report</Act>
                        <Act actions={actions} action="openReport" enabled={!!view.preview} ghost>Open report</Act>
                        <Act actions={actions} action="copyReport" enabled={!!view.preview} ghost>Copy report</Act>
                        <Act actions={actions} action="saveReport" enabled={!!view.preview} ghost>Save report…</Act>
                    </div>
                    {view.preview && <pre class="excerpt" tabIndex={0}>{view.preview}</pre>}
                </article>
            </>}
        </Frame>
    );
}
