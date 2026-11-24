// SPDX-License-Identifier: Apache-2.0
// Node 24+: read AIRK's local JSON API from the native endpoint record.
// Plain HTTP on 127.0.0.1 with no token (U110); reads need a scope:
// --index/--set (repeatable) or --workspace.
'use strict';
const fs = require('node:fs/promises');
const path = require('node:path');
const http = require('node:http');
const MAX_REPLY = 1024 * 1024;

async function privateBytes(filename, maximum) {
    const samePath = (a, b) => process.platform === 'win32' ? path.toNamespacedPath(path.resolve(a)).toLowerCase() === path.toNamespacedPath(path.resolve(b)).toLowerCase() : a === b;
    if (typeof filename !== 'string' || !path.isAbsolute(filename) || !samePath(await fs.realpath(filename), filename)) throw new Error('Expected an absolute native-owned file without redirects');
    const before = await fs.lstat(filename, { bigint: true });
    if (!before.isFile() || before.isSymbolicLink() || before.nlink !== 1n || before.size > BigInt(maximum)) throw new Error('Invalid private file');
    if (process.platform !== 'win32' && (before.uid !== BigInt(process.geteuid()) || (before.mode & 0o77n))) throw new Error('File is not private to this user');
    const same = (a, b) => ['dev', 'ino', 'size', 'mtimeNs', 'ctimeNs'].every(key => a[key] === b[key]);
    const file = await fs.open(filename, 'r');
    try {
        if (!same(before, await file.stat({ bigint: true }))) throw new Error('Private file changed while opening');
        const data = Buffer.alloc(maximum + 1);
        let length = 0;
        while (length < data.length) { const read = await file.read(data, length, data.length - length, null); if (!read.bytesRead) break; length += read.bytesRead; }
        if (length !== Number(before.size) || !same(before, await file.stat({ bigint: true })) || !same(before, await fs.lstat(filename, { bigint: true })) || !samePath(await fs.realpath(filename), filename)) throw new Error('Private file changed while reading');
        return data.subarray(0, length);
    } finally { await file.close(); }
}

function scoped(arguments_, indexes = [], sets = [], workspace) {
    // U110: merge per-call scope selectors into the tool arguments.
    if (!arguments_ || typeof arguments_ !== 'object' || Array.isArray(arguments_)) throw new Error('Use a JSON argument object');
    const merged = { ...arguments_ };
    for (const [key, values] of [['index_ids', indexes], ['index_set_ids', sets]]) {
        if (!values.length) continue;
        const existing = merged[key] ?? [];
        if (!Array.isArray(existing)) throw new Error(key + ' must be a list');
        merged[key] = [...existing, ...values.filter(value => !existing.includes(value))];
    }
    if (workspace !== undefined) {
        if (!path.isAbsolute(workspace) || (merged.workspace !== undefined && merged.workspace !== workspace)) throw new Error('Use one absolute workspace folder');
        merged.workspace = workspace;
    }
    return merged;
}

async function query(controlDir, tool, arguments_ = {}) {
    if (typeof controlDir !== 'string' || !path.isAbsolute(controlDir)) throw new Error('Expected the absolute service control directory');
    const native = path.join(controlDir, 'http-1');
    const endpoint = JSON.parse((await privateBytes(path.join(native, 'endpoint.json'), 64 * 1024)).toString('utf8'));
    if (endpoint?.schema !== 1 || endpoint.state !== 'ready') throw new Error('The native endpoint is not ready; start the AIRK service');
    const match = /^http:\/\/127\.0\.0\.1:([1-9][0-9]{0,4})\/mcp$/.exec(endpoint.url);
    if (!match || Number(match[1]) > 65535) throw new Error('Invalid loopback endpoint');
    let route = '/api/v1/tools', body;
    if (tool !== undefined) {
        if (!/^[A-Za-z0-9_]{1,64}$/.test(tool) || !arguments_ || typeof arguments_ !== 'object' || Array.isArray(arguments_)) throw new Error('Use an advertised tool and JSON argument object');
        route += '/' + tool;
        body = Buffer.from(JSON.stringify(arguments_), 'utf8');
        if (body.length > 65536) throw new Error('Request exceeds transport budget');
    }
    return new Promise((resolve, reject) => {
        // Plain HTTP straight to the literal loopback address. agent:false
        // keeps one owned connection and no environment proxy is consulted.
        const request = http.request({ hostname: '127.0.0.1', port: Number(match[1]), path: route, method: body ? 'POST' : 'GET',
            agent: false, maxHeaderSize: 65536,
            headers: body ? { 'Content-Type': 'application/json', 'Content-Length': body.length } : {} }); // U110: no Authorization header.
        const timer = setTimeout(() => request.destroy(new Error('Request deadline exhausted')), 125000);
        const done = (error, result) => { clearTimeout(timer); if (error) reject(error); else resolve(result); };
        request.once('error', error => done(error));
        request.once('response', response => {
            const chunks = []; let bytes = 0;
            response.on('data', chunk => { bytes += chunk.length; if (bytes > MAX_REPLY) response.destroy(new Error('Response exceeds transport budget')); else chunks.push(chunk); });
            response.once('error', error => done(error));
            response.once('end', () => { try { done(undefined, { status: response.statusCode, result: JSON.parse(Buffer.concat(chunks).toString('utf8')) }); } catch (error) { done(error); } });
        });
        // No proxy, redirect following, automatic retry or MCP session is involved.
        request.end(body);
    });
}

async function main(argv) {
    const options = {}, indexes = [], sets = [];
    for (let i = 0; i < argv.length; i += 2) {
        const [flag, value] = [argv[i], argv[i + 1]];
        if (value === undefined) throw new Error('Invalid arguments');
        if (flag === '--index') indexes.push(value);
        else if (flag === '--set') sets.push(value);
        else if (['--control-dir', '--tool', '--arguments', '--arguments-file', '--workspace'].includes(flag) && options[flag] === undefined) options[flag] = value;
        else throw new Error('Invalid arguments');
    }
    if (!options['--control-dir']) throw new Error('Expected --control-dir <absolute service control directory>');
    if (options['--arguments'] && options['--arguments-file']) throw new Error('Choose one argument input');
    const scope = indexes.length || sets.length || options['--workspace'] !== undefined;
    if (scope && options['--tool'] === undefined) throw new Error('Scope options need --tool');
    const arguments_ = options['--arguments-file'] ? (await privateBytes(options['--arguments-file'], 65536)).toString('utf8') : options['--arguments'] || '{}';
    const parsed = JSON.parse(arguments_);
    const sent = options['--tool'] === undefined ? parsed : scoped(parsed, indexes, sets, options['--workspace']);
    const { status, result } = await query(options['--control-dir'], options['--tool'], sent);
    process.stdout.write(JSON.stringify(result, null, 2) + '\n');
    return status === 200 && !result?.isError ? 0 : 1;
}
module.exports = { query, scoped };
if (require.main === module) main(process.argv.slice(2)).then(code => { process.exitCode = code; }, () => { process.stderr.write('AIRK local HTTP read failed; check the service control directory and native status.\n'); process.exitCode = 1; });
