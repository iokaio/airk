// SPDX-License-Identifier: Apache-2.0
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { applyEdits, findNodeAtLocation, getNodeValue, modify, parseTree, ParseError, Node } from 'jsonc-parser';
import { ASSISTANCE_KEY, assistanceEntry, LaunchSpec, retrievalEntry, ServerEntry } from './model';
import { launchDescriptorPath } from './explorerAssistanceLaunch';
import { readJsonConfig } from './agentConfig';

export type LocalAgent = 'codex' | 'claude' | 'copilot';
const locations: Record<LocalAgent, string[]> = { codex: ['.codex', 'config.toml'], claude: ['.mcp.json'], copilot: ['.vscode', 'mcp.json'] };
function same(left: string, right: string): boolean {
    return path.relative(process.platform === 'win32' ? path.toNamespacedPath(left) : left,
        process.platform === 'win32' ? path.toNamespacedPath(right) : right) === '';
}

/** The per-installation stable launcher, which earlier releases wrote into agent entries. */
function installLauncher(spec: LaunchSpec): string | undefined {
    return spec.service && path.join(spec.service.installDir, process.platform === 'win32' ? 'airk-launcher.exe' : 'airk-launcher');
}
const strings = (args: unknown): args is string[] => Array.isArray(args) && args.every(value => typeof value === 'string');
/** This installation's launchers: the current command, its per-installation launcher, or the universal launcher. */
function ownLauncher(spec: LaunchSpec, command: unknown): boolean {
    return typeof command === 'string' && path.isAbsolute(command) && (same(command, spec.command) || same(command, installLauncher(spec)!));
}
/** `runtime launch --install-dir <this install> -- …rest`, or just `…rest` for the universal launcher. */
function forwarded(spec: LaunchSpec, args: string[]): string[] | undefined {
    if (args[0] !== 'runtime') { return args; }
    return args.length > 5 && args[1] === 'launch' && args[2] === '--install-dir' && same(args[3], spec.service!.installDir) && args[4] === '--' ? args.slice(5) : undefined;
}

/** Accept only extension-owned legacy launches or this installation's launchers. Never execute the old command. */
function recognized(spec: LaunchSpec, storage: string, command: unknown, args: unknown): boolean {
    if (!spec.service || typeof command !== 'string' || !path.isAbsolute(command) || !strings(args)) { return false; }
    const relative = path.relative(storage, command).split(path.sep);
    const legacyCommand = relative.join('/') === `cli/bin/${process.platform === 'win32' ? 'airk.exe' : 'airk'}`
        || relative.length === 3 && relative[0] === 'helpers' && /^[A-Za-z0-9._-]+$/.test(relative[1])
            && ['airk', 'airk.exe'].includes(relative[2]);
    if (legacyCommand && args.length === 7 && args[0] === '--root' && same(args[1], spec.root)
        && args[2] === '--data-dir' && path.isAbsolute(args[3]) && args[4] === '--collection'
        && args[5] === spec.collection && args[6] === 'mcp') { return true; }
    // Already migrated entries may need the current data location or launcher, but must retain the same native binding.
    const rest = ownLauncher(spec, command) ? forwarded(spec, args) : undefined;
    return !!rest && rest.length === 9 && rest[0] === '--root' && same(rest[1], spec.root)
        && rest[2] === '--data-dir' && path.isAbsolute(rest[3]) && rest[4] === '--collection' && rest[5] === spec.collection
        && rest[6] === '--service-project' && rest[7] === spec.service.project && rest[8] === 'mcp';
}

/** An Explorer Assistance entry this extension wrote: this installation's launcher and its launch descriptor. */
function recognizedAssistance(spec: LaunchSpec, storage: string, command: unknown, args: unknown): boolean {
    if (!spec.service || !ownLauncher(spec, command) || !strings(args)) { return false; }
    const rest = forwarded(spec, args);
    return !!rest && rest.length === 3 && rest[0] === 'explorer-assistance' && rest[1] === '--descriptor' && same(rest[2], launchDescriptorPath(storage));
}

interface Target { readonly key: string; recognize(command: unknown, args: unknown): boolean; readonly entry: ServerEntry; readonly strict: boolean }

function unique(node: Node): void {
    if (node.type === 'object') {
        const keys = node.children!.map(property => property.children![0].value);
        if (new Set(keys).size !== keys.length) { throw new Error('Duplicate configuration keys; existing file preserved.'); }
    }
    node.children?.forEach(unique);
}

/** Rewrite command/args of one generated, line-oriented TOML table; undefined when the table is absent. */
function migrateToml(source: string, target: Target): string | undefined {
    const headers = [...source.matchAll(/^\s*\[([^\]\r\n]+)\][ \t]*(?:#[^\r\n]*)?\r?$/gm)];
    const matches = headers.filter(header => header[1] === `mcp_servers.${target.key}` || header[1] === `mcp_servers."${target.key}"`);
    if (!matches.length) { return undefined; }
    if (matches.length !== 1) { throw new Error('Duplicate AIRK TOML table; review manually.'); }
    const header = matches[0], start = header.index! + header[0].length;
    const end = headers.find(value => value.index! > header.index!)?.index ?? source.length;
    const body = source.slice(start, end);
    const keys = new Set<string>();
    for (const line of body.split(/\r?\n/)) {
        if (!line.trim() || line.trimStart().startsWith('#')) { continue; }
        const assignment = /^\s*([A-Za-z0-9_-]+)\s*=\s*(.*?)\s*$/.exec(line);
        if (!assignment || keys.has(assignment[1])) { throw new Error('Unsupported or duplicate TOML keys; review manually.'); }
        keys.add(assignment[1]);
        try { JSON.parse(assignment[2]); } catch { throw new Error('Unsupported TOML syntax; review manually.'); }
    }
    const fields = [...body.matchAll(/^[ \t]*(command|args|url|serverUrl|type)[ \t]*=[ \t]*(.*?)[ \t]*\r?$/gm)];
    const commands = fields.filter(value => value[1] === 'command'), arguments_ = fields.filter(value => value[1] === 'args');
    if (commands.length !== 1 || arguments_.length !== 1 || fields.some(value => ['url', 'serverUrl', 'type'].includes(value[1]))) {
        throw new Error('Unsupported AIRK TOML table; review manually.');
    }
    let command: unknown, args: unknown;
    try { command = JSON.parse(commands[0][2]); args = JSON.parse(arguments_[0][2]); }
    catch { throw new Error('Unsupported AIRK TOML launch syntax; review manually.'); }
    if (!target.recognize(command, args)) { throw new Error('Unrecognized AIRK connection; review manually.'); }
    let result = body;
    for (const field of [commands[0], arguments_[0]].sort((a, b) => b.index! - a.index!)) {
        const value = field[1] === 'command' ? target.entry.command : target.entry.args;
        result = result.slice(0, field.index!) + `${field[1]} = ${JSON.stringify(value)}` + result.slice(field.index! + field[0].length);
    }
    return source.slice(0, start) + result + source.slice(end);
}

/**
 * Update connection fields only. Missing entries never opt an agent in. Unsupported syntax is preserved.
 * Retrieval entries must be recognized; an Explorer Assistance entry is updated only when recognized and is
 * otherwise left exactly as it is.
 */
export function migrateAgentConfig(agent: LocalAgent, spec: LaunchSpec, storage: string, source: string): string {
    if (!spec.service) { throw new Error('Registered service binding required.'); }
    if (Buffer.byteLength(source) > 1_048_576) { throw new Error('Configuration exceeds 1 MiB.'); }
    const assistance = assistanceEntry(spec, launchDescriptorPath(storage));
    const targets: Target[] = [
        { key: `airk-${spec.collection}`, recognize: (command, args) => recognized(spec, storage, command, args), entry: retrievalEntry(spec), strict: true },
        ...(assistance ? [{ key: ASSISTANCE_KEY, recognize: (command: unknown, args: unknown) => recognizedAssistance(spec, storage, command, args), entry: assistance, strict: false }] : []),
    ];
    if (agent === 'codex') {
        // Restrict automatic TOML edits to the generated, line-oriented table format.
        // Multiline strings could disguise table headers, so defer those files to manual review.
        if (source.includes('"""') || source.includes("'''")) { throw new Error('Multiline TOML strings require manual review.'); }
        let result = source;
        for (const target of targets) {
            try { result = migrateToml(result, target) ?? result; }
            catch (error) { if (target.strict) { throw error; } }
        }
        return result;
    }
    const errors: ParseError[] = [], tree = parseTree(source, errors, { allowTrailingComma: true });
    if (!tree || tree.type !== 'object' || errors.length) { throw new Error('Invalid MCP JSON; existing file preserved.'); }
    unique(tree);
    let result = source;
    for (const target of targets) {
        const keys = [agent === 'copilot' ? 'servers' : 'mcpServers', target.key];
        const node = findNodeAtLocation(tree, keys);
        if (!node) { continue; }
        const entry = getNodeValue(node);
        if (node.type !== 'object' || 'url' in entry || 'serverUrl' in entry || entry.type && entry.type !== 'stdio'
            || !target.recognize(entry.command, entry.args)) {
            if (target.strict) { throw new Error('Unrecognized AIRK connection; review manually.'); }
            continue;
        }
        for (const [key, value] of [['command', target.entry.command], ['args', target.entry.args]] as const) {
            result = applyEdits(result, modify(result, [...keys, key], value, {}));
        }
    }
    return result;
}

/** Local workspace files only; editor save owns unsaved-edit/version/conflict checks. */
export async function migrateLocalAgents(spec: LaunchSpec, storage: string, check: () => void,
    save: (destination: string, before: string, after: string) => Promise<void>): Promise<string[]> {
    const messages: string[] = [];
    for (const agent of ['codex', 'claude', 'copilot'] as const) {
        check();
        const destination = path.join(spec.root, ...locations[agent]);
        const validate = async () => {
            if (!same(await fs.realpath(spec.root), spec.root)) { throw new Error('Workspace root redirected.'); }
            const directory = path.dirname(destination);
            const info = await fs.lstat(directory);
            if (!info.isDirectory() || info.isSymbolicLink() || !same(await fs.realpath(directory), directory)) {
                throw new Error('Configuration directory redirected.');
            }
            return readJsonConfig(destination);
        };
        try {
            const before = await validate();
            if (before === undefined) { continue; }
            const after = migrateAgentConfig(agent, spec, storage, before);
            if (after === before) { continue; }
            check();
            if (await validate() !== before) { throw new Error('Configuration changed during migration.'); }
            check(); await save(destination, before, after); check();
            messages.push(`Updated ${agent} AIRK connection: ${destination}. Restart the agent to reload MCP.`);
        } catch (error) {
            if ((error as NodeJS.ErrnoException).code !== 'ENOENT') { messages.push(`Could not update ${agent} AIRK connection: ${String(error)} (${destination})`); }
        }
    }
    return messages;
}
