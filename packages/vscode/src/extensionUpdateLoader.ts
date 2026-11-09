// SPDX-License-Identifier: Apache-2.0
/** Stable VS Code entry point; only AIRK's owned context and module graph are replaced. */
import * as vscode from 'vscode';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { builtinModules, createRequire } from 'node:module';
import { createHash, randomBytes } from 'node:crypto';
import { Script } from 'node:vm';
import { isDeepStrictEqual } from 'node:util';

const INVENTORY = 'airk-update.json';
const MARKER = Symbol.for('iokaio.airk.managedExtensionContext');
const ASSETS = Symbol.for('iokaio.airk.managedAssetRoot');
const MAX_FILE = 8 * 1024 * 1024, MAX_TOTAL = 32 * 1024 * 1024;
export interface UpdatePayload {
    root: string; identity: string; version: string;
    manifest: Record<string, unknown>; files: ReadonlyMap<string, Buffer>;
}
interface ManagedApi {
    getServerDefinitions(): Promise<vscode.McpServerDefinition[]>;
    whenReady(): Promise<void>;
    explorerHandoff?(): { open: boolean; route?: string };
    prepareForUpdate?(): Promise<void>;
}
interface ManagedModule { activate(context: vscode.ExtensionContext): Promise<ManagedApi>; deactivate?(): void | Promise<void> }
interface Session { module: ManagedModule; api: ManagedApi; subscriptions: vscode.Disposable[] }
const digest = (bytes: Buffer) => createHash('sha256').update(bytes).digest('hex');

async function boundedFile(filename: string, maximum: number): Promise<Buffer> {
    const file = await fs.open(filename, 'r');
    try {
        const stat = await file.stat();
        if (!stat.isFile() || stat.size > maximum) { throw new Error('AIRK update file exceeds its size budget'); }
        const bytes = Buffer.alloc(Math.min(maximum + 1, stat.size + 1));
        let used = 0;
        while (used < bytes.length) {
            const result = await file.read(bytes, used, bytes.length - used, null);
            if (!result.bytesRead) { break; } used += result.bytesRead;
        }
        if (used > maximum || used > stat.size) { throw new Error('AIRK update file grew during observation'); }
        return bytes.subarray(0, used);
    } finally { await file.close(); }
}

/** An inventory proves completeness, not trust: VS Code owns VSIX installation. */
export async function readUpdatePayload(root: string): Promise<UpdatePayload> {
    const canonical = await fs.realpath(root);
    const contained = async (name: string) => {
        const filename = path.join(root, ...name.split('/')), live = await fs.realpath(filename), relative = path.relative(canonical, live);
        if ((await fs.lstat(filename)).isSymbolicLink() || relative === '..' || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) { throw new Error('AIRK update file escapes its installed root'); }
        return filename;
    };
    const bytes = await boundedFile(await contained(INVENTORY), 256 * 1024);
    const inventory = JSON.parse(bytes.toString('utf8'));
    if (inventory.schema !== 'airk-extension-update/1' || typeof inventory.version !== 'string'
        || !inventory.files || typeof inventory.files !== 'object' || Array.isArray(inventory.files)
        || typeof inventory.package_manifest !== 'string' || Buffer.byteLength(inventory.package_manifest, 'utf8') > 128 * 1024) { throw new Error('Invalid AIRK update inventory'); }
    // VS Code rewrites package.json to attach its private install __metadata.
    // Seal the exact reviewed bytes separately, then compare every product field
    // as parsed JSON. No cross-language numeric/string canonicalization is used.
    const reviewedBytes = Buffer.from(inventory.package_manifest, 'utf8');
    if (digest(reviewedBytes) !== inventory.files['package.json']) { throw new Error('AIRK reviewed package manifest does not match its sealed digest'); }
    const reviewedManifest = JSON.parse(inventory.package_manifest);
    if (!reviewedManifest || typeof reviewedManifest !== 'object' || Array.isArray(reviewedManifest)
        || Object.hasOwn(reviewedManifest, '__metadata')) { throw new Error('Invalid AIRK reviewed package manifest'); }
    const entries = Object.entries(inventory.files);
    if (entries.length < 3 || entries.length > 256) { throw new Error('Invalid AIRK update inventory size'); }
    const files = new Map<string, Buffer>(); let total = 0;
    for (const [name, expected] of entries) {
        if (!/^(?:package\.json|native\/manifest\.json|images\/airk\.svg|out\/[A-Za-z0-9_-]+\.(?:js|css)|node_modules\/jsonc-parser\/(?:package\.json|lib\/umd\/(?:main|impl\/[A-Za-z-]+)\.js))$/.test(name)
            || typeof expected !== 'string' || !/^[0-9a-f]{64}$/.test(expected)) { throw new Error('Invalid AIRK update inventory entry'); }
        const content = await boundedFile(await contained(name), name === 'package.json' ? 256 * 1024 : MAX_FILE); total += content.length;
        if (total > MAX_TOTAL) { throw new Error('AIRK update payload exceeds its byte budget'); }
        if (name === 'package.json') {
            const installed = JSON.parse(content.toString('utf8'));
            if (!installed || typeof installed !== 'object' || Array.isArray(installed)) { throw new Error('Invalid installed AIRK package manifest'); }
            delete installed.__metadata;
            if (!isDeepStrictEqual(installed, reviewedManifest)) { throw new Error('Installed AIRK package fields differ from the reviewed manifest'); }
            files.set(name, reviewedBytes);
        } else {
            if (digest(content) !== expected) { throw new Error('AIRK update payload is incomplete or changed during installation'); }
            files.set(name, content);
        }
    }
    for (const required of ['package.json', 'out/serviceExtension.js', 'out/explorerController.js', 'out/explorerWebview.js', 'out/explorerWebview.css', 'native/manifest.json']) {
        if (!files.has(required)) { throw new Error(`AIRK update is missing ${required}`); }
    }
    const manifest = JSON.parse(files.get('package.json')!.toString('utf8'));
    const native = JSON.parse(files.get('native/manifest.json')!.toString('utf8'));
    if (manifest.name !== 'airk' || manifest.publisher !== 'iokaio' || manifest.version !== inventory.version || native.version !== inventory.version) { throw new Error('AIRK update versions do not agree'); }
    return { root: canonical, identity: digest(bytes), version: inventory.version, manifest, files };
}

/** Local imports execute captured bytes without changing Node's or another extension's cache. */
export function loadUpdateModule(payload: UpdatePayload, api: typeof vscode = vscode): ManagedModule {
    const cache = new Map<string, { exports: any }>();
    const builtins = new Set(builtinModules.map(name => name.replace(/^node:/, '')));
    const load = (name: string): any => {
        const prior = cache.get(name); if (prior) { return prior.exports; }
        const bytes = payload.files.get(name); if (!bytes) { throw new Error(`AIRK update import is outside its captured inventory: ${name}`); }
        if (name.endsWith('.json')) { const value = JSON.parse(bytes.toString('utf8')); cache.set(name, { exports: value }); return value; }
        const module = { exports: {} as any }; cache.set(name, module);
        const filename = path.join(payload.root, ...name.split('/')), nativeRequire = createRequire(filename);
        const requireCaptured = (request: string): any => {
            if (request === 'vscode') { return api; }
            if (builtins.has(request.replace(/^node:/, ''))) { return nativeRequire(request); }
            if (request === 'jsonc-parser') { return load('node_modules/jsonc-parser/lib/umd/main.js'); }
            if (!request.startsWith('./') && !request.startsWith('../')) { throw new Error(`Unsupported AIRK update dependency: ${request}`); }
            const relative = path.posix.normalize(path.posix.join(path.posix.dirname(name), request));
            const candidate = [relative, `${relative}.js`, `${relative}.json`].find(value => payload.files.has(value));
            if (!candidate) { throw new Error(`AIRK update import is not captured: ${request}`); }
            return load(candidate);
        };
        try {
            const wrapper = new Script(`(function(exports,require,module,__filename,__dirname){${bytes.toString('utf8')}\n})`, { filename }).runInThisContext();
            wrapper(module.exports, requireCaptured, module, filename, path.dirname(filename));
        } catch (error) { cache.delete(name); throw error; }
        return module.exports;
    };
    const result = load('out/serviceExtension.js');
    if (typeof result.activate !== 'function') { throw new Error('AIRK update has no activation entry'); }
    return result;
}

function managedContext(context: vscode.ExtensionContext, payload: UpdatePayload, subscriptions: vscode.Disposable[], assets: string): vscode.ExtensionContext {
    const uri = vscode.Uri.file(payload.root);
    // VS Code freezes its context. An empty facade target avoids Proxy get
    // invariants on its non-configurable, non-writable data properties.
    const extension = new Proxy({} as vscode.Extension<any>, { get(_target, key) {
        if (key === 'extensionPath') { return payload.root; }
        if (key === 'extensionUri') { return uri; }
        if (key === 'packageJSON') { return payload.manifest; }
        return Reflect.get(context.extension, key, context.extension);
    } });
    return new Proxy({} as vscode.ExtensionContext, { get(_target, key) {
        if (key === MARKER) { return true; }
        if (key === ASSETS) { return assets; }
        if (key === 'extensionPath') { return payload.root; }
        if (key === 'extensionUri') { return vscode.Uri.file(assets); }
        if (key === 'extension') { return extension; }
        if (key === 'subscriptions') { return subscriptions; }
        if (key === 'asAbsolutePath') { return (relative: string) => path.join(payload.root, relative); }
        const value = Reflect.get(context, key, context);
        return typeof value === 'function' ? value.bind(context) : value;
    } });
}
export function isManagedExtensionContext(context: vscode.ExtensionContext): boolean { return Reflect.get(context, MARKER) === true; }

/** Keep UI and Assistance bytes immutable even across an in-place VSIX replacement. */
async function captureAssets(context: vscode.ExtensionContext, payload: UpdatePayload): Promise<string> {
    const root = path.join(context.globalStorageUri.fsPath, 'interface-cache', payload.identity);
    await fs.mkdir(context.globalStorageUri.fsPath, { recursive: true });
    const directory = async (relative: string) => {
        let current = context.globalStorageUri.fsPath;
        for (const part of relative.split('/')) {
            current = path.join(current, part);
            try { await fs.mkdir(current, { mode: 0o700 }); } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'EEXIST') { throw error; } }
            const info = await fs.lstat(current);
            if (!info.isDirectory() || info.isSymbolicLink()) { throw new Error('AIRK interface cache contains a redirected directory'); }
        }
    };
    for (const [name, bytes] of payload.files) {
        if (name.startsWith('native/')) { continue; }
        const filename = path.join(root, ...name.split('/'));
        await directory(['interface-cache', payload.identity, ...name.split('/').slice(0, -1)].join('/'));
        let existing: Buffer | undefined;
        try {
            if ((await fs.lstat(filename)).isSymbolicLink()) { throw new Error('AIRK interface cache contains a redirected file'); }
            existing = await boundedFile(filename, MAX_FILE);
        } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') { throw error; } }
        if (!existing?.equals(bytes)) {
            // Windows share this content-addressed cache. Publish complete files
            // atomically so another window never reads a truncated replacement.
            const partial = path.join(path.dirname(filename), `.${path.basename(filename)}.${randomBytes(12).toString('hex')}.partial`);
            try {
                await fs.writeFile(partial, bytes, { flag: 'wx', mode: 0o600 });
                try { await fs.rename(partial, filename); }
                catch (error) {
                    let winner: Buffer | undefined;
                    try { winner = await boundedFile(filename, MAX_FILE); } catch { /* Retain the original publication failure below. */ }
                    if (!winner?.equals(bytes)) { throw error; }
                }
            } finally {
                try { await fs.unlink(partial); } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') { throw error; } }
            }
        }
    }
    return root;
}

/** Read only this profile's AIRK registration; never choose a directory by version sorting. */
export async function installedExtensionRoot(context: vscode.ExtensionContext): Promise<string> {
    const shared = path.dirname(context.extensionPath), profile = path.resolve(context.globalStorageUri.fsPath, '..', '..');
    for (const registry of [path.join(profile, 'extensions.json'), path.join(shared, 'extensions.json')]) {
        let bytes: Buffer;
        try { bytes = await boundedFile(registry, 4 * 1024 * 1024); }
        catch (error) { if ((error as NodeJS.ErrnoException).code === 'ENOENT') { continue; } throw error; }
        const entries: unknown = JSON.parse(bytes.toString('utf8'));
        if (!Array.isArray(entries)) { throw new Error('Invalid VS Code profile extension registry'); }
        const selected = entries.filter(entry => typeof entry?.identifier?.id === 'string' && entry.identifier.id.toLowerCase() === context.extension.id.toLowerCase());
        if (selected.length !== 1 || typeof selected[0].relativeLocation !== 'string' || !/^[A-Za-z0-9_.-]+$/.test(selected[0].relativeLocation)) { throw new Error('AIRK has no unique installed entry in this VS Code profile'); }
        const resolved = path.resolve(shared, selected[0].relativeLocation);
        if (path.dirname(resolved) !== path.resolve(shared)) { throw new Error('AIRK installed entry escapes its shared extension directory'); }
        return resolved;
    }
    return vscode.extensions.getExtension(context.extension.id)?.extensionPath ?? context.extensionPath;
}

export class ExtensionUpdateLoader implements vscode.Disposable {
    private session?: Session; private payload?: UpdatePayload; private check?: Promise<void>;
    private disposed = false; private readonly subscriptions: vscode.Disposable[] = [];
    private timer?: ReturnType<typeof setInterval>; private lastFailure?: string; private failedIdentity?: string;
    private closing?: Promise<void>;
    private starting?: Promise<Session>;
    private readonly closedSessions = new WeakSet<object>();
    private readonly output = vscode.window.createOutputChannel('AIRK updates');
    constructor(private readonly context: vscode.ExtensionContext,
        private readonly read = readUpdatePayload, private readonly root = installedExtensionRoot, private readonly load = loadUpdateModule,
        private readonly settle: () => Promise<void> = () => new Promise(resolve => setTimeout(resolve, 350))) {}
    async start(): Promise<ManagedApi> {
        const payload = await this.read(this.context.extensionPath);
        if (this.disposed) { throw new Error('AIRK update loader was disposed before activation'); }
        const session = await (this.starting = this.startSession(payload)); this.starting = undefined;
        if (this.disposed) { await this.closeSession(session); throw new Error('AIRK update loader was disposed during activation'); }
        this.session = session; this.payload = payload;
        this.subscriptions.push(vscode.extensions.onDidChange(() => { void this.refresh(); }));
        if (vscode.window.onDidChangeWindowState) { this.subscriptions.push(vscode.window.onDidChangeWindowState(state => { if (state.focused) { void this.refresh(); } })); }
        this.timer = setInterval(() => { void this.refresh(); }, 2000); this.timer.unref?.(); this.context.subscriptions.push(this);
        return { getServerDefinitions: async () => { await this.check; return this.session ? this.session.api.getServerDefinitions() : []; },
            whenReady: async () => { await this.check; await this.session?.api.whenReady(); } };
    }
    private async startSession(payload: UpdatePayload, prepared = this.load(payload), assets?: string): Promise<Session> {
        const module = prepared, subscriptions: vscode.Disposable[] = [];
        try { const api = await module.activate(managedContext(this.context, payload, subscriptions, assets ?? await captureAssets(this.context, payload))); return { module, api, subscriptions }; }
        catch (error) { await this.closeSession({ module, subscriptions }); throw error; }
    }
    private async closeSession(session?: Pick<Session, 'module' | 'subscriptions'> & Partial<Pick<Session, 'api'>>): Promise<void> {
        if (!session || this.closedSessions.has(session)) { return; } this.closedSessions.add(session);
        try { await session.api?.prepareForUpdate?.(); await session.module.deactivate?.(); }
        catch (error) { this.output.appendLine(`AIRK deactivation: ${String(error)}`); }
        finally { for (const item of session.subscriptions.splice(0).reverse()) { try { item.dispose(); } catch (error) { this.output.appendLine(`AIRK context disposal: ${String(error)}`); } } }
        await Promise.resolve(); await Promise.resolve();
    }
    refresh(): Promise<void> {
        if (this.disposed) { return Promise.resolve(); } if (this.check) { return this.check; }
        const work = this.refreshOnce().catch(error => {
            const message = String(error);
            if (message !== this.lastFailure) { this.lastFailure = message; this.output.appendLine(`AIRK update is waiting: ${message}`); }
        });
        this.check = work; void work.finally(() => { if (this.check === work) { this.check = undefined; } }); return work;
    }
    private async refreshOnce(): Promise<void> {
        const root = await this.root(this.context);
        // A sealed inventory changes for every shipped byte change. Routine polls
        // read its small descriptor only; full hashing belongs to a changed candidate.
        if (this.read === readUpdatePayload && this.payload && path.relative(path.resolve(root), this.payload.root) === ''
            && digest(await boundedFile(path.join(root, INVENTORY), 256 * 1024)) === this.payload.identity) { return; }
        const next = await this.read(root);
        if (this.disposed || (next.root === this.payload?.root && next.identity === this.payload.identity) || next.identity === this.failedIdentity) { return; }
        await this.settle(); const confirmed = await this.read(await this.root(this.context));
        if (this.disposed || confirmed.root !== next.root || confirmed.identity !== next.identity) { return; }
        const prepared = this.load(confirmed), assets = await captureAssets(this.context, confirmed);
        if (this.disposed) { return; }
        const previous = this.payload!, old = this.session, explorer = old?.api.explorerHandoff?.();
        this.session = undefined; await this.closeSession(old); if (this.disposed) { return; }
        let replacement: Session;
        try {
            replacement = await this.startSession(confirmed, prepared, assets);
        } catch (error) {
            this.failedIdentity = confirmed.identity;
            this.output.appendLine(`AIRK ${confirmed.version} activation failed: ${String(error)}. Restoring the previous interface.`);
            replacement = await this.startSession(previous);
            if (this.disposed) { await this.closeSession(replacement); return; }
            this.session = replacement; this.payload = previous;
            if (explorer?.open) { await this.reopen(explorer.route); }
            void vscode.window.showWarningMessage(`AIRK ${confirmed.version} could not load. The previous interface was restored.`, 'Retry AIRK update', 'Show update log').then(action => {
                if (this.disposed) { return; }
                if (action === 'Retry AIRK update') { this.failedIdentity = undefined; void this.refresh(); }
                if (action === 'Show update log') { this.output.show(false); }
            }, error => { if (!this.disposed) { this.output.appendLine(`AIRK update notification failed: ${String(error)}`); } });
            return;
        }
        if (this.disposed) { await this.closeSession(replacement); return; }
        this.session = replacement; this.payload = confirmed; this.failedIdentity = undefined; this.lastFailure = undefined;
        if (explorer?.open) { await this.reopen(explorer.route); }
        this.output.appendLine(`AIRK ${confirmed.version} interface loaded automatically. The VS Code window was kept open.`);
    }
    private async reopen(route?: string): Promise<void> {
        try { await vscode.commands.executeCommand('airk.explorer.open', route); }
        catch (error) { this.output.appendLine(`The updated AIRK interface loaded, but Explorer could not reopen: ${String(error)}. Use AIRK: Open Explorer.`); }
    }
    dispose(): void {
        void this.stop();
    }
    stop(): Promise<void> {
        if (this.closing) { return this.closing; } this.disposed = true; if (this.timer) { clearInterval(this.timer); }
        for (const item of this.subscriptions.splice(0)) { item.dispose(); }
        const session = this.session, checking = this.check, starting = this.starting; this.session = undefined;
        this.closing = (async () => { await this.closeSession(session); if (starting) { await starting.then(value => this.closeSession(value), () => undefined); } await checking; this.output.dispose(); })();
        return this.closing;
    }
}

let active: ExtensionUpdateLoader | undefined, development: ManagedModule | undefined;
export async function activate(context: vscode.ExtensionContext): Promise<ManagedApi> {
    // Repository development fixtures have no packaged inventory and retain normal Node loading.
    try { await fs.access(path.join(context.extensionPath, INVENTORY)); }
    catch (error) {
        if ((error as NodeJS.ErrnoException).code !== 'ENOENT') { throw error; }
        if (context.extensionMode !== 2) { throw new Error('The installed AIRK extension is missing its update inventory. Reinstall the complete VSIX.'); }
        development = require('./serviceExtension') as ManagedModule; return development.activate(context);
    }
    active = new ExtensionUpdateLoader(context); return active.start();
}
export async function deactivate(): Promise<void> { await active?.stop(); active = undefined; await development?.deactivate?.(); development = undefined; }
