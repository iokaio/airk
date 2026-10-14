// SPDX-License-Identifier: Apache-2.0
// VS Code extension-test entry point, deliberately excluded from the VSIX.
import { strict as assert } from 'node:assert';
import * as vscode from 'vscode';
import * as path from 'node:path';
import * as fs from 'node:fs/promises';
import { probeMcp, runJson } from '../native';
import { LaunchSpec } from '../model';

export async function run(): Promise<void> {
    const fixture = process.env.AIRK_EDITOR_FIXTURE;
    if (!fixture || path.basename(fixture) !== 'review-workspace') { throw new Error('Editor tests require the generated review fixture.'); }
    const extension = vscode.extensions.getExtension('iokaio.airk'); assert.ok(extension, 'Packaged extension discovered');
    const api = await extension.activate();
    const commands = await vscode.commands.getCommands(true);
    for (const name of ['airk.index', 'airk.reindex', 'airk.chooseStorage', 'airk.collections', 'airk.setupAgent', 'airk.verifyMcp', 'airk.status', 'airk.usage']) {
        assert.ok(commands.includes(name), name);
    }
    await vscode.commands.executeCommand('airk.pause');
    await vscode.workspace.getConfiguration('airk').update('index.mode', 'manual', vscode.ConfigurationTarget.Global);
    const definitions: vscode.McpStdioServerDefinition[] = await api.getServerDefinitions();
    assert.equal(definitions.length, 1); assert.equal(await fs.realpath(definitions[0].cwd!.fsPath), await fs.realpath(fixture));
    assert.ok(definitions[0].command.includes('helpers'), 'Stable per-user native helper cache');
    const args = definitions[0].args;
    const spec: LaunchSpec = { command: definitions[0].command, root: args[args.indexOf('--root') + 1],
        dataDir: args[args.indexOf('--data-dir') + 1], collection: args[args.indexOf('--collection') + 1], name: 'Editor fixture' };
    // Both refresh coverage and indexing publish collection metadata. Exercise
    // simultaneous commands against the packaged native helper on a fresh root.
    await Promise.all([
        vscode.commands.executeCommand('airk.status'),
        vscode.commands.executeCommand('airk.index')
    ]);
    const status = await runJson<{ generation: number; collection: string; files: number }>(spec, 'status');
    assert.equal(status.collection, spec.collection); assert.ok(status.files >= 2, 'Fixture sources indexed');
    const symbols = await runJson<{ facts: unknown[] }>(spec, 'symbol', ['authenticate']); assert.equal(symbols.facts.length, 1);
    await vscode.commands.executeCommand('airk.reindex');
    assert.ok((await runJson<{ generation: number }>(spec, 'status')).generation > status.generation);
    assert.equal((await probeMcp(spec)).tools.length, 14);
    const selected = path.join(path.dirname(fixture), 'editor-selected-storage-' + Date.now()); await fs.mkdir(selected);
    await vscode.workspace.getConfiguration('airk').update('storage.path', selected, vscode.ConfigurationTarget.Global);
    const moved: vscode.McpStdioServerDefinition[] = await api.getServerDefinitions();
    assert.equal(moved[0].args[moved[0].args.indexOf('--data-dir') + 1], await fs.realpath(selected));
    await vscode.commands.executeCommand('airk.index');
    assert.ok((await fs.stat(path.join(spec.dataDir, 'collections', spec.collection, 'index.sqlite3'))).isFile(), 'Old index preserved after storage selection');
    assert.ok((await fs.stat(path.join(selected, 'collections', spec.collection, 'index.sqlite3'))).isFile(), 'New index stored on selected drive path');
    console.log('AIRK packaged extension-host smoke passed: commands, helper, full reindex, MCP and storage selection.');
}
