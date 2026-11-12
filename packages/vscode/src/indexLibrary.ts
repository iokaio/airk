// SPDX-License-Identifier: Apache-2.0
//! Read-only index library for the Explorer's Indexing page (U97). Each read is
//! one native catalog `ControlMethod` relayed through the window's `airkctl`
//! session (I1 metadata relay). Nothing here prepares, applies or retries a
//! catalog change; management is in AIRK Admin (U6). The complete native
//! response is returned unchanged so the page can show and copy every field.
import type { LIBRARY_KINDS, LIBRARY_METHODS } from './explorerContracts';

/** Native page budgets (CatalogPageRequest: limit ≤ MAX_SELECTION, max_bytes ≤ MAX_PAGE_BYTES = 48 KiB). */
export const LIBRARY_PAGE_LIMIT = 32;
export const LIBRARY_PAGE_BYTES = 49152;

export interface LibraryRequest {
    readonly method: typeof LIBRARY_METHODS[number];
    readonly kind?: typeof LIBRARY_KINDS[number];
    /** `idx_…` or `prj_…`. */
    readonly id?: string;
    /** The previous page's complete `continuation` cursor. */
    readonly after?: Record<string, unknown>;
    /** The first page's catalog revision, kept for continuations. */
    readonly revision?: number;
    /** A split operation ID (32 hex). */
    readonly operation?: string;
}
export interface LibraryError { readonly code: string; readonly message: string; readonly retryable: boolean }
/** One read: the exact native method sent and either the complete response or the error. */
export interface LibraryReply {
    readonly method: Record<string, unknown>;
    /** The `ControlResponse` status a successful read returns. */
    readonly expected: string;
    readonly response?: Record<string, unknown>;
    readonly error?: LibraryError;
    readonly observedAt: number;
}

export class LibraryRequestInvalid extends Error {
    readonly code = 'library_request_invalid';
    constructor(detail: string) { super(`Index library request is invalid: ${detail}`); }
}

const INDEX = /^idx_[0-9a-f]{32}$/, SET = /^prj_[0-9a-f]{32}$/, OPERATION = /^[0-9a-f]{32}$/;

/** The exact native method for one library read; refuses what the native validator would refuse. */
export function libraryMethod(request: LibraryRequest): { method: Record<string, unknown>; expected: string } {
    const id = request.id;
    switch (request.method) {
        case 'page': {
            const kind = request.kind;
            if (!kind) { throw new LibraryRequestInvalid('a page needs a kind'); }
            let index_id: string | null = null, project_id: string | null = null;
            if (kind === 'sources') {
                if (!id || !INDEX.test(id)) { throw new LibraryRequestInvalid('sources need an index'); }
                index_id = id;
            } else if (kind === 'memberships') {
                if (id !== undefined) {
                    if (INDEX.test(id)) { index_id = id; } else if (SET.test(id)) { project_id = id; } else { throw new LibraryRequestInvalid('memberships filter by an index or an index set'); }
                }
            } else if (id !== undefined) { throw new LibraryRequestInvalid(`${kind} take no filter`); }
            if (request.after === undefined && request.revision !== undefined) { throw new LibraryRequestInvalid('a revision belongs to a continuation'); }
            return {
                expected: 'catalog_page',
                method: { catalog_page: { request: {
                    kind, index_id, project_id, after: request.after ?? null,
                    expected_catalog_revision: request.after === undefined ? null : request.revision ?? null,
                    limit: LIBRARY_PAGE_LIMIT, max_bytes: LIBRARY_PAGE_BYTES,
                } } },
            };
        }
        case 'detail':
            if (!id) { throw new LibraryRequestInvalid('detail needs an index or an index set'); }
            return { expected: 'catalog_detail', method: { catalog_detail: { entity: { kind: INDEX.test(id) ? 'index' : 'project', id }, expected_catalog_revision: null } } };
        case 'policy':
            if (!id || !INDEX.test(id)) { throw new LibraryRequestInvalid('policy status needs an index'); }
            return { expected: 'catalog_policy_status', method: { catalog_policy_status: { index_id: id } } };
        case 'split':
            if (!request.operation || !OPERATION.test(request.operation)) { throw new LibraryRequestInvalid('split status needs an operation ID'); }
            return { expected: 'catalog_split_status', method: { catalog_split_status: { operation_id: request.operation } } };
    }
}

function errorOf(error: unknown): LibraryError {
    const value = error as { code?: unknown; reason?: unknown; retryable?: unknown } | undefined;
    const code = typeof value?.code === 'string' ? value.code : typeof value?.reason === 'string' ? value.reason : 'internal';
    return { code, message: error instanceof Error ? error.message : String(error), retryable: value?.retryable === true };
}

/**
 * One library read. A transport or native failure is returned with its code,
 * never thrown away or turned into an empty page; a response with another
 * status is returned complete and marked with that status as its code.
 */
export async function readLibrary(request: LibraryRequest, control: (method: Record<string, unknown>) => Promise<Record<string, unknown>>,
    now: () => number = Date.now): Promise<LibraryReply> {
    const { method, expected } = libraryMethod(request);
    try {
        const response = await control(method);
        if (response?.status === expected) { return { method, expected, response, observedAt: now() }; }
        const status = typeof response?.status === 'string' ? response.status : 'malformed';
        const code = typeof response?.code === 'string' ? response.code : `unexpected_${status}`;
        return { method, expected, response, error: { code, message: `The service answered ${status} instead of ${expected}.`, retryable: false }, observedAt: now() };
    } catch (error) {
        return { method, expected, error: errorOf(error), observedAt: now() };
    }
}
