// SPDX-License-Identifier: Apache-2.0
import { useEffect, useState } from 'preact/hooks';
import { request } from './api';
import { shortToken } from './format';
import type { ScreenProps } from './workspaces';

// Shapes returned by the host's review module (explorerReview.ts); rendered as text only.
interface ChangedFile { path: string; change: string; staged: boolean; workingTree?: boolean; originalPath?: string; indexed: boolean; live: string; language?: string }
interface Changes { git: 'available' | 'unavailable' | 'no_repository'; files: ChangedFile[]; truncated: boolean; publication?: string; notes: string[] }
interface ImpactItem { name: string; kind: string; path: string; line?: number; references: number; referenceFiles: string[]; testReferences: number; truncated: boolean }
interface Impact { items: ImpactItem[]; files: number; truncated: boolean; publication?: string; notes: string[] }
interface Tests { candidates: { path: string; line: number; mentions: string; text: string }[]; declarations: number; pattern: string; truncated: boolean; notes: string[] }

const LIVE: Record<string, string> = { matches_index: 'matches index', changed_since_index: 'changed since index', not_saved: 'unsaved edits', missing: 'missing', unchecked: 'comparison unavailable', deleted: 'deleted', outside_workspace: 'outside workspace' };

function useReview<T>(type: 'review.changes' | 'review.impact' | 'review.tests', connected: boolean): { data?: T; error?: string; busy: boolean; load(): void } {
    const [data, setData] = useState<T>();
    const [error, setError] = useState<string>();
    const [busy, setBusy] = useState(false);
    const load = () => {
        if (!connected) { return; }
        setBusy(true); setError(undefined);
        request<T>(type, {}).then(setData, failure => setError(failure instanceof Error ? failure.message : String(failure))).finally(() => setBusy(false));
    };
    useEffect(load, [connected]);
    return { data, error, busy, load };
}
const connectedOf = (props: ScreenProps) => props.state.connection === 'ready' || props.state.connection === 'degraded';

function Frame(props: { busy: boolean; error?: string; reload(): void; connected: boolean; children: preact.ComponentChildren }) {
    return (
        <div class="review-screen">
            <div class="actions"><button class="ghost" disabled={props.busy || !props.connected} onClick={props.reload}>{props.busy ? 'Reading…' : 'Refresh'}</button></div>
            {!props.connected && <p class="callout">Review needs a connected AIRK service and a published index.</p>}
            {props.error && <p class="callout" role="alert">{props.error}</p>}
            {props.children}
        </div>
    );
}

export function ReviewChangesScreen(props: ScreenProps) {
    const connected = connectedOf(props);
    const { data, error, busy, load } = useReview<Changes>('review.changes', connected);
    return (
        <Frame busy={busy} error={error} reload={load} connected={connected}>
            {data && <>
                <p class="result-meta"><strong>{data.files.length} changed file(s){data.truncated ? ' · more exist' : ''}</strong>{data.publication && <> · publication <span title={data.publication}>{shortToken(data.publication)}</span></>}</p>
                {data.notes.map(note => <p key={note} class="muted">{note}</p>)}
                {data.files.length > 0 && (
                    <div class="table-wrap"><table class="grid">
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
        <Frame busy={busy} error={error} reload={load} connected={connected}>
            {data && <>
                {data.notes.map(note => <p key={note} class="muted">{note}</p>)}
                <p class="result-meta"><strong>{data.items.length} changed declaration(s)</strong> in {data.files} changed file(s){data.truncated ? ' · more exist' : ''}</p>
                {!data.items.length && <p class="muted">No indexed declarations were found in the changed files.</p>}
                <ul class="checklist">
                    {data.items.map(item => {
                        const key = `${item.path}:${item.name}`;
                        return (
                            <li key={key}>
                                <label class="check"><input type="checkbox" checked={!!checked[key]} onChange={event => setChecked(prior => ({ ...prior, [key]: (event.target as HTMLInputElement).checked }))} />
                                    <span><strong>{item.name}</strong> ({item.kind}) at <code>{item.path}{item.line ? `:${item.line}` : ''}</code> — {item.references}{item.truncated ? '+' : ''} textual reference(s)
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
        <Frame busy={busy} error={error} reload={load} connected={connected}>
            {data && <>
                {data.notes.map(note => <p key={note} class="callout">{note}</p>)}
                <p class="result-meta"><strong>{data.candidates.length} declared test candidate(s)</strong> for {data.declarations} changed declaration(s){data.truncated ? ' · more may exist' : ''}</p>
                <p class="muted">Test paths match <code>{data.pattern}</code> (setting airk.explorer.review.testPathPattern).</p>
                <ol class="hits">{data.candidates.map(candidate => (
                    <li key={`${candidate.path}:${candidate.line}:${candidate.mentions}`} class="hit card">
                        <div class="hit-head"><code class="path">{candidate.path}:{candidate.line}</code><span class="tag">mentions {candidate.mentions}</span><span class="tag">not executed</span></div>
                        <pre class="excerpt">{candidate.text}</pre>
                    </li>
                ))}</ol>
            </>}
        </Frame>
    );
}
