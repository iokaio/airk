// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { spawn } from 'node:child_process';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { test } from 'node:test';
import { ASSISTANCE_SERVER_NAME, ASSISTANCE_TOOLS } from '../explorerContracts';
import { FEATURES } from '../explorerFeatures';
import { ROUTE_KEYS } from '../explorerRoutes';

// Black-box checks of the built out/explorerAssistance.js. A preload replaces every
// process, socket and file-write entry point with a recorder, so any attempt to reach
// the Rust runtime, the service or the workspace is visible (requirements AT-01, AT-26).
const bundle = path.resolve(__dirname, '..', 'explorerAssistance.js');
const PRELOAD = `
const record = name => () => { process.stderr.write('VIOLATION ' + name + '\\n'); throw new Error('blocked ' + name); };
const cp = require('node:child_process');
for (const key of ['spawn', 'spawnSync', 'exec', 'execSync', 'execFile', 'execFileSync', 'fork']) cp[key] = record('child_process.' + key);
const net = require('node:net');
for (const key of ['connect', 'createConnection', 'createServer']) net[key] = record('net.' + key);
const fs = require('node:fs');
for (const key of ['writeFile', 'writeFileSync', 'appendFile', 'appendFileSync', 'mkdir', 'mkdirSync', 'rm', 'rmSync', 'unlink', 'unlinkSync', 'rename', 'renameSync', 'open', 'openSync']) fs[key] = record('fs.' + key);
for (const key of ['writeFile', 'appendFile', 'mkdir', 'rm', 'unlink', 'rename', 'open']) fs.promises[key] = record('fs.promises.' + key);
`;

type Response = { id: number | null; result?: any; error?: { code: number; message: string } };

/** Send lines to a fresh server with an isolated environment and collect every response. */
async function exchange(lines: string[]): Promise<{ responses: Response[]; stderr: string; code: number | null }> {
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'airk-assistance-'));
    try {
        const preload = path.join(directory, 'preload.cjs');
        fs.writeFileSync(preload, PRELOAD);
        const child = spawn(process.execPath, ['--require', preload, bundle], { cwd: directory, stdio: ['pipe', 'pipe', 'pipe'],
            env: { PATH: '', HOME: directory, USERPROFILE: directory, SystemRoot: process.env.SystemRoot ?? '' } });
        let stdout = '', stderr = '';
        child.stdout.setEncoding('utf8').on('data', chunk => { stdout += chunk; });
        child.stderr.setEncoding('utf8').on('data', chunk => { stderr += chunk; });
        child.stdin.end(lines.map(line => line + '\n').join(''));
        const code = await new Promise<number | null>(resolve => child.on('close', resolve));
        assert.deepEqual(fs.readdirSync(directory), ['preload.cjs'], 'the server created no files in its working directory');
        return { responses: stdout.split('\n').filter(Boolean).map(line => JSON.parse(line)), stderr, code };
    } finally { fs.rmSync(directory, { recursive: true, force: true }); }
}

const rpc = (id: number, method: string, params?: object) => JSON.stringify({ jsonrpc: '2.0', id, method, ...(params ? { params } : {}) });
const call = (id: number, name: string, args?: object) => rpc(id, 'tools/call', { name, ...(args ? { arguments: args } : {}) });
const INIT = rpc(0, 'initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'test', version: '1' } });
const byId = (responses: Response[], id: number) => responses.find(response => response.id === id)!;
const structured = (responses: Response[], id: number) => byId(responses, id).result.structuredContent;

test('the server answers help offline with no process, socket or file effects', { timeout: 30000 }, async () => {
    assert.ok(fs.existsSync(bundle), 'compile builds out/explorerAssistance.js');
    const { responses, stderr, code } = await exchange([
        INIT,
        JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }),
        rpc(1, 'tools/list'),
        call(2, 'airk_explorer_help_search', { query: 'repair rebuild verify', limit: 3 }),
        call(3, 'airk_explorer_help_search', { query: 'repair rebuild verify', limit: 3 }),
        call(4, 'airk_explorer_help_get', { topic_id: 'management.repair' }),
        call(5, 'airk_explorer_ui_status'),
        rpc(6, 'resources/list'),
        rpc(7, 'resources/read', { uri: 'airk-explorer-help://management.repair' }),
    ]);
    assert.equal(stderr, '', 'no blocked call was attempted');
    assert.equal(code, 0);
    const init = byId(responses, 0).result;
    assert.equal(init.protocolVersion, '2025-06-18');
    assert.equal(init.serverInfo.name, ASSISTANCE_SERVER_NAME);
    assert.deepEqual(Object.keys(init.capabilities).sort(), ['resources', 'tools']);

    const tools = byId(responses, 1).result.tools;
    assert.deepEqual(tools.map((tool: { name: string }) => tool.name), [...ASSISTANCE_TOOLS], 'exactly the shipped Explorer tools; no airk_* retrieval tools');
    for (const tool of tools) {
        assert.equal(tool.annotations.readOnlyHint, true);
        assert.equal(tool.inputSchema.additionalProperties, false);
    }

    const search = structured(responses, 2);
    assert.deepEqual(search, structured(responses, 3), 'ranking is deterministic');
    assert.equal(search.results[0].topic_id, 'management.repair');
    assert.ok(search.results.length <= 3);

    const topic = structured(responses, 4);
    assert.equal(topic.applicability, FEATURES['manage/repair']);
    assert.equal(topic.location, 'AIRK Explorer › Management › Repair');
    assert.match(topic.topic_revision, /^[0-9a-f]{16}$/);
    assert.equal(topic.source_id, 'packages/vscode/src/explorerHelp/manage-repair.md');
    assert.ok(topic.steps.length >= 3 && topic.steps[1].includes('Create repair preview'));
    assert.ok(topic.related_airk_help.includes('maintenance'));

    const status = structured(responses, 5);
    assert.equal(status.ui.attached, false);
    assert.equal(status.ui.code, 'UI_NOT_ATTACHED');
    assert.deepEqual(status.routes.map((route: { route: string }) => route.route), [...ROUTE_KEYS]);
    for (const route of status.routes) { assert.equal(route.availability, FEATURES[route.route]); }
    assert.doesNotMatch(JSON.stringify(status), /[A-Za-z]:\\\\|\/(?:home|Users|tmp|var)\//, 'ui_status carries no paths');

    const resources = byId(responses, 6).result.resources;
    assert.ok(resources.some((resource: { uri: string }) => resource.uri === 'airk-explorer-help://management.repair'));
    assert.match(byId(responses, 7).result.contents[0].text, /^# Diagnose and repair/);
});

test('unshipped screens are labeled and invalid requests are refused, never coerced', { timeout: 30000 }, async () => {
    const planned = Object.entries(FEATURES).find(([, value]) => value === 'prototype_only')?.[0];
    const { responses, stderr } = await exchange([
        rpc(1, 'tools/list'),
        INIT,
        call(2, 'airk_explorer_help_search', { query: 'x', unexpected: true }),
        call(3, 'airk_explorer_help_search', { query: '   ' }),
        call(4, 'airk_explorer_help_search', { query: 'search', limit: 50 }),
        call(5, 'airk_explorer_help_get', { topic_id: 'no.such.topic' }),
        call(6, 'airk_explorer_help_search', { query: 'search index project management', limit: 20, max_bytes: 512 }),
        call(7, 'airk_explorer_suggest_plan', {}),
        rpc(8, 'resources/read', { uri: 'file:///etc/passwd' }),
        call(9, 'airk_explorer_help_search', { query: planned ?? 'studio', limit: 20 }),
        'not json',
        'x'.repeat(1_100_000),
        rpc(10, 'ping'),
    ]);
    assert.equal(stderr, '');
    assert.equal(byId(responses, 1).error!.code, -32002, 'nothing but ping before initialize');
    for (const [id, code] of [[2, 'INVALID_SCHEMA'], [3, 'INVALID_SCHEMA'], [4, 'INVALID_SCHEMA'], [5, 'HELP_TOPIC_UNKNOWN'], [7, 'CAPABILITY_DISABLED']] as const) {
        const result = byId(responses, id).result;
        assert.equal(result.isError, true, `request ${id}`);
        assert.equal(result.structuredContent.error.code, code, `request ${id}`);
    }
    const bounded = structured(responses, 6);
    assert.equal(bounded.truncated, true);
    assert.ok(Buffer.byteLength(JSON.stringify(bounded)) <= 512);
    assert.equal(byId(responses, 8).error!.code, -32002);
    if (planned) {
        const unshipped = structured(responses, 9).results.find((result: { route?: string }) => result.route === planned);
        assert.equal(unshipped?.applicability, 'prototype_only');
    }
    assert.ok(responses.some(response => response.id === null && response.error?.code === -32700), 'malformed JSON gets a parse error');
    assert.ok(responses.some(response => response.id === null && response.error?.message === 'Message too large'));
    assert.deepEqual(byId(responses, 10).result, {}, 'the server keeps serving after refused input');
});
