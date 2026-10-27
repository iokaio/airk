// SPDX-License-Identifier: Apache-2.0
// Finds the trusted Explorer window that owns the agent's working directory and asks it
// for status over its private endpoint. Help never depends on this.
import { promises as fs } from 'node:fs';
import * as net from 'node:net';
import * as path from 'node:path';
import { ASSISTANCE_IPC_LIMIT, type AssistanceIpcReply, parseWindowDescriptor, WINDOW_DESCRIPTOR_LIMIT, type WindowDescriptor } from '../explorerContracts';
import type { Attachment, AttachmentProbe } from './tools';

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

function request(descriptor: WindowDescriptor): Promise<AssistanceIpcReply | undefined> {
    return new Promise(resolve => {
        let buffer = '';
        const socket = net.createConnection(descriptor.endpoint);
        const done = (reply?: AssistanceIpcReply) => { socket.destroy(); resolve(reply); };
        socket.setEncoding('utf8');
        socket.setTimeout(REPLY_TIMEOUT_MS, () => done());
        socket.on('error', () => done());
        socket.on('connect', () => socket.write(JSON.stringify({ v: 1, credential: descriptor.credential, type: 'status' }) + '\n'));
        socket.on('data', (chunk: string) => {
            buffer += chunk;
            if (buffer.length > ASSISTANCE_IPC_LIMIT) { done(); return; }
            const newline = buffer.indexOf('\n');
            if (newline >= 0) { try { done(JSON.parse(buffer.slice(0, newline))); } catch { done(); } }
        });
        socket.on('end', () => done());
    });
}

/** No directory (for example a server started by hand) means no window can be attached. */
export function attachmentProbe(directory: string | undefined, cwd: string): AttachmentProbe {
    return async (): Promise<Attachment> => {
        if (!directory) { return { attached: false, code: 'UI_NOT_ATTACHED' }; }
        const matches = (await descriptors(directory, Date.now())).filter(descriptor => descriptor.roots.some(root => contains(root, cwd)));
        if (!matches.length) { return { attached: false, code: 'UI_NOT_ATTACHED' }; }
        if (matches.length > 1) { return { attached: false, code: 'AMBIGUOUS_TARGET' }; }
        const reply = await request(matches[0]);
        if (!reply?.ok) { return { attached: false, code: reply ? 'STALE_BINDING' : 'HOST_UNAVAILABLE' }; }
        return { attached: true, bindingFingerprint: reply.status.binding_fingerprint, explorerOpen: reply.status.explorer_open };
    };
}
