// SPDX-License-Identifier: Apache-2.0
// Per-window rendezvous for the Explorer Assistance server. A trusted window listens on
// a private endpoint and publishes a user-only descriptor with a rotating credential;
// the server reads it at connect time. Requests are typed; nothing here runs commands.
import { createHash, randomBytes, timingSafeEqual } from 'node:crypto';
import * as fs from 'node:fs';
import * as net from 'node:net';
import * as os from 'node:os';
import * as path from 'node:path';
import { ASSISTANCE_IPC_LIMIT, type AssistanceIpcReply, parseWindowDescriptor, WINDOW_DESCRIPTOR_LIMIT, WINDOW_SCHEMA, type WindowDescriptor } from './explorerContracts';

export const DESCRIPTOR_TTL_MS = 30 * 60_000;
export const ROTATE_MS = 10 * 60_000;
const REQUEST_TIMEOUT_MS = 5000;

export interface AssistanceHostPort {
    /** <globalStorage>/explorer-assistance */
    readonly directory: string;
    readonly version: string;
    /** Workspace folder paths of this window; canonicalized before they are published. */
    readonly roots: readonly string[];
    explorerOpen(): boolean;
}

export const windowsDirectory = (directory: string) => path.join(directory, 'windows');

/** Write a user-only file atomically: temporary sibling, then rename. */
export function writePrivate(file: string, content: string): void {
    fs.mkdirSync(path.dirname(file), { recursive: true, mode: 0o700 });
    if (process.platform !== 'win32') { fs.chmodSync(path.dirname(file), 0o700); }
    const temporary = `${file}.${randomBytes(8).toString('hex')}.tmp`;
    try {
        fs.writeFileSync(temporary, content, { flag: 'wx', mode: 0o600 });
        fs.renameSync(temporary, file);
    } finally { fs.rmSync(temporary, { force: true }); }
}

/** Remove expired or unreadable window descriptors left by windows that did not close cleanly. */
export function sweepExpired(directory: string, now = Date.now()): void {
    let names: string[];
    try { names = fs.readdirSync(windowsDirectory(directory)); } catch { return; }
    for (const name of names.filter(entry => /^[0-9a-f]{32}\.json$/.test(entry))) {
        const file = path.join(windowsDirectory(directory), name);
        try {
            if (fs.statSync(file).size > WINDOW_DESCRIPTOR_LIMIT) { fs.rmSync(file, { force: true }); continue; }
            const descriptor = parseWindowDescriptor(JSON.parse(fs.readFileSync(file, 'utf8')));
            if (!descriptor || descriptor.expires_unix_ms < now) { fs.rmSync(file, { force: true }); }
        } catch { fs.rmSync(file, { force: true }); }
    }
}

export class AssistanceIpcHost {
    private readonly session = randomBytes(16).toString('hex');
    private credential = randomBytes(32).toString('hex');
    private readonly server: net.Server;
    private readonly timer: NodeJS.Timeout;
    private readonly fingerprint: string;
    private disposed = false;

    private constructor(private readonly port: AssistanceHostPort, private readonly roots: readonly string[], private readonly endpoint: string, private readonly socketDirectory?: string) {
        this.fingerprint = createHash('sha256').update([this.session, ...roots].join('\0')).digest('hex').slice(0, 16);
        this.server = net.createServer(socket => this.accept(socket));
        this.timer = setInterval(() => this.rotate(), ROTATE_MS);
        this.timer.unref();
    }

    /** Listen and publish this window's descriptor; undefined when the window has no usable folder. */
    static async start(port: AssistanceHostPort): Promise<AssistanceIpcHost | undefined> {
        const roots = [...new Set(port.roots.flatMap(root => { try { return [fs.realpathSync.native(root)]; } catch { return []; } }))].slice(0, 32);
        if (!roots.length) { return undefined; }
        sweepExpired(port.directory);
        let endpoint: string, socketDirectory: string | undefined;
        if (process.platform === 'win32') {
            endpoint = `\\\\.\\pipe\\airk-explorer-${randomBytes(16).toString('hex')}`;
        } else {
            // A short private directory keeps the socket path under the platform length limit.
            socketDirectory = fs.mkdtempSync(path.join(os.tmpdir(), 'airk-explorer-'));
            fs.chmodSync(socketDirectory, 0o700);
            endpoint = path.join(socketDirectory, 'airk-explorer.sock');
        }
        const host = new AssistanceIpcHost(port, roots, endpoint, socketDirectory);
        try {
            await new Promise<void>((resolve, reject) => { host.server.once('error', reject); host.server.listen(endpoint, () => { host.server.off('error', reject); resolve(); }); });
            if (socketDirectory) { fs.chmodSync(endpoint, 0o600); }
            host.publish();
            return host;
        } catch (error) { host.dispose(); throw error; }
    }

    private get file(): string { return path.join(windowsDirectory(this.port.directory), `${this.session}.json`); }

    private publish(): void {
        const descriptor: WindowDescriptor = { schema: WINDOW_SCHEMA, session_id: this.session, endpoint: this.endpoint, credential: this.credential,
            roots: this.roots, binding_fingerprint: this.fingerprint, extension_version: this.port.version, expires_unix_ms: Date.now() + DESCRIPTOR_TTL_MS };
        writePrivate(this.file, JSON.stringify(descriptor));
    }

    /** New credential and expiry; a server holding the old credential must reread the descriptor. */
    rotate(): void {
        if (this.disposed) { return; }
        this.credential = randomBytes(32).toString('hex');
        try { this.publish(); } catch { /* the next rotation retries; requests with the old credential fail closed */ }
    }

    private accept(socket: net.Socket): void {
        let buffer = '';
        const finish = (reply?: AssistanceIpcReply) => { if (reply) { socket.end(JSON.stringify(reply) + '\n'); } else { socket.destroy(); } };
        socket.setEncoding('utf8');
        socket.setTimeout(REQUEST_TIMEOUT_MS, () => finish());
        socket.on('error', () => socket.destroy());
        socket.on('data', (chunk: string) => {
            buffer += chunk;
            if (Buffer.byteLength(buffer, 'utf8') > ASSISTANCE_IPC_LIMIT) { finish(); return; }
            const newline = buffer.indexOf('\n');
            if (newline >= 0) { socket.removeAllListeners('data'); finish(this.answer(buffer.slice(0, newline))); }
        });
    }

    private answer(line: string): AssistanceIpcReply | undefined {
        let request: Record<string, unknown>;
        try { request = JSON.parse(line); } catch { return undefined; }
        if (!request || typeof request !== 'object' || typeof request.credential !== 'string') { return undefined; }
        const expected = Buffer.from(this.credential, 'hex'), offered = Buffer.from(request.credential, 'hex');
        // A wrong credential gets no reply at all: the endpoint does not confirm that it is an Explorer window.
        if (offered.length !== expected.length || !timingSafeEqual(offered, expected) || this.disposed) { return undefined; }
        if (request.v !== 1) { return { v: 1, ok: false, code: 'SCHEMA_UNSUPPORTED' }; }
        if (request.type !== 'status' || Object.keys(request).some(key => !['v', 'credential', 'type'].includes(key))) { return { v: 1, ok: false, code: 'INVALID_SCHEMA' }; }
        return { v: 1, ok: true, status: { binding_fingerprint: this.fingerprint, explorer_open: this.port.explorerOpen() } };
    }

    /** Stop listening and withdraw the descriptor synchronously, so it is gone before the host exits. */
    dispose(): void {
        if (this.disposed) { return; }
        this.disposed = true;
        clearInterval(this.timer);
        this.server.close();
        fs.rmSync(this.file, { force: true });
        if (this.socketDirectory) { fs.rmSync(this.socketDirectory, { recursive: true, force: true }); }
    }
}
