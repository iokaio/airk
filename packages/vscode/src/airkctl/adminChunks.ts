// SPDX-License-Identifier: Apache-2.0
// NativeAdminChunk is Rust-owned; this owner validates bounded assembly only.
import type { NativeAdminChunk } from '../generated/i2';
export const MAX_ADMIN_RESULT_BYTES = 32 * 1024 * 1024;
export const MAX_ADMIN_CHUNK_BYTES = 64 * 1024;
export class AdminChunkError extends Error {
    constructor(readonly observedStatus: number | null) { super('admin_invalid_chunks'); this.name = 'AdminChunkError'; }
}
export function createAdminChunks(onStatus?: (status: number) => void) {
    let status: number | null = null, total = 0, offset = 0, complete = false;
    const parts: Uint8Array[] = [];
    const invalid = (): never => { throw new AdminChunkError(status); };
    return {
        get offset(): number { return offset; },
        get status(): number | null { return status; },
        accept(chunk: NativeAdminChunk): { httpStatus: number; body: string } | null {
            if (!chunk || typeof chunk !== 'object' || complete) return invalid();
            // First observed headers remain authoritative even if bytes fail.
            if (status === null && Number.isInteger(chunk.http_status) && chunk.http_status >= 100 && chunk.http_status <= 599) {
                status = chunk.http_status; onStatus?.(status);
            }
            if (Object.keys(chunk).length !== 5 || status === null || chunk.http_status !== status || !Number.isSafeInteger(chunk.total_bytes)
                || chunk.total_bytes < 0 || chunk.total_bytes > MAX_ADMIN_RESULT_BYTES || !Number.isSafeInteger(chunk.offset)
                || chunk.offset !== offset || typeof chunk.done !== 'boolean' || !Array.isArray(chunk.data)
                || chunk.data.length > MAX_ADMIN_CHUNK_BYTES || chunk.data.some(byte => !Number.isInteger(byte) || byte < 0 || byte > 255)) return invalid();
            if (offset === 0) total = chunk.total_bytes;
            else if (chunk.total_bytes !== total) return invalid();
            if (chunk.data.length > total - offset || chunk.done !== (offset + chunk.data.length === total)
                || !chunk.done && chunk.data.length === 0) return invalid();
            parts.push(Uint8Array.from(chunk.data)); offset += chunk.data.length;
            if (!chunk.done) return null;
            complete = true;
            const bytes = new Uint8Array(total); let written = 0;
            for (const part of parts) { bytes.set(part, written); written += part.length; }
            let body: string;
            try { body = new TextDecoder('utf-8', { fatal: true }).decode(bytes); JSON.parse(body); }
            catch { return invalid(); }
            return { httpStatus: status, body };
        },
        finish(): never { return invalid(); },
    };
}
