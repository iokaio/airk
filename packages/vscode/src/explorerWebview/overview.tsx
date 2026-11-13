// SPDX-License-Identifier: Apache-2.0
import { Notice, noticeKey } from './notices';
import { useState } from 'preact/hooks';
import { ExplorerViewState, ProjectSummary } from '../explorerContracts';
import { Route } from '../explorerRoutes';
import { savedState, saveState } from './api';
import { CopyDetails, Facts, WorkspaceIcon } from './components';
import { ago, observed } from './format';
import { IndexContents, ProjectHealth, WorkActions, WorkProgress } from './projectStatus';
import { AdminButton, CreateIndexForFolders, openAdmin } from './adminLinks';
import type { AdminPage } from '../explorerContracts';
import { semanticExplanation } from '../explorerStatus';
import { ScopeBanner } from './scopeBanner';

const JOURNEYS: { id: string; title: string; text: string; route: Route; admin?: AdminPage }[] = [
    { id: 'explore', title: 'Explore', text: 'Search code and documents; every hit is a citation.', route: { workspace: 'search', tab: 'query' } },
    { id: 'investigate', title: 'Investigate', text: 'Inspect declarations, references and candidate relationships.', route: { workspace: 'code', tab: 'symbols' } },
    { id: 'review', title: 'Review', text: 'Check changed files against indexed facts.', route: { workspace: 'review', tab: 'changes' } },
    { id: 'prepare', title: 'Prepare', text: 'Compile bounded, cited evidence into a prompt.', route: { workspace: 'strategies', tab: 'studio' } },
    { id: 'handoff', title: 'Hand off', text: 'Save a plan file an agent can follow.', route: { workspace: 'strategies', tab: 'planner' } },
    { id: 'maintain', title: 'Maintain', text: 'Index setup, recovery, storage and agents in AIRK Admin.', route: { workspace: 'manage', tab: 'overview' }, admin: 'overview' },
];
/** Lenses reorder journeys only; they never hide a workspace. */
const LENSES: Record<string, string[]> = {
    developer: ['explore', 'investigate', 'prepare', 'handoff', 'review', 'maintain'],
    tester: ['review', 'explore', 'investigate', 'prepare', 'handoff', 'maintain'],
    architect: ['investigate', 'explore', 'review', 'prepare', 'handoff', 'maintain'],
    writer: ['explore', 'prepare', 'handoff', 'review', 'investigate', 'maintain'],
};
const ROLE_HINTS: Record<string, string> = {
    developer: 'Start with Agent Lean for code navigation and bug/API work; use Rich Structure when broader structural facts or continuity are needed.',
    tester: 'Keep requirements and acceptance documents alongside code and tests. Agent Lean is a starting point; richer document/table evidence may need Rich Structure.',
    architect: 'Compare design decisions with written imports and candidate relationships. Use Rich Structure when lean evidence is insufficient; inspect every boundary claim.',
    writer: 'Use Search Only for text-only lookup. Choose Agent Lean or richer detail when you need requirement, heading, table or decision facts.',
};

export function Overview(props: { state: ExplorerViewState; project?: ProjectSummary; go(route: Route): void }) {
    const { state, project } = props, status = state.status;
    const management = state.management;
    const managed = management?.projects.find(entry => entry.summary.project === state.selectedProject);
    const semantic = semanticExplanation(status);
    const [lens, setLens] = useState<string>(() => { const value = savedState<{ lens?: unknown }>()?.lens; return typeof value === 'string' && Object.hasOwn(LENSES, value) ? value : 'developer'; });
    const [journeyError, setJourneyError] = useState('');
    const route = state.searchScope?.route;
    const actionsDisabled = !state.trusted || state.nativeExecutionEnabled === false || state.connection !== 'ready' && state.connection !== 'degraded';
    const order = LENSES[lens] ?? LENSES.developer;
    if(state.trusted&&state.connection==='connecting'&&!state.projects.length)return <section class="workspace">
        <div class="workspace-heading"><div><p class="eyebrow">AIRK workspace</p><h1>Connecting to AIRK</h1>
            <p>Checking the local service and saved project registrations. Project details have not been loaded yet.</p></div></div>
        <p class="callout" role="status">Your project has not been reported as removed or unregistered. Waiting for the service response.</p>
        <AdminButton page="service" label="View connection details in AIRK Admin" ghost />
    </section>;
    // The connection and compatibility come first: without them "no index" is not known.
    const first = !state.trusted ? state.workspaceTrusted===false?'VS Code Restricted Mode is active. Manage Workspace Trust to review this workspace.':'AIRK client authority is unavailable; no current evidence access is assumed.'
        : state.connection === 'connecting' ? 'Connecting to AIRK and checking registered projects. Saved project details will appear when the service responds.'
        : state.connection === 'disconnected' ? 'The AIRK service is not connected. Start it, or review it in AIRK Admin › Service.'
        : state.connection === 'incompatible' ? 'The installed AIRK Service does not match this extension. Review it in AIRK Admin › Service.'
        : !state.projects.length ? 'AIRK has no index for this window yet. Create one in AIRK Admin.'
        : undefined;
    return (
        <section class="workspace">
            <div class="workspace-heading"><div><p class="eyebrow">Local evidence. Clear provenance.</p><h1>{project?.name ?? 'Your workspace'}</h1>
                <p>{project && status?.indexState === 'available' ? 'Your published index is available. Current indexing work is shown separately below.' : 'Review project health, indexing progress and published search availability.'}</p></div>
                <AdminButton page="overview" disabled={!state.trusted} /></div>
            <ScopeBanner state={state} go={props.go} />
            {first && <Notice condition noticeKey={noticeKey("overview-connection", first)}><p>{first}</p></Notice>}
            {journeyError && <Notice condition noticeKey={noticeKey("overview-error", journeyError)} role="alert"><p>{journeyError}</p></Notice>}
            {project && route?.state === 'not_readable' && <article class="card not-readable" role="status">
                <h2>Not readable yet</h2>
                <p>{route.message ?? `${route.indexName ?? 'This index'} is in no index set, so searches cannot read it yet.`} Indexing continues; progress is shown below. Reads need at least one index set (code <code>{route.code ?? 'index_read_unavailable'}</code>).</p>
                <AdminButton page="index-library" index={route.indexId} label="Make readable in AIRK Admin" disabled={!state.trusted} choose={false} />
            </article>}
            {project && route?.state === 'failed' && <Notice condition noticeKey={noticeKey('read-route', route.code, route.message)} role="alert"><p>This window's index could not be resolved in the index library ({route.code}). {route.message}</p></Notice>}
            {state.trusted && state.connection === 'ready' && !state.projects.length && <CreateIndexForFolders folders={state.workspaceFolders} />}
            {management?.message && <Notice noticeKey={noticeKey("control-outcome", management.message)}><p>Last control outcome: {management.message}</p></Notice>}
            {project&&<IndexContents project={project} status={status} connected={management?.connected??state.connection==='ready'}/>}
            {managed && management && <>
                <ProjectHealth entry={managed} compact />
                {(managed.jobId||managed.pendingJobId||managed.configuration.pending)&&<WorkProgress entry={managed} />}
                <div class="actions">
                    <button type="button" class="ghost" onClick={() => props.go({ workspace: 'manage', tab: 'activity' })}>View recent activity</button>
                </div>
            </>}
            {project && <WorkActions project={project} disabled={actionsDisabled} />}
            {project && !managed && <p class="muted">Current worker progress is not present in this project page. <AdminButton page="overview" label="Review indexes in AIRK Admin" ghost choose={false} /></p>}
            <details class="card project-details"><summary>Project and semantic settings</summary>
            <div class="cards two">
                <article class="card">
                    <h2>Selected project</h2>
                    <Facts rows={[
                        ['Collection', project ? project.collection : 'none'],
                        ['Source root', project?.root || 'not observed'],
                        ['Watching', observed(project?.watching)],
                        ['Paused', observed(project?.paused)],
                        ['Current job', project?.jobState ? `${project.jobKind ?? 'job'} · ${project.jobState}${project.phase ? ` · ${project.phase}` : ''}` : 'none observed'],
                        ['Console observation', `cached · ${ago(state.projectsObservedAt)}`],
                    ]} />
                </article>
            <article class="card semantic-status">
                <div class="section-head"><h2>Semantic search — {semantic.title}</h2>{state.selectedProject&&<CopyDetails section="semantic" project={state.selectedProject} />}</div><p>{semantic.detail}</p>
                {(status?.semanticReadiness || status?.legacyAdoption) && <details><summary>Complete semantic readiness and saved-evidence compatibility</summary><pre>{JSON.stringify({semantic_readiness:status.semanticReadiness,legacy_adoption:status.legacyAdoption},null,2)}</pre></details>}
                <Facts rows={[
                    ['Saved vector policy', status?.vectorsEnabled === undefined ? 'Not checked' : status.vectorsEnabled ? 'Enabled' : 'Disabled'],
                    ['Model configured in publication', status?.localModelReady === undefined ? 'Not checked' : status.localModelReady ? 'Yes (not proof of a running helper)' : 'No'],
                    ['Published semantic state', status?.semantic ?? 'Not checked'],
                    ['Completed / selected artifacts', status ? `${observed(status.semanticCompleted)} / ${observed(status.semanticSelected)}` : 'Not checked'],
                    ['Published status observed', ago(status?.observedAt)],
                    ['Detailed observations', ago(status?.detailsObservedAt)],
                ]} />
                <p class="muted">Saved setup policy can be newer than this publication. Source indexing and semantic work run separately.</p>
                <div class="actions"><AdminButton page="index-library" label="Review semantic setup in AIRK Admin" choose={false} />
                    <button type="button" class="ghost" onClick={() => props.go({ workspace: 'help', focus: 'indexing-choices' })}>Why these states?</button></div>
            </article>
            </div></details>
            <div class="section-head">
                <h2>Journeys</h2>
                <label>Lens <select value={lens} onChange={event => {
                    const next = (event.target as HTMLSelectElement).value;
                    setLens(next); saveState({ ...savedState<object>(), lens: next });
                }}>
                    <option value="developer">Developer</option><option value="tester">Tester</option>
                    <option value="architect">Architect</option><option value="writer">Writer</option>
                </select></label>
            </div>
            <article class="card">
                <h2>{lens[0].toUpperCase() + lens.slice(1)} guidance</h2>
                <p>{ROLE_HINTS[lens] ?? ROLE_HINTS.developer}</p>
                <p class="muted">A lens reorders journeys; it does not change indexing settings.</p>
                <div class="actions"><button class="ghost" onClick={() => props.go({ workspace: 'help', focus: `role-${lens}` })}>Role workflow help</button>
                    <button class="ghost" onClick={() => props.go({ workspace: 'help', focus: 'indexing-choices' })}>Choose indexing by purpose</button>
                    <button class="ghost" onClick={() => props.go({ workspace: 'help', focus: 'document-types' })}>Document type guidance</button></div>
            </article>
            <div class="cards three">
                {order.map(id => JOURNEYS.find(entry => entry.id === id)!).map(journey => (
                    <button type="button" key={journey.id} class="card journey" onClick={() => { setJourneyError(''); if (journey.admin) { void openAdmin({ page: journey.admin }).catch(failure => setJourneyError(failure instanceof Error ? failure.message : String(failure))); } else { props.go(journey.route); } }}>
                        <WorkspaceIcon name={journey.route.workspace} /><strong>{journey.title}</strong><span>{journey.text}</span>
                    </button>
                ))}
            </div>
        </section>
    );
}
