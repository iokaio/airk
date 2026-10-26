// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { parse } from 'jsonc-parser';
import { installJsonConfig, JsonClient, jsonConfigPath, mergeJsonConfig, readJsonConfig, reviewLegacyJsonConfig, replaceReviewedLegacyJsonConfig } from '../agentConfig';
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

test('legacy replacement reviews only the selected bound server and preserves policies and other entries', async () => fixture(async root => {
    const legacy = { ...spec, root, command: path.join(root, 'old', process.platform === 'win32' ? 'airk.exe' : 'airk') };
    const selected = { ...legacy, command: path.join(root, 'launcher', process.platform === 'win32' ? 'airk.exe' : 'airk'),
        dataDir: path.join(root, 'fresh-index'), service: { installDir: path.join(root, 'install'), project: 'a'.repeat(64) } };
    for (const client of clients) {
        await installJsonConfig(client, legacy, () => {}, async () => assert.fail('new configuration'));
        const destination = jsonConfigPath(client, legacy);
        const value = parse(await fs.readFile(destination, 'utf8'));
        value.mcpServers.other = { command: 'preserve', trust: false };
        value.mcpServers[`airk-${legacy.collection}`].trust = false;
        value.approvalPolicy = ['ask'];
        const before = '// Preserve this comment\r\n' + JSON.stringify(value, null, '\t') + '\r\n';
        await fs.writeFile(destination, before);
        const review = await reviewLegacyJsonConfig(client, selected, () => {});
        assert.equal(await fs.readFile(destination, 'utf8'), before, 'review never writes');
        assert.match(review.after, /Preserve this comment\r\n/);
        assert.deepEqual(parse(review.after).mcpServers.other, value.mcpServers.other);
        assert.deepEqual(parse(review.after).approvalPolicy, ['ask']);
        assert.equal(parse(review.after).mcpServers[`airk-${legacy.collection}`].trust, false);
        assert.deepEqual(parse(review.after).mcpServers[`airk-${legacy.collection}`].args, launchArgs(selected));
        await replaceReviewedLegacyJsonConfig(client, selected, review, () => {}, async (file, original, after) => {
            assert.equal(original, before); await fs.writeFile(file, after);
        });
        await assert.rejects(replaceReviewedLegacyJsonConfig(client, selected, review, () => {}, async () => assert.fail('replay')), /expired/);
    }
}));

test('legacy review rejects foreign bindings, ambiguous configuration and redirected files', async () => fixture(async root => {
    const legacy = { ...spec, root, command: path.join(root, 'airk.exe') };
    const selected = { ...legacy, service: { installDir: path.join(root, 'install'), project: 'b'.repeat(64) } };
    const destination = jsonConfigPath('claude', selected);
    for (const entry of [
        { command: legacy.command, args: launchArgs({ ...legacy, root: path.join(root, 'foreign') }) },
        { command: legacy.command, args: [...launchArgs(legacy), '--extra'] },
        { command: path.join(root, 'unknown.exe'), args: launchArgs(legacy) },
        { command: legacy.command, args: launchArgs(legacy), url: '' },
        { command: legacy.command, args: launchArgs(legacy), type: false },
    ]) {
        const before = JSON.stringify({ mcpServers: { [`airk-${legacy.collection}`]: entry } });
        await fs.writeFile(destination, before);
        await assert.rejects(reviewLegacyJsonConfig('claude', selected, () => {}), /does not match/);
        assert.equal(await fs.readFile(destination, 'utf8'), before);
    }
    await fs.writeFile(destination, '{"mcpServers":{},"mcpServers":{}}');
    await assert.rejects(reviewLegacyJsonConfig('claude', selected, () => {}), /duplicate/);
    const linked = path.join(root, 'linked.json'); await fs.link(destination, linked);
    await assert.rejects(reviewLegacyJsonConfig('claude', selected, () => {}), /regular, unlinked/);
}));

test('reviewed legacy replacement refuses edits, forged proposals and revoked workspace trust', async () => fixture(async root => {
    const legacy = { ...spec, root, command: path.join(root, 'airk.exe') };
    const selected = { ...legacy, service: { installDir: path.join(root, 'install'), project: 'c'.repeat(64) } };
    const destination = jsonConfigPath('claude', selected);
    await installJsonConfig('claude', legacy, () => {}, async () => assert.fail('new configuration'));
    const review = await reviewLegacyJsonConfig('claude', selected, () => {});
    const save = async () => assert.fail('stale or unapproved proposal must not write');
    await assert.rejects(replaceReviewedLegacyJsonConfig('claude', selected, { ...review }, () => {}, save), /expired/);
    await assert.rejects(replaceReviewedLegacyJsonConfig('claude', selected, review, () => { throw new Error('trust revoked'); }, save), /trust revoked/);
    await fs.writeFile(destination, review.before + '\n// Concurrent edit\n');
    await assert.rejects(replaceReviewedLegacyJsonConfig('claude', selected, review, () => {}, save), /expired/);
    assert.match(await fs.readFile(destination, 'utf8'), /Concurrent edit/);
}));

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
