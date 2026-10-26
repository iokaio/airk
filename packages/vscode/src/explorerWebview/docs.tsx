// SPDX-License-Identifier: Apache-2.0
import { useState } from 'preact/hooks';
import { FactListView } from '../explorerContracts';
import { request } from './api';
import { FactList, useAsync } from './facts';
import type { ScreenProps } from './workspaces';

const KINDS: [string, string][] = [['any', 'All'], ['requirement', 'Requirements'], ['decision', 'Decisions'], ['design', 'Design'],
    ['architecture', 'Architecture'], ['feature', 'Features'], ['acceptance', 'Acceptance'], ['date', 'Dates'], ['section', 'Sections'],
    ['table_row', 'Table rows'], ['table_cell', 'Table cells']];
type Kind = 'any' | 'requirement' | 'decision' | 'design' | 'architecture' | 'feature' | 'acceptance' | 'date' | 'section' | 'table_row' | 'table_cell';

export function DocumentFactsScreen(_props: ScreenProps) {
    const [kind, setKind] = useState<Kind>('requirement');
    const [name, setName] = useState('');
    const [prefix, setPrefix] = useState('');
    const result = useAsync<FactListView>();
    const run = (next: Kind = kind) => result.run(() => request<FactListView>('docs.facts', { kind: next, limit: 50,
        ...(name.trim() ? { name: name.trim() } : {}), ...(prefix.trim() ? { pathPrefix: prefix.trim() } : {}) }));
    return (
        <div>
            <div class="chips" role="group" aria-label="Fact kind">
                {KINDS.map(([value, label]) => (
                    <button type="button" key={value} class={value === kind ? 'chip selected' : 'chip'} aria-pressed={value === kind}
                        onClick={() => { setKind(value as Kind); void run(value as Kind); }}>{label}</button>
                ))}
            </div>
            <form class="search-form" onSubmit={event => { event.preventDefault(); void run(); }}>
                <label class="grow">Text contains<input value={name} maxLength={256} placeholder="Optional substring of the fact text" onInput={event => setName((event.target as HTMLInputElement).value)} /></label>
                <label>Path scope<input value={prefix} maxLength={1024} placeholder="e.g. docs/" onInput={event => setPrefix((event.target as HTMLInputElement).value)} /></label>
                <button type="submit" disabled={result.busy}>{result.busy ? 'Searching…' : 'Find facts'}</button>
            </form>
            <p class="muted">Facts come from Markdown, PDF, Word, Excel and PowerPoint extraction with their original locations. They are candidates found by document cues, not verified requirements or live code behavior.</p>
            {result.error && <p class="callout" role="alert">{result.error}</p>}
            {result.value && <FactList list={result.value} empty="No indexed document fact of this kind matched." />}
        </div>
    );
}
