// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import * as path from 'node:path';
import { test } from 'node:test';
import { parse } from 'jsonc-parser';
import { mergeJsonConfig } from '../agentConfig';
import { connectionAction, renderConnectionGuide } from '../agentConnections';
import { migrateAgentConfig } from '../agentMigration';
import { ASSISTANCE_KEY, assistanceEntry, clients, configFor, LaunchSpec, retrievalEntry, serializeServers } from '../model';

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

test('every client format carries both connections, and the retrieval-only form is unchanged', () => {
    for (const client of clients) {
        const text = serializeServers(client, both);
        assert.ok(text.includes(ASSISTANCE_KEY) && text.includes(retrievalEntry(spec).key), client);
        assert.ok(text.includes('explorer-assistance'), client);
        assert.equal(serializeServers(client, [retrievalEntry(spec)]), configFor(client, spec), client);
    }
    const codex = serializeServers('codex', both);
    assert.ok(codex.startsWith('[mcp_servers.airk-project-fixture]\n') && codex.indexOf('\n\n[mcp_servers.airk_explorer]\ncommand = ') > 0, codex);
    assert.deepEqual(Object.keys(JSON.parse(serializeServers('claude', both)).mcpServers), ['airk-project-fixture', 'airk_explorer']);
    assert.deepEqual(Object.keys(JSON.parse(serializeServers('vscode', both)).servers), ['airk-project-fixture', 'airk_explorer']);
});

test('JSON installation adds both entries, keeps existing ones, and refuses any conflict without editing', () => {
    const source = '{\n  // policy\n  "mcpServers": {\n    "other": { "command": "x" }\n  }\n}\n';
    const merged = mergeJsonConfig('claude', spec, source, both);
    const servers = parse(merged).mcpServers;
    assert.deepEqual(Object.keys(servers), ['other', 'airk-project-fixture', 'airk_explorer']);
    assert.deepEqual(servers.airk_explorer, { command: assistance.command, args: assistance.args });
    assert.ok(merged.includes('// policy'));
    assert.equal(mergeJsonConfig('claude', spec, merged, both), merged, 'idempotent');
    const conflicting = merged.replace('"explorer-assistance"', '"something-else"');
    assert.throws(() => mergeJsonConfig('claude', spec, conflicting, both), /airk_explorer already has different settings/);
    const retrievalOnly = mergeJsonConfig('gemini', spec, '{}\n');
    assert.deepEqual(Object.keys(parse(retrievalOnly).mcpServers), ['airk-project-fixture'], 'callers that pass no entries keep the retrieval-only install');
});

test('the guide shows two labeled connections with separate checks, and migration leaves Assistance alone', () => {
    const html = renderConnectionGuide('claude', spec, 'Install.', undefined, false, false, both);
    assert.ok(html.includes('AIRK Retrieval (evidence)') && html.includes('AIRK Explorer Assistance (help and UI status)'));
    assert.ok(html.includes('data-action="verify"') && html.includes('data-action="verifyAssistance"') && html.includes('data-action="copyAssistancePrompt"'));
    assert.ok(!renderConnectionGuide('claude', spec, 'Install.').includes('data-action="verifyAssistance"'));
    assert.equal(connectionAction({ action: 'verifyAssistance' }, 'codex'), 'verifyAssistance');
    assert.equal(connectionAction({ action: 'runAssistanceCommand' }, 'codex'), undefined);

    const storage = path.resolve('storage');
    const current = serializeServers('claude', both);
    assert.deepEqual(parse(migrateAgentConfig('claude', spec, storage, current)), JSON.parse(current), 'a current two-connection file keeps both entries');
    const toml = serializeServers('codex', both), table = toml.slice(toml.indexOf('[mcp_servers.airk_explorer]'));
    assert.ok(migrateAgentConfig('codex', spec, storage, toml).endsWith(table), 'the Assistance table is never rewritten');
});
