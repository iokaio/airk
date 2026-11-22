// SPDX-License-Identifier: Apache-2.0
import { transportDeadlineFor } from './answerDeadline';
import * as path from 'node:path';
import { isDeepStrictEqual } from 'node:util';
import { readRetryDelay } from './readRetry';
import { explorerError, ExplorerError } from './explorerErrors';
import { McpSessionError, McpStdioOptions, McpStdioSession } from './mcpStdioSession';
import { LaunchSpec, launchArgs } from './model';
import { ToolCallError, type ToolTransport } from './airkctl/tools';
import { hasNativeMetadataReferences, materializeNativeMetadata } from './nativeMetadata';

import {NATIVE_TOOLS, WORKBENCH_TOOLS, EXPLORER_CLIENT_NAME, EvidenceFailure, publicationOf, type EvidenceTool, type CallOptions, type ToolResult} from './explorerEvidenceCore';
export * from './explorerEvidenceCore';

/** Refusals of a read under the window search scope (contracts §2.5 W4, §3.7 Q4),
 * shown with their native code instead of a generic tool error. */
export const WINDOW_SCOPE_READ_ERRORS: Readonly<Record<string, string>> = Object.freeze({
    scoped_read_unavailable: 'Search scope out of date — reselect it in Indexing › Search scope.',
    window_scope_not_granted: 'This window has no search scope. Choose one in Indexing › Search scope.',
    not_yet_served: 'The AIRK Service does not serve this read yet (not_yet_served).',
});
const windowScopeError = (code: unknown): ExplorerError | undefined =>
    typeof code === 'string' && Object.hasOwn(WINDOW_SCOPE_READ_ERRORS, code) ? { code, message: WINDOW_SCOPE_READ_ERRORS[code] } : undefined;

function toolFailure(text: string, nativeResponse:Record<string,unknown>): EvidenceFailure {
    let nativePayload: Record<string,unknown> | undefined;
    try {
        const parsed: unknown = JSON.parse(text);
        if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
            nativePayload = materializeNativeMetadata(parsed as Record<string,unknown>);
        }
    } catch { nativePayload = {}; /* Refuse derived cards; retain the original response for copying. */ }
    try {
        const parsed = JSON.parse(text), scoped = windowScopeError(parsed?.error?.code ?? parsed?.code);
        if (scoped) { return new EvidenceFailure(scoped,nativeResponse,nativePayload); }
    } catch { /* Not a coded native error. */ }
    if (/current revision expired|known_hash does not match/.test(text)) { return new EvidenceFailure(explorerError('EVIDENCE_MISMATCH'),nativeResponse,nativePayload); }
    if (/workspace is not indexed|index_unavailable|no readable publication/i.test(text)) { return new EvidenceFailure(explorerError('INDEX_UNAVAILABLE'),nativeResponse,nativePayload); }
    try {
        const parsed = JSON.parse(text);
        const error = parsed?.error ?? parsed;
        if (error && typeof error === 'object' && typeof error.code === 'string') {
            // Native messages may contain source excerpts or paths. Preserve
            // them only in the original result, never in persistable guidance.
            return new EvidenceFailure(explorerError(error.code),nativeResponse,nativePayload);
        }
    } catch { /* Plain native error text belongs only to nativeResponse. */ }
    return new EvidenceFailure(explorerError('TOOL_ERROR'),nativeResponse,nativePayload);
}

interface Task { tool: EvidenceTool; args: Record<string, unknown>; options: CallOptions; generation?: number; resolve(value: ToolResult): void; reject(error: Error): void }
type Opener = (options: McpStdioOptions, handshakeTimeout: number) => Promise<{ session: McpStdioSession }>;
/** Opens the window's evidence transport for one reviewed selection. */
export type EvidenceTransportFactory = (spec: LaunchSpec, client: 'airk-explorer' | 'airk-workbench') => ToolTransport;
let defaultTransport: EvidenceTransportFactory | undefined;
/** Installed by the composition root: reads go through the window's `airkctl session` (plan §8.1). */
export function setEvidenceTransport(factory: EvidenceTransportFactory | undefined): void { defaultTransport = factory; }

/**
 * Explorer evidence uses the window's `airkctl session` (`tools call`) for its bound scope.
 * This client keeps a bounded queue, with three calls outstanding by default;
 * native admission independently enforces the configured service capacity.
 */
export class EvidenceClient {
    private session?: McpStdioSession|ToolTransport;
    private opening?: Promise<McpStdioSession|ToolTransport>;
    private readonly queue: Task[] = [];
    private readonly generations = new Map<string, number>();
    private active = 0;
    private disposed = false;

    constructor(readonly spec: LaunchSpec, private readonly clientVersion: string, private readonly options: {
        open?: Opener; maxInFlight?: number; timeout?: number; retryBudget?: number; handshakeTimeout?: number;
        clientName?: 'airk-explorer' | 'airk-workbench'; workbench?: boolean;
        resolveConnection?:(spec:LaunchSpec,client:'airk-explorer'|'airk-workbench')=>Promise<LaunchSpec>;
        transport?: EvidenceTransportFactory;
    } = {}) {
        if (!spec.service) { throw new EvidenceFailure(explorerError('NO_PROJECT')); }
    }

    get project(): string { return this.spec.service!.project; }

    /** Fence queued and admitted replies without claiming native work was undone. */
    cancel(key: string): void {
        this.generations.set(key, (this.generations.get(key) ?? 0) + 1);
        for (let i=this.queue.length-1;i>=0;i--) {
            if (this.queue[i].options.supersede===key) { this.queue.splice(i,1)[0].reject(new EvidenceFailure(explorerError('REQUEST_SUPERSEDED'))); }
        }
    }

    call(tool: EvidenceTool, args: Record<string, unknown>, options: CallOptions = {}): Promise<ToolResult> {
        if (this.disposed) { return Promise.reject(new EvidenceFailure(explorerError('SERVICE_UNAVAILABLE', 'The Explorer evidence session was closed.'))); }
        if (!((this.options.workbench ? WORKBENCH_TOOLS : NATIVE_TOOLS) as readonly string[]).includes(tool)) { return Promise.reject(new EvidenceFailure(explorerError('INVALID_MESSAGE'))); }
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
        const requestDeadline=Date.now()+(task.options.timeout??this.options.timeout??transportDeadlineFor(task.tool));
        const originalArguments=structuredClone(task.args);
        let expectedScope:unknown,scopeCaptured=false,previous:any,retries=0;
        while (true) {
            const session = await this.connect();
            const activeScope=()=>('readScope' in session&&typeof session.readScope==='function')?session.readScope():this.spec.combinedSession??this.spec.catalogSession;
            if(!scopeCaptured){expectedScope=structuredClone(activeScope());scopeCaptured=true;}
            else if(Date.now()>=Math.min(deadline,requestDeadline)||!isDeepStrictEqual(activeScope(),expectedScope)){
                const text=previous?.content?.find((part:any)=>part?.type==='text')?.text;
                throw toolFailure(typeof text==='string'?text:'The read authority changed before retry.',previous);
            }
            const remaining=requestDeadline-Date.now();
            if(remaining<=0){
                if(previous){
                    const text=previous.content?.find((part:any)=>part?.type==='text')?.text;
                    throw toolFailure(typeof text==='string'?text:'The original read deadline expired.',previous);
                }
                throw new EvidenceFailure(explorerError('REQUEST_TIMEOUT'));
            }
            let envelope: any;
            try {
                envelope = await session.request('tools/call', { name: task.tool, arguments: originalArguments }, remaining);
            } catch (error) {
                if (error instanceof McpSessionError && error.code === 'overloaded') { throw new EvidenceFailure(explorerError('RATE_LIMITED')); }
                if (error instanceof McpSessionError && error.code === 'timeout') { throw new EvidenceFailure(explorerError('REQUEST_TIMEOUT')); }
                if (error instanceof McpSessionError && error.code === 'rpc') { throw new EvidenceFailure(explorerError('TOOL_ERROR', error.message)); }
                if (error instanceof ToolCallError && error.code === 'overloaded') { throw new EvidenceFailure(explorerError('RATE_LIMITED')); }
                if (error instanceof ToolCallError && error.code === 'timeout') { throw new EvidenceFailure(explorerError('REQUEST_TIMEOUT')); }
                // The complete airkctl result (exit class, stdout, stderr, error line) stays with the failure.
                if (error instanceof ToolCallError && error.code === 'rpc') {
                    const scoped = windowScopeError(error.result?.error?.code);
                    throw new EvidenceFailure(scoped ?? explorerError('TOOL_ERROR', error.message), error.result ? { ...error.result } : undefined);
                }
                throw new EvidenceFailure(explorerError('SERVICE_UNAVAILABLE', error instanceof Error ? error.message : String(error)));
            }
            const delay = task.tool === 'airk_parse' || task.tool === 'airk_answer' ? undefined : readRetryDelay(envelope,expectedScope,originalArguments);
            const windowScope=!!expectedScope&&typeof expectedScope==='object'&&'window_scope_selection' in expectedScope;
            if (delay !== undefined && (!windowScope||retries===0) && Date.now() + delay < Math.min(deadline,requestDeadline)
                && !this.disposed && isDeepStrictEqual(activeScope(),expectedScope)) {
                previous=envelope;retries++;
                await new Promise(resolve => setTimeout(resolve, delay)); continue;
            }
            const text = Array.isArray(envelope?.content) ? envelope.content.find((part: any) => part?.type === 'text')?.text : undefined;
            if (envelope?.isError === true) { throw toolFailure(typeof text === 'string' ? text : 'AIRK returned an error without text.',envelope); }
            if (typeof text !== 'string') { throw new EvidenceFailure(explorerError('TOOL_ERROR', 'AIRK returned no text content.')); }
            let payload: unknown;
            try { payload = JSON.parse(text); } catch { throw new EvidenceFailure(explorerError('TOOL_ERROR', 'AIRK returned content that is not JSON.')); }
            if (!payload || typeof payload !== 'object' || Array.isArray(payload)) { throw new EvidenceFailure(explorerError('TOOL_ERROR', 'AIRK returned an unexpected payload.')); }
            const raw = payload as Record<string, unknown>;
            let record: Record<string,unknown>;
            try { record = materializeNativeMetadata(raw); }
            catch { throw new EvidenceFailure(explorerError('TOOL_ERROR', 'AIRK native metadata could not be decoded. The original response is retained.'), envelope, {}); }
            return { payload: record, ...(hasNativeMetadataReferences(raw) ? { rawPayload: raw } : {}), publication: publicationOf(record) };
        }
    }

    private async connect(): Promise<McpStdioSession|ToolTransport> {
        if (this.session && !this.session.closed) { return this.session; }
        if (!this.opening) {
            // An injected test opener speaks MCP over stdio; production reads use the
            // window session's `airkctl tools call` with the reviewed selection named.
            const open = this.options.open, client = this.options.clientName ?? EXPLORER_CLIENT_NAME;
            const transport = this.options.transport ?? defaultTransport;
            const connected: Promise<{ session: McpStdioSession|ToolTransport }> = open
                ? open({ command: this.spec.command, args: launchArgs(this.spec,'mcp',true), cwd: path.dirname(this.spec.command),
                    clientName: client, clientVersion: this.clientVersion }, this.options.handshakeTimeout ?? 90000)
                : (this.options.resolveConnection ? this.options.resolveConnection(this.spec, client) : Promise.resolve(this.spec)).then(spec => {
                    if (!transport) { throw new Error('evidence_transport_unavailable'); }
                    // A read always names its reviewed selection; nothing is read without one.
                    if (!spec.service?.project && !spec.catalogSession && !spec.combinedSession) { throw new Error('evidence_selection_required'); }
                    return { session: transport(spec, client) };
                });
            this.opening = connected
                .then(({ session }) => {
                    if (this.disposed) { session.close(); throw new EvidenceFailure(explorerError('SERVICE_UNAVAILABLE', 'The Explorer evidence session was closed.')); }
                    this.session = session; return session;
                }, error => {
                    if(error instanceof EvidenceFailure)throw error;
                    const detail=error instanceof Error?error.message:String(error);
                    throw new EvidenceFailure(explorerError(detail==='evidence_selection_required'?'SCOPE_NOT_GRANTED':'SERVICE_UNAVAILABLE',detail));
                })
                .finally(() => { this.opening = undefined; });
        }
        return this.opening;
    }
}
