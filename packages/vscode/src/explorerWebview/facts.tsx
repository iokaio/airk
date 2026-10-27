// SPDX-License-Identifier: Apache-2.0
import { ComponentChildren } from 'preact';
import { useEffect, useState } from 'preact/hooks';
import { FactItemView, FactListView, OpenSourceResult } from '../explorerContracts';
import { request } from './api';
import { countLine, shortToken } from './format';

const CLASS_TITLE: Record<string, string> = {
    E1: 'E1 indexed text: a textual hit, not a binding or coverage',
    E2: 'E2 syntax fact: written declaration or call the indexer saw; targets are not resolved',
    E3: 'E3 rule observation',
    E4: 'E4 provider-asserted: a producer claim, not promoted to a resolved fact',
};

export function useAsync<T>() {
    const [value, setValue] = useState<T>();
    const [error, setError] = useState<string>();
    const [busy, setBusy] = useState(false);
    const run = async (work: () => Promise<T>) => {
        setBusy(true); setError(undefined);
        try { setValue(await work()); }
        catch (failure) {
            if ((failure as { error?: { code?: string } }).error?.code !== 'REQUEST_SUPERSEDED') { setValue(undefined); setError(failure instanceof Error ? failure.message : String(failure)); }
        } finally { setBusy(false); }
    };
    return { value, setValue, error, busy, run };
}

export function OpenButton(props: { ref_: string }) {
    const [outcome, setOutcome] = useState<string>();
    return (
        <>
            <button type="button" onClick={() => {
                void request<OpenSourceResult>('source.open', { ref: props.ref_ }).then(result => setOutcome(result.message), failure => setOutcome(String(failure.message ?? failure)));
            }}>Open source</button>
            {outcome && <span class="muted" role="status"> {outcome}</span>}
        </>
    );
}

/** Generic cited fact list with honest counts and cursor continuation where AIRK provides one. */
export function FactList(props: { list: FactListView; empty: string; actions?: (item: FactItemView) => ComponentChildren; inspect?: (ref: string) => void }) {
    const [list, setList] = useState(props.list);
    const [error, setError] = useState<string>();
    const [loading, setLoading] = useState(false);
    useEffect(() => { setList(props.list); setError(undefined); }, [props.list]);
    const more = async () => {
        if (!list.more) { return; }
        setLoading(true); setError(undefined);
        try {
            const next = await request<FactListView>('list.more', { more: list.more });
            setList({ ...next, items: [...list.items, ...next.items], returned: list.returned + next.returned });
        } catch (failure) { setError(failure instanceof Error ? failure.message : String(failure)); }
        finally { setLoading(false); }
    };
    return (
        <section class="fact-list">
            <p class="result-meta"><strong>{countLine(list.returned, list.limit, list.truncated || !!list.more)}</strong>
                {' · '}publication <span title={list.publication}>{shortToken(list.publication)}</span></p>
            {list.notes.map(note => <p key={note} class="muted">{note}</p>)}
            {!list.items.length && <p class="callout">{props.empty}</p>}
            <ol class="hits">
                {list.items.map((item, index) => (
                    <li key={`${item.ref ?? item.path}-${index}`} class="hit card">
                        <div class="hit-head">
                            <code class="path">{item.path}{item.startLine ? `:${item.startLine}${item.endLine && item.endLine !== item.startLine ? `–${item.endLine}` : ''}` : ''}</code>
                            {item.location && <span class="tag" title="Original document location">{item.location}</span>}
                            <span class="tag" title={CLASS_TITLE[item.evidenceClass]}>{item.evidenceClass}</span>
                            <span class="tag">{item.kind}</span>
                            {item.resolution && <span class="tag" title="Written targets are not resolved bindings">{item.resolution.replaceAll('_', ' ')}</span>}
                            <span class="spacer" />
                            {props.actions?.(item)}
                            {item.ref && props.inspect && <button type="button" class="ghost" onClick={() => props.inspect!(item.ref!)}>Inspect</button>}
                            {item.ref && <OpenButton ref_={item.ref} />}
                        </div>
                        <div class="source-text fact-name"><span>{item.name || '(unnamed)'}</span>{item.signature && item.signature !== item.name && <code class="sig"> {item.signature}</code>}</div>
                        {item.heading && <p class="muted">{item.heading.join(' › ')}</p>}
                        {item.qualifiedName && item.qualifiedName !== item.name && <p class="muted">qualified: <code class="source-inline">{item.qualifiedName}</code></p>}
                        {item.detail && (item.kind === 'occurrence' ? <pre class="excerpt">{item.detail}</pre> : <p class="source-text">{item.detail}</p>)}
                        {item.producer && <p class="muted">producer {item.producer}{item.basis ? ` · basis ${item.basis}` : ''}</p>}
                    </li>
                ))}
            </ol>
            {list.more && <button type="button" class="ghost" disabled={loading} onClick={() => { void more(); }}>{loading ? 'Loading…' : 'Load more'}</button>}
            {!list.more && list.truncated && <p class="muted">This tool has no continuation cursor. Raise the limit or narrow the query.</p>}
            {error && <p class="callout" role="alert">{error}</p>}
            {list.provenance.length > 0 && (
                <details><summary>Producers and coverage</summary>
                    <ul>{list.provenance.map((entry, index) => <li key={index}><code>{entry.producer}</code>{entry.basis ? ` · ${entry.basis}` : ''}{entry.coverage ? ` — ${entry.coverage}` : ''}</li>)}</ul>
                </details>
            )}
        </section>
    );
}
