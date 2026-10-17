// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { parse } from 'jsonc-parser';
import { installJsonConfig, JsonClient, jsonConfigPath, mergeJsonConfig, readJsonConfig } from '../agentConfig';
import { LaunchSpec, launchArgs } from '../model';

const spec: LaunchSpec = { command: 'C:\\Program Files\\AIRK\\airk.exe', root: '/fixture', dataDir: '/index with spaces', collection: 'project-fixture', name: 'Fixture' };
const clients: JsonClient[] = ['claude', 'gemini', 'antigravity'];
async function fixture(run: (root: string) => Promise<void>): Promise<void> {
    const root = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-json-config-'));
    try { await run(await fs.realpath(root)); }
    finally {
        const resolved = await fs.realpath(root);
        assert.equal(path.dirname(resolved), await fs.realpath(os.tmpdir()));
        assert.ok(path.basename(resolved).startsWith('airk-json-config-'));
        await fs.rm(resolved, { recursive: true, force: true });
    }
}

test('JSON installation preserves comments, other servers and approval policy for all three clients', () => {
    const source = '{\r\n\t// Keep local approval policy\r\n\t"mcp": {"excluded": ["other"]},\r\n\t"mcpServers": {"other": {"command": "original", "trust": false}},\r\n}\r\n';
    for (const client of clients) {
        const after = mergeJsonConfig(client, spec, source), data = parse(after);
        assert.match(after, /\/\/ Keep local approval policy\r\n/);
        assert.match(after, /\t"mcp"/);
        assert.deepEqual(data.mcp, { excluded: ['other'] });
        assert.deepEqual(data.mcpServers.other, { command: 'original', trust: false });
        assert.deepEqual(data.mcpServers['airk-project-fixture'], { command: spec.command, args: launchArgs(spec) });
        assert.equal(mergeJsonConfig(client, spec, after), after, 'reinstall makes no edits');
        Object.assign(data.mcpServers['airk-project-fixture'], { trust: false });
        const policy = JSON.stringify(data);
        assert.equal(mergeJsonConfig(client, spec, policy), policy, 'same launch preserves client policy');
        assert.throws(() => mergeJsonConfig(client, { ...spec, dataDir: '/changed' }, policy), /different settings/);
    }
});

test('JSON installation rejects malformed, ambiguous, oversized and conflicting settings', () => {
    for (const source of ['', '[]', '{broken', '{"mcpServers": []}', '{"mcpServers":null}',
        '{"mcpServers":{}, "mcpServers":{}}', '{"settings":{"key":1,"key":2}}',
        '{"mcpServers":{"airk-project-fixture":{"command":"different"}}}', ' '.repeat(1_048_577)]) {
        assert.throws(() => mergeJsonConfig('claude', spec, source), /valid JSON|must be an object|duplicate|different settings|limit/);
    }
});

test('install creates exact project destinations and merges through the guarded editor callback', async () => fixture(async root => {
    const selected = { ...spec, root };
    for (const client of clients) {
        const destination = jsonConfigPath(client, selected);
        await installJsonConfig(client, selected, () => {}, async () => { assert.fail('New file needs no editor rewrite'); });
        assert.deepEqual(parse((await fs.readFile(destination, 'utf8'))).mcpServers['airk-project-fixture'].args, launchArgs(selected));
        const existing = '{"mcpServers":{"other":{"command":"keep"}},"policy":false}\n';
        await fs.writeFile(destination, existing);
        let saves = 0;
        await installJsonConfig(client, selected, () => {}, async (file, before, after) => {
            saves++; assert.equal(file, destination); assert.equal(before, existing);
            await fs.writeFile(file, after);
        });
        assert.equal(saves, 1);
        assert.equal(parse(await fs.readFile(destination, 'utf8')).policy, false);
        await installJsonConfig(client, selected, () => {}, async () => { assert.fail('Already configured must not save'); });
    }
    assert.equal(jsonConfigPath('claude', selected), path.join(root, '.mcp.json'));
    assert.equal(jsonConfigPath('gemini', selected), path.join(root, '.gemini', 'settings.json'));
    assert.equal(jsonConfigPath('antigravity', selected), path.join(root, '.agents', 'mcp_config.json'));
}));

test('install refuses stale guides, concurrent creators and redirected or hardlinked configuration', async () => fixture(async root => {
    const selected = { ...spec, root }, destination = jsonConfigPath('claude', selected);
    let checks = 0;
    await assert.rejects(installJsonConfig('claude', selected, () => { if (++checks === 2) { throw new Error('stale guide'); } }, async () => {}), /stale guide/);
    assert.equal(await readJsonConfig(destination), undefined);
    const outcomes = await Promise.allSettled([1, 2].map(() => installJsonConfig('claude', selected, () => {}, async () => {})));
    assert.ok(outcomes.some(result => result.status === 'fulfilled'));
    assert.deepEqual(parse(await fs.readFile(destination, 'utf8')).mcpServers['airk-project-fixture'].args, launchArgs(selected));
    const outside = path.join(root, 'elsewhere'); await fs.mkdir(outside);
    await fs.symlink(outside, path.join(root, '.gemini'), process.platform === 'win32' ? 'junction' : 'dir');
    await assert.rejects(installJsonConfig('gemini', selected, () => {}, async () => {}), /regular directory/);
    assert.deepEqual(await fs.readdir(outside), []);
    const linked = path.join(outside, 'linked.json'); await fs.link(destination, linked);
    await assert.rejects(readJsonConfig(destination), /regular, unlinked file/);
}));
