// SPDX-License-Identifier: Apache-2.0
// Read-only index library (U97): indexes, index sets, their sources and
// memberships, policy and split status, read through the window's airkctl
// session (native catalog_page|catalog_detail|catalog_policy_status|
// catalog_split_status). Every native response is kept complete for display
// and Copy JSON; a failed read shows its native code and is never shown as an
// empty library. Changes are made in AIRK Admin.
import { useEffect, useRef, useState } from 'preact/hooks';
import type { ExplorerViewState } from '../explorerContracts';
import { request } from './api';
import { AdminButton, CreateIndexForFolders } from './adminLinks';

type Obj = Record<string, unknown>;
const obj = (value: unknown): Obj => value && typeof value === 'object' && !Array.isArray(value) ? value as Obj : {};
export interface LibraryReplyView { method: Obj; expected: string; response?: Obj; error?: { code: string; message: string; retryable: boolean }; observedAt: number }
interface Item { kind: string; value: Obj }
export interface Listing { replies: LibraryReplyView[]; items: Item[]; continuation?: Obj; revision?: number; error?: LibraryReplyView['error']; complete: boolean }
type PageKind = 'indexes' | 'projects' | 'memberships' | 'sources';

const replyError = (reply: LibraryReplyView | undefined): LibraryReplyView['error'] =>
    !reply ? { code: 'no_reply', message: 'The index library read returned nothing.', retryable: true }
        : reply.error ?? (reply.response ? undefined : { code: 'malformed', message: 'The index library read returned no response.', retryable: false });
const pageOf = (reply: LibraryReplyView) => obj(obj(reply.response).page);
const itemsOf = (reply: LibraryReplyView): Item[] => {
    const items = pageOf(reply).items;
    return Array.isArray(items) ? items.map(obj).filter(item => typeof item.kind === 'string').map(item => ({ kind: String(item.kind), value: obj(item.value) })) : [];
};

/** Read one page; `prior` continues it with the previous continuation and the first page's revision. */
export async function readPage(kind: PageKind, id?: string, prior?: Listing): Promise<Listing> {
    const more = !!prior?.continuation;
    const reply = await request<LibraryReplyView>('library.read', { method: 'page', kind, ...(id ? { id } : {}),
        ...(more ? { after: prior!.continuation!, ...(prior!.revision !== undefined ? { revision: prior!.revision } : {}) } : {}) });
    const replies = [...(more ? prior!.replies : []), reply];
    const error = replyError(reply);
    // A failed continuation keeps the rows and cursor already read, so Load more can be retried.
    if (error) { return more ? { ...prior!, replies, error, complete: false } : { replies, items: [], error, complete: false }; }
    const page = pageOf(reply), continuation = page.continuation && typeof page.continuation === 'object' ? obj(page.continuation) : undefined;
    const revision = more ? prior!.revision : typeof page.revision === 'number' ? page.revision : undefined;
    return { replies, items: [...(more ? prior!.items : []), ...itemsOf(reply)], ...(continuation ? { continuation } : {}), ...(revision !== undefined ? { revision } : {}), complete: !continuation };
}

function ErrorLine(props: { error?: LibraryReplyView['error'] }) {
    if (!props.error) { return null; }
    return <p class="notice" role="alert"><strong>Could not read the index library</strong> (<code>{props.error.code}</code>{props.error.retryable ? ', retryable' : ''}): {props.error.message} A failed read does not mean the library is empty.</p>;
}
function Complete(props: { title: string; value: unknown }) {
    const [message, setMessage] = useState('');
    const json = JSON.stringify(props.value, null, 2);
    return <details><summary>{props.title}</summary>
        <div class="actions"><button type="button" class="ghost" onClick={() => { void request('clipboard.write', { text: json }).then(() => setMessage('Copied'), failure => setMessage(failure instanceof Error ? failure.message : String(failure))); }}>Copy JSON</button>{message && <span role="status"> {message}</span>}</div>
        <pre>{json}</pre>
    </details>;
}

interface Selected { kind: 'index' | 'project'; id: string; name: string; detail?: LibraryReplyView; sources?: Listing; memberships?: Listing; policy?: LibraryReplyView; split?: LibraryReplyView }

export function LibraryView(props: { tab: 'library' | 'projects'; state: ExplorerViewState }) {
    const kind: PageKind = props.tab === 'projects' ? 'projects' : 'indexes';
    const { state } = props;
    const ready = state.trusted && state.nativeExecutionEnabled !== false && state.connection === 'ready';
    const [listing, setListing] = useState<Listing>();
    const [selected, setSelected] = useState<Selected>();
    const [busy, setBusy] = useState(false), [failure, setFailure] = useState('');
    const [operation, setOperation] = useState('');
    const automatic = useRef('');
    const working = useRef(false);
    const run = async (work: () => Promise<void>) => {
        if (working.current) { return; }
        working.current = true; setBusy(true); setFailure('');
        try { await work(); } catch (error) { setFailure(error instanceof Error ? error.message : String(error)); } finally { working.current = false; setBusy(false); }
    };
    const load = (more = false) => run(async () => {
        const next = await readPage(kind, undefined, more ? listing : undefined);
        setListing(next);
        if (!more) { setSelected(undefined); }
    });
    // Read on open, after Check again and when the window regains focus (the
    // host's project refresh renews `projectsObservedAt`). A failed read waits
    // for Refresh; it is not retried in a loop.
    const owner = JSON.stringify([kind, state.projectsObservedAt, ready]);
    useEffect(() => {
        if (!ready || automatic.current === owner) { return; }
        automatic.current = owner;
        void load();
    }, [owner]);

    const open = (item: Item) => run(async () => {
        const id = String(item.value.id), name = String(item.value.name ?? id), isIndex = item.kind === 'index';
        const detail = await request<LibraryReplyView>('library.read', { method: 'detail', id });
        const memberships = await readPage('memberships', id);
        const sources = isIndex ? await readPage('sources', id) : undefined;
        const policy = isIndex ? await request<LibraryReplyView>('library.read', { method: 'policy', id }) : undefined;
        setSelected({ kind: isIndex ? 'index' : 'project', id, name, detail, memberships, ...(sources ? { sources } : {}), ...(policy ? { policy } : {}) });
    });
    const more = (part: 'sources' | 'memberships') => run(async () => {
        if (!selected) { return; }
        const next = await readPage(part, selected.id, selected[part]);
        setSelected(part === 'sources' ? { ...selected, sources: next } : { ...selected, memberships: next });
    });

    const live = listing?.items.filter(item => item.value.tombstone !== true) ?? [];
    const noIndex = ready && (state.projects.length === 0 || kind === 'indexes' && !!listing && listing.complete && !listing.error && live.length === 0);
    const label = kind === 'projects' ? 'index sets' : 'indexes';
    return <section class="index-library">
        <article class="card">
            <div class="section-head"><h2>{kind === 'projects' ? 'Index sets' : 'Index library'}</h2>
                <AdminButton page="index-library" disabled={!state.trusted} /></div>
            <p>{kind === 'projects' ? 'Reusable selections of indexes, read from the AIRK Service. Create and change index sets in AIRK Admin.' : 'Indexes on this computer, read from the AIRK Service. Create, change, split or delete indexes in AIRK Admin.'}</p>
            {!state.trusted && <p class="callout">Trust this workspace to read the index library.</p>}
            {state.trusted && state.nativeExecutionEnabled === false && <p class="callout">Native execution is turned off; the index library is not read.</p>}
            {state.trusted && state.nativeExecutionEnabled !== false && state.connection !== 'ready' && <p role="status">The AIRK Service is not ready. The library loads when it is.</p>}
            <div class="actions"><button type="button" disabled={busy || !ready} onClick={() => { void load(); }}>Refresh {label}</button></div>
            {failure && <p class="notice" role="alert">{failure}</p>}
            <ErrorLine error={listing?.error} />
            {busy && <p role="status">Reading the index library…</p>}
            {listing?.items.map(item => {
                const id = String(item.value.id), isIndex = item.kind === 'index';
                return <article class="card" key={id}>
                    <strong>{String(item.value.name ?? id)}</strong>{item.value.tombstone === true && <span class="tag">retired</span>}
                    <p><code>{id}</code>{isIndex && typeof obj(item.value.backing).root === 'string' && <> · <code class="path">{String(obj(item.value.backing).root)}</code></>}</p>
                    <div class="actions">
                        <button type="button" disabled={busy} onClick={() => { void open(item); }}>{isIndex ? 'Show index details' : 'Show index set details'}</button>
                        <AdminButton page="index-library" {...(isIndex ? { index: id } : { set: id })} label="Open in AIRK Admin" ghost choose={false} />
                    </div>
                </article>;
            })}
            {listing && !listing.error && listing.items.length === 0 && <p>No {label} in the catalog.</p>}
            {listing?.continuation && <button type="button" disabled={busy || !ready} onClick={() => { void load(true); }}>Load more {label}</button>}
            {listing && <Complete title={`Complete native ${label} responses (${listing.replies.length})`} value={listing.replies} />}
        </article>
        {noIndex && <CreateIndexForFolders folders={state.workspaceFolders} disabled={!state.trusted} />}
        {selected && <article class="card indexing-detail">
            <div class="section-head"><h2>{selected.kind === 'index' ? 'Index' : 'Index set'}: {selected.name}</h2>
                <AdminButton page="index-library" {...(selected.kind === 'index' ? { index: selected.id } : { set: selected.id })} label="Open in AIRK Admin" choose={false} /></div>
            <ErrorLine error={replyError(selected.detail)} />
            {selected.detail?.response && <Complete title="Complete native detail" value={selected.detail} />}
            {selected.sources && <section><h3>Sources</h3><ErrorLine error={selected.sources.error} />
                {selected.sources.items.map(item => <p key={String(item.value.id)}><code class="path">{String(item.value.canonical_path)}</code> · {item.value.enabled === true ? 'Enabled' : 'Disabled'} · <code>{String(item.value.id)}</code></p>)}
                {!selected.sources.error && !selected.sources.items.length && <p>No sources returned.</p>}
                {selected.sources.continuation && <button type="button" disabled={busy} onClick={() => { void more('sources'); }}>Load more sources</button>}
                <Complete title="Complete native sources responses" value={selected.sources.replies} /></section>}
            {selected.memberships && <section><h3>{selected.kind === 'index' ? 'Index sets using this index' : 'Indexes in this set'}</h3><ErrorLine error={selected.memberships.error} />
                {selected.memberships.items.map((item, at) => <p key={at}><code>{String(selected.kind === 'index' ? item.value.project_id : item.value.index_id)}</code></p>)}
                {!selected.memberships.error && !selected.memberships.items.length && <p>No memberships returned.</p>}
                {selected.memberships.continuation && <button type="button" disabled={busy} onClick={() => { void more('memberships'); }}>Load more memberships</button>}
                <Complete title="Complete native membership responses" value={selected.memberships.replies} /></section>}
            {selected.policy && <section><h3>Path exclusions and cleanup</h3><ErrorLine error={replyError(selected.policy)} />
                {selected.policy.response && <p>{obj(selected.policy.response).snapshot ? 'Policy recorded for this index.' : 'No policy recorded for this index.'}{obj(selected.policy.response).job ? ` Cleanup job: ${String(obj(obj(selected.policy.response).job).state ?? 'unknown')}.` : ''}</p>}
                <Complete title="Complete native policy status" value={selected.policy} /></section>}
            {selected.kind === 'index' && <section><h3>Split status</h3>
                <p>Read a split by its operation ID. Splits are started and recovered in AIRK Admin.</p>
                <form onSubmit={event => { event.preventDefault(); void run(async () => { const split = await request<LibraryReplyView>('library.read', { method: 'split', operation }); setSelected(current => current ? { ...current, split } : current); }); }}>
                    <label>Split operation ID<input required pattern="[0-9a-f]{32}" value={operation} onInput={event => setOperation(event.currentTarget.value)} /></label>
                    <button disabled={busy || !/^[0-9a-f]{32}$/.test(operation)}>Read split status</button>
                </form>
                {selected.split && <><ErrorLine error={replyError(selected.split)} /><Complete title="Complete native split status" value={selected.split} /></>}</section>}
        </article>}
    </section>;
}
