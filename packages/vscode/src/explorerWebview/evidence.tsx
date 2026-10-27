// SPDX-License-Identifier: Apache-2.0
import { useEffect, useState } from 'preact/hooks';
import { ChangesView, CitationCheckView, InspectView } from '../explorerContracts';
import { request } from './api';
import { Facts } from './components';
import { OpenButton, useAsync } from './facts';
import { ago, observed, shortToken } from './format';
import type { ScreenProps } from './workspaces';

const LIVE_CLASS: Record<string, string> = { matches_index: 'ok', changed_since_index: 'warn', not_saved: 'warn', missing: 'error', unchecked: 'info', outside_workspace: 'info' };

export function InspectorScreen(props: ScreenProps) {
    const view = useAsync<InspectView>();
    useEffect(() => { if (props.inspectRef) { void view.run(() => request<InspectView>('evidence.inspect', { ref: props.inspectRef! })); } }, [props.inspectRef]);
    if (!props.inspectRef) {
        return <p class="callout">Choose <strong>Inspect</strong> on a search hit, fact or checked citation to see its indexed record and compare it with the saved file.</p>;
    }
    return (
        <div>
            {view.busy && <p class="muted">Reading the indexed record…</p>}
            {view.error && <p class="callout" role="alert">{view.error}</p>}
            {view.value && (
                <article class="card">
                    <div class="hit-head">
                        <code class="path">{view.value.path}:{view.value.startLine}{view.value.endLine !== view.value.startLine ? `–${view.value.endLine}` : ''}</code>
                        <span class={`pill ${LIVE_CLASS[view.value.live] ?? 'info'}`}>{view.value.live.replaceAll('_', ' ')}</span>
                        <span class="spacer" /><OpenButton ref_={view.value.ref} />
                    </div>
                    <p>{view.value.liveMessage}</p>
                    <Facts rows={[
                        ['Publication', <span title={view.value.publication}>{shortToken(view.value.publication)}</span>],
                        ['Indexed content hash', <code>{view.value.contentHash}</code>],
                        ['Language · parse', `${observed(view.value.language)} · ${observed(view.value.parseStatus)}`],
                        ['Extraction · line basis', `${observed(view.value.extractionMethod)} · ${observed(view.value.lineBasis)}`],
                        ['Indexed lines · bytes', `${observed(view.value.indexedLines)} · ${observed(view.value.sourceBytes)}`],
                        ['Chunks · facts', `${observed(view.value.chunks)} · ${observed(view.value.facts)}`],
                        ['Source root', <code class="path">{view.value.sourceRoot ?? 'not observed'}</code>],
                    ]} />
                    <p class="muted">The hash compares saved bytes only. A match shows the file is unchanged since indexing; it does not verify what the code does.</p>
                </article>
            )}
        </div>
    );
}

const STATUS_CLASS: Record<string, string> = { valid: 'ok', changed_since_index: 'warn', not_saved: 'warn', line_out_of_range: 'error', not_found: 'error', missing: 'error', outside_collection: 'error', unchecked: 'warn' };

export function CheckerScreen(props: ScreenProps) {
    const [text, setText] = useState('');
    const view = useAsync<CitationCheckView>();
    return (
        <div>
            <form onSubmit={event => { event.preventDefault(); if (text.trim()) { void view.run(() => request<CitationCheckView>('evidence.check', { text })); } }}>
                <label class="block">Paste an agent report, plan or PR description
                    <textarea rows={8} value={text} maxLength={65536} placeholder="Citations like src/auth/session.ts:84-103 or docs/spec.md#L3-L9"
                        onInput={event => setText((event.target as HTMLTextAreaElement).value)} />
                </label>
                <button type="submit" disabled={view.busy || !text.trim()}>{view.busy ? 'Checking…' : 'Check citations'}</button>
            </form>
            <p class="muted">Supported forms: <code>path:line</code>, <code>path:start-end</code>, <code>path#Lx-Ly</code>. Prose without citations is unsupported, never a pass.</p>
            {view.error && <p class="callout" role="alert">{view.error}</p>}
            {view.value && !view.value.supported && <p class="callout">No supported citation was found. Nothing was verified.</p>}
            {view.value && view.value.supported && (
                <div class="table-wrap">
                    <p class="result-meta"><strong>{view.value.citations.length} citations checked</strong>{view.value.limited ? ' · more were present and not checked' : ''}
                        {' · '}publication <span title={view.value.publication}>{shortToken(view.value.publication)}</span></p>
                    <table class="grid">
                        <thead><tr><th scope="col">Citation</th><th scope="col">Result</th><th scope="col">Detail</th><th scope="col"><span class="sr-only">Actions</span></th></tr></thead>
                        <tbody>{view.value.citations.map((entry, index) => (
                            <tr key={`${entry.text}-${index}`}>
                                <th scope="row"><code class="path">{entry.path}:{entry.startLine}{entry.endLine !== entry.startLine ? `–${entry.endLine}` : ''}</code></th>
                                <td><span class={`pill ${STATUS_CLASS[entry.status] ?? 'info'}`}>{entry.status.replaceAll('_', ' ')}</span></td>
                                <td>{entry.message}</td>
                                <td class="actions">{entry.ref && <><button type="button" class="ghost" onClick={() => props.inspect(entry.ref!)}>Inspect</button><OpenButton ref_={entry.ref} /></>}</td>
                            </tr>
                        ))}</tbody>
                    </table>
                </div>
            )}
        </div>
    );
}

export function ChangesScreen(props: ScreenProps) {
    const [since, setSince] = useState(props.publications[1]?.token ?? '');
    const view = useAsync<ChangesView>();
    const valid = /^airk-r1:[0-9A-Za-z:_-]+$/.test(since) && since.length <= 256;
    return (
        <div>
            <form class="search-form" onSubmit={event => { event.preventDefault(); if (valid) { void view.run(() => request<ChangesView>('evidence.changes', { since, limit: 50 })); } }}>
                <label class="grow">Since publication
                    <input value={since} maxLength={256} placeholder="airk-r1:… (from a plan, a report or this session)" onInput={event => setSince((event.target as HTMLInputElement).value.trim())} />
                </label>
                <button type="submit" disabled={view.busy || !valid}>{view.busy ? 'Comparing…' : 'Show changes'}</button>
            </form>
            {props.publications.length > 0 && (
                <p class="muted">Seen in this session: {props.publications.map(entry => (
                    <button type="button" class="chip" key={entry.token} title={entry.token} onClick={() => setSince(entry.token)}>{shortToken(entry.token)} · {ago(entry.observedAt)}</button>
                ))}</p>
            )}
            <p class="muted">Changes are bounded observations between two publications; they do not prove filesystem intent, and unavailable baselines are reported as such.</p>
            {view.error && <p class="callout" role="alert">{view.error}</p>}
            {view.value && (
                <section>
                    <p class="result-meta"><strong>{view.value.items.length} changes{view.value.truncated ? ' · more may exist' : ''}</strong> · baseline {view.value.baseline.replaceAll('_', ' ')}
                        {' · '}now <span title={view.value.publication}>{shortToken(view.value.publication)}</span></p>
                    {!view.value.items.length && <p class="callout">No indexed change was observed between these publications.</p>}
                    <ul class="hits">{view.value.items.map((item, index) => (
                        <li key={index} class="hit card"><code class="path">{item.path ?? `(${item.subject})`}</code> <span class="tag">{item.subject}</span>
                            {item.causes.map(cause => <span class="tag" key={cause}>{cause.replaceAll('_', ' ')}</span>)}
                            {item.dimensions.length > 0 && <p class="muted">changed: {item.dimensions.join(', ')}</p>}</li>
                    ))}</ul>
                    {view.value.coverage.length > 0 && <details><summary>Coverage</summary><ul>{view.value.coverage.map(line => <li key={line}>{line}</li>)}</ul></details>}
                </section>
            )}
        </div>
    );
}
