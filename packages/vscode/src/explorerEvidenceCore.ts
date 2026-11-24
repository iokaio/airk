// SPDX-License-Identifier: Apache-2.0
// Pure evidence observations shared by editor and browser adapters.
import {explorerError, type ExplorerError} from './explorerErrors';

/** Read-only native tool catalog. The Explorer calls nothing else on AIRK Retrieval. */
export const NATIVE_TOOLS = ['airk_search', 'airk_read_file', 'airk_list_files', 'airk_status', 'airk_diagnostics', 'airk_help', 'airk_file_info', 'airk_outline',
    'airk_symbols', 'airk_imports', 'airk_document_facts', 'airk_references', 'airk_context', 'airk_capabilities', 'airk_code_facts',
    'airk_relationships', 'airk_changes', 'airk_query_scope'] as const;
export type NativeTool = typeof NATIVE_TOOLS[number];
export const WORKBENCH_TOOLS = [...NATIVE_TOOLS, 'airk_parse', 'airk_answer'] as const;
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

export function publicationValues(payload:Record<string,unknown>):readonly unknown[] {
    const read = payload.read as Record<string, unknown> | undefined;
    const evidence = payload.evidence as Record<string, unknown> | undefined;
    return [payload.publication,evidence?.publication,read?.current_publication,read?.publication];
}
export function publicationOf(payload: Record<string, unknown>): string | undefined {
    const tokens = publicationValues(payload).flatMap(publication => {
        if (typeof publication === 'string') { return [publication]; }
        return publication && typeof publication === 'object' && typeof (publication as Record<string, unknown>).token === 'string'
            ? [(publication as Record<string, unknown>).token as string] : [];
    });
    if (new Set(tokens).size > 1) { throw new EvidenceFailure(explorerError('EVIDENCE_MISMATCH', 'AIRK returned inconsistent publication identities.')); }
    return tokens[0];
}

export interface EvidenceReadPort {
    call(tool: EvidenceTool, args: Record<string,unknown>, options?: CallOptions): Promise<ToolResult>;
}
