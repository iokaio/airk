// SPDX-License-Identifier: Apache-2.0
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { getNodeValue, Node, parseTree, ParseError } from 'jsonc-parser';
import { readJsonConfig } from './agentConfig';
import { Client, LaunchSpec } from './model';

type Scope = 'project' | 'user';
interface Candidate { client: Client; scope: Scope; configPath: string; base: string; format: 'json' | 'toml'; servers?: string[] }
export interface AgentConfigurationEntry {
    client: Client; scope: Scope; configPath: string; serverName: string;
    helperPath?: string; helperVersion?: string;
    status: 'matches' | 'different_helper' | 'different_binding' | 'unresolved'; issue?: string;
}
export interface AgentConfigurationReport {
    checkedAt: string; state: 'checked' | 'partial' | 'not_checked'; scope: 'known_project_files_and_codex_user';
    filesChecked: number; filesPresent: number; entries: AgentConfigurationEntry[];
    issues: { client: Client; scope: Scope; configPath: string; reason: string }[];
    truncated: boolean; connection: 'not_verified';
}
export interface AgentConfigurationOptions { codexHome?: string; shouldContinue?: () => boolean }
const MAX_ENTRIES = 32, MAX_SERVERS = 256;
const object = (value: unknown): Record<string, unknown> | undefined => value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : undefined;
const samePath = (left: string, right: string) => {
    const normalize = (value: string) => process.platform === 'win32' ? path.toNamespacedPath(path.resolve(value)).toLowerCase() : path.resolve(value);
    return normalize(left) === normalize(right);
};
const bounded = (value: string, length = 4096) => value.length <= length ? value : value.slice(0, length) + '[truncated]';

function candidates(root: string, codexHome: string): Candidate[] {
    const project: [Client, string, string[]][] = [
        ['vscode', '.vscode/mcp.json', ['servers']], ['claude', '.mcp.json', ['mcpServers']],
        ['cursor', '.cursor/mcp.json', ['mcpServers']], ['gemini', '.gemini/settings.json', ['mcpServers']],
        ['antigravity', '.agents/mcp_config.json', ['mcpServers']], ['roo', '.roo/mcp.json', ['mcpServers']],
        ['junie', '.junie/mcp/mcp.json', ['mcpServers']], ['opencode', 'opencode.json', ['mcp']],
        ['opencode', 'opencode.jsonc', ['mcp']], ['kilo', 'kilo.json', ['mcp']]
    ];
    return [...project.map(([client, file, servers]): Candidate => ({ client, scope: 'project', base: root,
        configPath: path.join(root, ...file.split('/')), format: 'json', servers })),
        { client: 'codex', scope: 'project', base: root, configPath: path.join(root, '.codex', 'config.toml'), format: 'toml' },
        { client: 'codex', scope: 'user', base: codexHome, configPath: path.join(codexHome, 'config.toml'), format: 'toml' }];
}

/** Recognizes the bounded table/string/array syntax emitted by configFor.
 * Unsupported TOML is reported as partial instead of guessed or rewritten. */
function codexEntries(source: string): { entries: [string, unknown][]; partial: boolean } {
    const entries: [string, unknown][] = [];
    let table: Record<string, unknown> | undefined, partial = false, pending = '';
    const seen = new Set<string>();
    const string = (value: string): string | undefined => {
        if (/^'[^'\r\n]*'$/.test(value)) { return value.slice(1, -1); }
        try { const parsed: unknown = JSON.parse(value); return typeof parsed === 'string' ? parsed : undefined; } catch { return undefined; }
    };
    for (const line of source.split(/\r?\n/u)) {
        const trimmed = line.trim();
        if (!pending && (!trimmed || trimmed.startsWith('#'))) { continue; }
        if (!pending && trimmed.startsWith('[')) {
            table = undefined;
            const header = /^\[mcp_servers\.([A-Za-z0-9_-]+|"(?:[^"\\]|\\.)*"|'[^']*')\]\s*(?:#.*)?$/u.exec(trimmed);
            if (!header) { if (/^\[\[?\s*(?:["']?mcp_servers)/u.test(trimmed)) { partial = true; } continue; }
            const name = header[1].startsWith('"') || header[1].startsWith("'") ? string(header[1]) : header[1];
            if (!name || seen.has(name)) {
                partial = true;
                const duplicate = entries.find(entry => entry[0] === name); if (duplicate) { duplicate[1] = {}; }
                continue;
            }
            seen.add(name);
            if (entries.length >= MAX_SERVERS) { partial = true; break; }
            table = {}; entries.push([name, table]); continue;
        }
        if (!table) { if (/^(?:mcp_servers|["']mcp_servers["'])\s*=/u.test(trimmed)) { partial = true; } continue; }
        const assignment = /^(command|args|url|serverUrl|type)\s*=\s*([\s\S]*)$/u.exec(pending ? pending + '\n' + line : trimmed);
        if (!assignment) {
            if (/^(?:["']?(?:command|args|url|serverUrl|type))\b/u.test(trimmed)) { partial = true; table.command = undefined; }
            continue;
        }
        const [, key, raw] = assignment;
        if (Object.hasOwn(table, key)) { partial = true; table[key] = undefined; continue; }
        let value: unknown;
        if (key !== 'args') { value = string(raw.trim()); }
        else {
            try { value = JSON.parse(raw); } catch {
                if (!raw.trimEnd().endsWith(']') && pending.length + line.length < 65536) { pending = assignment[0]; continue; }
            }
        }
        pending = '';
        if (value === undefined || key === 'args' && (!Array.isArray(value) || !value.every(item => typeof item === 'string'))) { partial = true; }
        table[key] = value;
    }
    return { entries, partial: partial || !!pending };
}

function jsonEntries(source: string, keys: string[]): { entries: [string, unknown][]; partial: boolean } {
    const errors: ParseError[] = [];
    const tree = parseTree(source, errors, { allowTrailingComma: true });
    if (errors.length || !tree || tree.type !== 'object') { return { entries: [], partial: true }; }
    const pending: Node[] = [tree];
    while (pending.length) {
        const node = pending.pop()!;
        if (node.type === 'object') {
            const names = node.children!.map(property => property.children![0].value);
            if (new Set(names).size !== names.length) { return { entries: [], partial: true }; }
        }
        for (const child of node.children ?? []) { pending.push(child); }
    }
    let value: unknown = getNodeValue(tree);
    for (const key of keys) { value = object(value)?.[key]; }
    // Both existing OpenCode schemas have been emitted by the extension.
    if (keys[0] === 'mcp' && object(object(value)?.servers)) { value = object(value)?.servers; }
    if (value === undefined) { return { entries: [], partial: false }; }
    const servers = object(value);
    if (!servers) { return { entries: [], partial: true }; }
    const entries = Object.entries(servers);
    return { entries: entries.slice(0, MAX_SERVERS), partial: entries.length > MAX_SERVERS };
}

function entryFor(spec: LaunchSpec, candidate: Candidate, serverName: string, value: unknown): AgentConfigurationEntry | undefined {
    const server = object(value);
    const command = Array.isArray(server?.command) ? server.command[0] : server?.command;
    const args: unknown = Array.isArray(server?.command) ? server.command.slice(1) : server?.args;
    const strings = Array.isArray(args) && args.length <= 64 && args.every(arg => typeof arg === 'string' && arg.length <= 32768) ? args as string[] : undefined;
    const flags = new Map<string, string>(); let ambiguous = false;
    if (strings) {
        for (let i = 0; i < strings.length; i++) {
            const flag = strings[i];
            if (['--root', '--data-dir', '--collection'].includes(flag)) {
                if (flags.has(flag) || i + 1 >= strings.length) { ambiguous = true; break; }
                flags.set(flag, strings[++i]);
            }
        }
    }
    const root = flags.get('--root'), collection = flags.get('--collection'), dataDir = flags.get('--data-dir');
    const named = serverName === `airk-${spec.collection}`;
    const airk = typeof command === 'string' && /^airk(?:\.exe)?$/iu.test(path.basename(command));
    if (!named && !(airk && collection === spec.collection && root && path.isAbsolute(root) && samePath(root, spec.root))) { return; }
    const entry: AgentConfigurationEntry = { client: candidate.client, scope: candidate.scope, configPath: candidate.configPath,
        serverName: bounded(serverName, 256), status: 'unresolved' };
    if (typeof command === 'string') {
        entry.helperPath = bounded(command);
        entry.helperVersion = /(?:^|[\\/])(\d+\.\d+\.\d+)-(?:win32|linux|darwin)-/u.exec(command)?.[1];
    }
    if (!strings || strings.length !== 7 || ambiguous || typeof command !== 'string' || !path.isAbsolute(command)
        || !root || !dataDir || !collection || !path.isAbsolute(root) || !path.isAbsolute(dataDir) || strings.at(-1) !== 'mcp') {
        entry.issue = 'The saved entry is not a complete absolute AIRK launch binding. Review it in the agent configuration.';
    } else if (Object.hasOwn(server!, 'url') || Object.hasOwn(server!, 'serverUrl') || server?.type !== undefined && !['stdio', 'local'].includes(String(server.type))) {
        entry.issue = 'The saved transport is not an unambiguous local stdio helper launch. Review it in the agent configuration.';
    } else if (!samePath(root, spec.root) || !samePath(dataDir, spec.dataDir) || collection !== spec.collection) {
        entry.status = 'different_binding'; entry.issue = 'The saved entry selects a different workspace, storage location, or collection.';
    } else if (!samePath(command, spec.command)) {
        entry.status = 'different_helper'; entry.issue = 'The agent is configured to launch a different helper path. Review the current connection configuration, then reload the agent.';
    } else { entry.status = 'matches'; }
    return entry;
}

async function readCandidate(candidate: Candidate): Promise<string | undefined> {
    const base = await fs.realpath(candidate.base);
    const relative = path.relative(candidate.base, candidate.configPath);
    if (relative.startsWith('..') || path.isAbsolute(relative)) { throw new Error('Configuration is outside the inspected directory.'); }
    const components = relative.split(path.sep); let directory = base;
    for (const component of components.slice(0, -1)) {
        directory = path.join(directory, component);
        const info = await fs.lstat(directory);
        if (!info.isDirectory() || info.isSymbolicLink()) { throw new Error('Redirected configuration directories are not inspected.'); }
    }
    return readJsonConfig(path.join(base, relative));
}

/** Read-only: never launches configured commands, copies unrelated arguments or edits agent settings. */
export async function inspectAgentConfiguration(spec: LaunchSpec, options: AgentConfigurationOptions = {}): Promise<AgentConfigurationReport> {
    const report: AgentConfigurationReport = { checkedAt: new Date().toISOString(), state: 'not_checked', scope: 'known_project_files_and_codex_user',
        filesChecked: 0, filesPresent: 0, entries: [], issues: [], truncated: false, connection: 'not_verified' };
    if (options.shouldContinue?.() === false) { return report; }
    let root: string;
    try { root = await fs.realpath(spec.root); } catch { return report; }
    const codexHome = path.resolve(options.codexHome ?? process.env.CODEX_HOME ?? path.join(os.homedir(), '.codex'));
    for (const candidate of candidates(root, codexHome)) {
        if (options.shouldContinue?.() === false) { report.state = report.filesChecked ? 'partial' : 'not_checked'; return report; }
        report.filesChecked++;
        try {
            const source = await readCandidate(candidate);
            if (options.shouldContinue?.() === false) { report.state = 'partial'; return report; }
            if (source === undefined) { continue; }
            report.filesPresent++;
            const parsed = candidate.format === 'toml' ? codexEntries(source) : jsonEntries(source, candidate.servers!);
            if (parsed.partial) { report.issues.push({ client: candidate.client, scope: candidate.scope, configPath: candidate.configPath, reason: 'Unsupported, ambiguous, invalid, or oversized configuration syntax; this file was only partially inspected.' }); }
            for (const [name, value] of parsed.entries) {
                const entry = entryFor(spec, candidate, name, value);
                if (!entry) { continue; }
                if (report.entries.length >= MAX_ENTRIES) { report.truncated = true; break; }
                report.entries.push(entry);
            }
        } catch (error) {
            if ((error as NodeJS.ErrnoException).code === 'ENOENT') { continue; }
            report.issues.push({ client: candidate.client, scope: candidate.scope, configPath: candidate.configPath,
                reason: 'Configuration could not be safely read as a regular, unlinked UTF-8 file of at most 1 MiB.' });
        }
    }
    report.state = report.issues.length || report.truncated ? 'partial' : 'checked';
    return report;
}
