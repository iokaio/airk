// SPDX-License-Identifier: Apache-2.0
import * as path from 'node:path';
import { explorerError, ExplorerError } from './explorerErrors';
import { McpSessionError, McpStdioOptions, McpStdioSession } from './mcpStdioSession';
import { LaunchSpec, launchArgs } from './model';

/** The 16 read-only native tools. The Explorer calls nothing else on AIRK Retrieval. */
export const NATIVE_TOOLS = ['airk_search', 'airk_read_file', 'airk_list_files', 'airk_status', 'airk_help', 'airk_file_info', 'airk_outline',
    'airk_symbols', 'airk_imports', 'airk_document_facts', 'airk_references', 'airk_context', 'airk_capabilities', 'airk_code_facts',
    'airk_relationships', 'airk_changes'] as const;
export type NativeTool = typeof NATIVE_TOOLS[number];

/** Usage attribution label recorded by the native worker (ClientFamily::Explorer). */
export const EXPLORER_CLIENT_NAME = 'airk-explorer';

export class EvidenceFailure extends Error {
    constructor(readonly error: ExplorerError) { super(error.message); }
}
export interface ToolResult { readonly payload: Record<string, unknown>; readonly publication?: string }
/** One report must never silently combine different publications. Missing identity fails closed. */
export function samePublication(expected: string | undefined, result: ToolResult): string {
    if (!result.publication || expected && result.publication !== expected) { throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH', 'The index changed during this report; refresh it.')); }
    return result.publication;
}
export function absentFromIndex(payload: Record<string, unknown>): boolean {
    const read = payload.read as Record<string, unknown> | undefined;
    return read?.status === 'removed_from_index' || read?.status === 'unknown' && read?.reason === 'path is absent from the current index';
}
export interface CallOptions {
    /** UI deadline. The native call may still finish; its late reply is discarded. */
    readonly timeout?: number;
    /** Calls sharing a key supersede older ones: only the newest result is delivered. */
    readonly supersede?: string;
}

/**
 * Accept only the native "safe to repeat this exact read" replies (publication or
 * selection busy, or an unsubmitted busy admission). Anything else is a real result.
 */
export function readRetryDelay(envelope: unknown): number | undefined {
    if (!envelope || typeof envelope !== 'object') { return; }
    const value = envelope as Record<string, unknown>, content = value.content;
    if (value.isError !== true || !Array.isArray(content) || content.length !== 1 || content[0]?.type !== 'text' || typeof content[0]?.text !== 'string') { return; }
    let hint: Record<string, any>;
    try { hint = JSON.parse(content[0].text); } catch { return; }
    if (!hint || typeof hint !== 'object') { return; }
    let delay: unknown;
    if (hint.status === 'retry') {
        if (hint.retryable !== true || hint.action !== 'repeat_same_request' || !['publication_busy', 'selection_busy'].includes(hint.code)) { return; }
        delay = hint.retry_after_ms;
    } else {
        const error = hint.error;
        if (!error || !['direct_query_admission', 'direct_query_setup'].includes(error.code) || error.reason !== 'busy'
            || error.query_submitted !== false || error.retryable !== true || error.fresh_brokerage_required !== (error.code === 'direct_query_setup')) { return; }
        delay = error.retry_after_ms;
    }
    return Number.isInteger(delay) && Number(delay) >= 1 && Number(delay) <= 1000 ? Number(delay) : undefined;
}

function publicationOf(payload: Record<string, unknown>): string | undefined {
    const read = payload.read as Record<string, unknown> | undefined;
    const publication = payload.publication ?? read?.current_publication ?? read?.publication;
    if (typeof publication === 'string') { return publication; }
    if (publication && typeof publication === 'object' && typeof (publication as Record<string, unknown>).token === 'string') {
        return (publication as Record<string, unknown>).token as string;
    }
    return undefined;
}

function toolFailure(text: string): EvidenceFailure {
    if (/current revision expired|known_hash does not match/.test(text)) { return new EvidenceFailure(explorerError('EVIDENCE_MISMATCH')); }
    if (/workspace is not indexed|index_unavailable|no readable publication/i.test(text)) { return new EvidenceFailure(explorerError('INDEX_UNAVAILABLE')); }
    let detail = text;
    try {
        const parsed = JSON.parse(text);
        const error = parsed?.error ?? parsed;
        if (error && typeof error === 'object' && typeof error.message === 'string') {
            detail = error.message;
            if (typeof error.code === 'string') { return new EvidenceFailure(explorerError(error.code, detail)); }
        }
    } catch { /* Plain-text tool errors are reported as written. */ }
    return new EvidenceFailure(explorerError('TOOL_ERROR', detail));
}

interface Task { tool: NativeTool; args: Record<string, unknown>; options: CallOptions; generation?: number; resolve(value: ToolResult): void; reject(error: Error): void }
type Opener = (options: McpStdioOptions, handshakeTimeout: number) => Promise<{ session: McpStdioSession }>;

/**
 * The Explorer's evidence session: one persistent stdio MCP connection to the bound
 * project through the same verified launcher agents use. The native bridge accepts
 * at most 4 in-flight calls and serializes them per connection, so this client keeps
 * its own queue with at most 3 calls outstanding.
 */
export class EvidenceClient {
    private session?: McpStdioSession;
    private opening?: Promise<McpStdioSession>;
    private readonly queue: Task[] = [];
    private readonly generations = new Map<string, number>();
    private active = 0;
    private disposed = false;

    constructor(readonly spec: LaunchSpec, private readonly clientVersion: string, private readonly options: {
        open?: Opener; maxInFlight?: number; timeout?: number; retryBudget?: number; handshakeTimeout?: number;
    } = {}) {
        if (!spec.service) { throw new EvidenceFailure(explorerError('NO_PROJECT')); }
    }

    get project(): string { return this.spec.service!.project; }

    call(tool: NativeTool, args: Record<string, unknown>, options: CallOptions = {}): Promise<ToolResult> {
        if (this.disposed) { return Promise.reject(new EvidenceFailure(explorerError('SERVICE_UNAVAILABLE', 'The Explorer evidence session was closed.'))); }
        if (!(NATIVE_TOOLS as readonly string[]).includes(tool)) { return Promise.reject(new EvidenceFailure(explorerError('INVALID_MESSAGE'))); }
        return new Promise((resolve, reject) => {
            const task: Task = { tool, args, options, resolve, reject };
            if (options.supersede) {
                const generation = (this.generations.get(options.supersede) ?? 0) + 1;
                this.generations.set(options.supersede, generation); task.generation = generation;
                // A queued call that has not started yet is dropped immediately.
                for (let index = this.queue.length - 1; index >= 0; index--) {
                    if (this.queue[index].options.supersede === options.supersede) {
                        this.queue.splice(index, 1)[0].reject(new EvidenceFailure(explorerError('REQUEST_SUPERSEDED')));
                    }
                }
            }
            if (this.queue.length >= 64) { reject(new EvidenceFailure(explorerError('RATE_LIMITED', 'The evidence request queue is full.'))); return; }
            this.queue.push(task); this.pump();
        });
    }

    dispose(): void {
        this.disposed = true;
        for (const task of this.queue.splice(0)) { task.reject(new EvidenceFailure(explorerError('SERVICE_UNAVAILABLE', 'The Explorer evidence session was closed.'))); }
        this.session?.close(); this.session = undefined;
    }

    private pump(): void {
        while (!this.disposed && this.active < (this.options.maxInFlight ?? 3) && this.queue.length) {
            const task = this.queue.shift()!;
            this.active++;
            void this.run(task).then(result => {
                const current = task.options.supersede ? this.generations.get(task.options.supersede) : undefined;
                if (current !== undefined && current !== task.generation) { task.reject(new EvidenceFailure(explorerError('REQUEST_SUPERSEDED'))); }
                else { task.resolve(result); }
            }, error => task.reject(error)).finally(() => { this.active--; this.pump(); });
        }
    }

    private async run(task: Task): Promise<ToolResult> {
        const deadline = Date.now() + (this.options.retryBudget ?? 5000);
        while (true) {
            const session = await this.connect();
            let envelope: any;
            try {
                envelope = await session.request('tools/call', { name: task.tool, arguments: task.args }, task.options.timeout ?? this.options.timeout ?? 15000);
            } catch (error) {
                if (error instanceof McpSessionError && error.code === 'overloaded') { throw new EvidenceFailure(explorerError('RATE_LIMITED')); }
                if (error instanceof McpSessionError && error.code === 'timeout') { throw new EvidenceFailure(explorerError('REQUEST_TIMEOUT')); }
                if (error instanceof McpSessionError && error.code === 'rpc') { throw new EvidenceFailure(explorerError('TOOL_ERROR', error.message)); }
                throw new EvidenceFailure(explorerError('SERVICE_UNAVAILABLE', error instanceof Error ? error.message : String(error)));
            }
            const delay = readRetryDelay(envelope);
            if (delay !== undefined && Date.now() + delay < deadline && !this.disposed) {
                await new Promise(resolve => setTimeout(resolve, delay)); continue;
            }
            const text = Array.isArray(envelope?.content) ? envelope.content.find((part: any) => part?.type === 'text')?.text : undefined;
            if (envelope?.isError === true) { throw toolFailure(typeof text === 'string' ? text : 'AIRK returned an error without text.'); }
            if (typeof text !== 'string') { throw new EvidenceFailure(explorerError('TOOL_ERROR', 'AIRK returned no text content.')); }
            let payload: unknown;
            try { payload = JSON.parse(text); } catch { throw new EvidenceFailure(explorerError('TOOL_ERROR', 'AIRK returned content that is not JSON.')); }
            if (!payload || typeof payload !== 'object' || Array.isArray(payload)) { throw new EvidenceFailure(explorerError('TOOL_ERROR', 'AIRK returned an unexpected payload.')); }
            const record = payload as Record<string, unknown>;
            return { payload: record, publication: publicationOf(record) };
        }
    }

    private async connect(): Promise<McpStdioSession> {
        if (this.session && !this.session.closed) { return this.session; }
        if (!this.opening) {
            const open = this.options.open ?? ((options, timeout) => McpStdioSession.open(options, timeout));
            this.opening = open({ command: this.spec.command, args: launchArgs(this.spec), cwd: path.dirname(this.spec.command),
                clientName: EXPLORER_CLIENT_NAME, clientVersion: this.clientVersion }, this.options.handshakeTimeout ?? 90000)
                .then(({ session }) => {
                    if (this.disposed) { session.close(); throw new EvidenceFailure(explorerError('SERVICE_UNAVAILABLE', 'The Explorer evidence session was closed.')); }
                    this.session = session; return session;
                }, error => {
                    throw error instanceof EvidenceFailure ? error : new EvidenceFailure(explorerError('SERVICE_UNAVAILABLE', error instanceof Error ? error.message : String(error)));
                })
                .finally(() => { this.opening = undefined; });
        }
        return this.opening;
    }
}
