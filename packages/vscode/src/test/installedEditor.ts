// SPDX-License-Identifier: Apache-2.0
// A separate private driver exercises an allowlisted stage or an installed VSIX.
import { strict as assert } from 'node:assert';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as vscode from 'vscode';
import { parse as parseJsonc, type ParseError } from 'jsonc-parser';
import { UNIFIED_TOOLS } from '../model';
import { readRetryDelay } from '../explorerEvidence';

const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
class NativeReplyError extends Error {
    readonly code?:string;
    constructor(readonly reply:unknown,body?:any){super('Native MCP refused: '+JSON.stringify(reply));const code=body?.error?.code??body?.code??(reply as any)?.error?.data?.code;this.code=typeof code==='string'?code:undefined;}
}

async function waitFor(file: string): Promise<void> {
    const end = Math.min(Date.now()+150000,Number(process.env.AIRK_EDITOR_DEADLINE));
    while (Date.now() < end) {
        try { await fs.access(file); return; } catch { await sleep(100); }
    }
    throw new Error(`Timed out waiting for ${path.basename(file)}`);
}

class Mcp {
    private readonly child;
    private stderr = '';
    private sequence = 0;
    private terminal?:Error;
    private readonly closed:Promise<{code:number|null;signal:NodeJS.Signals|null}>;
    private closeResult?:{code:number|null;signal:NodeJS.Signals|null};
    readonly observations:unknown[]=[];
    private static readonly cohort=new Map<string,{count:number;deadline:number}>();
    private readonly cohort:{count:number;deadline:number};
    private pending = new Map<number, { resolve(value: any): void; reject(error: Error): void; timer: NodeJS.Timeout }>();
    constructor(definition: vscode.McpStdioServerDefinition) {
        const key=process.env.AIRK_EDITOR_RUN+'/'+process.env.AIRK_EDITOR_SCENARIO;
        this.cohort=Mcp.cohort.get(key)??{count:0,deadline:Number(process.env.AIRK_EDITOR_DEADLINE)};Mcp.cohort.set(key,this.cohort);
        assert.ok(Number.isSafeInteger(this.cohort.deadline)&&this.cohort.deadline>Date.now(),'Shared native scenario deadline');
        const environment={...process.env};
        for(const [key,value]of Object.entries(definition.env??{})){if(value===null)delete environment[key];else environment[key]=String(value);}
        // Omitted cwd stays omitted: Node inherits this real extension host's
        // cwd. Do not invent command-directory equivalence for a definition.
        this.observations.push({launch:{command:definition.command,args:[...definition.args],cwd_explicit:definition.cwd!==undefined,cwd:definition.cwd?.fsPath,omitted_cwd_behavior:definition.cwd===undefined?'node_inherits_extension_host_cwd':undefined,inherited_process_cwd:definition.cwd===undefined?process.cwd():undefined}});
        this.child = spawn(definition.command, [...definition.args], { ...(definition.cwd?{cwd:definition.cwd.fsPath}:{}), windowsHide: true,
            env: environment, stdio: 'pipe' });
        this.closed=new Promise(resolve=>this.child.once('close',(code,signal)=>{this.closeResult={code,signal};resolve(this.closeResult);}));
        let buffer = '',stdoutBytes=0;
        this.child.stdout.setEncoding('utf8');
        this.child.stdout.on('data', data => {
            stdoutBytes+=Buffer.byteLength(data);if(stdoutBytes>67108864){this.fail(new Error('Native stdout client byte bound exceeded'));this.child.kill();return;}
            buffer += data;
            if (Buffer.byteLength(buffer)>4*1024*1024){this.fail(new Error('Native stdout buffer bound exceeded'));this.child.kill();return;}
            let end;
            while ((end = buffer.indexOf('\n')) >= 0) {
                const frame=buffer.slice(0,end);buffer=buffer.slice(end+1);
                if(Buffer.byteLength(frame)>1048576){this.fail(new Error('Native stdout frame bound exceeded'));this.child.kill();return;}
                let value:any;try{value=JSON.parse(frame);}catch{this.observations.push({invalid_stdout_frame:frame});this.fail(new Error('Invalid native stdout frame'));this.child.kill();return;}
                const request = this.pending.get(value.id);
                if (request) { clearTimeout(request.timer); this.pending.delete(value.id); request.resolve(value); }
            }
        });
        this.child.stderr.on('data', data => {this.stderr+=data.toString();if(Buffer.byteLength(this.stderr)>1048576){this.observations.push({stderr_bound_exceeded:true,retained_prefix:this.stderr.slice(0,1048576)});this.stderr=this.stderr.slice(0,1048576);this.fail(new Error('Native stderr bound exceeded'));this.child.kill();}});
        this.child.on('error', error => this.fail(error));
        this.child.on('exit', code => {let body:any;try{body=JSON.parse(this.stderr.trim().split(/\r?\n/).at(-1)??'null');}catch{}this.fail(new NativeReplyError({exit_code:code,stderr:this.stderr,partial_stdout:buffer},body));});
        this.child.stdin.on('error', error => this.fail(error));
    }
    private fail(error: Error): void {
        this.terminal??=error;
        for (const request of this.pending.values()) { clearTimeout(request.timer); request.reject(error); }
        this.pending.clear();
    }
    async request(method: string, params: object, timeout = method === 'initialize' ? 90000 : 30000): Promise<any> {
        if(this.terminal)throw this.terminal;
        assert.ok(++this.cohort.count<=512,'Shared scenario native request budget exhausted');
        const counter=path.join(process.env.AIRK_EDITOR_RUN!,process.env.AIRK_EDITOR_SCENARIO!,'native-request-budget');
        await fs.appendFile(counter,'x');assert.ok((await fs.stat(counter)).size<=512,'Cross-window native scenario request budget exhausted');
        if(this.terminal)throw this.terminal;
        assert.ok(this.pending.size<8,'Native pending request bound exhausted');
        assert.ok(this.pending.size<8,'Native pending request bound exhausted');
        timeout=Math.min(timeout,this.cohort.deadline-Date.now());assert.ok(timeout>0,'Shared native scenario deadline exhausted');
        const id = ++this.sequence;
        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => { this.pending.delete(id); reject(new Error(`MCP timed out: ${method}`)); }, timeout);
            this.pending.set(id, { resolve, reject, timer });
            this.child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n');
        });
    }
    async start(): Promise<void> {
        const result = await this.request('initialize', { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'airk-installed-editor-test', version: '1' } });
        if(result.error)throw new NativeReplyError(result,result.error.data);
        this.child.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
        const tools=await this.request('tools/list',{});
        assert.deepEqual(tools.result.tools.map((tool:any)=>tool.name).sort(),[...UNIFIED_TOOLS].sort());
    }
    async call(name: string, args: object, rejected = false): Promise<any> {
        const deadline = Math.min(Date.now()+30000,this.cohort.deadline);
        while (true) {
            const result = await this.request('tools/call', { name, arguments: {response_mode:'full',...args} }, Math.max(1,deadline-Date.now()));
            this.observations.push({name,args,result});if(this.observations.length>512)throw new Error('Native observation bound exhausted');
            let body:any;
            try { body=JSON.parse(result.result?.content?.[0]?.text); } catch { /* Preserve protocol errors below. */ }
            // Only the shared typed read-retry contract authorizes repeating the exact call.
            const delay=readRetryDelay(result.result);
            if(delay!==undefined&&Date.now()+delay<deadline){await sleep(delay);continue;}
            const error = !!(result.error || result.result?.isError);
            if(error&&!rejected)throw new NativeReplyError(result,body);
            assert.equal(error, rejected, JSON.stringify(result));
            return body;
        }
    }
    async close():Promise<unknown>{
        const wait=async(ms:number)=>{let timer:NodeJS.Timeout|undefined;try{return await Promise.race([this.closed,new Promise<undefined>(resolve=>{timer=setTimeout(()=>resolve(undefined),ms);})]);}finally{if(timer)clearTimeout(timer);}};
        this.child.stdin.end();let result=this.closeResult??await wait(5000),forced=false;
        if(!result){forced=true;this.child.kill();result=await wait(3000);}
        if(!result){this.child.kill('SIGKILL');result=await wait(2000);}
        const terminal=this.terminal?.message;this.fail(new Error('MCP closed'));const receipt={pid:this.child.pid,closed:!!result,result,forced,terminal};
        await fs.writeFile(path.join(process.env.AIRK_EDITOR_RUN!,process.env.AIRK_EDITOR_SCENARIO!,`${process.env.AIRK_EDITOR_ROLE}-mcp-${this.child.pid}-cleanup.json`),JSON.stringify(receipt,null,2));
        assert.ok(result&&!forced,'Owned MCP required force or did not close');return receipt;
    }
}

export async function run(): Promise<void> {
    const runRoot=process.env.AIRK_EDITOR_RUN!,role=process.env.AIRK_EDITOR_ROLE!,scenario=process.env.AIRK_EDITOR_SCENARIO!;
    assert.ok(runRoot&&['a','b'].includes(role)&&['same','different','multi','usage','untrusted','legacy'].includes(scenario));
    const allowed=path.resolve(__dirname,'../../../../artifacts/editor-qualification');
    const relative=path.relative(allowed,path.resolve(runRoot));
    assert.ok(relative&&!relative.startsWith('..')&&!path.isAbsolute(relative),'Private qualification directory');
    const sharedDeadline=Number(process.env.AIRK_EDITOR_DEADLINE);assert.ok(Number.isSafeInteger(sharedDeadline)&&sharedDeadline>Date.now(),'Shared editor scenario deadline');
    const folder=path.join(runRoot,scenario),marker=(name:string,owner=role)=>path.join(folder,owner+'-'+name+'.json');
    const mark=(name:string,value:unknown={})=>fs.writeFile(marker(name),JSON.stringify(value,null,2));
    const extension=vscode.extensions.getExtension('iokaio.airk');assert.ok(extension);
    const packageVersion=JSON.parse(await fs.readFile(path.resolve(__dirname,'../../package.json'),'utf8')).version;
    assert.equal(extension.packageJSON.version,packageVersion);
    const stage=process.env.AIRK_EDITOR_DEVELOPMENT_STAGE;
    if(stage)assert.equal(await fs.realpath(extension.extensionPath),await fs.realpath(stage));
    else assert.ok(path.resolve(extension.extensionPath).startsWith(path.join(runRoot,'extensions')+path.sep));
    const api=await extension.activate();await api.whenReady();
    const evidence:any={scenario,role,editor:vscode.version,version:packageVersion,installed:!stage,trusted:vscode.workspace.isTrusted,extensionHostPid:process.pid,checks:[],native:[]};
    const checks=evidence.checks as string[];
    const phaseFile=marker('phase'),phase=JSON.parse(await fs.readFile(phaseFile,'utf8').catch(()=>'null'));
    let sequence=phase?.sequence??0;
    const ui=async(action:string,input:any={},route?:string):Promise<any>=>{
        if(route)await vscode.commands.executeCommand('airk.explorer.open',route);
        const id=++sequence;await mark('ui-request',{sequence:id,action,...input});
        const file=marker('ui-done-'+id);await waitFor(file);
        const value=JSON.parse(await fs.readFile(file,'utf8'));
        assert.ok(Buffer.byteLength(JSON.stringify(value))<=1048576,'Complete UI evidence exceeds the disclosed fixture bound');
        evidence.native.push({action,value});return value;
    };
    const definitions=async()=>await api.getServerDefinitions() as vscode.McpStdioServerDefinition[];
    const noSelection=async(label:string)=>assert.deepEqual(await definitions(),[],label);
    const route={library:'#indexing/library',projects:'#indexing/projects',window:'#indexing/window'};
    const applied=(view:any)=>{
        assert.equal(view.outcome?.status,'catalog_applied',JSON.stringify(view));
        assert.match(view.outcome.receipt.operation_id,/^[0-9a-f]{32}$/);
        if(view.outcome.receipt.authority_fence_required)assert.ok(view.outcome.fenced_receipt?.proof,'Complete native delivery-fence proof');
        return view;
    };
    const context=()=>ui('context',{},route.window);
    const use=async(selection:any)=>{
        await context();await ui('page',{kind:'grants'});await ui('inspect',{kind:'grant',id:selection.grant});
        const result=await ui('useGrant',{index:selection.index,project:selection.project});
        assert.equal(result.outcome.selected_subset,true);
        const current=await definitions();assert.equal(current.length,1,'One deliberately selected Index, independent of open folder count');
        const argument=current[0].args.indexOf('--catalog-session');assert.ok(argument>=0,'Actual native catalog selector');
        const selector=JSON.parse(current[0].args[argument+1]);
        assert.equal(selector.index_id,selection.index);assert.equal(selector.grant_id,selection.grant);
        assert.deepEqual(selector.project_ids,[selection.project]);
        assert.ok(!current[0].args.includes('--root'),'No reconstructed legacy source selector');
        evidence.definitions=(evidence.definitions??[]).concat(current.map(value=>({command:value.command,args:[...value.args],env:value.env,cwd:value.cwd?.fsPath,cwd_explicit:value.cwd!==undefined})));
        return current[0];
    };
    const scoped=(body:any,selection:any)=>{
        const scope=body.scope_observation;assert.equal(scope?.qualifier,'selected_subset');
        assert.equal(scope.provenance,'user_reviewed_project_grant');assert.equal(scope.grant.id,selection.grant);
        assert.match(scope.fingerprint,/^[0-9a-f]{64}$/);assert.equal(scope.members.length,1);
        assert.equal(scope.members[0].index_id,selection.index);
        assert.deepEqual(scope.members[0].memberships.map((value:any)=>value.project_id),[selection.project]);
        return body;
    };
    const finish=async()=>{await mark('result',evidence);await mark('capture');await waitFor(path.join(folder,'capture-complete.json'));};
    await vscode.commands.executeCommand('airk.dashboard.focus');
    if(scenario==='legacy'){
        const fixture=JSON.parse(await fs.readFile(path.join(folder,'legacy-fixture.json'),'utf8'));
        assert.equal(fixture.qualification?.status,'qualified_native_fixture','Legacy editor execution requires the verified actual native producer observations');
        assert.equal(fixture.fixture_kind,'managed_native_catalog_upgrade');
        const configBytes=await fs.readFile(fixture.legacy_config.path);
        assert.equal(createHash('sha256').update(configBytes).digest('hex'),fixture.legacy_config.sha256);
        const errors:ParseError[]=[],parsed=parseJsonc(configBytes.toString('utf8'),errors,{allowTrailingComma:true}),entry=parsed.servers[fixture.legacy_config.entry_key];assert.equal(errors.length,0);
        assert.equal(entry.command,fixture.legacy_config.command);assert.deepEqual(entry.args,fixture.legacy_config.args);
        assert.deepEqual(entry.env??{},fixture.legacy_config.env??{});
        const definition=new vscode.McpStdioServerDefinition('Retained manual legacy configuration',entry.command,entry.args,entry.env);
        definition.cwd=vscode.Uri.file(fixture.legacy_config.cwd);
        const client=new Mcp(definition);
        try{
            if(fixture.legacy_config.binding_kind==='pinned_predecessor'){
                let refusal:unknown;try{await client.start();await client.call('symbols',{name:fixture.probe.symbol,compact:false});}catch(error){refusal=error;}
                assert.ok(refusal instanceof NativeReplyError&&refusal.code&&fixture.expected_refusal_codes.includes(refusal.code),'A timeout or arbitrary failure is not incompatible-format proof');
                evidence.native.push({predecessorRefusal:refusal.reply,code:refusal.code});evidence.legacyRetrievalParity=false;
            }else{
                await client.start();const result=await client.call('symbols',{name:fixture.probe.symbol,compact:false});
                assert.equal(result.scope_observation?.provenance,'imported_legacy_snapshot');
                const at=fixture.legacy_config.args.indexOf('--service-project');
                const imported=fixture.native.upgrade_receipt.plan.indexes.find((index:any)=>index.registration.binding===fixture.legacy_config.args[at+1]);
                assert.ok(imported,'Original connection must belong to the native upgrade roster');
                assert.equal(result.scope_observation.members.length,1);
                assert.equal(result.scope_observation.members[0].index_id,imported.index_id);
                assert.deepEqual(result.scope_observation.members[0].memberships.map((member:any)=>member.project_id),[imported.project_id]);
                assert.equal(result.items.length,1);assert.equal(result.items[0].citation.path,fixture.probe.path);
                assert.equal(result.items[0].citation.content_hash,fixture.probe.content_sha256);
                evidence.native.push({legacyProbe:result});evidence.legacyRetrievalParity=true;
            }
        }catch(error){evidence.failure={error:String(error),stack:error instanceof Error?error.stack:undefined,native:error instanceof NativeReplyError?error.reply:undefined};throw error;}finally{evidence.native.push({transport:client.observations});await mark('retained-evidence',evidence);await client.close();}
        assert.equal(createHash('sha256').update(await fs.readFile(fixture.legacy_config.path)).digest('hex'),fixture.legacy_config.sha256);
        await noSelection('Existing manual agent configuration owns its connection; no duplicate automatic definition');
        evidence.legacyUpgradeCoverage=true;checks.push(fixture.legacy_config.binding_kind==='pinned_predecessor'?'unchanged incapable pinned predecessor refuses upgraded format':'unchanged stable launcher retrieves through sealed imported scope');
        await finish();return;
    }
    if(scenario==='untrusted'){
        assert.equal(vscode.workspace.isTrusted,false);await noSelection('Restricted Mode advertises no native scope');
        await assert.rejects(Promise.resolve(vscode.commands.executeCommand('airk.index')),/workspace_untrusted/);
        await assert.rejects(Promise.resolve(vscode.commands.executeCommand('airk.usage')),/workspace_untrusted/);
        await ui('restricted',{},route.library);
        assert.deepEqual(await fs.readdir(path.join(folder,'storage')),[]);
        checks.push('Restricted Mode native handlers and actual Indexing forms blocked');
        await finish();return;
    }
    assert.equal(vscode.workspace.isTrusted,true);
    const source=await fs.realpath(path.join(folder,scenario==='different'&&role==='b'?'workspace-b':'workspace-a'));
    const additional=await fs.realpath(path.join(folder,scenario==='different'&&role==='b'?'shared-b':'shared-a'));
    const side=scenario==='different'&&role==='b'?'b':'a',data=path.join(folder,'storage','index-'+side);
    const own=side==='b'?'WindowBetaEvidence':'WindowAlphaEvidence',foreign=side==='b'?'WindowAlphaEvidence':'WindowBetaEvidence';
    const canonical=async(value:string)=>{const actual=await fs.realpath(value);return process.platform==='win32'?path.toNamespacedPath(actual).toLowerCase():actual;};
    const createIndex=async(suffix:string,primary:string,extra:string,leaf:string)=>{
        await ui('page',{kind:'indexes'},route.library);
        const draft=await ui('setupDraft',{name:'Fixture index '+suffix,source:primary,storage:leaf,collection:'fixture-'+suffix,additional:{alias:'shared',path:extra}});
        assert.equal(draft.setupReview.response.status,'catalog_index_setup_draft');
        await noSelection('Native setup preview conveys no permission');await assert.rejects(fs.stat(leaf),{code:'ENOENT'});
        const setup=await ui('setupApply');assert.equal(setup.outcome.status,'catalog_index_setup_applied');
        assert.equal(setup.outcome.receipt.stage,'ready','Actual native setup completion');
        const index=setup.outcome.receipt.index_id;assert.match(index,/^idx_[0-9a-f]{32}$/);
        await noSelection('Completed setup remains unselected');await ui('page',{kind:'indexes'});
        const detail=await ui('inspect',{kind:'index',id:index}),nativeIndex=detail.detail.detail.item.value,backing=nativeIndex.backing;
        assert.equal(await canonical(backing.root),await canonical(primary));assert.equal(await canonical(backing.data_root),await canonical(leaf));
        const nativeExtra=setup.outcome.receipt.request.sources.find((row:any)=>row.owner===false&&row.aliases.includes('shared'));
        assert.ok(nativeExtra);assert.equal(await canonical(nativeExtra.canonical_path),await canonical(extra));
        await ui('page',{kind:'projects'},route.projects);
        const projectDraft=await ui('projectDraft',{name:'Fixture Project '+suffix}),project=projectDraft.review.request.operation.id;
        assert.match(project,/^prj_[0-9a-f]{32}$/);applied(await ui('apply'));await ui('page',{kind:'projects'});await ui('inspect',{kind:'project',id:project});
        const member=await ui('membershipDraft',{index,roots:[{path:backing.root,relative:'fixture.ts',kind:'exact'},{path:nativeExtra.canonical_path,relative:'',kind:'subtree'}]});
        assert.equal(member.review.request.operation.membership.scope.length,2);applied(await ui('apply'));
        await noSelection('Metadata Project membership is not permission');await context();await ui('grantDraft',{project,index});
        const grantDraft=await ui('read'),grant=grantDraft.review.request.operation.grant.id;
        assert.match(grant,/^grt_[0-9a-f]{32}$/);applied(await ui('apply',{grant:true}));await noSelection('Persisted reviewed grant is inert');
        checks.push('native setup and multi-source membership full previews/receipts','persisted grant remains inert');
        return {index,project,grant,backing,name:nativeIndex.name,source:primary,additional:extra,data:leaf};
    };
    let selection:any=phase?.selection;
    if(phase?.stage==='reloaded'){
        await noSelection('Reloaded saved preferences never restore permission');
        await vscode.commands.executeCommand('airk.collections');await ui('connect',{},'#manage/service');
        await noSelection('Reconnect does not turn a saved preference into permission');
        checks.push('real reload retained an inert saved choice and withdrew automatic scope');
    }else{
        await noSelection('Fresh trusted folders are not indexes or permission');
        await vscode.commands.executeCommand('airk.collections');
        if(role==='b')await waitFor(marker('created','a'));
        await ui(role==='a'?'startService':'connect',{},'#manage/service');
        await ui('page',{kind:'indexes'},route.library);
        if(scenario==='same'&&role==='b'){
            await waitFor(marker('created','a'));
            selection=JSON.parse(await fs.readFile(marker('created','a'),'utf8'));
            checks.push('second window reuses the existing native Index without setup');
        }else{
            if(scenario==='different'&&role==='b')await waitFor(marker('created','a'));
            selection=await createIndex(side,source,additional,data);
        }
        await mark('created',selection);
        if(scenario==='usage'){
            const secondary=await createIndex('b',await fs.realpath(path.join(folder,'workspace-b')),await fs.realpath(path.join(folder,'shared-b')),path.join(folder,'storage','index-b'));
            await mark('secondary',secondary);
        }
        if(scenario==='multi'){
            await context();await ui('associationDraft',{project:selection.project,index:selection.index});applied(await ui('apply'));
            await noSelection('Saved default association is not permission');
            const other=path.join(folder,'storage','index-b');await assert.rejects(fs.stat(other),{code:'ENOENT'});
            checks.push('two open folders do not select or create all indexes');
        }
    }
    if(['same','different'].includes(scenario)){await mark('metadata-ready');await waitFor(marker('metadata-ready',role==='a'?'b':'a'));}
    const definition=await use(selection);
    if(scenario==='multi'&&!phase){
        await mark('phase',{stage:'reloaded',selection,sequence});
        try{await vscode.commands.executeCommand('workbench.action.reloadWindow');}
        catch(error){
            // The real replacement host can cancel the old host's command
            // promise during teardown. Only that exact cancellation is expected;
            // the external harness still requires the replacement-host evidence.
            if(!(error instanceof Error&&error.name==='Canceled'&&error.message==='Canceled'))throw error;
        }
        // The actual replacement extension host resumes from the harness marker.
        await new Promise<void>(()=>{});return;
    }
    if(['same','different'].includes(scenario)){
        await mark('ready');await waitFor(marker('ready',role==='a'?'b':'a'));
    }
    const client=new Mcp(definition);
    try{
        await client.start();
        const fixtureDocument=await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(source,'fixture.ts')));
        await vscode.window.showTextDocument(fixtureDocument);await vscode.commands.executeCommand('airk.index');
        let symbols:any;const until=Math.min(Date.now()+180000,sharedDeadline);
        do{
            symbols=scoped(await client.call('symbols',{name:own,compact:false}),selection);
            if(symbols.items?.length===1)break;
            assert.ok(Date.now()<until,'Explicit native indexing did not publish fixture');await sleep(500);
        }while(true);
        assert.equal(symbols.items[0].citation.path,'fixture.ts');
        assert.equal(symbols.items[0].citation.content_hash,createHash('sha256').update(await fs.readFile(path.join(source,'fixture.ts'))).digest('hex'));
        const extra=scoped(await client.call('symbols',{name:'SharedAllowedEvidence',compact:false}),selection);assert.equal(extra.items.length,1);
        assert.equal((await client.call('search',{query:foreign,mode:'lexical',limit:5})).hits.length,0);
        assert.equal((await client.call('search',{query:'HiddenPrivateEvidence',mode:'lexical',limit:5})).hits.length,0);
        const denied=await client.call('read_file',{path:'hidden.ts',start_line:1,end_line:2},true);scoped(denied,selection);
        await client.call('read_file',{path:'../workspace-b/fixture.ts',start_line:1,end_line:2},true);
        evidence.native.push({symbols,additional:extra,denied});
        checks.push('actual raw definition command/args/env','complete native tool catalog','native complete scope observation','citation bytes and out-of-grant isolation');
        const before=definition.args.slice(),otherData=path.join(folder,'selected-storage');
        await fs.mkdir(otherData,{recursive:true});
        await vscode.workspace.getConfiguration('airk').update('storage.path',otherData,vscode.ConfigurationTarget.Global);
        const after=await definitions();assert.equal(after.length,1);assert.deepEqual(after[0].args,before);
        assert.deepEqual(await fs.readdir(otherData),[],'Storage preference does not move immutable backing');
        checks.push('storage preference preserves selected Index identity and backing');
        if(['same','different'].includes(scenario)){
            await mark('reading');await waitFor(marker('reading',role==='a'?'b':'a'));
            if(role==='a'){
                await fs.appendFile(path.join(source,'fixture.ts'),'\nexport function SharedEditEvidence() { return 7; }\n');
                await vscode.commands.executeCommand('airk.index');
            }
            await mark('edited');await waitFor(marker('edited',role==='a'?'b':'a'));
            const expected=scenario==='same'||role==='a'?1:0,end=Math.min(Date.now()+180000,sharedDeadline);
            let changed:any;
            do{changed=await client.call('symbols',{name:'SharedEditEvidence',compact:false});if(changed.items.length===expected)break;assert.ok(Date.now()<end);await sleep(500);}while(true);
            scoped(changed,selection);assert.equal(changed.items.length,expected);
            if(expected)assert.equal(changed.items[0].citation.content_hash,createHash('sha256').update(await fs.readFile(path.join(source,'fixture.ts'))).digest('hex'));
            await mark('retrieved');await waitFor(marker('retrieved',role==='a'?'b':'a'));
            const peer=JSON.parse(await fs.readFile(marker('created',role==='a'?'b':'a'),'utf8'));
            if(scenario==='same')assert.equal(selection.index,peer.index);else assert.notEqual(selection.index,peer.index);
            checks.push('peer edit publication and Index isolation');
            if(scenario==='same'){
                if(role==='a'){
                    await ui('page',{kind:'projects'},route.projects);await ui('inspect',{kind:'project',id:selection.project});
                    const old=await ui('renameDraft',{name:'Stale review must not apply'});assert.equal(old.review.request.operation.operation,'rename_project');
                    await mark('stale-review-ready');await waitFor(marker('peer-mutation-done','b'));
                    const rejected=await ui('staleApply');
                    evidence.native.push({staleOriginal:old.review,staleRefusal:rejected});await mark('retained-evidence',evidence);
                    // CatalogApply wraps this Store stale-preview error in the
                    // complete native Unknown contract. This non-fenced request
                    // has no retained Gate absence proof. The canonical native
                    // envelope omits an absent optional observation.
                    assert.match(rejected.error??'',/^(?:AIRK service: )?catalog_apply_outcome_unknown$/);
                    assert.equal(rejected.outcome?.status,'catalog_outcome_unknown');assert.equal(rejected.outcome.code,'catalog_apply_outcome_unknown');
                    assert.equal(rejected.outcome.caller,old.review.request.caller);assert.deepEqual(rejected.outcome.key,old.review.request.key);
                    assert.equal(rejected.outcome.request_digest,old.review.requestDigest);assert.equal(Object.hasOwn(rejected.outcome,'observation'),false);
                    assert.equal(rejected.review,undefined);
                    assert.deepEqual(rejected.submittedReview,old.review);assert.deepEqual(rejected.original_review,old.review);
                    assert.equal(rejected.dispatch_count,1);assert.ok(Number.isSafeInteger(rejected.ui_dispatch?.dispatch));
                    const pending=rejected.pendingDetails.find((row:any)=>row.id===old.review.request.key.nonce);assert.ok(pending);
                    assert.deepEqual(pending.intent.catalog_apply.request,old.review.request);assert.equal(pending.intent.request_digest,old.review.requestDigest);
                    assert.equal(pending.intent.catalog_apply.preview_digest,old.review.previewDigest);
                    const reconciled=await ui('reconcile'),receipts=reconciled.outcome.receipts;
                    assert.equal(receipts.length,1);assert.equal(receipts[0].status,'catalog_receipt');assert.equal(receipts[0].receipt,null);
                    assert.equal(receipts[0].fence,undefined);
                    assert.ok(reconciled.pending.includes(old.review.request.key.nonce));
                    const retained=reconciled.pendingDetails.find((row:any)=>row.id===old.review.request.key.nonce);assert.ok(retained);
                    assert.deepEqual(retained.intent,pending.intent);evidence.native.push({staleReconciliation:reconciled});
                    await ui('page',{kind:'projects'});const current=await ui('inspect',{kind:'project',id:selection.project});
                    const peer=JSON.parse(await fs.readFile(marker('peer-mutation-done','b'),'utf8'));
                    const appliedRevision=peer.applied.outcome.receipt.catalog_revision;
                    assert.ok(appliedRevision>old.review.request.expected_catalog_revision);
                    assert.equal(current.revision,appliedRevision);assert.equal(current.detail.detail.revision,appliedRevision);
                    assert.deepEqual(current.detail.detail.item.value,peer.current.detail.detail.item.value);
                    assert.equal(current.detail.detail.item.value.name,'Renamed by peer');
                    evidence.native.push({peerApplied:peer,observedCurrent:current});await mark('stale-checked');
                }else{
                    await waitFor(marker('stale-review-ready','a'));await ui('page',{kind:'projects'},route.projects);await ui('inspect',{kind:'project',id:selection.project});
                    const review=await ui('renameDraft',{name:'Renamed by peer'}),result=applied(await ui('apply'));
                    await ui('page',{kind:'projects'});const current=await ui('inspect',{kind:'project',id:selection.project});
                    await mark('peer-mutation-done',{review,applied:result,current});await waitFor(marker('stale-checked','a'));
                }
                checks.push('observed stale request remained unapplied; null receipt keeps exact unknown local intent retained without resubmission');
            }
        }
        if(scenario==='usage'){
            const secondary=JSON.parse(await fs.readFile(marker('secondary'),'utf8'));
            const report=async(target:any)=>{
                // Close only this private profile's real AIRK Usage panels so
                // a previous hidden report cannot satisfy a later DOM read.
                const previous=vscode.window.tabGroups.all.flatMap(group=>group.tabs).filter(tab=>tab.input instanceof vscode.TabInputWebview&&tab.input.viewType.includes('airk.usage'));
                if(previous.length)assert.equal(await vscode.window.tabGroups.close(previous,true),true);
                await use(target);await vscode.commands.executeCommand('airk.usage');await ui('usageScoped');
                await ui('usageAdmin',{collection:target.backing.collection});await ui('usageJson',{collection:target.backing.collection});
                const end=Math.min(Date.now()+30000,sharedDeadline);let value:any;
                do{try{value=JSON.parse(vscode.window.activeTextEditor?.document.getText()??'');}catch{}if(value?.administrative_scope?.index_id===target.index)break;assert.ok(Date.now()<end);await sleep(100);}while(true);
                assert.equal(value.administrative_scope.label,'Entire Index history and shared data-root totals');
                assert.equal(value.administrative_scope.qualification,'host_administrative_context_not_read_grant');assert.ok(value.administrative_scope.native_observation);
                evidence.native.push({administrativeUsage:value});
                assert.equal(vscode.window.activeTextEditor?.document.isUntitled,true,'Only the owned generated report may be discarded');
                await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
                return value;
            };
            const secondaryDefinition=await use(secondary),otherClient=new Mcp(secondaryDefinition);
            try{await otherClient.start();await vscode.commands.executeCommand('airk.index');
                const deadline=Math.min(Date.now()+180000,sharedDeadline);
                while(!scoped(await otherClient.call('symbols',{name:'WindowBetaEvidence',compact:false}),secondary).items.length){assert.ok(Date.now()<deadline);await sleep(500);}
            }catch(error){evidence.failure={error:String(error),stack:error instanceof Error?error.stack:undefined,native:error instanceof NativeReplyError?error.reply:undefined};throw error;}finally{evidence.native.push({transport:otherClient.observations});await mark('retained-evidence',evidence);await otherClient.close();}
            const before=await report(secondary);assert.ok(BigInt(before.report.calls)>0n,'The other Index must have real recorded calls before testing preservation');
            const primaryBefore=await report(selection);assert.ok(BigInt(primaryBefore.report.calls)>0n,'Primary Index must have actual usage to clear');
            await ui('usageClear',{index:selection.index,name:selection.name,collection:selection.backing.collection});
            await ui('usageJson',{collection:selection.backing.collection});
            const clearDeadline=Math.min(Date.now()+30000,Number(process.env.AIRK_EDITOR_DEADLINE));let cleared:any;
            do{try{cleared=JSON.parse(vscode.window.activeTextEditor?.document.getText()??'');}catch{}if(cleared?.administrative_scope?.index_id===selection.index&&BigInt(cleared.report.calls)===0n)break;assert.ok(Date.now()<clearDeadline);await sleep(100);}while(true);
            evidence.native.push({primaryAfterClear:cleared});await mark('retained-evidence',evidence);
            assert.equal(BigInt(cleared.report.calls),0n);assert.equal(BigInt(cleared.lifetime?.calls??0),0n);
            assert.deepEqual(cleared.supplemental??[],[]);assert.deepEqual(cleared.history??[],[]);
            assert.equal(vscode.window.activeTextEditor?.document.isUntitled,true,'Only the owned generated report may be discarded');
            await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
            const after=await report(secondary);
            assert.ok(BigInt(after.report.calls)>=BigInt(before.report.calls),'Clearing one Index must preserve the other Index ledger');
            await use(selection);
            scoped(await client.call('symbols',{name:own,compact:false}),selection);
            checks.push('scoped Usage remains unavailable until explicit entire-Index administration','actual clear preserves the other Index ledger and read grant');
        }
        if(scenario==='multi'){
            await vscode.commands.executeCommand('airk.explorer.search','return 42');await mark('explorer-ready');await waitFor(path.join(folder,'explorer-checked.json'));
            checks.push('installed Explorer cited search, metadata Projects and live help');
            await context();await ui('page',{kind:'grants'});await ui('inspect',{kind:'grant',id:selection.grant});
            const revoke=await ui('revokeDraft');assert.equal(revoke.review.request.operation.operation,'revoke_grant');
            const receipt=applied(await ui('apply'));assert.ok(receipt.outcome.fenced_receipt?.proof);
            const observedBefore=client.observations.length;let refusal:unknown;
            try{await client.call('symbols',{name:own,compact:false});}catch(error){refusal=error;}
            evidence.native.push({revocationRefusal:refusal instanceof NativeReplyError?refusal.reply:{error:String(refusal)}});await mark('retained-evidence',evidence);
            assert.ok(refusal instanceof NativeReplyError,'Timeout/EOF is not native revocation proof');
            const envelope=refusal.reply as any;assert.equal(envelope.result?.isError,true);
            const body=JSON.parse(envelope.result.content[0].text),error=body.error;
            assert.equal(error?.code,'scoped_observation_unavailable');assert.equal(error.query_submitted,null);
            assert.equal(error.retryable,true);assert.equal(error.fresh_brokerage_required,true);assert.equal(error.query_replay_safe,false);
            assert.equal(body.requested_scope.confirmed,false);
            assert.deepEqual(body.requested_scope.selection,JSON.parse(definition.args[definition.args.indexOf('--catalog-session')+1]));
            for(const field of ['items','hits','evidence','result'])assert.equal(body[field],undefined);
            assert.equal(client.observations.length,observedBefore+1,'Revocation must not replay or rebroker the query');
            checks.push('actual grant revocation fences the retained MCP session');
        }
    }catch(error){evidence.failure={error:String(error),stack:error instanceof Error?error.stack:undefined,native:error instanceof NativeReplyError?error.reply:undefined};throw error;}finally{evidence.native.push({transport:client.observations});await mark('retained-evidence',evidence);await client.close();}
    await mark('transport-closed');
    if(['same','different'].includes(scenario))await waitFor(marker('transport-closed',role==='a'?'b':'a'));
    if(role==='a'){
        const selector=JSON.parse(definition.args[definition.args.indexOf('--catalog-session')+1]);
        const stop=await ui('serviceStop',{installDir:path.join(folder,'native-service'),runtimeSha256:process.env.AIRK_EDITOR_RUNTIME_SHA256,namespace:selector.namespace},'#manage/service');
        await fs.writeFile(path.join(folder,'native-stop.json'),JSON.stringify(stop,null,2));
        evidence.native.push({ownedServiceStop:stop});
    }else await waitFor(path.join(folder,'native-stop.json'));
    await vscode.commands.executeCommand('airk.explorer.open',route.window);
    await finish();
}
