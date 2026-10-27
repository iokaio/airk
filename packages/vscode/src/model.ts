// SPDX-License-Identifier: Apache-2.0
import { createHash } from 'node:crypto';
import * as path from 'node:path';
import type { VectorStatus } from './vectorModel';

export interface LaunchSpec { command: string; root: string; dataDir: string; collection: string; name: string;
    service?: { installDir: string; project: string } }
export interface Usage { collection: string; disk_bytes: number; indexed_files: number | null; generation: number | null; status: string }
export interface IndexReport { generation: number; indexed: number; reused: number; removed: number; source_bytes?: number; skipped_truncated?: number;
    repaired_retained_artifacts?: number; retained_repair_retries?: number; skipped: { path: string; reason: string }[];
    path_scoped?: boolean; yield_to_source?: boolean; scan_extract_us?: number; publish_us?: number; first_ready_us?: number | null; fully_ready_us?: number | null;
    embedding_model_load_us?: number; embedding_inference_us?: number; vector_graph_insert_us?: number;
    indexed_vectors?: number; semantic_readiness?: VectorStatus['semantic_readiness']; discovery_readiness?: VectorStatus['discovery_readiness'];
    maintenance_error?: string | null }
export type IndexMode = 'manual' | 'onSave' | 'debounced' | 'idle';
/** Result of the extension's own MCP handshake check for one collection (not proof that an agent connected). */
export interface McpVerification {
    phase: 'preparing' | 'checking' | 'verified' | 'failed'; checkedAt?: number; tools?: string[]; error?: string;
    indexState?: 'not_indexed' | 'unavailable' | 'published';
}
export const clients = ['vscode', 'claude', 'codex', 'cursor', 'gemini', 'antigravity', 'cline', 'roo', 'junie', 'copilot-cli', 'opencode', 'opencode-v2', 'kilo', 'amazon-q', 'windsurf'] as const;
export type Client = typeof clients[number];

export function collectionId(canonicalRoot: string, platform = process.platform): string {
    const identity = platform === 'win32' ? canonicalRoot.toLowerCase() : canonicalRoot;
    return 'project-' + createHash('sha256').update(identity).digest('hex').slice(0, 24);
}

export function launchArgs(spec: LaunchSpec, command = 'mcp'): string[] {
    if(spec.service){
        if(!path.isAbsolute(spec.service.installDir)||!/^[a-f0-9]{64}$/.test(spec.service.project))throw new Error('Invalid trusted service launch binding.');
        return ['runtime','launch','--install-dir',spec.service.installDir,'--','--root',spec.root,'--data-dir',spec.dataDir,
            '--collection',spec.collection,'--service-project',spec.service.project,command];
    }
    return ['--root', spec.root, '--data-dir', spec.dataDir, '--collection', spec.collection, command];
}

export function eligibleEvent(file: string, root: string, dataDir: string): boolean {
    const relative = path.relative(root, file);
    if (!relative || relative.startsWith('..' + path.sep) || relative === '..' || path.isAbsolute(relative)) { return false; }
    const inData = path.relative(dataDir, file);
    if (!inData.startsWith('..' + path.sep) && inData !== '..' && !path.isAbsolute(inData)) { return false; }
    return !relative.split(path.sep).some(part => {
        const name = part.toLowerCase();
        return ['.git', '.airk', 'node_modules', 'target', '.venv', '__pycache__', '.aws', '.ssh', '.env'].includes(name)
            || name.startsWith('.env.') || name.endsWith('.pem') || name.endsWith('.key');
    });
}

/** One MCP server an agent configuration carries; keys of the two AIRK connection types can never collide. */
export interface ServerEntry { readonly key: string; readonly command: string; readonly args: readonly string[]; readonly label: string }
export const ASSISTANCE_KEY = 'airk_explorer';

/** AIRK Retrieval: the native read-only evidence tools for one collection. */
export function retrievalEntry(spec: LaunchSpec): ServerEntry {
    return { key: 'airk-' + spec.collection, command: spec.command, args: launchArgs(spec), label: 'AIRK Retrieval (evidence)' };
}

/**
 * AIRK Explorer Assistance for external agents: the stable launcher forwards to the runtime's
 * descriptor-checked `explorer-assistance` subcommand, which starts the extension's bundled server.
 */
export function assistanceEntry(spec: LaunchSpec, descriptor: string): ServerEntry | undefined {
    if (!spec.service || !path.isAbsolute(descriptor)) { return undefined; }
    return { key: ASSISTANCE_KEY, command: spec.command, label: 'AIRK Explorer Assistance (help and UI status)',
        args: ['runtime', 'launch', '--install-dir', spec.service.installDir, '--', 'explorer-assistance', '--descriptor', descriptor] };
}

/** Serialize entries in the selected client's configuration format. */
export function serializeServers(client: Client, entries: readonly ServerEntry[]): string {
    if (client === 'amazon-q' || client === 'windsurf') {
        // Connection details only: these clients' agent/version-specific schemas differ.
        const details = (entry: ServerEntry) => ({ transport: 'stdio', command: entry.command, args: entry.args });
        return JSON.stringify(entries.length === 1 ? details(entries[0]) : Object.fromEntries(entries.map(entry => [entry.key, details(entry)])), null, 2) + '\n';
    }
    if (client === 'codex') {
        return entries.map(entry => `[mcp_servers.${entry.key}]\ncommand = ${JSON.stringify(entry.command)}\nargs = ${JSON.stringify(entry.args)}\n`).join('\n');
    }
    const servers: Record<string, unknown> = {};
    for (const entry of entries) {
        const server: Record<string, unknown> = { command: entry.command, args: entry.args };
        if (client === 'vscode') { servers[entry.key] = { type: 'stdio', ...server }; }
        else if (client === 'opencode' || client === 'opencode-v2' || client === 'kilo') { servers[entry.key] = { type: 'local', command: [entry.command, ...entry.args] }; }
        else {
            if (client === 'cursor') { server.type = 'stdio'; }
            if (client === 'copilot-cli') { server.type = 'local'; server.tools = ['*']; }
            servers[entry.key] = server;
        }
    }
    const config = client === 'vscode' ? { servers } : client === 'opencode-v2' ? { mcp: { servers } }
        : client === 'opencode' || client === 'kilo' ? { mcp: servers } : { mcpServers: servers };
    return JSON.stringify(config, null, 2) + '\n';
}

/** The retrieval connection alone, as earlier releases configured it. */
export function configFor(client: Client, spec: LaunchSpec): string {
    return serializeServers(client, [retrievalEntry(spec)]);
}
