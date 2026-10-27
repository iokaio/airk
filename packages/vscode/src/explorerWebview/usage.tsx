// SPDX-License-Identifier: Apache-2.0
import { useEffect, useRef, useState } from 'preact/hooks';
import { request } from './api';

/** Only the host's escaped usage renderer supplies this markup; no scripts or source HTML. */
export function UsageReport(props: { available: boolean }) {
    const [html, setHtml] = useState<string>();
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string>();
    const sequence = useRef(0), pending = useRef(false), report = useRef<HTMLDivElement>(null);
    const load = async (action?: string, days?: number) => {
        if (pending.current) { return; }
        pending.current = true;
        const active = ++sequence.current;
        setBusy(true); setError(undefined);
        try {
            const result = await request<{ html: string }>(action ? 'usage.action' : 'usage.read', action ? { action, ...(days !== undefined ? { days } : {}) } : {});
            if (active === sequence.current) { setHtml(result.html); }
        } catch (failure) { if (active === sequence.current) { setError(failure instanceof Error ? failure.message : String(failure)); } }
        finally { pending.current = false; if (active === sequence.current) { setBusy(false); } }
    };
    useEffect(() => { if (props.available) { void load(); } return () => { sequence.current++; }; }, [props.available]);
    if (!props.available) { return <p class="callout">Select an open, trusted AIRK project to view its usage report.</p>; }
    return <article class="usage-report" aria-busy={busy}>
        <h2>Usage &amp; Savings</h2>
        <p class="muted">AIRK measures its own activity. Token and cost estimates are comparisons, not a record of your agent’s bill.</p>
        {busy && <p role="status">Loading usage report…</p>}
        {error && <div class="callout" role="alert"><p>{error}</p>{html && <p>The previous report remains below. Refresh to update it.</p>}
            <button disabled={busy} onClick={() => void load('refresh')}>Retry report</button></div>}
        <fieldset disabled={busy}>
            {html && <button class="ghost" onClick={() => void load('json')}>Open as JSON</button>}
            <div ref={report} onClick={event => {
                const action = (event.target as HTMLElement).closest<HTMLElement>('button[data-usage]')?.dataset.usage;
                if (action && !busy) { void load(action); }
            }} onChange={event => {
                const target = event.target as HTMLSelectElement;
                if (target.id === 'usage-days' && !busy) { void load('days', Number(target.value)); }
            }} dangerouslySetInnerHTML={{ __html: html ?? '' }} />
        </fieldset>
    </article>;
}
