// SPDX-License-Identifier: Apache-2.0
//! The extension's removal-only recognizer for the no-service case (plan
//! §6.4, §8.4; U12, U92). It recognizes exactly what `airkctl agents`
//! recognizes (crates/airkctl/src/agents/config/recognize.rs) and removes an
//! entry exactly as `agents remove` does (edit.rs `remove`), and is tested
//! against the shared cases in fixtures/split/agents/config_cases.json. It
//! never writes, repairs or enrolls: with a service, `airkctl agents …` owns
//! every edit.
import * as jsonc from './jsonc';
import * as toml from './toml';

export type Agent = 'codex' | 'claude' | 'copilot' | 'gemini' | 'antigravity' | 'cursor' | 'roo' | 'junie';
export type Scope = 'project' | 'user';
type Json = Record<string, unknown>;
export interface Declaration { serverKey: string; container: string[]; fields: Json }
export interface Target { containerPath: string[]; serverKey: string }
export type Recognized =
    | { kind: 'stdio'; command: string; root?: string; collection?: string }
    | { kind: 'https'; id: string; url: string; helper?: string; proven: boolean };

/** Project-relative files, in the native order (locations.rs PROJECT_FILES). */
export const PROJECT_FILES: readonly (readonly [Agent, readonly string[]])[] = [
    ['codex', ['.codex', 'config.toml']], ['claude', ['.mcp.json']], ['copilot', ['.vscode', 'mcp.json']],
    ['gemini', ['.gemini', 'settings.json']], ['antigravity', ['.agents', 'mcp_config.json']], ['cursor', ['.cursor', 'mcp.json']],
    ['roo', ['.roo', 'mcp.json']], ['junie', ['.junie', 'mcp', 'mcp.json']],
];
export const MAX_BYTES = 1_048_576;
const MAX_DECLARATIONS = 512;
const HELPER_MIDDLE = ' agents credential --connection-id ';
const HELPER_END = ' --format header-json';
const INCARNATION = /^[0-9a-f]{32}$/;

const parser = (agent: Agent) => agent === 'codex' ? 'codex' : agent === 'copilot' ? 'copilot' : 'claude';
export const containerOf = (agent: Agent) => agent === 'codex' ? 'mcp_servers' : agent === 'copilot' ? 'servers' : 'mcpServers';
const isObject = (value: unknown): value is Json => !!value && typeof value === 'object' && !Array.isArray(value);
const compareLists = (a: string[], b: string[]) => {
    for (let index = 0; index < Math.min(a.length, b.length); index++) { if (a[index] !== b[index]) { return a[index] < b[index] ? -1 : 1; } }
    return a.length - b.length;
};

/** Every server declaration, sorted by container and key. */
export function declarations(agent: Agent, source: string): Declaration[] {
    if (Buffer.byteLength(source, 'utf8') > MAX_BYTES) { throw new Error('configuration_size_bound'); }
    let rows: Declaration[];
    if (parser(agent) === 'codex') { rows = toml.declarations(source); }
    else {
        const root = jsonc.valueOf(jsonc.parse(source)) as Json;
        rows = [];
        const group = (container: string[], raw: unknown) => {
            if (raw === undefined) { return; }
            if (!isObject(raw)) { throw new Error('invalid_mcp_server_group'); }
            for (const [key, fields] of Object.entries(raw)) {
                if (!isObject(fields)) { throw new Error('invalid_mcp_server_declaration'); }
                rows.push({ serverKey: key, container, fields });
                if (rows.length > MAX_DECLARATIONS) { throw new Error('configuration_declaration_bound'); }
            }
        };
        if (parser(agent) === 'copilot') { group(['servers'], root.servers); }
        else {
            group(['mcpServers'], root.mcpServers);
            if (root.projects !== undefined) {
                if (!isObject(root.projects)) { throw new Error('invalid_claude_projects'); }
                for (const [project, settings] of Object.entries(root.projects)) {
                    if (!isObject(settings)) { throw new Error('invalid_claude_project'); }
                    group(['projects', project, 'mcpServers'], settings.mcpServers);
                }
            }
        }
    }
    return rows.sort((a, b) => compareLists(a.container, b.container) || (a.serverKey < b.serverKey ? -1 : a.serverKey > b.serverKey ? 1 : 0));
}

export const airkKey = (key: string) => key === 'airk' || key.startsWith('airk-') || key.startsWith('airk_');
/** Exactly `https://127.0.0.1:<port>/mcp`. */
export function httpUrl(value: string): boolean {
    const match = /^https:\/\/127\.0\.0\.1:([1-9][0-9]{0,4})\/mcp$/.exec(value);
    return !!match && Number(match[1]) <= 65535;
}
export function isAbsolute(value: string): boolean {
    return value.startsWith('/') || value.startsWith('\\\\') || /^[A-Za-z]:[\\/]/.test(value);
}
export function plainPath(value: string): string {
    if (value.slice(0, 8).toUpperCase() === '\\\\?\\UNC\\') { return '\\\\' + value.slice(8); }
    if (value.startsWith('\\\\?\\') && /^[A-Za-z]:/.test(value.slice(4, 6))) { return value.slice(4); }
    return value;
}
export function samePath(left: string, right: string, windows: boolean): boolean {
    const normal = (value: string) => { const plain = plainPath(value); return windows ? plain.replaceAll('/', '\\').toLowerCase() : plain; };
    return normal(left) === normal(right);
}
const isAirkctl = (file: string) => { const name = file.split(/[/\\]/).pop() ?? file; return name === 'airkctl' || name.toLowerCase() === 'airkctl.exe'; };

/** The executable and connection of a U92 helper, or undefined for any other command. */
export function helperConnection(command: string): { executable: string; id: string } | undefined {
    if (command.length > 4096) { return undefined; }
    let executable: string, rest: string;
    if (command.startsWith('"')) {
        const end = command.indexOf('"', 1);
        if (end < 0) { return undefined; }
        executable = command.slice(1, end); rest = command.slice(end + 1);
    } else {
        const end = command.search(/\s/);
        executable = end < 0 ? command : command.slice(0, end); rest = end < 0 ? '' : command.slice(end);
    }
    if (!executable || executable.includes('"') || !rest.startsWith(HELPER_MIDDLE) || !rest.endsWith(HELPER_END)) { return undefined; }
    const id = rest.slice(HELPER_MIDDLE.length, rest.length - HELPER_END.length);
    if (!INCARNATION.test(id)) { return undefined; }
    return isAbsolute(executable) && isAirkctl(executable) ? { executable, id } : undefined;
}
function envReference(value: string): string | undefined {
    const match = /^Bearer \$\{(?:env:)?AIRK_MCP_TOKEN_([0-9A-F]{32})\}$/.exec(value);
    return match ? match[1].toLowerCase() : undefined;
}
export function inputReference(value: string): string | undefined {
    const match = /^Bearer \$\{input:airkToken_([0-9a-f]{32})\}$/.exec(value);
    return match?.[1];
}
function stdioCommand(fields: Json): string | undefined {
    if (typeof fields.command !== 'string' || Object.hasOwn(fields, 'url') || Object.hasOwn(fields, 'serverUrl')) { return undefined; }
    if (fields.type !== undefined && fields.type !== 'stdio' && fields.type !== 'local') { return undefined; }
    return fields.command;
}
function argument(fields: Json, flag: string): string | undefined {
    if (!Array.isArray(fields.args)) { return undefined; }
    const first = fields.args.indexOf(flag), last = fields.args.lastIndexOf(flag);
    if (first < 0 || first !== last) { return undefined; }
    const value = fields.args[first + 1];
    return typeof value === 'string' ? value : undefined;
}
function header(fields: Json, name: string): string | undefined | null {
    if (fields.headers === undefined) { return undefined; }
    if (!isObject(fields.headers)) { return null; }
    const value = fields.headers[name];
    if (value === undefined) { return undefined; }
    return typeof value === 'string' ? value : null;
}

/** Whether `row` is an AIRK entry (recognize.rs `recognize`). */
export function recognize(agent: Agent, scope: Scope, row: Declaration, verified: readonly string[] = [], windows = false): Recognized | undefined {
    const fields = row.fields, keyed = airkKey(row.serverKey);
    const command = stdioCommand(fields);
    if (command !== undefined) {
        const owned = isAbsolute(plainPath(command)) && verified.some(launcher => samePath(launcher, command, windows));
        if (!owned && !(scope === 'project' && keyed)) { return undefined; }
        const root = argument(fields, '--root'), plainRoot = root === undefined ? undefined : plainPath(root);
        const collection = argument(fields, '--collection');
        return {
            kind: 'stdio', command,
            ...(plainRoot !== undefined && plainRoot.length <= 4096 && isAbsolute(plainRoot) && !/[\u0000-\u001f\u007f]/.test(plainRoot) ? { root: plainRoot } : {}),
            ...(collection && collection.length <= 128 && /^[A-Za-z0-9._-]+$/.test(collection) && collection !== '.' && collection !== '..' ? { collection } : {}),
        };
    }
    if (typeof fields.url !== 'string' || !httpUrl(fields.url)) { return undefined; }
    let id: string, helper: string | undefined;
    switch (parser(agent)) {
        case 'codex': {
            if (Object.hasOwn(fields, 'bearer_token_env_var') || typeof fields.http_headers_helper !== 'string') { return undefined; }
            const found = helperConnection(fields.http_headers_helper);
            if (!found) { return undefined; }
            ({ id, executable: helper } = found);
            break;
        }
        case 'claude': {
            if (Object.hasOwn(fields, 'command') || typeof fields.headersHelper !== 'string') { return undefined; }
            const found = helperConnection(fields.headersHelper);
            if (!found) { return undefined; }
            const authorization = header(fields, 'Authorization');
            if (authorization === null) { return undefined; }
            if (authorization !== undefined && envReference(authorization) !== found.id) { return undefined; }
            ({ id, executable: helper } = found);
            break;
        }
        default: {
            if (Object.hasOwn(fields, 'command') || Object.hasOwn(fields, 'args')) { return undefined; }
            const authorization = header(fields, 'Authorization');
            const reference = typeof authorization === 'string' ? inputReference(authorization) : undefined;
            if (!reference) { return undefined; }
            id = reference;
        }
    }
    const proven = helper !== undefined;
    if (scope === 'project' && !keyed && !proven) { return undefined; }
    return { kind: 'https', id, url: fields.url, ...(helper ? { helper } : {}), proven };
}

function find(rows: Declaration[], target: Target): Declaration | undefined {
    return rows.find(row => row.serverKey === target.serverKey && row.container.length === target.containerPath.length
        && row.container.every((part, index) => part === target.containerPath[index]));
}
function owned(agent: Agent, scope: Scope, source: string, target: Target): Recognized {
    const row = find(declarations(agent, source), target);
    if (!row) { throw new Error(`No server ${target.serverKey} in ${target.containerPath.join('.')}.`); }
    if (scope === 'project' && !(row.container.length === 1 && row.container[0] === containerOf(agent))) {
        throw new Error(`AIRK edits project servers only under ${containerOf(agent)}.`);
    }
    const recognized = recognize(agent, scope, row, [], false);
    if (!recognized) { throw new Error(`The server ${target.serverKey} is not an entry AIRK wrote; nothing was changed. Review it manually.`); }
    return recognized;
}

function references(value: unknown, marker: string): number {
    const count = (text: string) => text.split(marker).length - 1;
    if (typeof value === 'string') { return count(value); }
    if (Array.isArray(value)) { return value.reduce<number>((sum, item) => sum + references(item, marker), 0); }
    if (isObject(value)) { return Object.entries(value).reduce<number>((sum, [key, item]) => sum + count(key) + references(item, marker), 0); }
    return 0;
}
const VSCODE_COMMAND = 'airk.httpCredential';
function inputFor(id: string): Json {
    return { id: `airkToken_${id}`, type: 'command', command: VSCODE_COMMAND, args: { connection_id: id } };
}
function deepEqual(a: unknown, b: unknown): boolean {
    if (Array.isArray(a) || Array.isArray(b)) {
        return Array.isArray(a) && Array.isArray(b) && a.length === b.length && a.every((item, index) => deepEqual(item, b[index]));
    }
    if (isObject(a) && isObject(b)) {
        const keys = Object.keys(a);
        return keys.length === Object.keys(b).length && keys.every(key => Object.hasOwn(b, key) && deepEqual(a[key], b[key]));
    }
    return a === b;
}
/** edit.rs `sync_input(…, remove = true)`: drop the connection's VS Code
 * input once nothing references it; an ambiguous or conflicting array is refused. */
function removeInput(source: string, id: string): string {
    const tree = jsonc.parse(source);
    const node = jsonc.at(tree, ['inputs']);
    let items: jsonc.Node[] = [];
    if (node) {
        if (node.kind.type !== 'array') { throw new Error('VS Code inputs must be an array; existing configuration preserved.'); }
        items = node.kind.items;
    }
    const wanted = `airkToken_${id}`, seen = new Set<string>();
    let existing: { index: number; value: unknown } | undefined;
    items.forEach((item, index) => {
        const value = jsonc.valueOf(item);
        const name = isObject(value) && typeof value.id === 'string' ? value.id : undefined;
        if (name === undefined || seen.has(name)) { throw new Error('Ambiguous VS Code inputs; existing configuration preserved.'); }
        seen.add(name);
        if (name === wanted) { existing = { index, value }; }
    });
    if (existing && !deepEqual(existing.value, inputFor(id))) { throw new Error('Conflicting AIRK credential input; existing configuration preserved.'); }
    const count = references(jsonc.valueOf(tree), `\${input:${wanted}}`);
    return existing && count === 0 ? jsonc.removeElement(source, ['inputs'], existing.index) : source;
}

/** Delete a recognized entry (HTTPS or retired stdio) and, for VS Code, its
 * credential input once nothing references it (edit.rs `remove`). */
export function remove(agent: Agent, scope: Scope, source: string, target: Target): string {
    const recognized = owned(agent, scope, source, target);
    if (parser(agent) === 'codex') {
        if (!(target.containerPath.length === 1 && target.containerPath[0] === 'mcp_servers')) {
            throw new Error('AIRK edits Codex servers only in the top-level mcp_servers table.');
        }
        return toml.removeServer(source, target.serverKey);
    }
    const result = jsonc.removeMember(source, target.containerPath, target.serverKey);
    return recognized.kind === 'https' && agent === 'copilot' ? removeInput(result, recognized.id) : result;
}

/** A recognized entry the no-service sweep may remove: project entries only
 * (user-level entries need the service's confirmation, §6.4). */
export interface Removal { agent: Agent; target: Target; recognized: Recognized }
export function removableEntries(agent: Agent, source: string): Removal[] {
    return declarations(agent, source).flatMap(row => {
        if (!(row.container.length === 1 && row.container[0] === containerOf(agent))) { return []; }
        const recognized = recognize(agent, 'project', row);
        return recognized ? [{ agent, target: { containerPath: row.container, serverKey: row.serverKey }, recognized }] : [];
    });
}
/** Remove every removable entry of one project file; returns the new text and what was removed. */
export function removeAll(agent: Agent, source: string, select: (entry: Removal) => boolean = () => true): { text: string; removed: Removal[] } {
    let text = source;
    const removed: Removal[] = [];
    for (const entry of removableEntries(agent, source).filter(select)) {
        text = remove(agent, 'project', text, entry.target);
        removed.push(entry);
    }
    return { text, removed };
}
