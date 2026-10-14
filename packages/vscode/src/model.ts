// SPDX-License-Identifier: Apache-2.0
import { createHash } from 'node:crypto';
import * as path from 'node:path';

export interface LaunchSpec { command: string; root: string; dataDir: string; collection: string; name: string }
export interface Usage { collection: string; disk_bytes: number; indexed_files: number | null; generation: number | null; status: string }
export interface IndexReport { generation: number; indexed: number; reused: number; removed: number; source_bytes?: number; skipped_truncated?: number; skipped: { path: string; reason: string }[] }
export type IndexMode = 'manual' | 'onSave' | 'debounced' | 'idle';
export const clients = ['vscode', 'claude', 'codex', 'cursor', 'gemini', 'cline', 'roo', 'junie', 'copilot-cli', 'opencode', 'opencode-v2', 'kilo', 'amazon-q', 'windsurf'] as const;
export type Client = typeof clients[number];

export function collectionId(canonicalRoot: string, platform = process.platform): string {
    const identity = platform === 'win32' ? canonicalRoot.toLowerCase() : canonicalRoot;
    return 'project-' + createHash('sha256').update(identity).digest('hex').slice(0, 24);
}

export function launchArgs(spec: LaunchSpec, command = 'mcp'): string[] {
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

export function configFor(client: Client, spec: LaunchSpec): string {
    const key = 'airk-' + spec.collection;
    const args = launchArgs(spec);
    if (client === 'amazon-q' || client === 'windsurf') {
        // Connection details only: these clients' agent/version-specific schemas differ.
        return JSON.stringify({ transport: 'stdio', command: spec.command, args }, null, 2) + '\n';
    }
    if (client === 'codex') {
        return `[mcp_servers.${key}]\ncommand = ${JSON.stringify(spec.command)}\nargs = ${JSON.stringify(args)}\n`;
    }
    const server: Record<string, unknown> = { command: spec.command, args };
    let config: unknown;
    if (client === 'vscode') { config = { servers: { [key]: { type: 'stdio', ...server } } }; }
    else if (client === 'opencode' || client === 'opencode-v2' || client === 'kilo') {
        const entry = { type: 'local', command: [spec.command, ...args] };
        config = client === 'opencode-v2' ? { mcp: { servers: { [key]: entry } } } : { mcp: { [key]: entry } };
    } else {
        if (client === 'cursor') { server.type = 'stdio'; }
        if (client === 'copilot-cli') { server.type = 'local'; server.tools = ['*']; }
        config = { mcpServers: { [key]: server } };
    }
    return JSON.stringify(config, null, 2) + '\n';
}
