// SPDX-License-Identifier: Apache-2.0
// Read-only project status shared by Overview and the Explorer shell. Every
// action that changes a project, a job or the service is in AIRK Admin.
import { useState } from 'preact/hooks';
import { Notice, noticeKey } from './notices';
import type { AgentConfigAlerts, AgentConfigFinding, FactRow, ManagedProject, ProjectSummary, StatusSummary } from '../explorerContracts';
import { request } from './api';
import { CompleteNativeResponse, CopyDetails, Facts } from './components';
import { ago, duration, observed, shortToken } from './format';
import { indexContents, sourceWorkExplanation, workProgressDisplay } from '../explorerStatus';
import { AdminButton } from './adminLinks';

function Rows(props: { rows: readonly FactRow[]; hideUnobserved?: boolean }) { return <Facts rows={props.rows.filter(([, value]) => !props.hideUnobserved || value.toLowerCase() !== 'not observed').map(([name, value]) => [name, value])} />; }

/** Read-only project health; actions that change a project are in AIRK Admin (U6, U97). */
export function ProjectHealth(props: { entry: ManagedProject; compact?: boolean }) {
    const { entry } = props, health = entry.health;
    // Ordinary live work is explained by the adjacent progress card. Keep failures,
    // pauses, recovery and waits prominent instead of repeating a generic running banner.
    if (props.compact && health.routineWork) { return null; }
    return <section class={`project-health health-${health.severity}`} aria-label={`${entry.summary.name} health`}>
        <div class="section-head"><h3>{health.title}</h3><CopyDetails section="health" project={entry.summary.project} /></div><p>{health.detail}</p>
        {(health.action || health.reviewTab || health.severity === 'error') && <div class="actions">
            <AdminButton page={health.reviewTab === 'repair' ? 'recovery' : health.reviewTab === 'logs' || !health.action ? 'logs' : 'activity'}
                label={health.reviewTab === 'repair' ? 'Review recovery in AIRK Admin' : health.action && health.actionLabel ? `${health.actionLabel} in AIRK Admin` : 'Review logs in AIRK Admin'} choose={false} />
        </div>}
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
/** Explorer-wide warning while broken or retired AIRK agent entries remain. */
export function AgentConfigAlert(props: { alerts: AgentConfigAlerts; showDetails: () => void }) {
    const { alerts } = props;
    const [outcome, setOutcome] = useState<string>();
    const reconnect = async () => { try { await request('manage.command', { command: 'airk.reconnectAgents' }); } catch (failure) { setOutcome(failure instanceof Error ? failure.message : String(failure)); } };
    const agents = [...new Set(alerts.findings.map(finding => finding.label))].join(', ');
    return <Notice global noticeKey={noticeKey('agent-config-alert', alerts.digest)} class="card" role="alert">
        <h1>{alerts.total ? 'Coding agents cannot reach AIRK' : 'AIRK Reconnect results'}</h1>
        {alerts.total > 0 && <p>{alerts.total === 1 ? 'One AIRK entry' : `${alerts.total} AIRK entries`} in {agents || 'agent'} configuration {alerts.total === 1 ? 'needs' : 'need'} attention. {alerts.findings[0] ? agentFindingExplanation(alerts.findings[0]) : ''}</p>}
        {alerts.results?.length ? <ul>{alerts.results.map(result => <li key={`${result.file}\u0000${result.serverKey}`}><strong>{result.label}</strong> <code class="path">{result.file}</code>: {result.status.split('_').join(' ')}{result.detail ? ` (${result.detail})` : ''}{result.hostAction ? ` ${result.hostAction}` : ''}</li>)}</ul> : null}
        <div class="actions">
            {alerts.total > 0 && <button type="button" disabled={alerts.reconnecting} onClick={() => { void reconnect(); }}>{alerts.reconnecting ? 'Reconnecting…' : 'Reconnect…'}</button>}
            <button type="button" class="ghost" onClick={props.showDetails}>Show details</button>
        </div>
        {outcome && <p role="status">{outcome}</p>}
    </Notice>;
}
