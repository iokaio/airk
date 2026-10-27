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

interface Script { save?: Uri; open?: Uri; replace?: boolean; dirty?: string[]; directory?: string; confirm?(): void }

test('Studio Save round trips all inputs without a picker and guards edits, names and ownership', async t => {
    const root = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), 'airk-studio-save-')));
    t.after(() => fs.rmSync(root, { recursive: true, force: true }));
    const { CORE_PACK, STUDIO_SCHEMA, validateStudio } = await import('../explorerStrategies');
    const document: import('../explorerStrategies').StudioDocument = { schema: STUDIO_SCHEMA, strategy: CORE_PACK.find(entry => entry.strategy.id === 'airk.core/bug')!.strategy,
        objective: 'Review the Explorer UI', params: { query: 'Search', symbol: 'ReviewTestsScreen' }, pathPrefix: 'packages/vscode/src/', profile: 'mcp_aware' as const,
        editedPrompt: 'My edited instructions' };
    assert.ok(validateStudio(document));
    for (const bad of [{ ...document, run: {} }, { ...document, params: { foreign: 'value' } }, { ...document, pathPrefix: '../outside' },
        { ...document, objective: 'x'.repeat(4001) }, { ...document, profile: 'foreign' }]) { assert.equal(validateStudio(bad), undefined); }
    const f = load(root, {}), allowed = () => f.stub.workspace.isTrusted;
    const first = await f.io.saveStudioInWorkspace(document, 'ui-refinements', root, undefined, allowed);
    assert.ok(first.saved); if (!first.saved) { return; }
    assert.deepEqual(f.events, [], 'no folder picker, confirmation or editor navigation for a new save');
    const target = path.join(root, first.path), saved = { ...document, strategy: { ...document.strategy, id: 'user/ui-refinements' } };
    assert.deepEqual(JSON.parse(fs.readFileSync(target, 'utf8')), saved);
    assert.deepEqual(await f.io.loadWorkspaceFile('strategies', Uri.file(root).toString(), 'ui-refinements.studio.airk.json'), saved);
    assert.deepEqual(await load(root, { open: Uri.file(target) }).io.importStrategy(), saved);
    const updated = await f.io.saveStudioInWorkspace({ ...document, objective: 'Updated objective' }, 'ui-refinements', root, first.hash, allowed);
    assert.ok(updated.saved); assert.deepEqual(f.events, []);
    const changedBytes = fs.readFileSync(target, 'utf8');
    assert.equal((await f.io.saveStudioInWorkspace(document, 'ui-refinements', root, first.hash, allowed)).saved, false, 'outside/stale edits must not be overwritten');
    assert.equal(fs.readFileSync(target, 'utf8'), changedBytes);
    const dirty = load(root, { dirty: [target] });
    assert.equal((await dirty.io.saveStudioInWorkspace(document, 'ui-refinements', root, undefined, () => true)).saved, false);
    for (const name of ['../outside', 'con', 'a/b', 'with.dot']) { await assert.rejects(f.io.saveStudioInWorkspace(document, name, root, undefined, allowed)); }
    assert.equal((await f.io.saveStudioInWorkspace(document, 'foreign', path.join(root, 'missing'), undefined, allowed)).saved, false);
    f.stub.workspace.isTrusted = false;
    assert.equal((await f.io.saveStudioInWorkspace(document, 'untrusted', root, undefined, allowed)).saved, false);
    assert.equal(fs.existsSync(path.join(root, '.airk', 'strategies', 'untrusted.studio.airk.json')), false);
});

test('Studio Save checks redirected directories and authorization after collision review', async t => {
    const root = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), 'airk-studio-guard-')));
    t.after(() => fs.rmSync(root, { recursive: true, force: true }));
    const { CORE_PACK, STUDIO_SCHEMA } = await import('../explorerStrategies');
    const document: import('../explorerStrategies').StudioDocument = { schema: STUDIO_SCHEMA, strategy: CORE_PACK[0].strategy, objective: '', params: {}, pathPrefix: '', profile: 'portable' as const };
    const outside = path.join(root, 'outside'); fs.mkdirSync(outside); fs.mkdirSync(path.join(root, '.airk'));
    fs.symlinkSync(outside, path.join(root, '.airk', 'strategies'), process.platform === 'win32' ? 'junction' : 'dir');
    const f = load(root, {});
    await assert.rejects(f.io.saveStudioInWorkspace(document, 'sample', root, undefined, () => true), /redirected/);
    fs.unlinkSync(path.join(root, '.airk', 'strategies')); fs.mkdirSync(path.join(root, '.airk', 'strategies'));
    const target = path.join(root, '.airk', 'strategies', 'sample.studio.airk.json'); fs.writeFileSync(target, 'existing');
    let allowed = true;
    const collision = load(root, { replace: true, confirm: () => { allowed = false; } });
    assert.equal((await collision.io.saveStudioInWorkspace(document, 'sample', root, undefined, () => allowed)).saved, false);
    assert.equal(fs.readFileSync(target, 'utf8'), 'existing');
});

test('local plan and strategy imports start in .airk folders, are bounded and preserve the selected file', async t => {
    const root = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), 'airk-plan-import-')));
    t.after(() => fs.rmSync(root, { recursive: true, force: true }));
    const planFile = path.join(root, 'saved.plan.airk.json');
    const plan = { schema: 'airk.explorer.plan/1', title: 'Loaded plan', objective: 'Review this', tasks: [{ id: 'T1', title: 'Read', instructions: 'Inspect the cited source.', dependsOn: [], state: 'draft' }] };
    const text = JSON.stringify(plan); fs.writeFileSync(planFile, text);
    const f = load(root, { open: Uri.file(planFile) });
    assert.deepEqual((await f.io.importPlan())?.plan, plan);
    assert.equal(f.events[0], `open ${path.join(root, '.airk', 'plans')}`); assert.equal(fs.readFileSync(planFile, 'utf8'), text);
    fs.writeFileSync(planFile, Buffer.alloc(262145, 97));
    await assert.rejects(f.io.importPlan(), /exceeds 262144 bytes/);
    const strategyFile = path.join(root, 'strategy.json'), { CORE_PACK } = await import('../explorerStrategies');
    fs.writeFileSync(strategyFile, JSON.stringify(CORE_PACK[0].strategy));
    const strategy = load(root, { open: Uri.file(strategyFile) });
    assert.equal(((await strategy.io.importStrategy()) as import('../explorerStrategies').Strategy)?.id, CORE_PACK[0].strategy.id);
    assert.equal(strategy.events[0], `open ${path.join(root, '.airk', 'strategies')}`);
});

test('workspace libraries browse nested files without creating folders and load each validated format', async t => {
    const root = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), 'airk-libraries-')));
    t.after(() => fs.rmSync(root, { recursive: true, force: true }));
    const f = load(root, {}), workspace = Uri.file(root).toString();
    assert.equal((await f.io.listWorkspaceFiles('plans', '')).missing, true);
    assert.equal(fs.existsSync(path.join(root, '.airk')), false, 'browsing never creates a directory');
    const { CORE_PACK } = await import('../explorerStrategies');
    const plan = { schema: 'airk.explorer.plan/1', title: 'Nested', objective: 'Read', tasks: [] };
    const recipe = { schema: 'airk.explorer.search-recipe/1', name: 'Find', query: 'needle', mode: 'lexical', limit: 10, maxBytes: 8192, fullChunks: false };
    for (const [kind, value] of [['plans', plan], ['strategies', CORE_PACK[0].strategy], ['recipes', recipe]] as const) {
        const folder = path.join(root, '.airk', kind, 'release'); fs.mkdirSync(folder, { recursive: true });
        const file = path.join(folder, 'saved.json'), text = JSON.stringify(value); fs.writeFileSync(file, text);
        assert.ok((await f.io.listWorkspaceFiles(kind, '', workspace)).entries.some(entry => entry.directory && entry.path === 'release'));
        const nested = await f.io.listWorkspaceFiles(kind, 'release', workspace);
        assert.deepEqual(nested.entries, [{ name: 'saved.json', path: 'release/saved.json', directory: false }]);
        const imported = await f.io.loadWorkspaceFile(kind, workspace, 'release/saved.json');
        assert.deepEqual(kind === 'plans' ? (imported as any).plan : imported, value);
        assert.equal(fs.readFileSync(file, 'utf8'), text);
    }
    const plans = path.join(root, '.airk', 'plans');
    for (let at = 0; at < 110; at++) { fs.writeFileSync(path.join(plans, `${at}.json`), '{}'); }
    const first = await f.io.listWorkspaceFiles('plans', '', workspace);
    assert.ok(first.more !== undefined); assert.ok(first.entries.length <= 100);
    const second = await f.io.listWorkspaceFiles('plans', '', workspace, first.more);
    assert.ok(second.entries.length > 0); assert.equal(second.more, undefined);
    assert.equal(new Set([...first.entries, ...second.entries].map(entry => entry.path)).size, 111);
    fs.writeFileSync(path.join(plans, 'bad.json'), '{');
    await assert.rejects(f.io.loadWorkspaceFile('plans', workspace, 'bad.json'));
    fs.writeFileSync(path.join(root, '.airk', 'recipes', 'oversize.json'), ' '.repeat(8193));
    await assert.rejects(f.io.loadWorkspaceFile('recipes', workspace, 'oversize.json'), /exceeds/);
    for (const entry of ['../private.json', '/private.json', 'release/../../private.json']) { await assert.rejects(f.io.loadWorkspaceFile('plans', workspace, entry), /relative/); }
    await assert.rejects(f.io.listWorkspaceFiles('plans', '', 'file:///foreign'), /open workspace/);
    await assert.rejects(f.io.listWorkspaceFiles('plans', '', workspace, 10001), /paging/);
    const outside = path.join(root, 'outside'); fs.mkdirSync(outside);
    fs.symlinkSync(outside, path.join(plans, 'redirected'), process.platform === 'win32' ? 'junction' : 'dir');
    assert.ok(!(await f.io.listWorkspaceFiles('plans', '', workspace)).entries.some(entry => entry.name === 'redirected'));
    await assert.rejects(f.io.listWorkspaceFiles('plans', 'redirected', workspace), /redirected/);
    fs.linkSync(path.join(plans, 'bad.json'), path.join(plans, 'hard.json'));
    await assert.rejects(f.io.loadWorkspaceFile('plans', workspace, 'hard.json'), /unlinked/);
    f.stub.workspace.isTrusted = false;
    await assert.rejects(f.io.listWorkspaceFiles('plans', '', workspace), /Trust/);
    await assert.rejects(f.io.loadWorkspaceFile('plans', workspace, 'release/saved.json'), /Trust/);
});
/** Load explorerWorkspaceIO against a stub editor whose folder is a real temporary directory. */
function load(root: string, script: Script) {
    const events: string[] = [];
    const stub = {
        Uri, FileType: { File: 1, Directory: 2 }, ViewColumn: { Beside: 2 },
        workspace: {
            isTrusted: true,
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
            showOpenDialog: async (options: { defaultUri: Uri }) => { events.push(`open ${options.defaultUri.fsPath}`); return script.open ? [script.open] : undefined; },
            showSaveDialog: async (options: { defaultUri: Uri }) => { events.push(`dialog ${options.defaultUri.fsPath}`); return script.save; },
            showWarningMessage: async (text: string) => { events.push(`confirm ${text}`); script.confirm?.(); return script.replace ? 'Replace' : undefined; },
            showTextDocument: async (uri: Uri) => { events.push(`open ${uri.fsPath}`); },
            showQuickPick: async () => undefined,
        },
    };
    const loader = Module as unknown as { _load: (...args: any[]) => any }, original = loader._load;
    loader._load = function (request: string, ...args: any[]) { return request === 'vscode' ? stub : original.call(this, request, ...args); };
    delete require.cache[require.resolve('../explorerWorkspaceIO')];
    try { return { io: require('../explorerWorkspaceIO') as typeof import('../explorerWorkspaceIO'), events, stub }; }
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

test('guarded plan saves reject redirected parents, hard links, changed files and revoked authorization', async t => {
    const root = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), 'airk-guarded-plan-')));
    t.after(() => fs.rmSync(root, { recursive: true, force: true }));
    const workspace = path.join(root, 'workspace'), outside = path.join(root, 'outside');
    fs.mkdirSync(workspace); fs.mkdirSync(outside);
    const link = path.join(workspace, 'redirected');
    fs.symlinkSync(outside, link, process.platform === 'win32' ? 'junction' : 'dir');
    const redirected = load(workspace, { save: Uri.file(path.join(link, 'plan.md')) });
    await assert.rejects(redirected.io.savePlanInWorkspace('plan', 'plan.md'), /redirected/);
    assert.deepEqual(fs.readdirSync(outside), []);
    const target = path.join(workspace, 'plan.md'); fs.writeFileSync(target, 'original');
    const hard = path.join(workspace, 'hard.md'); fs.linkSync(target, hard);
    await assert.rejects(load(workspace, { save: Uri.file(hard), replace: true }).io.savePlanInWorkspace('plan', 'hard.md'), /Linked/);
    fs.unlinkSync(hard);
    const changed = load(workspace, { save: Uri.file(target), replace: true, confirm: () => fs.writeFileSync(target, 'concurrent') });
    assert.match(JSON.stringify(await changed.io.savePlanInWorkspace('replacement', 'plan.md')), /changed during review/);
    assert.equal(fs.readFileSync(target, 'utf8'), 'concurrent');
    let trusted = true;
    const revoked = load(workspace, { save: Uri.file(target), replace: true, confirm: () => { trusted = false; } });
    assert.match(JSON.stringify(await revoked.io.savePlanInWorkspace('replacement', 'plan.md', () => trusted)), /authorization changed/);
    assert.equal(fs.readFileSync(target, 'utf8'), 'concurrent');
    const dirty = load(workspace, { save: Uri.file(target), replace: true, confirm: () => dirty.stub.workspace.textDocuments.push({ uri: Uri.file(target), isDirty: true }) });
    assert.match(JSON.stringify(await dirty.io.savePlanInWorkspace('replacement', 'plan.md')), /unsaved edits/);
    assert.equal(fs.readFileSync(target, 'utf8'), 'concurrent');
    const swapped = load(workspace, { get save() {
        fs.renameSync(workspace, path.join(root, 'moved'));
        fs.symlinkSync(outside, workspace, process.platform === 'win32' ? 'junction' : 'dir');
        return Uri.file(path.join(workspace, 'new.md'));
    } });
    await assert.rejects(swapped.io.savePlanInWorkspace('replacement', 'new.md'), /workspace root changed/);
    assert.deepEqual(fs.readdirSync(outside), []);
});

test('denied stat and a concurrent creator cannot become permission to overwrite', async t => {
    const root = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), 'airk-plan-race-')));
    t.after(() => fs.rmSync(root, { recursive: true, force: true }));
    const target = path.join(root, 'plan.md');
    const native = require('node:fs') as typeof fs;
    const lstat = native.lstatSync;
    const denied = t.mock.method(native, 'lstatSync', ((file: fs.PathLike, ...args: any[]) => {
        if (file === target) { throw Object.assign(new Error('denied'), { code: 'EACCES' }); }
        return (lstat as any)(file, ...args);
    }) as any);
    await assert.rejects(load(root, { save: Uri.file(target) }).io.savePlanInWorkspace('plan', 'plan.md'), /denied/);
    denied.mock.restore(); assert.equal(fs.existsSync(target), false);
    const open = native.openSync;
    const race = t.mock.method(native, 'openSync', ((file: fs.PathLike, flags: any, ...args: any[]) => {
        if (file === target && flags === 'wx') {
            const handle = open(file, 'wx'); fs.writeSync(handle, 'concurrent creator'); fs.closeSync(handle);
        }
        return (open as any)(file, flags, ...args);
    }) as any);
    await assert.rejects(load(root, { save: Uri.file(target) }).io.savePlanInWorkspace('plan', 'plan.md'), /exist/i);
    race.mock.restore(); assert.equal(fs.readFileSync(target, 'utf8'), 'concurrent creator');
});
