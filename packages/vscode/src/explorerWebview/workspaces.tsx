// SPDX-License-Identifier: Apache-2.0
import { JSX } from 'preact';
import { ExplorerViewState } from '../explorerContracts';
import type { SearchRecipe } from '../explorerStrategies';
import type { NavigationFilters } from '../explorerDrafts';
import { Route, routeKey, WORKSPACES } from '../explorerRoutes';
import { OutlineScreen, ReferencesScreen, SymbolsScreen } from './code';
import { DocumentFactsScreen } from './docs';
import { ChangesScreen, CheckerScreen, InspectorScreen } from './evidence';
import { HelpScreen } from './help';
import { ProjectsScreen } from './manage';
import { RecipesScreen, SearchScreen } from './search';
import { PlannerScreen } from './planner';
import { ReviewChangesScreen, ReviewImpactScreen, ReviewTestsScreen } from './review';
import { LibraryScreen, StudioScreen } from './strategies';

export interface ScreenProps {
    manageEmbedded?: boolean;
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
    'manage/overview': ProjectsScreen,
    'search/query': SearchScreen,
    'search/recipes': RecipesScreen,
    'manage/activity': ProjectsScreen,
    'manage/repair': ProjectsScreen,
    'manage/storage': ProjectsScreen,
    'manage/service': ProjectsScreen,
    'manage/agents': ProjectsScreen,
    'manage/usage': ProjectsScreen,
    'manage/logs': ProjectsScreen,
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

export function WorkspaceView(props: ScreenProps) {
    const workspace = WORKSPACES.find(entry => entry.id === props.route.workspace)!;
    const key = routeKey(props.route);
    const Screen = props.state.features[key] === 'implemented' ? SCREENS[key] : undefined;
    const management = workspace.id === 'manage';
    const tabs = workspace.tabs;
    const activeTab = props.route.tab;
    return (
        <section class="workspace">
            <div class="workspace-heading"><div><p class="eyebrow">AIRK workspace</p><h1>{workspace.title}</h1></div>
            </div>
            <div>
            {tabs.length > 0 && (
                <div class="tabs" role="tablist" aria-orientation="horizontal" aria-label={`${workspace.title} views`}>
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
            <div class="workspace-content" role={activeTab ? "tabpanel" : undefined} id={`panel-${workspace.id}`} aria-labelledby={activeTab ? `tab-${workspace.id}-${activeTab}` : undefined}>{Screen ? <Screen key={`${management ? 'manage' : key}:${props.state.selectedProject}:${props.state.selectionEpoch}:${!management && props.navigation?.route === key ? props.navigation.serial : ''}`} {...props} /> : <Planned routeKey={key} availability={props.state.features[key] ?? 'unsupported'} />}</div>
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
