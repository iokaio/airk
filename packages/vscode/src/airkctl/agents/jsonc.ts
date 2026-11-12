// SPDX-License-Identifier: Apache-2.0
//! Strict JSONC reader with spans and the removal edits only: a port of
//! crates/airkctl/src/agents/config/jsonc.rs (`parse`, `remove_member`,
//! `remove_element`). Offsets are UTF-16 indices; every edit slices the same
//! string, so results equal the Rust byte edits. Comments and trailing commas
//! are accepted; syntax errors, duplicate keys and a non-object root are refused.
const MAX_DEPTH = 128;

export type Kind =
    | { type: 'object'; members: Member[] }
    | { type: 'array'; items: Node[] }
    | { type: 'string'; value: string }
    | { type: 'scalar'; value: unknown };
export interface Node { start: number; end: number; kind: Kind }
export interface Member { key: string; start: number; value: Node }

export function member(node: Node, key: string): Member | undefined {
    return node.kind.type === 'object' ? node.kind.members.find(item => item.key === key) : undefined;
}
export function at(node: Node, path: readonly string[]): Node | undefined {
    let current: Node | undefined = node;
    for (const key of path) { current = current && member(current, key)?.value; }
    return current;
}
export function valueOf(node: Node): unknown {
    switch (node.kind.type) {
        case 'object': return Object.fromEntries(node.kind.members.map(item => [item.key, valueOf(item.value)]));
        case 'array': return node.kind.items.map(valueOf);
        default: return node.kind.value;
    }
}
function childSpans(node: Node): [number, number][] {
    if (node.kind.type === 'object') { return node.kind.members.map(item => [item.start, item.value.end]); }
    if (node.kind.type === 'array') { return node.kind.items.map(item => [item.start, item.end]); }
    return [];
}

/** Parse a configuration file whose root must be an object. A leading BOM is skipped. */
export function parse(source: string): Node {
    const reader = new Reader(source, source.startsWith('﻿') ? 1 : 0);
    reader.trivia();
    const root = reader.value(0);
    reader.trivia();
    if (reader.at !== source.length) { throw new Error(reader.error('unexpected content after the configuration object')); }
    if (root.kind.type !== 'object') { throw new Error('invalid JSON configuration: the root must be an object'); }
    return root;
}

class Reader {
    constructor(readonly text: string, public at: number) {}
    error(what: string): string { return `invalid JSON configuration at offset ${this.at}: ${what}`; }
    peek(): string | undefined { return this.at < this.text.length ? this.text[this.at] : undefined; }
    trivia(): void {
        for (;;) {
            const c = this.peek(), next = this.text[this.at + 1];
            if (c === ' ' || c === '\t' || c === '\r' || c === '\n') { this.at++; }
            else if (c === '/' && next === '/') { while (this.peek() !== undefined && this.peek() !== '\n') { this.at++; } }
            else if (c === '/' && next === '*') {
                const end = this.text.indexOf('*/', this.at + 2);
                if (end < 0) { throw new Error(this.error('unterminated comment')); }
                this.at = end + 2;
            } else { return; }
        }
    }
    value(depth: number): Node {
        if (depth > MAX_DEPTH) { throw new Error(this.error('nesting exceeds the bound')); }
        const start = this.at;
        let kind: Kind;
        const c = this.peek();
        if (c === '{') {
            this.at++;
            const members: Member[] = [];
            for (;;) {
                this.trivia();
                if (this.peek() === '}') { break; }
                if (this.peek() !== '"') { throw new Error(this.error('expected a property name')); }
                const keyStart = this.at, key = this.string();
                if (members.some(item => item.key === key)) { throw new Error(`invalid JSON configuration: duplicate key ${JSON.stringify(key)}`); }
                this.trivia();
                if (this.peek() !== ':') { throw new Error(this.error("expected ':'")); }
                this.at++; this.trivia();
                members.push({ key, start: keyStart, value: this.value(depth + 1) });
                this.trivia();
                if (this.peek() === ',') { this.at++; }
                else if (this.peek() === '}') { break; }
                else { throw new Error(this.error("expected ',' or '}'")); }
            }
            this.at++;
            kind = { type: 'object', members };
        } else if (c === '[') {
            this.at++;
            const items: Node[] = [];
            for (;;) {
                this.trivia();
                if (this.peek() === ']') { break; }
                items.push(this.value(depth + 1));
                this.trivia();
                if (this.peek() === ',') { this.at++; }
                else if (this.peek() === ']') { break; }
                else { throw new Error(this.error("expected ',' or ']'")); }
            }
            this.at++;
            kind = { type: 'array', items };
        } else if (c === '"') {
            kind = { type: 'string', value: this.string() };
        } else if (c !== undefined && /[-0-9]/.test(c)) {
            while (this.peek() !== undefined && /[-+.eE0-9]/.test(this.peek()!)) { this.at++; }
            let number: unknown;
            try { number = JSON.parse(this.text.slice(start, this.at)); } catch { throw new Error(this.error('invalid number')); }
            if (typeof number !== 'number') { throw new Error(this.error('invalid number')); }
            kind = { type: 'scalar', value: number };
        } else {
            const rest = this.text.slice(this.at);
            const word = (['true', 'false', 'null'] as const).find(item => rest.startsWith(item));
            if (!word) { throw new Error(this.error('unexpected character')); }
            this.at += word.length;
            if (this.peek() !== undefined && /[A-Za-z0-9_]/.test(this.peek()!)) { throw new Error(this.error('unexpected character')); }
            kind = { type: 'scalar', value: word === 'true' ? true : word === 'false' ? false : null };
        }
        return { start, end: this.at, kind };
    }
    private hex4(): number {
        const digits = this.text.slice(this.at, this.at + 4);
        if (!/^[0-9A-Fa-f]{4}$/.test(digits)) { throw new Error(this.error('invalid unicode escape')); }
        this.at += 4;
        return parseInt(digits, 16);
    }
    string(): string {
        this.at++;
        let out = '', run = this.at;
        for (;;) {
            const c = this.peek();
            if (c === undefined) { throw new Error(this.error('unterminated string')); }
            if (c === '"') { out += this.text.slice(run, this.at); this.at++; return out; }
            if (c === '\\') {
                out += this.text.slice(run, this.at);
                const escape = this.text[this.at + 1];
                if (escape === undefined) { throw new Error(this.error('unterminated string')); }
                this.at += 2;
                const simple: Record<string, string> = { '"': '"', '\\': '\\', '/': '/', b: '\b', f: '\f', n: '\n', r: '\r', t: '\t' };
                if (Object.hasOwn(simple, escape)) { out += simple[escape]; }
                else if (escape === 'u') {
                    const unit = this.hex4();
                    let decoded = unit >= 0xd800 && unit < 0xe000 ? '�' : String.fromCharCode(unit);
                    if (unit >= 0xd800 && unit < 0xdc00 && this.text.startsWith('\\u', this.at)) {
                        const save = this.at; this.at += 2;
                        const low = this.hex4();
                        if (low >= 0xdc00 && low < 0xe000) { decoded = String.fromCodePoint(0x10000 + ((unit - 0xd800) << 10) + (low - 0xdc00)); }
                        else { this.at = save; }
                    }
                    out += decoded;
                } else { throw new Error(this.error('invalid escape')); }
                run = this.at;
                continue;
            }
            if (c.charCodeAt(0) <= 0x1f) { throw new Error(this.error('control character in string')); }
            this.at++;
        }
    }
}

function lineStart(source: string, offset: number): number { return source.lastIndexOf('\n', offset - 1) + 1; }
const blank = (text: string) => /^[ \t]*$/.test(text);

/** An optional comma, then only spaces and same-line comments up to `end`. */
function afterLine(source: string, from: number): { comma?: number; end: number; clean: boolean } {
    let offset = from, comma: number | undefined;
    const skip = () => { while (source[offset] === ' ' || source[offset] === '\t') { offset++; } };
    skip();
    if (source[offset] === ',') { comma = offset; offset++; skip(); }
    for (;;) {
        if (source.startsWith('//', offset)) {
            while (offset < source.length && source[offset] !== '\n' && source[offset] !== '\r') { offset++; }
        } else if (source.startsWith('/*', offset)) {
            const end = source.indexOf('*/', offset + 2);
            if (end >= 0 && !source.slice(offset, end).includes('\n')) { offset = end + 2; skip(); continue; }
        }
        break;
    }
    return { ...(comma === undefined ? {} : { comma }), end: offset, clean: offset >= source.length || source[offset] === '\n' || source[offset] === '\r' };
}
/** The first comma after `from`, before `limit`, skipping spaces and comments. */
function commaAfter(source: string, from: number, limit: number): number | undefined {
    let offset = from;
    while (offset < limit) {
        const c = source[offset];
        if (c === ',') { return offset; }
        if (c === ' ' || c === '\t' || c === '\r' || c === '\n') { offset++; }
        else if (source.startsWith('//', offset)) { const end = source.indexOf('\n', offset); offset += end < 0 ? limit - offset : end - offset; }
        else if (source.startsWith('/*', offset)) { const end = source.indexOf('*/', offset); offset += end < 0 ? limit - offset : end - offset + 2; }
        else { return undefined; }
    }
    return undefined;
}

/** Remove one child with its line when it has a line of its own, keeping the
 * remaining children's commas valid; an emptied container becomes `{}`/`[]`. */
function removeChild(source: string, container: Node, index: number): string {
    const spans = childSpans(container);
    const [start, end] = spans[index];
    const open = container.start, close = container.end - 1;
    const line = lineStart(source, start);
    const after = afterLine(source, end);
    if (blank(source.slice(line, start)) && after.clean) {
        let cut = after.end;
        if (source.startsWith('\r\n', cut)) { cut += 2; } else if (source.startsWith('\n', cut)) { cut += 1; }
        if (spans.length === 1 && !source.slice(open + 1, line).trim() && !source.slice(cut, close).trim()) {
            return source.slice(0, open + 1) + source.slice(close);
        }
        let out = source.slice(0, line) + source.slice(cut);
        if (after.comma === undefined && index > 0) {
            const comma = commaAfter(source, spans[index - 1][1], line);
            if (comma !== undefined) { out = out.slice(0, comma) + out.slice(comma + 1); }
        }
        return out;
    }
    if (index > 0) { return source.slice(0, spans[index - 1][1]) + source.slice(end); }
    if (spans.length > 1) { return source.slice(0, start) + source.slice(spans[1][0]); }
    return source.slice(0, open + 1) + source.slice(close);
}

export function removeMember(source: string, path: readonly string[], key: string): string {
    const tree = parse(source);
    const node = at(tree, path);
    if (!node) { throw new Error(`${path.join('.')} is missing`); }
    if (node.kind.type !== 'object') { throw new Error(`${path.join('.')} must be an object; existing configuration preserved.`); }
    const index = node.kind.members.findIndex(item => item.key === key);
    if (index < 0) { throw new Error(`${key} is missing`); }
    return removeChild(source, node, index);
}
export function removeElement(source: string, path: readonly string[], index: number): string {
    const node = at(parse(source), path);
    if (!node || node.kind.type !== 'array' || index >= node.kind.items.length) { throw new Error(`${path.join('.')} changed; existing configuration preserved.`); }
    return removeChild(source, node, index);
}
