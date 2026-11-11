// SPDX-License-Identifier: Apache-2.0
import { Notice, noticeKey } from './notices';
import { ComponentChildren } from 'preact';
import { useEffect, useState } from 'preact/hooks';
import { AGENT_INSTRUCTION_FILES, AIRK_AGENT_INSTRUCTIONS, AgentConfigAlerts, AgentConfigFinding, AgentConfigHealthView, FactRow, MANAGE_ACTIONS, MANAGE_COMMANDS, ManagedProject, ManagementView, ProjectSummary, StatusSummary } from '../explorerContracts';
import { request } from './api';
import { CompleteNativeResponse, CopyDetails, Facts } from './components';
import type { Route } from '../explorerRoutes';
import { ago, byteSize, duration, observed, shortToken } from './format';
import type { ScreenProps } from './workspaces';
import { UsageReport } from './usage';
import { indexContents, semanticExplanation, sourceWorkExplanation, workProgressDisplay } from '../explorerStatus';
import { Markdown } from './help';

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
    if (!props.trusted || props.view?.trusted === false) { return <p class="callout">AIRK client authority is unavailable. Review the window's trust or client notice before making management requests.</p>; }
    if (!props.view) { return <p class="muted">Reading the AIRK service state…</p>; }
    return (
        <div class="manage">
            <p class="muted">{props.intro} {props.view.authority} · {props.view.connected ? 'connected' : props.view.busy?'connecting':'disconnected'}{props.view.busy&&props.view.connected ? ' · checking status' : ''} · Last service check: {ago(props.view.checkedAt)}</p>
            {props.actions.outcome && <Notice noticeKey={noticeKey("control-outcome", props.actions.outcome)}><p>{props.actions.outcome}</p></Notice>}
            {props.view.message && !props.actions.outcome && <Notice noticeKey={noticeKey("control-outcome", props.view.message)}><p>Last control outcome: <code>{props.view.message}</code></p></Notice>}
            {props.children}
        </div>
    );
}

function Rows(props: { rows: readonly FactRow[]; hideUnobserved?: boolean }) { return <Facts rows={props.rows.filter(([, value]) => !props.hideUnobserved || value.toLowerCase() !== 'not observed').map(([name, value]) => [name, value])} />; }

function CleanBreakProgress({value}:{value:Record<string,unknown>}) {
    const object=(input:unknown):Record<string,unknown>=>input&&typeof input==='object'&&!Array.isArray(input)?input as Record<string,unknown>:{};
    const indexes=Array.isArray(value.indexes)?value.indexes.map(object):[];
    const count=(state:string)=>indexes.filter(index=>index.readiness===state).length;
    const cleanup=object(value.cleanup),complete=value.stage==='ready'&&cleanup.complete===true;
    return <article class="card" data-testid="clean-break-progress"><h2>Clean installation and fresh indexes</h2>
        <p>Original source files are unchanged. {value.indexes_complete===true?`${indexes.length} observed indexes of ${String(value.expected_indexes)} total: ${count('ready')} ready, ${count('pending')} pending, ${count('paused')} paused, ${count('failed')} failed, ${count('unavailable')} unavailable.`:`The source roster is being captured: ${String(value.expected_indexes)} expected indexes; ${indexes.length} observed so far.`}</p>
        <p>Legacy cleanup: {cleanup.complete===true?'complete':cleanup.state==='blocked'?'blocked':cleanup.state==='removing'?'removing artifacts':'waiting for fresh indexes'}. {complete?'Connect coding agents with a new reviewed scope.':'The clean installation is not complete until every fresh index is ready and owned legacy artifacts are removed.'}</p>
        {indexes.map((index,position)=>{const original=object(index.original),replacement=object(index.replacement),failure=object(index.failure);return <div class="section" key={String(original.binding??position)}>
            <strong>{String(original.collection??original.root??`Index ${position+1}`)}</strong>
            <Rows rows={[[ 'Source',String(original.root??'Not reported')],['Fresh index',String(replacement.data_root??'Not reported')],['Readiness',String(index.readiness??'unavailable')],['Paused',String(index.paused??'Not reported')],...(typeof index.settings_available==='boolean'?[[ 'Saved settings available',String(index.settings_available)] as FactRow]:[]),...(Array.isArray(index.discarded_preferences)&&index.discarded_preferences.length?[[ 'Discarded preferences',index.discarded_preferences.map(String).join(', ')] as FactRow]:[]),...(typeof failure.code==='string'?[[ 'Failure',`${failure.code}${typeof failure.message==='string'?`: ${failure.message}`:''}`] as FactRow]:[])]} />
        </div>;})}
        <details><summary>Complete cached reset observation</summary><pre>{JSON.stringify(value,null,2)}</pre><p class="muted">Runtime diagnostics JSON copy/open/export retains this complete observation with its native bootstrap result and capture time.</p></details>
    </article>;
}

function RuntimeVersions(props: { state: ScreenProps['state']; view: ManagementView }) {
    return <section class="card runtime-versions" aria-label="AIRK executable versions">
        <h2>AIRK executable versions</h2>
        <Rows rows={[
            ['VS Code extension', props.state.extensionVersion], ...props.view.runtimeVersions,
            ...(props.state.status?.runtimeVersions ?? [['MCP frontend (airk)', 'not observed'], ['Index worker (airk)', 'not observed']] as FactRow[]),
        ]} hideUnobserved />
        <p class="muted">CLI and historical helper versions describe the selected installation. Process observations describe the service and the selected project’s MCP connection and worker; they may differ during an upgrade. {(props.state.status?.runtimeVersions || props.state.status?.versionsMatch !== undefined) ? `Project runtime details observed ${ago(props.state.status.detailsObservedAt ?? props.state.status.observedAt)}.` : 'Connect to the selected project to observe its processes.'} {!props.view.connected && 'Disconnected: observations may be stale.'}</p>
        {props.state.status?.versionsMatch === false && <Notice noticeKey={noticeKey("runtime-versions", props.state.extensionVersion)} role="alert"><p>The observed runtime versions do not match. Review Service before relying on the new version’s behavior.</p></Notice>}
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

export function ProjectHealth(props: { entry: ManagedProject; view: ManagementView; actions: ReturnType<typeof useActions>; compact?:boolean; go?(route:Route):void }) {
    const { entry, view, actions } = props, health = entry.health;
    // Ordinary live work is explained by the adjacent progress card. Keep failures,
    // pauses, recovery and waits prominent instead of repeating a generic running banner.
    if(props.compact&&health.routineWork)return null;
    return <section class={`project-health health-${health.severity}`} aria-label={`${entry.summary.name} health`}>
        <div class="section-head"><h3>{health.title}</h3><CopyDetails section="health" project={entry.summary.project} /></div><p>{health.detail}</p>
        {health.severity==='error'&&<Act actions={actions} action="openRawLogs" ghost>Open raw diagnostic log</Act>}
        {health.reviewTab&&props.go&&<button type="button" class="ghost" onClick={()=>props.go!({workspace:'manage',tab:health.reviewTab})}>{health.reviewTab==='repair'?'Review recovery plan':'Review diagnostics'}</button>}
        {health.action && <div class="actions"><Act actions={actions} action={health.action}
            fields={health.action === 'retryJob' ? { id: entry.jobId } : { project: entry.summary.project }}
            enabled={view.enabled.projectActions && !view.inactive && (health.action !== 'retryJob' || !!entry.jobId)}>{health.actionLabel}</Act></div>}
        {health.action === 'restartProject' && <p class="muted">After restart, confirm that a worker connects and new progress is observed. The original failure cause may be unavailable.</p>}
    </section>;
}

/** The same publication summary leads Overview and Management; work counters never replace it. */
export function IndexContents(props:{project:ProjectSummary;status?:StatusSummary;connected:boolean}) {
    const saved=indexContents(props.project,props.status);
    const coverageKnown=saved.semanticCompleted!==undefined&&saved.semanticSelected!==undefined&&saved.semanticSelected>0&&saved.semanticCompleted<=saved.semanticSelected;
    const available=saved.indexState==='available'||saved.indexState==='ready';
    return <section class="card index-contents" aria-label="Published index contents">
        <div class="section-head"><h2>In your index</h2><CopyDetails section="index" project={props.project.project}/></div>
        <p><strong>{available?'Published for search':saved.indexState==='unavailable'?'Published index unavailable':'Waiting for a published index observation'}</strong> · observed {ago(saved.observedAt)}{!props.connected?' · disconnected; showing retained observations':''}</p>
        {props.status?.combined && <p class={props.status.combined.partial ? 'callout' : 'muted'}>{props.status.combined.available} of {props.status.combined.total} selected member indexes are readable. {props.status.combined.countsQualifier}</p>}
        <dl class="project-metrics progress-metrics">
            <div><dt>Files indexed</dt><dd>{observed(saved.files)}</dd></div>
            <div><dt>Searchable text chunks</dt><dd>{observed(saved.chunks)}</dd></div>
            <div><dt>Semantic vectors published</dt><dd>{observed(saved.vectors)}</dd></div>
        </dl>
        <div class="semantic-coverage">
            <p><strong>Semantic coverage</strong> · {saved.semanticCompleted===undefined&&saved.semanticSelected===undefined?'Not reported yet':`${observed(saved.semanticCompleted)} of ${saved.semanticSelected===undefined?'an unknown number of':observed(saved.semanticSelected)} selected files`}</p>
            {coverageKnown&&<progress value={saved.semanticCompleted} max={saved.semanticSelected} aria-label="Selected files with published semantic data"/>}
            <p class="muted">{saved.semantic==='ready'?'Semantic coverage is ready.':saved.semantic==='partial'?'Some selected files have published semantic data. Current work and waits are shown below.':saved.semantic==='unavailable'?'The semantic pipeline for this saved evidence is unavailable. This does not establish a vector opt-out or current semantic readiness.':saved.semantic==='disabled'?'This publication has no semantic coverage. Saved setup and current work are separate.':saved.semantic==='failed'?'Semantic indexing reported a failure; review Activity for its outcome.':`Semantic readiness: ${saved.semantic}.`} {saved.discoveryState==='ready'?'Source discovery completed for this snapshot.':saved.discoveryState==='partial'?'Source discovery is still partial.':''}</p>
        </div>
        <p>{sourceWorkExplanation(saved.freshness)}</p>
        <CompleteNativeResponse value={props.status?.nativeFailure ?? props.status?.combined?.nativeResult} />
        <details><summary>Publication details</summary><p>These are saved results, separate from the current attempt below. Updates appear automatically. File coverage is not a time estimate.</p><Facts rows={[
            ['Publication',shortToken(saved.publication)],['Observed',saved.observedAt===undefined?'Not observed':new Date(saved.observedAt).toISOString()],
            ['Source discovery',saved.discoveryState??'Not observed'],['Source updates',saved.freshness??'Not observed'],
        ]}/></details>
    </section>;
}

/** Shared dashboard for one exact native attempt. Previous-worker counters remain labeled. */
export function WorkProgress(props: { entry: ManagedProject }) {
    const work = props.entry.work;
    const display=workProgressDisplay(work,props.entry.summary.jobKind,props.entry.jobState);
    const sample=<section class="card work-progress" aria-label="Indexing progress">
        <div class="section-head"><h2>{work.current&&props.entry.jobState==='running'?'Working now':'Latest work attempt'}</h2><span class="muted">Updated {ago(work.observedAt)}</span><CopyDetails section="progress" project={props.entry.summary.project} /></div>
        <h3>{display.title}</h3>
        <p class="muted">{work.current ? 'Worker connected · matching job sample' : 'Retained sample · current progress unconfirmed'}.
            {' '}Last advancement: {work.lastAdvanceMs===undefined?'not reported':`${duration(work.lastAdvanceMs)} ago at sample`} · This attempt: {duration(work.elapsedMs)}.</p>
        {display.explanation&&<p>{display.explanation}</p>}
        <div class="progress-stages" aria-label="Indexing stages">{display.stages.map(stage => <span key={stage.phase} class={stage.active ? 'active' : ''}>{stage.label}</span>)}</div>
        {work.wait !== 'No matching native wait observation' && <p class="callout">{work.wait}</p>}
        <dl class="project-metrics progress-metrics">
            {display.metrics.map(([label,value])=><div key={label}><dt>{label}</dt><dd>{value===undefined?'Not reported':observed(value)}</dd></div>)}
        </dl>
        <details><summary>More progress details</summary>
            <p class="muted">Capture, extraction and publication counters describe this attempt. Captured includes reused results. Semantic units describe retained semantic work and are not a file count. Stages can repeat by batch; no overall percentage or ETA is inferred.</p>
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
                <Act actions={actions} action="registerWorkspace" enabled={view.enabled.registerWorkspace} ghost>Set up index…</Act></div>
                <span class="muted">Updates automatically</span></div>
            <details><summary>Browse active or removed projects</summary><Paging view={view} actions={actions} /></details>
            {!view.connected && !view.busy && <p class="callout">The service connection is unavailable. Automatic checks will retry and start a service that stopped while idle. If it stays unavailable, choose Start service in the Service tab.</p>}
            {!entry && <p class="callout" role="status">{!view.connected?view.busy?'Connecting to AIRK and loading saved project details…':'Project registrations could not be checked. Retained work has not been reported as removed.':`No ${view.inactive?'removed':'active'} projects on this page. Set up index opens an explicit native source and storage review in Indexing.`}</p>}
            {entry && scoped && <>
                <h2>{entry.summary.name}</h2><p class="path">{entry.summary.root}</p>
                {tab==='overview'&&!view.inactive&&<IndexContents project={entry.summary} status={entry.summary.project===props.state.selectedProject?props.state.status:undefined} connected={view.connected}/>}
                <ProjectHealth entry={entry} view={view} actions={actions} go={props.go} compact={tab==='overview'||tab==='activity'} />
                {tab === 'overview' && !view.inactive && !entry.health.action && <div class="actions">
                    <Act actions={actions} action={entry.jobState === 'running' || entry.jobState === 'pending' || entry.pendingJobId ? 'pause' : 'index'} fields={{ project: entry.summary.project }} enabled={view.enabled.projectActions}>
                        {entry.jobState === 'running' || entry.jobState === 'pending' || entry.pendingJobId ? 'Pause indexing' : 'Update index'}</Act>
                </div>}
                {view.inactive && <p class="callout">{entry.lifecycle}. Removal stops all work and deletes AIRK index data and saved settings. Source files are untouched. Removal continues automatically.</p>}
                {tab === 'overview' && <>
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
    const detailNames: Record<string,string> = { agent_lean: 'Agent Lean', essential: 'Search Only', balanced: 'Rich Structure (legacy balanced)', detailed: 'Detailed' };
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
        {(entry.configuration.configuredRetention||entry.configuration.publishedRetention)&&<p role="status">Configured retained profile: {entry.configuration.configuredRetention??'legacy'}. Published retained profile: {entry.configuration.publishedRetention??'legacy'}. Unqualified files keep the legacy fallback shown above.</p>}
        <p class="muted">Evidence retention controls supported source structure, independently of replies and semantic search. Review a new Lean or Rich profile in Advanced settings.</p>
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
        :plan?.phase==='running'?plan.items.some(item=>item.phase==='running')?'Repair running':'Repair queued':plan?.phase==='finished'?plan.outcome==='failed'?'Repair failed':plan.outcome==='succeeded'?'Repair succeeded':plan.outcome==='cleanup_pending'?'Repair succeeded; cleanup pending':'Repair ended without completing all projects':plan?.phase==='canceling'?'Canceling repair':'Repair status';
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
                        :plan.phase==='finished'?plan.outcome==='failed'?'One or more repair operations failed. Open the raw diagnostic log for the retained error details.':plan.outcome==='succeeded'?'All repair operations succeeded. Source freshness and semantic coverage are shown in Indexing.':plan.outcome==='cleanup_pending'?'Repair operations succeeded, but cleanup was not complete at the end of this plan. Storage shows the latest cleanup status.':'Some repair operations were canceled, skipped or became stale. Review the results below.'
                        :'The latest native repair state is shown below.'}</p>
                    {plan.outcome==='failed'&&<Act actions={actions} action="openRawLogs" ghost>Open raw diagnostic log</Act>}
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
                    {view.inactive && <p class="muted">AIRK is deleting index data and saved settings. Removal resumes automatically after interruptions.</p>}
                </article>)}
                <p class="muted">Remove a project from Storage and removal, even if its source folder is gone. AIRK then deletes its owned index data and saved settings automatically. Quotas and extra roots are in Advanced settings.</p>
                {view.inactive && <p class="muted">Removed projects finish deleting their AIRK data automatically. Source files are untouched.</p>}
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
                    <Act actions={actions} action="queryConcurrency" enabled={on('queryConcurrency')} ghost>Concurrent requests…</Act>
                </div>
                {view.lifecycle === 'external' && <p class="muted">Startup, stop, restart, upgrades and rollback are controlled by the external service manager.</p>}
                {view.capturedLifecycle&&<details class="card" data-testid="captured-native-lifecycle"><summary>Captured native lifecycle response (historical)</summary><p>This records the complete native request and response at capture time, with the actual pre-operation health response when available. It is not current health, evidence authority or a new process inspection. JSON includes machine details and any native diagnostic fields.</p><pre>{JSON.stringify(view.capturedLifecycle,null,2)}</pre><div class="actions"><Act actions={actions} action="copyLifecycle">Copy complete JSON</Act><Act actions={actions} action="openLifecycle" ghost>Open complete JSON</Act><Act actions={actions} action="saveLifecycle" ghost>Export complete JSON</Act></div></details>}
                {view.capturedLifecycleUnavailable&&<p class="callout">The native lifecycle result exceeded the complete-capture budget. Its saved request remains tracked for native recovery; no completed receipt is being displayed.</p>}
                {view.cleanBreakRebuild&&<CleanBreakProgress value={view.cleanBreakRebuild} />}
                <div class="cards two">
                    <article class="card"><div class="section-head"><h2>Service</h2><CopyDetails section="service" /></div><Rows rows={view.service} hideUnobserved />
                        <p class="muted">Health confirms responsiveness. It does not prove indexing progress, source freshness or complete semantic coverage.</p></article>
                    <article class="card"><h2>Runtime</h2><Rows rows={view.runtime} hideUnobserved />
                        <p class="muted">Selected summary fields. Complete runtime diagnostics retain the cached native bootstrap and diagnosis, including upgrade blockers. They are an observation at capture time; later runtime state may differ.</p>
                        {view.runtimeDiagnostics&&<details><summary>Complete runtime diagnostics</summary><p>Captured {new Date(view.runtimeDiagnostics.capturedAt).toISOString()}. Full JSON includes machine paths and native diagnostic fields.</p><div class="actions"><Act actions={actions} action="openRuntimeDiagnostics" ghost>Open complete JSON</Act><Act actions={actions} action="copyRuntimeDiagnostics" ghost>Copy complete JSON</Act><Act actions={actions} action="saveRuntimeDiagnostics" ghost>Export complete JSON</Act></div></details>}
                        <div class="actions">
                            <Act actions={actions} action="upgradeRuntime" enabled={on('upgradeRuntime')} ghost>Review bundled runtime upgrade</Act>
                            <Act actions={actions} action="rollbackRuntime" enabled={on('rollbackRuntime')} ghost>Review previous runtime</Act>
                            <Act actions={actions} action="repairRuntime" enabled={on('repairRuntime')} ghost>Review runtime payload repair</Act>
                            <Act actions={actions} action="resumeActivation" enabled={on('resumeActivation')} ghost>Review saved runtime activation</Act>
                            <Act actions={actions} action="recoverLifecycle" enabled={on('recoverLifecycle')} ghost>Resume saved lifecycle request</Act>
                            <Act actions={actions} action="abandonLifecycle" enabled={on('abandonLifecycle')} ghost>Abandon saved lifecycle request</Act>
                        </div></article>
                    <article class="card"><h2>Resources</h2><Rows rows={view.resources} hideUnobserved /></article>
                    <article class="card"><h2>Inference helper</h2><Rows rows={view.inference} hideUnobserved />
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

function AgentInstructions(props: ScreenProps) {
    const [busy, setBusy] = useState<string>(), [result, setResult] = useState<string>();
    const project = props.state.selectedProject;
    const run = async (label: string, action: () => Promise<string>) => {
        setBusy(label); setResult(undefined);
        try { setResult(await action()); } catch (error) { setResult(error instanceof Error ? error.message : String(error)); }
        finally { setBusy(undefined); }
    };
    return <article class="card agent-instructions">
        <h2>Help your agent discover and use AIRK</h2>
        <p>Add this suggestion to your project instructions so the agent checks its tools before claiming AIRK is unavailable, uses focused indexed evidence and verifies live files before edits.</p>
        <div class="instruction-suggestion"><Markdown text={AIRK_AGENT_INSTRUCTIONS}/></div>
        <div class="actions">
            <button type="button" disabled={!!busy} onClick={() => void run('copy', async () => {
                await request('clipboard.write', { text: AIRK_AGENT_INSTRUCTIONS + '\n' }); return 'Instructions copied. Open an instruction file, paste and review them, then save.';
            })}>{busy === 'copy' ? 'Copying…' : 'Copy instructions'}</button>
            <button type="button" class="ghost" onClick={() => props.go({ workspace: 'help', focus: 'agent-instructions' })}>Instruction file help</button>
        </div>
        <p>Open instructions for <strong>{props.state.projects.find(entry => entry.project === project)?.name ?? 'the selected project'}</strong>:</p>
        <div class="actions">
            {(Object.keys(AGENT_INSTRUCTION_FILES) as (keyof typeof AGENT_INSTRUCTION_FILES)[]).map(agent => <button key={agent} type="button" class="ghost" disabled={!!busy || !project || !props.state.trusted}
                title={AGENT_INSTRUCTION_FILES[agent].file} onClick={() => void run(agent, async () => (await request<{ message: string }>('instructions.open', { agent, project: project! })).message)}>
                {busy === agent ? 'Opening…' : `Open ${AGENT_INSTRUCTION_FILES[agent].name} instructions`}</button>)}
        </div>
        {busy && <p role="status">{busy === 'copy' ? 'Copying instructions…' : `Opening ${AGENT_INSTRUCTION_FILES[busy as keyof typeof AGENT_INSTRUCTION_FILES].file}…`}</p>}
        {result && <Notice noticeKey={noticeKey("agent-result", result)}><p>{result}</p></Notice>}
        <p class="muted">Claude Code: <code>CLAUDE.md</code> · Codex: <code>AGENTS.md</code> · Copilot: <code>.github/copilot-instructions.md</code>. Existing files open without changes; missing files open as unsaved drafts. Review with your existing rules before saving. Other assistants can use the copied text in their supported project instruction or rules file. Instructions guide the agent; they do not configure MCP or guarantee tool access.</p>
    </article>;
}

/** What one finding means for the user, in plain words. */
export function agentFindingExplanation(finding: AgentConfigFinding): string {
    switch (finding.problem) {
        case 'stdio_missing_command': return `This entry starts an AIRK program that no longer exists (an earlier AIRK installation was replaced or removed). AIRK now connects agents over local HTTPS. Reconnect replaces this entry for ${finding.label}; other servers and settings are kept.`;
        case 'stdio_retired': return `This entry starts AIRK as a local stdio program. AIRK no longer accepts stdio MCP connections; agents connect over local HTTPS. Reconnect replaces this entry for ${finding.label}; other servers and settings are kept.`;
        case 'https_unknown_connection': return 'This HTTPS entry uses a connection that the current AIRK service does not know (its installation was replaced). Reconnect creates a new connection and updates this entry.';
        case 'https_endpoint_mismatch': return 'This HTTPS entry points to an earlier AIRK address. Reconnect updates it to the address the service publishes now.';
        case 'https_helper_missing': return 'The credential helper of this HTTPS entry names an AIRK program that no longer exists (the runtime was upgraded). Reconnect points it at the current program; the connection itself is unchanged.';
        case 'https_trust_missing': return 'This entry is current, but your user certificate store does not trust the AIRK local HTTPS issuer, so an agent started outside VS Code cannot verify the connection. Reconnect offers to install that trust.';
        default: return `This file could not be checked: ${finding.detail ?? 'it could not be read.'} Review it manually.`;
    }
}
/** Rendered inline, not as a component, so each finding keys directly in its list. */
function findingCard(finding: AgentConfigFinding, actions: ReturnType<typeof useActions>) {
    return <div key={`${finding.file}\u0000${finding.serverKey ?? ''}`} class="callout" role="group" aria-label={`${finding.label} configuration`}>
        <p><strong>{finding.label}</strong>{finding.scope === 'user' ? ' (user settings)' : ''} · <code class="path">{finding.file}</code>{finding.serverKey !== undefined && <> · server <code>{finding.serverKey}</code></>}</p>
        {finding.command && <p>{finding.problem === 'stdio_missing_command' ? 'Missing command' : 'Command'}: <code class="path">{finding.command}</code></p>}
        {finding.url && <p>Address: <code>{finding.url}</code></p>}
        {finding.root && <p>Project folder: <code class="path">{finding.root}</code></p>}
        <p>{agentFindingExplanation(finding)}</p>
        {finding.problem !== 'unreadable' && <div class="actions">
            <button type="button" onClick={() => { void actions.command('airk.reconnectAgents'); }}>Reconnect…</button>
            <button type="button" class="ghost" onClick={() => { void actions.command('airk.setupAgent'); }}>Connect a coding agent…</button>
        </div>}
    </div>;
}

/** Explorer-wide warning while broken or retired AIRK agent entries remain. */
export function AgentConfigAlert(props: { alerts: AgentConfigAlerts; showDetails: () => void }) {
    const actions = useActions(), { alerts } = props;
    const agents = [...new Set(alerts.findings.map(finding => finding.label))].join(', ');
    return <Notice global noticeKey={noticeKey('agent-config-alert', alerts.digest)} class="card" role="alert">
        <h1>{alerts.total ? 'Coding agents cannot reach AIRK' : 'AIRK Reconnect results'}</h1>
        {alerts.total > 0 && <p>{alerts.total === 1 ? 'One AIRK entry' : `${alerts.total} AIRK entries`} in {agents || 'agent'} configuration {alerts.total === 1 ? 'needs' : 'need'} attention. {alerts.findings[0] ? agentFindingExplanation(alerts.findings[0]) : ''}</p>}
        {alerts.results?.length ? <ul>{alerts.results.map(result => <li key={`${result.file}\u0000${result.serverKey}`}><strong>{result.label}</strong> <code class="path">{result.file}</code>: {result.status.split('_').join(' ')}{result.detail ? ` (${result.detail})` : ''}{result.hostAction ? ` ${result.hostAction}` : ''}</li>)}</ul> : null}
        <div class="actions">
            {alerts.total > 0 && <button type="button" disabled={alerts.reconnecting} onClick={() => { void actions.command('airk.reconnectAgents'); }}>{alerts.reconnecting ? 'Reconnecting…' : 'Reconnect…'}</button>}
            <button type="button" class="ghost" onClick={props.showDetails}>Show details</button>
        </div>
        {actions.outcome && <p role="status">{actions.outcome}</p>}
    </Notice>;
}

/** Read-only check of the selected project's own agent configuration files; keyed by project so results never carry over. */
function AgentConfigCheck(props: { project?: string; trusted: boolean; actions: ReturnType<typeof useActions> }) {
    const [health, setHealth] = useState<AgentConfigHealthView>(), [error, setError] = useState<string>();
    const [busy, setBusy] = useState(false), [attempt, setAttempt] = useState(0);
    const available = !!props.project && props.trusted;
    useEffect(() => {
        if (!available) { return; }
        let current = true;
        setBusy(true); setError(undefined);
        request<AgentConfigHealthView>('agents.configHealth', { project: props.project! }).then(
            value => { if (current) { setHealth(value); } },
            failure => { if (current) { setHealth(undefined); setError(failure instanceof Error ? failure.message : String(failure)); } },
        ).finally(() => { if (current) { setBusy(false); } });
        return () => { current = false; };
    }, [props.project, available, attempt]);
    return <article class="card agent-config-check">
        <h2>Agent configuration check</h2>
        <p>Looks for AIRK entries in this project’s agent configuration files that no longer work: stdio entries (AIRK now connects agents over local HTTPS), missing programs, and HTTPS entries with an unknown connection, an old address or an untrusted issuer. Files are only read; nothing changes until you choose Reconnect.</p>
        {busy && <p role="status">Checking agent configuration files…</p>}
        {error && <Notice noticeKey={noticeKey("agent-config-check", props.project, error)}><p>{error}</p></Notice>}
        {health && !busy && (health.findings.length ? health.findings.map(finding => findingCard(finding, props.actions))
            : <p class="muted">No broken AIRK entries found in this project's agent configuration files.</p>)}
        {health && !busy && health.checked.length > 0 && <p class="muted">Checked: {health.checked.map((file, index) => <span key={file}>{index ? ', ' : ''}<code>{file}</code></span>)}</p>}
        <div class="actions"><button type="button" class="ghost" disabled={busy || !available} onClick={() => setAttempt(value => value + 1)}>{busy ? 'Checking…' : 'Check again'}</button></div>
    </article>;
}

export function AgentsScreen(props: ScreenProps) {
    const view = useManagement(props.state), actions = useActions();
    const check=props.state.connectionCheck;
    const checking=check?.phase==='preparing'||check?.phase==='checking';
    const [now,setNow]=useState(Date.now()),[copied,setCopied]=useState<string>();
    useEffect(()=>setCopied(undefined),[check?.startedAt]);
    useEffect(()=>{if(!checking)return;const timer=setInterval(()=>setNow(Date.now()),1000);return ()=>clearInterval(timer);},[checking]);
    return (
        <Frame view={view} trusted={props.state.trusted} actions={actions} intro="Connect coding agents to AIRK on">
            {view && <>
                <div class="cards two">
                    <article class="card"><h2>AIRK agent connection</h2>
                        <p>One connection provides 27 tools for indexed evidence, help and Explorer drafts. Drafts wait for your review.</p>
                        <div class="actions">
                            <button type="button" onClick={() => { void actions.command('airk.setupAgent'); }}>Connect a coding agent…</button>
                            <button type="button" class="ghost" disabled={checking||!props.state.selectedProject} onClick={() => { void actions.command('airk.verifyMcp'); }}>{checking?'Checking AIRK…':'Check AIRK connection'}</button>
                            <button type="button" class="ghost" onClick={() => { void actions.command('airk.manageVscodeMcp'); }}>VS Code / Copilot MCP controls</button>
                        </div>
                        {check&&<Notice noticeKey={noticeKey("connection-check", check.project, checking ? "checking" : check.phase, checking ? undefined : check.message ?? check.error)}>
                            <strong>{checking?'Checking connection':check.phase==='verified'?'AIRK connection verified':'AIRK connection check failed'}</strong>
                            {check.project&&<p>Project: {props.state.projects.find(project=>project.project===check.project)?.name??check.project}</p>}
                            <p>{check.message??check.error}</p>
                            {check.startedAt&&<p>{checking?'Elapsed':'Duration'}: {duration(Math.max(0,(checking?now:check.checkedAt??now)-check.startedAt))}</p>}
                            {check.checkedAt&&<p>Checked: {new Date(check.checkedAt).toLocaleString()}</p>}
                            <button class="ghost" onClick={async()=>{try{await request('clipboard.write',{text:`AIRK ${props.state.extensionVersion} — MCP connection check\n${JSON.stringify(check,null,2)}`});setCopied('Details copied.');}catch(error){setCopied(String(error));}}}>Copy connection details</button>
                            {copied&&<p>{copied}</p>}
                        </Notice>}
                        <p class="muted">This checks the selected HTTPS connection, MCP handshake, tool list and index status. It does not establish that Claude Code, Codex or Copilot loaded their configuration. If AIRK passes but an agent has no tools, inspect that agent’s MCP connection error.</p></article>
                    <article class="card"><h2>Responses and terminal</h2>
                        <p>MCP response mode: <strong>{view.responseMode}</strong>.</p>
                        <p><strong>Terse (default)</strong> suits routine code searches and navigation. It keeps focused evidence and citations while reducing repeated metadata, leaving more of the agent’s context budget for the task.</p>
                        <p><strong>Full</strong> adds detailed provenance, readiness and coverage information. Choose it to investigate missing or surprising results, freshness problems or parser limitations, or to audit where evidence came from. Responses are larger; Full does not make the index more complete.</p>
                        <p class="muted">Agents can request Full for an individual call with <code>response_mode: full</code>. Changing this default requires reconnecting the agent’s MCP session.</p>
                        <div class="actions">
                            <Act actions={actions} action="responseMode" enabled={view.enabled.responseMode} ghost>Choose terse / full responses</Act>
                            <button type="button" class="ghost" onClick={() => { void actions.command('airk.installCli'); }}>Install or repair CLI</button>
                            <button type="button" class="ghost" onClick={() => { void actions.command('airk.openTools'); }}>Open AIRK Tools terminal</button>
                            <button type="button" class="ghost" onClick={() => { void actions.command('airk.httpIssuerRotationStatus'); }}>Open HTTPS issuer rotation receipt</button>
                            <button type="button" class="ghost" onClick={() => { void actions.command('airk.rotateHttpIssuer'); }}>Review HTTPS issuer rotation…</button>
                        </div></article>
                </div>
                <AgentConfigCheck key={`config-check:${props.state.selectedProject ?? ''}`} project={props.state.selectedProject} trusted={props.state.trusted} actions={actions}/>
                <AgentInstructions key={props.state.selectedProject} {...props}/>
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
                <article class="card">
                    <h2>Raw diagnostic log</h2>
                    <p>Retained indexing, setup, connection and diagnostic-action failures. Entries identify their operation, time and job when available. These local logs can contain paths and error values; review before sharing. Nothing is uploaded. Each retained stream purges its oldest entries from 200 MB to at most 100 MB.</p>
                    <div class="actions">
                        <Act actions={actions} action="openRawLogs">Open raw diagnostic log</Act>
                        <Act actions={actions} action="saveRawLogs" ghost>Save raw diagnostic log…</Act>
                    </div>
                </article>
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
                    <p class="muted">Up to 32 recent service and job events are shown, including available job IDs and failure reasons. Open service logs writes this bounded native log page to AIRK Service output. Save retained service logs exports the full available history.</p>
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
