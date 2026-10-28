// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import * as os from 'node:os';
import { test } from 'node:test';
import { McpSessionError, McpStdioSession } from '../mcpStdioSession';

// A tiny line-delimited JSON-RPC server: behaviour is selected by the method name.
const SERVER = `
const rl = require('node:readline').createInterface({ input: process.stdin });
const send = value => process.stdout.write(JSON.stringify(value) + '\\n');
rl.on('line', line => {
  const m = JSON.parse(line);
  if (m.id === undefined) { if (m.method === 'notifications/cancelled') send({ jsonrpc: '2.0', method: 'seen-cancel', params: m.params }); return; }
  switch (m.method) {
    case 'initialize': send({ jsonrpc: '2.0', id: m.id, result: { echo: m.params } }); break;
    case 'echo': send({ jsonrpc: '2.0', id: m.id, result: m.params }); break;
    case 'slow': setTimeout(() => send({ jsonrpc: '2.0', id: m.id, result: m.params }), m.params.delay); break;
    case 'error': send({ jsonrpc: '2.0', id: m.id, error: { code: -32602, message: 'bad params' } }); break;
    case 'invalid': process.stdout.write('{not json\\n'); break;
    case 'big': send({ jsonrpc: '2.0', id: m.id, result: 'x'.repeat(m.params.bytes) }); break;
    case 'exit': process.exit(3);
  }
});`;
function open(maxMessageBytes?: number) {
    return McpStdioSession.open({ command: process.execPath, args: ['-e', SERVER], cwd: os.tmpdir(), clientName: 'airk-explorer',
        clientVersion: 'test', maxMessageBytes }, 10000);
}

test('handshake sends clientInfo and matches concurrent replies by id', async () => {
    const { session, initialize } = await open();
    try {
        assert.equal(initialize.echo.clientInfo.name, 'airk-explorer');
        assert.equal(initialize.echo.protocolVersion, '2025-11-25');
        const [slow, fast] = await Promise.all([session.request('slow', { delay: 60, value: 'first' }), session.request('echo', { value: 'second' })]);
        assert.equal(slow.value, 'first');
        assert.equal(fast.value, 'second');
    } finally { session.close(); }
});

test('a timed-out request fails alone and its late reply is discarded', async () => {
    const { session } = await open();
    try {
        await assert.rejects(session.request('slow', { delay: 300, value: 'late' }, 40), (error: unknown) => error instanceof McpSessionError && error.code === 'timeout');
        assert.equal(session.closed, false);
        assert.equal((await session.request('slow', { delay: 400, value: 'after' })).value, 'after');
    } finally { session.close(); }
});

test('JSON-RPC errors reject only the failing request', async () => {
    const { session } = await open();
    try {
        await assert.rejects(session.request('error', {}), (error: unknown) => error instanceof McpSessionError && error.code === 'rpc' && /bad params/.test(error.message));
        assert.equal((await session.request('echo', { ok: true })).ok, true);
    } finally { session.close(); }
});

test('invalid JSON, oversize replies and exits close the session and reject pending work', async () => {
    for (const [method, params, code, limit] of [['invalid', {}, 'invalid_json', undefined], ['big', { bytes: 4096 }, 'oversize', 1024], ['exit', {}, 'exited', undefined]] as const) {
        const { session } = await open(limit);
        const closed = new Promise<McpSessionError>(resolve => session.onClose(resolve));
        const pending = session.request('slow', { delay: 5000 });
        await assert.rejects(session.request(method, params), (error: unknown) => error instanceof McpSessionError && error.code === code);
        await assert.rejects(pending, (error: unknown) => error instanceof McpSessionError && error.code === code);
        assert.equal((await closed).code, code);
        assert.equal(session.closed, true);
        await assert.rejects(session.request('echo', {}), (error: unknown) => error instanceof McpSessionError && error.code === code);
    }
});

test('close rejects outstanding requests and later calls', async () => {
    const { session } = await open();
    const pending = session.request('slow', { delay: 5000 });
    session.close();
    await assert.rejects(pending, (error: unknown) => error instanceof McpSessionError && error.code === 'closed');
    await assert.rejects(session.request('echo', {}), (error: unknown) => error instanceof McpSessionError && error.code === 'closed');
});

test('a failed handshake kills the child and rejects', async () => {
    await assert.rejects(McpStdioSession.open({ command: process.execPath, args: ['-e', 'process.exit(2)'], cwd: os.tmpdir(),
        clientName: 'airk-explorer', clientVersion: 'test' }, 5000), (error: unknown) => error instanceof McpSessionError && error.code === 'exited');
});

test('a silent handshake timeout retains bounded launcher diagnostics and the failed method',async()=>{
    await assert.rejects(McpStdioSession.open({command:process.execPath,
        args:['-e','process.stderr.write("fixture: waiting for runtime lock");setInterval(()=>{},1000);'],
        cwd:os.tmpdir(),clientName:'airk-test',clientVersion:'test'},1000),error=>{
        assert.ok(error instanceof McpSessionError);assert.equal(error.code,'timeout');
        assert.match(error.message,/initialize.*1000 ms/);assert.match(error.message,/waiting for runtime lock/);return true;
    });
});
