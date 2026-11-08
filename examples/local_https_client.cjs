// SPDX-License-Identifier: Apache-2.0
// Node 24+: read AIRK's local JSON API from fresh, protected native metadata.
'use strict';
const fs = require('node:fs/promises');
const path = require('node:path');
const https = require('node:https');
const tls = require('node:tls');
const { X509Certificate, createHash } = require('node:crypto');
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

async function query(connectionFile, tool, arguments_ = {}) {
    const connection = JSON.parse((await privateBytes(connectionFile, 60 * 1024)).toString('utf8'));
    const endpoint = connection.endpoint;
    if (connection.schema !== 1 || endpoint?.schema !== 1 || endpoint.state !== 'ready') throw new Error('Obtain a current ready connection from native AIRK setup');
    const match = /^https:\/\/127\.0\.0\.1:([1-9][0-9]{0,4})\/mcp$/.exec(endpoint.url);
    if (!match || Number(match[1]) > 65535) throw new Error('Invalid loopback endpoint');
    const pem = await privateBytes(connection.ca_certificate_file, 8192);
    if ((pem.toString('ascii').match(/-----BEGIN CERTIFICATE-----/g) || []).length !== 1) throw new Error('Expected one native CA certificate');
    const ca = new X509Certificate(pem);
    if (!ca.ca || createHash('sha256').update(ca.raw).digest('hex') !== endpoint.ca_sha256) throw new Error('Native CA identity changed');
    const token = (await privateBytes(connection.credential_file, 103)).toString('ascii');
    if (!/^airk1\.[0-9a-f]{32}\.[0-9a-f]{64}$/.test(token) || token.split('.')[1] !== connection.connection_id) throw new Error('Invalid native credential identity');
    let route = '/api/v1/tools', body;
    if (tool !== undefined) {
        if (!/^[A-Za-z0-9_]{1,64}$/.test(tool) || !arguments_ || typeof arguments_ !== 'object' || Array.isArray(arguments_)) throw new Error('Use an advertised tool and JSON argument object');
        route += '/' + tool;
        body = Buffer.from(JSON.stringify(arguments_), 'utf8');
        if (body.length > 65536) throw new Error('Request exceeds transport budget');
    }
    return new Promise((resolve, reject) => {
        // Explicit CA is additive to Node's effective default roots, including
        // configured enterprise roots. IP/name/validity checks remain enabled.
        const request = https.request({ hostname: '127.0.0.1', port: Number(match[1]), path: route, method: body ? 'POST' : 'GET',
            ca: [...tls.getCACertificates('default'), pem], rejectUnauthorized: true, agent: false, maxHeaderSize: 65536,
            headers: { Authorization: 'Bearer ' + token, ...(body ? { 'Content-Type': 'application/json', 'Content-Length': body.length } : {}) } });
        const timer = setTimeout(() => request.destroy(new Error('Request deadline exhausted')), 125000);
        const done = (error, result) => { clearTimeout(timer); if (error) reject(error); else resolve(result); };
        request.once('error', error => done(error));
        request.once('response', response => {
            const chunks = []; let bytes = 0;
            response.on('data', chunk => { bytes += chunk.length; if (bytes > MAX_REPLY) response.destroy(new Error('Response exceeds transport budget')); else chunks.push(chunk); });
            response.once('error', error => done(error));
            response.once('end', () => { try { done(undefined, { status: response.statusCode, result: JSON.parse(Buffer.concat(chunks).toString('utf8')) }); } catch (error) { done(error); } });
        });
        // HTTPS waits for authenticated TLS before sending these headers. No
        // proxy, redirect following, automatic retry or MCP session is involved.
        request.end(body);
    });
}

async function main(argv) {
    const options = {};
    for (let i = 0; i < argv.length; i += 2) { if (!['--connection', '--tool', '--arguments', '--arguments-file'].includes(argv[i]) || argv[i + 1] === undefined || options[argv[i]] !== undefined) throw new Error('Invalid arguments'); options[argv[i]] = argv[i + 1]; }
    if (!options['--connection']) throw new Error('Expected --connection <protected-native-JSON>');
    if (options['--arguments'] && options['--arguments-file']) throw new Error('Choose one argument input');
    const arguments_ = options['--arguments-file'] ? (await privateBytes(options['--arguments-file'], 65536)).toString('utf8') : options['--arguments'] || '{}';
    const { status, result } = await query(options['--connection'], options['--tool'], JSON.parse(arguments_));
    process.stdout.write(JSON.stringify(result, null, 2) + '\n');
    return status === 200 && !result?.isError ? 0 : 1;
}
module.exports = { query };
if (require.main === module) main(process.argv.slice(2)).then(code => { process.exitCode = code; }, () => { process.stderr.write('AIRK local HTTPS read failed; obtain current verified connection metadata and inspect native status.\n'); process.exitCode = 1; });
