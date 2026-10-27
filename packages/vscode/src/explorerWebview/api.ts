// SPDX-License-Identifier: Apache-2.0
import { ExplorerError, EXPLORER_ERRORS, friendlyError } from '../explorerErrors';
import { ExplorerViewState, HostMessage, MESSAGE_VERSION, WebviewMessageOf, WebviewMessageType } from '../explorerContracts';

const host = acquireVsCodeApi();
let counter = 0;
const pending = new Map<string, { resolve(value: unknown): void; reject(error: ExplorerError): void; timer: ReturnType<typeof setTimeout>; uncertain: boolean }>();
const listeners = new Set<(message: HostMessage) => void>();

export class HostFailure extends Error {
    constructor(readonly error: ExplorerError) { super(friendlyError(error)); }
}

type Fields<T extends WebviewMessageType> = Omit<WebviewMessageOf<T>, 'v' | 'type' | 'requestId'>;

/** Send one typed request; the host validates it again and answers with a result message. */
export function request<T = unknown, K extends WebviewMessageType = WebviewMessageType>(type: K, fields: Fields<K>): Promise<T> {
    const requestId = `w${++counter}`;
    if (pending.size >= 64) { return Promise.reject(new HostFailure({ code: 'RATE_LIMITED', message: 'Explorer already has 64 requests waiting.' })); }
    const uncertain = /^(manage\.(action|command)|planner\.(saveInWorkspace|exportJSON)|strategy\.export|studio\.save|usage\.action|suggestions\.(resolve|applied|discardAll)|handoff\.share|recipes\.(save|delete|export))$/.test(type);
    const interactive = uncertain || type === 'planner.import' || type === 'strategy.import' || type === 'recipes.import' || type === 'source.open' || type === 'source.choose' || type === 'help.choose';
    return new Promise<T>((resolve, reject) => {
        const fail = (code: 'REQUEST_TIMEOUT' | 'REQUEST_OUTCOME_UNKNOWN' | 'HOST_UNAVAILABLE') => {
            const waiter = pending.get(requestId); if (!waiter) { return; }
            pending.delete(requestId); clearTimeout(waiter.timer); waiter.reject({ code, message: EXPLORER_ERRORS[code].message });
        };
        const timer = setTimeout(() => fail(uncertain ? 'REQUEST_OUTCOME_UNKNOWN' : 'REQUEST_TIMEOUT'), interactive ? 900000 : 180000);
        pending.set(requestId, { resolve: value => resolve(value as T), reject: error => reject(new HostFailure(error)), timer, uncertain });
        try { host.postMessage({ ...fields, v: MESSAGE_VERSION, type, requestId }); }
        catch { fail('HOST_UNAVAILABLE'); }
    });
}

/** Fire-and-forget notification (the host still replies; the reply is ignored). */
export function notify<K extends WebviewMessageType>(type: K, fields: Fields<K>): void {
    try { host.postMessage({ ...fields, v: MESSAGE_VERSION, type, requestId: `n${++counter}` }); } catch { /* A closed webview cannot report revisions; it will report current state when reopened. */ }
}

export function onHost(listener: (message: HostMessage) => void): () => void {
    listeners.add(listener);
    return () => listeners.delete(listener);
}

export function savedState<T>(): T | undefined { return host.getState() as T | undefined; }
export function saveState(state: unknown): void { host.setState(state); }

window.addEventListener('message', event => {
    const message = event.data as HostMessage;
    if (!message || typeof message !== 'object' || message.v !== MESSAGE_VERSION) { return; }
    if (message.type === 'result') {
        const waiter = pending.get(message.requestId);
        if (!waiter) { return; }
        pending.delete(message.requestId);
        clearTimeout(waiter.timer);
        if (message.ok) { waiter.resolve(message.data); } else { waiter.reject(message.error); }
        return;
    }
    for (const listener of listeners) { listener(message); }
});

window.addEventListener('pagehide', () => {
    for (const waiter of pending.values()) {
        clearTimeout(waiter.timer);
        const code = waiter.uncertain ? 'REQUEST_OUTCOME_UNKNOWN' : 'HOST_UNAVAILABLE';
        waiter.reject({ code, message: EXPLORER_ERRORS[code].message });
    }
    pending.clear();
});

export function ready(): void { host.postMessage({ v: MESSAGE_VERSION, type: 'ready', requestId: 'ready' }); }

export type { ExplorerViewState };
