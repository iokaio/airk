// SPDX-License-Identifier: Apache-2.0
import { ComponentChildren } from 'preact';
import { useEffect, useRef, useState } from 'preact/hooks';
import { request } from './api';
import type { WebviewMessageOf } from '../explorerContracts';

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

const ICONS: Record<string, string> = {
    overview: 'M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z',
    search: 'M10.5 3a7.5 7.5 0 1 0 0 15 7.5 7.5 0 0 0 0-15M16 16l5 5',
    code: 'm8 6-6 6 6 6M16 6l6 6-6 6M14 3l-4 18',
    docs: 'M5 3h9l5 5v13H5zM14 3v6h5M8 13h8M8 17h6',
    evidence: 'M12 2 3 6v6c0 5 9 10 9 10s9-5 9-10V6zM8 12l3 3 5-6',
    review: 'M9 5h12M9 12h12M9 19h12M2 5l1 1 2-2M2 12l1 1 2-2M2 19l1 1 2-2',
    strategies: 'M4 4h6v6H4zM14 14h6v6h-6zM14 7h5v7M7 10v7h7',
    manage: 'M4 7h16M4 17h16M8 4v6M16 14v6',
    help: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18M9 9a3 3 0 1 1 4 3c-1 .5-1 1-1 2M12 17h.01',
};
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
