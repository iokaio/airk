// SPDX-License-Identifier: Apache-2.0
import { useEffect, useRef, useState } from 'preact/hooks';
import { request } from './api';
import { shortToken } from './format';
import type { ScreenProps } from './workspaces';

// Shapes returned by the host's review module (explorerReview.ts); rendered as text only.
interface ChangedFile { path: string; change: string; staged: boolean; workingTree?: boolean; originalPath?: string; indexed: boolean; live: string; language?: string }
interface Changes { git: 'available' | 'unavailable' | 'no_repository'; files: ChangedFile[]; truncated: boolean; publication?: string; notes: string[]; offset: number; total: number; limit: number; snapshot: string }
interface ImpactItem { name: string; kind: string; path: string; line?: number; references: number; referenceFiles: string[]; testReferences: number; truncated: boolean }
interface Impact { items: ImpactItem[]; files: number; truncated: boolean; publication?: string; notes: string[] }
interface Tests { candidates: { path: string; line?: number; mentions: string; text: string; reason?: string }[]; declarations: number | null; pattern: string; truncated: boolean; notes: string[]; publication?: string; analysis: 'complete' | 'partial' | 'unavailable'; analysisError?: string }

const LIVE: Record<string, string> = { matches_index: 'matches index', changed_since_index: 'changed since index', not_saved: 'unsaved edits', missing: 'missing', unchecked: 'comparison unavailable', deleted: 'deleted', outside_workspace: 'outside workspace' };

interface Page { offset: number; snapshot: string; publication?: string }
function useReview<T>(type: 'review.changes' | 'review.impact' | 'review.tests', connected: boolean): { data?: T; error?: string; busy: boolean; load(page?: Page): void } {
    const [data, setData] = useState<T>();
    const [error, setError] = useState<string>();
    const [busy, setBusy] = useState(false);
    const sequence = useRef(0);
    const load = (page?: Page) => {
        if (!connected) { return; }
        const active = ++sequence.current;
        setBusy(true); setError(undefined);
        request<T>(type, page ?? {}).then(value => { if (active === sequence.current) { setData(value); } }, failure => {
            if (active === sequence.current) { setError(failure instanceof Error ? failure.message : String(failure)); }
        }).finally(() => { if (active === sequence.current) { setBusy(false); } });
    };
    useEffect(() => { setData(undefined); setError(undefined); setBusy(false); load(); return () => { sequence.current++; }; }, [connected]);
    return { data, error, busy, load };
}
const connectedOf = (props: ScreenProps) => props.state.connection === 'ready' || props.state.connection === 'degraded';

function Frame(props: { busy: boolean; error?: string; reload(): void; connected: boolean; children: preact.ComponentChildren }) {
    return (
        <div class="review-screen" aria-busy={props.busy}>
            <div class="actions"><button class="ghost" disabled={props.busy || !props.connected} onClick={props.reload}>{props.busy ? 'Reading…' : 'Refresh'}</button></div>
            {!props.connected && <p class="callout">Review needs a connected AIRK service and a published index.</p>}
            {props.error && <p class="callout" role="alert">{props.error}</p>}
            {props.connected && props.children}
        </div>
    );
}

export function ReviewChangesScreen(props: ScreenProps) {
    const connected = connectedOf(props);
    const { data, error, busy, load } = useReview<Changes>('review.changes', connected);
    return (
        <Frame busy={busy} error={error} reload={() => load()} connected={connected}>
            {data && <>
                <p class="result-meta"><strong>{data.total ? `${data.offset + 1}–${data.offset + data.files.length} of ${data.total}` : '0'} changed file(s)</strong>{data.publication && <> · publication <span title={data.publication}>{shortToken(data.publication)}</span></>}</p>
                {error && <p class="muted">The last successful page is shown below. Refresh to restart the list before paging again.</p>}
                <nav class="actions pagination" aria-label="Changed file pages">
                    <button class="ghost" disabled={busy || !!error || !connected || data.offset === 0} onClick={() => load({ offset: Math.max(0, data.offset - data.limit), snapshot: data.snapshot, publication: data.publication })}>Previous</button>
                    <span role="status" aria-live="polite" aria-atomic="true">Page {Math.floor(data.offset / data.limit) + 1} of {Math.max(1, Math.ceil(data.total / data.limit))}</span>
                    <button class="ghost" disabled={busy || !!error || !connected || !data.truncated} onClick={() => load({ offset: data.offset + data.limit, snapshot: data.snapshot, publication: data.publication })}>Next</button>
                </nav>
                {data.notes.map(note => <p key={note} class="muted">{note}</p>)}
                {data.total === 0 && data.git === 'available' && <p class="muted">No uncommitted Git changes were found.</p>}
                {data.files.length > 0 && (
                    <div class="table-wrap" tabIndex={0} role="region" aria-label="Changed files; scroll horizontally if needed"><table class="grid">
                        <thead><tr><th>Path</th><th>Change</th><th>Indexed</th><th>Saved bytes</th></tr></thead>
                        <tbody>{data.files.map(file => (
                            <tr key={file.path}><td><code>{file.path}</code></td><td>{file.change}{file.staged ? ' (staged)' : ''}{file.workingTree ? ' (working tree)' : ''}{file.originalPath ? ` from ${file.originalPath}` : ''}</td><td>{file.indexed ? file.language ?? 'yes' : 'no'}</td>
                                <td>{LIVE[file.live] ?? file.live}</td></tr>
                        ))}</tbody>
                    </table></div>
                )}
            </>}
        </Frame>
    );
}

export function ReviewImpactScreen(props: ScreenProps) {
    const connected = connectedOf(props);
    const { data, error, busy, load } = useReview<Impact>('review.impact', connected);
    const [checked, setChecked] = useState<Record<string, boolean>>({});
    return (
        <Frame busy={busy} error={error} reload={() => load()} connected={connected}>
            {data && <>
                {data.notes.map(note => <p key={note} class="muted">{note}</p>)}
                <p class="result-meta">{data.items.length} sampled declaration(s) from {data.files} changed file(s){data.truncated ? ' · incomplete review' : ''}</p>
                {!data.items.length && <p class="muted">No indexed declarations were found in the changed files.</p>}
                <ul class="checklist">
                    {data.items.map(item => {
                        const key = `${item.path}:${item.name}`;
                        return (
                            <li key={key}>
                                <label class="check"><input type="checkbox" checked={!!checked[key]} onChange={event => setChecked(prior => ({ ...prior, [key]: (event.target as HTMLInputElement).checked }))} />
                                    <span><code class="source-inline">{item.name}</code> ({item.kind}) at <code>{item.path}{item.line ? `:${item.line}` : ''}</code> — {item.references}{item.truncated ? '+' : ''} textual reference(s)
                                        {item.referenceFiles.length ? <> in {item.referenceFiles.map(file => <code key={file}>{file}</code>).reduce((all: preact.ComponentChildren[], node, index) => index ? [...all, ', ', node] : [node], [])}</> : null}
                                        {item.testReferences ? ` · ${item.testReferences} in test paths` : ''}</span></label>
                            </li>
                        );
                    })}
                </ul>
                <p class="muted">Ticks are for your own review in this session; they are not saved and prove nothing.</p>
            </>}
        </Frame>
    );
}

export function ReviewTestsScreen(props: ScreenProps) {
    const connected = connectedOf(props);
    const { data, error, busy, load } = useReview<Tests>('review.tests', connected);
    return (
        <Frame busy={busy} error={error} reload={() => load()} connected={connected}>
            <p>Test candidates for your uncommitted Git changes. Refresh rechecks the same change set and published index; unchanged inputs produce the same candidates. It does not run tests.</p>
            {data && <>
                <p class="result-meta">{data.declarations === null ? 'Indexed analysis unavailable' : `${data.declarations} sampled declarations examined`}{data.analysis === 'partial' ? ' · partial candidate list' : ''}
                    {data.publication && <> · publication <span title={data.publication}>{shortToken(data.publication)}</span></>}</p>
                {data.analysisError && <p class="callout" role="alert">{data.analysisError}</p>}
                {(['changed_test_file', 'identifier_reference'] as const).map(reason => {
                    const items = data.candidates.filter(candidate => candidate.reason === reason);
                    const files = new Set(items.map(item => item.path)).size;
                    return <section key={reason} aria-label={reason === 'changed_test_file' ? 'Changed test files' : 'Indexed test references'}>
                        <h2>{reason === 'changed_test_file' ? `Changed test files (${files})` : data.analysis === 'unavailable' ? 'Indexed test references (unavailable)' : `Indexed test references (${items.length} in ${files} files)`}</h2>
                        <p class="muted">{reason === 'changed_test_file' ? 'Review these because Git reports edits to them, even if they have not been indexed.' : 'These indexed test passages mention sampled declarations from changed source files. Textual matches suggest where to investigate; they do not prove a dependency or coverage.'}</p>
                        {!items.length && <p class="muted">{reason === 'changed_test_file' ? 'No changed test files match the configured test-path pattern.' : data.analysis === 'unavailable' ? 'No indexed test-reference result is available.' : 'No candidates were found in the sampled scope. Relevant tests may exist outside it.'}</p>}
                        <ol class="hits">{items.map(candidate => <li key={`${candidate.path}:${candidate.line}:${candidate.mentions}`} class="hit card">
                            <div class="hit-head"><code class="path">{candidate.path}{candidate.line ? `:${candidate.line}` : ''}</code>{candidate.mentions && <span class="tag">mentions {candidate.mentions}</span>}<span class="tag">not run</span></div>
                            {candidate.text && <pre class="excerpt">{candidate.text}</pre>}
                        </li>)}</ol>
                    </section>;
                })}
                <details><summary>Scope and limitations</summary>
                    {data.notes.map(note => <p key={note} class="muted">{note}</p>)}
                    <p class="muted">Test paths match <code>{data.pattern}</code> (setting airk.explorer.review.testPathPattern).</p>
                </details>
            </>}
        </Frame>
    );
}
