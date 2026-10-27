// SPDX-License-Identifier: Apache-2.0
import catalog from 'airk-explorer-help';
import { ComponentChildren } from 'preact';
import { useEffect, useMemo, useRef, useState } from 'preact/hooks';
import { ABOUT_LINKS, EngineHelpView, HelpTopic } from '../explorerContracts';
import { parseRoute } from '../explorerRoutes';
import { request } from './api';
import type { ScreenProps } from './workspaces';

/** Inline `code` and **bold** only; everything renders as text, never HTML. */
function inline(text: string): ComponentChildren[] {
    return text.split(/(`[^`]+`|\*\*[^*]+\*\*)/).filter(Boolean).map((part, index) =>
        part.startsWith('`') ? <code key={index}>{part.slice(1, -1)}</code> : part.startsWith('**') ? <strong key={index}>{part.slice(2, -2)}</strong> : part);
}

/** Minimal Markdown: headings, paragraphs, bulleted and numbered lists. */
export function Markdown(props: { text: string }) {
    const blocks: ComponentChildren[] = [];
    let list: { ordered: boolean; items: string[] } | undefined;
    let paragraph: string[] = [];
    const flush = () => {
        if (paragraph.length) { blocks.push(<p key={blocks.length}>{inline(paragraph.join(' '))}</p>); paragraph = []; }
        if (list) { const items = list.items.map((item, index) => <li key={index}>{inline(item)}</li>); blocks.push(list.ordered ? <ol key={blocks.length}>{items}</ol> : <ul key={blocks.length}>{items}</ul>); list = undefined; }
    };
    for (const line of props.text.split('\n')) {
        const heading = /^(#{1,3})\s+(.*)$/.exec(line), bullet = /^[-*]\s+(.*)$/.exec(line), numbered = /^\d+\.\s+(.*)$/.exec(line);
        if (heading) { flush(); blocks.push(heading[1].length === 1 ? <h2 key={blocks.length}>{inline(heading[2])}</h2> : <h3 key={blocks.length}>{inline(heading[2])}</h3>); }
        else if (bullet || numbered) {
            if (paragraph.length) { blocks.push(<p key={blocks.length}>{inline(paragraph.join(' '))}</p>); paragraph = []; }
            const ordered = !!numbered;
            if (list && list.ordered !== ordered) { flush(); }
            list ??= { ordered, items: [] }; list.items.push((bullet ?? numbered)![1]);
        } else if (!line.trim()) { flush(); }
        else { if (list) { flush(); } paragraph.push(line.trim()); }
    }
    flush();
    return <div class="markdown">{blocks}</div>;
}

export function helpScore(topic: HelpTopic, query: string): number {
    const phrase = query.trim().toLowerCase(), terms = [...new Set(phrase.match(/[a-z0-9]+/g) ?? [])];
    if (!terms.length) { return 0; }
    const title = topic.title.toLowerCase(), aliases = topic.aliases.join(' ').toLowerCase(), body = topic.body.toLowerCase();
    return (topic.id === phrase ? 40 : 0) + (title.includes(phrase) ? 24 : 0) + (aliases.includes(phrase) ? 16 : 0)
        + terms.reduce((score, term) => score + (title.includes(term) ? 8 : 0) + (aliases.includes(term) ? 6 : 0) + (body.includes(term) ? 1 : 0), 0);
}

export function HelpScreen(props: ScreenProps) {
    const [query, setQuery] = useState('');
    const [area, setArea] = useState('');
    const [referenceType, setReferenceType] = useState<'topic' | 'command' | 'tool'>('tool');
    const [reference, setReference] = useState('airk_search');
    const [lastQuery, setLastQuery] = useState<{ topic?: string; command?: string; tool?: string }>({});
    const [linkError, setLinkError] = useState<string>();
    const [selected, setSelected] = useState<HelpTopic>(catalog.topics.find(topic => topic.id.replaceAll('.', '-') === props.route.focus) ?? catalog.topics.find(topic => topic.id === 'explorer.overview') ?? catalog.topics[0]);
    const [engine, setEngine] = useState<{ value?: EngineHelpView; error?: string; busy: boolean }>({ busy: false });
    const [showEngine, setShowEngine] = useState(false);
    const referenceEpoch = useRef(0);
    const closeReference = () => { referenceEpoch.current++; setShowEngine(false); setEngine({ busy: false }); };
    useEffect(() => () => { referenceEpoch.current++; }, []);
    useEffect(() => {
        const topic = catalog.topics.find(entry => entry.id.replaceAll('.', '-') === props.route.focus);
        if (topic) { setSelected(topic); setQuery(''); setArea(''); closeReference(); }
    }, [props.route.focus]);
    const topics = useMemo(() => catalog.topics.filter(topic => !area || topic.area === area).map(topic => ({ topic, score: query.trim() ? helpScore(topic, query) : 1 }))
        .filter(entry => entry.score > 0).sort((a, b) => b.score - a.score || a.topic.id.localeCompare(b.topic.id)).map(entry => entry.topic), [query, area]);
    const loadEngine = async (query: { topic?: string; command?: string; tool?: string }, startLine = 1) => {
        const epoch = ++referenceEpoch.current;
        setLastQuery(query); setShowEngine(true); setEngine({ busy: true });
        try { const value = await request<EngineHelpView>('help.engine', { ...query, startLine }); if (epoch === referenceEpoch.current) { setEngine({ value, busy: false }); } }
        catch (failure) { if (epoch === referenceEpoch.current) { setEngine({ error: failure instanceof Error ? failure.message : String(failure), busy: false }); } }
    };
    const openEngine = (topic?: string) => loadEngine(topic ? { topic } : {});
    const openLink = async (destination: string) => {
        setLinkError(undefined);
        try { const reply = await request<{ opened: boolean }>('help.openExternal', { destination }); if (!reply.opened) { setLinkError('The browser could not open this link. Use the address shown in About AIRK.'); } }
        catch (failure) { setLinkError(failure instanceof Error ? failure.message : String(failure)); }
    };
    const route = selected.route ? parseRoute(selected.route) : undefined;
    return (
        <div class="help">
            <aside class="help-index">
                <input value={query} maxLength={256} aria-label="Search help" placeholder="Search Explorer and engine help (e.g. tester PDF)" onInput={event => setQuery((event.target as HTMLInputElement).value)} />
                <label>Area <select value={area} onChange={event => setArea((event.target as HTMLSelectElement).value)}><option value="">All topics</option>
                    {[...new Set(catalog.topics.map(topic => topic.area))].sort().map(value => <option key={value} value={value}>{value}</option>)}</select></label>
                <p class="muted" role="status">{topics.length} matching topic(s)</p>
                <div class="chips" aria-label="Help starting points">{['role-developer', 'role-tester', 'role-architect', 'role-writer', 'indexing-choices', 'document-types', 'assistance-tools', 'agent-handoffs', 'recovering-errors', 'about-airk'].map(id => {
                    const topic = catalog.topics.find(entry => entry.id === id);
                    return topic && <button type="button" class="chip" key={id} onClick={() => { setSelected(topic); setQuery(''); setArea(''); closeReference(); }}>{topic.title}</button>;
                })}</div>
                <ul>{topics.map(topic => (
                    <li key={topic.id}><button type="button" class={topic.id === selected.id ? 'link current' : 'link'} onClick={() => { setSelected(topic); closeReference(); }}>
                        {topic.title}{topic.applicability !== 'implemented' && <span class="tag">planned</span>}</button></li>
                ))}</ul>
                {!topics.length && <p class="muted">No bundled topic matches. Try fewer words or choose a role above.</p>}
                <button type="button" class="ghost" disabled={!props.state.trusted || !props.state.selectedProject} onClick={() => { void openEngine(); }}>Live AIRK engine reference</button>
                <form onSubmit={event => { event.preventDefault(); if (reference.trim()) { void loadEngine({ [referenceType]: reference.trim() }); } }}>
                    <label>Live reference type<select value={referenceType} onChange={event => setReferenceType((event.target as HTMLSelectElement).value as 'topic' | 'command' | 'tool')}>
                        <option value="tool">MCP tool schema</option><option value="command">CLI command help</option><option value="topic">Engine topic</option></select></label>
                    <label>Exact name<input value={reference} maxLength={referenceType === 'command' ? 160 : 128} placeholder="airk_search, collections extraction detail, or indexing" onInput={event => setReference((event.target as HTMLInputElement).value)} /></label>
                    <button type="submit" class="ghost" disabled={engine.busy || !reference.trim() || !props.state.trusted || !props.state.selectedProject}>Read live reference</button>
                </form>
                <p class="muted">Bundled Explorer and engine guidance {catalog.extensionVersion}. Available offline; live reference needs an authorized project connection.</p>
            </aside>
            <article class="help-topic">
                {showEngine ? (
                    <section>
                        {engine.busy && <p class="muted">Reading airk_help…</p>}
                        {engine.error && <p class="callout" role="alert">{engine.error} Bundled guidance remains available below.</p>}
                        {engine.value && <>
                            <p class="muted">AIRK engine help · {engine.value.topic}{engine.value.version ? ` · runtime ${engine.value.version}` : ''}</p>
                            <h2>{engine.value.title}</h2>
                            <pre class="log">{engine.value.content}</pre>
                            <p class="muted">{engine.value.tool ?? engine.value.command ?? engine.value.topic} · from line {engine.value.startLine ?? 1}</p>
                            {engine.value.nextLine ? <button type="button" class="ghost" disabled={engine.busy} onClick={() => { void loadEngine(lastQuery, engine.value!.nextLine); }}>Next page</button>
                                : engine.value.truncated && <p class="muted">The runtime reported incomplete help without a continuation. Check its version and retry.</p>}
                            {(engine.value.startLine ?? 1) > 1 && <button type="button" class="ghost" onClick={() => { void loadEngine(lastQuery); }}>First page</button>}
                            {engine.value.topics.length > 0 && <div class="chips">{engine.value.topics.map(topic => (
                                <button type="button" class="chip" key={topic.id} onClick={() => { void openEngine(topic.id); }}>{topic.title}</button>
                            ))}</div>}
                        </>}
                        <button type="button" class="ghost" onClick={closeReference}>Back to {selected.title}</button>
                    </section>
                ) : (
                    <section>
                        <p class="muted">{selected.area} · bundled {catalog.extensionVersion} · {selected.applicability === 'implemented' ? 'available in this version' : 'planned; not built in this version'}</p>
                        <Markdown text={selected.body} />
                        {selected.id === 'about-airk' && <nav class="actions" aria-label="AIRK public links">{Object.entries(ABOUT_LINKS).map(([destination, link]) =>
                            <button type="button" class="ghost" key={destination} onClick={() => { void openLink(destination); }}>{link.title} ↗</button>)}</nav>}
                        {linkError && <p role="alert">{linkError}</p>}
                        <div class="actions">
                            {route && selected.applicability === 'implemented' && <button type="button" onClick={() => props.go(route)}>Open this screen</button>}
                            {selected.relatedAirkHelp.filter(id => selected.id !== `engine.${id}`).map(id => <button type="button" class="ghost" key={id} onClick={() => { const topic = catalog.topics.find(entry => entry.id === `engine.${id}`); if (topic) { setSelected(topic); } }}>Engine guide: {id}</button>)}
                            {selected.relatedAirkHelp.map(id => <button type="button" class="ghost" key={`live-${id}`} disabled={!props.state.trusted || !props.state.selectedProject} onClick={() => { void openEngine(id); }}>Live reference: {id}</button>)}
                        </div>
                    </section>
                )}
            </article>
        </div>
    );
}
