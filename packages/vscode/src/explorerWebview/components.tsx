// SPDX-License-Identifier: Apache-2.0
import { ComponentChildren } from 'preact';
import { useEffect, useRef, useState } from 'preact/hooks';
import { request } from './api';

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
