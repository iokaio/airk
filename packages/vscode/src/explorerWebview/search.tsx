// SPDX-License-Identifier: Apache-2.0
import { useEffect, useRef, useState } from 'preact/hooks';
import { OpenSourceResult, PassageView, SearchMode, SearchResultView } from '../explorerContracts';
import { HostFailure, request } from './api';
import { countLine, shortToken } from './format';
import type { ScreenProps } from './workspaces';

interface Form { query: string; mode: SearchMode; pathPrefix: string; limit: number; maxBytes: number; fullChunks: boolean }
const DEFAULT: Form = { query: '', mode: 'policy', pathPrefix: '', limit: 20, maxBytes: 16384, fullChunks: false };

export function SearchScreen(props: ScreenProps) {
    const status = props.state.status;
    const [form, setForm] = useState<Form>(DEFAULT);
    const [result, setResult] = useState<SearchResultView>();
    const [error, setError] = useState<string>();
    const [busy, setBusy] = useState(false);
    const [passages, setPassages] = useState<Record<string, PassageView>>({});
    const [opened, setOpened] = useState<Record<string, OpenSourceResult | string>>({});
    const input = useRef<HTMLInputElement>(null);
    const vectorsOff = status?.vectorsEnabled === false || status?.semantic === 'disabled';
    const vectorReason = vectorsOff ? 'Vector search is turned off for this project.' : status?.semantic === 'pending' ? 'Vectors are still pending; results may be empty or unavailable.' : '';
    const canSearch = props.state.connection === 'ready' || props.state.connection === 'degraded';

    const run = async (next: Form) => {
        if (!next.query.trim()) { return; }
        setBusy(true); setError(undefined); setPassages({}); setOpened({});
        try {
            setResult(await request<SearchResultView>('search.run', { query: next.query, mode: next.mode, limit: next.limit, maxBytes: next.maxBytes,
                fullChunks: next.fullChunks, ...(next.pathPrefix.trim() ? { pathPrefix: next.pathPrefix.trim() } : {}) }));
        } catch (failure) {
            if (failure instanceof HostFailure && failure.error.code === 'REQUEST_SUPERSEDED') { return; }
            setResult(undefined); setError(failure instanceof Error ? failure.message : String(failure));
        } finally { setBusy(false); }
    };

    useEffect(() => {
        if (props.incoming?.query) { const next = { ...form, query: props.incoming.query }; setForm(next); void run(next); }
        else { input.current?.focus(); }
    }, [props.incoming?.serial]);

    const expand = async (ref: string) => {
        try { const passage = await request<PassageView>('evidence.expand', { ref, before: 5, after: 5 }); setPassages(prior => ({ ...prior, [ref]: passage })); }
        catch (failure) { setOpened(prior => ({ ...prior, [ref]: failure instanceof Error ? failure.message : String(failure) })); }
    };
    const open = async (ref: string) => {
        try { const outcome = await request<OpenSourceResult>('source.open', { ref }); setOpened(prior => ({ ...prior, [ref]: outcome })); }
        catch (failure) { setOpened(prior => ({ ...prior, [ref]: failure instanceof Error ? failure.message : String(failure) })); }
    };
    const field = <K extends keyof Form>(key: K, value: Form[K]) => setForm(prior => ({ ...prior, [key]: value }));

    return (
        <div class="search">
            <form class="search-form" onSubmit={event => { event.preventDefault(); void run(form); }}>
                <label class="grow">Query
                    <input ref={input} value={form.query} maxLength={4096} required placeholder="Words that must all appear"
                        onInput={event => field('query', (event.target as HTMLInputElement).value)} />
                </label>
                <label>Mode
                    <select value={form.mode} onChange={event => field('mode', (event.target as HTMLSelectElement).value as SearchMode)}>
                        <option value="policy">Collection policy</option>
                        <option value="lexical">Lexical (all words)</option>
                        <option value="hybrid" disabled={vectorsOff}>Hybrid</option>
                        <option value="vector" disabled={vectorsOff}>Vector</option>
                    </select>
                </label>
                <label>Path scope
                    <input value={form.pathPrefix} maxLength={1024} placeholder="e.g. src/auth/" onInput={event => field('pathPrefix', (event.target as HTMLInputElement).value)} />
                </label>
                <label>Limit
                    <input type="number" min={1} max={100} value={form.limit} onInput={event => field('limit', Math.min(100, Math.max(1, Number((event.target as HTMLInputElement).value) || 1)))} />
                </label>
                <label>Bytes
                    <select value={String(form.maxBytes)} onChange={event => field('maxBytes', Number((event.target as HTMLSelectElement).value))}>
                        <option value="8192">8 KB</option><option value="16384">16 KB</option><option value="32768">32 KB</option><option value="65536">64 KB</option>
                    </select>
                </label>
                <label class="check"><input type="checkbox" checked={form.fullChunks} onChange={event => field('fullChunks', (event.target as HTMLInputElement).checked)} /> Full chunks</label>
                <button type="submit" disabled={busy || !canSearch}>{busy ? 'Searching…' : 'Search'}</button>
            </form>
            {vectorReason && <p class="muted">{vectorReason}</p>}
            {!canSearch && <p class="callout">Search needs a connected AIRK service and a published index ({props.state.connection}).</p>}
            {error && <p class="callout" role="alert">{error}</p>}
            {result && (
                <section aria-label="Search results">
                    <p class="result-meta">
                        <strong>{countLine(result.returned, result.limit, result.truncated)}</strong>
                        {' · '}mode: {result.appliedMode}{' · '}publication <span title={result.publication}>{shortToken(result.publication)}</span>
                    </p>
                    {result.warnings.map(warning => <p key={warning} class="muted">{warning}</p>)}
                    {!result.hits.length && <p class="callout">No indexed evidence matched. That is not proof the text is absent from unindexed or changed files.</p>}
                    <ol class="hits">
                        {result.hits.map(hit => {
                            const outcome = opened[hit.ref];
                            const passage = passages[hit.ref];
                            return (
                                <li key={hit.ref} class="hit card">
                                    <div class="hit-head">
                                        <code class="path">{hit.path}:{hit.startLine}{hit.endLine !== hit.startLine ? `–${hit.endLine}` : ''}</code>
                                        <span class="tag" title="E1: indexed text — not a resolved binding or coverage">{hit.evidenceClass}</span>
                                        {hit.truncated && <span class="tag">truncated</span>}
                                        <span class="spacer" />
                                        <button type="button" class="ghost" onClick={() => { void expand(hit.ref); }}>Expand</button>
                                        <button type="button" onClick={() => { void open(hit.ref); }}>Open source</button>
                                    </div>
                                    <pre class="excerpt">{passage ? passage.text : hit.text}</pre>
                                    {passage && <p class="muted">Indexed lines {passage.startLine}–{passage.endLine}{passage.truncated ? ' · truncated' : ''}</p>}
                                    {outcome && <p class={typeof outcome === 'string' || outcome.live !== 'matches_index' ? 'callout' : 'muted'} role="status">
                                        {typeof outcome === 'string' ? outcome : outcome.message}</p>}
                                </li>
                            );
                        })}
                    </ol>
                    <p class="muted">A matched phrase is not a verified implementation. Open the source to investigate.</p>
                </section>
            )}
        </div>
    );
}
