// SPDX-License-Identifier: Apache-2.0
import * as path from 'node:path';
import { readRetryDelay } from './readRetry';
import { explorerError, ExplorerError } from './explorerErrors';
import { McpSessionError, McpStdioOptions, McpStdioSession } from './mcpStdioSession';
import { LaunchSpec, launchArgs } from './model';
import { ToolCallError, type ToolTransport } from './airkctl/tools';
import { hasNativeMetadataReferences, materializeNativeMetadata } from './nativeMetadata';

/** Read-only native tool catalog. The Explorer calls nothing else on AIRK Retrieval. */
export const NATIVE_TOOLS = ['airk_search', 'airk_read_file', 'airk_list_files', 'airk_status', 'airk_diagnostics', 'airk_help', 'airk_file_info', 'airk_outline',
    'airk_symbols', 'airk_imports', 'airk_document_facts', 'airk_references', 'airk_context', 'airk_capabilities', 'airk_code_facts',
    'airk_relationships', 'airk_changes', 'airk_query_scope'] as const;
export type NativeTool = typeof NATIVE_TOOLS[number];
export const WORKBENCH_TOOLS = [...NATIVE_TOOLS, 'airk_parse'] as const;
export type EvidenceTool = typeof WORKBENCH_TOOLS[number];

/** Usage attribution label recorded by the native worker (ClientFamily::Explorer). */
export const EXPLORER_CLIENT_NAME = 'airk-explorer';

export class EvidenceFailure extends Error {
    /** An actual native error reply, retained separately from friendly guidance.
     * Transport failures have no native response and never synthesize one. */
    readonly nativeResponse?: Record<string,unknown>;
    readonly nativePayload?: Record<string,unknown>;
    constructor(readonly error: ExplorerError, nativeResponse?:Record<string,unknown>, nativePayload?:Record<string,unknown>) {
        super(error.message);
        if(nativeResponse)this.nativeResponse=structuredClone(nativeResponse);
        if(nativePayload)this.nativePayload=structuredClone(nativePayload);
    }
}
export interface ToolResult { readonly payload: Record<string, unknown>; readonly rawPayload?: Record<string, unknown>; readonly publication?: string }
/** A changed, present identity is retryable at report scope; missing identity is not. */
export class PublicationChanged extends EvidenceFailure {
    constructor() { super(explorerError('EVIDENCE_MISMATCH', 'The index changed during this report; refresh it.')); }
}
/** One report must never silently combine different publications. Missing identity fails closed. */
export function samePublication(expected: string | undefined, result: ToolResult): string {
    if (!result.publication) { throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH', 'AIRK returned no publication identity; this report cannot be verified.')); }
    if (expected && result.publication !== expected) { throw new PublicationChanged(); }
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

function publicationOf(payload: Record<string, unknown>): string | undefined {
    const read = payload.read as Record<string, unknown> | undefined;
    const evidence = payload.evidence as Record<string, unknown> | undefined;
    const tokens = [payload.publication, evidence?.publication, read?.current_publication, read?.publication].flatMap(publication => {
        if (typeof publication === 'string') { return [publication]; }
        return publication && typeof publication === 'object' && typeof (publication as Record<string, unknown>).token === 'string'
            ? [(publication as Record<string, unknown>).token as string] : [];
    });
    if (new Set(tokens).size > 1) { throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH', 'AIRK returned inconsistent publication identities.')); }
    return tokens[0];
}

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
        while (true) {
            const session = await this.connect();
            let envelope: any;
            try {
                envelope = await session.request('tools/call', { name: task.tool, arguments: task.args }, task.options.timeout ?? this.options.timeout ?? 15000);
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
            const delay = task.tool === 'airk_parse' ? undefined : readRetryDelay(envelope,this.spec.catalogSession);
            if (delay !== undefined && Date.now() + delay < deadline && !this.disposed) {
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
