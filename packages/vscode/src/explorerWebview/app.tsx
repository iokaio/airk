// SPDX-License-Identifier: Apache-2.0
import { useEffect, useMemo, useRef, useState } from 'preact/hooks';
import { ExplorerViewState, ProjectSummary } from '../explorerContracts';
import { RECIPE_SCHEMA, type SearchRecipe } from '../explorerStrategies';
import { formatRoute, parseRoute, Route, routeKey, ROUTE_KEYS, WORKSPACES } from '../explorerRoutes';
import catalog from 'airk-explorer-help';
import { headline } from '../explorerStatus';
import { notify, onHost, ready, request, savedState, saveState } from './api';
import { BrandMark, Facts, useDialogFocus, WorkspaceIcon } from './components';
import { ago, observed, shortToken } from './format';
import { Inbox } from './inbox';
import { Overview } from './overview';
import { reportRevisions, selectDraftProject } from './strategyStore';
import { WorkspaceView } from './workspaces';
import type { NavigationFilters } from '../explorerDrafts';
import { friendlyError } from '../explorerErrors';
import { acceptSearchSession, selectSearchProject } from './searchStore';

interface Persisted { route?: string; lens?: string; themed?: boolean }

export function App() {
    const persisted = savedState<Persisted>() ?? {};
    const [state, setState] = useState<ExplorerViewState | undefined>();
    const [route, setRoute] = useState<Route>(parseRoute(persisted.route ?? '#overview') ?? { workspace: 'overview' });
    const [palette, setPalette] = useState(false);
    const [pill, setPill] = useState(false);
    const [themed, setThemed] = useState(persisted.themed !== false);
    const [choosingSource, setChoosingSource] = useState(false), [sourceError, setSourceError] = useState<string>();
    const [incoming, setIncoming] = useState<{ query: string; serial: number; recipe?: SearchRecipe; run?: boolean }>();
    const [inbox, setInbox] = useState(false);
    const [navigation, setNavigation] = useState<{ route: string; filters: NavigationFilters; serial: number }>();
    const [inspectRef, setInspectRef] = useState<string>();
    const [publications, setPublications] = useState<{ token: string; observedAt: number }[]>([]);
    const owner = useRef<string>();

    useEffect(() => {
        const stop = onHost(message => {
            if (message.type === 'state') {
                const nextOwner = `${message.state.selectedProject}:${message.state.selectionEpoch}`;
                if (owner.current !== undefined && owner.current !== nextOwner) { setInspectRef(undefined); setPublications([]); setIncoming(undefined); setNavigation(undefined); }
                owner.current = nextOwner;
                selectDraftProject(message.state.selectedProject, message.state.selectionEpoch);
                selectSearchProject(message.state.selectedProject, message.state.selectionEpoch);
                setState(message.state);
                const token = message.state.status?.publication;
                if (token) { setPublications(prior => prior.some(entry => entry.token === token) ? prior : [{ token, observedAt: Date.now() }, ...prior].slice(0, 20)); }
            }
            else if (message.type === 'search.session') { acceptSearchSession(message.session); }
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
    // Nested project views share this observer; child unmounts cannot turn it off.
    useEffect(() => {
        const open = route.workspace === 'manage' || route.workspace === 'overview';
        void request('manage.observe', { open }).catch(() => undefined);
        return () => { if (open) { void request('manage.observe', { open: false }).catch(() => undefined); } };
    }, [route.workspace]);

    const go = (next: Route) => { setRoute(next); setPalette(false); notify('nav.go', { route: formatRoute(next) }); };
    const chooseSource = async () => {
        setChoosingSource(true); setSourceError(undefined);
        try { await request('source.choose', {}); }
        catch (failure) { setSourceError(failure instanceof Error ? failure.message : String(failure)); }
        finally { setChoosingSource(false); }
    };
    const openHelp = async () => {
        const key = routeKey(route);
        try {
            const destination = await request<'screen' | 'general' | null>('help.choose', { route: key });
            if (!destination) { return; }
            const topic = destination === 'screen' ? catalog.topics.find(entry => entry.route === key) : undefined;
            go({ workspace: 'help', ...(topic ? { focus: topic.id.replaceAll('.', '-') } : {}) });
        } catch (failure) { setSourceError(failure instanceof Error ? failure.message : String(failure)); }
    };

    if (!state) { return <main class="airk-explorer loading"><p>Connecting to the AIRK extension host…</p><button type="button" onClick={ready}>Retry connection</button>
        <p class="muted">If it remains unavailable, close and reopen Explorer after the extension host restarts.</p></main>; }
    const project = state.projects.find(entry => entry.project === state.selectedProject);
    const managed = ['manage','overview'].includes(route.workspace) ? state.management?.projects.find(entry => entry.summary.project === state.selectedProject) : undefined;
    const fallback = headline(state.connection, state.status, state.projects.length);
    const status = state.extensionUpdate ? { word: 'Reload to finish update', severity: 'warn' }
        : managed && state.connection !== 'untrusted' && state.connection !== 'incompatible' ? { word: managed.health.title, severity: managed.health.severity } : fallback;
    return (
        <div class={`airk-explorer${themed ? ' airk-themed' : ''}`}>
            <header class="topbar">
                <strong class="brand" title={`Running AIRK ${state.extensionVersion}`}><BrandMark /><span>AIRK <span class="brand-subtitle">Explorer</span></span></strong>
                <ProjectPicker projects={state.projects} selected={state.selectedProject} loading={state.connection === 'connecting'} />
                <button type="button" class={`pill ${status.severity}`} aria-expanded={pill} onClick={() => setPill(open => !open)}
                    title="Service, published index, source work and semantic coverage">{status.word}</button>
                <div class="topbar-actions">
                <button type="button" class="ghost" onClick={() => setPalette(true)} title="Go to a screen (Ctrl/⌘+K)">Go to… <kbd>Ctrl K</kbd></button>
                <button type="button" class={state.suggestions?.open ? '' : 'ghost'} aria-expanded={inbox} onClick={() => setInbox(open => !open)}
                    title="Drafts proposed by coding agents">Suggestions <span aria-live="polite">{state.suggestions?.open ?? 0}</span></button>
                <button type="button" class="ghost" aria-haspopup="listbox" onClick={() => void openHelp()}>Help <span aria-hidden="true">⌄</span></button>
                <button type="button" class="ghost" onClick={() => go({ workspace: 'help', focus: 'about-airk' })}>About</button>
                <button type="button" class={`theme-toggle ghost${themed ? ' active' : ''}`} aria-pressed={themed}
                    title={themed ? 'AIRK design is on. Switch to the VS Code theme.' : 'Use the AIRK design.'}
                    onClick={() => { const next = !themed; setThemed(next); saveState({ ...savedState<Persisted>(), themed: next }); }}>
                    <span class="toggle-track" aria-hidden="true"><span /></span>Themed
                </button>
                </div>
            </header>
            <div class="source-context"><span class="eyebrow">Source file</span><code title={state.sourceFile}>{state.sourceFile ?? 'No file selected'}</code>
                <button class="link" disabled={choosingSource || !state.trusted || !state.selectedProject} onClick={() => void chooseSource()}>Choose file…</button>
                <small>Choose a file for Code › Outline and Evidence › Inspector. These views use the saved index.</small></div>
            {sourceError && <p class="notice" role="alert">{sourceError}</p>}
            {pill && <StatusPopover state={state} project={project} onClose={() => setPill(false)} />}
            {state.notice && <div class="notice" role="alert"><p>{friendlyError(state.notice)}</p><button class="ghost" onClick={() => go({ workspace: 'help', focus: 'recovering-errors' })}>Recovery help</button></div>}
            {inbox && <Inbox view={state.suggestions} close={() => setInbox(false)}
                applyNavigation={body => { const next = parseRoute(body.route); if (next) { setNavigation(prior => ({ route: body.route, filters: body.filters, serial: (prior?.serial ?? 0) + 1 })); go(next); } }}
                applySearch={search => {
                    const recipe: SearchRecipe = { schema: RECIPE_SCHEMA, name: 'agent draft', query: search.query, mode: search.mode, ...(search.pathPrefix ? { pathPrefix: search.pathPrefix } : {}),
                        limit: search.limit, maxBytes: search.maxBytes, fullChunks: search.fullChunks };
                    setIncoming(prior => ({ query: search.query, recipe, run: false, serial: (prior?.serial ?? 0) + 1 })); go({ workspace: 'search', tab: 'query' });
                }}
                goStudio={() => go({ workspace: 'strategies', tab: 'studio' })} goPlanner={() => go({ workspace: 'strategies', tab: 'planner' })} go={go} />}
            <div class="frame">
                <Rail route={route} state={state} go={go} />
                <main class="content" aria-label={WORKSPACES.find(entry => entry.id === route.workspace)?.title}>
                    {state.startup&&!state.extensionUpdate&&<StartupProgress startup={state.startup} />}
                    {state.extensionUpdate ? <section class="workspace"><article class="card" role="status">
                        <h1>Reload to finish the AIRK update</h1>
                        <p>AIRK {state.extensionUpdate.installedVersion} is installed. This window is still running AIRK {state.extensionUpdate.runningVersion}.</p>
                        <p>Reload this window to load the updated interface and finish runtime activation. Saved indexing work is retained by the AIRK service.</p>
                        <button type="button" onClick={() => { void request('extension.reload', {}).catch(() => undefined); }}>Reload to finish update</button>
                    </article></section> : route.workspace === 'overview'
                        ? <Overview state={state} project={project} go={go} />
                        : <WorkspaceView route={route} state={state} go={go} incoming={incoming} navigation={navigation} inspectRef={inspectRef} publications={publications}
                            inspect={ref => { setInspectRef(ref); go({ workspace: 'evidence', tab: 'inspector' }); }}
                            runRecipe={recipe => { setIncoming(prior => ({ query: recipe.query, recipe, serial: (prior?.serial ?? 0) + 1 })); go({ workspace: 'search', tab: 'query' }); }} />}
                </main>
            </div>
            {palette && <Palette state={state} go={go} close={() => setPalette(false)} />}
        </div>
    );
}

function StartupProgress({startup}:{startup:NonNullable<ExplorerViewState['startup']>}) {
    const [now,setNow]=useState(Date.now());
    useEffect(()=>{if(startup.failed)return;const timer=setInterval(()=>setNow(Date.now()),1000);return ()=>clearInterval(timer);},[startup.startedAt,startup.failed]);
    return <aside class="callout" role="status"><strong>{startup.failed?'Connection needs attention':'Connecting to AIRK'}</strong>
        <p>{startup.message}</p>{!startup.failed&&<p>Elapsed: {Math.max(0,Math.floor((now-startup.startedAt)/1000))} seconds. You can keep using Explorer while AIRK connects.</p>}
        <button class="ghost" onClick={()=>{void request('manage.command',{command:'airk.showExtensionOutput'}).catch(()=>undefined);}}>Show connection log</button></aside>;
}

function ProjectPicker(props: { projects: readonly ProjectSummary[]; selected?: string; loading?: boolean }) {
    if (!props.projects.length) { return <span class="muted">{props.loading ? 'Loading projects…' : 'No registered project'}</span>; }
    return (
        <label class="picker">
            <span class="sr-only">Project</span>
            <select value={props.selected ?? ''} onChange={event => {
                const project = (event.target as HTMLSelectElement).value;
                if (/^[0-9a-f]{64}$/.test(project)) { void request('project.select', { project }).catch(() => undefined); }
            }}>
                {props.projects.map(entry => <option key={entry.project} value={entry.project} disabled={!entry.current} title={entry.current ? entry.collection : 'Open this project in the workspace to select its evidence'}>{entry.name}{entry.current ? ' (open)' : ' (open workspace to select)'}</option>)}
            </select>
        </label>
    );
}

function StatusPopover(props: { state: ExplorerViewState; project?: ProjectSummary; onClose(): void }) {
    const dialog = useDialogFocus(props.onClose);
    const { state, project } = props, status = state.status;
    return (
        <section ref={dialog} class="popover" role="dialog" aria-modal="true" aria-label="AIRK status">
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
            <li key={id}><button type="button" title={workspace.title} aria-label={workspace.title} aria-current={current ? 'page' : undefined} class={current ? 'current' : ''}
                onClick={() => props.go({ workspace: workspace.id, ...(workspace.tabs[0] ? { tab: workspace.tabs[0].id } : {}) })}>
                <WorkspaceIcon name={id} /><span class="nav-label">{workspace.title}</span>{preview(id) && <span class="tag">preview</span>}
            </button></li>
        );
    };
    return (
        <nav class="rail" aria-label="Explorer workspaces">
            <div class="rail-caption" aria-hidden="true">Workspace</div>
            <ul>{WORKSPACES.filter(entry => entry.placement === 'main' || entry.placement === 'pinned').map(entry => item(entry.id))}</ul>
        </nav>
    );
}

function Palette(props: { state: ExplorerViewState; go(route: Route): void; close(): void }) {
    const dialog = useDialogFocus(props.close);
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
            <section ref={dialog} class="palette" role="dialog" aria-modal="true" aria-label="Go to screen" onClick={event => event.stopPropagation()}>
                <input ref={input} value={query} role="combobox" aria-expanded="true" aria-controls="screen-options" aria-activedescendant={entries[index] ? `screen-option-${index}` : undefined} aria-label="Screen name" placeholder="Type a screen name"
                    onInput={event => { setQuery((event.target as HTMLInputElement).value); setIndex(0); }}
                    onKeyDown={event => {
                        if (event.key === 'ArrowDown') { event.preventDefault(); setIndex(value => Math.min(value + 1, entries.length - 1)); }
                        if (event.key === 'ArrowUp') { event.preventDefault(); setIndex(value => Math.max(value - 1, 0)); }
                        if (event.key === 'Enter') { event.preventDefault(); choose(index); }
                    }} />
                <ul id="screen-options" role="listbox">
                    {entries.map((entry, position) => (
                        <li key={entry.key} id={`screen-option-${position}`} role="option" aria-selected={position === index} class={position === index ? 'active' : ''} onClick={() => choose(position)}>
                            {entry.label}{entry.availability !== 'implemented' && <span class="tag">{entry.availability}</span>}
                        </li>
                    ))}
                    {!entries.length && <li class="muted">No screen matches.</li>}
                </ul>
            </section>
        </div>
    );
}
