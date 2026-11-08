// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import * as path from 'node:path';
import { test } from 'node:test';
import { parse } from 'jsonc-parser';
import { migrateAgentConfig } from '../agentMigration';
import { launchDescriptorPath } from '../explorerAssistanceLaunch';
import { assistanceEntry, launchArgs, LaunchSpec, retrievalEntry, unifiedEntry, serializeServers, UNIFIED_TOOLS } from '../model';
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
    assert.deepEqual(launchArgs(universalSpec), ['--root', base.root, 'mcp']);
    assert.deepEqual(launchArgs(installSpec).slice(0, 5), ['runtime', 'launch', '--install-dir', installDir, '--'], 'without it, the per-installation shape is unchanged');
    assert.deepEqual(assistanceEntry(universalSpec, launchDescriptorPath(storage))!.args, ['explorer-assistance', '--descriptor', launchDescriptorPath(storage)]);
    for (const text of [serializeServers('claude', [retrievalEntry(universalSpec), assistanceEntry(universalSpec, launchDescriptorPath(storage))!]), serializeServers('codex', [retrievalEntry(universalSpec)])]) {
        assert.ok(!text.includes('service-1.4'), 'no versioned install path in the configuration');
    }
});

test('migration moves this installation\'s per-install launcher entries to the universal launcher, and consolidates the former connections', () => {
    const before = serializeServers('claude', [retrievalEntry(installSpec), assistanceEntry(installSpec, launchDescriptorPath(storage))!]);
    const after = parse(migrateAgentConfig('claude', universalSpec, storage, before)).mcpServers;
    assert.deepEqual(Object.keys(after),['airk']);
    assert.deepEqual(after.airk, { command: universal, args: [...unifiedEntry(universalSpec,launchDescriptorPath(storage)).args] });

    const toml = serializeServers('codex', [retrievalEntry(installSpec), assistanceEntry(installSpec, launchDescriptorPath(storage))!]);
    const migrated = migrateAgentConfig('codex', universalSpec, storage, toml);
    assert.ok(!migrated.includes('service-1.4'), migrated);
    assert.equal(migrateAgentConfig('codex', universalSpec, storage, migrated), migrated, 'idempotent');

    const foreign = JSON.stringify({ mcpServers: { airk_explorer: { command: path.resolve('elsewhere', 'node'), args: ['server.js'] } } });
    assert.equal(migrateAgentConfig('claude', universalSpec, storage, foreign), foreign, 'an Assistance entry this extension did not write is left alone');
    const otherInstall = serializeServers('claude', [retrievalEntry({ ...installSpec, command: path.resolve('other', 'airk-launcher'), service: { installDir: path.resolve('other'), project } })]);
    assert.throws(() => migrateAgentConfig('claude', universalSpec, storage, otherInstall), /Unrecognized/, 'another installation\'s launcher is never adopted');
});

test('modern native installation retires stdio launchers and keeps HTTPS authority explicit', {skip:!process.env.AIRK_TEST_BINARY,timeout:300000},async()=>{
    const fixture=await NativeServiceFixture.create(process.env.AIRK_TEST_BINARY!);
    try{
        const spec=await fixture.register('modern-endpoint'),inspected=await fixture.run(['runtime','inspect','--install-dir',fixture.install]);
        assert.equal(inspected.launcher,null);assert.equal(fixture.launcher,null);assert.ok(spec.http);
        assert.equal((await probeMcp(spec)).tools.length,UNIFIED_TOOLS.length,'Actual reviewed native HTTPS endpoint');
        const {runText}=await import('../native');
        await assert.rejects(runText(fixture.binary,['runtime','launch','--install-dir',fixture.install,'--','mcp'],fixture.owner,15000),/stdio_mcp_retired/);
        const connection=await fixture.run(['runtime','http-connection-info','--install-dir',fixture.install,'--connection-id',spec.http!.connection_id]);
        const {validateHttpConnection}=await import('../serviceHttp'),observed=validateHttpConnection(connection,spec.http!.selection);
        assert.deepEqual(observed.endpoint,spec.http!.endpoint);assert.equal(observed.connection_id,spec.http!.connection_id);
        assert.equal(observed.credential_file,spec.http!.credential_file);assert.equal(observed.ca_certificate_file,spec.http!.ca_certificate_file);
        assert.equal((await probeMcp(spec)).tools.length,UNIFIED_TOOLS.length,'Read remains attached to exact native endpoint');
    }catch(error){await fixture.retainFailure(error);throw error;}finally{await fixture.dispose();}
});
