// SPDX-License-Identifier: Apache-2.0
import { JSX } from 'preact';
import { ExplorerViewState } from '../explorerContracts';
import type { SearchRecipe } from '../explorerStrategies';
import { Route, routeKey, WORKSPACES } from '../explorerRoutes';
import { OutlineScreen, ReferencesScreen, SymbolsScreen } from './code';
import { DocumentFactsScreen } from './docs';
import { ChangesScreen, CheckerScreen, InspectorScreen } from './evidence';
import { HelpScreen } from './help';
import { ActivityScreen, AgentsScreen, IndexingScreen, LogsScreen, ProjectsScreen, RepairScreen, ServiceScreen, StorageScreen, UsageScreen } from './manage';
import { RecipesScreen, SearchScreen } from './search';
import { PlannerScreen } from './planner';
import { ReviewChangesScreen, ReviewImpactScreen, ReviewTestsScreen } from './review';
import { LibraryScreen, StudioScreen } from './strategies';

export interface ScreenProps {
    route: Route;
    state: ExplorerViewState;
    go(route: Route): void;
    incoming?: { query: string; serial: number; recipe?: SearchRecipe; run?: boolean };
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
    'search/query': SearchScreen,
    'search/recipes': RecipesScreen,
    'manage/projects': ProjectsScreen,
    'manage/activity': ActivityScreen,
    'manage/repair': RepairScreen,
    'manage/indexing': IndexingScreen,
    'manage/storage': StorageScreen,
    'manage/service': ServiceScreen,
    'manage/agents': AgentsScreen,
    'manage/usage': UsageScreen,
    'manage/logs': LogsScreen,
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
    return (
        <section class="workspace">
            <h1>{workspace.title}</h1>
            {workspace.tabs.length > 0 && (
                <div class="tabs" role="tablist" aria-label={`${workspace.title} views`}>
                    {workspace.tabs.map(tab => {
                        const selected = tab.id === props.route.tab;
                        const available = props.state.features[`${workspace.id}/${tab.id}`];
                        return (
                            <button type="button" role="tab" key={tab.id} aria-selected={selected} class={selected ? 'selected' : ''}
                                onClick={() => props.go({ workspace: workspace.id, tab: tab.id })}>
                                {tab.title}{available !== 'implemented' && <span class="tag">preview</span>}
                            </button>
                        );
                    })}
                </div>
            )}
            <div role="tabpanel">{Screen ? <Screen {...props} /> : <Planned routeKey={key} availability={props.state.features[key] ?? 'unsupported'} />}</div>
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
