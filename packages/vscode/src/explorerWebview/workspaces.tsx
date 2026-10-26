// SPDX-License-Identifier: Apache-2.0
import { JSX } from 'preact';
import { ExplorerViewState } from '../explorerContracts';
import { Route, routeKey, WORKSPACES } from '../explorerRoutes';
import { OutlineScreen, ReferencesScreen, SymbolsScreen } from './code';
import { DocumentFactsScreen } from './docs';
import { ProjectsScreen } from './manage';
import { SearchScreen } from './search';

export interface ScreenProps { route: Route; state: ExplorerViewState; go(route: Route): void; incoming?: { query: string; serial: number } }

/** Screens backed by real AIRK operations, keyed by route. Everything else is labeled, never simulated. */
export const SCREENS: Record<string, (props: ScreenProps) => JSX.Element> = {
    'search/query': SearchScreen,
    'manage/projects': ProjectsScreen,
    'code/symbols': SymbolsScreen,
    'code/references': ReferencesScreen,
    'code/outline': OutlineScreen,
    'docs/facts': DocumentFactsScreen,
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
