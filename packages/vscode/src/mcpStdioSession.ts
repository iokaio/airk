// SPDX-License-Identifier: Apache-2.0
import { ChildProcessWithoutNullStreams, spawn } from 'node:child_process';
import { requireNativeExecution } from './nativeGate';

export class McpSessionError extends Error {
    constructor(readonly code: 'exited' | 'invalid_json' | 'oversize' | 'timeout' | 'closed' | 'rpc' | 'overloaded', message: string, readonly detail?: unknown) {
        super(message);
    }
}
export interface McpStdioOptions {
    command: string;
    args: string[];
    cwd: string;
    env?: NodeJS.ProcessEnv;
    /** Reported as MCP clientInfo.name; the native worker derives usage attribution from it. */
    clientName: string;
    clientVersion: string;
    /** Largest single protocol message accepted from the server. */
    maxMessageBytes?: number;
    launch?: (command: string, args: string[], options: { cwd: string; env: NodeJS.ProcessEnv }) => ChildProcessWithoutNullStreams;
}
interface Pending { resolve(value: any): void; reject(error: Error): void; timer?: NodeJS.Timeout }

/**
 * One bounded newline-delimited JSON-RPC stdio connection to an MCP server.
 * Replies are matched by monotonically increasing ids, so a reply that arrives
 * after its request timed out is discarded rather than delivered to another call.
 */
export class McpStdioSession {
    private readonly pending = new Map<number, Pending>();
    private readonly closeListeners: ((error: McpSessionError) => void)[] = [];
    private nextId = 1;
    private buffer = '';
    private stderr = '';
    private failure?: McpSessionError;

    private constructor(private readonly child: ChildProcessWithoutNullStreams, private readonly maxMessageBytes: number) {
        child.on('error', error => this.fail(new McpSessionError('exited', `AIRK MCP failed to start: ${error.message}`)));
        child.on('exit', code => this.fail(new McpSessionError('exited', `AIRK MCP exited (${code}): ${this.stderr}`)));
        child.stdin.on('error', error => this.fail(new McpSessionError('exited', `AIRK MCP input closed: ${error.message}`)));
        child.stdout.setEncoding('utf8');
        child.stderr.on('data', bytes => { this.stderr = (this.stderr + bytes.toString()).slice(-4096); });
        child.stdout.on('data', (text: string) => this.read(text));
    }

    /** Spawn, initialize and acknowledge; rejects (and kills the child) if the handshake fails. */
    static async open(options: McpStdioOptions, handshakeTimeout = 90000): Promise<{ session: McpStdioSession; initialize: any }> {
        requireNativeExecution();
        const env = { ...process.env, ...options.env };
        const child = (options.launch ?? ((command, args, launch) => spawn(command, args, { ...launch, windowsHide: true, stdio: 'pipe' })))(
            options.command, options.args, { cwd: options.cwd, env });
        const session = new McpStdioSession(child, options.maxMessageBytes ?? 1024 * 1024);
        try {
            const initialize = await session.request('initialize', { protocolVersion: '2025-11-25', capabilities: {},
                clientInfo: { name: options.clientName, version: options.clientVersion } }, handshakeTimeout);
            session.notify('notifications/initialized');
            return { session, initialize };
        } catch (error) { session.close(); throw error; }
    }

    get closed(): boolean { return this.failure !== undefined; }
    get closeReason(): McpSessionError | undefined { return this.failure; }
    onClose(listener: (error: McpSessionError) => void): void {
        if (this.failure) { listener(this.failure); } else { this.closeListeners.push(listener); }
    }

    request(method: string, params: unknown, timeout = 120000): Promise<any> {
        if (this.failure) { return Promise.reject(this.failure); }
        if (this.pending.size >= 64 || this.child.stdin.writableLength > 1024 * 1024) { return Promise.reject(new McpSessionError('overloaded', 'AIRK MCP request queue is full.')); }
        const id = this.nextId++;
        return new Promise((resolve, reject) => {
            const pending: Pending = { resolve, reject };
            if (timeout > 0) {
                pending.timer = setTimeout(() => {
                    if (this.pending.delete(id)) {
                        // The server may still finish the work; its late reply is discarded.
                        this.notify('notifications/cancelled', { requestId: id, reason: 'client deadline' });
                        reject(new McpSessionError('timeout', `AIRK MCP request ${method} exceeded ${timeout} ms.${this.stderr ? `\nLauncher/server stderr: ${this.stderr}` : '\nNo launcher/server error output was received.'}`));
                    }
                }, timeout);
            }
            this.pending.set(id, pending);
            this.write({ jsonrpc: '2.0', id, method, params });
        });
    }

    notify(method: string, params?: unknown): void {
        if (!this.failure) { this.write(params === undefined ? { jsonrpc: '2.0', method } : { jsonrpc: '2.0', method, params }); }
    }

    close(): void {
        this.fail(new McpSessionError('closed', 'AIRK MCP session closed.'));
    }

    private write(message: unknown): void {
        this.child.stdin.write(JSON.stringify(message) + '\n');
    }

    private read(text: string): void {
        this.buffer += text;
        let end: number;
        while ((end = this.buffer.indexOf('\n')) >= 0) {
            const line = this.buffer.slice(0, end); this.buffer = this.buffer.slice(end + 1);
            if (Buffer.byteLength(line) > this.maxMessageBytes) { this.fail(new McpSessionError('oversize', 'AIRK MCP response exceeded budget.')); return; }
            if (!line.trim()) { continue; }
            let message: any;
            try { message = JSON.parse(line); } catch { this.fail(new McpSessionError('invalid_json', 'AIRK MCP emitted invalid JSON.')); return; }
            const request = typeof message?.id === 'number' ? this.pending.get(message.id) : undefined;
            if (!request) { continue; }
            this.pending.delete(message.id); clearTimeout(request.timer);
            if (message.error) { request.reject(new McpSessionError('rpc', JSON.stringify(message.error), message.error)); }
            else { request.resolve(message.result); }
        }
        if (Buffer.byteLength(this.buffer) > this.maxMessageBytes) { this.fail(new McpSessionError('oversize', 'AIRK MCP response exceeded budget.')); }
    }

    private fail(error: McpSessionError): void {
        if (this.failure) { return; }
        this.failure = error;
        for (const request of this.pending.values()) { clearTimeout(request.timer); request.reject(error); }
        this.pending.clear();
        this.buffer = '';
        this.child.stdin.end();
        if (this.child.exitCode === null && this.child.signalCode === null) { this.child.kill(); }
        for (const listener of this.closeListeners.splice(0)) { listener(error); }
    }
}
