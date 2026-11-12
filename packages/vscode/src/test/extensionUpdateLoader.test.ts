// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import Module from 'node:module';
import * as os from 'node:os';
import * as path from 'node:path';
import test from 'node:test';
import type { UpdatePayload } from '../extensionUpdateLoader';

const calls: { command: string; args: unknown[] }[] = [], logs: string[] = [], warnings: string[] = [];
const disposable = { dispose() {} };
const vscode = {
    Uri: { file: (fsPath: string) => ({ scheme: 'file', fsPath }) },
    extensions: { onDidChange: () => disposable, getExtension: () => undefined },
    commands: { executeCommand: async (command: string, ...args: unknown[]) => { calls.push({ command, args }); } },
    window: {
        onDidChangeWindowState: () => disposable,
        createOutputChannel: () => ({ appendLine: (value: string) => logs.push(value), show() {}, dispose() {} }),
        showWarningMessage: async (value: string) => { warnings.push(value); return undefined; },
    },
};
const modules = Module as unknown as { _load: (...args: any[]) => any }, original = modules._load;
modules._load = function (request: string, ...args: any[]) { return request === 'vscode' ? vscode : original.call(this, request, ...args); };
const { ExtensionUpdateLoader, installedExtensionRoot, loadUpdateModule, readUpdatePayload, isManagedExtensionContext } = require('../extensionUpdateLoader') as typeof import('../extensionUpdateLoader');
modules._load = original;

const hash = (bytes: Buffer) => createHash('sha256').update(bytes).digest('hex');
const flush = async () => { for (let i = 0; i < 12; i++) { await Promise.resolve(); } };

function payload(root: string, identity = 'first', version = '1.8.12'): UpdatePayload {
    return { root, identity, version, manifest: { name: 'airk', publisher: 'iokaio', version }, files: new Map() };
}
function context(root: string) {
    return {
        extensionPath: root, extensionUri: vscode.Uri.file(root), extension: { id: 'iokaio.airk', packageJSON: { version: '1.8.12' } },
        subscriptions: [] as any[], globalStorageUri: vscode.Uri.file(path.join(path.dirname(root), 'user', 'globalStorage', 'iokaio.airk')),
        globalState: { marker: 'global' }, workspaceState: { marker: 'workspace' },
        asAbsolutePath: (relative: string) => path.join(root, relative),
    } as any;
}

async function diskFixture() {
    const root = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-update-payload-'));
    const files: Record<string, string> = {
        'package.json': JSON.stringify({ name: 'airk', publisher: 'iokaio', version: '1.8.12' }),
        'out/airkctl/index.js': 'exports.facade=true;',
        'out/serviceExtension.js': 'exports.activate=async()=>({getServerDefinitions:async()=>[],whenReady:async()=>{}});',
        'out/explorerController.js': 'exports.controller=true;',
        'out/explorerWebview.js': '// fictional Explorer bundle',
        'out/explorerWebview.css': '.fictional { color: inherit; }',
        'node_modules/jsonc-parser/lib/umd/impl/string-intern.js': 'exports.intern=value=>value;',
    };
    const write = async () => {
        for (const [name, content] of Object.entries(files)) { const filename = path.join(root, ...name.split('/')); await fs.mkdir(path.dirname(filename), { recursive: true }); await fs.writeFile(filename, content); }
        await fs.writeFile(path.join(root, 'airk-update.json'), JSON.stringify({ schema: 'airk-extension-update/1', version: '1.8.12', package_manifest: files['package.json'], files: Object.fromEntries(Object.entries(files).map(([name, bytes]) => [name, hash(Buffer.from(bytes))])) }));
    };
    await write();
    return { root, files, write, close: () => fs.rm(root, { recursive: true, force: true }) };
}

test('update inventory captures exact source bytes and refuses an incomplete replacement', async () => {
    const f = await diskFixture();
    try {
        const captured = await readUpdatePayload(f.root);
        assert.equal(captured.version, '1.8.12');
        assert.equal(captured.files.get('out/explorerWebview.js')!.toString(), f.files['out/explorerWebview.js']);
        await fs.writeFile(path.join(f.root, 'out/explorerWebview.js'), '// unrelated replacement');
        await assert.rejects(readUpdatePayload(f.root), /incomplete or changed/);
        assert.equal(captured.files.get('out/explorerWebview.js')!.toString(), f.files['out/explorerWebview.js'], 'captured host graph cannot become mixed with later installation bytes');
        await f.write(); await fs.unlink(path.join(f.root, 'out/explorerController.js'));
        await assert.rejects(readUpdatePayload(f.root), /ENOENT/);
    } finally { await f.close(); }
});

test('Windows path aliases keep the sealed interface active during an in-flight action', {skip:process.platform!=='win32'}, async () => {
    const f=await diskFixture(), events:string[]=[];
    const alias=f.root.toUpperCase();assert.notEqual(alias,f.root);
    const loader=new ExtensionUpdateLoader(context(f.root),readUpdatePayload,async()=>alias,()=>({
        activate:async()=>{events.push('activate');return {getServerDefinitions:async()=>[],whenReady:async()=>{},prepareForUpdate:async()=>{events.push('retire');}};},
        deactivate:async()=>{events.push('deactivate');}
    }),async()=>{});
    try{
        const before=await readUpdatePayload(f.root),after=await readUpdatePayload(alias);
        assert.equal(before.root,await fs.realpath(f.root));
        assert.equal(after.root,before.root);assert.equal(after.identity,before.identity);
        await loader.start();
        await loader.refresh();await loader.refresh();
        assert.deepEqual(events,['activate'],'An unchanged installed interface must not invalidate an ongoing native action');
    }finally{await loader.stop();await f.close();}
});

test('installed manifests accept only VS Code private metadata and formatting changes while retaining reviewed bytes', async () => {
    const f = await diskFixture();
    try {
        f.files['package.json'] = '{\n  "name":"airk", "publisher":"iokaio", "version":"1.8.12", "floating":1e-7, "unicode":"é雪", "contributes":{"settings":{"__metadata":"product field","default":[1,true,null]}}\n}\n';
        await f.write();
        const reviewed = JSON.parse(f.files['package.json']);
        const installed = { ...reviewed, __metadata: { installedTimestamp: 1794000000000, targetPlatform: 'win32-x64', source: 'vsix', arbitraryPlatformField: true } };
        await fs.writeFile(path.join(f.root, 'package.json'), JSON.stringify(installed));
        const captured = await readUpdatePayload(f.root);
        assert.equal(captured.files.get('package.json')!.toString('utf8'), f.files['package.json']);
        assert.deepEqual(captured.manifest, reviewed);
        assert.equal(Object.hasOwn(captured.manifest, '__metadata'), false);
        for (const changed of [
            { ...installed, version: '1.8.13' },
            { ...installed, publisher: 'other' },
            { ...installed, newProductField: 'not reviewed' },
            { ...installed, contributes: { settings: { __metadata: 'changed nested product field', default: [1, true, null] } } },
        ]) {
            await fs.writeFile(path.join(f.root, 'package.json'), JSON.stringify(changed));
            await assert.rejects(readUpdatePayload(f.root), /package fields differ/);
        }
        const { contributes: _removed, ...deletedField } = installed;
        await fs.writeFile(path.join(f.root, 'package.json'), JSON.stringify(deletedField));
        await assert.rejects(readUpdatePayload(f.root), /package fields differ/);
    } finally { await f.close(); }
});

test('host installation generation is validated separately from the unchanged reviewed payload', async () => {
    const f = await diskFixture();
    try {
        const original = await readUpdatePayload(f.root), reviewed = JSON.parse(f.files['package.json']);
        for (const installedTimestamp of [1794000000000, 1794000000001]) {
            await fs.writeFile(path.join(f.root, 'package.json'), JSON.stringify({ ...reviewed, __metadata: { installedTimestamp } }));
            const captured = await readUpdatePayload(f.root);
            assert.equal(Reflect.get(captured, 'installationGeneration'), String(installedTimestamp));
            assert.equal(captured.identity, original.identity, 'host installation metadata is outside the payload seal');
            assert.deepEqual(captured.manifest, original.manifest);
            assert.equal(captured.files.get('package.json')!.toString(), f.files['package.json']);
        }
        for (const metadata of [undefined, null, [], 'not metadata', {}, { installedTimestamp: '1794000000000' },
            { installedTimestamp: null }, { installedTimestamp: 0 }, { installedTimestamp: -1 },
            { installedTimestamp: 1.5 }, { installedTimestamp: 8640000000000001 }, { installedTimestamp: Number.MAX_SAFE_INTEGER + 1 }]) {
            await fs.writeFile(path.join(f.root, 'package.json'), JSON.stringify({ ...reviewed, __metadata: metadata }));
            const captured = await readUpdatePayload(f.root);
            assert.equal(Reflect.get(captured, 'installationGeneration'), undefined, `invalid host metadata: ${JSON.stringify(metadata)}`);
            assert.equal(captured.identity, original.identity);
            assert.deepEqual(captured.manifest, original.manifest);
        }
    } finally { await f.close(); }
});

test('the compatibility generation reader shares bounded timestamp validation and refuses a crossed product manifest', async () => {
    const f = await diskFixture();
    try {
        const read: unknown = Reflect.get(require('../extensionUpdateLoader'), 'readInstallationGeneration');
        assert.equal(typeof read, 'function');
        const generation = read as (root: string, expectedVersion: string) => Promise<string | undefined>;
        const reviewed = JSON.parse(f.files['package.json']), filename = path.join(f.root, 'package.json');
        await fs.writeFile(filename, JSON.stringify({ ...reviewed, __metadata: { installedTimestamp: 1794000000000 } }));
        assert.equal(await generation(f.root, '1.8.12'), '1794000000000');
        assert.equal(await generation(f.root, '1.8.12'), Reflect.get(await readUpdatePayload(f.root), 'installationGeneration'));
        for (const metadata of [undefined, {}, null, [], { installedTimestamp: '1794000000000' }, { installedTimestamp: -1 }, { installedTimestamp: 1.5 }, { installedTimestamp: 8640000000000001 }]) {
            await fs.writeFile(filename, JSON.stringify({ ...reviewed, __metadata: metadata }));
            assert.equal(await generation(f.root, '1.8.12'), undefined);
        }
        for (const changed of [{ version: '1.8.13' }, { publisher: 'other' }, { name: 'other' }]) {
            await fs.writeFile(filename, JSON.stringify({ ...reviewed, ...changed, __metadata: { installedTimestamp: 1794000000000 } }));
            await assert.rejects(generation(f.root, '1.8.12'), /AIRK installation manifest/);
        }
        await fs.writeFile(filename, 'x'.repeat(256 * 1024 + 1));
        await assert.rejects(generation(f.root, '1.8.12'), /size budget/);
        await fs.unlink(filename);assert.equal(await generation(f.root, '1.8.12'), undefined);
    } finally { await f.close(); }
});

test('an identical VSIX with a new host generation refreshes once without using asset mtimes', async () => {
    const f = await diskFixture(), events: string[] = [];
    const generation = Symbol.for('iokaio.airk.installationGeneration'), reviewed = JSON.parse(f.files['package.json']);
    const install = (metadata: unknown) => fs.writeFile(path.join(f.root, 'package.json'), JSON.stringify({ ...reviewed, __metadata: metadata }));
    const loader = new ExtensionUpdateLoader(context(f.root), readUpdatePayload, async () => f.root, () => ({
        activate: async managed => {
            events.push(`activate:${Reflect.get(managed, generation)}`);
            assert.equal(Object.hasOwn(managed.extension.packageJSON, '__metadata'), false);
            return { getServerDefinitions: async () => [], whenReady: async () => {} };
        },
        deactivate: async () => { events.push('deactivate'); },
    }), async () => {});
    try {
        await install({ installedTimestamp: 1794000000000 }); await loader.start();
        await loader.refresh();
        await install({ installedTimestamp: 1794000000001 }); await loader.refresh(); await loader.refresh();
        assert.deepEqual(events, ['activate:1794000000000', 'deactivate', 'activate:1794000000001']);
        for (const metadata of [undefined, { installedTimestamp: 'invalid' }, { installedTimestamp: 1794000000001 }]) {
            await install(metadata); await loader.refresh();
        }
        assert.deepEqual(events, ['activate:1794000000000', 'deactivate', 'activate:1794000000001'], 'missing or malformed metadata cannot invent an installation event');
        const asset = path.join(f.root, 'out/serviceExtension.js');
        await fs.utimes(asset, new Date(1794000000002), new Date(1794000000002)); await loader.refresh();
        assert.equal(events.length, 3, 'asset mtimes do not authorize a reinstall');
    } finally { await loader.stop(); await f.close(); }
});

test('reviewed manifest snapshot remains digest sealed and bounded even when installed metadata is ignored', async () => {
    const f = await diskFixture();
    try {
        const filename = path.join(f.root, 'airk-update.json'), original = JSON.parse(await fs.readFile(filename, 'utf8'));
        for (const package_manifest of [undefined, {}, 'x'.repeat(131073)]) {
            await fs.writeFile(filename, JSON.stringify({ ...original, package_manifest }));
            await assert.rejects(readUpdatePayload(f.root), /Invalid AIRK update inventory/);
        }
        await fs.writeFile(filename, JSON.stringify({ ...original, package_manifest: original.package_manifest + ' ' }));
        await assert.rejects(readUpdatePayload(f.root), /reviewed package manifest does not match its sealed digest/);
        for (const package_manifest of ['null', '[]', JSON.stringify({ ...JSON.parse(original.package_manifest), __metadata: { notAProductField: true } })]) {
            await fs.writeFile(filename, JSON.stringify({ ...original, package_manifest, files: { ...original.files, 'package.json': hash(Buffer.from(package_manifest)) } }));
            await assert.rejects(readUpdatePayload(f.root), /Invalid AIRK reviewed package manifest/);
        }
    } finally { await f.close(); }
});

test('update inventory rejects unlisted required files, unsafe paths and version disagreement', async () => {
    const f = await diskFixture();
    try {
        delete f.files['out/explorerController.js']; await f.write();
        await assert.rejects(readUpdatePayload(f.root), /missing out\/explorerController/);
        f.files['out/explorerController.js'] = 'exports.controller=true;';
        f.files['out/../outside.js'] = 'exports.outside=true;'; await f.write();
        await assert.rejects(readUpdatePayload(f.root), /Invalid AIRK update inventory entry/);
        delete f.files['out/../outside.js']; f.files['native/manifest.json'] = JSON.stringify({ version: '1.8.12' }); await f.write();
        await assert.rejects(readUpdatePayload(f.root), /Invalid AIRK update inventory entry/, 'the split VSIX carries no native entry');
        delete f.files['native/manifest.json']; f.files['package.json'] = JSON.stringify({ name: 'airk', publisher: 'iokaio', version: '1.8.11' }); await f.write();
        await assert.rejects(readUpdatePayload(f.root), /versions do not agree/);
    } finally { await f.close(); }
});

test('isolated module graphs load captured CommonJS, JSON and vscode without polluting Node caches', async () => {
    const root = path.resolve(os.tmpdir(), 'airk-captured-graph');
    const first = payload(root), files = new Map<string, Buffer>([
        ['out/serviceExtension.js', Buffer.from("const feature=require('./feature');const api=require('vscode');const meta=require('../package.json');exports.activate=async()=>({getServerDefinitions:async()=>[feature.value,meta.version,api.testIdentity],whenReady:async()=>{}});")],
        ['out/feature.js', Buffer.from("exports.value='first';")],
        ['package.json', Buffer.from('{"version":"1.8.12"}')],
    ]);
    first.files = files;
    const cacheBefore = Object.keys(require.cache);
    const firstModule = loadUpdateModule(first, { testIdentity: 'own-api' } as any);
    const firstApi = await firstModule.activate({} as any);
    assert.deepEqual(await firstApi.getServerDefinitions(), ['first', '1.8.12', 'own-api']);
    const second = { ...first, identity: 'same-version-second', files: new Map(files) };
    second.files.set('out/feature.js', Buffer.from("exports.value='second';"));
    const secondApi = await loadUpdateModule(second, { testIdentity: 'own-api' } as any).activate({} as any);
    assert.deepEqual(await secondApi.getServerDefinitions(), ['second', '1.8.12', 'own-api']);
    assert.deepEqual(await firstApi.getServerDefinitions(), ['first', '1.8.12', 'own-api']);
    assert.deepEqual(Object.keys(require.cache), cacheBefore);
    for (const source of ["require('./not-captured');exports.activate=()=>{};", "require('not-allowed');exports.activate=()=>{};"]) {
        assert.throws(() => loadUpdateModule({ ...first, files: new Map([['out/serviceExtension.js', Buffer.from(source)]]) }), /not captured|Unsupported AIRK update dependency/);
    }
});

test('captured service code can import the loader generation reader without activating another service module', async () => {
    const f = await diskFixture();
    try {
        await fs.writeFile(path.join(f.root, 'package.json'), JSON.stringify({ name: 'airk', publisher: 'iokaio', version: '1.8.12', __metadata: { installedTimestamp: 1794000000000 } }));
        const captured = payload(f.root);
        captured.files = new Map([
            ['out/serviceExtension.js', Buffer.from("const {readInstallationGeneration}=require('./extensionUpdateLoader');exports.activate=async(context)=>({getServerDefinitions:async()=>[await readInstallationGeneration(context.extensionPath,'1.8.12')],whenReady:async()=>{}});")],
            ['out/extensionUpdateLoader.js', await fs.readFile(path.join(__dirname, '..', 'extensionUpdateLoader.js'))],
        ]);
        const api = await loadUpdateModule(captured).activate(context(f.root));
        assert.deepEqual(await api.getServerDefinitions(), ['1794000000000']);
    } finally { await f.close(); }
});

function sessionFixture(options: { open?: boolean; settle?: () => Promise<void>; fail?: string; failGeneration?: string; freezeContext?: boolean;
    activationGate?: { identity: string; promise: Promise<void>; entered?(): void };
    prepareGate?: { identity: string; promise: Promise<void>; entered(): void }; deactivateFailure?: string } = {}) {
    calls.length = logs.length = warnings.length = 0;
    const root = path.resolve(os.tmpdir(), 'airk-loader-lifecycle'), original = payload(root), host = context(root);
    if (options.freezeContext) { Object.freeze(host.extension); Object.freeze(host); }
    let current = original;
    const lifecycle: string[] = [], contexts: any[] = [];
    const read = async () => current;
    const load = (next: UpdatePayload) => ({
        async activate(managed: any) {
            contexts.push(managed); lifecycle.push(`activate:${next.identity}`);
            assert.equal(managed.globalState, host.globalState); assert.equal(managed.workspaceState, host.workspaceState);
            assert.notEqual(managed.subscriptions, host.subscriptions); assert.equal(isManagedExtensionContext(managed), true);
            assert.equal(managed.extensionPath, next.root);
            assert.equal(managed.extensionUri.fsPath, path.join(host.globalStorageUri.fsPath, 'interface-cache', next.identity));
            assert.equal(managed.extension.extensionUri.fsPath, next.root);
            assert.equal(managed.extension.packageJSON.version, next.version);
            assert.equal(managed.asAbsolutePath('out/example.js'), path.join(next.root, 'out/example.js'));
            managed.subscriptions.push({ dispose: () => lifecycle.push(`dispose:${next.identity}`) });
            if (next.identity === options.fail) { throw new Error('fictional activation failure'); }
            if (next.installationGeneration !== undefined && next.installationGeneration === options.failGeneration) { throw new Error('fictional installation generation failure'); }
            if (next.identity === options.activationGate?.identity) { options.activationGate.entered?.(); await options.activationGate.promise; }
            return { getServerDefinitions: async () => [next.identity] as any, whenReady: async () => {},
                explorerHandoff: () => ({ open: options.open ?? true, route: 'indexing/library' }),
                prepareForUpdate: async () => {
                    if (next.identity !== options.prepareGate?.identity) { return; }
                    lifecycle.push(`prepare:${next.identity}`); options.prepareGate.entered();
                    await options.prepareGate.promise; lifecycle.push(`saved:${next.identity}`);
                } };
        },
        async deactivate() { lifecycle.push(`deactivate:${next.identity}`); if (next.identity === options.deactivateFailure) { throw new Error('fictional deactivation failure'); } },
    });
    const loader = new ExtensionUpdateLoader(host, read, async () => current.root, load, options.settle ?? (async () => {}));
    return { loader, lifecycle, contexts, host, original, next(next: UpdatePayload) { current = next; } };
}

test('a failed installation generation stays suppressed while a later installation of identical bytes can load', async () => {
    const f = sessionFixture({ failGeneration: '1794000000001' }); await f.loader.start();
    try {
        f.next({ ...f.original, installationGeneration: '1794000000001' }); await f.loader.refresh(); await f.loader.refresh();
        assert.equal(f.lifecycle.filter(event => event === 'activate:first').length, 3, 'initial, failed candidate and restored interface only');
        assert.equal(warnings.length, 1);
        assert.equal(Reflect.get(f.contexts.at(-1), Symbol.for('iokaio.airk.installationGeneration')), undefined);
        f.next({ ...f.original, installationGeneration: '1794000000002' }); await f.loader.refresh(); await f.loader.refresh();
        assert.equal(f.lifecycle.filter(event => event === 'activate:first').length, 4, 'later host generation has its own activation attempt');
        assert.equal(Reflect.get(f.contexts.at(-1), Symbol.for('iokaio.airk.installationGeneration')), '1794000000002');
    } finally { await f.loader.stop(); }
});

test('installation generation must remain unchanged through candidate settlement', async () => {
    let f!: ReturnType<typeof sessionFixture>;
    f = sessionFixture({ settle: async () => { const next = { ...f.original, installationGeneration: '1794000000002' }; f.next(next); } });
    await f.loader.start();
    try {
        const next = { ...f.original, installationGeneration: '1794000000001' }; f.next(next); await f.loader.refresh();
        assert.deepEqual(f.lifecycle, ['activate:first'], 'a different installation was observed while settling');
        await f.loader.refresh(); await f.loader.refresh();
        assert.deepEqual(f.lifecycle, ['activate:first', 'deactivate:first', 'dispose:first', 'activate:first']);
        assert.equal(Reflect.get(f.contexts[1], Symbol.for('iokaio.airk.installationGeneration')), '1794000000002');
    } finally { await f.loader.stop(); }
});

test('automatic handoff disposes only the AIRK context, preserves its route and forwards the original API', async () => {
    const f = sessionFixture(), api = await f.loader.start();
    try {
        assert.deepEqual(await api.getServerDefinitions(), ['first']);
        f.next(payload(path.join(f.original.root, '..', 'airk-next'), 'second', '1.8.13'));
        await f.loader.refresh();
        assert.deepEqual(f.lifecycle, ['activate:first', 'deactivate:first', 'dispose:first', 'activate:second']);
        assert.deepEqual(await api.getServerDefinitions(), ['second']);
        assert.deepEqual(calls, [{ command: 'airk.explorer.open', args: ['indexing/library'] }]);
        assert.equal(calls.some(call => /reloadWindow|restartExtensionHost/.test(call.command)), false);
        assert.equal(f.host.subscriptions.length, 1, 'the original host owns only the stable loader');
        await f.loader.refresh();
        assert.equal(f.lifecycle.filter(event => event === 'activate:second').length, 1);
    } finally { f.loader.dispose(); await flush(); }
});

test('automatic handoff waits for local preference saves before retiring the AIRK context', { timeout: 10000 }, async () => {
    let release!: () => void, entered!: () => void;
    const promise = new Promise<void>(resolve => { release = resolve; });
    const preparing = new Promise<void>(resolve => { entered = resolve; });
    const f = sessionFixture({ prepareGate: { identity: 'first', promise, entered } }), api = await f.loader.start();
    try {
        f.next(payload(f.original.root, 'second')); const pending = f.loader.refresh(); await preparing;
        assert.deepEqual(f.lifecycle, ['activate:first', 'prepare:first']);
        assert.deepEqual(calls, [], 'waiting for a local save sends no native command or editor restart');
        release(); await pending;
        assert.deepEqual(f.lifecycle, ['activate:first', 'prepare:first', 'saved:first', 'deactivate:first', 'dispose:first', 'activate:second']);
        assert.deepEqual(await api.getServerDefinitions(), ['second']); assert.equal(calls.length, 1);
    } finally { release(); await f.loader.stop(); }
});

test('throwing AIRK deactivation still disposes subscriptions before activating one successor', async () => {
    const f = sessionFixture({ deactivateFailure: 'first' }), api = await f.loader.start();
    try {
        f.next(payload(f.original.root, 'second')); await f.loader.refresh(); await f.loader.refresh();
        assert.deepEqual(f.lifecycle, ['activate:first', 'deactivate:first', 'dispose:first', 'activate:second']);
        assert.deepEqual(await api.getServerDefinitions(), ['second']); assert.equal(calls.length, 1);
        assert.ok(logs.some(value => /AIRK deactivation.*fictional deactivation failure/.test(value)));
    } finally { await f.loader.stop(); }
});

test('a closed Explorer stays closed after a same-version update', async () => {
    const f = sessionFixture({ open: false }); await f.loader.start();
    try {
        f.next(payload(f.original.root, 'rebuilt')); await f.loader.refresh();
        assert.equal(f.lifecycle.includes('activate:rebuilt'), true);
        assert.deepEqual(calls, []);
    } finally { f.loader.dispose(); await flush(); }
});

test('managed activation preserves frozen VS Code context invariants and uses owned subscriptions', async () => {
    const f = sessionFixture({ freezeContext: true }); await f.loader.start();
    try {
        f.next(payload(path.join(f.original.root, '..', 'frozen-next'), 'frozen-second', '1.8.13')); await f.loader.refresh();
        assert.deepEqual(f.lifecycle, ['activate:first', 'deactivate:first', 'dispose:first', 'activate:frozen-second']);
        assert.equal(Object.isFrozen(f.host), true); assert.equal(Object.isFrozen(f.host.extension), true);
        assert.equal(f.host.extensionPath, f.original.root); assert.equal(f.host.extension.packageJSON.version, '1.8.12');
    } finally { f.loader.dispose(); await flush(); }
});

test('failure to reopen Explorer does not roll back or leak an activated successor context', async () => {
    const f = sessionFixture(), api = await f.loader.start(), execute = vscode.commands.executeCommand;
    vscode.commands.executeCommand = async (command, ...args) => { calls.push({ command, args }); throw new Error('fictional tab reopen failure'); };
    try {
        f.next(payload(f.original.root, 'second')); await f.loader.refresh();
        assert.deepEqual(await api.getServerDefinitions(), ['second']);
        assert.deepEqual(f.lifecycle, ['activate:first', 'deactivate:first', 'dispose:first', 'activate:second']);
        assert.equal(warnings.length, 0); assert.ok(logs.some(value => /could not reopen/.test(value)));
    } finally { vscode.commands.executeCommand = execute; f.loader.dispose(); await flush(); }
    assert.deepEqual(f.lifecycle.slice(-2), ['deactivate:second', 'dispose:second']);
});

test('duplicate update events coalesce and a changing second observation cannot retire the active context', async () => {
    let release!: () => void;
    const gate = new Promise<void>(resolve => { release = resolve; });
    const f = sessionFixture({ settle: () => gate }); await f.loader.start();
    try {
        f.next(payload(f.original.root, 'second'));
        const first = f.loader.refresh(), repeated = f.loader.refresh(); assert.equal(first, repeated);
        await flush(); f.next(payload(f.original.root, 'third')); release(); await first;
        assert.deepEqual(f.lifecycle, ['activate:first']);
        await f.loader.refresh();
        assert.deepEqual(f.lifecycle, ['activate:first', 'deactivate:first', 'dispose:first', 'activate:third']);
        assert.equal(calls.length, 1);
    } finally { release(); f.loader.dispose(); await flush(); }
});

test('an activation failure disposes the failed context, restores the previous graph and does not loop', async () => {
    const f = sessionFixture({ fail: 'broken' }), api = await f.loader.start();
    try {
        f.next(payload(f.original.root, 'broken')); await f.loader.refresh();
        assert.deepEqual(f.lifecycle, ['activate:first', 'deactivate:first', 'dispose:first', 'activate:broken', 'deactivate:broken', 'dispose:broken', 'activate:first']);
        assert.deepEqual(await api.getServerDefinitions(), ['first']);
        assert.equal(warnings.length, 1); assert.match(warnings[0], /previous interface was restored/);
        await f.loader.refresh();
        assert.equal(f.lifecycle.filter(value => value === 'activate:broken').length, 1);
    } finally { f.loader.dispose(); await flush(); }
});

test('disposal during settling cannot create a replacement context', async () => {
    let release!: () => void;
    const f = sessionFixture({ settle: () => new Promise<void>(resolve => { release = resolve; }) }); await f.loader.start();
    f.next(payload(f.original.root, 'second')); const pending = f.loader.refresh(); await flush();
    f.loader.dispose(); release(); await pending; await flush();
    assert.deepEqual(f.lifecycle, ['activate:first', 'deactivate:first', 'dispose:first']); assert.deepEqual(calls, []);
});

test('disposal while successor activation waits closes its completed context without reopening Explorer', { timeout: 10000 }, async () => {
    let release!: () => void, entered!: () => void;
    const promise = new Promise<void>(resolve => { release = resolve; });
    const activationEntered = new Promise<void>(resolve => { entered = resolve; });
    const f = sessionFixture({ activationGate: { identity: 'second', promise, entered } }), api = await f.loader.start();
    try {
        f.next(payload(f.original.root, 'second')); const pending = f.loader.refresh(); await activationEntered;
        assert.equal(f.lifecycle.at(-1), 'activate:second');
        f.loader.dispose(); release(); await pending; await flush();
        assert.deepEqual(f.lifecycle, ['activate:first', 'deactivate:first', 'dispose:first', 'activate:second', 'deactivate:second', 'dispose:second']);
        assert.deepEqual(await api.getServerDefinitions(), []); assert.deepEqual(calls, []);
    } finally { release(); f.loader.dispose(); await flush(); }
});

test('disposal during initial activation closes its context and rejects the unavailable API', { timeout: 10000 }, async () => {
    let release!: () => void, entered!: () => void;
    const promise = new Promise<void>(resolve => { release = resolve; });
    const activationEntered = new Promise<void>(resolve => { entered = resolve; });
    const f = sessionFixture({ activationGate: { identity: 'first', promise, entered } });
    const start = f.loader.start();
    try {
        await activationEntered; f.loader.dispose(); release();
        await assert.rejects(start, /disposed during activation/); await flush();
        assert.deepEqual(f.lifecycle, ['activate:first', 'deactivate:first', 'dispose:first']);
        assert.deepEqual(calls, []); assert.equal(f.host.subscriptions.length, 0);
    } finally { release(); f.loader.dispose(); await flush(); }
});

test('installed root resolution uses the current profile exact AIRK entry and rejects ambiguous or escaping paths', async () => {
    const root = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-profile-registry-'));
    try {
        const extensionRoot = path.join(root, 'extensions', 'iokaio.airk-1.8.12'), host = context(extensionRoot);
        const profile = path.resolve(host.globalStorageUri.fsPath, '..', '..');
        await fs.mkdir(profile, { recursive: true }); await fs.mkdir(path.dirname(extensionRoot), { recursive: true });
        const entry = (relativeLocation: string) => ({ identifier: { id: 'iokaio.airk' }, relativeLocation });
        await fs.writeFile(path.join(path.dirname(extensionRoot), 'extensions.json'), JSON.stringify([entry('iokaio.airk-default')]));
        await fs.writeFile(path.join(profile, 'extensions.json'), JSON.stringify([{ identifier: { id: 'other.example' }, relativeLocation: 'other.example' }, entry('iokaio.airk-profile')]));
        assert.equal(await installedExtensionRoot(host), path.join(path.dirname(extensionRoot), 'iokaio.airk-profile'));
        for (const entries of [[entry('../outside')], [entry('first'), entry('second')], []]) {
            await fs.writeFile(path.join(profile, 'extensions.json'), JSON.stringify(entries));
            await assert.rejects(installedExtensionRoot(host), /no unique installed entry/);
        }
        await fs.writeFile(path.join(profile, 'extensions.json'), JSON.stringify([entry('..')]));
        await assert.rejects(installedExtensionRoot(host), /escapes its shared extension directory/);
    } finally { await fs.rm(root, { recursive: true, force: true }); }
});
