// SPDX-License-Identifier: Apache-2.0
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { applyEdits, findNodeAtLocation, getNodeValue, modify, Node, ParseError, parseTree } from 'jsonc-parser';
import { Client, LaunchSpec, configFor, launchArgs } from './model';

export type JsonClient = 'claude' | 'gemini' | 'antigravity';
export function isJsonClient(client: Client): client is JsonClient {
    return client === 'claude' || client === 'gemini' || client === 'antigravity';
}
export function jsonConfigPath(client: JsonClient, spec: LaunchSpec): string {
    return path.join(spec.root, ...({ claude: ['.mcp.json'], gemini: ['.gemini', 'settings.json'],
        antigravity: ['.agents', 'mcp_config.json'] }[client]));
}
const limit = 1_048_576;

function configurationTree(source: string): Node {
    if (Buffer.byteLength(source) > limit) { throw new Error('Configuration exceeds the 1 MiB editing limit. Use manual setup.'); }
    const errors: ParseError[] = [];
    const tree = parseTree(source, errors, { allowTrailingComma: true });
    if (errors.length || tree?.type !== 'object') { throw new Error('Configuration must be a valid JSON object. Fix it before installing AIRK.'); }
    const unique = (node: Node): void => {
        if (node.type === 'object') {
            const keys = node.children!.map(property => property.children![0].value);
            if (new Set(keys).size !== keys.length) { throw new Error('Configuration contains duplicate keys. Resolve them before installing AIRK.'); }
        }
        node.children?.forEach(unique);
    };
    unique(tree);
    return tree;
}

/** Insert only a missing server; retain comments, other settings and client approval policy. */
export function mergeJsonConfig(client: JsonClient, spec: LaunchSpec, source: string): string {
    const tree = configurationTree(source);
    const servers = findNodeAtLocation(tree, ['mcpServers']);
    if (servers && servers.type !== 'object') { throw new Error('mcpServers must be an object. Existing configuration was preserved.'); }
    const name = `airk-${spec.collection}`;
    const entry = JSON.parse(configFor(client, spec)).mcpServers[name];
    const existing = findNodeAtLocation(tree, ['mcpServers', name]);
    if (existing) {
        const value = getNodeValue(existing);
        if (existing.type === 'object' && value.command === entry.command && JSON.stringify(value.args) === JSON.stringify(entry.args)
            && (!value.type || value.type === 'stdio') && !value.url && !value.serverUrl) { return source; }
        throw new Error('An AIRK server with this name already has different settings. Open the configuration and review it; nothing was replaced.');
    }
    const indent = source.match(/(?:^|\n)([\t ]+)"/u)?.[1];
    return applyEdits(source, modify(source, ['mcpServers', name], entry, { formattingOptions: {
        insertSpaces: !indent?.includes('\t'), tabSize: indent?.length || 2, eol: source.includes('\r\n') ? '\r\n' : '\n'
    } }));
}

async function configurationFile(client: JsonClient, spec: LaunchSpec, create = true): Promise<string> {
    const root = await fs.realpath(spec.root);
    const destination = jsonConfigPath(client, { ...spec, root }), directory = path.dirname(destination);
    if (directory !== root) {
        try { if (create) { await fs.mkdir(directory); } }
        catch (error) { if ((error as NodeJS.ErrnoException).code !== 'EEXIST') { throw error; } }
        const info = await fs.lstat(directory);
        if (!info.isDirectory() || info.isSymbolicLink() || await fs.realpath(directory) !== directory) {
            throw new Error('Configuration directory must be a regular directory inside this workspace. Use manual setup for redirected configuration.');
        }
    }
    return destination;
}

export interface LegacyJsonReview { readonly destination: string; readonly before: string; readonly after: string }
const legacyReviews = new WeakSet<LegacyJsonReview>();

function legacyReplacement(client: JsonClient, spec: LaunchSpec, source: string): string {
    if (!spec.service) { throw new Error('Legacy review requires a registered native service project.'); }
    const tree = configurationTree(source), name = `airk-${spec.collection}`;
    const node = findNodeAtLocation(tree, ['mcpServers', name]);
    if (node?.type !== 'object') { throw new Error('No same-named legacy AIRK server was found. Use normal installation or manual review.'); }
    const entry = getNodeValue(node);
    const args: unknown = entry.args;
    if (typeof entry.command !== 'string' || !['airk', 'airk.exe'].includes(path.basename(entry.command).toLowerCase())
        || !path.isAbsolute(entry.command) || 'url' in entry || 'serverUrl' in entry || (entry.type !== undefined && entry.type !== 'stdio')
        || !Array.isArray(args) || args.length !== 7 || args.some(value => typeof value !== 'string')
        || !path.isAbsolute(args[3])
        || JSON.stringify(args) !== JSON.stringify(launchArgs({ ...spec, service: undefined, dataDir: args[3] }))) {
        throw new Error('This entry does not match the bounded legacy AIRK launch for this root and collection. Review it manually.');
    }
    const replacement = { ...entry, ...JSON.parse(configFor(client, spec)).mcpServers[name] };
    const indent = source.match(/(?:^|\n)([\t ]+)"/u)?.[1];
    return applyEdits(source, modify(source, ['mcpServers', name], replacement, { formattingOptions: {
        insertSpaces: !indent?.includes('\t'), tabSize: indent?.length || 2, eol: source.includes('\r\n') ? '\r\n' : '\n'
    } }));
}

/** Read-only candidate discovery. Matching arguments do not prove an old process owner. */
export async function reviewLegacyJsonConfig(client: JsonClient, spec: LaunchSpec, checkCurrent: () => void): Promise<LegacyJsonReview> {
    checkCurrent();
    const destination = await configurationFile(client, spec, false), before = await readJsonConfig(destination);
    if (before === undefined) { throw new Error('No project configuration exists to review.'); }
    const review = Object.freeze({ destination, before, after: legacyReplacement(client, spec, before) });
    checkCurrent();
    if (await configurationFile(client, spec, false) !== destination || await readJsonConfig(destination) !== before) {
        throw new Error('Configuration changed while preparing review. Review the current file again.');
    }
    legacyReviews.add(review); return review;
}

/** Apply only the exact reviewed candidate after fresh trust, scope and file checks. */
export async function replaceReviewedLegacyJsonConfig(client: JsonClient, spec: LaunchSpec, review: LegacyJsonReview,
    checkCurrent: () => void, save: (destination: string, before: string, after: string) => Promise<void>): Promise<void> {
    checkCurrent();
    if (!legacyReviews.has(review) || await configurationFile(client, spec, false) !== review.destination
        || legacyReplacement(client, spec, review.before) !== review.after || await readJsonConfig(review.destination) !== review.before) {
        throw new Error('Legacy configuration review expired. Review the current file again.');
    }
    checkCurrent();
    await save(review.destination, review.before, review.after);
    legacyReviews.delete(review);
}

export async function readJsonConfig(destination: string): Promise<string | undefined> {
    try {
        const info = await fs.lstat(destination);
        if (!info.isFile() || info.isSymbolicLink() || info.nlink !== 1 || info.size > limit) {
            throw new Error('Configuration must be a regular, unlinked file of at most 1 MiB. Use manual setup.');
        }
        const handle = await fs.open(destination, 'r');
        try {
            const opened = await handle.stat();
            if (info.ino !== opened.ino || info.dev !== opened.dev) { throw new Error('Configuration changed. Retry setup.'); }
            const buffer = Buffer.alloc(limit + 1);
            let length = 0;
            while (length < buffer.length) {
                const { bytesRead } = await handle.read(buffer, length, buffer.length - length, length);
                if (!bytesRead) { break; }
                length += bytesRead;
            }
            if (length > limit) { throw new Error('Configuration exceeds the 1 MiB editing limit.'); }
            return new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(buffer.subarray(0, length));
        } finally { await handle.close(); }
    } catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') { return; } throw error; }
}

/** Existing files are saved through the editor, which checks document versions and disk conflicts. */
export async function installJsonConfig(client: JsonClient, spec: LaunchSpec,
    checkCurrent: () => void, save: (destination: string, before: string, after: string) => Promise<void>): Promise<string> {
    checkCurrent();
    const destination = await configurationFile(client, spec);
    const before = await readJsonConfig(destination);
    const after = mergeJsonConfig(client, spec, before ?? '{}\n');
    checkCurrent();
    if (await configurationFile(client, spec) !== destination || await readJsonConfig(destination) !== before) {
        throw new Error('Configuration changed while preparing setup. Retry after reviewing the file.');
    }
    checkCurrent();
    if (before === undefined) {
        await fs.writeFile(destination, after, { encoding: 'utf8', flag: 'wx', mode: 0o600 });
    } else if (after !== before) { await save(destination, before, after); }
    return destination;
}
