// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import Module = require('node:module');
import { test } from 'node:test';
import { AGENT_INSTRUCTION_FILES, AIRK_AGENT_INSTRUCTIONS, parseWebviewMessage } from '../explorerContracts';

class Uri {
    constructor(readonly fsPath: string, readonly scheme = 'file') {}
    static file(value: string) { return new Uri(value); }
    with(change: { scheme: string }) { return new Uri(this.fsPath, change.scheme); }
    toString() { return `${this.scheme}:${this.fsPath}`; }
}
function fixture(t: { after(callback: () => void): void }) {
    const root = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), 'airk-instructions-')));
    t.after(() => fs.rmSync(root, { recursive: true, force: true }));
    const opened: Uri[] = [], shown: unknown[] = [];
    let afterOpen = () => {};
    const existing = { uri: Uri.file(path.join(root, 'AGENTS.md')), isDirty: true, text: 'unsaved user rules' };
    const stub = {
        Uri, ViewColumn: { Beside: 2 },
        workspace: { isTrusted: true, workspaceFolders: [{ uri: Uri.file(root) }],
            openTextDocument: async (uri: Uri) => { opened.push(uri); afterOpen(); return uri.toString() === existing.uri.toString() ? existing : { uri }; } },
        window: { showTextDocument: async (document: unknown) => { shown.push(document); } },
    };
    const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
    loader._load = function (name: string, ...args: any[]) { return name === 'vscode' ? stub : original.call(this, name, ...args); };
    delete require.cache[require.resolve('../explorerWorkspaceIO')];
    let io: typeof import('../explorerWorkspaceIO');
    try { io = require('../explorerWorkspaceIO'); } finally { loader._load = original; }
    return { root, stub, opened, shown, existing, io, onOpen(callback: () => void) { afterOpen = callback; } };
}

test('instruction buttons open only the named project files; missing files stay unsaved and folders are not created', async t => {
    const f = fixture(t);
    for (const agent of Object.keys(AGENT_INSTRUCTION_FILES) as (keyof typeof AGENT_INSTRUCTION_FILES)[]) {
        const result = await f.io.openAgentInstructions(f.root, agent, () => true);
        const uri = f.opened.at(-1)!;
        assert.equal(uri.scheme, 'untitled');
        assert.equal(uri.fsPath, path.join(f.root, AGENT_INSTRUCTION_FILES[agent].file));
        assert.match(result.message, /unsaved/);
    }
    assert.deepEqual(fs.readdirSync(f.root), []);
    assert.equal(f.shown.length, 3);
});

test('existing instructions and dirty editor contents are preserved', async t => {
    const f = fixture(t), file = path.join(f.root, 'AGENTS.md');
    fs.writeFileSync(file, 'saved user rules\r\n');
    const result = await f.io.openAgentInstructions(f.root, 'codex', () => true);
    assert.match(result.message, /alongside your existing/);
    assert.equal(f.opened[0].scheme, 'file');
    assert.equal(f.shown[0], f.existing);
    assert.equal(f.existing.text, 'unsaved user rules');
    assert.equal(fs.readFileSync(file, 'utf8'), 'saved user rules\r\n');
});

test('instruction opening refuses other roots, invalid agents, lost trust and superseded selection', async t => {
    const f = fixture(t);
    await assert.rejects(f.io.openAgentInstructions(path.dirname(f.root), 'codex', () => true), /workspace folder/);
    await assert.rejects(f.io.openAgentInstructions(f.root, '../outside' as 'codex', () => true), /supported/);
    f.stub.workspace.isTrusted = false;
    await assert.rejects(f.io.openAgentInstructions(f.root, 'codex', () => true), /changed/);
    f.stub.workspace.isTrusted = true;
    let current = true;
    f.onOpen(() => { current = false; });
    await assert.rejects(f.io.openAgentInstructions(f.root, 'claude', () => current), /changed/);
    assert.deepEqual(f.shown, []);
    assert.deepEqual(fs.readdirSync(f.root), []);
});

test('instruction paths cannot follow a redirected parent or open a directory as a file', async t => {
    const f = fixture(t), other = path.join(f.root, 'other');
    fs.mkdirSync(other);
    fs.symlinkSync(other, path.join(f.root, '.github'), process.platform === 'win32' ? 'junction' : 'dir');
    await assert.rejects(f.io.openAgentInstructions(f.root, 'copilot', () => true), /redirected/);
    fs.mkdirSync(path.join(f.root, 'CLAUDE.md'));
    await assert.rejects(f.io.openAgentInstructions(f.root, 'claude', () => true), /regular/);
    assert.deepEqual(f.opened, []);
});

test('instruction messages bind a project and fixed agent rather than accepting webview paths', () => {
    const message = { v: 1, type: 'instructions.open', requestId: 'test', project: 'a'.repeat(64), agent: 'copilot' };
    assert.deepEqual(parseWebviewMessage(message), message);
    assert.equal(parseWebviewMessage({ ...message, path: '/elsewhere' }), undefined);
    assert.equal(parseWebviewMessage({ ...message, agent: 'shell' }), undefined);
    assert.equal(parseWebviewMessage({ ...message, project: '' }), undefined);
    assert.match(AIRK_AGENT_INSTRUCTIONS, /deferred tool discovery/);
    assert.match(AIRK_AGENT_INSTRUCTIONS, /verify relevant live file contents before edits/);
});
