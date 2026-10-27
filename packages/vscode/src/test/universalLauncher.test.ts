// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { createHash } from 'node:crypto';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { test } from 'node:test';
import { parse } from 'jsonc-parser';
import { migrateAgentConfig } from '../agentMigration';
import { launchDescriptorPath } from '../explorerAssistanceLaunch';
import { assistanceEntry, launchArgs, LaunchSpec, retrievalEntry, serializeServers } from '../model';
import { probeMcp } from '../native';
import { NativeServiceFixture } from './nativeServiceFixture';

const installDir = path.resolve('AIRK', 'service-1.4');
const universal = path.resolve('AIRK', 'launcher', process.platform === 'win32' ? 'airk-launcher.exe' : 'airk-launcher');
const perInstall = path.join(installDir, process.platform === 'win32' ? 'airk-launcher.exe' : 'airk-launcher');
const base = { root: path.resolve('source'), dataDir: path.resolve('data'), collection: 'project-fixture', name: 'fixture' };
const project = 'ab'.repeat(32);
const universalSpec: LaunchSpec = { ...base, command: universal, service: { installDir, project, universal: true } };
const installSpec: LaunchSpec = { ...base, command: perInstall, service: { installDir, project } };
const storage = path.resolve('editor-storage');

test('the universal launcher keeps every versioned install path out of agent entries', () => {
    assert.deepEqual(launchArgs(universalSpec), ['--root', base.root, '--data-dir', base.dataDir, '--collection', base.collection, '--service-project', project, 'mcp']);
    assert.deepEqual(launchArgs(installSpec).slice(0, 5), ['runtime', 'launch', '--install-dir', installDir, '--'], 'without it, the per-installation shape is unchanged');
    assert.deepEqual(assistanceEntry(universalSpec, launchDescriptorPath(storage))!.args, ['explorer-assistance', '--descriptor', launchDescriptorPath(storage)]);
    for (const text of [serializeServers('claude', [retrievalEntry(universalSpec), assistanceEntry(universalSpec, launchDescriptorPath(storage))!]), serializeServers('codex', [retrievalEntry(universalSpec)])]) {
        assert.ok(!text.includes('service-1.4'), 'no versioned install path in the configuration');
    }
});

test('migration moves this installation\'s per-install launcher entries to the universal launcher, for both connections', () => {
    const before = serializeServers('claude', [retrievalEntry(installSpec), assistanceEntry(installSpec, launchDescriptorPath(storage))!]);
    const after = parse(migrateAgentConfig('claude', universalSpec, storage, before)).mcpServers;
    assert.deepEqual(after['airk-project-fixture'], { command: universal, args: launchArgs(universalSpec) });
    assert.deepEqual(after.airk_explorer, { command: universal, args: ['explorer-assistance', '--descriptor', launchDescriptorPath(storage)] });

    const toml = serializeServers('codex', [retrievalEntry(installSpec), assistanceEntry(installSpec, launchDescriptorPath(storage))!]);
    const migrated = migrateAgentConfig('codex', universalSpec, storage, toml);
    assert.ok(!migrated.includes('service-1.4'), migrated);
    assert.equal(migrateAgentConfig('codex', universalSpec, storage, migrated), migrated, 'idempotent');

    const foreign = JSON.stringify({ mcpServers: { airk_explorer: { command: path.resolve('elsewhere', 'node'), args: ['server.js'] } } });
    assert.equal(migrateAgentConfig('claude', universalSpec, storage, foreign), foreign, 'an Assistance entry this extension did not write is left alone');
    const otherInstall = serializeServers('claude', [retrievalEntry({ ...installSpec, command: path.resolve('other', 'airk-launcher'), service: { installDir: path.resolve('other'), project } })]);
    assert.throws(() => migrateAgentConfig('claude', universalSpec, storage, otherInstall), /Unrecognized AIRK connection/, 'another installation\'s launcher is never adopted');
});

test('a real universal launcher reads its pointer on every launch and forwards to the installation\'s launcher', { skip: !process.env.AIRK_TEST_BINARY, timeout: 300000 }, async () => {
    const fixture = await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'airk-universal-'));
    try {
        const spec = await fixture.register('universal');
        const launcherDir = path.join(directory, 'launcher');
        fs.mkdirSync(launcherDir, { mode: 0o700 });
        const executable = path.join(launcherDir, path.basename(spec.command));
        fs.copyFileSync(spec.command, executable);
        const sha = createHash('sha256').update(fs.readFileSync(executable)).digest('hex');
        const pointer = (install: string) => fs.writeFileSync(path.join(launcherDir, 'airk-launcher.json'), JSON.stringify({ schema: 1, install, launcher_sha256: sha }), { mode: 0o600 });
        pointer(spec.service!.installDir);
        const universalCall: LaunchSpec = { ...spec, command: executable, service: { ...spec.service!, universal: true } };
        assert.equal((await probeMcp(universalCall)).tools.length, 16, 'the universal launcher reaches the installed service');
        pointer(path.join(directory, 'missing-install'));
        await assert.rejects(probeMcp(universalCall), 'a pointer to a missing installation fails instead of guessing');
        pointer(spec.service!.installDir);
        assert.equal((await probeMcp(universalCall)).tools.length, 16, 'the next launch follows the rewritten pointer');
    } finally {
        fs.rmSync(directory, { recursive: true, force: true });
        await fixture.dispose();
    }
});
