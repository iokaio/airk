// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { inspectAgentConfiguration } from '../agentDiagnostics';
import { configFor, LaunchSpec } from '../model';

async function fixture(run: (spec: LaunchSpec, codexHome: string, directory: string) => Promise<void>): Promise<void> {
    const directory = await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(), 'airk-agent-diagnostics-')));
    const root = path.join(directory, 'project'), codexHome = path.join(directory, 'codex-user');
    await fs.mkdir(root); await fs.mkdir(codexHome);
    const spec = { root, command: path.join(directory, 'helpers', '1.2.4-win32-x64-current', 'airk.exe'),
        dataDir: path.join(directory, 'index'), collection: 'project-fixture', name: 'Fixture' };
    try { await run(spec, codexHome, directory); }
    finally {
        assert.equal(path.dirname(directory), await fs.realpath(os.tmpdir()));
        assert.ok(path.basename(directory).startsWith('airk-agent-diagnostics-'));
        await fs.rm(directory, { recursive: true, force: true });
    }
}
async function write(destination: string, source: string): Promise<void> {
    await fs.mkdir(path.dirname(destination), { recursive: true }); await fs.writeFile(destination, source);
}

test('diagnostics identify saved old helper paths in project and user configs without running or rewriting them', async () => fixture(async (spec, codexHome, directory) => {
    const old = { ...spec, command: path.join(directory, 'helpers', '1.2.3-win32-x64-5a56b1a3136dba4b', 'airk.exe') };
    const codex = path.join(spec.root, '.codex', 'config.toml'), user = path.join(codexHome, 'config.toml'), claude = path.join(spec.root, '.mcp.json');
    const unrelated = '\n[mcp_servers.other]\ncommand = "PRIVATE_COMMAND"\nargs = ["PRIVATE_ARG"]\n';
    await write(codex, configFor('codex', old) + unrelated);
    await write(user, configFor('codex', old));
    const json = JSON.parse(configFor('claude', old));
    json.mcpServers[`airk-${spec.collection}`].env = { API_KEY: 'PRIVATE_SECRET' };
    json.mcpServers.other = { command: 'PRIVATE_OTHER_COMMAND', args: ['PRIVATE_OTHER_ARG'] };
    await write(claude, JSON.stringify(json));
    const before = await Promise.all([codex, user, claude].map(file => fs.readFile(file)));
    const report = await inspectAgentConfiguration(spec, { codexHome });
    assert.equal(report.state, 'checked'); assert.equal(report.connection, 'not_verified');
    assert.equal(report.entries.length, 3);
    assert.ok(report.entries.every(entry => entry.status === 'different_helper' && entry.helperPath === old.command && entry.helperVersion === '1.2.3'));
    assert.ok(report.entries.some(entry => entry.scope === 'user'));
    assert.ok(!JSON.stringify(report).includes('PRIVATE_'));
    assert.deepEqual(await Promise.all([codex, user, claude].map(file => fs.readFile(file))), before);
    await assert.rejects(fs.stat(spec.command), /ENOENT/, 'The configured helper need not exist and must never be executed by this probe');
}));

test('agent helper and binding comparisons stay scoped to the selected collection and explicit launch fields', async () => fixture(async (spec, codexHome, directory) => {
    const same = { ...spec, command: process.platform === 'win32' ? path.toNamespacedPath(spec.command).toUpperCase() : spec.command };
    await write(path.join(spec.root, '.vscode', 'mcp.json'), configFor('vscode', same));
    await write(path.join(spec.root, '.mcp.json'), configFor('claude', { ...spec, dataDir: path.join(directory, 'other-index') }));
    const cursor = JSON.parse(configFor('cursor', spec)); cursor.mcpServers[`airk-${spec.collection}`].command = 'airk';
    cursor.mcpServers['airk-other'] = JSON.parse(configFor('claude', { ...spec, collection: 'other' })).mcpServers['airk-other'];
    await write(path.join(spec.root, '.cursor', 'mcp.json'), JSON.stringify(cursor));
    const gemini = JSON.parse(configFor('gemini', spec)); gemini.mcpServers[`airk-${spec.collection}`].url = 'https://PRIVATE_ENDPOINT';
    await write(path.join(spec.root, '.gemini', 'settings.json'), JSON.stringify(gemini));
    const alternate = JSON.parse(configFor('antigravity', spec));
    alternate.mcpServers[`airk-${spec.collection}`].args.splice(-1, 0, '--root=' + path.join(directory, 'other-project'));
    await write(path.join(spec.root, '.agents', 'mcp_config.json'), JSON.stringify(alternate));
    await write(path.join(spec.root, '.codex', 'config.toml'), configFor('codex', spec) + 'url = "https://PRIVATE_CODEX_ENDPOINT"\n');
    const report = await inspectAgentConfiguration(spec, { codexHome });
    assert.equal(report.entries.length, 6);
    assert.equal(report.entries.find(entry => entry.client === 'vscode')?.status, 'matches');
    assert.equal(report.entries.find(entry => entry.client === 'claude')?.status, 'different_binding');
    assert.equal(report.entries.find(entry => entry.client === 'cursor')?.status, 'unresolved');
    assert.equal(report.entries.find(entry => entry.client === 'gemini')?.status, 'unresolved');
    assert.equal(report.entries.find(entry => entry.client === 'antigravity')?.status, 'unresolved');
    assert.equal(report.entries.find(entry => entry.client === 'codex')?.status, 'unresolved');
    assert.ok(!JSON.stringify(report).includes('PRIVATE_ENDPOINT'));
    assert.ok(!JSON.stringify(report).includes('PRIVATE_CODEX_ENDPOINT'));
    assert.ok(!JSON.stringify(report).includes('airk-other'));
}));

test('ambiguous JSON, unsupported TOML, redirected directories and oversized configs are reported as partial', async () => fixture(async (spec, codexHome, directory) => {
    await write(path.join(spec.root, '.mcp.json'), '{"mcpServers":{},"mcpServers":{}}');
    await write(path.join(codexHome, 'config.toml'), '[mcp_servers]\n"airk-project-fixture" = { command = "airk", args = [] }\n');
    await write(path.join(spec.root, '.cursor', 'mcp.json'), 'x'.repeat(1_048_577));
    const redirected = path.join(directory, 'redirected'); await fs.mkdir(redirected);
    await write(path.join(redirected, 'config.toml'), configFor('codex', { ...spec, command: path.join(directory, 'PRIVATE_REDIRECTED', 'airk.exe') }));
    await fs.symlink(redirected, path.join(spec.root, '.codex'), process.platform === 'win32' ? 'junction' : 'dir');
    const report = await inspectAgentConfiguration(spec, { codexHome });
    assert.equal(report.state, 'partial'); assert.equal(report.issues.length, 4); assert.equal(report.entries.length, 0);
    assert.ok(!JSON.stringify(report).includes('PRIVATE_REDIRECTED'));
}));

test('configuration checks stop after lost trust and bound reported AIRK entries', async () => fixture(async (spec, codexHome) => {
    await write(path.join(spec.root, '.vscode', 'mcp.json'), configFor('vscode', spec));
    const stopped = await inspectAgentConfiguration(spec, { codexHome, shouldContinue: () => false });
    assert.equal(stopped.state, 'not_checked'); assert.equal(stopped.filesChecked, 0);
    let checks = 0;
    const changed = await inspectAgentConfiguration(spec, { codexHome, shouldContinue: () => ++checks < 3 });
    assert.equal(changed.state, 'partial'); assert.equal(changed.filesChecked, 1); assert.deepEqual(changed.entries, []);
    const servers: Record<string, unknown> = {};
    const entry = JSON.parse(configFor('claude', spec)).mcpServers[`airk-${spec.collection}`];
    for (let index = 0; index < 40; index++) { servers['airk-alias-' + index] = entry; }
    await write(path.join(spec.root, '.mcp.json'), JSON.stringify({ mcpServers: servers }));
    const bounded = await inspectAgentConfiguration(spec, { codexHome });
    assert.equal(bounded.entries.length, 32); assert.equal(bounded.truncated, true); assert.equal(bounded.state, 'partial');
}));
