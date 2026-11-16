// SPDX-License-Identifier: Apache-2.0
import { ComponentChildren } from 'preact';
import { useEffect, useRef, useState } from 'preact/hooks';
import { request } from './api';
import type { WebviewMessageOf } from '../explorerContracts';
import { DESTINATION_ICONS } from '../../../ui/src/navigation';

/** Complete canonical native data survives the friendly cards and both copies. */
export function CompleteNativeResponse(props: { value?: Record<string, unknown> }) {
    const [message, setMessage] = useState('');
    useEffect(() => { setMessage(''); }, [props.value]);
    if (!props.value) { return null; }
    const json = JSON.stringify(props.value, null, 2);
    const copy = async (markdown: boolean) => {
        try { await request('clipboard.write', { text: markdown ? `Complete AIRK native response, including independent member publications and dated receipts.\n\n\`\`\`json\n${json}\n\`\`\`\n` : json }); setMessage('Copied complete response'); }
        catch (error) { setMessage(error instanceof Error ? error.message : String(error)); }
    };
    return <details><summary>Complete native response</summary>
        <p class="muted">Includes every member outcome, publication, execution policy, retry detail and saved-source receipt. Viewing or copying does not refresh these observations.</p>
        <div class="actions"><button class="ghost" onClick={() => { void copy(false); }}>Copy JSON</button><button class="ghost" onClick={() => { void copy(true); }}>Copy Markdown</button>{message && <span role="status">{message}</span>}</div>
        <pre>{json}</pre>
    </details>;
}

export function CopyDetails(props: { section: WebviewMessageOf<'details.copy'>['section']; project?: string }) {
    const [message,setMessage]=useState(''),[busy,setBusy]=useState(false);
    useEffect(()=>{setMessage('');},[props.section,props.project]);
    return <span class="copy-details"><button type="button" class="ghost" disabled={busy}
        title="Copy displayed diagnostic metadata, including any displayed paths and project names. No source contents are copied."
        onClick={async()=>{setBusy(true);setMessage('');try{await request('details.copy',{section:props.section,...(props.project?{project:props.project}:{})});setMessage('Copied');}catch(error){setMessage(error instanceof Error?error.message:'Copy failed');}finally{setBusy(false);}}}>{busy?'Copying…':'Copy details'}</button>
        {message&&<span class="muted" role="status"> {message}</span>}</span>;
}

/** Original inline vectors: no icon font, remote asset or extra package. */
export function BrandMark() {
    return <svg class="brand-mark" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="m2.5 19 6-14 6 14M5 13.5h7" /><path class="brand-chevron" d="m17 7 5 5-5 5" />
    </svg>;
}

const ICONS: Readonly<Record<string, string>> = { ...DESTINATION_ICONS, indexing: DESTINATION_ICONS.indexes, manage: DESTINATION_ICONS.service };
export function WorkspaceIcon(props: { name: string }) {
    return <svg class="workspace-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d={ICONS[props.name] ?? ICONS.overview} /></svg>;
}

/** User-initiated sharing of one complete snapshot, never general draft access. */
export function ShareSnapshot(props: { kind: 'studio' | 'planner'; revision: number; content: string; publication?: string; disabled?: boolean }) {
    const [token, setToken] = useState<string>(), [error, setError] = useState<string>(), [busy, setBusy] = useState(false);
    const generation = JSON.stringify([props.kind, props.revision, props.content, props.publication]), latest = useRef(generation);
    latest.current = generation;
    useEffect(() => { setToken(undefined); }, [props.kind, props.revision, props.content, props.publication]);
    const share = async () => {
        setBusy(true); setError(undefined); setToken(undefined);
        try {
            const reply = await request<{ shared: boolean; token?: string }>('handoff.share', { kind: props.kind, revision: props.revision, content: props.content,
                ...(props.publication ? { publication: props.publication } : {}) });
            if (reply.shared && latest.current === generation) { setToken(reply.token); }
        } catch (failure) { setError(failure instanceof Error ? failure.message : String(failure)); }
        finally { setBusy(false); }
    };
    return <div class="callout">
        <button class="ghost" disabled={busy || props.disabled || !props.content.trim() || new TextEncoder().encode(props.content).length > 24576} onClick={() => void share()}>Share snapshot with agent</button>
        <p class="muted">Shares this complete text for ten minutes through Explorer Assistance. Editing the draft or changing its binding/evidence revokes the token. Maximum 24 KiB; text is never truncated.</p>
        {token && <><p role="status">Ask your agent to call <code>airk_explorer_handoff_get</code> with this token:</p><code>{token}</code>
            <button class="ghost" onClick={async () => { try { await request('clipboard.write', { text: token }); } catch (failure) { setError(String(failure)); } }}>Copy share token</button></>}
        {error && <p role="alert">{error}</p>}
    </div>;
}

/** Keep keyboard focus within an open dialog and restore its invoking control on close. */
export function dialogFocus(element: HTMLElement | null, close?: () => void): () => void {
    if (!element) { return () => undefined; }
    const previous = document.activeElement as HTMLElement | null;
    const controls = () => [...element.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex="0"]')];
    const key = (event: KeyboardEvent) => {
        if (event.key === 'Escape' && close) { event.preventDefault(); close(); }
        if (event.key === 'Tab') {
            const list = controls(), first = list[0], last = list.at(-1);
            if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
            else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
        }
    };
    controls()[0]?.focus(); element.addEventListener('keydown', key);
    return () => { element.removeEventListener('keydown', key); if (previous?.isConnected) { previous.focus(); } };
}
export function useDialogFocus(close?: () => void) {
    const element = useRef<HTMLElement>(null);
    useEffect(() => dialogFocus(element.current, close), []);
    return element;
}

/** A definition list of observations; values are rendered as text, never HTML. */
export function Facts(props: { rows: [string, ComponentChildren][] }) {
    return <dl class="facts">{props.rows.map(([name, value]) => [<dt key={`${name}-t`}>{name}</dt>, <dd key={`${name}-d`}>{value}</dd>])}</dl>;
}
