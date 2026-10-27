// SPDX-License-Identifier: Apache-2.0
import { useEffect, useRef, useState } from 'preact/hooks';
import type { WorkspaceFileKind, WorkspaceFilesView } from '../explorerContracts';
import { request } from './api';

export interface LibraryFile { workspace: string; path: string }
const TITLES = { plans: 'Saved plans', strategies: 'Saved strategies', recipes: 'Saved search files' };

/** Browsing is read-only. Only Load or a file double-click asks the owning draft to import. */
export function FileBrowser(props: { kind: WorkspaceFileKind; trusted: boolean; disabled?: boolean; load(file: LibraryFile): Promise<void> }) {
    const [view, setView] = useState<WorkspaceFilesView>();
    const [busy, setBusy] = useState(false), [loading, setLoading] = useState(false), [error, setError] = useState<string>();
    const epoch = useRef(0), loadingFile = useRef(false);
    const browse = async (directory: string, workspace = view?.workspace, offset = 0) => {
        const owner = ++epoch.current;
        setBusy(true); setError(undefined);
        try {
            const next = await request<WorkspaceFilesView>('files.list', { kind: props.kind, directory, ...(workspace ? { workspace } : {}), ...(offset ? { offset } : {}) });
            if (epoch.current === owner) { setView(next); }
        } catch (failure) { if (epoch.current === owner) { setError(failure instanceof Error ? failure.message : String(failure)); } }
        finally { if (epoch.current === owner) { setBusy(false); } }
    };
    useEffect(() => { if (props.trusted) { void browse(''); } return () => { epoch.current++; }; }, [props.kind, props.trusted]);
    const load = async (path: string) => {
        if (!view || busy || props.disabled || loadingFile.current) { return; }
        loadingFile.current = true; setLoading(true); setError(undefined);
        try { await props.load({ workspace: view.workspace, path }); }
        catch (failure) { setError(failure instanceof Error ? failure.message : String(failure)); }
        finally { loadingFile.current = false; setLoading(false); }
    };
    const blocked = busy || loading || props.disabled;
    const parts = view?.directory.split('/').filter(Boolean) ?? [];
    return <section class="card file-library" aria-label={TITLES[props.kind]}>
        <div class="section-head"><div><p class="eyebrow">Workspace files</p><h2>{TITLES[props.kind]}</h2></div>
            <button class="ghost" disabled={!props.trusted || blocked} onClick={() => void browse(view?.directory ?? '')}>Refresh files</button></div>
        {!props.trusted ? <p class="muted">Trust this workspace to browse saved files.</p> : <>
            {view && <>
                {view.roots.length > 1 && <label>Workspace folder <select aria-label={`${TITLES[props.kind]} workspace`} value={view.workspace} disabled={blocked}
                    onChange={event => void browse('', (event.target as HTMLSelectElement).value)}>{view.roots.map(root => <option key={root.id} value={root.id}>{root.name}</option>)}</select></label>}
                <nav class="file-breadcrumbs" aria-label={`${TITLES[props.kind]} folder`}>
                    <button class="link" disabled={blocked} onClick={() => void browse('')}>{view.base}</button>
                    {parts.map((part, at) => <span key={at}><span aria-hidden="true"> / </span><button class="link" disabled={blocked} onClick={() => void browse(parts.slice(0, at + 1).join('/'))}>{part}</button></span>)}
                </nav>
                {parts.length > 0 && <button class="ghost" disabled={blocked} onClick={() => void browse(parts.slice(0, -1).join('/'))}>Up one folder</button>}
                <ul class="file-list">{view.entries.map(entry => <li key={entry.path} class={entry.directory ? 'folder' : 'file'}
                    onDblClick={event => { if (!entry.directory && !(event.target as HTMLElement).closest('button')) { void load(entry.path); } }}>
                    {entry.directory ? <button class="link file-name" disabled={blocked} onClick={() => void browse(entry.path)}><span aria-hidden="true">▸ </span>{entry.name}/</button>
                        : <><span class="file-name" title="Double-click to load">{entry.name}</span><button disabled={blocked} onClick={() => void load(entry.path)}>Load</button></>}
                </li>)}</ul>
                {!view.entries.length && <p class="muted">{view.missing ? `No ${view.base} folder exists yet. Save or export a file there to start your library.` : 'No supported files or subfolders on this page.'}</p>}
                {view.more !== undefined && <button class="ghost" disabled={blocked} onClick={() => void browse(view.directory, view.workspace, view.more)}>Next files</button>}
                {view.more === undefined && <p class="muted">{view.entries.length} file(s) and folder(s) on this page. Double-click a file or choose Load.</p>}
                <button class="link" disabled={blocked} onClick={() => void browse(view.directory, view.workspace)}>First page</button>
            </>}
            {busy && <p class="muted" role="status">Reading saved files…</p>}
            {loading && <p class="muted" role="status">Loading file…</p>}
            {error && <p class="callout" role="alert">{error}</p>}
        </>}
    </section>;
}
