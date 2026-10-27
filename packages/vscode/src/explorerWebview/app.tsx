// SPDX-License-Identifier: Apache-2.0
import { useEffect, useMemo, useRef, useState } from 'preact/hooks';
import { ExplorerViewState, ProjectSummary } from '../explorerContracts';
import { RECIPE_SCHEMA, type SearchRecipe } from '../explorerStrategies';
import { formatRoute, parseRoute, Route, ROUTE_KEYS, WORKSPACES } from '../explorerRoutes';
import { headline } from '../explorerStatus';
import { notify, onHost, ready, request, savedState, saveState } from './api';
import { Facts } from './components';
import { ago, observed, shortToken } from './format';
import { Inbox } from './inbox';
import { Overview } from './overview';
import { reportRevisions } from './strategyStore';
import { WorkspaceView } from './workspaces';

interface Persisted { route?: string; lens?: string }

export function App() {
    const persisted = savedState<Persisted>() ?? {};
    const [state, setState] = useState<ExplorerViewState | undefined>();
    const [route, setRoute] = useState<Route>(parseRoute(persisted.route ?? '#overview') ?? { workspace: 'overview' });
    const [palette, setPalette] = useState(false);
    const [pill, setPill] = useState(false);
    const [incoming, setIncoming] = useState<{ query: string; serial: number; recipe?: SearchRecipe; run?: boolean }>();
    const [inbox, setInbox] = useState(false);
    const [inspectRef, setInspectRef] = useState<string>();
    const [publications, setPublications] = useState<{ token: string; observedAt: number }[]>([]);

    useEffect(() => {
        const stop = onHost(message => {
            if (message.type === 'state') {
                setState(message.state);
                const token = message.state.status?.publication;
                if (token) { setPublications(prior => prior.some(entry => entry.token === token) ? prior : [{ token, observedAt: Date.now() }, ...prior].slice(0, 20)); }
            }
            else if (message.type === 'navigate') { const next = parseRoute(message.route); if (next) { setRoute(next); } }
            else if (message.type === 'inbox') { setInbox(true); }
            else if (message.type === 'search') { setRoute({ workspace: 'search', tab: 'query' }); setIncoming(prior => ({ query: message.query, serial: (prior?.serial ?? 0) + 1 })); }
        });
        ready();
        reportRevisions();
        const keys = (event: KeyboardEvent) => {
            if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') { event.preventDefault(); setPalette(open => !open); }
            if (event.key === 'Escape') { setPalette(false); setPill(false); }
        };
        window.addEventListener('keydown', keys);
        return () => { stop(); window.removeEventListener('keydown', keys); };
    }, []);

    useEffect(() => { saveState({ ...savedState<Persisted>(), route: formatRoute(route) }); }, [route]);

    const go = (next: Route) => { setRoute(next); setPalette(false); notify('nav.go', { route: formatRoute(next) }); };

    if (!state) { return <main class="airk-explorer loading"><p>Connecting to the AIRK extension host…</p></main>; }
    const project = state.projects.find(entry => entry.project === state.selectedProject);
    const status = headline(state.connection, state.status, state.projects.length);
    return (
        <div class="airk-explorer">
            <header class="topbar">
                <strong class="brand">AIRK Explorer</strong>
                <ProjectPicker projects={state.projects} selected={state.selectedProject} />
                <button type="button" class={`pill ${status.severity}`} aria-expanded={pill} onClick={() => setPill(open => !open)}
                    title="Service, published index, source work and semantic coverage">{status.word}</button>
                <span class="spacer" />
                <button type="button" class="ghost" onClick={() => setPalette(true)} title="Go to a screen (Ctrl/⌘+K)">Go to… <kbd>Ctrl K</kbd></button>
                <button type="button" class={state.suggestions?.open ? '' : 'ghost'} aria-expanded={inbox} onClick={() => setInbox(open => !open)}
                    title="Drafts proposed by coding agents">Suggestions <span aria-live="polite">{state.suggestions?.open ?? 0}</span></button>
                <button type="button" class="ghost" onClick={() => go({ workspace: 'help' })}>Help</button>
            </header>
            {pill && <StatusPopover state={state} project={project} onClose={() => setPill(false)} />}
            {state.notice && <p class="notice" role="alert">{state.notice.message}</p>}
            {inbox && <Inbox view={state.suggestions} close={() => setInbox(false)}
                applySearch={search => {
                    const recipe: SearchRecipe = { schema: RECIPE_SCHEMA, name: 'agent draft', query: search.query, mode: search.mode, ...(search.pathPrefix ? { pathPrefix: search.pathPrefix } : {}),
                        limit: search.limit, maxBytes: search.maxBytes, fullChunks: search.fullChunks };
                    setIncoming(prior => ({ query: search.query, recipe, run: false, serial: (prior?.serial ?? 0) + 1 })); go({ workspace: 'search', tab: 'query' });
                }}
                goStudio={() => go({ workspace: 'strategies', tab: 'studio' })} goPlanner={() => go({ workspace: 'strategies', tab: 'planner' })} />}
            <div class="frame">
                <Rail route={route} state={state} go={go} />
                <main class="content" aria-label={WORKSPACES.find(entry => entry.id === route.workspace)?.title}>
                    {route.workspace === 'overview'
                        ? <Overview state={state} project={project} go={go} />
                        : <WorkspaceView route={route} state={state} go={go} incoming={incoming} inspectRef={inspectRef} publications={publications}
                            inspect={ref => { setInspectRef(ref); go({ workspace: 'evidence', tab: 'inspector' }); }}
                            runRecipe={recipe => { setIncoming(prior => ({ query: recipe.query, recipe, serial: (prior?.serial ?? 0) + 1 })); go({ workspace: 'search', tab: 'query' }); }} />}
                </main>
            </div>
            {palette && <Palette state={state} go={go} close={() => setPalette(false)} />}
        </div>
    );
}

function ProjectPicker(props: { projects: readonly ProjectSummary[]; selected?: string }) {
    if (!props.projects.length) { return <span class="muted">No registered project</span>; }
    return (
        <label class="picker">
            <span class="sr-only">Project</span>
            <select value={props.selected ?? ''} onChange={event => {
                const project = (event.target as HTMLSelectElement).value;
                if (/^[0-9a-f]{64}$/.test(project)) { void request('project.select', { project }).catch(() => undefined); }
            }}>
                {props.projects.map(entry => <option key={entry.project} value={entry.project} title={entry.collection}>{entry.name}{entry.current ? ' (open)' : ''}</option>)}
            </select>
        </label>
    );
}

function StatusPopover(props: { state: ExplorerViewState; project?: ProjectSummary; onClose(): void }) {
    const { state, project } = props, status = state.status;
    return (
        <section class="popover" role="dialog" aria-label="AIRK status">
            <Facts rows={[
                ['Service connection', state.connection],
                ['Project', project ? `${project.collection} (${project.root || project.name})` : 'none selected'],
                ['Published index', status?.indexState ?? 'not observed'],
                ['Publication', <span title={status?.publication}>{shortToken(status?.publication)}</span>],
                ['Source work (scheduler)', status?.freshness ?? 'not observed'],
                ['Live source verified', observed(status?.liveSourceVerified)],
                ['Semantic coverage', status ? `${status.semantic}${status.semanticSelected !== undefined ? ` · ${observed(status.semanticCompleted)} / ${observed(status.semanticSelected)}` : ''}` : 'not observed'],
                ['Runtime versions match', observed(status?.versionsMatch)],
                ['Observed', ago(status?.observedAt)],
            ]} />
            <p class="muted">Health, published readiness, source freshness and semantic coverage are separate observations. Ready is not proof that saved files still match the index.</p>
            <div class="actions">
                <button type="button" onClick={() => { void request('status.refresh', {}).catch(() => undefined); }}>Refresh</button>
                <button type="button" class="ghost" onClick={props.onClose}>Close</button>
            </div>
        </section>
    );
}

function Rail(props: { route: Route; state: ExplorerViewState; go(route: Route): void }) {
    const preview = (id: string) => {
        const workspace = WORKSPACES.find(entry => entry.id === id)!;
        const keys = workspace.tabs.length ? workspace.tabs.map(tab => `${id}/${tab.id}`) : [id];
        return keys.every(key => props.state.features[key] !== 'implemented');
    };
    const item = (id: string) => {
        const workspace = WORKSPACES.find(entry => entry.id === id)!;
        const current = props.route.workspace === workspace.id;
        return (
            <li key={id}><button type="button" aria-current={current ? 'page' : undefined} class={current ? 'current' : ''}
                onClick={() => props.go({ workspace: workspace.id, ...(workspace.tabs[0] ? { tab: workspace.tabs[0].id } : {}) })}>
                {workspace.title}{preview(id) && <span class="tag">preview</span>}
            </button></li>
        );
    };
    return (
        <nav class="rail" aria-label="Explorer workspaces">
            <ul>{WORKSPACES.filter(entry => entry.placement === 'main').map(entry => item(entry.id))}</ul>
            <ul class="pinned">{WORKSPACES.filter(entry => entry.placement === 'pinned').map(entry => item(entry.id))}</ul>
        </nav>
    );
}

function Palette(props: { state: ExplorerViewState; go(route: Route): void; close(): void }) {
    const [query, setQuery] = useState('');
    const [index, setIndex] = useState(0);
    const input = useRef<HTMLInputElement>(null);
    useEffect(() => { input.current?.focus(); }, []);
    const entries = useMemo(() => ROUTE_KEYS.map(key => {
        const route = parseRoute(key)!;
        const workspace = WORKSPACES.find(entry => entry.id === route.workspace)!;
        const tab = workspace.tabs.find(entry => entry.id === route.tab);
        return { key, route, label: tab ? `${workspace.title} › ${tab.title}` : workspace.title, availability: props.state.features[key] };
    }).filter(entry => entry.label.toLowerCase().includes(query.toLowerCase())), [query, props.state.features]);
    const choose = (position: number) => { const entry = entries[position]; if (entry) { props.go(entry.route); } };
    return (
        <div class="scrim" onClick={props.close}>
            <section class="palette" role="dialog" aria-label="Go to screen" onClick={event => event.stopPropagation()}>
                <input ref={input} value={query} aria-label="Screen name" placeholder="Type a screen name"
                    onInput={event => { setQuery((event.target as HTMLInputElement).value); setIndex(0); }}
                    onKeyDown={event => {
                        if (event.key === 'ArrowDown') { event.preventDefault(); setIndex(value => Math.min(value + 1, entries.length - 1)); }
                        if (event.key === 'ArrowUp') { event.preventDefault(); setIndex(value => Math.max(value - 1, 0)); }
                        if (event.key === 'Enter') { event.preventDefault(); choose(index); }
                    }} />
                <ul role="listbox">
                    {entries.map((entry, position) => (
                        <li key={entry.key} role="option" aria-selected={position === index} class={position === index ? 'active' : ''} onClick={() => choose(position)}>
                            {entry.label}{entry.availability !== 'implemented' && <span class="tag">{entry.availability}</span>}
                        </li>
                    ))}
                    {!entries.length && <li class="muted">No screen matches.</li>}
                </ul>
            </section>
        </div>
    );
}
