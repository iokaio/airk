// SPDX-License-Identifier: Apache-2.0
import { ExplorerError } from '../explorerErrors';
import { ExplorerViewState, HostMessage, MESSAGE_VERSION, WebviewMessageOf, WebviewMessageType } from '../explorerContracts';

const host = acquireVsCodeApi();
let counter = 0;
const pending = new Map<string, { resolve(value: unknown): void; reject(error: ExplorerError): void }>();
const listeners = new Set<(message: HostMessage) => void>();

export class HostFailure extends Error {
    constructor(readonly error: ExplorerError) { super(error.message); }
}

type Fields<T extends WebviewMessageType> = Omit<WebviewMessageOf<T>, 'v' | 'type' | 'requestId'>;

/** Send one typed request; the host validates it again and answers with a result message. */
export function request<T = unknown, K extends WebviewMessageType = WebviewMessageType>(type: K, fields: Fields<K>): Promise<T> {
    const requestId = `w${++counter}`;
    return new Promise<T>((resolve, reject) => {
        pending.set(requestId, { resolve: value => resolve(value as T), reject: error => reject(new HostFailure(error)) });
        host.postMessage({ ...fields, v: MESSAGE_VERSION, type, requestId });
    });
}

/** Fire-and-forget notification (the host still replies; the reply is ignored). */
export function notify<K extends WebviewMessageType>(type: K, fields: Fields<K>): void {
    void request(type, fields).catch(() => undefined);
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
        if (message.ok) { waiter.resolve(message.data); } else { waiter.reject(message.error); }
        return;
    }
    for (const listener of listeners) { listener(message); }
});

export function ready(): void { host.postMessage({ v: MESSAGE_VERSION, type: 'ready', requestId: 'ready' }); }

export type { ExplorerViewState };
