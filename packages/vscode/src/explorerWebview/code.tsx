// SPDX-License-Identifier: Apache-2.0
import { useEffect, useState } from 'preact/hooks';
import { FactItemView, FactListView } from '../explorerContracts';
import { request } from './api';
import { FactList, useAsync } from './facts';
import type { ScreenProps } from './workspaces';

function Relationships(props: { symbol: string }) {
    const [direction, setDirection] = useState<'incoming' | 'outgoing'>('outgoing');
    const [hops, setHops] = useState(1);
    const edges = useAsync<FactListView>();
    const load = () => edges.run(() => request<FactListView>('code.relationships', { symbol: props.symbol, direction, hops, limit: 50 }));
    return (
        <div class="nested">
            <div class="inline-form">
                <select value={direction} aria-label="Direction" onChange={event => setDirection((event.target as HTMLSelectElement).value as 'incoming' | 'outgoing')}>
                    <option value="outgoing">Outgoing (dependencies, containment)</option>
                    <option value="incoming">Incoming (callers, implementations)</option>
                </select>
                <label>Hops <select value={String(hops)} onChange={event => setHops(Number((event.target as HTMLSelectElement).value))}>
                    <option value="1">1</option><option value="2">2</option><option value="3">3</option></select></label>
                <button type="button" disabled={edges.busy} onClick={() => { void load(); }}>{edges.busy ? 'Loading…' : 'Show candidates'}</button>
            </div>
            {edges.error && <p class="callout" role="alert">{edges.error}</p>}
            {edges.value && <FactList list={edges.value} empty="No candidate edges were found. Absence of an edge proves nothing."
                actions={item => item.symbolRef ? <Relationships symbol={item.symbolRef} /> : null} />}
        </div>
    );
}

function SymbolActions(props: { item: FactItemView }) {
    const [open, setOpen] = useState(false);
    if (!props.item.symbolRef) { return null; }
    return (
        <>
            <button type="button" class="ghost" aria-expanded={open} onClick={() => setOpen(value => !value)}>Relationships</button>
            {open && <Relationships symbol={props.item.symbolRef} />}
        </>
    );
}

export function SymbolsScreen(_props: ScreenProps) {
    const [name, setName] = useState('');
    const [prefix, setPrefix] = useState('');
    const [exact, setExact] = useState(true);
    const result = useAsync<FactListView>();
    const search = () => result.run(() => exact && name.trim()
        ? request<FactListView>('code.facts', { name: name.trim(), limit: 50, ...(prefix.trim() ? { pathPrefix: prefix.trim() } : {}) })
        : request<FactListView>('code.symbols', { limit: 50, ...(name.trim() ? { name: name.trim() } : {}), ...(prefix.trim() ? { pathPrefix: prefix.trim() } : {}) }));
    return (
        <div>
            <form class="search-form" onSubmit={event => { event.preventDefault(); void search(); }}>
                <label class="grow">Symbol name<input value={name} maxLength={512} placeholder="Exact definition name, e.g. refreshSession (empty lists all)" onInput={event => setName((event.target as HTMLInputElement).value)} /></label>
                <label>Path scope<input value={prefix} maxLength={1024} placeholder="e.g. src/" onInput={event => setPrefix((event.target as HTMLInputElement).value)} /></label>
                <label class="check"><input type="checkbox" checked={exact} onChange={event => setExact((event.target as HTMLInputElement).checked)} /> Code facts (symbol ids)</label>
                <button type="submit" disabled={result.busy}>{result.busy ? 'Searching…' : 'Find'}</button>
            </form>
            <p class="muted">Names are exact native definitions. A written call is not a resolved cross-file binding; relationships are candidates.</p>
            {result.error && <p class="callout" role="alert">{result.error}</p>}
            {result.value && <FactList list={result.value} empty="No indexed definition has that exact name." actions={item => <SymbolActions item={item} />} />}
        </div>
    );
}

export function ReferencesScreen(_props: ScreenProps) {
    const [name, setName] = useState('');
    const [prefix, setPrefix] = useState('');
    const result = useAsync<FactListView>();
    return (
        <div>
            <form class="search-form" onSubmit={event => { event.preventDefault(); if (name.trim()) { void result.run(() => request<FactListView>('code.references', { name: name.trim(), limit: 50, ...(prefix.trim() ? { pathPrefix: prefix.trim() } : {}) })); } }}>
                <label class="grow">Identifier<input value={name} required maxLength={256} placeholder="Exact-case whole identifier" onInput={event => setName((event.target as HTMLInputElement).value)} /></label>
                <label>Path scope<input value={prefix} maxLength={1024} placeholder="e.g. src/" onInput={event => setPrefix((event.target as HTMLInputElement).value)} /></label>
                <button type="submit" disabled={result.busy}>{result.busy ? 'Searching…' : 'Find occurrences'}</button>
            </form>
            <p class="muted">Textual occurrences include comments, strings and definitions. They are not resolved references and not calls.</p>
            {result.error && <p class="callout" role="alert">{result.error}</p>}
            {result.value && <FactList list={result.value} empty="No indexed occurrence of that identifier." />}
        </div>
    );
}

export function OutlineScreen(_props: ScreenProps) {
    const result = useAsync<FactListView>();
    const load = () => result.run(() => request<FactListView>('code.outline', {}));
    useEffect(() => { void load(); }, []);
    return (
        <div>
            <div class="section-head">
                <p class="muted">Outline of the active editor's file in the selected project, from the published index (not the unsaved buffer).</p>
                <button type="button" class="ghost" disabled={result.busy} onClick={() => { void load(); }}>Refresh for active file</button>
            </div>
            {result.error && <p class="callout" role="alert">{result.error}</p>}
            {result.value && <FactList list={result.value} empty="The index holds no outline facts for this file." actions={item => <SymbolActions item={item} />} />}
        </div>
    );
}
