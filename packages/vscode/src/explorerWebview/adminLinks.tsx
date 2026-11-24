// SPDX-License-Identifier: Apache-2.0
// Explorer management navigation. Browser launch lives in the top bar.
import { createContext } from 'preact';
import { useContext } from 'preact/hooks';
import type { Route } from '../explorerRoutes';
import type { AdminPage } from '../explorerContracts';
import type { LogTarget } from '../../../ui/src/management';
import type { ScreenProps } from './workspaces';


export interface AdminLink { page: AdminPage; folder?: string; index?: string; set?: string; logTarget?: LogTarget }

export const ManagementNavigation = createContext<(link: AdminLink) => void>(() => {});
export const MANAGEMENT_ROUTES: Readonly<Record<AdminPage, Route>> = {
    overview: { workspace: 'manage', tab: 'overview' },
    'index-library': { workspace: 'indexing', tab: 'library' },
    activity: { workspace: 'manage', tab: 'activity' },
    agents: { workspace: 'manage', tab: 'agents' },
    storage: { workspace: 'manage', tab: 'storage' },
    recovery: { workspace: 'manage', tab: 'repair' },
    service: { workspace: 'manage', tab: 'service' },
    logs: { workspace: 'manage', tab: 'logs' },
    usage: { workspace: 'manage', tab: 'usage' },
    search: { workspace: 'search', tab: 'query' },
    help: { workspace: 'help' },
};

const labels: Record<AdminPage, string> = {
    overview: 'Service overview', 'index-library': 'Index library', activity: 'Activity',
    agents: 'Agent connections', storage: 'Storage', recovery: 'Recovery', service: 'Service',
    logs: 'Logs', usage: 'Usage', search: 'Search', help: 'Help',
};

export function AdminButton({ label, ghost, disabled, choose: _choose, ...link }: AdminLink & { label?: string; ghost?: boolean; disabled?: boolean; choose?: boolean }) {
    const navigate = useContext(ManagementNavigation);
    return <button type="button" class={ghost ? 'ghost' : ''} disabled={disabled} onClick={() => navigate(link)}>{label ?? labels[link.page]}</button>;
}

/** No index yet: one "Create an index for this folder" per window folder (U97). */
export function CreateIndexForFolders(props: { folders?: readonly string[]; disabled?: boolean }) {
    const folders = props.folders ?? [];
    return <article class="card create-index">
        <h2>No index yet</h2>
        <p>AIRK has no index for this window. Create one for a folder in this window, or open the Index library.</p>
        {folders.length
            ? <ul>{folders.map(folder => <li key={folder}><code class="path">{folder}</code> <AdminButton page="index-library" folder={folder} label="Create an index for this folder" disabled={props.disabled} choose={false} /></li>)}</ul>
            : <p>Open a folder in VS Code to create an index for it, or <AdminButton page="index-library" label="open the Index library" ghost disabled={props.disabled} choose={false} />.</p>}
    </article>;
}

/** Description shown when native management is unavailable. */
const MANAGE_PAGES: Readonly<Record<string, { title: string; text: string }>> = {
    overview: { title: 'Overview', text: 'Service health, indexes and work in progress.' },
    storage: { title: 'Storage', text: 'Index storage, quotas and cleanup.' },
    repair: { title: 'Recovery', text: 'Recovery plans for indexes that need repair.' },
    service: { title: 'Service', text: 'Manage service-wide GPU use, review the runtime, and stop or restart the AIRK Service.' },
    agents: { title: 'Agents', text: 'Coding-agent connections and their access.' },
    logs: { title: 'Logs', text: 'Service and extension logs.' },
};
export function ManagementUnavailable(props: ScreenProps) {
    const target = MANAGE_PAGES[props.route.tab ?? 'overview'] ?? MANAGE_PAGES.overview;
    return <article class="card">
        <h2>{target.title}</h2>
        <p>{target.text}</p>
        <p>Management requires a trusted workspace, enabled native execution, and a compatible local AIRK Service.</p>
    </article>;
}
