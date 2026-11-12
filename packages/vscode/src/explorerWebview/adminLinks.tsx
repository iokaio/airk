// SPDX-License-Identifier: Apache-2.0
// Open AIRK Admin (U97). Management lives in the admin pages (U6); the
// Explorer only links there. The host opens them where `airk.admin.openIn`
// says (VS Code's Simple Browser or the default browser); no sign-in URL ever
// reaches this webview.
import { useState } from 'preact/hooks';
import type { AdminPage } from '../explorerContracts';
import type { ScreenProps } from './workspaces';
import { request } from './api';

export interface AdminLink { page: AdminPage; folder?: string; index?: string; set?: string }

export async function openAdmin(link: AdminLink, choose = false): Promise<void> {
    await request('admin.open', { page: link.page, ...(link.folder !== undefined ? { folder: link.folder } : {}),
        ...(link.index ? { index: link.index } : {}), ...(link.set ? { set: link.set } : {}), ...(choose ? { choose: true } : {}) });
}

/** "Open AIRK Admin" plus "Choose where…" (In VS Code / In my browser, remembered). */
export function AdminButton(props: AdminLink & { label?: string; ghost?: boolean; disabled?: boolean; choose?: boolean }) {
    const [busy, setBusy] = useState(false), [error, setError] = useState('');
    const run = async (choose: boolean) => {
        setBusy(true); setError('');
        try { await openAdmin(props, choose); }
        catch (failure) { setError(failure instanceof Error ? failure.message : String(failure)); }
        finally { setBusy(false); }
    };
    return <span class="admin-link">
        <button type="button" class={props.ghost ? 'ghost' : ''} disabled={busy || props.disabled} onClick={() => { void run(false); }}>{props.label ?? 'Open AIRK Admin'}</button>
        {props.choose !== false && <button type="button" class="ghost" disabled={busy || props.disabled} title="Open AIRK Admin in VS Code or in your browser; the choice is remembered" onClick={() => { void run(true); }}>Choose where…</button>}
        {error && <span class="muted" role="alert"> {error}</span>}
    </span>;
}

/** No index yet: one "Create an index for this folder" per window folder (U97). */
export function CreateIndexForFolders(props: { folders?: readonly string[]; disabled?: boolean }) {
    const folders = props.folders ?? [];
    return <article class="card create-index">
        <h2>No index yet</h2>
        <p>AIRK has no index for this window. Create one in AIRK Admin; this page shows it once it exists. Indexing, sources and index sets are managed in AIRK Admin.</p>
        {folders.length
            ? <ul>{folders.map(folder => <li key={folder}><code class="path">{folder}</code> <AdminButton page="index-library" folder={folder} label="Create an index for this folder" disabled={props.disabled} choose={false} /></li>)}</ul>
            : <p>Open a folder in VS Code to create an index for it, or <AdminButton page="index-library" label="open the AIRK Admin index library" ghost disabled={props.disabled} choose={false} />.</p>}
    </article>;
}

/** Management tabs open the matching admin page; routes stay valid for links, help and agent drafts. */
const MANAGE_PAGES: Readonly<Record<string, { page: AdminPage; title: string; text: string }>> = {
    overview: { page: 'overview', title: 'Overview', text: 'Service health, indexes and work in progress.' },
    activity: { page: 'activity', title: 'Activity', text: 'Indexing jobs, their progress and their outcomes.' },
    storage: { page: 'storage', title: 'Storage', text: 'Index storage, quotas and cleanup.' },
    repair: { page: 'recovery', title: 'Recovery', text: 'Recovery plans for indexes that need repair.' },
    service: { page: 'service', title: 'Service', text: 'Start, stop and restart the AIRK Service and review its runtime.' },
    agents: { page: 'agents', title: 'Agents', text: 'Coding-agent connections and their access.' },
    usage: { page: 'usage', title: 'Usage', text: 'Local usage and savings estimates.' },
    logs: { page: 'logs', title: 'Logs', text: 'Service and extension logs.' },
};
export function AdminHandoffScreen(props: ScreenProps) {
    const target = MANAGE_PAGES[props.route.tab ?? 'overview'] ?? MANAGE_PAGES.overview;
    return <article class="card admin-handoff">
        <h2>{target.title} is in AIRK Admin</h2>
        <p>{target.text} Management moved to the AIRK admin pages; this window reads indexes and evidence only.</p>
        <AdminButton page={target.page} label={`Open ${target.title} in AIRK Admin`} disabled={!props.state.trusted} />
    </article>;
}
