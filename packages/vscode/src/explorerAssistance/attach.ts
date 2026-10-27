// SPDX-License-Identifier: Apache-2.0
// Finds the trusted Explorer window that owns the agent's working directory and sends it one
// typed request over its private endpoint. Help never depends on this.
import { promises as fs } from 'node:fs';
import * as net from 'node:net';
import * as path from 'node:path';
import { ASSISTANCE_IPC_LIMIT, type AssistanceIpcReply, type AssistanceIpcRequest, parseWindowDescriptor, WINDOW_DESCRIPTOR_LIMIT, type WindowDescriptor } from '../explorerContracts';
import type { ExplorerErrorCode } from '../explorerErrors';

const REPLY_TIMEOUT_MS = 3000;
const MAX_DESCRIPTORS = 64;

export function contains(root: string, target: string, platform: NodeJS.Platform = process.platform): boolean {
    const paths = platform === 'win32' ? path.win32 : path.posix;
    const fold = (value: string) => platform === 'win32' ? value.toLowerCase() : value;
    const relative = paths.relative(fold(root), fold(target));
    return relative === '' || (relative !== '..' && !relative.startsWith('..' + paths.sep) && !paths.isAbsolute(relative));
}

async function descriptors(directory: string, now: number): Promise<WindowDescriptor[]> {
    const windows = path.join(directory, 'windows');
    let names: string[];
    try { names = (await fs.readdir(windows)).filter(name => /^[0-9a-f]{32}\.json$/.test(name)).sort().slice(0, MAX_DESCRIPTORS); } catch { return []; }
    const found: WindowDescriptor[] = [];
    for (const name of names) {
        try {
            const file = path.join(windows, name);
            if ((await fs.stat(file)).size > WINDOW_DESCRIPTOR_LIMIT) { continue; }
            const descriptor = parseWindowDescriptor(JSON.parse(await fs.readFile(file, 'utf8')));
            if (descriptor && descriptor.expires_unix_ms > now && `${descriptor.session_id}.json` === name) { found.push(descriptor); }
        } catch { /* a descriptor being replaced or removed is skipped */ }
    }
    return found;
}

type Body = AssistanceIpcRequest extends infer R ? R extends unknown ? Omit<R, 'v' | 'credential'> : never : never;

function send(descriptor: WindowDescriptor, body: Body): Promise<AssistanceIpcReply | undefined> {
    return new Promise(resolve => {
        let buffer = '';
        const socket = net.createConnection(descriptor.endpoint);
        const done = (reply?: AssistanceIpcReply) => { socket.destroy(); resolve(reply); };
        socket.setEncoding('utf8');
        socket.setTimeout(REPLY_TIMEOUT_MS, () => done());
        socket.on('error', () => done());
        socket.on('connect', () => socket.write(JSON.stringify({ ...body, v: 1, credential: descriptor.credential }) + '\n'));
        socket.on('data', (chunk: string) => {
            buffer += chunk;
            if (buffer.length > ASSISTANCE_IPC_LIMIT) { done(); return; }
            const newline = buffer.indexOf('\n');
            if (newline >= 0) { try { done(JSON.parse(buffer.slice(0, newline))); } catch { done(); } }
        });
        socket.on('end', () => done());
    });
}

export type WindowReply = { ok: true; reply: Extract<AssistanceIpcReply, { ok: true }> } | { ok: false; code: ExplorerErrorCode; message?: string };
export type WindowChannel = (body: Body) => Promise<WindowReply>;

/**
 * A channel to the one window whose folders contain `cwd`. Descriptors are read at call time;
 * a request that gets no reply is retried once with a fresh descriptor, because the window
 * rotates its credential. No directory (for example a server started by hand) means no window.
 */
export function windowChannel(directory: string | undefined, cwd: string): WindowChannel {
    return async body => {
        if (!directory) { return { ok: false, code: 'UI_NOT_ATTACHED' }; }
        for (let attempt = 0; attempt < 2; attempt++) {
            const matches = (await descriptors(directory, Date.now())).filter(descriptor => descriptor.roots.some(root => contains(root, cwd)));
            if (!matches.length) { return { ok: false, code: 'UI_NOT_ATTACHED' }; }
            if (matches.length > 1) { return { ok: false, code: 'AMBIGUOUS_TARGET' }; }
            const reply = await send(matches[0], body);
            if (reply?.ok) { return { ok: true, reply }; }
            if (reply) { return { ok: false, code: reply.code, ...(reply.message ? { message: reply.message } : {}) }; }
        }
        return { ok: false, code: 'HOST_UNAVAILABLE' };
    };
}
