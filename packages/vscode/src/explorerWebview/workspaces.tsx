// SPDX-License-Identifier: Apache-2.0
import { h, type ComponentChildren, type JSX } from 'preact';
import {useRef,useState} from 'preact/hooks';
import { ExplorerViewState } from '../explorerContracts';
import type { SearchRecipe } from '../explorerStrategies';
import type { NavigationFilters } from '../explorerDrafts';
import { parseRoute, Route, routeKey, WORKSPACES } from '../explorerRoutes';
import { OutlineScreen, ReferencesScreen, SymbolsScreen } from './code';
import { DocumentFactsScreen } from './docs';
import { ChangesScreen, CheckerScreen, InspectorScreen } from './evidence';
import { HelpScreen } from './help';
import { ManagementUnavailable, MANAGEMENT_ROUTES, type AdminLink } from './adminLinks';
import { RecipesScreen, SearchScreen } from './search';
import { PlannerScreen } from './planner';
import { ReviewChangesScreen, ReviewImpactScreen, ReviewTestsScreen } from './review';
import { LibraryScreen, StudioScreen } from './strategies';
import { IndexingScreen } from './indexing';
import { ActivityScreen, UsageScreen } from './manageViews';
import { AgentsScreen } from './agents';
import { editorDestination, MAIN_DESTINATIONS, evidenceToolsPresentation, isEvidenceToolRoute, type EvidenceToolsNode } from '../../../ui/src/navigation';
import { SharedManagement } from './sharedManagement';

export interface ScreenProps {
    managementTarget?: { link: AdminLink; serial: number };
    scopePrefill?: { id: string; serial: number };
    openScope?(id: string): void;
    lastEvidenceRoute?: string;
    route: Route;
    state: ExplorerViewState;
    go(route: Route): void;
    incoming?: { query: string; serial: number; recipe?: SearchRecipe; run?: boolean };
    navigation?: { route: string; filters: NavigationFilters; serial: number };
    /** Open Search with a saved recipe's arguments and run it. */
    runRecipe(recipe: SearchRecipe): void;
    /** Open Evidence › Inspector for a host-issued citation handle. */
    inspect(ref: string): void;
    inspectRef?: string;
    /** Publication tokens observed in this session, newest first (Changes baselines). */
    publications: readonly { token: string; observedAt: number }[];
}

/** Screens backed by real AIRK operations, keyed by route. Everything else is labeled, never simulated. */
export const SCREENS: Record<string, (props: ScreenProps) => JSX.Element> = {
    'indexing/window': IndexingScreen,
    'indexing/library': IndexingScreen,
    'indexing/projects': IndexingScreen,
    'manage/overview': ManagementUnavailable,
    'search/query': SearchScreen,
    'search/recipes': RecipesScreen,
    'search/tools': EvidenceToolsScreen,
    // Older runtimes can still render the supported read-only editor views.
    'manage/activity': ActivityScreen,
    'manage/repair': ManagementUnavailable,
    'manage/storage': ManagementUnavailable,
    'manage/service': ManagementUnavailable,
    'manage/agents': AgentsScreen,
    'manage/usage': UsageScreen,
    'manage/logs': ManagementUnavailable,
    'code/symbols': SymbolsScreen,
    'code/references': ReferencesScreen,
    'code/outline': OutlineScreen,
    'docs/facts': DocumentFactsScreen,
    'evidence/inspector': InspectorScreen,
    'evidence/checker': CheckerScreen,
    'evidence/changes': ChangesScreen,
    'review/changes': ReviewChangesScreen,
    'review/impact': ReviewImpactScreen,
    'review/tests': ReviewTestsScreen,
    'strategies/library': LibraryScreen,
    'strategies/studio': StudioScreen,
    'strategies/planner': PlannerScreen,
    'help': HelpScreen,
};

function EvidenceToolsScreen(props:ScreenProps):JSX.Element {
    const show=(value:EvidenceToolsNode|string):ComponentChildren=>typeof value==='string'?value:h(value.tag,
        {...(value.href?{href:value.href}:{}),...(value.route?{type:'button',onClick:()=>{const route=parseRoute(value.route!);if(route)props.go(route);}}:{})},...value.children.map(show));
    return <div class="evidence-tools" aria-label="Evidence tools hub">{show(evidenceToolsPresentation('editor',props.lastEvidenceRoute))}</div>;
}

export function WorkspaceView(props: ScreenProps) {
    const workspace = WORKSPACES.find(entry => entry.id === props.route.workspace)!;
    const key = routeKey(props.route);
    const [advancedTools, setAdvancedTools] = useState(false);
    const queryPage = key === 'search/query';
    const activeEvidence=isEvidenceToolRoute(key),hub=key==='search/tools';
    const held=useRef<ScreenProps>();
    const sameOwner=(prior:ScreenProps)=>prior.state.selectedProject===props.state.selectedProject&&prior.state.selectionEpoch===props.state.selectionEpoch
        &&prior.state.trusted===props.state.trusted&&prior.state.nativeExecutionEnabled===props.state.nativeExecutionEnabled;
    if(held.current&&!sameOwner(held.current))held.current=undefined;
    if(activeEvidence)held.current=props;else if(!hub)held.current=undefined;
    const retained=activeEvidence?props:hub?held.current:undefined;
    const RetainedScreen=retained&&props.state.features[routeKey(retained.route)]==='implemented'?SCREENS[routeKey(retained.route)]:undefined;
    const Screen = props.state.features[key] === 'implemented' ? SCREENS[key] : undefined;

    const tabs = workspace.tabs;
    const activeTab = props.route.tab;
    const destination = editorDestination(key);
    const title = activeEvidence?workspace.title:MAIN_DESTINATIONS.find(entry => entry.id === destination)?.title ?? workspace.title;
    // Connect remains the primary Agents task. The shared view supplies manual setup.
    const sharedPage = queryPage && advancedTools ? 'search' : ({ 'manage/overview': 'overview', 'manage/agents': 'agents', 'manage/activity': 'activity', 'manage/repair': 'recovery', 'manage/storage': 'storage',
        'manage/service': 'service', 'manage/usage': 'usage', 'manage/logs': 'logs',
        'indexing/library': 'index-library' } as const)[key as 'manage/activity'];
    const target = props.managementTarget?.link.page === sharedPage ? props.managementTarget : undefined;
    return (
        <section class="workspace">
            <div class="workspace-heading"><div><p class="eyebrow">{activeEvidence?'Search → Evidence tools':'AIRK workspace'}</p><h1 tabIndex={-1}>{title}</h1></div>
                {activeEvidence&&<button type="button" class="ghost" onClick={()=>props.go({workspace:'search',tab:'tools'})}>Back to Evidence tools</button>}
            </div>
            <div>
            {tabs.length > 0 && (
                <div class="tabs" role="tablist" aria-orientation="horizontal" aria-label={`${title} views`}>
                    {tabs.map(tab => {
                        const selected = tab.id === activeTab;
                        const available = props.state.features[`${workspace.id}/${tab.id}`];
                        return (
                            <button type="button" role="tab" key={tab.id} id={`tab-${workspace.id}-${tab.id}`} aria-controls={`panel-${workspace.id}`} tabIndex={selected ? 0 : -1} aria-selected={selected} class={selected ? 'selected' : ''}
                                onKeyDown={event => {
                                    const at = tabs.findIndex(entry => entry.id === tab.id);
                                    const next = event.key === 'ArrowRight' ? (at + 1) % tabs.length : event.key === 'ArrowLeft' ? (at + tabs.length - 1) % tabs.length
                                        : event.key === 'Home' ? 0 : event.key === 'End' ? tabs.length - 1 : undefined;
                                    if (next !== undefined) { event.preventDefault(); props.go({ workspace: workspace.id, tab: tabs[next].id }); document.getElementById(`tab-${workspace.id}-${tabs[next].id}`)?.focus(); }
                                }}
                                onClick={() => props.go({ workspace: workspace.id, tab: tab.id })}>
                                {tab.title}{available !== 'implemented' && <span class="tag">preview</span>}
                            </button>
                        );
                    })}
                </div>
            )}
            <div class="workspace-content" role={activeTab ? "tabpanel" : undefined} id={`panel-${workspace.id}`} aria-labelledby={activeTab ? `tab-${workspace.id}-${activeTab}` : undefined}>
            {queryPage && Screen && <Screen key={`${key}:${props.state.selectedProject}:${props.state.selectionEpoch}:${props.navigation?.route === key ? props.navigation.serial : ''}`} {...props} />}
            {queryPage && <button type="button" class="ghost" aria-expanded={advancedTools}
                aria-controls="management-tasks" onClick={() => setAdvancedTools(open => !open)}>Advanced native tools</button>}
            {props.state.adminManagement ? <section id="management-tasks" hidden={!sharedPage}>
                {key === 'manage/agents' && props.state.adminManagement.available && <AgentsScreen {...props} />}
                <SharedManagement state={props.state} page={sharedPage} target={target} active={!!sharedPage} onUseScope={props.openScope} onPageChange={page => {
                    props.go(MANAGEMENT_ROUTES[page]);
                }} />
            </section> : sharedPage ? <p id="management-tasks" role="status">These controls require a compatible local AIRK Service, a trusted workspace, and enabled native execution.</p> : null}
            {retained&&RetainedScreen?<section hidden={hub} data-retained-evidence-view={routeKey(retained.route)}><RetainedScreen key={`${routeKey(retained.route)}:${retained.state.selectedProject}:${retained.state.selectionEpoch}:${retained.navigation?.route===routeKey(retained.route)?retained.navigation.serial:''}`} {...retained}/></section>:null}
            {!activeEvidence&&!queryPage&&(sharedPage && props.state.adminManagement?.available ? null
                : Screen ? <Screen key={workspace.id==='indexing'?'indexing':`${key}:${props.state.selectedProject}:${props.state.selectionEpoch}:${workspace.id !== 'manage' && props.navigation?.route === key ? props.navigation.serial : ''}`} {...props} /> : <Planned routeKey={key} availability={props.state.features[key] ?? 'unsupported'} />)}</div>
            </div>
        </section>
    );
}

function Planned(props: { routeKey: string; availability: string }) {
    return (
        <article class="card planned">
            <h2>Not available in this build</h2>
            <p>This screen is designed in the AIRK Explorer plan but is not implemented yet (<code>{props.availability}</code>).
                Nothing here is simulated: no sample data, no pretend results.</p>
            <p class="muted">Route <code>{props.routeKey}</code></p>
        </article>
    );
}
