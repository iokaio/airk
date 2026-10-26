// SPDX-License-Identifier: Apache-2.0
// A separate private driver exercises an allowlisted stage or an installed VSIX.
import { strict as assert } from 'node:assert';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as vscode from 'vscode';
import { LaunchSpec, launchArgs } from '../model';
import { probeMcp, runJson } from '../native';

const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

async function waitFor(file: string): Promise<void> {
    const end = Date.now() + 150000;
    while (Date.now() < end) {
        try { await fs.access(file); return; } catch { await sleep(100); }
    }
    throw new Error(`Timed out waiting for ${path.basename(file)}`);
}

class Mcp {
    private readonly child;
    private stderr = '';
    private sequence = 0;
    private pending = new Map<number, { resolve(value: any): void; reject(error: Error): void; timer: NodeJS.Timeout }>();
    constructor(spec: LaunchSpec) {
        this.child = spawn(spec.command, launchArgs(spec), { cwd: spec.root, windowsHide: true,
            env: { ...process.env }, stdio: 'pipe' });
        let buffer = '';
        this.child.stdout.setEncoding('utf8');
        this.child.stdout.on('data', data => {
            buffer += data;
            if (buffer.length > 4 * 1024 * 1024) { this.close(); return; }
            let end;
            while ((end = buffer.indexOf('\n')) >= 0) {
                const value = JSON.parse(buffer.slice(0, end)); buffer = buffer.slice(end + 1);
                const request = this.pending.get(value.id);
                if (request) { clearTimeout(request.timer); this.pending.delete(value.id); request.resolve(value); }
            }
        });
        this.child.stderr.on('data', data => { this.stderr = (this.stderr + data.toString()).slice(-8192); });
        this.child.on('error', error => this.fail(error));
        this.child.on('exit', code => this.fail(new Error(`MCP exited: ${code}: ${this.stderr}`)));
        this.child.stdin.on('error', error => this.fail(error));
    }
    private fail(error: Error): void {
        for (const request of this.pending.values()) { clearTimeout(request.timer); request.reject(error); }
        this.pending.clear();
    }
    async request(method: string, params: object, timeout = method === 'initialize' ? 90000 : 30000): Promise<any> {
        const id = ++this.sequence;
        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => { this.pending.delete(id); reject(new Error(`MCP timed out: ${method}`)); }, timeout);
            this.pending.set(id, { resolve, reject, timer });
            this.child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n');
        });
    }
    async start(): Promise<void> {
        const result = await this.request('initialize', { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'airk-installed-editor-test', version: '1' } });
        assert.ok(!result.error);
        this.child.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
    }
    async call(name: string, args: object, rejected = false): Promise<any> {
        const deadline = Date.now() + 30000;
        while (true) {
            const result = await this.request('tools/call', { name, arguments: {response_mode:'full',...args} }, Math.max(1,deadline-Date.now()));
            let body:any;
            try { body=JSON.parse(result.result?.content?.[0]?.text); } catch { /* Preserve protocol errors below. */ }
            const busy=body?.error;
            if(result.result?.isError&&busy?.code==='direct_query_admission'&&busy.query_submitted===false&&busy.retryable===true&&busy.reason==='busy'&&busy.fresh_brokerage_required===false&&Date.now()<deadline){
                await sleep(Math.min(100,Math.max(25,Number(busy.retry_after_ms)||25)));continue;
            }
            const error = !!(result.error || result.result?.isError);
            assert.equal(error, rejected, JSON.stringify(result));
            return rejected ? undefined : body;
        }
    }
    close(): void { this.child.stdin.end(); this.child.kill(); this.fail(new Error('MCP closed')); }
}

export async function run(): Promise<void> {
    const runRoot = process.env.AIRK_EDITOR_RUN!, role = process.env.AIRK_EDITOR_ROLE!;
    const scenario = process.env.AIRK_EDITOR_SCENARIO!;
    assert.ok(runRoot && ['a', 'b'].includes(role) && ['same', 'different', 'multi', 'usage', 'untrusted'].includes(scenario));
    const allowed = path.resolve(__dirname, '../../../../artifacts/editor-qualification');
    const relative = path.relative(allowed, path.resolve(runRoot));
    assert.ok(relative && !relative.startsWith('..') && !path.isAbsolute(relative), 'Private qualification directory');
    const folder = path.join(runRoot, scenario);
    const marker = (name: string, owner = role) => path.join(folder, `${owner}-${name}.json`);
    const mark = (name: string, value: unknown = {}) => fs.writeFile(marker(name), JSON.stringify(value));
    const barrier = async (name: string) => { await mark(name); await waitFor(marker(name, role === 'a' ? 'b' : 'a')); };
    const extension = vscode.extensions.getExtension('iokaio.airk'); assert.ok(extension, 'Installed AIRK discovered');
    const packageVersion = JSON.parse(await fs.readFile(path.resolve(__dirname, '../../package.json'), 'utf8')).version;
    assert.equal(extension.packageJSON.version, packageVersion);
    const stage=process.env.AIRK_EDITOR_DEVELOPMENT_STAGE;
    if(stage)assert.equal(await fs.realpath(extension.extensionPath),await fs.realpath(stage));
    else assert.ok(path.resolve(extension.extensionPath).startsWith(path.join(runRoot,'extensions')+path.sep));
    const api = await extension.activate();
    const commands = await vscode.commands.getCommands(true);
    for (const name of ['airk.index', 'airk.reindex', 'airk.chooseStorage', 'airk.collections', 'airk.setupAgent', 'airk.verifyMcp', 'airk.status', 'airk.usage']) {
        assert.ok(commands.includes(name), name);
    }
    await vscode.commands.executeCommand('airk.dashboard.focus');
    const evidence: Record<string, unknown> = { scenario, role, editor: vscode.version, version: extension.packageJSON.version,
        installed: !stage, trusted: vscode.workspace.isTrusted, extensionHostPid: process.pid, checks: [] };
    const checks = evidence.checks as string[];
    const openFixture = async (spec: LaunchSpec) => {
        const nativePath=(value:string)=>process.platform==='win32'?path.toNamespacedPath(value):value;
        for(const folder of vscode.workspace.workspaceFolders??[]) {
            if(path.relative(nativePath(await fs.realpath(folder.uri.fsPath)),nativePath(spec.root))==='') {
                return vscode.workspace.openTextDocument(vscode.Uri.joinPath(folder.uri,'fixture.ts'));
            }
        }
        throw new Error('Fixture has no matching opened workspace URI');
    };
    let uiSequence=0;
    const reviewedRegistration=async(root:string)=>{
        root=await fs.realpath(root);
        await vscode.commands.executeCommand('airk.serviceConnect');
        const sequence=++uiSequence;
        await mark('ui-request',{sequence,action:'register',root:path.basename(root),source:root,storage:vscode.workspace.getConfiguration('airk').inspect<string>('storage.path')?.globalValue});
        await waitFor(marker(`ui-done-${sequence}`));
        const until=Date.now()+180000;
        while(Date.now()<until){const definitions:vscode.McpStdioServerDefinition[]=await api.getServerDefinitions();
            for(const definition of definitions){const args=definition.args;
                if(await fs.realpath(args[args.indexOf('--root')+1])===root&&await fs.realpath(args[args.indexOf('--data-dir')+1])===await fs.realpath(vscode.workspace.getConfiguration('airk').inspect<string>('storage.path')!.globalValue!))return;}
            await sleep(100);
        }
        throw new Error('Reviewed native registration did not become available');
    };
    if (scenario === 'untrusted') {
        assert.equal(vscode.workspace.isTrusted, false);
        assert.deepEqual(await api.getServerDefinitions(), []);
        await assert.rejects(Promise.resolve(vscode.commands.executeCommand('airk.index')), /runtime_client_no_longer_authorized/);
        await assert.rejects(Promise.resolve(vscode.commands.executeCommand('airk.usage')), /runtime_client_no_longer_authorized/);
        assert.deepEqual(await fs.readdir(path.join(folder, 'storage')), [], 'Untrusted workspace must not create an index');
        checks.push('untrusted definitions empty', 'index and usage commands rejected', 'storage untouched', 'dashboard focus');
        await vscode.commands.executeCommand('airk.service.focus');
    } else {
        assert.equal(vscode.workspace.isTrusted, true);
        assert.deepEqual(await api.getServerDefinitions(),[],'A fresh profile must review roots before advertising MCP');
        for(const workspace of vscode.workspace.workspaceFolders??[])await reviewedRegistration(await fs.realpath(workspace.uri.fsPath));
        const definitions: vscode.McpStdioServerDefinition[] = await api.getServerDefinitions();
        assert.equal(definitions.length, ['multi', 'usage'].includes(scenario) ? 2 : 1);
        const specs: LaunchSpec[] = definitions.map(definition => ({ command: definition.command,
            root: definition.args[definition.args.indexOf('--root') + 1], dataDir: definition.args[definition.args.indexOf('--data-dir') + 1],
            collection: definition.args[definition.args.indexOf('--collection') + 1], name: definition.label,
            service:{installDir:definition.args[definition.args.indexOf('--install-dir')+1],project:definition.args[definition.args.indexOf('--service-project')+1]} }));
        assert.ok(specs.every(spec=>spec.command.includes('airk-launcher')&&/^[a-f0-9]{64}$/.test(spec.service!.project)),'Reviewed stable-launcher bindings');
        assert.deepEqual((await Promise.all(specs.map(spec => fs.realpath(spec.root)))).sort(),
            (await Promise.all((vscode.workspace.workspaceFolders ?? []).map(item => fs.realpath(item.uri.fsPath)))).sort());
        evidence.collections = specs.map(spec => spec.collection);
        for (const spec of specs) { assert.equal(await fs.realpath(spec.dataDir), await fs.realpath(path.join(folder, 'storage'))); }
        if (scenario === 'same' || scenario === 'different') { await barrier('ready'); }
        for (const spec of specs) {
            const document = await openFixture(spec);
            await vscode.window.showTextDocument(document);
            await mark('progress',{step:'index',root:spec.root,activeUri:document.uri.toString(),workspace:vscode.workspace.getWorkspaceFolder(document.uri)?.uri.toString()});
            await vscode.commands.executeCommand('airk.index');
            const ready=new Mcp(spec);await ready.start();
            try{const until=Date.now()+180000;let found=false;
                while(Date.now()<until){const symbols=await ready.call('airk_symbols',{name:path.basename(spec.root)==='workspace-b'?'WindowBetaEvidence':'WindowAlphaEvidence',compact:false});
                    if(symbols.items?.length===1){found=true;break;}await sleep(150);}
                assert.ok(found,'Native accepted work published the fixture symbols');
            }finally{ready.close();}
        }
        if (scenario === 'same' && role === 'a') {
            // The installed Explorer: Search renders a cited hit and Management lists the project (checked over CDP).
            await vscode.commands.executeCommand('airk.explorer.search', 'WindowAlphaEvidence');
            await mark('explorer-ready');
            await waitFor(path.join(folder, 'explorer-checked.json'));
            checks.push('installed Explorer search cites the fixture', 'installed Explorer Management lists the project');
        }
        if (scenario === 'usage') {
            const before = await Promise.all(specs.map(spec => runJson<any>(spec, 'usage', ['report'])));
            assert.ok(before.every(report => report.report.calls > 0));
            const sourceBefore = await Promise.all(specs.map(spec => fs.readFile(path.join(spec.root, 'fixture.ts'), 'utf8')));
            await fs.writeFile(path.join(folder, 'measurement.json'), JSON.stringify({ schema_version: 1,
                event_id: 'b'.repeat(32), unix_ms: Date.now(), client: 'codex', content_kind: 'code', provenance: 'client_reported',
                context: null, paired_task: null, airk_request_ids: [], session: { model_revision_sha256: 'a'.repeat(64),
                    requests: 1, input_tokens: 12000, cached_input_tokens: 2000, output_tokens: 1000, elapsed_ms: 45000, tool_round_trips: 3 } }));
            const otherMeasurement = path.join(folder, 'other-measurement.json');
            const other = JSON.parse(await fs.readFile(path.join(folder, 'measurement.json'), 'utf8'));
            other.event_id = 'c'.repeat(32); other.session.input_tokens = 888; other.session.cached_input_tokens = 0;
            await fs.writeFile(otherMeasurement, JSON.stringify(other));
            const draft=path.join(folder,'other-usage-draft.json');await fs.writeFile(draft,JSON.stringify({operation:'import_usage',path:otherMeasurement}));
            await runJson(specs[1], 'configure', ['--file', draft]);
            await vscode.window.showTextDocument(await openFixture(specs[0]));
            await vscode.commands.executeCommand('airk.usage');
            await mark('usage-ready', { specs });
            await waitFor(path.join(folder, 'usage-open-second.json'));
            await vscode.window.showTextDocument(await openFixture(specs[1]));
            await vscode.commands.executeCommand('airk.usage');
            await mark('usage-second-open');
            await waitFor(path.join(folder, 'usage-ui-complete.json'));
            const after = await Promise.all(specs.map(spec => runJson<any>(spec, 'usage', ['report'])));
            assert.equal(after[0].report.calls, 0); assert.equal(after[0].supplemental.length, 0); assert.equal(after[0].lifetime, null);
            assert.ok(after[1].report.calls >= before[1].report.calls, 'other collection can gain background observations');
            assert.equal(after[1].supplemental.flatMap((bucket: any) => bucket.summaries)
                .reduce((total: bigint, summary: any) => total + BigInt(summary.sessions.input_tokens), 0n), 888n,
                'clearing first collection preserves second collection supplied evidence');
            assert.deepEqual(await Promise.all(specs.map(spec => fs.readFile(path.join(spec.root, 'fixture.ts'), 'utf8'))), sourceBefore);
            for (const spec of specs) { assert.ok((await fs.stat(path.join(spec.dataDir, 'collections-1.4', spec.collection, 'active.json'))).isFile()); }
            checks.push('native reports for two collections', 'clear scoped to selected collection', 'source bytes and indexes preserved');
            const next = path.join(folder, 'selected-storage'); await fs.mkdir(next);
            await vscode.workspace.getConfiguration('airk').update('storage.path', next, vscode.ConfigurationTarget.Global);
            assert.deepEqual((await api.getServerDefinitions()).map((value:vscode.McpStdioServerDefinition)=>value.args),definitions.map(value=>value.args),'Storage preference preserves reviewed bindings');
            await reviewedRegistration(specs[0].root);
            const reboundContents = await fs.readdir(next);
            await mark('usage-storage-changed');
            await waitFor(path.join(folder, 'usage-stale-checked.json'));
            assert.deepEqual(await fs.readdir(next), reboundContents, 'stale usage panel must not create additional storage');
            checks.push('reviewed storage rebinding invalidates open reporting panels');
            await vscode.commands.executeCommand('airk.service.focus');
            await mark('result', evidence); await mark('capture');
            await waitFor(path.join(folder, 'capture-complete.json'));
            return;
        }
        if (scenario === 'same' || scenario === 'different') { await barrier('indexed'); }
        checks.push('native command indexing', 'shared storage selection');
        for (const spec of specs) {
            const own = path.basename(spec.root) === 'workspace-b' ? 'WindowBetaEvidence' : 'WindowAlphaEvidence';
            const foreign = own === 'WindowAlphaEvidence' ? 'WindowBetaEvidence' : 'WindowAlphaEvidence';
            const client = new Mcp(spec);
            try {
                await client.start();
                const symbols = await client.call('airk_symbols', { name: own, compact: false });
                assert.equal(symbols.collection, spec.collection); assert.equal(symbols.items.length, 1);
                const sourceHash = createHash('sha256').update(await fs.readFile(path.join(spec.root, 'fixture.ts'))).digest('hex');
                assert.equal(symbols.items[0].citation.path, 'fixture.ts');
                assert.equal(symbols.items[0].citation.content_hash, sourceHash);
                const search = await client.call('airk_search', { query: own, limit: 5, mode: 'lexical' });
                assert.equal(search.collection, spec.collection); assert.ok(search.hits.length);
                assert.equal((await client.call('airk_search', { query: foreign, limit: 5, mode: 'lexical' })).hits.length, 0);
                await client.call('airk_search', { query: foreign, collection: 'other' }, true);
                await client.call('airk_read_file', { path: '../workspace-b/fixture.ts', start_line: 1, end_line: 2 }, true);
                if (scenario === 'same' || scenario === 'different') {
                    await barrier('reading');
                    if (role === 'a') {
                        await fs.appendFile(path.join(spec.root, 'fixture.ts'), '\nexport function SharedEditEvidence() { return 7; }\n');
                        await vscode.commands.executeCommand('airk.index');
                    } else {
                        for (let n = 0; n < 10; n++) {
                            const reply = await client.call('airk_symbols', { name: own, compact: false });
                            assert.equal(reply.collection, spec.collection); assert.equal(reply.items.length, 1);
                            await sleep(50);
                        }
                    }
                    await barrier('edited');
                    let changed = await client.call('airk_symbols', { name: 'SharedEditEvidence', compact: false });
                    if (scenario === 'same' || role === 'a') {
                        const deadline = Date.now() + 180000;
                        while (!changed.items.length && Date.now() < deadline) {
                            await sleep(150);
                            changed = await client.call('airk_symbols', { name: 'SharedEditEvidence', compact: false });
                        }
                    }
                    assert.equal(changed.collection, spec.collection);
                    assert.equal(changed.items.length, scenario === 'same' || role === 'a' ? 1 : 0);
                    if (changed.items.length) {
                        assert.equal(changed.items[0].citation.path, 'fixture.ts');
                        assert.equal(changed.items[0].citation.content_hash,
                            createHash('sha256').update(await fs.readFile(path.join(spec.root, 'fixture.ts'))).digest('hex'));
                    }
                    checks.push('concurrent MCP reads during peer indexing', 'post-edit collection isolation');
                }
            } finally { client.close(); }
            assert.equal((await probeMcp(spec)).tools.length, 16);
        }
        checks.push('16-tool handshake', 'MCP positive/negative retrieval', 'citation path/hash matches source', 'collection override rejected', 'path traversal rejected');
        if (scenario === 'same' || scenario === 'different') {
            const count = (await fs.readdir(path.join(folder, 'storage', 'collections-1.4'))).filter(name => name.startsWith('project-')).length;
            assert.equal(count, scenario === 'same' ? 1 : 2);
            checks.push(scenario === 'same' ? 'one shared collection for two windows' : 'two separate collections for two windows');
            await barrier('retrieved');
        }
        await vscode.workspace.getConfiguration('airk').update('mcp.registrationEnabled', false, vscode.ConfigurationTarget.Global);
        assert.deepEqual(await api.getServerDefinitions(), []);
        await vscode.workspace.getConfiguration('airk').update('mcp.registrationEnabled', true, vscode.ConfigurationTarget.Global);
        assert.equal((await api.getServerDefinitions()).length, specs.length);
        checks.push('registration off/on');
        // A private per-profile setting change exercises the same binding refresh
        // used after the folder picker, without automating an OS-native dialog.
        if (scenario === 'multi') {
            await vscode.window.showTextDocument(await openFixture(specs[0]));
            await vscode.commands.executeCommand('airk.reindex');
            const sequence=++uiSequence;
            await mark('ui-request',{sequence,action:'approveRepair',root:path.basename(specs[0].root),source:await fs.realpath(specs[0].root),storage:specs[0].dataDir,project:specs[0].service!.project});
            await waitFor(marker(`ui-done-${sequence}`));
            const rebuilt=new Mcp(specs[0]);await rebuilt.start();
            try{const symbols=await rebuilt.call('airk_symbols',{name:'WindowAlphaEvidence',compact:false});
                assert.equal(symbols.items.length,1);assert.equal(symbols.items[0].citation.content_hash,createHash('sha256').update(await fs.readFile(path.join(specs[0].root,'fixture.ts'))).digest('hex'));
            }finally{rebuilt.close();}
            checks.push('real one-project rebuild preview approval and successful native receipt');
            const next = path.join(folder, 'selected-storage'); await fs.mkdir(next);
            await vscode.workspace.getConfiguration('airk').update('storage.path', next, vscode.ConfigurationTarget.Global);
            assert.deepEqual((await api.getServerDefinitions()).map((value:vscode.McpStdioServerDefinition)=>value.args),definitions.map(value=>value.args));
            assert.deepEqual(await fs.readdir(next),[],'Preference alone cannot create a native binding');
            await reviewedRegistration(specs[1].root);
            const moved: vscode.McpStdioServerDefinition[] = await api.getServerDefinitions();assert.equal(moved.length,2);
            assert.ok((await fs.stat(path.join(specs[0].dataDir,'collections-1.4',specs[0].collection,'active.json'))).isFile());
            await waitFor(path.join(next,'collections-1.4',specs[1].collection,'active.json'));
            assert.ok((await fs.stat(path.join(next,'collections-1.4',specs[1].collection,'active.json'))).isFile());
            checks.push('storage switch keeps previous indexes');
        }
        await vscode.commands.executeCommand('airk.service.focus');
    }
    await mark('result', evidence);
    await mark('capture');
    await waitFor(path.join(folder, 'capture-complete.json'));
    console.log(JSON.stringify(evidence));
}
