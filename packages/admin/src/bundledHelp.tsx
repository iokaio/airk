// SPDX-License-Identifier: Apache-2.0
// Exact versioned Help bytes are shared with the editor and distributable site.
import { h, type JSX, type VNode } from 'preact';
import { useLayoutEffect, useRef, useState } from 'preact/hooks';
import manifest from '../../ui/generated/help-manifest.json';
import { helpExcerpt, helpScore } from '../../ui/src/helpSearch';
import { helpToolPresentation, renderHelpNode } from '../../ui/src/helpToolPresentation';
import { formatHelpLocation, parseHelpLocation } from '../../ui/src/helpRoute';
import { Markdown } from './help';
import { Text } from './render';

export type BundledHelpProps = { params?: URLSearchParams; seq?: number; onNavigate?(params: URLSearchParams, replace: boolean): void };
export function BundledHelp(props: BundledHelpProps): JSX.Element {
  const initial = parseHelpLocation(props.params);
  const [query, setQuery] = useState(initial.query);
  const [selected, setSelected] = useState<string | null>(initial.guide);
  const [toolName, setToolName] = useState(initial.tool);
  useLayoutEffect(() => {
    const next = parseHelpLocation(props.params);
    setQuery(next.query); setSelected(next.guide); setToolName(next.tool);
  }, [props.seq]);
  const navigate = (next: Partial<{ query: string; guide: string | null; tool: string }>, replace = false): void => {
    const value = { query, guide: selected, tool: toolName, ...next };
    setQuery(value.query); setSelected(value.guide); setToolName(value.tool);
    props.onNavigate?.(formatHelpLocation(value), replace);
  };
  const article = useRef<HTMLElement>(null), search = useRef<HTMLInputElement>(null), previous = useRef(selected);
  useLayoutEffect(() => {
    if (selected === previous.current) return;
    previous.current = selected;
    if (selected === null) search.current?.focus();
    else { article.current?.focus(); article.current?.scrollIntoView({ block: 'start' }); }
  }, [selected]);
  const matches = manifest.topics.map(topic => ({ topic, score: query.trim() ? helpScore(topic, query) : 1 }))
    .filter(entry => entry.score > 0).sort((left, right) => right.score - left.score || left.topic.id.localeCompare(right.topic.id));
  const topic = manifest.topics.find(entry => entry.id === selected);
  const tool = manifest.tools.find(entry => entry.name === toolName);
  return <section class="panel bundled-help" aria-label="Offline Help">
    <h2>Find a guide</h2>
    <p>Installation, portable use, trust, indexes and recovery. Bundled AIRK {manifest.productVersion} guidance is available without a service connection.</p>
    <div class="actions" aria-label="Help starting tasks">{[
      ['indexing.library', 'Create an index'], ['management.agents', 'Connect an agent'], ['recovering-errors', 'Find missing results'],
    ].map(([id, label]) => <button type="button" class="secondary" key={id} onClick={() => navigate({ guide: id! })}>{label}</button>)}</div>
    <label>Search complete guides<input ref={search} type="search" value={query} maxLength={256} onInput={event => navigate({ query: event.currentTarget.value }, true)} /></label>
    <p role="status">{matches.length} matching guides</p>
    {matches.length === 0 ? <p>No guide matches. Your selected article and query remain available.</p> : null}
    <details open={selected === null}><summary>Guides</summary><ul>{matches.map(({ topic }) => <li key={topic.id}>
      <button type="button" class="link" aria-current={selected === topic.id ? 'page' : undefined} onClick={() => navigate({ guide: topic.id })}><Text value={topic.title} /></button>
      {query.trim() ? <p class="small muted"><Text value={topic.area} /> · Preview: <Text value={helpExcerpt(topic.body, query)} /></p> : null}
    </li>)}</ul></details>
    <details><summary>Bundled public tool reference</summary>
      <label>Tool<select value={toolName} onChange={event => navigate({ tool: event.currentTarget.value })}><option value="">Choose a tool</option>
        {manifest.tools.map(entry => <option key={entry.name} value={entry.name}>{entry.name}</option>)}</select></label>
      {tool ? renderHelpNode<VNode>((tag, children) => h(tag, null, ...children), helpToolPresentation(tool)) : null}
    </details>
    {topic ? <article ref={article} tabIndex={-1}><h3><Text value={topic.title} /></h3>
      <p class="muted">Bundled {manifest.productVersion} · <Text value={topic.area} /> · {topic.applicability === 'implemented' ? 'available in this version' : 'planned'}</p>
      <Markdown source={topic.body} />
      <button type="button" class="secondary" onClick={() => navigate({ guide: null })}>Back to guides</button>
    </article> : null}
  </section>;
}
