// SPDX-License-Identifier: Apache-2.0
// Per-window rendezvous for the Explorer Assistance server. A trusted window listens on
// a private endpoint and publishes a user-only descriptor with a rotating credential;
// the server reads it at connect time. Requests are typed; nothing here runs commands.
import { createHash, randomBytes, timingSafeEqual } from 'node:crypto';
import * as fs from 'node:fs';
import * as net from 'node:net';
import * as os from 'node:os';
import * as path from 'node:path';
import { ASSISTANCE_IPC_LIMIT, type AssistanceIpcReply, type AssistanceWindowStatus, type ExplorerHandoff, parseWindowDescriptor, WINDOW_DESCRIPTOR_LIMIT, WINDOW_SCHEMA, type WindowDescriptor } from './explorerContracts';
import { DRAFT_KINDS, type DraftKind } from './explorerDrafts';
import type { DraftClient, Submission, SuggestionStore } from './explorerSuggestions';
import { restrictWindowsDirectory } from './privateDirectory';

export const DESCRIPTOR_TTL_MS = 30 * 60_000;
export const ROTATE_MS = 10 * 60_000;
const REQUEST_TIMEOUT_MS = 5000;

export interface AssistanceHostPort {
    /** <globalStorage>/explorer-assistance */
    readonly directory: string;
    readonly version: string;
    /** Workspace folder paths of this window; canonicalized before they are published. */
    readonly roots: readonly string[];
    authorized?(): boolean;
    explorerOpen(): boolean;
    uiStatus?(): Pick<AssistanceWindowStatus, 'active_route' | 'binding_generation' | 'blockers' | 'selected_project'>;
    validate?(kind: DraftKind, draft: unknown): Record<string, unknown>;
    handoff?(token: string): ExplorerHandoff | undefined;
    targetRevisions?(): { readonly studio?: number; readonly planner?: number };
    /** Admit a draft to this window's Suggestions Inbox. Absent when drafts are not offered. */
    suggest?(kind: DraftKind, draft: unknown, client: DraftClient): Submission;
    proposalStatus?(id: string): ReturnType<SuggestionStore['status']>;
}

export const windowsDirectory = (directory: string) => path.join(directory, 'windows');

/** Write a user-only file atomically: temporary sibling, then rename. */
export function writePrivate(file: string, content: string): void {
    fs.mkdirSync(path.dirname(file), { recursive: true, mode: 0o700 });
    if (process.platform !== 'win32') { fs.chmodSync(path.dirname(file), 0o700); }
    else { restrictWindowsDirectory(path.dirname(file)); }
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
            if (!descriptor || descriptor.expires_unix_ms < now) {
                fs.rmSync(file, { force: true });
                // A crashed window's private socket directory goes with its descriptor.
                const socketDirectory = descriptor && !descriptor.endpoint.startsWith('\\\\') ? path.dirname(descriptor.endpoint) : undefined;
                if (socketDirectory && path.dirname(socketDirectory) === os.tmpdir() && path.basename(socketDirectory).startsWith('airk-explorer-')) {
                    fs.rmSync(socketDirectory, { recursive: true, force: true });
                }
            }
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
        const previous = this.credential;
        this.credential = randomBytes(32).toString('hex');
        // Keep answering the published credential if the new descriptor could not be written.
        try { this.publish(); } catch { this.credential = previous; }
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
        if (this.port.authorized?.() === false) { return { v: 1, ok: false, code: 'WORKSPACE_UNTRUSTED' }; }
        if (request.project !== undefined && (typeof request.project !== 'string' || !/^[a-f0-9]{64}$/.test(request.project))) {
            return { v: 1, ok: false, code: 'INVALID_SCHEMA' };
        }
        // Enforce the agent's configured project at admission, not merely in an earlier status read.
        if (request.project !== undefined && request.type !== 'status' && request.project !== this.port.uiStatus?.().selected_project) {
            return { v: 1, ok: false, code: 'STALE_BINDING', message: 'Select the agent\'s configured project in Explorer before applying a UI request.' };
        }
        const fields = (...allowed: string[]) => Object.keys(request).every(key => ['v', 'credential', 'type', 'project', ...allowed].includes(key));
        switch (request.type) {
            case 'status':
                if (!fields()) { break; }
                return { v: 1, ok: true, status: { binding_fingerprint: this.bindingFingerprint, explorer_open: this.port.explorerOpen(), target_revisions: this.port.targetRevisions?.() ?? {}, ...this.port.uiStatus?.() } };
            case 'validate':
                if (!fields('kind', 'draft') || !DRAFT_KINDS.includes(request.kind as DraftKind)) { break; }
                return this.port.validate ? { v: 1, ok: true, validation: this.port.validate(request.kind as DraftKind, request.draft) } : { v: 1, ok: false, code: 'CAPABILITY_DISABLED' };
            case 'handoff_get': {
                if (!fields('token') || typeof request.token !== 'string' || !/^ex-share-[0-9a-f]{64}$/.test(request.token)) { break; }
                const handoff = this.port.handoff?.(request.token);
                return handoff ? { v: 1, ok: true, handoff } : { v: 1, ok: false, code: 'PROPOSAL_EXPIRED', message: 'Handoff is unknown, expired or its draft/binding changed; ask the user to share again.' };
            }
            case 'suggest': {
                const client = request.client as Record<string, unknown> | undefined;
                if (!fields('kind', 'draft', 'client') || !DRAFT_KINDS.includes(request.kind as DraftKind) || !client || typeof client.name !== 'string' || typeof client.version !== 'string') { break; }
                if (!this.port.suggest) { return { v: 1, ok: false, code: 'CAPABILITY_DISABLED' }; }
                // The host validates the draft again; the server's validation is only a fast first check.
                const result = this.port.suggest(request.kind as DraftKind, request.draft, { name: client.name, version: client.version });
                return result.ok ? { v: 1, ok: true, receipt: result.receipt } : { v: 1, ok: false, code: result.code, message: result.message.slice(0, 2000) };
            }
            case 'proposal_status': {
                if (!fields('proposal_id') || typeof request.proposal_id !== 'string' || !/^ex-prop-[0-9a-f]{16}$/.test(request.proposal_id)) { break; }
                const proposal = this.port.proposalStatus?.(request.proposal_id);
                return proposal ? { v: 1, ok: true, proposal } : { v: 1, ok: false, code: 'PROPOSAL_EXPIRED', message: 'No proposal with that id is held by this window.' };
            }
        }
        return { v: 1, ok: false, code: 'INVALID_SCHEMA' };
    }

    get bindingFingerprint(): string {
        const generation = this.port.uiStatus?.().binding_generation;
        return generation === undefined ? this.fingerprint : createHash('sha256').update(`${this.fingerprint}:${generation}`).digest('hex').slice(0, 16);
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
