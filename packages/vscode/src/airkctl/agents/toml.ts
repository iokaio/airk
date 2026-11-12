// SPDX-License-Identifier: Apache-2.0
//! Bounded, non-executing TOML statement scan, MCP declaration decoder and
//! `[mcp_servers.<key>]` removal: a port of the parts of
//! crates/airkctl/src/agents/config/toml.rs that removal needs (`scan`,
//! `declarations`, `server_tables`, `remove_server`). Offsets are UTF-16
//! indices on one string, so the span edits equal the Rust byte edits.
const MAX_DECLARATIONS = 512;
const NESTED = ['env', 'http_headers', 'env_http_headers'];
type Json = Record<string, unknown>;

export interface Statement { text: string; start: number; end: number; comments: string[] }
export interface Declaration { serverKey: string; container: string[]; fields: Json }

const fail = (code: string): never => { throw new Error(`unsupported TOML configuration: ${code}`); };
const isSpace = (c: string) => /\s/.test(c);

export function scan(source: string): Statement[] {
    const out: Statement[] = [];
    let text = '', comments: string[] = [], start = 0;
    const finish = (end: number) => {
        if (text.trim() || comments.length) { out.push({ text: text.trim(), start, end, comments }); }
        text = ''; comments = []; start = end;
    };
    let quote: string | undefined, triple = false, escaped = false, comment = false, commentText = '', depth = 0, at = 0;
    while (at < source.length) {
        const c = source[at], next = at + 1;
        if (comment) {
            if (c === '\n') {
                comment = false; comments.push(commentText.trimEnd()); commentText = '';
                if (depth === 0) { finish(next); } else { text += '\n'; }
            } else { commentText += c; }
            at = next; continue;
        }
        if (quote !== undefined) {
            text += c;
            if (escaped) { escaped = false; }
            else if (quote === '"' && c === '\\') { escaped = true; }
            else if (c === quote) {
                if (!triple) { quote = undefined; }
                else if (source.startsWith(quote.repeat(3), at)) {
                    let end = at + 3;
                    while (source[end] === quote) { end++; }
                    if (end - at > 5) { fail('invalid_toml_string'); }
                    text += source.slice(at + 1, end);
                    quote = undefined; triple = false; at = end; continue;
                }
            }
            at = next; continue;
        }
        if (c === '#') { comment = true; commentText = '#'; }
        else if (c === '"' || c === "'") {
            quote = c; triple = source.startsWith(c.repeat(3), at);
            const width = triple ? 3 : 1;
            text += source.slice(at, at + width); at += width; continue;
        } else if (c === '\n' && depth === 0) { finish(next); }
        else {
            if (c === '[' || c === '{') { depth++; }
            else if (c === ']' || c === '}') { if (depth === 0) { fail('invalid_toml_structure'); } depth--; }
            text += c;
        }
        at = next;
    }
    if (quote !== undefined || depth !== 0) { fail('unterminated_toml_structure'); }
    if (comment) { comments.push(commentText.trimEnd()); }
    finish(source.length);
    return out;
}

class Cursor {
    at = 0;
    constructor(readonly text: string) {}
    rest(): string { return this.text.slice(this.at); }
    peek(): string | undefined { return this.at < this.text.length ? this.text[this.at] : undefined; }
    space(): void { const rest = this.rest(); this.at += rest.length - rest.trimStart().length; }
    eat(expected: string): void { if (this.peek() !== expected) { fail('invalid_toml_assignment'); } this.at++; }
    string(): string {
        const quote = this.peek();
        if (quote === undefined) { return fail('invalid_toml_string'); }
        const triple = this.rest().startsWith(quote.repeat(3));
        const closing = triple ? quote.repeat(3) : quote;
        this.at += closing.length;
        if (triple) { if (this.rest().startsWith('\r\n')) { this.at += 2; } else if (this.rest().startsWith('\n')) { this.at += 1; } }
        let value = '';
        for (;;) {
            if (this.rest().startsWith(closing)) {
                if (triple) {
                    let end = this.at + 3;
                    while (this.text[end] === quote) { end++; }
                    if (end - this.at > 5) { fail('invalid_toml_string'); }
                    value += quote.repeat(end - this.at - 3); this.at = end;
                } else { this.at++; }
                return value;
            }
            const c = this.peek();
            if (c === undefined) { return fail('unterminated_toml_string'); }
            this.at++;
            if (c !== '\\' || quote === "'") {
                if (!triple && (c === '\n' || c === '\r')) { fail('invalid_toml_string'); }
                value += c; continue;
            }
            const escape = this.peek();
            if (escape === undefined) { return fail('unterminated_toml_string'); }
            this.at++;
            if (triple && isSpace(escape)) { this.space(); continue; }
            const simple: Record<string, string> = { b: '\b', t: '\t', n: '\n', f: '\f', r: '\r', '"': '"', '\\': '\\' };
            if (Object.hasOwn(simple, escape)) { value += simple[escape]; continue; }
            if (escape === 'u' || escape === 'U') {
                const width = escape === 'u' ? 4 : 8, hex = this.rest().slice(0, width);
                const code = hex.length === width && /^[0-9A-Fa-f]+$/.test(hex) ? parseInt(hex, 16) : NaN;
                if (!Number.isFinite(code) || code > 0x10ffff || code >= 0xd800 && code < 0xe000) { fail('invalid_toml_escape'); }
                value += String.fromCodePoint(code); this.at += width; continue;
            }
            fail('unsupported_toml_escape');
        }
    }
    keys(): string[] {
        const keys: string[] = [];
        for (;;) {
            this.space();
            let key: string;
            if (this.peek() === '"' || this.peek() === "'") { key = this.string(); }
            else {
                const match = /^[A-Za-z0-9_-]*/.exec(this.rest())![0];
                if (!match) { fail('invalid_toml_key'); }
                this.at += match.length; key = match;
            }
            keys.push(key);
            if (keys.length > 32) { fail('toml_key_depth_bound'); }
            this.space();
            if (this.peek() !== '.') { return keys; }
            this.at++;
        }
    }
    value(depth: number): unknown {
        if (depth > 32) { fail('toml_value_depth_bound'); }
        this.space();
        const c = this.peek();
        if (c === '"' || c === "'") { return this.string(); }
        if (c === '[') {
            this.at++;
            const items: unknown[] = [];
            this.space();
            while (this.peek() !== ']') {
                items.push(this.value(depth + 1));
                this.space();
                if (this.peek() === ',') { this.at++; this.space(); } else if (this.peek() !== ']') { fail('invalid_toml_array'); }
                if (items.length > 65536) { fail('toml_array_bound'); }
            }
            this.at++;
            return items;
        }
        if (c === '{') {
            this.at++;
            const table: Json = {};
            this.space();
            while (this.peek() !== '}') {
                const keys = this.keys();
                if (this.peek() !== '=') { fail('invalid_inline_toml'); }
                this.at++;
                set(table, keys, this.value(depth + 1));
                this.space();
                if (this.peek() === ',') { this.at++; this.space(); } else if (this.peek() !== '}') { fail('invalid_inline_toml'); }
            }
            this.at++;
            return table;
        }
        const rest = this.rest();
        const width = rest.search(/[\s,\]}]/) < 0 ? rest.length : rest.search(/[\s,\]}]/);
        if (width === 0) { fail('invalid_toml_value'); }
        this.at += width;
        return scalar(rest.slice(0, width));
    }
    complete(): void { this.space(); if (this.at !== this.text.length) { fail('unsupported_mcp_toml_value'); } }
}

function scalar(raw: string): unknown {
    if (raw === 'true' || raw === 'false') { return raw === 'true'; }
    const opaque = () => ({ tomlScalar: raw });
    const digits = raw.replaceAll('_', ''), unsigned = digits.replace(/^[+-]+/, '');
    const radix = ([['0x', 16], ['0o', 8], ['0b', 2]] as const).find(([prefix]) => raw.startsWith(prefix));
    if (radix) {
        const body = digits.slice(2), valid = { 16: /^[0-9A-Fa-f]+$/, 8: /^[0-7]+$/, 2: /^[01]+$/ }[radix[1]];
        if (!body || !valid.test(body)) { fail('unsupported_mcp_toml_value'); }
        const value = parseInt(body, radix[1]);
        return Math.abs(value) < 2 ** 53 ? value : opaque();
    }
    if (unsigned && /^[0-9]/.test(unsigned) && /^[0-9.eE+-]+$/.test(unsigned)) {
        if (/^[+-]?[0-9]+$/.test(digits)) { const value = Number(digits); return Number.isSafeInteger(value) ? value : opaque(); }
        const value = Number(digits);
        return Number.isFinite(value) ? value : opaque();
    }
    const date = raw.length >= 10 && /^[0-9]{4}-/.test(raw), time = raw.length >= 8 && /^[0-9]{2}:/.test(raw);
    if (date || time || unsigned === 'inf' || unsigned === 'nan') { return opaque(); }
    return fail('unsupported_mcp_toml_value');
}

function set(target: Json, keys: string[], value: unknown): void {
    const last = keys[keys.length - 1];
    if (last === undefined) { fail('invalid_toml_key'); }
    let node = target;
    for (const key of keys.slice(0, -1)) {
        if (!Object.hasOwn(node, key)) { node[key] = {}; }
        const next = node[key];
        if (!next || typeof next !== 'object' || Array.isArray(next)) { fail('conflicting_mcp_toml_field'); }
        node = next as Json;
    }
    if (Object.hasOwn(node, last)) { fail('duplicate_mcp_toml_field'); }
    node[last] = value;
}

function headerKeys(text: string): { array: boolean; keys: string[] } | undefined {
    if (!text.startsWith('[')) { return undefined; }
    const array = text.startsWith('[['), close = array ? ']]' : ']';
    if (!text.endsWith(close) || text.length < 2 * close.length) { fail('invalid_toml_header'); }
    const cursor = new Cursor(text.slice(close.length, text.length - close.length));
    const keys = cursor.keys(); cursor.complete();
    return { array, keys };
}

const sameKeys = (a: readonly string[], b: readonly string[]) => a.length === b.length && a.every((key, index) => key === b[index]);

/** Every server under any `mcp_servers` table. */
export function declarations(source: string): Declaration[] {
    const root: Json = {};
    const seen: string[][] = [], sealed: string[][] = [];
    let table: string[] = [];
    for (const statement of scan(source)) {
        const text = statement.text;
        if (!text) { continue; }
        const header = headerKeys(text);
        if (header) {
            table = header.keys;
            if (table.includes('mcp_servers')) {
                if (header.array) { fail('unsupported_mcp_array_table'); }
                if (seen.some(keys => sameKeys(keys, table))) { fail('duplicate_mcp_toml_table'); }
                seen.push(table);
            }
            continue;
        }
        const cursor = new Cursor(text);
        const keys = cursor.keys();
        cursor.eat('=');
        const full = [...table, ...keys];
        if (!full.includes('mcp_servers')) { continue; }
        if (sealed.some(keys => keys.length <= full.length && sameKeys(keys, full.slice(0, keys.length)))) { fail('conflicting_inline_mcp_toml'); }
        cursor.space();
        const inline = cursor.peek() === '{';
        const value = cursor.value(0);
        cursor.complete();
        set(root, full, value);
        if (inline) { sealed.push(full); }
    }
    const rows: Declaration[] = [];
    visit(root, [], rows);
    return rows;
}
function visit(value: Json, prefix: string[], rows: Declaration[]): void {
    // serde_json::Map is ordered by key; visit in the same order.
    for (const key of Object.keys(value).sort()) {
        const item = value[key];
        if (key === 'mcp_servers') {
            if (!item || typeof item !== 'object' || Array.isArray(item)) { fail('invalid_mcp_server_group'); }
            for (const serverKey of Object.keys(item as Json).sort()) {
                const fields = (item as Json)[serverKey];
                if (!fields || typeof fields !== 'object' || Array.isArray(fields)) { fail('invalid_mcp_server_declaration'); }
                rows.push({ serverKey, container: [...prefix, key], fields: fields as Json });
                if (rows.length > MAX_DECLARATIONS) { fail('configuration_declaration_bound'); }
            }
        } else if (item && typeof item === 'object' && !Array.isArray(item)) {
            visit(item as Json, [...prefix, key], rows);
        }
    }
}

const eol = (source: string) => source.includes('\r\n') ? '\r\n' : '\n';

function serverTables(source: string, key: string): { statements: Statement[]; tables: { header: number; start: number; end: number; nested: boolean }[] } {
    declarations(source);
    const statements = scan(source);
    const headers: [number, string[]][] = [];
    statements.forEach((statement, index) => { const header = headerKeys(statement.text); if (header) { headers.push([index, header.keys]); } });
    const tables: { header: number; start: number; end: number; nested: boolean }[] = [];
    headers.forEach(([index, keys], position) => {
        if (keys[0] !== 'mcp_servers' || keys[1] !== key) { return; }
        let nested: boolean;
        if (keys.length === 2) { nested = false; }
        else if (keys.length === 3 && NESTED.includes(keys[2])) { nested = true; }
        else { throw new Error('Custom AIRK TOML subtables require review; existing file preserved.'); }
        const following = headers[position + 1];
        tables.push({ header: index, start: statements[index].start, end: following ? statements[following[0]].start : source.length, nested });
    });
    if (!tables.some(table => !table.nested)) { throw new Error('Inline or dotted AIRK TOML requires manual review; existing file preserved.'); }
    return { statements, tables };
}

/** Remove `[mcp_servers.<key>]` and its AIRK subtables; their comments stay in place. */
export function removeServer(source: string, key: string): string {
    const { statements, tables } = serverTables(source, key);
    const newline = eol(source);
    let out = source;
    for (const table of [...tables].reverse()) {
        const comments = statements.filter(statement => statement.start >= table.start && statement.start < table.end).flatMap(statement => statement.comments);
        const replacement = comments.length ? comments.join(newline) + newline : '';
        out = out.slice(0, table.start) + replacement + out.slice(table.end);
    }
    return out;
}
