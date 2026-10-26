// SPDX-License-Identifier: Apache-2.0
// Pure route manifest shared by the extension host, the Explorer webview and the
// Explorer Assistance server. Nothing outside this list is a route.

export type WorkspaceId = 'overview' | 'search' | 'code' | 'docs' | 'evidence' | 'review' | 'strategies' | 'manage' | 'help';

export interface TabDefinition { readonly id: string; readonly title: string }
export interface WorkspaceDefinition {
    readonly id: WorkspaceId;
    readonly title: string;
    /** Rail placement: Management is pinned at the bottom; Help opens as a drawer. */
    readonly placement: 'main' | 'pinned' | 'drawer';
    readonly tabs: readonly TabDefinition[];
}

export const WORKSPACES: readonly WorkspaceDefinition[] = [
    { id: 'overview', title: 'Overview', placement: 'main', tabs: [] },
    { id: 'search', title: 'Search', placement: 'main', tabs: [{ id: 'query', title: 'Query' }, { id: 'recipes', title: 'Recipes' }] },
    { id: 'code', title: 'Code', placement: 'main', tabs: [{ id: 'symbols', title: 'Symbols' }, { id: 'references', title: 'References' }, { id: 'outline', title: 'Outline' }] },
    { id: 'docs', title: 'Documents', placement: 'main', tabs: [{ id: 'facts', title: 'Facts' }] },
    { id: 'evidence', title: 'Evidence', placement: 'main', tabs: [{ id: 'inspector', title: 'Inspector' }, { id: 'checker', title: 'Checker' }, { id: 'changes', title: 'Changes' }] },
    { id: 'review', title: 'Review', placement: 'main', tabs: [{ id: 'changes', title: 'Changes' }, { id: 'impact', title: 'Impact' }, { id: 'tests', title: 'Tests' }] },
    { id: 'strategies', title: 'Strategies', placement: 'main', tabs: [{ id: 'library', title: 'Library' }, { id: 'studio', title: 'Studio' }, { id: 'planner', title: 'Planner' }] },
    { id: 'manage', title: 'Management', placement: 'pinned', tabs: [
        { id: 'projects', title: 'Projects' }, { id: 'activity', title: 'Activity' }, { id: 'repair', title: 'Repair' },
        { id: 'indexing', title: 'Indexing' }, { id: 'storage', title: 'Storage' }, { id: 'service', title: 'Service' },
        { id: 'agents', title: 'Agents' }, { id: 'usage', title: 'Usage' }, { id: 'logs', title: 'Logs' },
    ] },
    { id: 'help', title: 'Help', placement: 'drawer', tabs: [] },
];

export interface Route { readonly workspace: WorkspaceId; readonly tab?: string; readonly focus?: string }

const FOCUS = /^[a-z][a-z0-9-]{0,31}$/;

/** Canonical key used by the feature manifest and help topics, e.g. "search/query" or "overview". */
export function routeKey(route: Route): string {
    return route.tab ? `${route.workspace}/${route.tab}` : route.workspace;
}

/** Every addressable route key, in rail order. */
export const ROUTE_KEYS: readonly string[] = WORKSPACES.flatMap(workspace =>
    workspace.tabs.length ? workspace.tabs.map(tab => `${workspace.id}/${tab.id}`) : [workspace.id]);

/**
 * Parse "#workspace/tab?focus=x" (leading "#" optional). A workspace without a tab
 * resolves to its first tab. Unknown workspaces, tabs, parameters or malformed focus
 * values are rejected rather than coerced.
 */
export function parseRoute(text: string): Route | undefined {
    if (typeof text !== 'string' || text.length > 128) { return undefined; }
    const [pathPart, query, extra] = text.replace(/^#/, '').split('?');
    if (extra !== undefined) { return undefined; }
    const [workspaceId, tabId, more] = pathPart.split('/');
    if (more !== undefined) { return undefined; }
    const workspace = WORKSPACES.find(entry => entry.id === workspaceId);
    if (!workspace) { return undefined; }
    let tab: string | undefined;
    if (workspace.tabs.length) {
        tab = tabId === undefined || tabId === '' ? workspace.tabs[0].id : tabId;
        if (!workspace.tabs.some(entry => entry.id === tab)) { return undefined; }
    } else if (tabId !== undefined && tabId !== '') { return undefined; }
    let focus: string | undefined;
    if (query !== undefined) {
        const match = /^focus=(.*)$/.exec(query);
        if (!match || !FOCUS.test(match[1])) { return undefined; }
        focus = match[1];
    }
    return { workspace: workspace.id, ...(tab ? { tab } : {}), ...(focus ? { focus } : {}) };
}

export function formatRoute(route: Route): string {
    return '#' + routeKey(route) + (route.focus ? `?focus=${route.focus}` : '');
}
