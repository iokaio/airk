// SPDX-License-Identifier: Apache-2.0
import { useState } from 'preact/hooks';
import { ExplorerViewState, ProjectSummary } from '../explorerContracts';
import { Route } from '../explorerRoutes';
import { request, savedState, saveState } from './api';
import { Facts } from './components';
import { ago, observed, shortToken } from './format';

const JOURNEYS: { id: string; title: string; text: string; route: Route }[] = [
    { id: 'explore', title: 'Explore', text: 'Search code and documents; every hit is a citation.', route: { workspace: 'search', tab: 'query' } },
    { id: 'investigate', title: 'Investigate', text: 'Inspect declarations, references and candidate relationships.', route: { workspace: 'code', tab: 'symbols' } },
    { id: 'review', title: 'Review', text: 'Check changed files against indexed facts.', route: { workspace: 'review', tab: 'changes' } },
    { id: 'prepare', title: 'Prepare', text: 'Compile bounded, cited evidence into a prompt.', route: { workspace: 'strategies', tab: 'studio' } },
    { id: 'handoff', title: 'Hand off', text: 'Save a plan file an agent can follow.', route: { workspace: 'strategies', tab: 'planner' } },
    { id: 'maintain', title: 'Maintain', text: 'Index, repair, storage, agents and usage.', route: { workspace: 'manage', tab: 'projects' } },
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
    const [lens, setLens] = useState<string>(() => { const value = savedState<{ lens?: unknown }>()?.lens; return typeof value === 'string' && Object.hasOwn(LENSES, value) ? value : 'developer'; });
    const order = LENSES[lens] ?? LENSES.developer;
    const first = !state.trusted ? 'Trust this workspace in VS Code before AIRK can connect or read evidence.'
        : !state.projects.length ? 'No AIRK project is registered for this window yet. Registration and indexing start automatically for trusted folders, or use Management.'
        : state.connection === 'disconnected' ? 'The AIRK service is not connected. Open Management › Service to start or reconnect it.'
        : state.connection === 'incompatible' ? 'The selected AIRK runtime does not match this extension. Review the runtime in Management › Service.'
        : undefined;
    return (
        <section class="workspace">
            <h1>Overview</h1>
            {first && <p class="callout">{first}</p>}
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
                <article class="card">
                    <h2>Published index</h2>
                    <Facts rows={[
                        ['Service', state.connection],
                        ['Index', status?.indexState ?? 'not observed'],
                        ['Publication', <span title={status?.publication}>{shortToken(status?.publication)}</span>],
                        ['Files · chunks', status ? `${observed(status.files)} · ${observed(status.chunks)}` : 'not observed'],
                        ['Source work', status?.freshness ?? 'not observed'],
                        ['Semantic coverage', status ? `${status.semantic}${status.vectorsEnabled === false ? ' (vectors off)' : ''}` : 'not observed'],
                        ['Observed', ago(status?.observedAt)],
                    ]} />
                    <p class="muted">Counts come from AIRK. Ready means a publication exists; it is not proof that saved files still match.</p>
                    <button type="button" onClick={() => { void request('status.refresh', {}).catch(() => undefined); }}>Refresh status</button>
                </article>
            </div>
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
                        <strong>{journey.title}</strong><span>{journey.text}</span>
                    </button>
                ))}
            </div>
        </section>
    );
}
