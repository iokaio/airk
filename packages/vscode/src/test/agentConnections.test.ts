// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { agentChoices, connectionAction, createCodexConfig, renderConnectionGuide } from '../agentConnections';
import { LaunchSpec, configFor } from '../model';

const spec: LaunchSpec = { command: 'C:\\Program Files\\AIRK\\airk.exe', root: '/fixture', dataDir: '/index with spaces', collection: 'project-fixture', name: 'Fixture' };

test('Codex setup preserves launch identity, existing config and concurrent creators', async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-agent-setup-'));
    try {
        const selected = { ...spec, root: await fs.realpath(directory) };
        const outcomes = await Promise.allSettled([createCodexConfig(selected), createCodexConfig(selected)]);
        assert.equal(outcomes.filter(result => result.status === 'fulfilled').length, 1);
        const destination = path.join(directory, '.codex', 'config.toml');
        assert.equal(await fs.readFile(destination, 'utf8'), configFor('codex', selected));
        const existing = '# User settings and approvals\r\nmodel = "fixture"\r\n[mcp_servers.other]\r\ncommand = "other"\r\n';
        await fs.writeFile(destination, existing);
        await assert.rejects(createCodexConfig(selected), /already exists and was preserved/);
        assert.equal(await fs.readFile(destination, 'utf8'), existing);
    } finally {
        const resolved = await fs.realpath(directory);
        assert.equal(path.dirname(resolved), await fs.realpath(os.tmpdir()));
        assert.ok(path.basename(resolved).startsWith('airk-agent-setup-'));
        await fs.rm(resolved, { recursive: true, force: true });
    }
});

test('Codex setup refuses redirected configuration directories', async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-agent-setup-'));
    try {
        const root = path.join(directory, 'workspace'), outside = path.join(directory, 'elsewhere');
        await fs.mkdir(root); await fs.mkdir(outside);
        await fs.symlink(outside, path.join(root, '.codex'), process.platform === 'win32' ? 'junction' : 'dir');
        await assert.rejects(createCodexConfig({ ...spec, root }), /regular directory inside this workspace/);
        assert.deepEqual(await fs.readdir(outside), []);
    } finally {
        const resolved = await fs.realpath(directory);
        assert.equal(path.dirname(resolved), await fs.realpath(os.tmpdir()));
        assert.ok(path.basename(resolved).startsWith('airk-agent-setup-'));
        await fs.rm(resolved, { recursive: true, force: true });
    }
});

test('guide separates client setup from helper checks and escapes all workspace content', () => {
    const html = renderConnectionGuide('codex', { ...spec, name: '<img>', root: '</pre><script>alert(1)</script>' }, '<instructions>', '<error>');
    assert.match(html, /Create Codex project configuration/);
    assert.match(html, /restart the extension/);
    assert.match(html, /cannot confirm that Codex loaded/);
    assert.match(html, /airk_status/);
    assert.match(html, /&lt;error&gt;/);
    assert.match(html, /&lt;instructions&gt;/);
    assert.doesNotMatch(html, /<script>alert/);
    assert.match(html, /default-src 'none'/);
    assert.doesNotMatch(renderConnectionGuide('codex', spec, '', '', true), /data-action="createCodex"/);
    assert.doesNotMatch(renderConnectionGuide('codex', spec, '', 'Checking…', false, true), /<button data-action="[^"]+">/);
    assert.match(renderConnectionGuide('vscode', spec, ''), /This controls Copilot Chat/);
    assert.ok(agentChoices().some(item => item.client === 'codex' && item.label === 'Codex'));
});

test('service guide makes legacy review explicit and describes MCP verification as read-only', () => {
    const selected = { ...spec, service: { installDir: path.resolve('/fixture/install'), project: 'a'.repeat(64) } };
    const html = renderConnectionGuide('claude', selected, 'Review configuration');
    assert.match(html, /data-action="reviewLegacy"/);
    assert.match(html, /without starting indexing/);
    assert.doesNotMatch(html, /helper check updates this collection/);
    assert.doesNotMatch(renderConnectionGuide('claude', selected, '', '', true), /data-action="reviewLegacy"/);
    assert.doesNotMatch(renderConnectionGuide('codex', selected, ''), /data-action="reviewLegacy"/);
    assert.match(renderConnectionGuide('codex', selected, ''), /manual review/);
    assert.equal(connectionAction({ action: 'reviewLegacy' }, 'claude'), 'reviewLegacy');
    assert.equal(connectionAction({ action: 'reviewLegacy' }, 'codex'), undefined);
});

test('guide messages cannot run arbitrary actions or another client’s installer', () => {
    for (const value of [null, {}, { action: 'delete' }, { action: ['createCodex'] }]) {
        assert.equal(connectionAction(value, 'codex'), undefined);
    }
    assert.equal(connectionAction({ action: 'createCodex' }, 'claude'), undefined);
    assert.equal(connectionAction({ action: 'vscode' }, 'codex'), undefined);
    assert.equal(connectionAction({ action: 'createCodex' }, 'codex'), 'createCodex');
    assert.equal(connectionAction({ action: 'installJson' }, 'codex'), undefined);
    assert.equal(connectionAction({ action: 'enableCopilot' }, 'gemini'), undefined);
    for (const client of ['claude', 'gemini', 'antigravity'] as const) {
        assert.equal(connectionAction({ action: 'installJson' }, client), 'installJson');
        assert.match(renderConnectionGuide(client, spec, ''), /data-action="installJson"/);
        assert.doesNotMatch(renderConnectionGuide(client, spec, '', '', true), /data-action="installJson"/);
    }
});
