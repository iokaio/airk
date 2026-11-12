// SPDX-License-Identifier: Apache-2.0
//! Explorer and Workbench evidence reads over the window session: each MCP
//! `tools/call` becomes `airkctl tools call <tool> --args <json> --output json`
//! with the reviewed selection named explicitly (plan §8.1 "All reads go
//! through the airkctl session"). `tools call` prints the unchanged MCP result
//! envelope, so the Explorer keeps the complete native result (read parity).
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
/** The reviewed selection a read names. Exactly one form is sent. */
export interface ToolSelection {
    readonly project?: string;
    readonly catalogSession?: unknown;
    readonly combinedSession?: unknown;
}
export function selectionArguments(selection: ToolSelection): string[] {
    if (selection.combinedSession) { return ['--combined-session', JSON.stringify(selection.combinedSession)]; }
    if (selection.catalogSession) { return ['--catalog-session', JSON.stringify(selection.catalogSession)]; }
    if (selection.project) { return ['--service-project', selection.project]; }
    return [];
}

export class AirkctlToolSession implements ToolTransport {
    private stopped = false;
    private readonly aborts = new Set<AbortController>();
    constructor(private readonly session: AirkctlSession, private readonly selection: ToolSelection) {}
    get closed(): boolean { return this.stopped || this.session.closed; }
    close(): void { this.stopped = true; for (const abort of this.aborts) { abort.abort(); } this.aborts.clear(); }

    async request(_method: 'tools/call', params: { name: string; arguments: Record<string, unknown> }, timeoutMs: number): Promise<unknown> {
        if (this.closed) { throw new ToolCallError('unavailable', 'The evidence session was closed.'); }
        const args = JSON.stringify(params.arguments ?? {});
        if (Buffer.byteLength(args, 'utf8') > MAX_INVOKE_ARG_BYTES) {
            throw new ToolCallError('too_large', 'The tool arguments exceed the 32 KiB session argument budget.');
        }
        const argv = ['tools', 'call', params.name, '--args', args, ...selectionArguments(this.selection), '--output', 'json'];
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
        // A failed native read prints its complete result on stdout with exit 1 (contracts §3.3).
        if (result.class === 'success' || result.class === 'refused' && result.stdout.trim().startsWith('{')) {
            try { return JSON.parse(result.stdout); } catch { /* Fall through to the error line. */ }
        }
        const error = result.error;
        if (error?.retryable && /busy|overload|capacity/.test(error.code)) { throw new ToolCallError('overloaded', error.message, result); }
        throw new ToolCallError('rpc', error ? `${error.code}: ${error.message}` : `airkctl tools call exited with ${result.class}`, result);
    }
}
