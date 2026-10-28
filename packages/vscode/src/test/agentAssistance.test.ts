// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import * as path from 'node:path';
import { test } from 'node:test';
import { parse } from 'jsonc-parser';
import { mergeJsonConfig } from '../agentConfig';
import { connectionAction, renderConnectionGuide } from '../agentConnections';
import { migrateAgentConfig } from '../agentMigration';
import { ASSISTANCE_KEY, assistanceEntry, clients, configFor, LaunchSpec, retrievalEntry, unifiedEntry, serializeServers } from '../model';

const spec: LaunchSpec = { command: path.resolve('installed', 'airk-launcher'), root: path.resolve('source'), dataDir: path.resolve('data'),
    collection: 'project-fixture', name: 'Fixture', service: { installDir: path.resolve('installed'), project: 'ab'.repeat(32) } };
const descriptor = path.resolve('storage', 'explorer-assistance', 'launch-1.json');
const assistance = assistanceEntry(spec, descriptor)!;
const both = [retrievalEntry(spec), assistance];

test('Explorer Assistance is a second, separately keyed launcher entry that never reaches retrieval', () => {
    assert.equal(assistance.key, ASSISTANCE_KEY);
    assert.notEqual(assistance.key, retrievalEntry(spec).key, 'airk_explorer can never equal an airk-<collection> key');
    assert.equal(assistance.command, spec.command, 'the stable launcher, not a system Node');
    assert.deepEqual(assistance.args, ['runtime', 'launch', '--install-dir', spec.service!.installDir, '--', 'explorer-assistance', '--descriptor', descriptor]);
    assert.ok(!assistance.args.includes('--service-project') && !assistance.args.includes('mcp'), 'no project binding or retrieval subcommand');
    assert.equal(assistanceEntry({ ...spec, service: undefined }, descriptor), undefined, 'unregistered workspaces have no Assistance entry');
    assert.equal(assistanceEntry(spec, 'relative/launch-1.json'), undefined);
});

test('every supported client format has one named endpoint with evidence and optional UI adapter', () => {
    const entry=unifiedEntry(spec,descriptor);
    for (const client of clients) {
        const text=serializeServers(client,[entry]);
        assert.ok(!text.includes(ASSISTANCE_KEY)&&!text.includes(retrievalEntry(spec).key),client);
        assert.ok(text.includes('--explorer-descriptor'),client);
        assert.equal(configFor(client,spec),serializeServers(client,[unifiedEntry(spec)]));
    }
    assert.deepEqual(Object.keys(JSON.parse(serializeServers('claude',[entry])).mcpServers),['airk']);
    assert.deepEqual(Object.keys(JSON.parse(serializeServers('vscode',[entry])).servers),['airk']);
    const other={...spec,service:{...spec.service!,project:'cd'.repeat(32)}};
    const multi=unifiedEntry(spec,descriptor,[other]);
    assert.ok(multi.args.includes('--no-default-project'));
    assert.deepEqual(multi.args.slice(-3),['--additional-project',other.service.project,'--no-default-project']);
    assert.throws(()=>unifiedEntry(spec,descriptor,[spec]),/distinct bindings/);
});

test('JSON installation writes one endpoint and refuses conflicting custom settings',()=>{
    const entry=unifiedEntry(spec,descriptor);
    const source='{\n  // policy\n  "mcpServers": {"other":{"command":"x"}}\n}\n';
    const after=mergeJsonConfig('claude',spec,source,[entry]);
    assert.ok(after.includes('// policy'));
    assert.deepEqual(Object.keys(parse(after).mcpServers),['other','airk']);
    assert.equal(mergeJsonConfig('claude',spec,after,[entry]),after);
    assert.throws(()=>mergeJsonConfig('claude',spec,after.replace('"--explorer-descriptor"','"--custom"'),[entry]),/different settings/);
});

test('connection guide describes a single catalog and one connection check',()=>{
    const html=renderConnectionGuide('claude',spec,'Install.',undefined,false,false,[unifiedEntry(spec,descriptor)]);
    assert.ok(html.includes('One AIRK connection'));
    assert.ok(html.includes('data-action="verify"')&&!html.includes('data-action="verifyAssistance"'));
    assert.ok(!html.includes('separately keyed')&&!html.includes('both connections'));
    assert.equal(connectionAction({action:'runAssistanceCommand'},'codex'),undefined);
    const storage=path.resolve('storage');
    const after=migrateAgentConfig('claude',spec,storage,serializeServers('claude',both));
    assert.deepEqual(Object.keys(parse(after).mcpServers),['airk']);
});
