// SPDX-License-Identifier: Apache-2.0
//! Explorer and Workbench evidence reads over the window session: each MCP
//! `tools/call` becomes `airkctl tools call <tool> --args <json> --output json`
//! with the reviewed selection named explicitly (plan §8.1 "All reads go
//! through the airkctl session"). `tools call` prints the unchanged MCP result
//! envelope, so the Explorer keeps the complete native result (read parity).
//! A read under the window search scope names no selection: while the window
//! holds a W4 grant the session reads it with `selection: Window` and
//! re-selects once on revision drift (contracts §3.7, Q4).
import type { AirkctlSession, InvokeResult } from './session';
import { MAX_INVOKE_ARG_BYTES, SessionError } from './session';

/** The subset of the former MCP session the evidence client uses. */
export interface ToolTransport {
    request(method: 'tools/call', params: { name: string; arguments: Record<string, unknown> }, timeoutMs: number): Promise<unknown>;
    readonly closed: boolean;
    close(): void;
}
export class ToolCallError extends Error {
    constructor(readonly code: 'timeout' | 'overloaded' | 'rpc' | 'unavailable' | 'too_large', message: string, readonly result?: InvokeResult) {
        super(message); this.name = 'ToolCallError';
    }
}
/** The reviewed selection a read names. Exactly one form is sent; `window`
 * names none, so the read runs under the window's search scope. */
export interface ToolSelection {
    readonly window?: boolean;
    readonly project?: string;
    readonly catalogSession?: unknown;
    readonly combinedSession?: unknown;
}
export function selectionArguments(selection: ToolSelection): string[] {
    if (selection.window) { return []; }
    if (selection.combinedSession) { return ['--combined-session', JSON.stringify(selection.combinedSession)]; }
    if (selection.catalogSession) { return ['--catalog-session', JSON.stringify(selection.catalogSession)]; }
    if (selection.project) { return ['--service-project', selection.project]; }
    return [];
}

const object = (value: unknown): Record<string, unknown> | undefined => value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : undefined;
/** The native error code of a failed read: the `airkctl` error line, the
 * printed result's `error.code`, or the code in an MCP error envelope's text. */
export function readFailureCode(result: InvokeResult): string | undefined {
    if (result.error?.code) { return result.error.code; }
    let printed: Record<string, unknown> | undefined;
    try { printed = object(JSON.parse(result.stdout)); } catch { return undefined; }
    const direct = object(printed?.error)?.code ?? (printed?.isError === true ? undefined : printed?.code);
    if (typeof direct === 'string') { return direct; }
    const text = Array.isArray(printed?.content) ? (printed!.content as unknown[]).map(object).find(part => part?.type === 'text')?.text : undefined;
    if (typeof text !== 'string') { return undefined; }
    try { const body = object(JSON.parse(text)); const code = object(body?.error)?.code ?? body?.code; return typeof code === 'string' ? code : undefined; } catch { return undefined; }
}

export interface ToolSessionHooks {
    /** A read under the window scope failed with this native code (for example `scoped_read_unavailable`). */
    windowReadFailed?(code: string): void;
}

export class AirkctlToolSession implements ToolTransport {
    private stopped = false;
    private readonly aborts = new Set<AbortController>();
    /** `selection` is read per call: the window scope can be chosen or cleared while the transport lives. */
    constructor(private readonly session: AirkctlSession, private readonly selection: ToolSelection | (() => ToolSelection), private readonly hooks: ToolSessionHooks = {}) {}
    get closed(): boolean { return this.stopped || this.session.closed; }
    close(): void { this.stopped = true; for (const abort of this.aborts) { abort.abort(); } this.aborts.clear(); }

    async request(_method: 'tools/call', params: { name: string; arguments: Record<string, unknown> }, timeoutMs: number): Promise<unknown> {
        if (this.closed) { throw new ToolCallError('unavailable', 'The evidence session was closed.'); }
        const args = JSON.stringify(params.arguments ?? {});
        if (Buffer.byteLength(args, 'utf8') > MAX_INVOKE_ARG_BYTES) {
            throw new ToolCallError('too_large', 'The tool arguments exceed the 32 KiB session argument budget.');
        }
        const selection = typeof this.selection === 'function' ? this.selection() : this.selection;
        const argv = ['tools', 'call', params.name, '--args', args, ...selectionArguments(selection), '--output', 'json'];
        const abort = new AbortController(); this.aborts.add(abort);
        let result: InvokeResult;
        try { result = await this.session.invoke(argv, { deadlineMs: timeoutMs, signal: abort.signal }); }
        catch (error) {
            if (error instanceof SessionError) {
                if (error.code === 'session_deadline_exceeded') { throw new ToolCallError('timeout', 'The read did not finish before its deadline.'); }
                if (error.code === 'session_busy' || error.code === 'session_queue_full') { throw new ToolCallError('overloaded', error.code); }
            }
            throw new ToolCallError('unavailable', error instanceof Error ? error.message : String(error));
        } finally { this.aborts.delete(abort); }
        // A refused read may also come back as an MCP error envelope on a success exit.
        if (selection.window && (result.class !== 'success' || /scoped_read_unavailable|window_scope_not_granted/.test(result.stdout))) {
            const code = readFailureCode(result);
            if (code) { try { this.hooks.windowReadFailed?.(code); } catch { /* Observers never change the read's outcome. */ } }
        }
        // A failed native read prints its complete result on stdout with exit 1 (contracts §3.3).
        if (result.class === 'success' || result.class === 'refused' && result.stdout.trim().startsWith('{')) {
            try { return JSON.parse(result.stdout); } catch { /* Fall through to the error line. */ }
        }
        const error = result.error;
        if (error?.retryable && /busy|overload|capacity/.test(error.code)) { throw new ToolCallError('overloaded', error.message, result); }
        throw new ToolCallError('rpc', error ? `${error.code}: ${error.message}` : `airkctl tools call exited with ${result.class}`, result);
    }
}
