// SPDX-License-Identifier: Apache-2.0
import { createContext, type ComponentChildren } from 'preact';
import { useContext, useState } from 'preact/hooks';
import { request } from './api';

/** A dismissal belongs to the notice content, never the current route or elapsed time. */
export function noticeKey(kind: string, ...identity: unknown[]): string {
    return JSON.stringify([kind, ...identity]);
}

export const NoticeContext = createContext<{ dismissed: readonly string[]; project?: string }>({ dismissed: [] });

/** Shared by global banners and screen-local outcomes. The host persists before hiding. */
export function Notice(props: { noticeKey: string; children: ComponentChildren; class?: string; role?: 'status' | 'alert'; global?: boolean }) {
    const context = useContext(NoticeContext);
    const key = props.global ? props.noticeKey : noticeKey('project', context.project, props.noticeKey);
    const [hidden, setHidden] = useState<string>();
    const [saving, setSaving] = useState(false), [error, setError] = useState<{ key: string; message: string }>();
    if (context.dismissed.includes(key) || hidden === key) { return null; }
    const dismiss = async () => {
        setSaving(true); setError(undefined);
        try { await request('notice.dismiss', { key }); setHidden(key); }
        catch (failure) { setError({ key, message: `Could not save dismissal: ${failure instanceof Error ? failure.message : String(failure)}` }); }
        finally { setSaving(false); }
    };
    return <aside class={`dismissible-notice ${props.class ?? 'callout'}`} role={props.role ?? 'status'}>
        <button type="button" class="ghost notice-dismiss" aria-label="Dismiss notice" title="Dismiss this notice permanently" disabled={saving} onClick={() => void dismiss()}>×</button>
        <div class="notice-body">{props.children}</div>
        {error?.key === key && <p role="alert">{error.message}</p>}
    </aside>;
}
