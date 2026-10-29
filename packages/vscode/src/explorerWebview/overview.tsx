// SPDX-License-Identifier: Apache-2.0
import { useState } from 'preact/hooks';
import { ExplorerViewState, ProjectSummary } from '../explorerContracts';
import { Route } from '../explorerRoutes';
import { savedState, saveState } from './api';
import { CopyDetails, Facts, WorkspaceIcon } from './components';
import { ago, observed } from './format';
import { Act, IndexContents, ProjectHealth, useActions, WorkProgress } from './manage';
import { semanticExplanation } from '../explorerStatus';

const JOURNEYS: { id: string; title: string; text: string; route: Route }[] = [
    { id: 'explore', title: 'Explore', text: 'Search code and documents; every hit is a citation.', route: { workspace: 'search', tab: 'query' } },
    { id: 'investigate', title: 'Investigate', text: 'Inspect declarations, references and candidate relationships.', route: { workspace: 'code', tab: 'symbols' } },
    { id: 'review', title: 'Review', text: 'Check changed files against indexed facts.', route: { workspace: 'review', tab: 'changes' } },
    { id: 'prepare', title: 'Prepare', text: 'Compile bounded, cited evidence into a prompt.', route: { workspace: 'strategies', tab: 'studio' } },
    { id: 'handoff', title: 'Hand off', text: 'Save a plan file an agent can follow.', route: { workspace: 'strategies', tab: 'planner' } },
    { id: 'maintain', title: 'Maintain', text: 'Index, repair, storage, agents and usage.', route: { workspace: 'manage', tab: 'overview' } },
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
    const actions = useActions(), management = state.management;
    const managed = management?.projects.find(entry => entry.summary.project === state.selectedProject);
    const semantic = semanticExplanation(status);
    const [lens, setLens] = useState<string>(() => { const value = savedState<{ lens?: unknown }>()?.lens; return typeof value === 'string' && Object.hasOwn(LENSES, value) ? value : 'developer'; });
    const order = LENSES[lens] ?? LENSES.developer;
    if(state.trusted&&state.connection==='connecting'&&!state.projects.length)return <section class="workspace">
        <div class="workspace-heading"><div><p class="eyebrow">AIRK workspace</p><h1>Connecting to AIRK</h1>
            <p>Checking the local service and saved project registrations. Project details have not been loaded yet.</p></div></div>
        <p class="callout" role="status">Your project has not been reported as removed or unregistered. Waiting for the service response.</p>
        <button type="button" class="ghost" onClick={()=>props.go({workspace:'manage',tab:'service'})}>View connection details</button>
    </section>;
    const first = !state.trusted ? 'Trust this workspace in VS Code before AIRK can connect or read evidence.'
        : state.connection === 'connecting' ? 'Connecting to AIRK and checking registered projects. Saved project details will appear when the service responds.'
        : !state.projects.length ? 'No AIRK project is registered for this window yet. Registration and indexing start automatically for trusted folders, or use Management.'
        : state.connection === 'disconnected' ? 'The AIRK service is not connected. Open Management › Service to start or reconnect it.'
        : state.connection === 'incompatible' ? 'The selected AIRK runtime does not match this extension. Review the runtime in Management › Service.'
        : undefined;
    return (
        <section class="workspace">
            <div class="workspace-heading"><div><p class="eyebrow">Local evidence. Clear provenance.</p><h1>{project?.name ?? 'Your workspace'}</h1>
                <p>{project && status?.indexState === 'available' ? 'Your published index is available. Current indexing work is shown separately below.' : 'Review project health, indexing progress and published search availability.'}</p></div></div>
            {first && <p class="callout">{first}</p>}
            {actions.outcome && <p class="callout" role="status">{actions.outcome}</p>}
            {management?.message && !actions.outcome && <p class="muted" role="status">Last control outcome: {management.message}</p>}
            {project&&<IndexContents project={project} status={status} connected={management?.connected??state.connection==='ready'}/>}
            {managed && management && <>
                <ProjectHealth entry={managed} view={management} actions={actions} go={props.go} compact />
                {(managed.jobId||managed.pendingJobId||managed.configuration.pending)&&<WorkProgress entry={managed} />}
                <div class="actions">
                    <button type="button" class="ghost" onClick={() => props.go({ workspace: 'manage', tab: 'activity' })}>View full activity</button>
                    <Act actions={actions} action={managed.summary.paused ? 'resume' : 'pause'} fields={{ project: managed.summary.project }} enabled={management.enabled.projectActions && !management.inactive} ghost>{managed.summary.paused ? 'Resume project' : 'Pause project'}</Act>
                    {managed.jobId && ['running', 'pending'].includes(managed.jobState ?? '') && <Act actions={actions} action="cancelJob" fields={{ id: managed.jobId }} enabled={management.enabled.projectActions} ghost>Cancel job</Act>}
                </div>
            </>}
            {!managed && <p class="muted">Current worker progress is not present in this project page. <button type="button" class="ghost" onClick={() => props.go({ workspace: 'manage', tab: 'overview' })}>Review projects</button></p>}
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
                <Facts rows={[
                    ['Saved vector policy', status?.vectorsEnabled === undefined ? 'Not checked' : status.vectorsEnabled ? 'Enabled' : 'Disabled'],
                    ['Model configured in publication', status?.localModelReady === undefined ? 'Not checked' : status.localModelReady ? 'Yes (not proof of a running helper)' : 'No'],
                    ['Published semantic state', status?.semantic ?? 'Not checked'],
                    ['Completed / selected artifacts', status ? `${observed(status.semanticCompleted)} / ${observed(status.semanticSelected)}` : 'Not checked'],
                    ['Published status observed', ago(status?.observedAt)],
                ]} />
                <p class="muted">Saved setup policy can be newer than this publication. Source indexing and semantic work run separately.</p>
                <div class="actions"><button type="button" onClick={() => props.go({ workspace: 'manage', tab: 'overview' })}>Review semantic setup</button>
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
                    <button type="button" key={journey.id} class="card journey" onClick={() => props.go(journey.route)}>
                        <WorkspaceIcon name={journey.route.workspace} /><strong>{journey.title}</strong><span>{journey.text}</span>
                    </button>
                ))}
            </div>
        </section>
    );
}
