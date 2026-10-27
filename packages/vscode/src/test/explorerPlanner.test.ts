// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { createHash } from 'node:crypto';
import * as fs from 'node:fs';
import Module from 'node:module';
import * as os from 'node:os';
import * as path from 'node:path';
import { test } from 'node:test';

/** Minimal file-scheme URI with the members the plan save flow uses. */
class Uri {
    constructor(readonly scheme: string, readonly authority: string, readonly path: string) {}
    get fsPath(): string { return process.platform === 'win32' ? this.path.slice(1).replaceAll('/', '\\') : this.path; }
    static file(value: string): Uri { const posix = value.replaceAll('\\', '/'); return new Uri('file', '', posix.startsWith('/') ? posix : `/${posix}`); }
    static joinPath(base: Uri, ...parts: string[]): Uri { return new Uri(base.scheme, base.authority, path.posix.join(base.path, ...parts)); }
    toString(): string { return `${this.scheme}://${this.authority}${this.path}`; }
}

interface Script { save?: Uri; replace?: boolean; dirty?: string[]; directory?: string }
/** Load explorerWorkspaceIO against a stub editor whose folder is a real temporary directory. */
function load(root: string, script: Script) {
    const events: string[] = [];
    const stub = {
        Uri, FileType: { File: 1, Directory: 2 },
        workspace: {
            workspaceFolders: [{ name: 'fixture', uri: Uri.file(root) }],
            textDocuments: (script.dirty ?? []).map(file => ({ uri: Uri.file(file), isDirty: true })),
            getConfiguration: () => ({ get: (_key: string, fallback: unknown) => script.directory ?? fallback }),
            fs: {
                stat: async (uri: Uri) => ({ type: fs.statSync(uri.fsPath).isFile() ? 1 : 2 }),
                createDirectory: async (uri: Uri) => { fs.mkdirSync(uri.fsPath, { recursive: true }); },
                writeFile: async (uri: Uri, bytes: Uint8Array) => { events.push(`write ${uri.fsPath}`); fs.writeFileSync(uri.fsPath, bytes); },
                readFile: async (uri: Uri) => fs.readFileSync(uri.fsPath),
            },
        },
        window: {
            showSaveDialog: async (options: { defaultUri: Uri }) => { events.push(`dialog ${options.defaultUri.fsPath}`); return script.save; },
            showWarningMessage: async (text: string) => { events.push(`confirm ${text}`); return script.replace ? 'Replace' : undefined; },
            showTextDocument: async (uri: Uri) => { events.push(`open ${uri.fsPath}`); },
            showQuickPick: async () => undefined,
        },
    };
    const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
    loader._load = function (request: string, ...args: any[]) { return request === 'vscode' ? stub : original.call(this, request, ...args); };
    delete require.cache[require.resolve('../explorerWorkspaceIO')];
    try { return { io: require('../explorerWorkspaceIO') as typeof import('../explorerWorkspaceIO'), events }; }
    finally { loader._load = original; }
}

test('plans are saved only through the dialog, inside the workspace, never over unsaved or unconfirmed files', async t => {
    const root = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), 'airk-plan-')));
    t.after(() => fs.rmSync(root, { recursive: true, force: true }));
    const target = path.join(root, '.airk', 'plans', 'renewal.md');

    const canceled = load(root, {});
    assert.deepEqual(await canceled.io.savePlanInWorkspace('# plan\n', 'renewal.md'), { saved: false, reason: 'Save was canceled; nothing was written.' });
    assert.deepEqual(canceled.events, [`dialog ${target}`], 'the dialog opens in .airk/plans by default');

    const outside = load(root, { save: Uri.file(path.join(path.dirname(root), 'elsewhere.md')) });
    assert.match((await outside.io.savePlanInWorkspace('# plan\n', 'renewal.md') as { reason: string }).reason, /inside the chosen workspace folder/);
    assert.ok(!outside.events.some(event => event.startsWith('write')));

    const saved = load(root, { save: Uri.file(target) });
    const result = await saved.io.savePlanInWorkspace('# plan\n', 'renewal.md');
    assert.deepEqual(result, { saved: true, path: '.airk/plans/renewal.md', hash: createHash('sha256').update('# plan\n').digest('hex') });
    assert.equal(fs.readFileSync(target, 'utf8'), '# plan\n');
    assert.ok(saved.events.includes(`open ${target}`), 'the saved file opens in the editor');

    const keep = load(root, { save: Uri.file(target), replace: false });
    assert.match((await keep.io.savePlanInWorkspace('# changed\n', 'renewal.md') as { reason: string }).reason, /existing file was kept/);
    assert.equal(fs.readFileSync(target, 'utf8'), '# plan\n');

    const dirty = load(root, { save: Uri.file(target), replace: true, dirty: [target] });
    assert.match((await dirty.io.savePlanInWorkspace('# changed\n', 'renewal.md') as { reason: string }).reason, /unsaved edits/);
    assert.equal(fs.readFileSync(target, 'utf8'), '# plan\n');

    const replace = load(root, { save: Uri.file(target), replace: true });
    assert.equal((await replace.io.savePlanInWorkspace('# changed\n', 'renewal.md') as { saved: boolean }).saved, true);
    assert.equal(fs.readFileSync(target, 'utf8'), '# changed\n');

    const custom = load(root, { directory: '../escape' });
    await custom.io.savePlanInWorkspace('# plan\n', 'renewal.md');
    assert.deepEqual(custom.events, [`dialog ${target}`], 'an unsafe directory setting falls back to .airk/plans');
});
