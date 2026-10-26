// SPDX-License-Identifier: Apache-2.0
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { applyEdits, findNodeAtLocation, getNodeValue, modify, parseTree, ParseError, Node } from 'jsonc-parser';
import { LaunchSpec, launchArgs } from './model';
import { readJsonConfig } from './agentConfig';

export type LocalAgent = 'codex' | 'claude' | 'copilot';
const locations: Record<LocalAgent, string[]> = { codex: ['.codex', 'config.toml'], claude: ['.mcp.json'], copilot: ['.vscode', 'mcp.json'] };
function same(left: string, right: string): boolean {
    return path.relative(process.platform === 'win32' ? path.toNamespacedPath(left) : left,
        process.platform === 'win32' ? path.toNamespacedPath(right) : right) === '';
}

/** Accept only extension-owned legacy launches or this installation's stable launcher. Never execute the old command. */
function recognized(spec: LaunchSpec, storage: string, command: unknown, args: unknown): boolean {
    if (!spec.service || typeof command !== 'string' || !path.isAbsolute(command) || !Array.isArray(args)
        || !args.every(value => typeof value === 'string')) { return false; }
    const relative = path.relative(storage, command).split(path.sep);
    const legacyCommand = relative.join('/') === `cli/bin/${process.platform === 'win32' ? 'airk.exe' : 'airk'}`
        || relative.length === 3 && relative[0] === 'helpers' && /^[A-Za-z0-9._-]+$/.test(relative[1])
            && ['airk', 'airk.exe'].includes(relative[2]);
    if (legacyCommand && args.length === 7 && args[0] === '--root' && same(args[1], spec.root)
        && args[2] === '--data-dir' && path.isAbsolute(args[3]) && args[4] === '--collection'
        && args[5] === spec.collection && args[6] === 'mcp') { return true; }
    // Already migrated entries may need the current data location, but must retain the same native binding.
    return same(command, spec.command) && args.length === 14
        && args[0] === 'runtime' && args[1] === 'launch' && args[2] === '--install-dir' && same(args[3], spec.service.installDir)
        && args[4] === '--' && args[5] === '--root' && same(args[6], spec.root)
        && args[7] === '--data-dir' && path.isAbsolute(args[8]) && args[9] === '--collection' && args[10] === spec.collection
        && args[11] === '--service-project' && args[12] === spec.service.project && args[13] === 'mcp';
}

function unique(node: Node): void {
    if (node.type === 'object') {
        const keys = node.children!.map(property => property.children![0].value);
        if (new Set(keys).size !== keys.length) { throw new Error('Duplicate configuration keys; existing file preserved.'); }
    }
    node.children?.forEach(unique);
}

/** Update connection fields only. Missing entries never opt an agent in. Unsupported syntax is preserved. */
export function migrateAgentConfig(agent: LocalAgent, spec: LaunchSpec, storage: string, source: string): string {
    const name = `airk-${spec.collection}`;
    if (!spec.service) { throw new Error('Registered service binding required.'); }
    if (Buffer.byteLength(source) > 1_048_576) { throw new Error('Configuration exceeds 1 MiB.'); }
    if (agent !== 'codex') {
        const errors: ParseError[] = [], tree = parseTree(source, errors, { allowTrailingComma: true });
        if (!tree || tree.type !== 'object' || errors.length) { throw new Error('Invalid MCP JSON; existing file preserved.'); }
        unique(tree);
        const keys = [agent === 'copilot' ? 'servers' : 'mcpServers', name];
        const node = findNodeAtLocation(tree, keys);
        if (!node) { return source; }
        const entry = getNodeValue(node);
        if (node.type !== 'object' || 'url' in entry || 'serverUrl' in entry || entry.type && entry.type !== 'stdio'
            || !recognized(spec, storage, entry.command, entry.args)) { throw new Error('Unrecognized AIRK connection; review manually.'); }
        let result = source;
        for (const [key, value] of [['command', spec.command], ['args', launchArgs(spec)]] as const) {
            result = applyEdits(result, modify(result, [...keys, key], value, {}));
        }
        return result;
    }
    // Restrict automatic TOML edits to the generated, line-oriented table format.
    // Multiline strings could disguise table headers, so defer those files to manual review.
    if (source.includes('"""') || source.includes("'''")) { throw new Error('Multiline TOML strings require manual review.'); }
    const headers = [...source.matchAll(/^\s*\[([^\]\r\n]+)\][ \t]*(?:#[^\r\n]*)?\r?$/gm)];
    const target = headers.filter(header => header[1] === `mcp_servers.${name}` || header[1] === `mcp_servers."${name}"`);
    if (!target.length) { return source; }
    if (target.length !== 1) { throw new Error('Duplicate AIRK TOML table; review manually.'); }
    const header = target[0], start = header.index! + header[0].length;
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
    if (!recognized(spec, storage, command, args)) { throw new Error('Unrecognized AIRK connection; review manually.'); }
    let result = body;
    for (const field of [commands[0], arguments_[0]].sort((a, b) => b.index! - a.index!)) {
        const value = field[1] === 'command' ? spec.command : launchArgs(spec);
        result = result.slice(0, field.index!) + `${field[1]} = ${JSON.stringify(value)}` + result.slice(field.index! + field[0].length);
    }
    return source.slice(0, start) + result + source.slice(end);
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
