// SPDX-License-Identifier: Apache-2.0
import catalog from 'airk-explorer-help';
import { ComponentChildren } from 'preact';
import { useMemo, useState } from 'preact/hooks';
import { EngineHelpView, HelpTopic } from '../explorerContracts';
import { parseRoute } from '../explorerRoutes';
import { request } from './api';
import { useAsync } from './facts';
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

function matches(topic: HelpTopic, query: string): boolean {
    const text = `${topic.title} ${topic.aliases.join(' ')} ${topic.body}`.toLowerCase();
    return query.toLowerCase().split(/\s+/).filter(Boolean).every(word => text.includes(word));
}

export function HelpScreen(props: ScreenProps) {
    const [query, setQuery] = useState('');
    const [selected, setSelected] = useState<HelpTopic>(catalog.topics.find(topic => topic.id === 'explorer.overview') ?? catalog.topics[0]);
    const engine = useAsync<EngineHelpView>();
    const topics = useMemo(() => catalog.topics.filter(topic => !query.trim() || matches(topic, query)), [query]);
    const openEngine = (topic?: string) => engine.run(() => request<EngineHelpView>('help.engine', topic ? { topic } : {}));
    const route = selected.route ? parseRoute(selected.route) : undefined;
    return (
        <div class="help">
            <aside class="help-index">
                <input value={query} aria-label="Search help" placeholder="Search help (e.g. index now)" onInput={event => setQuery((event.target as HTMLInputElement).value)} />
                <ul>{topics.map(topic => (
                    <li key={topic.id}><button type="button" class={topic.id === selected.id ? 'link current' : 'link'} onClick={() => { setSelected(topic); engine.setValue(undefined); }}>
                        {topic.title}{topic.applicability !== 'implemented' && <span class="tag">planned</span>}</button></li>
                ))}</ul>
                {!topics.length && <p class="muted">No Explorer topic matches.</p>}
                <button type="button" class="ghost" onClick={() => { void openEngine(); }}>AIRK engine topics</button>
                <p class="muted">Explorer help {catalog.extensionVersion}</p>
            </aside>
            <article class="help-topic">
                {engine.value || engine.busy || engine.error ? (
                    <section>
                        {engine.busy && <p class="muted">Reading airk_help…</p>}
                        {engine.error && <p class="callout" role="alert">{engine.error}</p>}
                        {engine.value && <>
                            <p class="muted">AIRK engine help · {engine.value.topic}{engine.value.version ? ` · runtime ${engine.value.version}` : ''}</p>
                            <h2>{engine.value.title}</h2>
                            {engine.value.content.split('\n').map((line, index) => <p key={index}>{line}</p>)}
                            {engine.value.truncated && <p class="muted">Truncated; ask an agent or run <code>airk tools call airk_help</code> for the rest.</p>}
                            {engine.value.topics.length > 0 && <div class="chips">{engine.value.topics.map(topic => (
                                <button type="button" class="chip" key={topic.id} onClick={() => { void openEngine(topic.id); }}>{topic.title}</button>
                            ))}</div>}
                        </>}
                        <button type="button" class="ghost" onClick={() => engine.setValue(undefined)}>Back to {selected.title}</button>
                    </section>
                ) : (
                    <section>
                        <p class="muted">{selected.area} · {selected.applicability === 'implemented' ? 'available in this version' : 'planned; not built in this version'}</p>
                        <Markdown text={selected.body} />
                        <div class="actions">
                            {route && selected.applicability === 'implemented' && <button type="button" onClick={() => props.go(route)}>Open this screen</button>}
                            {selected.relatedAirkHelp.map(id => <button type="button" class="ghost" key={id} onClick={() => { void openEngine(id); }}>Engine: {id}</button>)}
                        </div>
                    </section>
                )}
            </article>
        </div>
    );
}
