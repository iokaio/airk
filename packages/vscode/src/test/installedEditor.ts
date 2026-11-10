// SPDX-License-Identifier: Apache-2.0
// A separate private driver exercises an allowlisted stage or an installed VSIX.
import { strict as assert } from 'node:assert';
import { spawn } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import * as vscode from 'vscode';
import { UNIFIED_TOOLS,launchArgs } from '../model';
import {ASSISTANCE_TOOLS} from '../explorerContracts';
import {launchDescriptorPath} from '../explorerAssistanceLaunch';
import { readRetryDelay } from '../readRetry';
import {InstalledHttpTransport,InstalledHttpFailure,installedHttpIdentity} from './installedEditorHttp';
import {RuntimeHost,nativeRunner,type RuntimeBinding} from '../serviceBootstrap';
import {httpMaterial,sameHttpAuthority,publishNativeInput,removeNativeInput,type HttpConnectionInfo} from '../serviceHttp';
import {ownedConfigurationSource,assertOwnedConfigurationRefresh,assertInstalledWorkbenchCopies,installedWorkbenchCopy,installedParityValue,assertInstalledInputCatalog,assertInstalledEnvelopeParity,type InstalledAgent} from './installedCompletion';
import {refreshHttpAgentConfig} from '../agentMigration';
import {toolFormFields,type WorkbenchSession} from '../workbenchContracts';
import { installedDeliveryFence, installedSelector, installedToolBody, installedMember, installedRevocation, finishInstalledClient, InstalledNativeReplyError as NativeReplyError, installedSourceReview,installedPolicyReady,installedRestoredDefinition,installedDefinitionContinuity,installedPublicationBusy } from './installedEditorContracts';
import {checkInstalledEditorC8,checkInstalledEditorC8Endpoint} from './installedEditorC8';
import {installInstalledControlTrace} from './installedControlTrace';
const {publishMailbox,readMailbox}: {publishMailbox(file:string,value:unknown):Promise<void>;readMailbox(file:string):Promise<any>} = require(path.resolve(__dirname,'../../../../scripts/editor-mailbox.cjs'));

const sleep = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
async function waitFor(file: string): Promise<void> {
    const end = Math.min(Date.now()+150000,Number(process.env.AIRK_EDITOR_DEADLINE));
    while (Date.now() < end) {
        try { await fs.access(file); return; } catch { await sleep(100); }
    }
    throw new Error(`Timed out waiting for ${path.basename(file)}`);
}

class LegacyMcp {
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
        this.cohort=LegacyMcp.cohort.get(key)??{count:0,deadline:Number(process.env.AIRK_EDITOR_DEADLINE)};LegacyMcp.cohort.set(key,this.cohort);
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
        timeout=Math.min(timeout,this.cohort.deadline-Date.now());assert.ok(timeout>0,'Shared native scenario deadline exhausted');
        const id = ++this.sequence;
        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => { this.pending.delete(id); reject(new Error(`MCP timed out: ${method}`)); }, timeout);
            this.pending.set(id, { resolve, reject, timer });
            this.child.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n');
        });
    }
    async start(expectedTools:readonly string[]=UNIFIED_TOOLS): Promise<void> {
        const result = await this.request('initialize', { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'airk-installed-editor-test', version: '1' } });
        this.observations.push({method:'initialize',result});
        if(result.error)throw new NativeReplyError(result,result.error.data);
        this.child.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
        const tools=await this.request('tools/list',{});
        this.observations.push({method:'tools/list',result:tools});assert.equal(tools.error,undefined);assert.ok(Array.isArray(tools.result?.tools));
        assert.deepEqual(tools.result.tools.map((tool:any)=>tool.name).sort(),[...expectedTools].sort());
    }
    async call(name: string, args: object, rejected = false): Promise<any> {
        const deadline = Math.min(Date.now()+30000,this.cohort.deadline);
        while (true) {
            const result = await this.request('tools/call', { name, arguments: {response_mode:'full',...args} }, Math.max(1,deadline-Date.now()));
            this.observations.push({name,args,result});if(this.observations.length>512)throw new Error('Native observation bound exhausted');
            if(result.error)throw new NativeReplyError(result,result.error.data);
            const body=installedToolBody(result);
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
        assert.ok(result&&!forced&&result.code===0&&result.signal===null,'Owned MCP did not exit cleanly: '+JSON.stringify(receipt));return receipt;
    }
}

class Mcp extends InstalledHttpTransport {
    attempts=0;
    private readonly deadline=Number(process.env.AIRK_EDITOR_DEADLINE);
    override async request(method:string,params:object,timeout=method==='initialize'?90000:30000):Promise<any>{
        const counter=path.join(process.env.AIRK_EDITOR_RUN!,process.env.AIRK_EDITOR_SCENARIO!,'native-request-budget');
        await fs.appendFile(counter,'x');assert.ok((await fs.stat(counter)).size<=512,'Cross-window native request budget');
        timeout=Math.min(timeout,this.deadline-Date.now());assert.ok(timeout>0,'Original shared scenario deadline');
        this.attempts++;return super.request(method,params,timeout);
    }
    async start():Promise<void>{
        const initialized=await this.request('initialize',{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'airk-installed-editor-test',version:'1'}});
        if(initialized.error)throw new NativeReplyError(initialized,initialized.error.data);
        await this.notify('notifications/initialized',Math.min(30000,this.deadline-Date.now()));
        const tools=await this.request('tools/list',{});assert.equal(tools.error,undefined);assert.ok(Array.isArray(tools.result?.tools));
        assert.deepEqual(tools.result.tools.map((tool:any)=>tool.name).sort(),[...UNIFIED_TOOLS].sort());
    }
    async call(name:string,args:object,rejected=false):Promise<any>{
        const deadline=Math.min(Date.now()+30000,this.deadline);
        while(true){
            const reply=await this.request('tools/call',{name,arguments:{response_mode:'full',...args}},Math.max(1,deadline-Date.now()));
            if(reply.error)throw new NativeReplyError(reply,reply.error.data);
            const body=installedToolBody(reply),delay=readRetryDelay(reply.result);
            if(delay!==undefined&&Date.now()+delay<deadline){await sleep(delay);continue;}
            const error=reply.result?.isError===true;if(error&&!rejected)throw new NativeReplyError(reply,body);
            assert.equal(error,rejected,JSON.stringify(reply));return body;
        }
    }
}

let restoreControlTrace:(()=>void)|undefined;
export async function run():Promise<void>{try{await runEditor();}finally{restoreControlTrace?.();restoreControlTrace=undefined;}}
async function runEditor(): Promise<void> {
    const runRoot=process.env.AIRK_EDITOR_RUN!,role=process.env.AIRK_EDITOR_ROLE!,scenario=process.env.AIRK_EDITOR_SCENARIO!;
    assert.ok(runRoot&&['a','b'].includes(role)&&['same','different','multi','usage','untrusted'].includes(scenario));
    const allowed=path.resolve(__dirname,'../../../../artifacts/editor-qualification');
    const relative=path.relative(allowed,path.resolve(runRoot));
    assert.ok(relative&&!relative.startsWith('..')&&!path.isAbsolute(relative),'Private qualification directory');
    const sharedDeadline=Number(process.env.AIRK_EDITOR_DEADLINE);assert.ok(Number.isSafeInteger(sharedDeadline)&&sharedDeadline>Date.now(),'Shared editor scenario deadline');
    const folder=path.join(runRoot,scenario),marker=(name:string,owner=role)=>path.join(folder,owner+'-'+name+'.json');
    if(process.env.AIRK_EDITOR_CONTROL_TRACE==='1')restoreControlTrace=installInstalledControlTrace({installDir:path.join(folder,'native-service'),outputDirectory:folder,role,runtimeSha256:process.env.AIRK_EDITOR_RUNTIME_SHA256!});
    const mark=(name:string,value:unknown={})=>publishMailbox(marker(name),value);
    const extension=vscode.extensions.getExtension('iokaio.airk');assert.ok(extension);
    const packageVersion=JSON.parse(await fs.readFile(path.resolve(__dirname,'../../package.json'),'utf8')).version;
    assert.equal(extension.packageJSON.version,packageVersion);
    const stage=process.env.AIRK_EDITOR_DEVELOPMENT_STAGE;
    if(stage)assert.equal(path.toNamespacedPath(await fs.realpath(extension.extensionPath)),path.toNamespacedPath(await fs.realpath(stage)));
    else{const relative=path.relative(await fs.realpath(path.join(runRoot,'extensions')),await fs.realpath(extension.extensionPath));assert.ok(relative&&relative!=='..'&&!relative.startsWith('..'+path.sep)&&!path.isAbsolute(relative));}
    const api=await extension.activate();await api.whenReady();
    const evidence:any={scenario,role,editor:vscode.version,version:packageVersion,installed:!stage,trusted:vscode.workspace.isTrusted,extensionHostPid:process.pid,checks:[],native:[]};
    const checks=evidence.checks as string[];
    const phaseFile=marker('phase'),phase=await readMailbox(phaseFile);
    let sequence=phase?.sequence??0;
    const ui=async(action:string,input:any={},route?:string):Promise<any>=>{
        if(route)await vscode.commands.executeCommand('airk.explorer.open',route);
        const id=++sequence;await mark('ui-request',{sequence:id,action,...input});
        const file=marker('ui-done-'+id);await waitFor(file);
        const value=await readMailbox(file);
        assert.ok(Buffer.byteLength(JSON.stringify(value))<=1048576,'Complete UI evidence exceeds the disclosed fixture bound');
        evidence.native.push({action,value});return value;
    };
    const hostFor=(installDir?:string)=>new RuntimeHost(extension.extensionPath,packageVersion,{channel:'stable',installDir:installDir??vscode.workspace.getConfiguration('airk').get<string>('service.installDir')},
        (command,args,timeout,input)=>{const remaining=sharedDeadline-Date.now();assert.ok(remaining>0);return nativeRunner(command,args,Math.min(timeout??60000,remaining),input);},()=>vscode.workspace.isTrusted&&Date.now()<sharedDeadline);
    const provePrivateCacheScope=async(host:RuntimeHost,binding:RuntimeBinding)=>{
        if(evidence.cacheCleanupScope?.active===binding.runtimeSha256)return;
        const ownershipFile=path.join(folder,'profile-'+role+'-owned.json'),before=await fs.readFile(ownershipFile),ownership=JSON.parse(before.toString('utf8'));
        assert.equal(ownership.label,role);assert.equal(ownership.private_user_data_dir,true);
        const profile=await fs.realpath(ownership.path);assert.equal(path.toNamespacedPath(profile),path.toNamespacedPath(ownership.path));
        const contextStorage=await fs.realpath(path.join(profile,'User','globalStorage'));
        const configured=vscode.workspace.getConfiguration('airk').get<string>('service.installDir');assert.ok(configured);
        assert.equal(path.toNamespacedPath(await fs.realpath(binding.installDir)),path.toNamespacedPath(await fs.realpath(configured)),'Scope evidence belongs to the actual explicitly configured private installation');
        const inspection=await host.inspectLegacyCaches(binding,contextStorage);
        assert.equal(inspection.scope,'context-only','A private custom installation must never enumerate unrelated native-default caches');
        assert.ok(typeof inspection.context_global_storage==='string');assert.ok(Array.isArray(inspection.global_storage_roots));
        assert.equal(path.toNamespacedPath(inspection.context_global_storage),path.toNamespacedPath(contextStorage));
        assert.deepEqual(inspection.global_storage_roots.map((value:unknown)=>{assert.ok(typeof value==='string');return path.toNamespacedPath(value);}),[path.toNamespacedPath(contextStorage)]);
        assert.deepEqual(await fs.readFile(ownershipFile),before,'Original runner-owned profile identity remained immutable');
        evidence.cacheCleanupScope={...inspection,profile_ownership_sha256:createHash('sha256').update(before).digest('hex'),profile_ownership:ownership,scope_basis:'native_custom_install_identity'};
        checks.push('native custom installation keeps old-cache scope within the immutable owned editor profile');
        await mark('retained-evidence',evidence);
    };
    const definitions=async()=>{
        const current=await api.getServerDefinitions() as vscode.McpHttpServerDefinition[];
        if(!current.length)return [];
        const host=hostFor(),observed=await host.discover();assert.ok(observed.inspection);const binding=host.binding(observed.inspection,observed.installDir);
        await provePrivateCacheScope(host,binding);
        return Promise.all(current.map(async definition=>{
            const identity=installedHttpIdentity(definition),nativeConnection=await host.httpConnectionInfo(binding,identity.credentialId),material=await httpMaterial(nativeConnection);
            assert.equal(identity.url,material.url);assert.equal(Object.entries(identity.headers).find(([name])=>name.toLowerCase()==='authorization')?.[1],'Bearer '+material.credential);
            return {uri:definition.uri,headers:definition.headers,version:definition.version,nativeConnection,ca:material.ca};
        }));
    };
    const safeDefinition=(value:any)=>{const identity=installedHttpIdentity(value);return {transport:'https',uri:identity.url,headers:Object.fromEntries(Object.entries(identity.headers).map(([name,header])=>[name,name.toLowerCase()==='authorization'?'Bearer [redacted]':header])),credential_id:identity.credentialId,nativeConnection:value.nativeConnection,version:value.version};};
    const noSelection=async(label:string)=>assert.deepEqual(await definitions(),[],label);
    const route={library:'#indexing/library',projects:'#indexing/projects',window:'#indexing/window'};
    const applied=(view:any)=>{
        const observed=view.applyObservation,terminal=observed?.terminal??view.outcome;
        assert.equal(terminal?.status,observed?'catalog_receipt':'catalog_applied',JSON.stringify(view));
        if(observed){assert.equal(observed.mutation_dispatches,1);assert.equal(observed.initial.outcome.status,'catalog_outcome_unknown');assert.deepEqual(observed.review,observed.initial.submittedReview);assert.equal(terminal.fence?.state,'applied');}
        const receipt=terminal.receipt,full=terminal.fenced_receipt??terminal.fence?.value;
        assert.match(receipt.operation_id,/^[0-9a-f]{32}$/);
        if(receipt.authority_fence_required)assert.ok(full?.proof,'Complete native delivery-fence proof');
        return {...view,verifiedReceipt:receipt,verifiedFencedReceipt:full};
    };
    const use=async(selection:any)=>{
        const result=await ui('scopeSelect',{index:selection.index,project:selection.project,setName:selection.setName},route.window);
        assert.equal(result.outcome.status,'search_scope_selected');
        selection.grant=result.outcome.activation.selection.session.grant_id;
        const current=await definitions();
        evidence.definitions=(evidence.definitions??[]).concat(current.map(safeDefinition));
        await mark('retained-evidence',evidence); // Complete definitions/UI result retained even on an early contract refusal.
        assert.equal(result.outcome.activation.selected_subset,true);assert.equal(current.length,1,'One deliberately selected Index, independent of open folder count');
        const selector=installedSelector(current[0],result,selection);selection.session=selector;
        return current[0];
    };
    const scoped=(body:any,selection:any,denied=false)=>installedMember(body,selection,denied);
    const finish=async()=>{await mark('result',evidence);await mark('capture');await waitFor(path.join(folder,'capture-complete.json'));};
    await vscode.commands.executeCommand('airk.dashboard.focus');
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
        await ui('sources',{index},route.library);
        const sources=await ui('read'),roots=sources.sources.page.items.map((row:any)=>row.value);
        const ownerRoot=roots.find((row:any)=>row.index_id===index&&row.canonical_path===backing.root&&row.enabled===true);
        assert.ok(ownerRoot,'Native primary source root retained');
        const idle=await ui('indexIdle',{index,binding:backing.binding,root:backing.root,data:backing.data_root,collection:backing.collection});
        assert.equal(idle.index,index);assert.equal(idle.binding,backing.binding);
        assert.equal(idle.qualification,'dated_read_only_precondition_not_mutation_admission');
        // Setup admits asynchronous native work. Observe its actual completion
        // before this first explicit policy review; never retry/cancel a policy.
        const exclusion=await installedSourceReview(index,
            ()=>ui('blockDraft',{index,root:ownerRoot.id,path:'hidden.ts'}),
            async()=>{await ui('indexIdle',{index,binding:backing.binding,root:backing.root,data:backing.data_root,collection:backing.collection});},
            attempts=>mark('source-review-attempts-'+index,attempts),sharedDeadline);
        assert.equal(exclusion.review.request.operation.operation,'change_index_policy');
        assert.deepEqual(exclusion.review.request.operation.change.selector,{source_root_id:ownerRoot.id,kind:'exact',path:'hidden.ts'});
        assert.equal(exclusion.review.request.operation.change.operation,'trim');
        applied(await ui('apply'));
        const policyHost=hostFor(),policyInstallation=await policyHost.discover();assert.ok(policyInstallation.inspection);
        const policyClient=policyHost.client(policyHost.binding(policyInstallation.inspection,policyInstallation.installDir));
        try{await installedPolicyReady(index,exclusion.review.request.operation,async()=>({policyStatus:await policyClient.catalogPolicyStatus(index),
            indexDetail:await policyClient.rpc({catalog_detail:{entity:{kind:'index',id:index},expected_catalog_revision:null}},'catalog_detail')}),
            observations=>mark('policy-application-'+index,observations),Math.min(sharedDeadline,Date.now()+90000));}finally{policyClient.dispose();}
        await noSelection('Reviewed permanent exclusion does not select the index');
        const setName='Fixture index set '+suffix;
        const saved=await ui('setCreate',{name:setName,index,indexName:nativeIndex.name},route.projects);
        assert.equal(saved.outcome.status,'index_set_saved');const project=saved.outcome.id;
        assert.match(project,/^prj_[0-9a-f]{32}$/);
        await noSelection('Saved index set remains unselected until Apply selection');
        checks.push('native setup and reviewed permanent source exclusion','current index-set checkbox creation remains inert before Apply selection');
        return {index,project,setName,backing,name:nativeIndex.name,source:primary,additional:extra,data:leaf,source_ids:roots.filter((row:any)=>row.index_id===index&&row.enabled===true).map((row:any)=>row.id)};
    };
    let selection:any=phase?.selection;
    let restoredDefinition:Awaited<ReturnType<typeof installedRestoredDefinition>>|undefined;
    if(phase?.stage==='reloaded'){
        await vscode.commands.executeCommand('airk.collections');await ui('connect',{},'#manage/service');
        const restored=await installedRestoredDefinition(definitions,selection.session,observations=>mark('reload-definition-observations',observations),sharedDeadline);
        assert.ok(sameHttpAuthority(restored.nativeConnection.selection,{kind:'combined',selection:selection.session}),'Reload preserves exact authority with forward catalog clock');
        restoredDefinition=restored;
        checks.push('real reload revalidated the saved applied choice without widening its native selector');
    }else{
        await noSelection('Fresh trusted folders are not indexes or permission');
        await vscode.commands.executeCommand('airk.collections');
        if(role==='b')await waitFor(marker('created','a'));
        await ui(role==='a'?'startService':'connect',{},'#manage/service');
        await ui('page',{kind:'indexes'},route.library);
        if(scenario==='same'&&role==='b'){
            await waitFor(marker('created','a'));
            selection=await readMailbox(marker('created','a'));
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
            await noSelection('Index set creation requires explicit Apply selection');
            const other=path.join(folder,'storage','index-b');await assert.rejects(fs.stat(other),{code:'ENOENT'});
            checks.push('two open folders do not select or create all indexes');
        }
    }
    if(['same','different'].includes(scenario)){await mark('metadata-ready');await waitFor(marker('metadata-ready',role==='a'?'b':'a'));}
    if(['same','different'].includes(scenario)&&role==='b')await waitFor(marker('scope-selected','a'));
    // A reload must exercise the restored grant, without clearing it and
    // creating another grant through an unnecessary Apply selection.
    const definition=restoredDefinition??await use(selection);
    if(['same','different'].includes(scenario))await mark('scope-selected');
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
    const transportSelection=structuredClone(selection);let client=new Mcp(definition,definition.ca);
    try{
        await client.start();
        const fixtureDocument=await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(source,'fixture.ts')));
        await vscode.window.showTextDocument(fixtureDocument);await vscode.commands.executeCommand('airk.index');
        let symbols:any;const until=Math.min(Date.now()+180000,sharedDeadline);
        do{
            const observed=await client.call('symbols',{name:own,compact:false});
            if(!installedPublicationBusy(observed)){symbols=scoped(observed,transportSelection);if(symbols.items?.length===1)break;}
            assert.ok(Date.now()<until,'Explicit native indexing did not publish fixture');await sleep(500);
        }while(true);
        assert.equal(symbols.items[0].citation.path,'fixture.ts');
        assert.equal(symbols.items[0].citation.content_hash,createHash('sha256').update(await fs.readFile(path.join(source,'fixture.ts'))).digest('hex'));
        const extra=scoped(await client.call('symbols',{name:'SharedAllowedEvidence',compact:false}),transportSelection);assert.equal(extra.items.length,1);
        assert.equal(scoped(await client.call('search',{query:foreign,mode:'lexical',limit:5}),transportSelection).hits.length,0);
        assert.equal(scoped(await client.call('search',{query:'HiddenPrivateEvidence',mode:'lexical',limit:5}),transportSelection).hits.length,0);
        const denied=await client.call('read_file',{path:'hidden.ts',start_line:1,end_line:2});scoped(denied,transportSelection,true);
        const escaped=await client.call('read_file',{path:'../workspace-b/fixture.ts',start_line:1,end_line:2});scoped(escaped,transportSelection,true);
        evidence.native.push({symbols,additional:extra,denied});
        checks.push('actual authenticated HTTPS provider definition and native selected authority','advertised native tool catalog retained with inputs; supported names match','complete combined scope/member projection; original raw transport retained','citation bytes, reviewed permanent exclusion and index isolation');
        const before=safeDefinition(definition),otherData=path.join(folder,'selected-storage');
        await fs.mkdir(otherData,{recursive:true});
        await vscode.workspace.getConfiguration('airk').update('storage.path',otherData,vscode.ConfigurationTarget.Global);
        const after=await definitions();assert.equal(after.length,1);installedDefinitionContinuity(before,safeDefinition(after[0]));
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
            do{const observed=await client.call('symbols',{name:'SharedEditEvidence',compact:false});if(!installedPublicationBusy(observed)){changed=scoped(observed,transportSelection);if(changed.items.length===expected)break;}assert.ok(Date.now()<end);await sleep(500);}while(true);
            assert.equal(changed.items.length,expected);
            if(expected)assert.equal(changed.items[0].citation.content_hash,createHash('sha256').update(await fs.readFile(path.join(source,'fixture.ts'))).digest('hex'));
            await mark('retrieved');await waitFor(marker('retrieved',role==='a'?'b':'a'));
            const peer=await readMailbox(marker('created',role==='a'?'b':'a'));
            if(scenario==='same')assert.equal(selection.index,peer.index);else assert.notEqual(selection.index,peer.index);
            checks.push('peer edit publication and Index isolation');
            if(scenario==='same'){
                if(role==='a'){
                    await ui('page',{kind:'indexes'},route.library);await ui('inspect',{kind:'index',id:selection.index});
                    const old=await ui('renameDraft',{kind:'index',name:'Stale review must not apply'});assert.equal(old.review.request.operation.operation,'rename_index');
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
                    await ui('page',{kind:'indexes'});const current=await ui('inspect',{kind:'index',id:selection.index});
                    const peer=await readMailbox(marker('peer-mutation-done','b'));
                    const appliedRevision=peer.applied.verifiedReceipt.catalog_revision;
                    assert.ok(appliedRevision>old.review.request.expected_catalog_revision);
                    assert.equal(current.revision,appliedRevision);assert.equal(current.detail.detail.revision,appliedRevision);
                    assert.deepEqual(current.detail.detail.item.value,peer.current.detail.detail.item.value);
                    assert.equal(current.detail.detail.item.value.name,'Renamed by peer');
                    evidence.native.push({peerApplied:peer,observedCurrent:current});await mark('stale-checked');
                }else{
                    await waitFor(marker('stale-review-ready','a'));await ui('page',{kind:'indexes'},route.library);await ui('inspect',{kind:'index',id:selection.index});
                    const review=await ui('renameDraft',{kind:'index',name:'Renamed by peer'}),result=applied(await ui('apply'));
                    await ui('page',{kind:'indexes'});const current=await ui('inspect',{kind:'index',id:selection.index});
                    await mark('peer-mutation-done',{review,applied:result,current});await waitFor(marker('stale-checked','a'));
                }
                checks.push('observed stale request remained unapplied; null receipt keeps exact unknown local intent retained without resubmission');
            }
        }
        if(scenario==='usage'){
            const secondary=await readMailbox(marker('secondary'));
            const report=async(target:any)=>{
                // Close only this private profile's real AIRK Usage panels so
                // a previous hidden report cannot satisfy a later DOM read.
                const previous=vscode.window.tabGroups.all.flatMap(group=>group.tabs).filter(tab=>tab.input instanceof vscode.TabInputWebview&&tab.input.viewType.includes('airk.usage'));
                if(previous.length)assert.equal(await vscode.window.tabGroups.close(previous,true),true);
                await use(target);await vscode.commands.executeCommand('airk.usage');await ui('usageScoped');
                await ui('usageAdmin',{index:target.index,collection:target.backing.collection});await ui('usageJson',{collection:target.backing.collection});
                const end=Math.min(Date.now()+30000,sharedDeadline);let value:any;
                do{try{value=JSON.parse(vscode.window.activeTextEditor?.document.getText()??'');}catch{}if(value?.administrative_scope?.index_id===target.index)break;assert.ok(Date.now()<end);await sleep(100);}while(true);
                assert.equal(value.administrative_scope.label,'Entire Index history and shared data-root totals');
                assert.equal(value.administrative_scope.qualification,'host_administrative_context_not_read_grant');assert.ok(value.administrative_scope.native_observation);
                evidence.native.push({administrativeUsage:value});
                assert.equal(vscode.window.activeTextEditor?.document.isUntitled,true,'Only the owned generated report may be discarded');
                await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor');
                return value;
            };
            const secondaryDefinition=await use(secondary),otherClient=new Mcp(secondaryDefinition,secondaryDefinition.ca);
            try{await otherClient.start();await vscode.commands.executeCommand('airk.index');
                const deadline=Math.min(Date.now()+180000,sharedDeadline);
                while(true){const observed=await otherClient.call('symbols',{name:'WindowBetaEvidence',compact:false});if(!installedPublicationBusy(observed)&&scoped(observed,secondary).items.length)break;assert.ok(Date.now()<deadline);await sleep(500);}
            }catch(error){evidence.failure={error:String(error),stack:error instanceof Error?error.stack:undefined,native:error instanceof NativeReplyError?error.reply:error instanceof InstalledHttpFailure?{kind:error.kind,...error.detail}:undefined};throw error;}finally{await finishInstalledClient(otherClient,evidence,mark);}
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
            const resumed=await use(selection);
            // The scope selections above committed new catalog grants. That advances the
            // native admission ticket, so the earlier MCP session is fenced (never rebound)
            // and a client starts a new session with the current definition.
            await assert.rejects(client.call('symbols',{name:own,compact:false}),(error:unknown)=>error instanceof InstalledHttpFailure&&(error.detail as {status?:number}|undefined)?.status===404,'Earlier session is fenced after catalog changes');
            await finishInstalledClient(client,evidence,mark);
            client=new Mcp(resumed,resumed.ca);await client.start();
            scoped(await client.call('symbols',{name:own,compact:false}),selection);
            checks.push('scoped Usage remains unavailable until explicit entire-Index administration','actual clear preserves the other Index ledger and read grant');
        }
        if(scenario==='multi'){
            const runtimeHost=hostFor(),runtimeObservation=await runtimeHost.discover();assert.ok(runtimeObservation.inspection);
            const runtimeBinding=runtimeHost.binding(runtimeObservation.inspection,runtimeObservation.installDir);
            const capabilities=await client.call('capabilities',{response_mode:'full'}),operations=capabilities.workbench?.operations;
            assert.ok(operations&&typeof operations==='object','Actual canonical Workbench operation schemas');
            evidence.workbenchParity={catalog:capabilities,normalization:['request/observation timing fields only','positive original allocation deadline compared independently','delivery result_digest binds retained timing; native delivery/scope checked before normalization'],cases:[]};
            let originalClipboard=await vscode.env.clipboard.readText(),ownedClipboard:string|undefined,workbenchFailure:unknown;
            const copied=async(format:'json'|'markdown',selected:boolean,verify:(value:string)=>void)=>{
                const prior=await vscode.env.clipboard.readText();if(prior!==ownedClipboard)originalClipboard=prior;
                const sentinel='airk-owned-copy-'+randomBytes(16).toString('hex');await vscode.env.clipboard.writeText(sentinel);ownedClipboard=sentinel;
                await ui('workbenchCopy',{format,selected});
                for(let count=0;count<128;count++){
                    assert.ok(Date.now()<sharedDeadline,'Original Workbench copy deadline');const value=await vscode.env.clipboard.readText();
                    if(value!==sentinel){assert.ok(Buffer.byteLength(value)<=1048576,'Complete bounded Workbench UI copy');verify(value);ownedClipboard=value;return value;}
                    await sleep(25);
                }
                throw Error('Actual Workbench copy did not complete');
            };
            try{for(const [tool,arguments_]of [['capabilities',{}],['search',{query:own,mode:'lexical',limit:5,max_bytes:8192,full_chunks:true}]] as const){
                await vscode.commands.executeCommand('airk.workbench.newSession');await vscode.commands.executeCommand('airk.workbench.focus');
                const fields=Object.fromEntries(Object.entries(arguments_).map(([key,value])=>[toolFormFields(operations[tool]).find(field=>field.name===key)?.label??key,String(value)]));
                const rendered=await ui('workbenchRead',{tool,fields});assertInstalledInputCatalog(operations,rendered.catalog);
                const json=await copied('json',false,value=>{
                    const export_=JSON.parse(value),turn_=export_.turns?.at(-1);
                    assert.equal(export_.schema,'airk.workbench.export/1');assert.equal(export_.turns.length,1);assert.equal(turn_?.input,tool+' form');assert.equal(turn_?.request?.schema,'airk.parse.request/1');assert.equal(turn_.request.mode,'execute');assert.equal(turn_.request.plan.steps.length,1);assert.equal(turn_.request.plan.steps[0].tool,tool);assert.deepEqual(turn_.request.plan.steps[0].args,arguments_);assert.deepEqual(turn_.envelope,rendered.envelope);
                }),exported=JSON.parse(json),turn=exported.turns?.at(-1);
                assert.equal(exported.schema,'airk.workbench.export/1');assert.equal(exported.turns.length,1,'Isolated real Workbench session');assert.ok(turn?.request&&turn.envelope,'Original request and full reply retained');
                assert.deepEqual(turn.envelope,rendered.envelope,'Actual Complete response and full JSON retain identical raw result');
                const session={schema:1,id:'installed-observation',project:exported.project,title:'Actual UI',createdAt:1,updatedAt:1,revision:1,omitted:exported.omitted_turns,turns:exported.turns} as WorkbenchSession;
                assert.equal(turn.request.schema,'airk.parse.request/1');assert.equal(turn.request.mode,'execute');assert.equal(turn.request.plan.steps.length,1);assert.equal(turn.request.plan.steps[0].tool,tool);
                assert.deepEqual(turn.request.plan.steps[0].args,arguments_,'Exact typed UI form arguments preserved before native dispatch');
                const exact=(format:'json'|'markdown',selected=false)=>(value:string)=>assert.equal(value,installedWorkbenchCopy(session,format,selected),'Actual copied export must match before claiming clipboard ownership');
                const markdown=await copied('markdown',false,exact('markdown'));
                const copies={json,markdown,...(tool==='search'?{selectedJson:await copied('json',true,exact('json',true)),selectedMarkdown:await copied('markdown',true,exact('markdown',true))}:{})};
                assertInstalledWorkbenchCopies(session,copies,tool==='search');
                // Execute the exact retained request on canonical MCP and native
                // CLI paths; these are reads, with no reshaped arguments/replay.
                const parityClient=new Mcp({...definition,headers:{...definition.headers,'X-AIRK-Response-Mode':'full'}},definition.ca),transport:{native:unknown[];failure?:unknown}={native:[]};
                evidence.workbenchParity.transports??=[];evidence.workbenchParity.transports.push(transport);
                let wire:any;try{await parityClient.start();wire=await parityClient.request('tools/call',{name:'airk_parse',arguments:turn.request});}
                catch(error){transport.failure={code:'installed_parity_native_request_failed'};throw error;}
                finally{await finishInstalledClient(parityClient,transport,()=>mark('retained-evidence',evidence));}
                const mcp=installedToolBody(wire,{tool:'airk_parse',request:turn.request});
                const spec={command:runtimeBinding.executable,root:selection.backing.root,dataDir:selection.backing.data_root,collection:selection.backing.collection,name:'Installed parity',combinedSession:selection.session,service:{installDir:runtimeBinding.installDir,project:selection.backing.binding,direct:true}};
                const counter=path.join(folder,'native-request-budget');await fs.appendFile(counter,'x');assert.ok((await fs.stat(counter)).size<=512,'Shared scenario CLI read budget');
                const cliEnvelope=await nativeRunner(runtimeBinding.executable,[...launchArgs(spec,'tools'),'call','airk_parse','--args',JSON.stringify(turn.request)],Math.min(35000,sharedDeadline-Date.now()));
                const cli=installedToolBody({result:cliEnvelope},{tool:'airk_parse',request:turn.request});
                const observed={tool,arguments:arguments_,ui:rendered,copies,request:turn.request,mcp_wire:wire,cli_envelope:cliEnvelope};evidence.workbenchParity.cases.push(observed);await mark('retained-evidence',evidence);
                for(const result of [turn.envelope,mcp,cli]){
                    assert.equal(result.status,'complete');assert.equal(result.executed,true);assert.ok(Array.isArray(result.steps)&&result.steps.length===1,'Full single-step parse result');
                    if(tool==='search'){
                        // A selected-scope plan step retains its complete native tool envelope
                        // (text and structured copies), as native combined-read parity verifies.
                        const member=installedToolBody({result:result.steps[0].result});
                        scoped(member,transportSelection);
                        assert.equal(member.delivery_observation?.digest_basis,'canonical_body_without_delivery_observation');
                        assert.match(member.delivery_observation?.receipt?.result_digest,/^[a-f0-9]{64}$/,'Original native delivery digest retained before comparison');
                        assert.equal(member.delivery_observation?.receipt?.service_instance,definition.nativeConnection.endpoint.service_instance,'Native delivery belongs to the selected actual service');
                    }
                }
                assert.deepEqual(installedParityValue(mcp),installedParityValue(turn.envelope),'Complete actual Workbench/MCP parity');
                assert.deepEqual(installedParityValue(cli),installedParityValue(turn.envelope),'Complete actual Workbench/CLI parity');
                assertInstalledEnvelopeParity(wire.result,cliEnvelope);
            }}catch(error){workbenchFailure=error;throw error;}finally{
                try{if(ownedClipboard!==undefined&&await vscode.env.clipboard.readText()===ownedClipboard){await vscode.env.clipboard.writeText(originalClipboard);evidence.workbenchParity.clipboard_restored=true;}else evidence.workbenchParity.clipboard_restored='intervening_clipboard_change_preserved';}
                catch(error){evidence.workbenchParity.clipboard_cleanup_failure=true;if(!workbenchFailure)throw error;}
            }
            checks.push('actual installed Workbench full canonical results, native input catalog, JSON/Markdown full and selected UI copies agree with MCP and CLI');
            const control=runtimeHost.client(runtimeBinding);
            try{evidence.c8=await checkInstalledEditorC8({deadline:sharedDeadline,ui,mark,
                rpc:(method,expected)=>control.rpc(method,expected),call:async(name,args)=>{const value=await client.call(name,args);if(name==='diagnostics')scoped(value,transportSelection);return value;},
                uiStatus:async()=>{
                    const ownership=JSON.parse(await fs.readFile(path.join(folder,'profile-'+role+'-owned.json'),'utf8'));
                    assert.equal(ownership.private_user_data_dir,true);assert.equal(ownership.label,role);
                    const storage=path.join(ownership.path,'User','globalStorage','iokaio.airk'),descriptor=launchDescriptorPath(storage);
                    assert.equal(path.toNamespacedPath(await fs.realpath(descriptor)),path.toNamespacedPath(descriptor));
                    const assistance=new LegacyMcp({label:'Private installed Assistance',command:runtimeBinding.executable,args:['explorer-assistance','--descriptor',descriptor],cwd:vscode.Uri.file(source)} as vscode.McpStdioServerDefinition);
                    let primary=false;
                    try{await assistance.start(ASSISTANCE_TOOLS);
                        // native read: one standalone UI observation proves actual attachment without enriching a combined native reply.
                        const reply=await assistance.request('tools/call',{name:'airk_explorer_ui_status',arguments:{}});assistance.observations.push({name:'airk_explorer_ui_status',result:reply});
                        assert.notEqual(reply.result?.isError,true,JSON.stringify(reply));const body=installedToolBody(reply);return body.ui;
                    }catch(error){primary=true;throw error;}finally{
                        evidence.standaloneAssistance={transport:assistance.observations};
                        try{evidence.standaloneAssistance.cleanup=await assistance.close();}catch(error){evidence.standaloneAssistance.cleanup_failure=String(error);if(!primary)throw error;}
                        try{await mark('standalone-assistance',evidence.standaloneAssistance);}catch(error){if(!primary)throw error;}
                    }
                },
                lexical:async()=>{const value=scoped(await client.call('search',{query:own,mode:'lexical',limit:5}),transportSelection);assert.ok(value.hits.length>0);}});}finally{control.dispose();}
            checks.push('actual HTTPS help, installed standalone Assistance and single-dispatch model Stop preserve lexical evidence and service ownership');
            await finishInstalledClient(client,evidence,mark);
            const ownedConfigurations:Array<{agent:InstalledAgent;file:string;before:string;connection:HttpConnectionInfo;credential_sha256:string}>=[];
            evidence.c8Endpoint=await checkInstalledEditorC8Endpoint({deadline:sharedDeadline,ui,definitions,mark,
                installation:{installDir:path.join(folder,'native-service'),runtimeSha256:process.env.AIRK_EDITOR_RUNTIME_SHA256!},
                prepareOwnedConfigurations:async()=>{
                    const host=hostFor(),observed=await host.discover();assert.ok(observed.inspection);const binding=host.binding(observed.inspection,observed.installDir);
                    for(const agent of ['codex','claude','copilot'] as const){
                        const parts=agent==='codex'?['.codex','config.toml']:agent==='claude'?['.mcp.json']:['.vscode','mcp.json'],file=path.join(source,...parts);
                        await fs.mkdir(path.dirname(file),{recursive:true});assert.equal(path.toNamespacedPath(await fs.realpath(path.dirname(file))),path.toNamespacedPath(path.dirname(file)),'Config parent remains in the exact owned source');
                        const choice={kind:'combined' as const,selection:selection.session},input=await publishNativeInput(binding.controlDir,JSON.stringify(choice),()=>assert.ok(Date.now()<sharedDeadline&&vscode.workspace.isTrusted));
                        let connection:HttpConnectionInfo;try{connection=await host.httpConnection(binding,choice,input,randomBytes(16).toString('hex'),agent==='claude'?'claude-code':agent==='copilot'?'vscode':'codex');}finally{await removeNativeInput(input);}
                        const before=ownedConfigurationSource(agent,connection);await fs.writeFile(file,before,{flag:'wx'});
                        const material=await httpMaterial(connection),credential_sha256=createHash('sha256').update(material.credential).digest('hex');
                        ownedConfigurations.push({agent,file,before,connection,credential_sha256});
                    }
                    evidence.ownedConfigurationRefresh={scope:'only private fixture source configurations; no real user home',before:ownedConfigurations};await mark('retained-evidence',evidence);
                },
                verifyOwnedConfigurations:async()=>{
                    const host=hostFor(),observed=await host.discover();assert.ok(observed.inspection);const binding=host.binding(observed.inspection,observed.installDir),results=[];
                    for(const owned of ownedConfigurations){
                        const current=await host.httpConnectionInfo(binding,owned.connection.connection_id),expected=refreshHttpAgentConfig(owned.agent,owned.before,current,owned.connection.endpoint.url);
                        let after='';for(let count=0;count<128;count++){assert.ok(Date.now()<sharedDeadline,'Original configuration refresh deadline');after=await fs.readFile(owned.file,'utf8');if(after===expected)break;await sleep(100);}
                        results.push({...owned,current,after});evidence.ownedConfigurationRefresh.after=results;await mark('retained-evidence',evidence);
                        assert.equal(createHash('sha256').update((await httpMaterial(current)).credential).digest('hex'),owned.credential_sha256,'Ordinary restart preserves the exact native credential');
                        assertOwnedConfigurationRefresh(owned.agent,owned.before,after,owned.connection,current);
                    }
                    assert.equal(results.length,3,'Every supported owned saved config refreshed');
                },
                verifyFresh:async next=>{
                    const fresh=new Mcp(next,next.ca),retained:{native:unknown[];failure?:unknown}={native:[]};
                    evidence.native.push({endpointRefreshVerification:retained});
                    try{await fresh.start();scoped(await fresh.call('status',{response_mode:'full'}),transportSelection);}
                    catch(error){retained.failure={error:String(error),stack:error instanceof Error?error.stack:undefined,native:error instanceof NativeReplyError?error.reply:error instanceof InstalledHttpFailure?{kind:error.kind,...error.detail}:undefined};throw error;}
                    finally{await finishInstalledClient(fresh,retained,()=>mark('retained-evidence',evidence));}
                }});
            // The refreshed owned .vscode/mcp.json now names the current endpoint, so it is a
            // current manual VS Code connection and the provider must not add a second copy.
            assert.equal((await definitions()).length,0,'A current manual VS Code configuration owns the connection');
            const manual=ownedConfigurations.find(owned=>owned.agent==='copilot');assert.ok(manual,'Owned VS Code configuration');
            assert.equal(path.toNamespacedPath(await fs.realpath(manual.file)),path.toNamespacedPath(manual.file));await fs.rm(manual.file);
            // Removing that owned file returns the same saved selection to the provider.
            const resumed=await installedRestoredDefinition(definitions,selection.session,observations=>mark('endpoint-refresh-definition-observations',observations),sharedDeadline);
            assert.ok(sameHttpAuthority(resumed.nativeConnection.selection,{kind:'combined',selection:selection.session}),'Endpoint refresh preserves exact authority');
            client=new Mcp(resumed,resumed.ca);await client.start();
            checks.push('actual installed endpoint refresh retains selection and trust after owned port collision');
            await vscode.commands.executeCommand('airk.explorer.search','return 42');await mark('explorer-ready');await waitFor(path.join(folder,'explorer-checked.json'));
            checks.push('installed Explorer cited search, current Index sets and live help');
            await ui('page',{kind:'projects'},route.projects);await ui('inspect',{kind:'project',id:selection.project});
            const revoke=await ui('deleteSetDraft');assert.equal(revoke.review.request.operation.operation,'delete_project');
            const receipt=applied(await ui('apply'));assert.ok(receipt.verifiedFencedReceipt?.proof);
            assert.equal(revoke.review.request.operation.id,transportSelection.project);
            const observedBefore=client.attempts;let refusal:unknown;
            try{await client.call('symbols',{name:own,compact:false});}catch(error){refusal=error;}
            evidence.native.push({revocationRefusal:refusal instanceof NativeReplyError?refusal.reply:{error:String(refusal),detail:refusal instanceof InstalledHttpFailure?refusal.detail:undefined}});await mark('retained-evidence',evidence);
            if(refusal instanceof NativeReplyError){
                const envelope=refusal.reply as any;assert.equal(envelope.result?.isError,true);
                installedRevocation(installedToolBody({...envelope,error:undefined}),transportSelection.session);
                evidence.revocationDelivery={kind:'typed_native_refusal',query_replayed:false};
            }else{
                installedDeliveryFence(refusal,revoke.review,receipt,transportSelection.session);
                evidence.revocationDelivery={kind:'native_completed_deletion_transport_fence',detail:(refusal as InstalledHttpFailure).detail,query_submission:'unknown',query_replayed:false,evidence_delivered:false};
            }
            assert.equal(client.attempts,observedBefore+1,'Revocation must not replay or rebroker the query');
            checks.push('deleting the selected index set fences the retained MCP session');
        }
    }catch(error){evidence.failure={error:String(error),stack:error instanceof Error?error.stack:undefined,native:error instanceof NativeReplyError?error.reply:error instanceof InstalledHttpFailure?{kind:error.kind,...error.detail}:undefined};throw error;}finally{await finishInstalledClient(client,evidence,mark);}
    await mark('transport-closed');
    if(['same','different'].includes(scenario))await waitFor(marker('transport-closed',role==='a'?'b':'a'));
    if(role==='a'){
        const stop=await ui('serviceStop',{installDir:path.join(folder,'native-service'),runtimeSha256:process.env.AIRK_EDITOR_RUNTIME_SHA256},'#manage/service');
        await publishMailbox(path.join(folder,'native-stop.json'),stop);
        evidence.native.push({ownedServiceStop:stop});
    }else await waitFor(path.join(folder,'native-stop.json'));
    await vscode.commands.executeCommand('airk.explorer.open',route.window);
    await finish();
}
