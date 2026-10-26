// SPDX-License-Identifier: Apache-2.0
import { createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import type { LiveCheck } from './explorerContracts';
import { explorerError } from './explorerErrors';
import { EvidenceFailure } from './explorerEvidence';

/** AIRK reports Windows roots in namespaced form (\\?\C:\…); VS Code URIs need the ordinary form. */
export function displayPath(native: string): string {
    if (native.startsWith('\\\\?\\UNC\\')) { return '\\\\' + native.slice(8); }
    if (native.startsWith('\\\\?\\')) { return native.slice(4); }
    return native;
}

export const caseFold = (value: string) => process.platform === 'win32' ? value.toLowerCase() : value;
export function inside(parent: string, child: string): boolean {
    const relative = path.relative(caseFold(parent), caseFold(child));
    return relative === '' || (!relative.startsWith('..') && !path.isAbsolute(relative));
}

/**
 * Resolve an AIRK source location (root + collection-relative path from airk_file_info)
 * to a file that stays inside that root after resolving links.
 */
export async function sourceFile(root: string, relative: string): Promise<string> {
    const base = displayPath(root);
    if (!path.isAbsolute(base) || typeof relative !== 'string' || !relative || path.isAbsolute(relative)
        || relative.replaceAll('\\', '/').split('/').includes('..')) {
        throw new EvidenceFailure(explorerError('TOOL_ERROR', 'AIRK reported a source location outside its root.'));
    }
    const file = path.resolve(base, relative);
    if (!inside(base, file)) { throw new EvidenceFailure(explorerError('TOOL_ERROR', 'AIRK reported a source location outside its root.')); }
    try {
        const [realRoot, realFile] = await Promise.all([fs.realpath(base), fs.realpath(file)]);
        if (!inside(realRoot, realFile)) { throw new EvidenceFailure(explorerError('TOOL_ERROR', 'The source file resolves outside its project root.')); }
        return realFile;
    } catch (error) {
        if (error instanceof EvidenceFailure) { throw error; }
        return file;
    }
}

/** SHA-256 of the saved bytes, compared with AIRK's content_hash (raw source bytes). */
export async function liveCheck(file: string, contentHash: string, dirty: boolean): Promise<LiveCheck> {
    if (dirty) { return 'not_saved'; }
    let bytes: Buffer;
    try {
        const stat = await fs.stat(file);
        if (!stat.isFile()) { return 'missing'; }
        if (stat.size > 512 * 1024 * 1024) { return 'unchecked'; }
        bytes = await fs.readFile(file);
    } catch { return 'missing'; }
    return createHash('sha256').update(bytes).digest('hex') === contentHash ? 'matches_index' : 'changed_since_index';
}
