// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import {createHash,X509Certificate} from 'node:crypto';
import {rootCertificates} from 'node:tls';
import * as globalInventory from '../globalAgentInventory';
const realGlobalInventory=globalInventory.inventoryGlobalAgentConfigs;
import * as fs from 'node:fs/promises';
import { serviceTestHost } from './serviceTestHost';
import { renderServiceDashboard } from '../serviceDashboard';
import { RuntimeHost, legacyOriginLocatorIdentity, type LegacyOriginLocator } from '../serviceBootstrap';
import { ServiceWorkspace } from '../serviceWorkspace';
import { ServiceError } from '../serviceClient';
import { httpConfigurationConnectionId, refreshHttpAgentConfig } from '../agentMigration';
import { httpAuthorityDigest } from '../serviceHttp';
import { ServiceConsoleController } from '../serviceConsoleController';
import * as path from 'node:path';
import {serializeServers} from '../model';
import {credentialEnvironment} from '../serviceHttp';
import {AssistanceIpcHost} from '../explorerAssistanceHost';
import {assistanceDirectory,launchDescriptorPath} from '../explorerAssistanceLaunch';
import * as assistanceLaunch from '../explorerAssistanceLaunch';
import {predecessorHttpFixture} from './fixtures/predecessorHttpFixture';
import {IntentJournal,INTENT_PREFIX,INTENT_LIMIT,INTENT_MAX_TOTAL_BYTES,chargedRecordBytes} from '../serviceIntents';

const scopedJournalIntent=(namespace:string,nonce:string)=>({enqueue:{project:'21'.repeat(32),caller:'22'.repeat(16),kind:'index' as const,key:{namespace,epoch:1,nonce}}});

for(const late of [false,true])test(`published clean-break Genesis discards a full predecessor journal so a new User Stop is durable while indexes build${late?' after a late old-window Memento write':''}`,async context=>{
    const f=await serviceTestHost(context,{automatic:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    const controller=f.context.subscriptions.find((value:any)=>typeof value.resolveAgentSpec==='function'),old=new IntentJournal(f.intentStorage,f.binding.controlDir,'34'.repeat(16));
    try{
        await old.ready();
        for(let index=1;index<=INTENT_LIMIT;index++){
            const nonce=index.toString(16).padStart(32,'0'),key=INTENT_PREFIX+old.scope+'.'+nonce;
            await f.intentStorage.create!(key,{schema:1,scope:old.scope,title:'Predecessor pending request',intent:scopedJournalIntent('34'.repeat(16),nonce),fixture_padding:'x'.repeat(31500)});
        }
        const charged=old.list().reduce((total,item)=>total+chargedRecordBytes(item.value),0);
        assert.equal(old.list().length,INTENT_LIMIT);assert.ok(charged>INTENT_MAX_TOTAL_BYTES*.95&&charged<=INTENT_MAX_TOTAL_BYTES,'exercise both the complete record count and near-full original byte allowance');
        const transition=RuntimeHost.prototype.transition,discover=RuntimeHost.prototype.discover;
        context.mock.method(RuntimeHost.prototype,'transition',async function(this:RuntimeHost,...args:Parameters<RuntimeHost['transition']>){
            const response=await transition.apply(this,args),observed=f.binding.rebuild!;
            if(late){const nonce='65'.padStart(32,'0');f.values.set(INTENT_PREFIX+old.scope+'.'+nonce,{schema:1,scope:old.scope,title:'Late predecessor window',intent:scopedJournalIntent('34'.repeat(16),nonce)});}
            f.binding.rebuild={...observed,stage:'indexing',cleanup:{complete:false,state:'waiting_for_indexes',failure:null,observation:null},indexes:observed.indexes.map(item=>({...item,readiness:'pending',job:{...(item.job as object),state:'pending',outcome:null}}))};return response;
        });
        context.mock.method(RuntimeHost.prototype,'discover',async function(this:RuntimeHost,...args:Parameters<RuntimeHost['discover']>){
            const observed=await discover.apply(this,args);if(f.binding.rebuild){assert.ok(observed.inspection);observed.inspection.registry_namespace=f.binding.rebuild.successor_namespace;}return observed;
        });
        const transport=controller.runtime().client(f.binding).transport,request=transport.request.bind(transport);
        context.mock.method(transport,'request',async(input:unknown)=>{
            const response=await request(input),name=typeof input==='string'?input:Object.keys(input as object)[0],namespace=f.binding.rebuild?.successor_namespace;
            if(namespace&&name==='request_epoch')return {...response,namespace};
            if(namespace&&name==='events')return {...response,page:{...response.page,namespace,tail:{...response.page.tail,namespace}}};
            return response;
        });
        let stops=0;
        context.mock.method(RuntimeHost.prototype,'lifecycle',async(...[_binding,intent]:Parameters<RuntimeHost['lifecycle']>)=>{
            stops++;assert.equal(intent.lifecycle.action,'stop');assert.equal(intent.lifecycle.key.namespace,f.binding.rebuild!.successor_namespace);
            assert.equal(controller.console.intentStorage.keys().filter((key:string)=>key.startsWith(INTENT_PREFIX)).length,1,'the new Stop itself is durable before native dispatch');
            f.state.stopped=true;return {status:'lifecycle',record:{id:'71'.repeat(32),phase:'complete',request:{...structuredClone(intent.lifecycle),manager:false}}};
        });
        await f.commands.get('airk.installCli')!();await controller.console.controller.refresh();await controller.console.run('stop');
        assert.equal(stops,1,`new Stop must be admitted after old authority is revoked: ${controller.console.controller.state.message??'no refusal detail'}`);
        assert.equal(f.state.stopped,true);assert.deepEqual(old.list(),[]);
        assert.equal(f.binding.rebuild!.stage,'indexing');assert.equal(f.values.get(`airk.cleanBreak.complete.1.${f.binding.installDir}`),undefined,'fresh index and physical cleanup completion remain unqualified');
    }finally{await f.close();}
});

for(const late of ['foreign','malformed','wrong_namespace'] as const)test(`accepted old-scope cleanup preserves a late ${late} Memento request`,async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'}),old=new IntentJournal(f.intentStorage,f.binding.controlDir,'34'.repeat(16));
    try{
        await old.prepare('Captured old request',scopedJournalIntent('34'.repeat(16),'61'.repeat(16)));
        const foreign=new IntentJournal(f.intentStorage,f.binding.controlDir,'99'.repeat(16)),scope=late==='foreign'?foreign.scope:old.scope,nonce='62'.repeat(16),key=INTENT_PREFIX+scope+'.'+nonce;
        const value={schema:1,scope,title:'Late preserved request',intent:late==='malformed'?{shutdown:{}}:scopedJournalIntent('99'.repeat(16),nonce)},transition=RuntimeHost.prototype.transition;
        context.mock.method(RuntimeHost.prototype,'transition',async function(this:RuntimeHost,...args:Parameters<RuntimeHost['transition']>){const result=await transition.apply(this,args);f.values.set(key,structuredClone(value));return result;});
        await f.commands.get('airk.installCli')!();assert.deepEqual(f.values.get(key),value,'retired-scope authority never deletes unrelated or malformed request bytes');
        const controller=f.context.subscriptions.find((item:any)=>typeof item.resolveAgentSpec==='function');
        if(late==='foreign'){assert.equal(controller.cleanBreakClientResult.complete,true);assert.ok(f.intentStorage.keys().includes(key));}
        else{assert.equal(controller.cleanBreakClientResult.complete,false);assert.equal(f.values.has(`airk.cleanBreak.complete.1.${f.binding.installDir}`),false);}
        assert.equal(f.writes.length,0);
    }finally{await f.close();}
});

test('accepted clean break retires only its predecessor journal scope and preserves fresh and foreign unknown requests',async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    const old=new IntentJournal(f.intentStorage,f.binding.controlDir,'34'.repeat(16)),foreign=new IntentJournal(f.intentStorage,f.binding.controlDir,'99'.repeat(16));
    const oldNonce='31'.repeat(16),foreignNonce='32'.repeat(16);let fresh:IntentJournal|undefined;
    try{
        await old.prepare('Predecessor unknown',scopedJournalIntent('34'.repeat(16),oldNonce));await foreign.prepare('Foreign unknown',scopedJournalIntent('99'.repeat(16),foreignNonce));
        const foreignBefore=structuredClone(foreign.list()),transition=RuntimeHost.prototype.transition;
        context.mock.method(RuntimeHost.prototype,'transition',async function(this:RuntimeHost,...args:Parameters<RuntimeHost['transition']>){
            const response=await transition.apply(this,args);fresh=new IntentJournal(f.intentStorage,f.binding.controlDir,String(f.binding.rebuild!.successor_namespace));await fresh.prepare('Fresh unknown',scopedJournalIntent(String(f.binding.rebuild!.successor_namespace),'33'.repeat(16)));return response;
        });
        await f.commands.get('airk.installCli')!();
        assert.deepEqual(old.list(),[]);assert.deepEqual(foreign.list(),foreignBefore);assert.equal(fresh!.list().length,1);
        const capture=[...f.values.entries()].find(([name])=>name.startsWith('airk.cleanBreak.clients.1.'))?.[1] as any;
        assert.equal(capture.result.complete,true);assert.equal(capture.result.journal.complete,true);assert.equal(capture.result.journal.scope,old.scope);assert.equal(capture.result.journal.operation_id,f.binding.rebuild!.operation_id);
        assert.equal(f.writes.length,0,'retiring predecessor UI metadata never submits or replays an index job');
    }finally{await f.close();}
});

for(const changed of ['operation','file'] as const)test(`clean-break journal cleanup preserves captured records after ${changed} changes`,async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    const old=new IntentJournal(f.intentStorage,f.binding.controlDir,'34'.repeat(16));
    try{
        await old.prepare('Unknown predecessor',scopedJournalIntent('34'.repeat(16),'41'.repeat(16)));const before=structuredClone(old.list()),transition=RuntimeHost.prototype.transition;
        context.mock.method(RuntimeHost.prototype,'transition',async function(this:RuntimeHost,...args:Parameters<RuntimeHost['transition']>){
            const response=await transition.apply(this,args),entry=[...f.values.entries()].find(([name])=>name.startsWith('airk.cleanBreak.clients.1.'))!,capture=structuredClone(entry[1]) as any;
            if(changed==='operation'){capture.operation='42'.repeat(16);f.values.set(entry[0],capture);}
            else{const file=capture.journal.files.find((item:any)=>item.name.startsWith('b-')).name;await fs.appendFile(path.join(f.context.globalStorageUri.fsPath,'service-intents',file),' ');}
            return response;
        });
        await f.commands.get('airk.installCli')!();assert.deepEqual(old.list(),before,'changed ownership evidence cannot authorize deleting the pending request');
        const controller=f.context.subscriptions.find((value:any)=>typeof value.resolveAgentSpec==='function');assert.equal(controller.cleanBreakClientResult.complete,false);
        assert.match(JSON.stringify(controller.cleanBreakClientResult),changed==='operation'?/crossed_native_operation/:/changed|blocked/);
        assert.equal(f.writes.length,0);
    }finally{await f.close();}
});

test('same-contract runtime upgrade retains its current unknown journal requests',async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false,bundledVersion:'1.8.17',runtimeVersion:'1.8.16',installedVersion:'1.8.17'});
    f.binding.cleanBreak={required:false,baseline_version:'1.8.16',state_contract:'airk-state/1',installed_contract:{schema:1}};
    const journal=new IntentJournal(f.intentStorage,f.binding.controlDir,'34'.repeat(16));
    try{
        await journal.prepare('Current unknown',scopedJournalIntent('34'.repeat(16),'51'.repeat(16)));const before=structuredClone(journal.list());
        await f.commands.get('airk.installCli')!();assert.equal(f.binding.version,'1.8.17');assert.equal(f.binding.rebuild,undefined);assert.deepEqual(journal.list(),before);
        assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.cleanBreak.clients.1.')),false);assert.equal(f.writes.length,0);
    }finally{await f.close();}
});

async function predecessorHostHttp(f:Awaited<ReturnType<typeof serviceTestHost>>){
    const controller=f.context.subscriptions.find((value:any)=>typeof value.resolveAgentSpec==='function');
    const row=[...f.registrations.values()][0],registration={binding:row.binding,root:row.root,data_root:row.data_root,collection:row.collection};
    // Seed the same authoritative native observation that an earlier editor
    // session obtained, without bypassing the production prebaseline guard.
    const info=await controller.runtime().httpLegacyConnection(f.binding,registration,'fixture-selection','19'.repeat(32),'29'.repeat(16),'claude');
    const filename=path.join(row.root,'.mcp.json');
    await fs.writeFile(filename,serializeServers('claude',[{key:'airk',label:'AIRK',command:'',args:[],http:info}]));migrationEditor(f);
    return {controller,info};
}

test('stopped predecessor cleanup uses expired disposal ownership when old credential and CA files are already absent',async context=>{
    const f=await serviceTestHost(context,{automatic:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    const entries=new Map<string,{type:number;value:string}>(),collection=f.context.environmentVariableCollection;
    collection.get=(name:string)=>entries.get(name);collection.replace=(name:string,value:string)=>entries.set(name,{type:f.stub.EnvironmentVariableMutatorType.Replace,value});collection.delete=(name:string)=>entries.delete(name);
    const prototype=RuntimeHost.prototype as any,prior=Object.getOwnPropertyDescriptor(prototype,'lookupPredecessorHttpOwnership');
    try{
        const {controller,info}=await predecessorHostHttp(f);await controller.provisionHttpInfo(info,'claude',()=>{},f.context.globalStorageUri.fsPath);
        const memento='airk.http.legacy.1.'+'91'.repeat(32);f.values.set(memento,structuredClone(info));
        const originalControl=f.binding.controlDir,originalSha=f.binding.runtimeSha256,proof=predecessorHttpFixture(originalControl,originalSha,info.connection_id,'expired');proof.endpoint=structuredClone(info.endpoint);proof.snapshot.endpoint=structuredClone(info.endpoint);
        Object.defineProperty(prototype,'lookupPredecessorHttpOwnership',{configurable:true,writable:true,value:async(binding:any,id:string,expected:any)=>{
            assert.equal(id,info.connection_id);assert.equal(expected.predecessor,originalSha);assert.equal(expected.controlDir,originalControl);
            return {...structuredClone(proof),active:binding.runtimeSha256,operation_id:expected.operationId??null,qualification:{...proof.qualification,source:expected.operationId?'saved_predecessor_snapshot':'current_predecessor_metadata',current_namespace:expected.operationId?'78'.repeat(16):proof.namespace,current_namespace_ownership:expected.operationId?'not_owned':'same_predecessor'}};
        }});
        context.mock.method(RuntimeHost.prototype,'lookupHttpConnection',async()=>{throw new Error('ordinary live HTTP lookup is forbidden during predecessor disposal');});
        await fs.unlink(info.credential_file);await fs.unlink(info.ca_certificate_file);f.state.stopped=true;
        await f.commands.get('airk.installCli')!();
        assert.equal(entries.has(credentialEnvironment(info)),false);assert.equal(entries.has('NODE_EXTRA_CA_CERTS'),false);assert.equal(f.values.has(memento),false);
        const capture=[...f.values.entries()].find(([name])=>name.startsWith('airk.cleanBreak.clients.1.'))?.[1] as any;
        assert.equal(capture.result.complete,true);assert.equal(JSON.stringify(capture).includes('airk1.'),false);assert.equal(f.state.httpConnections.length,1,'disposal never restarts old authority or enrolls a replacement');
        assert.equal(f.state.stopped,false,'the predecessor Stop belongs to the discarded namespace, not the new Genesis');
    }finally{if(prior)Object.defineProperty(prototype,'lookupPredecessorHttpOwnership',prior);else delete prototype.lookupPredecessorHttpOwnership;await f.close();}
});

test('clean-break completion preserves a new User Stop after selection without ensuring a service',async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    const transition=RuntimeHost.prototype.transition;let ensured=0;
    context.mock.method(RuntimeHost.prototype,'transition',async function(this:RuntimeHost,...args:Parameters<RuntimeHost['transition']>){const result=await transition.apply(this,args);f.state.stopped=true;return result;});
    context.mock.method(RuntimeHost.prototype,'ensure',async()=>{ensured++;throw new ServiceError('service_explicitly_stopped');});
    try{
        await assert.doesNotReject(f.commands.get('airk.installCli')!());assert.equal(f.state.stopped,true);assert.equal(ensured,0,'completion reporting cannot elect or restart a newly stopped service');
        assert.equal(f.values.get(`airk.cleanBreak.complete.1.${f.binding.installDir}`),f.binding.rebuild!.operation_id);
    }finally{await f.close();}
});

test('offline headless completion consumes scoped saved reset metadata without client IPC or ensure',async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    try{
        const controller=f.context.subscriptions.find((value:any)=>typeof value.resolveAgentSpec==='function'),runtime=controller.runtime();await runtime.transition(f.binding,await runtime.reviewTransition(f.binding,'upgrade'));
        const observation=await runtime.discover();controller.binding=runtime.binding(observation.inspection,observation.installDir);
        let attempted=0;context.mock.method(RuntimeHost.prototype,'ensure',async()=>{attempted++;throw new ServiceError('service_unavailable');});context.mock.method(RuntimeHost.prototype,'client',()=>{attempted++;throw new Error('Saved completion metadata requires no client IPC');});
        await assert.doesNotReject(controller.installationComplete(true));assert.equal(attempted,0);
    }finally{await f.close();}
});

test('clean break retires a creator-proven old trust copy after its window environment reference was withdrawn',async context=>{
    const f=await serviceTestHost(context,{automatic:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    const entries=new Map<string,{type:number;value:string}>(),collection=f.context.environmentVariableCollection;
    collection.get=(name:string)=>entries.get(name);collection.replace=(name:string,value:string)=>entries.set(name,{type:f.stub.EnvironmentVariableMutatorType.Replace,value});collection.delete=(name:string)=>entries.delete(name);
    try{
        const {controller,info}=await predecessorHostHttp(f);await controller.provisionHttpInfo(info,'claude',()=>{},f.context.globalStorageUri.fsPath);const file=entries.get('NODE_EXTRA_CA_CERTS')!.value;
        entries.clear();await f.commands.get('airk.installCli')!();
        await assert.rejects(fs.stat(file),{code:'ENOENT'},'a missing environment reference does not discard actual creator/native ownership evidence');
    }finally{await f.close();}
});

test('a fresh namespace owning the same HTTP id and URL blocks predecessor configuration and host artifact disposal',async context=>{
    const f=await serviceTestHost(context,{automatic:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    const entries=new Map<string,{type:number;value:string}>(),collection=f.context.environmentVariableCollection;
    collection.get=(name:string)=>entries.get(name);collection.replace=(name:string,value:string)=>entries.set(name,{type:f.stub.EnvironmentVariableMutatorType.Replace,value});collection.delete=(name:string)=>entries.delete(name);
    try{
        const {controller,info}=await predecessorHostHttp(f);await controller.provisionHttpInfo(info,'claude',()=>{},f.context.globalStorageUri.fsPath);
        const file=path.join(f.folders[0].uri.fsPath,'.mcp.json'),before=await fs.readFile(file,'utf8'),trust=entries.get('NODE_EXTRA_CA_CERTS')!.value,lookup=RuntimeHost.prototype.lookupPredecessorHttpOwnership;
        context.mock.method(RuntimeHost.prototype,'lookupPredecessorHttpOwnership',async function(this:RuntimeHost,binding:Parameters<RuntimeHost['lookupPredecessorHttpOwnership']>[0],id:string,scope:Parameters<RuntimeHost['lookupPredecessorHttpOwnership']>[2]){
            const value=await lookup.call(this,binding,id,scope);if(!scope.operationId)return value;
            return {...value,status:'unavailable',ownership:null,credential_sha256:null,ca_certificate_file:null,credential_file:null,qualification:{...value.qualification,current_namespace:'78'.repeat(16),current_namespace_ownership:'owned'}} as any;
        });
        await f.commands.get('airk.installCli')!();
        assert.equal(await fs.readFile(file,'utf8'),before,'same bytes cannot distinguish freshly reenrolled identical selectors');assert.ok(entries.has(credentialEnvironment(info)));assert.ok(await fs.stat(trust));
        const capture=[...f.values.entries()].find(([name])=>name.startsWith('airk.cleanBreak.clients.1.'))?.[1] as any;
        assert.equal(capture.result.complete,false);assert.match(JSON.stringify(capture.result.configurations),/clean_break_http_current_authority_unconfirmed/);
        assert.equal(capture.result.httpRevalidation.queries[0].qualification.current_namespace_ownership,'owned');assert.equal(capture.result.httpRevalidation.snapshot.owners[0].connection_id,info.connection_id);
    }finally{await f.close();}
});

test('headless reset cleanup uses its retained predecessor snapshot after original control files disappear',async context=>{
    const f=await serviceTestHost(context,{automatic:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    try{
        const {controller,info}=await predecessorHostHttp(f),originalControl=f.binding.controlDir,sourceFile=path.join(f.folders[0].uri.fsPath,'.mcp.json'),memento='airk.http.legacy.1.'+'92'.repeat(32);
        f.values.set(memento,structuredClone(info));const runtime=controller.runtime();await runtime.transition(f.binding,await runtime.reviewTransition(f.binding,'upgrade'));
        assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.cleanBreak.clients.1.')),false,'headless activation has no private editor capture');
        const metadata=f.binding.cleanBreak!.predecessor_client_metadata as any,candidate=path.join(f.binding.installDir,'runtimes','ef'.repeat(32),'airk');
        metadata.legacy_launchers.legacy_runtime_images=[{path:candidate,sha256:'af'.repeat(32),runtime_sha256:'ef'.repeat(32)}];
        const source=JSON.parse(await fs.readFile(sourceFile,'utf8'));source.mcpServers.new_candidate={command:candidate,args:['--root',f.folders[0].uri.fsPath,'--data-dir',f.root,'--collection','new','mcp']};await fs.writeFile(sourceFile,JSON.stringify(source));
        assert.ok(originalControl.startsWith(f.root+path.sep));await fs.rm(originalControl,{recursive:true,force:true});
        context.mock.method(RuntimeHost.prototype,'inspectLegacyLaunchers',async()=>{throw new Error('retired predecessor command ownership must come from native saved metadata');});
        context.mock.method(RuntimeHost.prototype,'lookupHttpConnection',async()=>{throw new Error('headless recovery cannot consult the old live service');});
        const extension=require('../extension');await extension.deactivate();const replacement=await extension.activate(f.context);await replacement.whenReady();migrationEditor(f);await f.commands.get('airk.installCli')!();
        const saved=JSON.parse(await fs.readFile(sourceFile,'utf8'));assert.equal(saved.mcpServers.airk,undefined);assert.deepEqual(saved.mcpServers.new_candidate,source.mcpServers.new_candidate,'full legacy inventory never qualifies staged candidate commands for removal');
        assert.equal(f.values.has(memento),false);assert.equal(f.calls.filter(call=>call==='upgrade-applied').length,1);assert.equal(f.state.httpConnections.length,1);
        const capture=[...f.values.entries()].find(([name])=>name.startsWith('airk.cleanBreak.clients.1.'))?.[1] as any;
        assert.equal(capture.operation,f.binding.rebuild!.operation_id);assert.equal(capture.httpOwnership.qualification.source,'saved_predecessor_snapshot');assert.equal(capture.result.complete,true);
    }finally{await f.close();}
});

test('headless predecessor executable validation accepts ordinary installation and extended native Windows aliases',async context=>{
    const f=await serviceTestHost(context,{automatic:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    try{
        const row=[...f.registrations.values()][0],file=path.join(row.root,'.mcp.json');
        await fs.writeFile(file,JSON.stringify({mcpServers:{airk:{command:f.binding.executable,args:['--root',row.root,'--data-dir',row.data_root,'--collection',row.collection,'--service-project',row.binding,'mcp']}}}));migrationEditor(f);
        const controller=f.context.subscriptions.find((value:any)=>typeof value.resolveAgentSpec==='function'),runtime=controller.runtime();await runtime.transition(f.binding,await runtime.reviewTransition(f.binding,'upgrade'));
        const metadata=f.binding.cleanBreak!.predecessor_client_metadata as any;
        if(process.platform==='win32'){
            metadata.predecessor_executable.path=path.toNamespacedPath(metadata.predecessor_executable.path);
            for(const image of metadata.command_images)image.path=path.toNamespacedPath(image.path);
        }
        const extension=require('../extension');await extension.deactivate();const replacement=await extension.activate(f.context);await replacement.whenReady();await f.commands.get('airk.installCli')!();
        assert.equal(f.values.get(`airk.cleanBreak.complete.1.${f.binding.installDir}`),f.binding.rebuild!.operation_id,'native extended-path spelling must not invalidate exact approved predecessor executable scope');
        const scope=require('../runtimeRebuild') as any,sha='ab'.repeat(32),base='D:\\AIRK';
        assert.equal(scope.predecessorExecutableInRuntime(base,sha,`\\\\?\\D:\\AIRK\\runtimes\\${sha}\\airk.exe`,'win32'),true);
        assert.equal(scope.predecessorExecutableInRuntime(base,sha,`\\\\?\\D:\\AIRK\\runtimes\\${'cd'.repeat(32)}\\airk.exe`,'win32'),false);
    }finally{await f.close();}
});

test('clean break retires an orphan native-owned bare HTTP id memento and preserves an unrelated id',async context=>{
    const f=await serviceTestHost(context,{automatic:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    try{
        const {info}=await predecessorHostHttp(f);await fs.unlink(path.join(f.folders[0].uri.fsPath,'.mcp.json'));
        const owned='airk.http.connection.1.'+'93'.repeat(32),foreign='airk.http.connection.1.'+'94'.repeat(32);f.values.set(owned,info.connection_id);f.values.set(foreign,'95'.repeat(16));
        await f.commands.get('airk.installCli')!();
        assert.equal(f.values.has(owned),false,'a bare managed id requires native disposal lookup even without config, env or full-info references');
        assert.equal(f.values.get(foreign),'95'.repeat(16));
    }finally{await f.close();}
});

test('clean break inventories an unreferenced old CA copy after reload without adopting deletion authority',async context=>{
    const f=await serviceTestHost(context,{automatic:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    const entries=new Map<string,{type:number;value:string}>(),collection=f.context.environmentVariableCollection;
    collection.get=(name:string)=>entries.get(name);collection.replace=(name:string,value:string)=>entries.set(name,{type:f.stub.EnvironmentVariableMutatorType.Replace,value});collection.delete=(name:string)=>entries.delete(name);
    try{
        const {controller,info}=await predecessorHostHttp(f);await controller.provisionHttpInfo(info,'claude',()=>{},f.context.globalStorageUri.fsPath);
        const file=entries.get('NODE_EXTRA_CA_CERTS')!.value,bytes=await fs.readFile(file);entries.clear();await fs.unlink(path.join(f.folders[0].uri.fsPath,'.mcp.json'));
        const extension=require('../extension');await extension.deactivate();const replacement=await extension.activate(f.context);await replacement.whenReady();await f.commands.get('airk.installCli')!();
        assert.deepEqual(await fs.readFile(file),bytes,'membership in the old CA set is observation, never creator identity');
        const capture=[...f.values.entries()].find(([name])=>name.startsWith('airk.cleanBreak.clients.1.'))?.[1] as any;
        assert.equal(capture.result.complete,false,'a known remaining old CA copy prevents a false all-artifacts-clean completion');
        assert.match(JSON.stringify(capture.result.hostArtifacts),/clean_break_http_trust_ownership_unproven/);
    }finally{await f.close();}
});

for(const limit of ['unsafe','bytes'] as const)test(`managed trust census preserves ${limit} entries and refuses proof before expanding its read scope`,async context=>{
    const f=await serviceTestHost(context,{automatic:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    try{
        const controller=f.context.subscriptions.find((value:any)=>typeof value.resolveAgentSpec==='function'),directory=path.join(f.context.globalStorageUri.fsPath,'http-trust');await fs.mkdir(directory);
        if(limit==='unsafe'){await fs.mkdir(path.join(directory,'other'));await fs.writeFile(path.join(directory,'other','user-data'),'preserve');}
        else{await fs.writeFile(path.join(directory,'first'),Buffer.alloc(1048577));await fs.writeFile(path.join(directory,'second'),Buffer.alloc(1048577));}
        let probes=0;const blockers=await controller.censusPredecessorTrustCopies([],()=>{},async()=>{probes++;throw new Error('No native ownership probe is needed for unsupported read scope');});
        assert.ok(blockers.some((code:string)=>code.startsWith(limit==='unsafe'?'clean_break_http_trust_inventory_unsafe:':'clean_break_http_trust_inventory_bound:')));assert.equal(probes,0);
        if(limit==='unsafe')assert.equal(await fs.readFile(path.join(directory,'other','user-data'),'utf8'),'preserve');
        else{assert.equal((await fs.stat(path.join(directory,'first'))).size,1048577);assert.equal((await fs.stat(path.join(directory,'second'))).size,1048577);}
    }finally{await f.close();}
});

test('managed trust census enforces its total byte budget when candidates grow after the outer stat',async context=>{
    const f=await serviceTestHost(context,{automatic:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    try{
        const controller=f.context.subscriptions.find((value:any)=>typeof value.resolveAgentSpec==='function'),directory=path.join(f.context.globalStorageUri.fsPath,'http-trust');await fs.mkdir(directory);
        const files=new Set([path.join(directory,'first'),path.join(directory,'second')]);for(const file of files)await fs.writeFile(file,'x');
        const promises=require('node:fs/promises'),lstat=promises.lstat,open=promises.open,grown=new Set<string>();let readBytes=0;
        context.mock.method(promises,'lstat',async(filename:string,...args:unknown[])=>{
            const observed=await lstat(filename,...args);
            if(files.has(filename)&&!grown.has(filename)){grown.add(filename);await fs.writeFile(filename,Buffer.alloc(1048577));}
            return observed;
        });
        context.mock.method(promises,'open',async(filename:string,...args:unknown[])=>{
            const handle=await open(filename,...args);
            if(files.has(filename)){const read=handle.read.bind(handle);context.mock.method(handle,'read',async(...input:unknown[])=>{const result=await read(...input);readBytes+=result.bytesRead;return result;});}
            return handle;
        });
        const blockers=await controller.censusPredecessorTrustCopies([],()=>{},async()=>{throw new Error('No CA candidate requires a native proof');});
        assert.ok(readBytes<=2097152,'the claimed total budget limits actual reads, not stale metadata estimates');
        assert.ok(blockers.some((code:string)=>/^clean_break_http_trust_inventory_(?:bound|changed):/.test(code)),'growth is an explicit incomplete inventory observation');
        for(const file of files)assert.ok(await fs.stat(file),'a changing public candidate is preserved');
    }finally{await f.close();}
});

test('clean break retires the prior window token and trust environment before explicit fresh-namespace provisioning',async context=>{
    const f=await serviceTestHost(context,{automatic:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    const entries=new Map<string,{type:number;value:string}>(),collection=f.context.environmentVariableCollection;
    collection.get=(name:string)=>entries.get(name);collection.replace=(name:string,value:string)=>entries.set(name,{type:f.stub.EnvironmentVariableMutatorType.Replace,value});collection.delete=(name:string)=>entries.delete(name);
    try{
        entries.set('PATH',{type:f.stub.EnvironmentVariableMutatorType.Replace,value:'preserve-existing-cli-path'});
        // NTFS file IDs can exceed Number.MAX_SAFE_INTEGER. Exercise exact
        // capture/load/removal on every host, while ordinary reads retain their
        // real open-handle identity checks.
        const promises=require('node:fs/promises'),sync=require('node:fs'),lstat=promises.lstat,lstatSync=sync.lstatSync,open=promises.open,large=9007199254740993n;
        const identity=(filename:unknown,options:any,stat:any)=>options?.bigint&&String(filename).includes(path.sep+'http-trust'+path.sep)?Object.assign(Object.create(Object.getPrototypeOf(stat)),stat,{ino:large}):stat;
        context.mock.method(promises,'lstat',async(filename:unknown,options:any)=>identity(filename,options,await lstat(filename,options)));
        context.mock.method(sync,'lstatSync',(filename:unknown,options:any)=>identity(filename,options,lstatSync(filename,options)));
        context.mock.method(promises,'open',async(filename:unknown,...args:unknown[])=>{
            const handle=await open(filename,...args);
            if(String(filename).includes(path.sep+'http-trust'+path.sep)){
                const stat=handle.stat.bind(handle);
                context.mock.method(handle,'stat',async(options:any)=>identity(filename,options,await stat(options)));
            }
            return handle;
        });
        const {controller,info}=await predecessorHostHttp(f);
        await controller.provisionHttpInfo(info,'claude',()=>{},f.context.globalStorageUri.fsPath);
        const previousTrust=entries.get('NODE_EXTRA_CA_CERTS')!.value;
        assert.ok(entries.has(credentialEnvironment(info)));
        await f.commands.get('airk.installCli')!();
        assert.equal(entries.has(credentialEnvironment(info)),false,'old native token must stop reaching newly opened terminals');
        assert.equal(entries.has('NODE_EXTRA_CA_CERTS'),false,'an old owned issuer must not trip the reenrollment rotation guard');
        assert.equal(entries.get('PATH')!.value,'preserve-existing-cli-path','clean-break HTTP cleanup preserves CLI availability');
        await assert.rejects(fs.stat(previousTrust),{code:'ENOENT'},'a live-publisher-proven unchanged old public bundle is retired');
        const capture=[...f.values.entries()].find(([name])=>name.startsWith('airk.cleanBreak.clients.1.'))?.[1];
        assert.equal(JSON.stringify(capture).includes('airk1.'),false,'persisted recovery proof contains no plaintext token');
        assert.equal((capture as any).hostArtifacts.bundles[0].ino,large.toString(),'file identity is serialized exactly, without floating-point rounding');
        const next=structuredClone(info),id='39'.repeat(16),directory=path.join(f.binding.controlDir,'http-1');await fs.mkdir(directory,{recursive:true});
        next.connection_id=id;next.endpoint.namespace='78'.repeat(16);next.selection.selection.namespace=next.endpoint.namespace;
        next.ca_certificate_file=path.join(directory,'ca.pem');next.credential_file=path.join(directory,`credential-${id}.token`);
        next.endpoint.ca_sha256=createHash('sha256').update(new X509Certificate(rootCertificates[0]).raw).digest('hex');
        await fs.writeFile(next.ca_certificate_file,rootCertificates[0],{mode:0o600});await fs.writeFile(next.credential_file,`airk1.${id}.`+'b'.repeat(64),{mode:0o600});
        await assert.doesNotReject(controller.provisionHttpInfo(next,'claude',()=>{},f.context.globalStorageUri.fsPath));
        assert.notEqual(entries.get('NODE_EXTRA_CA_CERTS')!.value,previousTrust);
        assert.equal(f.state.httpConnections.length,1,'cleanup and provisioning do not automatically enroll a replacement connection');
    }finally{await f.close();}
});

test('clean break prunes only native-proven predecessor HTTP mementos and preserves unrelated connection state',async context=>{
    const f=await serviceTestHost(context,{automatic:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    try{
        const {info}=await predecessorHostHttp(f),oldKey='airk.http.legacy.1.'+'49'.repeat(32),foreignKey='airk.http.legacy.1.'+'59'.repeat(32);
        f.values.set(oldKey,structuredClone(info));
        const foreign=structuredClone(info);foreign.connection_id='69'.repeat(16);foreign.credential_file=path.join(path.dirname(info.credential_file),`credential-${foreign.connection_id}.token`);foreign.endpoint.namespace='99'.repeat(16);foreign.selection.selection.namespace=foreign.endpoint.namespace;
        f.values.set(foreignKey,foreign);f.values.set('user.preference',{keep:true});
        await f.commands.get('airk.installCli')!();
        assert.equal(f.values.has(oldKey),false,'private old namespace connection metadata must not survive a completed clean break');
        assert.deepEqual(f.values.get(foreignKey),foreign);assert.deepEqual(f.values.get('user.preference'),{keep:true});
    }finally{await f.close();}
});

test('clean break preserves an unproven public trust copy and reports that artifact instead of claiming completion',async context=>{
    const f=await serviceTestHost(context,{automatic:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    const entries=new Map<string,{type:number;value:string}>(),collection=f.context.environmentVariableCollection;
    collection.get=(name:string)=>entries.get(name);collection.delete=(name:string)=>entries.delete(name);
    try{
        const {info}=await predecessorHostHttp(f),directory=path.join(f.context.globalStorageUri.fsPath,'http-trust');await fs.mkdir(directory);
        const bytes=await fs.readFile(info.ca_certificate_file),file=path.join(directory,createHash('sha256').update(bytes).digest('hex')+'.pem');await fs.writeFile(file,bytes);
        entries.set('NODE_EXTRA_CA_CERTS',{type:f.stub.EnvironmentVariableMutatorType.Replace,value:file});
        await f.commands.get('airk.installCli')!();
        assert.deepEqual(await fs.readFile(file),bytes,'native CA bytes and a plausible filename do not prove who published this copy');
        const capture=[...f.values.entries()].find(([name])=>name.startsWith('airk.cleanBreak.clients.1.'))?.[1] as any;
        assert.equal(capture.result.complete,false);assert.match(JSON.stringify(capture.result.hostArtifacts),/clean_break_http_trust_ownership_unproven/);
        assert.equal(f.values.has(`airk.cleanBreak.complete.1.${f.binding.installDir}`),false);
        await fs.unlink(file);await f.commands.get('airk.installCli')!();
        const resumed=[...f.values.entries()].find(([name])=>name.startsWith('airk.cleanBreak.clients.1.'))?.[1] as any;
        assert.equal(resumed.result.complete,true,'an already absent unproven copy no longer blocks the retained cleanup proof');
    }finally{await f.close();}
});

test('provisioning an already existing matching trust copy does not acquire clean-break deletion authority',async context=>{
    const f=await serviceTestHost(context,{automatic:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    const entries=new Map<string,{type:number;value:string}>(),collection=f.context.environmentVariableCollection;
    collection.get=(name:string)=>entries.get(name);collection.replace=(name:string,value:string)=>entries.set(name,{type:f.stub.EnvironmentVariableMutatorType.Replace,value});collection.delete=(name:string)=>entries.delete(name);
    try{
        const {controller,info}=await predecessorHostHttp(f),directory=path.join(f.context.globalStorageUri.fsPath,'http-trust');await fs.mkdir(directory);
        const bytes=await fs.readFile(info.ca_certificate_file),file=path.join(directory,createHash('sha256').update(bytes).digest('hex')+'.pem');await fs.writeFile(file,bytes);
        await controller.provisionHttpInfo(info,'claude',()=>{},f.context.globalStorageUri.fsPath);
        await f.commands.get('airk.installCli')!();
        assert.deepEqual(await fs.readFile(file),bytes,'successful reuse never proves exclusive creation of a preexisting copy');
        const capture=[...f.values.entries()].find(([name])=>name.startsWith('airk.cleanBreak.clients.1.'))?.[1] as any;
        assert.equal(capture.result.complete,false);assert.match(JSON.stringify(capture.result.hostArtifacts),/clean_break_http_trust_ownership_unproven/);
    }finally{await f.close();}
});

test('a same-byte replacement after the exclusive trust link never inherits the original creator identity',async context=>{
    const f=await serviceTestHost(context,{automatic:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    const entries=new Map<string,{type:number;value:string}>(),collection=f.context.environmentVariableCollection;
    collection.get=(name:string)=>entries.get(name);collection.replace=(name:string,value:string)=>entries.set(name,{type:f.stub.EnvironmentVariableMutatorType.Replace,value});collection.delete=(name:string)=>entries.delete(name);
    try{
        const {controller,info}=await predecessorHostHttp(f),promises=require('node:fs/promises'),link=promises.link;let replaced:string|undefined;
        context.mock.method(promises,'link',async(source:string,target:string)=>{
            await link(source,target);
            if(path.dirname(target)===path.join(f.context.globalStorageUri.fsPath,'http-trust')){
                const bytes=await fs.readFile(target);await fs.unlink(target);await fs.writeFile(target,bytes,{mode:0o600});replaced=target;
                // The original inode still has its live stage link, so this
                // replacement must have a distinct exact filesystem identity.
                assert.notEqual((await fs.lstat(source,{bigint:true})).ino,(await fs.lstat(target,{bigint:true})).ino);
            }
        });
        await controller.provisionHttpInfo(info,'claude',()=>{},f.context.globalStorageUri.fsPath);assert.ok(replaced);const bytes=await fs.readFile(replaced!);
        await f.commands.get('airk.installCli')!();
        assert.deepEqual(await fs.readFile(replaced!),bytes,'equal bytes do not transfer ownership from the actual created inode');
        const capture=[...f.values.entries()].find(([name])=>name.startsWith('airk.cleanBreak.clients.1.'))?.[1] as any;
        assert.equal(capture.result.complete,false);assert.match(JSON.stringify(capture.result.hostArtifacts),/clean_break_http_trust_ownership_unproven/);
    }finally{await f.close();}
});

test('clean break preserves a same-path trust publication reassigned to a new namespace after capture',async context=>{
    const f=await serviceTestHost(context,{automatic:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    const entries=new Map<string,{type:number;value:string}>(),collection=f.context.environmentVariableCollection;
    collection.get=(name:string)=>entries.get(name);collection.replace=(name:string,value:string)=>entries.set(name,{type:f.stub.EnvironmentVariableMutatorType.Replace,value});collection.delete=(name:string)=>entries.delete(name);
    try{
        const {controller,info}=await predecessorHostHttp(f);await controller.provisionHttpInfo(info,'claude',()=>{},f.context.globalStorageUri.fsPath);
        const file=entries.get('NODE_EXTRA_CA_CERTS')!.value,bytes=await fs.readFile(file),next=structuredClone(info);next.connection_id='89'.repeat(16);next.endpoint.namespace='99'.repeat(16);next.selection.selection.namespace=next.endpoint.namespace;next.credential_file=path.join(path.dirname(info.credential_file),`credential-${next.connection_id}.token`);
        await fs.writeFile(next.credential_file,`airk1.${next.connection_id}.`+'c'.repeat(64),{mode:0o600});
        const transition=RuntimeHost.prototype.transition;
        context.mock.method(RuntimeHost.prototype,'transition',async function(this:RuntimeHost,...args:Parameters<RuntimeHost['transition']>){await controller.provisionHttpInfo(next,'claude',()=>{},f.context.globalStorageUri.fsPath);return transition.apply(this,args);});
        await f.commands.get('airk.installCli')!();
        assert.equal(entries.get('NODE_EXTRA_CA_CERTS')!.value,file);assert.ok(entries.has(credentialEnvironment(next)));assert.deepEqual(await fs.readFile(file),bytes);
        const capture=[...f.values.entries()].find(([name])=>name.startsWith('airk.cleanBreak.clients.1.'))?.[1] as any;
        assert.equal(capture.result.complete,false);assert.match(JSON.stringify(capture.result.hostArtifacts),/clean_break_http_environment_namespace_changed/);assert.match(JSON.stringify(capture.result.hostArtifacts),/clean_break_http_trust_current_namespace_reference/);
        entries.clear();const extension=require('../extension');await extension.deactivate();const replacement=await extension.activate(f.context);await replacement.whenReady();await f.commands.get('airk.installCli')!();
        assert.deepEqual(await fs.readFile(file),bytes,'losing nonpersistent window references on reload does not authorize removal of a known new-namespace copy');
        const resumed=[...f.values.entries()].find(([name])=>name.startsWith('airk.cleanBreak.clients.1.'))?.[1] as any;assert.equal(resumed.result.complete,false);
    }finally{await f.close();}
});

for(const failure of ['metadata','endpoint'] as const)test(`clean break retries Assistance ${failure} failure before claiming host artifact completion`,async context=>{
    const f=await serviceTestHost(context,{automatic:false,assistance:true,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    try{
        await fs.mkdir(path.join(f.root,'out'),{recursive:true});await fs.writeFile(path.join(f.root,'out','explorerAssistance.js'),'// current fixture server');
        const descriptor=launchDescriptorPath(f.context.globalStorageUri.fsPath),stale='{"extension_version":"1.8.11","old_metadata":true}';await fs.writeFile(descriptor,stale);
        const rejected=failure==='metadata'?context.mock.method(assistanceLaunch,'writeLaunchDescriptor',async()=>{throw new Error('fixture launch write blocked');}):context.mock.method(AssistanceIpcHost,'start',async()=>{throw new Error('fixture endpoint bind blocked');});
        await f.commands.get('airk.installCli')!();
        let capture=[...f.values.entries()].find(([name])=>name.startsWith('airk.cleanBreak.clients.1.'))?.[1] as any;
        assert.equal(capture.result.complete,false,'swallowed optional startup errors must remain explicit clean-break cleanup failures');
        assert.notEqual(capture.hostArtifacts.assistanceRefreshed,true);assert.match(JSON.stringify(capture.result.hostArtifacts),/clean_break_assistance_refresh_unconfirmed/);
        rejected.mock.restore();await f.commands.get('airk.installCli')!();
        capture=[...f.values.entries()].find(([name])=>name.startsWith('airk.cleanBreak.clients.1.'))?.[1] as any;
        assert.equal(capture.result.complete,true);assert.equal(capture.hostArtifacts.assistanceRefreshed,true);
        const current=JSON.parse(await fs.readFile(descriptor,'utf8'));assert.equal(current.extension_version,'1.8.16');assert.equal(current.server_sha256,createHash('sha256').update('// current fixture server').digest('hex'));
    }finally{await f.close();}
});

for(const limit of ['count','bytes'] as const)test(`clean break refuses an unloadable ${limit} capture before persistence or runtime selection`,async context=>{
    const f=await serviceTestHost(context,{automatic:false,assistance:true,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    try{
        const controller=f.context.subscriptions.find((value:any)=>typeof value.resolveAgentSpec==='function'),descriptor=controller.assistanceHost.descriptorPath;
        const invalid=limit==='count'?{mementos:Array.from({length:4097},(_,i)=>({key:'airk.http.connection.1.'+i,sha256:'11'.repeat(32)})),environment:[],bundles:[],blockers:[]}:{mementos:[],environment:[],bundles:[],blockers:Array(2000).fill('x'.repeat(8192))};
        context.mock.method(controller,'captureCleanBreakHostArtifacts',async()=>invalid);
        await f.commands.get('airk.installCli')!();
        assert.equal(f.calls.includes('upgrade-applied'),false,'reject the exact unloadable envelope before the destructive native boundary');
        assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.cleanBreak.clients.1.')),false,'no permanently invalid recovery plan is persisted');
        assert.ok(await fs.stat(descriptor),'capture refusal does not invalidate the existing owner descriptor');
    }finally{await f.close();}
});

test('clean break preserves token and metadata preimages changed after capture and exposes their cleanup blockers',async context=>{
    const f=await serviceTestHost(context,{automatic:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    const entries=new Map<string,{type:number;value:string}>(),collection=f.context.environmentVariableCollection;
    collection.get=(name:string)=>entries.get(name);collection.replace=(name:string,value:string)=>entries.set(name,{type:f.stub.EnvironmentVariableMutatorType.Replace,value});collection.delete=(name:string)=>entries.delete(name);
    try{
        const {controller,info}=await predecessorHostHttp(f),key='airk.http.legacy.1.'+'79'.repeat(32);
        await controller.provisionHttpInfo(info,'claude',()=>{},f.context.globalStorageUri.fsPath);f.values.set(key,structuredClone(info));
        const changed=structuredClone(info);changed.endpoint.namespace='99'.repeat(16);changed.selection.selection.namespace=changed.endpoint.namespace;
        const transition=RuntimeHost.prototype.transition;
        context.mock.method(RuntimeHost.prototype,'transition',async function(this:RuntimeHost,...args:Parameters<RuntimeHost['transition']>){
            entries.set(credentialEnvironment(info),{type:f.stub.EnvironmentVariableMutatorType.Replace,value:'new-owner-value'});f.values.set(key,changed);
            return transition.apply(this,args);
        });
        await f.commands.get('airk.installCli')!();
        assert.equal(entries.get(credentialEnvironment(info))!.value,'new-owner-value');assert.deepEqual(f.values.get(key),changed);
        const capture=[...f.values.entries()].find(([name])=>name.startsWith('airk.cleanBreak.clients.1.'))?.[1] as any;
        assert.equal(capture.result.complete,false);assert.match(JSON.stringify(capture.result.hostArtifacts),/clean_break_http_environment_changed/);assert.match(JSON.stringify(capture.result.hostArtifacts),/clean_break_http_memento_changed/);
    }finally{await f.close();}
});

test('clean break replaces only its own Assistance credential and retains verified current launch metadata',async context=>{
    const f=await serviceTestHost(context,{automatic:false,assistance:true,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    let other:AssistanceIpcHost|undefined;
    try{
        const controller=f.context.subscriptions.find((value:any)=>typeof value.resolveAgentSpec==='function'),own=controller.assistanceHost as AssistanceIpcHost;
        assert.ok(own?.descriptorPath);const oldPath=own.descriptorPath!,old=JSON.parse(await fs.readFile(oldPath,'utf8'));
        await fs.mkdir(path.join(f.root,'out'),{recursive:true});await fs.writeFile(path.join(f.root,'out','explorerAssistance.js'),'// current fixture server');
        other=await AssistanceIpcHost.start({directory:assistanceDirectory(f.context.globalStorageUri.fsPath),version:'1.8.16',roots:[f.folders[0].uri.fsPath],explorerOpen:()=>false});assert.ok(other?.descriptorPath);
        const otherPath=other.descriptorPath!,otherBytes=await fs.readFile(otherPath,'utf8');
        await f.commands.get('airk.installCli')!();
        await assert.rejects(fs.stat(oldPath),{code:'ENOENT'},'the host withdraws the exact descriptor it created before reset');
        const current=controller.assistanceHost as AssistanceIpcHost;assert.ok(current?.descriptorPath);assert.notEqual(current.descriptorPath,oldPath);
        const replacement=JSON.parse(await fs.readFile(current.descriptorPath!,'utf8'));assert.notEqual(replacement.credential,old.credential);
        assert.equal(await fs.readFile(otherPath,'utf8'),otherBytes,'another live editor owns its own descriptor');
        const launch=JSON.parse(await fs.readFile(launchDescriptorPath(f.context.globalStorageUri.fsPath),'utf8'));
        assert.equal(launch.extension_version,'1.8.16');assert.equal(launch.server_sha256,createHash('sha256').update('// current fixture server').digest('hex'));
        assert.equal(f.state.httpConnections.length,0,'Assistance restart does not reenroll native grants');
    }finally{other?.dispose();await f.close();}
});

test('prebaseline setup forces a clean break without migration choices and keeps fresh indexing separate from cleanup',async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    const policy={required:false,baseline_version:'1.8.16' as const,state_contract:'airk-state/1' as const,installed_contract:{schema:1}};
    const reset={schema:1 as const,operation_id:'cc'.repeat(16),stage:'indexing' as const,mode:'clean_break',indexes_complete:true,expected_indexes:0,cleanup:{complete:false,state:'waiting_for_indexes',failure:null,observation:null},provenance:'native_rebuild_from_saved_configuration',
        candidate:f.binding.runtimeSha256,predecessor:f.binding.runtimeSha256,original_namespace:'34'.repeat(16),successor_namespace:'22'.repeat(16),
        preserved_state:path.join(f.root,'old-state'),preserved_control:f.binding.controlDir,preserved_index_roots:[],successor_state:path.join(f.root,'fresh-state'),successor_control:path.join(f.root,'fresh-control'),
        original_failure:null,failure:null,index_scope:null,indexes:[]};
    let transitioned=0;
    context.mock.method(RuntimeHost.prototype,'transition',async()=>{transitioned++;f.binding.version='1.8.16';f.binding.controlDir=String(reset.successor_control);f.binding.cleanBreak=policy;f.binding.rebuild=reset;return {schema:1,active:f.binding.runtimeSha256};});
    f.state.pick=()=>{throw new Error('A prebaseline installation must not offer migration/rebuild choices');};
    try{
        await startupState(f,'open-clean-break');const enrolled=f.state.httpConnections.length;await f.commands.get('airk.installCli')!();
        assert.equal(transitioned,1);assert.equal(f.calls.includes('upgrade-reviewed'),true);
        assert.equal(f.state.httpConnections.length,enrolled,'clean reset cannot enroll or retarget old grants through migration');
        const panel=f.panels.find(panel=>panel.viewType==='airk.explorer');assert.ok(panel);
        const snapshot=()=>panel.posted.filter((entry:any)=>entry.type==='state').at(-1)?.state;
        assert.equal(snapshot().runtimeUpgrade.stage,'rebuilding');assert.equal(snapshot().runtimeUpgrade.cleanBreak,true);
        assert.match(snapshot().runtimeUpgrade.message,/Original sources are unchanged/);
        await f.commands.get('airk.installCli')!();assert.equal(transitioned,1,'current native reset observation must resume without reinstalling');
        f.binding.rebuild={...reset,stage:'ready'};
        await f.commands.get('airk.installCli')!();assert.equal(snapshot().runtimeUpgrade.stage,'cleaning');
        f.binding.rebuild={...reset,stage:'ready',cleanup:{complete:true,state:'complete',failure:null,observation:{removed:[]}}};
        await f.commands.get('airk.installCli')!();assert.equal(snapshot().runtimeUpgrade.stage,'ready');
        assert.match(snapshot().runtimeUpgrade.message,/owned legacy artifacts were removed/);
    }finally{await f.close();}
});

test('clean break captures old ownership before reset, removes that client entry without enrollment, and retains the scoped receipt',async context=>{
    const f=await serviceTestHost(context,{automatic:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    const registration=[...f.registrations.values()][0],filename=path.join(registration.root,'.mcp.json');
    const original={userPreference:'unrelated-SECRET',mcpServers:{old_airk_alias:{command:f.binding.executable,args:['--root',registration.root,'--data-dir',registration.data_root,'--collection',registration.collection,'--service-project',registration.binding,'mcp'],disabled:true},other:{command:'unrelated-tool',args:['keep']}}};
    await fs.writeFile(filename,JSON.stringify(original));migrationEditor(f);
    const transition=RuntimeHost.prototype.transition;let capturedBeforeTransition=false;
    context.mock.method(RuntimeHost.prototype,'transition',async function(this:RuntimeHost,...args:Parameters<RuntimeHost['transition']>){
        const capture=[...f.values.entries()].find(([key])=>key.startsWith('airk.cleanBreak.clients.1.'))?.[1] as any;
        assert.ok(capture);assert.equal(capture.predecessor,args[0].runtimeSha256);assert.equal(capture.controlDir,args[0].controlDir);assert.equal(capture.candidate,args[1].target);capturedBeforeTransition=true;
        return transition.apply(this,args);
    });
    try{
        await f.commands.get('airk.installCli')!();
        assert.equal(capturedBeforeTransition,true);
        const saved=JSON.parse(await fs.readFile(filename,'utf8'));assert.equal(saved.mcpServers.old_airk_alias,undefined);
        assert.deepEqual(saved.mcpServers.other,original.mcpServers.other);assert.equal(saved.userPreference,original.userPreference);
        assert.equal(f.state.httpConnections.length,0,'the reset removes old declarations and does not issue replacement credentials');
        const capture=[...f.values.entries()].find(([key])=>key.startsWith('airk.cleanBreak.clients.1.'))?.[1] as any;
        assert.ok(capture);assert.equal(capture.operation,f.binding.rebuild?.operation_id);assert.equal(capture.result.complete,true);
        assert.equal(capture.plan.files[0].destination,filename);assert.equal(JSON.stringify(capture).includes(original.userPreference),false,'private capture retains ownership proof rather than unrelated configuration bytes');
        assert.equal(f.values.get(`airk.cleanBreak.complete.1.${f.binding.installDir}`),capture.operation);
        assert.equal(f.calls.includes('legacy-caches-retired'),false,'native reset cleanup owns physical artifacts; editor does not run migration finalization');
    }finally{await f.close();}
});

test('unproven old client selectors remain unchanged and prevent a clean-install completion claim',async context=>{
    const f=await serviceTestHost(context,{automatic:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    const registration=[...f.registrations.values()][0],filename=path.join(registration.root,'.mcp.json');
    const source=JSON.stringify({mcpServers:{airk:{command:f.binding.executable,args:['--root',registration.root,'--data-dir',registration.data_root,'--collection',registration.collection,'--service-project','ff'.repeat(32),'mcp']}}});
    await fs.writeFile(filename,source);migrationEditor(f);
    try{
        await startupState(f,'open-blocked-clean-break');await f.commands.get('airk.installCli')!();
        assert.equal(await fs.readFile(filename,'utf8'),source);assert.equal(f.binding.rebuild?.cleanup&&((f.binding.rebuild.cleanup as any).complete),true,'native cleanup and host client cleanup are independent');
        const panel=f.panels.find(panel=>panel.viewType==='airk.explorer')!,state=panel.posted.filter((entry:any)=>entry.type==='state').at(-1)?.state;
        assert.equal(state.runtimeUpgrade.stage,'failed');assert.equal(state.runtimeUpgrade.clientCleanup.complete,false);
        assert.match(JSON.stringify(state.runtimeUpgrade.clientCleanup),/clean_break_client_ownership_unconfirmed/);
        assert.equal(f.values.has(`airk.cleanBreak.complete.1.${f.binding.installDir}`),false);assert.equal(f.state.httpConnections.length,0);
    }finally{await f.close();}
});

test('extension reload resumes persisted pre-reset client proof without rerunning the reset or reading the old ledger',async context=>{
    const f=await serviceTestHost(context,{automatic:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});
    const registration=[...f.registrations.values()][0],filename=path.join(registration.root,'.mcp.json');
    const before=JSON.stringify({mcpServers:{airk:{command:f.binding.executable,args:['--root',registration.root,'--data-dir',registration.data_root,'--collection',registration.collection,'--service-project',registration.binding,'mcp']},other:{command:'keep'}}});
    await fs.writeFile(filename,before);migrationEditor(f,true);
    try{
        await f.commands.get('airk.installCli')!();assert.equal(f.calls.filter(value=>value==='upgrade-applied').length,1);
        const captured=[...f.values.entries()].find(([key])=>key.startsWith('airk.cleanBreak.clients.1.'))?.[1] as any;
        assert.equal(captured.result.complete,false);assert.equal(await fs.readFile(filename,'utf8'),before);
        f.stub.workspace.textDocuments=[];migrationEditor(f);
        context.mock.method(RuntimeHost.prototype,'lookupHttpConnection',async()=>{throw new Error('The predecessor ledger is unavailable after reset');});
        context.mock.method(RuntimeHost.prototype,'lookupPredecessorHttpOwnership',async()=>{throw new Error('Persisted stdio cleanup already captured its complete predecessor metadata');});
        const extension=require('../extension');await extension.deactivate();const replacement=await extension.activate(f.context);await replacement.whenReady();
        await f.commands.get('airk.installCli')!();
        assert.equal(f.calls.filter(value=>value==='upgrade-applied').length,1,'resume must not restart the reset');
        assert.deepEqual(JSON.parse(await fs.readFile(filename,'utf8')),{mcpServers:{other:{command:'keep'}}});
        const resumed=[...f.values.entries()].find(([key])=>key.startsWith('airk.cleanBreak.clients.1.'))?.[1] as any;
        assert.equal(resumed.operation,captured.operation);assert.equal(resumed.result.complete,true);assert.equal(f.state.httpConnections.length,0);
    }finally{await f.close();}
});

test('a global inventory blocker without a file destination persists valid proof and can clear on a later clean-break retry',async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false,bundledVersion:'1.8.16',runtimeVersion:'1.8.11',installedVersion:'1.8.16'});let blocked=true;
    context.mock.method(globalInventory,'inventoryGlobalAgentConfigs',async()=>({files:[],checked:[],blockers:blocked?[{code:'global_inventory_temporarily_unavailable'}]:[]}));
    try{
        await f.commands.get('airk.installCli')!();
        const capture=[...f.values.entries()].find(([key])=>key.startsWith('airk.cleanBreak.clients.1.'))?.[1] as any;
        assert.ok(capture);assert.equal(capture.result.complete,false);assert.equal(path.isAbsolute(capture.plan.blockers[0].destination),true);
        blocked=false;await f.commands.get('airk.installCli')!();
        assert.equal(f.calls.filter(value=>value==='upgrade-applied').length,1);
        const resumed=[...f.values.entries()].find(([key])=>key.startsWith('airk.cleanBreak.clients.1.'))?.[1] as any;
        assert.equal(resumed.result.complete,true);assert.equal(resumed.operation,capture.operation);
    }finally{await f.close();}
});

async function startupState(f:Awaited<ReturnType<typeof serviceTestHost>>,requestId:string){
    await f.commands.get('airk.explorer.open')!();
    const panel=f.panels.find(panel=>panel.viewType==='airk.explorer');assert.ok(panel);
    await panel.send({v:1,type:'ready',requestId});
    return panel.posted.filter((entry:any)=>entry.type==='state').at(-1)?.state.startup;
}

test('setup failure reaches an already open Explorer before a pending early refresh finishes',async context=>{
    let release!:()=>void,releaseDiscovery!:()=>void;
    const connectionDiscoveryGate=new Promise<void>(resolve=>{release=resolve;});
    const discoveryGate=new Promise<void>(resolve=>{releaseDiscovery=resolve;});
    const f=await serviceTestHost(context,{automatic:true,deferStartup:true,registered:false,connectionDiscoveryGate,discoveryGate});
    try{
        f.state.down=true;
        await startupState(f,'open-before-failure');
        const panel=f.panels.find(panel=>panel.viewType==='airk.explorer');assert.ok(panel);
        releaseDiscovery();
        for(let turns=0;turns<100&&!f.calls.some(value=>value.startsWith('Automatic installation failed:'));turns++)await new Promise(resolve=>setTimeout(resolve,10));
        assert.ok(f.calls.some(value=>value.startsWith('Automatic installation failed:')));
        await new Promise(resolve=>setImmediate(resolve));
        const state=panel.posted.filter((entry:any)=>entry.type==='state').at(-1)?.state;
        assert.equal(state?.startup?.failed,true,'failure must replace the running timer without reopening Explorer or waiting for discovery');
        assert.match(state.startup.message,/service_unavailable|needs attention/);
        assert.notEqual(state.connection,'connecting','known setup failure must not retain a connecting header or overview');
    }finally{releaseDiscovery();release();await f.api.whenReady();await f.close();}
});

test('failed setup overrides a busy console connection and its bounded poll releases service controls',async context=>{
    let release!:()=>void,releaseDiscovery!:()=>void;
    const consoleGate=new Promise<void>(resolve=>{release=resolve;});
    const discoveryGate=new Promise<void>(resolve=>{releaseDiscovery=resolve;});
    const f=await serviceTestHost(context,{automatic:true,deferStartup:true,registered:false,discoveryGate});
    let controller:ServiceConsoleController|undefined;
    const refresh=ServiceConsoleController.prototype.refresh;
    context.mock.method(ServiceConsoleController.prototype,'refresh',function(this:ServiceConsoleController,automatic?:boolean){
        if(!controller){
            controller=this;
            const runtime=(this as any).host.runtime,discover=runtime.discover;
            runtime.discover=async()=>{await consoleGate;return discover();};
        }
        return refresh.call(this,automatic);
    });
    context.mock.timers.enable({apis:['setTimeout','Date']});
    try{
        f.state.down=true;
        await f.commands.get('airk.explorer.open')!('#manage/service');
        const panel=f.panels.find(panel=>panel.viewType==='airk.explorer');assert.ok(panel);
        await panel.send({v:1,type:'ready',requestId:'busy-console'});
        await panel.send({v:1,type:'manage.observe',requestId:'observe-busy-console',open:true});
        assert.ok(controller);assert.equal(controller.state.busy,true);assert.equal(controller.state.health.state,'unavailable');
        releaseDiscovery();await f.api.whenReady();
        const failed=panel.posted.filter((entry:any)=>entry.type==='state').at(-1)?.state;
        assert.equal(failed.startup.failed,true);assert.equal(controller.state.busy,true);
        assert.equal(failed.connection,'disconnected','terminal setup failure takes precedence over an unrelated pending status poll');
        context.mock.timers.tick(15001);
        for(let turns=0;turns<20&&controller.state.busy;turns++)await new Promise(resolve=>setImmediate(resolve));
        assert.equal(controller.state.busy,false);assert.equal(controller.state.health.code,'status_poll_timed_out');
        const recovered=panel.posted.filter((entry:any)=>entry.type==='state').at(-1)?.state;
        assert.equal(recovered.management.enabled.connect,true,'bounded status polling must release reconnect controls');
    }finally{releaseDiscovery();release();context.mock.timers.reset();await f.api.whenReady();await f.close();}
});

test('successful explicit setup clears an earlier failed startup without creating indexes',async context=>{
    let release!:()=>void;const discoveryGate=new Promise<void>(resolve=>{release=resolve;});
    const f=await serviceTestHost(context,{automatic:true,deferStartup:true,discoveryGate,registered:false});
    try{
        f.state.down=true;release();await f.api.whenReady();
        assert.equal((await startupState(f,'failed-startup'))?.failed,true);
        const failures=f.calls.filter(value=>value.startsWith('Automatic installation failed:')).length;assert.ok(failures>0);
        f.state.down=false;await f.commands.get('airk.setUp')!();
        assert.equal(await startupState(f,'successful-retry'),undefined);
        assert.equal(f.calls.filter(value=>value.startsWith('Automatic installation failed:')).length,failures,'recorded failures remain in output');
        assert.equal(f.registrations.size,0);assert.equal(f.writes.filter(value=>value.reserve_project||value.enqueue).length,0);
    }finally{release();await f.close();}
});

test('failed setup stays visible through missing-runtime refresh until successful setup',async context=>{
    const f=await serviceTestHost(context,{automatic:false,fresh:true,registered:false});
    const discover=RuntimeHost.prototype.discover;
    context.mock.method(RuntimeHost.prototype,'discover',async function(this:RuntimeHost){
        const value=await discover.call(this);
        return f.state.down?{...value,installed:false,inspection:undefined}:value;
    });
    try{
        f.state.down=true;await f.commands.get('airk.setUp')!();
        assert.match(f.statusBar.text,/setup failed/);
        await f.refreshFolders();assert.match(f.statusBar.text,/setup failed/,'refresh must not replace failure with neutral setup text');
        f.state.down=false;await f.commands.get('airk.setUp')!();
        assert.doesNotMatch(f.statusBar.text,/setup failed/);
        f.state.down=true;await f.commands.get('airk.setUp')!();assert.match(f.statusBar.text,/setup failed/);
        f.state.installedVersion='1.4.1';f.extensionsChanged();await new Promise(resolve=>setImmediate(resolve));
        await f.refreshFolders();assert.match(f.statusBar.text,/Reload to finish update/,'pending interface update keeps priority over setup failure');
    }finally{await f.close();}
});

test('upgrade stays incomplete until metadata, migration and launcher removal can finish',async context=>{
    let release!:()=>void;const discoveryGate=new Promise<void>(resolve=>{release=resolve;});
    const f=await serviceTestHost(context,{automatic:true,deferStartup:true,discoveryGate,registered:false});
    const preparation=context.mock.method(ServiceWorkspace.prototype,'reconcileStartup',async()=>{throw new ServiceError('project_metadata_unavailable');});
    try{
        release();await f.api.whenReady();assert.equal((await startupState(f,'preparation-failed'))?.failed,true);
        assert.ok(f.calls.some(value=>value.includes('upgrade completion blocked')));
        await f.commands.get('airk.setUp')!();
        assert.equal((await startupState(f,'retry-still-failed'))?.failed,true,'runtime health cannot erase unresolved preparation failure');
        preparation.mock.restore();const ensures=f.state.ensured,progress=f.progressRuns.length;
        await f.refreshFolders();
        assert.ok(f.calls.includes('legacy-launchers-retired'));
        assert.equal(await startupState(f,'healthy-reconnect'),undefined);
        assert.ok(f.state.ensured>ensures);assert.ok(f.progressRuns.length>progress,'an incomplete upgrade reruns its completion gates');
        assert.equal(f.registrations.size,0);assert.equal(f.writes.filter(value=>value.reserve_project||value.enqueue).length,0);
    }finally{release();await f.close();}
});

test('an installed update keeps one reload action visible and fences older installation attempts',async context=>{
    const f=await serviceTestHost(context,{automatic:true});
    try{
        const writes=f.writes.length,progress=f.progressRuns.length,ensures=f.state.ensured;
        f.state.installedVersion='1.4.1';f.extensionsChanged();
        await new Promise(resolve=>setImmediate(resolve));
        const notice=f.notices.at(-1)!;
        assert.match(notice.message,/1.4.1 is installed.*running 1.4.0/);
        assert.deepEqual(notice.actions,['Reload to finish update']);
        assert.equal(f.statusBar.command,'airk.reloadUpdatedExtension');
        assert.match(f.statusBar.text,/Reload to finish update/);
        assert.equal(f.calls.includes('workbench.action.reloadWindow'),false,'never reload without the user click');
        f.focus();f.extensionsChanged();await f.refreshFolders();
        assert.equal(f.notices.length,1,'dismissal and refocus do not repeat the toast');
        assert.deepEqual(await f.api.getServerDefinitions(),[],'old host must not create another MCP definition');
        assert.equal(f.writes.length,writes);assert.equal(f.progressRuns.length,progress);assert.equal(f.state.ensured,ensures);
        await f.commands.get('airk.installCli')!();assert.equal(f.progressRuns.length,progress);
        assert.equal(f.calls.some(line=>line.startsWith('AIRK: extension_reload_required')),false);
        await f.commands.get('airk.reloadUpdatedExtension')!();
        assert.equal(f.calls.filter(value=>value==='workbench.action.reloadWindow').length,1);
    }finally{await f.close();}
});

test('a pending update found at activation prevents runtime setup and preserves saved registrations',async context=>{
    const f=await serviceTestHost(context,{automatic:true,installedVersion:'1.4.1'});
    try{
        assert.equal(f.progressRuns.length,0);assert.equal(f.state.ensured,0);assert.equal(f.writes.length,0);
        assert.equal(f.registrations.size,1);assert.equal(f.notices.length,1);
        assert.match(f.statusBar.tooltip,/1.4.1 installed; 1.4.0 running/);
        assert.equal(f.calls.includes('workbench.action.reloadWindow'),false);
    }finally{await f.close();}
});

test('ordinary focus and extension-list changes keep a matching active version quiet',async context=>{
    const f=await serviceTestHost(context,{automatic:true});
    try{
        f.focus();f.extensionsChanged();await f.commands.get('airk.reloadUpdatedExtension')!();
        assert.equal(f.notices.length,0);assert.equal(f.calls.includes('workbench.action.reloadWindow'),false);
        assert.doesNotMatch(f.statusBar.text,/Reload to finish update/);
    }finally{await f.close();}
});

test('runtime installation never calls legacy project preparation for missing indexes and keeps reopen quiet',async context=>{
    const f=await serviceTestHost(context,{registered:false});
    try{
        let reviewed=0;context.mock.method(RuntimeHost.prototype,'reviewProject',async()=>{reviewed++;throw new ServiceError('source_unavailable');});
        f.settings.set('cli.autoInstall',true);
        await f.refreshFolders();
        assert.ok([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')));
        assert.equal(reviewed,0);assert.equal(f.registrations.size,0);assert.equal(f.warnings.length,0);
        assert.equal(f.calls.some(message=>message.startsWith('Automatic installation failed:')),false);
        assert.equal(f.calls.some(message=>message.startsWith('Installation did not complete.')),false);
        const progress=f.progressRuns.length,notices=f.warnings.length,retirements=f.calls.filter(message=>message==='previous-runtime-retired').length;
        await f.refreshFolders();
        assert.equal(f.progressRuns.length,progress,'project failure does not repeat setup progress');
        assert.equal(f.calls.filter(message=>message==='previous-runtime-retired').length,retirements);
        assert.equal(f.warnings.length,notices,'unchanged project warning is not repeated');
        assert.equal(f.writes.length,0,'failed registration never invents an index request');
    }finally{await f.close();}
});

test('trusted activation installs the runtime once without registering or indexing open folders',async context=>{
    const f=await serviceTestHost(context,{automatic:true,registered:false,roots:2});
    try{
        assert.equal(f.registrations.size,0);assert.equal(f.state.picks,0);assert.equal(f.warnings.length,0);
        assert.ok(f.calls.includes('previous-runtime-retired'));
        assert.equal(f.notices.length,0,'successful activation needs no notification decision');
        assert.ok(f.calls.some(message=>message.includes('This window is running the updated interface')));
        assert.equal(f.calls.includes('workbench.action.reloadWindow'),false);
        assert.equal(f.progressRuns.length,1);
        assert.match(f.progressRuns[0].title,/Checking AIRK .* installation/);
        assert.doesNotMatch(f.progressRuns[0].title,/setup/);
        assert.ok(f.progressRuns[0].messages.some(value=>value.includes('Checking selected runtime')));
        assert.ok(f.progressRuns[0].messages.some(value=>value.includes('Runtime ready. Set up new indexes explicitly in Indexing')));
        assert.equal(f.writes.filter(value=>value.reserve_project).length,0);assert.equal(f.writes.filter(value=>value.enqueue).length,0);
        const servers=await f.api.getServerDefinitions();assert.deepEqual(servers,[],'missing indexes and grants cannot produce MCP definitions');
        const writes=f.writes.length,retirements=f.calls.filter(value=>value==='previous-runtime-retired').length,ensures=f.state.ensured;
        await f.refreshFolders();assert.equal(f.writes.length,writes);
        assert.equal(f.state.ensured,ensures,'healthy reopen uses authenticated health without another native start/ensure');
        assert.equal(f.progressRuns.length,1,'verified reopen must not display installation progress');
        assert.equal(f.calls.filter(value=>value==='previous-runtime-retired').length,retirements,'verified reopen must not reinstall the terminal command or retire runtimes');
        assert.equal(f.notices.length,0,'routine reopen must remain quiet');
        f.state.stopped=true;await f.refreshFolders();assert.equal(f.writes.length,writes);
        assert.ok(f.calls.some(value=>value.includes('service_explicitly_stopped')));
    }finally{await f.close();}
});

test('completed activation and explicit repair finish without a reload or completion popup',async context=>{
    const f=await serviceTestHost(context);
    try{
        await f.commands.get('airk.installCli')!();
        assert.equal(f.notices.length,0);
        assert.ok(f.calls.some(message=>message.startsWith('AIRK 1.4.0 ready.')));
        assert.equal(f.calls.includes('workbench.action.reloadWindow'),false);
        assert.ok(f.calls.some(value=>value.includes('fully exit all VS Code windows')));
        assert.match(renderServiceDashboard(true,1,1),/command:airk.manageVscodeMcp/);
        assert.match(renderServiceDashboard(true,1,1),/Restart external Codex and Claude Code sessions separately/);
    }finally{await f.close();}
});

test('a launcher held by clients offers retry and does not report successful activation',async context=>{
    const f=await serviceTestHost(context,{upgrade:true});
    try{
        let attempts=0;
        context.mock.method(RuntimeHost.prototype,'transition',async()=>{
            attempts++;
            if(attempts===1)throw new Error('airk_client_restart_required: launcher is in use');
            f.binding.version='1.4.1';return {};
        });
        f.settings.set('cli.autoInstall',true);await f.refreshFolders();
        assert.equal(f.binding.version,'1.4.0');assert.equal(f.writes.length,0);
        const notice=f.notices.find(value=>value.message.includes('waiting for connected tools'));
        assert.ok(notice);assert.deepEqual(notice.actions,['MCP Servers','Retry Update','Show Details']);
        assert.equal(f.notices.some(value=>value.message.startsWith('AIRK 1.4.1 installed.')),false);
        assert.ok(f.progressRuns[0].messages.some(value=>value.startsWith('Upgrading 1.4.0 → 1.4.1')));
        assert.equal(f.progressRuns[0].title,'Updating AIRK 1.4.1 runtime');
        assert.ok(f.calls.some(value=>value.startsWith('Upgrade paused')));
        await f.commands.get('airk.retryUpdate')!();
        assert.equal(attempts,2);assert.equal(f.binding.version,'1.4.1');
        assert.equal((await f.api.getServerDefinitions()).length,1);
        assert.ok(f.calls.some(value=>value.startsWith('AIRK 1.4.1 ready.')));
        assert.equal(f.notices.some(value=>value.message.startsWith('AIRK 1.4.1 installed.')),false);
    }finally{await f.close();}
});

test('untrusted activation does not install or register and dashboard explains prerequisites and storage changes',async context=>{
    const f=await serviceTestHost(context,{automatic:true,registered:false,trusted:false});
    try{assert.equal(f.state.ensured,0);assert.equal(f.registrations.size,0);assert.equal(f.writes.length,0);}
    finally{await f.close();}
    assert.match(renderServiceDashboard(false,1,0),/Trust this workspace/);
    assert.match(renderServiceDashboard(true,0,0),/Open a project folder/);
    const html=renderServiceDashboard(true,1,1);
    assert.match(html,/1 workspace project is registered/);assert.match(html,/command:airk.chooseStorage/);
    assert.match(html,/Job acceptance means work is queued/);
});

test('1.4 patch upgrade retains existing storage bindings and does not reset watching or enqueue a fresh index',async context=>{
    const f=await serviceTestHost(context,{automatic:true,upgrade:true});
    try{
        assert.ok(f.calls.includes('upgrade-reviewed'));assert.ok(f.calls.includes('upgrade-applied'));
        assert.ok(f.calls.includes('previous-runtime-retired'));
        assert.equal(f.registrations.size,1);assert.equal(f.writes.length,0);
        const definitions=await f.api.getServerDefinitions();assert.equal(definitions.length,1);
        assert.equal(definitions[0].uri.toString(),f.httpUrl);const enrolled=f.state.httpConnections.at(-1)!.selection;assert.equal(enrolled.kind,'imported_legacy');if(enrolled.kind==='imported_legacy')assert.equal(enrolled.selection.registration.root,[...f.registrations.values()][0].root);
        const saved=[...f.values.entries()].filter(([key])=>key.startsWith('airk.service.workspace.1.'));
        await f.refreshFolders();assert.deepEqual([...f.values.entries()].filter(([key])=>key.startsWith('airk.service.workspace.1.')),saved);
        assert.equal(f.writes.length,0);
    }finally{await f.close();}
});

// Follow the shipped manifest so a release bump cannot silently leave its upgrade path disabled.
const releaseVersion:string=require('../../package.json').version;
test('fresh native registry does not recreate an index from surviving editor receipts',async context=>{
    const f=await serviceTestHost(context,{automatic:true,runtimeVersion:releaseVersion,bundledVersion:releaseVersion});
    try{
        f.registrations.clear(); // User deleted Local/Roaming AIRK state; VS Code's Memento survived.
        const writes=f.writes.length;
        await f.refreshFolders();
        assert.equal(f.registrations.size,0);assert.equal(f.writes.length,writes);assert.deepEqual(await f.api.getServerDefinitions(),[]);
        const again=f.writes.length;await f.refreshFolders();assert.equal(f.writes.length,again);
        assert.ok(f.calls.some(value=>value.includes('Indexing → Index library')));assert.ok(!f.calls.some(value=>value.includes('Restoring workspace registration')));
    }finally{await f.close();}
});
test('startup honors a native removed project instead of recreating it from editor receipts',async context=>{
    const f=await serviceTestHost(context,{automatic:true});
    try{
        for(const [key,value] of f.registrations)f.inactiveRegistrations.set(key,value);
        f.registrations.clear();const writes=f.writes.length;
        await f.refreshFolders();
        assert.equal(f.registrations.size,0);assert.equal(f.writes.length,writes);
        assert.equal((await f.api.getServerDefinitions()).length,0);
        await f.refreshFolders();assert.equal(f.writes.length,writes);
        assert.ok(f.calls.some(value=>value.startsWith('Index remains removed:')));
    }finally{await f.close();}
});
test('startup adopts an existing manually restored index for the same root without resetting it',async context=>{
    const f=await serviceTestHost(context,{automatic:true});
    try{
        const previous=[...f.registrations.values()][0],restored={...previous,binding:'cd'.repeat(32),collection:'manually-restored',data_root:path.join(f.root,'restored-index')};
        f.registrations.clear();f.registrations.set(restored.binding,restored);const writes=f.writes.length;
        await f.refreshFolders();assert.equal(f.registrations.size,1);assert.equal(f.writes.length,writes);
        await assert.rejects(f.api.getServerDefinitions(),/legacy_selection_review_required/,'a newly adopted different Index cannot inherit old provider authority');
        const again=f.writes.length;await f.refreshFolders();assert.equal(f.writes.length,again);
    }finally{await f.close();}
});
test('an unavailable registry never triggers automatic re-registration',async context=>{
    const f=await serviceTestHost(context,{automatic:true});
    try{
        f.registrations.clear();f.state.down=true;const writes=f.writes.length;
        await f.refreshFolders();assert.equal(f.registrations.size,0);assert.equal(f.writes.length,writes);
        assert.ok(f.calls.some(value=>value.includes('service_unavailable')));
    }finally{await f.close();}
});
for(const from of ['1.4.4','1.5.12','1.6.0','1.8.7','1.8.8']){
    const to=releaseVersion;
    test(`activation crosses the clean baseline from ${from} to ${to} once before connecting and reopens quietly`,async context=>{
        const f=await serviceTestHost(context,{automatic:true,runtimeVersion:from,bundledVersion:to});
        try{
            assert.equal(f.binding.version,to);
            assert.equal(f.binding.cleanBreak?.required,false,'the native fixture must publish the newly initialized state contract');
            assert.equal(f.calls.filter(value=>value==='upgrade-applied').length,1);
            assert.ok(f.calls.indexOf('upgrade-applied')<f.calls.indexOf('Checking selected runtime…'));
            assert.equal(f.calls.some(value=>value.includes('explicit_runtime_upgrade_or_matching_extension_required')),false);
            assert.equal(f.calls.some(value=>value.startsWith('Automatic installation failed:')),false);
            assert.equal(f.values.get(`airk.installComplete.2.${f.binding.installDir}`),`${to}:${f.binding.runtimeSha256}`);
            assert.equal(f.registrations.size,1);assert.equal(f.writes.length,0);
            assert.match(f.progressRuns[0].title,/Updating AIRK/);
            const registrations=structuredClone([...f.registrations]),ensures=f.state.ensured;
            await f.refreshFolders();
            assert.deepEqual([...f.registrations],registrations);
            assert.equal(f.state.ensured,ensures);assert.equal(f.progressRuns.length,1);
            assert.equal(f.calls.filter(value=>value==='upgrade-applied').length,1);
        }finally{await f.close();}
    });
}
test('unsupported automatic downgrade remains visible and never applies a transition',async context=>{
    const [major,minor,patch]=releaseVersion.split('.').map(Number);
    const newerRuntime=`${major}.${minor}.${patch+1}`;
    const f=await serviceTestHost(context,{automatic:true,runtimeVersion:newerRuntime,bundledVersion:releaseVersion});
    try{
        assert.equal(f.binding.version,newerRuntime);
        assert.ok(!f.calls.includes('upgrade-applied'));
        assert.ok(f.calls.some(line=>line.startsWith('Automatic installation failed:')&&line.includes('explicit_runtime_upgrade_or_matching_extension_required')));
        assert.ok(f.calls.some(line=>line.startsWith('AIRK installation needs attention:')));
    }finally{await f.close();}
});

test('failed supported automatic upgrade is reported by its setup owner',async context=>{
    const f=await serviceTestHost(context,{automatic:false,runtimeVersion:'1.6.0',bundledVersion:releaseVersion});
    try{
        context.mock.method(RuntimeHost.prototype,'transition',async()=>{throw new ServiceError('upgrade_fixture_refused');});
        const before=f.calls.length;
        f.settings.set('cli.autoInstall',true);await f.refreshFolders();
        assert.equal(f.binding.version,'1.6.0');assert.ok(!f.calls.includes('upgrade-applied'));
        assert.ok(f.calls.slice(before).some(line=>line.startsWith('Automatic installation failed:')&&line.includes('upgrade_fixture_refused')));
        assert.ok(f.calls.slice(before).some(line=>line.startsWith('AIRK installation needs attention:')));
    }finally{await f.close();}
});

for(const command of ['airk.installCli','airk.retryUpdate']){
    test(`${command} upgrades a completed older installation to the shipped version`,async context=>{
        const f=await serviceTestHost(context,{runtimeVersion:'1.5.12',bundledVersion:releaseVersion});
        try{
            const key=`airk.installComplete.2.${f.binding.installDir}`;
            f.values.set(key,`1.5.12:${f.binding.runtimeSha256}`);
            const start=f.calls.length,registrations=structuredClone([...f.registrations]);
            await f.commands.get(command)!();
            assert.equal(f.binding.version,releaseVersion);
            assert.equal(f.values.get(key),`${releaseVersion}:${f.binding.runtimeSha256}`);
            assert.equal(f.calls.slice(start).some(value=>value.includes('explicit_runtime_upgrade_or_matching_extension_required')),false);
            assert.deepEqual([...f.registrations],registrations);assert.equal(f.writes.length,0);
        }finally{await f.close();}
    });
}

test('storage command opens explicit setup without changing the saved location or existing watching',async context=>{
    const f=await serviceTestHost(context,{automatic:true});
    try{
        const moved=require('node:path').join(f.root,'moved-index'),storage=f.settings.get('storage.path');
        f.state.selectedFolders=[{fsPath:moved}];await f.commands.get('airk.chooseStorage')!();
        f.state.pick=items=>items.find(item=>item.enabled===false)??items[0];
        await f.commands.get('airk.indexMode')!();
        const saved=[...f.values.entries()].filter(([key])=>key.startsWith('airk.service.workspace.'));
        const writes=f.writes.length;await f.refreshFolders();assert.equal(f.writes.length,writes);
        assert.deepEqual([...f.values.entries()].filter(([key])=>key.startsWith('airk.service.workspace.')),saved);
        assert.equal(f.settings.get('storage.path'),storage);assert.ok(f.calls.some(value=>value.includes('Set up an index in Indexing')));
        assert.ok(f.writes.some(value=>value.configure_watch?.enabled===false));
    }finally{await f.close();}
});


test('clean break reports the new baseline while missing folders remain unregistered until explicit index setup',async context=>{
    const f=await serviceTestHost(context,{automatic:true,registered:false,roots:2,runtimeVersion:'1.6.0',bundledVersion:releaseVersion});
    try{
        assert.equal(f.binding.version,releaseVersion);
        assert.equal(f.registrations.size,0);
        assert.equal(f.state.picks,0);
        assert.equal(f.notices.length,1);assert.match(f.notices[0].message,/fresh indexes are ready.*native artifacts were removed/);
        assert.equal(f.warnings.length,0);
        const reservations=f.writes.filter(value=>value.reserve_project);
        assert.equal(reservations.length,0);
        const writes=f.writes.length;await f.refreshFolders();assert.equal(f.writes.length,writes);
    }finally{await f.close();}
});


test('a deleted open folder does not trigger registration of the remaining folders',async context=>{
    const fs=await import('node:fs/promises');
    const f=await serviceTestHost(context,{registered:false,roots:2});
    try{
        const missing=f.folders[0].uri.fsPath;
        const held=path.join(f.root,'moved-folder');
        await fs.rename(missing,held);
        f.settings.set('cli.autoInstall',true);await f.refreshFolders();
        assert.equal(f.registrations.size,0);
        assert.equal(f.warnings.length,0);
        assert.equal(f.state.picks,0);
        assert.ok(f.calls.some(value=>value.startsWith('Source folder is missing:')));
        assert.equal(f.writes.filter(value=>value.reserve_project).length,0);
    }finally{await f.close();}
});

test('transient selected metadata failure never authorizes creating missing indexes',async context=>{
    const f=await serviceTestHost(context,{registered:false,roots:2});
    try{
        context.mock.method(RuntimeHost.prototype as any,'invoke',async(_command:string,args:string[])=>{
            assert.deepEqual(args.slice(0,2),['runtime','source-scope']);
            throw new ServiceError('selected_metadata_fixture_unavailable');
        });
        f.settings.set('cli.autoInstall',true);await f.refreshFolders();
        assert.equal(f.registrations.size,0);
        assert.equal(f.warnings.length,0);
        assert.ok(f.calls.some(line=>line.includes('Selected project availability is unconfirmed:')&&line.includes('selected_metadata_fixture_unavailable')));
        assert.ok(!f.calls.some(line=>line.includes('Project preparation needs attention:')));
        assert.equal(f.writes.filter(value=>value.reserve_project).length,0);
    }finally{await f.close();}
});

test('first installation waits for review: dismiss and defer change nothing persistent until explicit setup',async context=>{
    const f=await serviceTestHost(context,{fresh:true,registered:false});
    try{
        f.settings.set('cli.autoInstall',true);
        await f.refreshFolders();
        const reviews=()=>f.notices.filter(notice=>notice.message==='Set up AIRK on this computer?').length;
        assert.equal(reviews(),1);
        assert.equal(f.state.ensured,0);assert.equal(f.registrations.size,0);
        assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')),false);
        f.state.noticeAction='Not now';await f.refreshFolders();
        assert.equal(reviews(),2);assert.equal(f.state.ensured,0);
        await f.refreshFolders();
        assert.equal(reviews(),2,'a saved deferral is honored without asking again');
        assert.equal(f.state.ensured,0);assert.equal(f.registrations.size,0);
        await f.commands.get('airk.setUp')!();
        assert.ok(f.state.ensured>0);assert.equal(f.registrations.size,0);assert.equal(f.writes.filter(value=>value.reserve_project).length,0);
    }finally{await f.close();}
});

test('first installation review can disable automatic setup or accept it once',async context=>{
    const f=await serviceTestHost(context,{fresh:true,registered:false});
    try{
        f.settings.set('cli.autoInstall',true);f.state.noticeAction="Don't set up automatically";
        await f.refreshFolders();
        assert.equal(f.settings.get('cli.autoInstall'),false);assert.equal(f.state.ensured,0);
        f.settings.set('cli.autoInstall',true);f.state.noticeAction='Set up';
        await f.refreshFolders();
        assert.ok(f.state.ensured>0);assert.equal(f.registrations.size,0);assert.equal(f.writes.filter(value=>value.reserve_project).length,0);
        const reviews=f.notices.length;await f.refreshFolders();
        assert.equal(f.notices.length,reviews,'accepted setup is not reviewed again');
    }finally{await f.close();}
});

test('customized first installation offers shell integration without folder registration choices',async context=>{
    const f=await serviceTestHost(context,{fresh:true,registered:false,roots:2});
    try{
        f.settings.set('cli.autoInstall',true);f.state.noticeAction='Customize…';
        let offered:any[]=[];
        f.state.pick=items=>{offered=items;return [];};
        await f.refreshFolders();
        assert.equal(offered.length,1);assert.equal(offered[0].label,'Terminal command in external shells');assert.equal(offered.some(item=>item.folder),false);
        assert.equal(f.settings.get('cli.shellIntegration'),'off');
        assert.equal(f.registrations.size,0);assert.equal(f.writes.filter(value=>value.reserve_project).length,0);
    }finally{await f.close();}
});

test('home folders and ordinary folders both remain unregistered after runtime setup',async context=>{
    const os=require('node:os') as typeof import('node:os');
    const f=await serviceTestHost(context,{fresh:true,registered:false,roots:2});
    try{
        context.mock.method(os,'homedir',()=>f.folders[0].uri.fsPath);
        f.settings.set('cli.autoInstall',true);f.state.noticeAction='Set up';
        await f.refreshFolders();
        assert.ok(f.notices.some(notice=>notice.message==='Set up AIRK on this computer?'));
        assert.equal(f.registrations.size,0);assert.equal(f.writes.filter(value=>value.reserve_project).length,0);
        assert.ok(f.calls.some(line=>line.includes('Opening folders does not create indexes')));
    }finally{await f.close();}
});

test('an uninstall that never wrote a final result is reported as interrupted, never as success',async context=>{
    const fs=await import('node:fs/promises');
    const f=await serviceTestHost(context);
    try{
        const directory=path.join(f.root,'native-state','extension-uninstall');await fs.mkdir(directory,{recursive:true});
        await fs.writeFile(path.join(directory,'last-result.json'),JSON.stringify({schema:1,state:'started',startedAt:'2026-10-30T00:00:00Z',pid:2147483646,job:'/tmp/airk-uninstall-fixture'}));
        await f.commands.get('airk.showLastUninstallResult')!();
        assert.ok(f.calls.some(line=>line.includes('interrupted or unknown')),f.calls.join('\n'));
        f.settings.set('cli.autoInstall',true);await f.refreshFolders();
        assert.ok(f.warnings.some(message=>message.includes('did not finish cleanly')));
    }finally{await f.close();}
});

test('data review is read-only until a plan digest is confirmed and never deletes project indexes directly',async context=>{
    const bootstrap=require('../serviceBootstrap') as typeof import('../serviceBootstrap');
    const f=await serviceTestHost(context);
    const calls:string[][]=[];
    context.mock.method(bootstrap,'nativeRunner',async(_main:string,args:string[])=>{
        calls.push(args);
        if(args[1]==='inventory')return {protocol:1,incomplete:false,not_covered:['exports'],projects:[{collection:'fixture',root:'/src',data_root:'/data',storage_warning:'storage_not_private: fixture'}],
            locations:[{id:'stable:logs',kind:'logs',path:'/logs',exists:true,bytes:1048576,complete:true,purge:'purgeable',consequence:'Deletes logs.'},
                {id:'stable:data',kind:'data',path:'/data',exists:true,bytes:1,complete:true,purge:'requires_project_removal',consequence:'Indexes.'}]};
        if(args.includes('--confirm'))return {protocol:1,purged:[{id:'stable:logs',path:'/logs',files_deleted:3}],retained:[],complete:true};
        return {protocol:1,plan:[{id:'stable:logs',path:'/logs',bytes:1048576,consequence:'Deletes logs.'}],plan_digest:'ab'.repeat(32)};
    });
    try{
        let offered:any[]=[];
        f.state.pick=items=>{offered=items;return items.filter((item:any)=>item.label==='stable:logs'||item.label==='stable:data');};
        f.state.confirm=false;
        await f.commands.get('airk.reviewData')!();
        assert.deepEqual(calls.map(args=>args.includes('--confirm')),[false,false],'declining the confirmation deletes nothing');
        assert.ok(f.calls.some(line=>line.includes('Not removable here')));
        assert.ok(offered.some((item:any)=>item.project&&String(item.detail).includes('other accounts may read')),'a storage privacy warning is shown');
        f.state.confirm=true;calls.length=0;
        await f.commands.get('airk.reviewData')!();
        assert.deepEqual(calls.at(-1)!.slice(-2),['--confirm','ab'.repeat(32)]);
        assert.ok(f.calls.some(line=>line.includes('Deleted 3 files')));
    }finally{await f.close();}
});

test('Windows extension-path casing aliases keep the same loaded code active',async context=>{
    if(process.platform!=='win32'){context.skip('Windows path identity');return;}
    const f=await serviceTestHost(context,{codeFile:true,automatic:true});
    try{
        for(const extensionPath of [f.root.toLowerCase(),f.root.toUpperCase(),path.toNamespacedPath(f.root.toUpperCase())]){
            f.stub.extensions.getExtension=()=>({extensionPath,packageJSON:{version:f.state.installedVersion}});
            f.focus();f.extensionsChanged();
            await new Promise(resolve=>setImmediate(resolve));
            assert.equal(f.notices.length,0,extensionPath);
            assert.doesNotMatch(f.statusBar.text,/Reload to finish update/,extensionPath);
            assert.notEqual((await f.api.getServerDefinitions()).length,0,'matching code retains native MCP admission');
        }
        assert.equal(f.calls.includes('workbench.action.reloadWindow'),false);
    }finally{await f.close();}
});

for(const change of ['size','mtime'] as const)test(`a same-version VSIX ${change} change still offers reload and fences older code`,async context=>{
    const fs=await import('node:fs/promises');
    const f=await serviceTestHost(context,{codeFile:true,automatic:true});
    try{
        f.focus();f.extensionsChanged();
        assert.equal(f.notices.some(notice=>notice.actions.includes('Reload to finish update')),false,'an unchanged installation stays quiet');
        const file=path.join(f.root,'out','serviceExtension.js'),before=await fs.stat(file);
        if(change==='size'){
            await fs.writeFile(file,'// fixture build 2 with different bytes\n');
            assert.notEqual((await fs.stat(file)).size,before.size);
        }else{
            await fs.utimes(file,before.atime,new Date(before.mtimeMs+5000));
            const after=await fs.stat(file);
            assert.equal(after.size,before.size,'mtime-only change retains identical file size');
            assert.notEqual(after.mtimeMs,before.mtimeMs);
        }
        f.focus();f.extensionsChanged();
        assert.ok(f.notices.some(notice=>notice.message.includes('(rebuilt) is installed')&&notice.actions.includes('Reload to finish update')));
        assert.match(f.statusBar.text,/Reload to finish update/);
        assert.deepEqual(await f.api.getServerDefinitions(),[],'changed code cannot admit native MCP through the older interface');
        assert.equal(f.calls.includes('workbench.action.reloadWindow'),false,'reload remains an explicit user action');
    }finally{await f.close();}
});

for(const asset of ['explorerWebview.js','explorerWebview.css','workbenchWebview.js','workbenchWebview.css'])test(`a same-version ${asset} replacement updates the open Explorer without refocusing`,async context=>{
    const fs=await import('node:fs/promises');
    const f=await serviceTestHost(context,{codeFile:true,automatic:true});
    try{
        await f.commands.get('airk.explorer.open')!();
        const panel=f.panels.find(panel=>panel.viewType==='airk.explorer');assert.ok(panel);
        await panel.send({v:1,type:'ready',requestId:'asset-ready'});
        const hostFile=path.join(f.root,'out','serviceExtension.js'),before=await fs.stat(hostFile);
        f.assetChanged();assert.equal(f.notices.length,0,'unchanged assets stay quiet');
        await fs.writeFile(path.join(f.root,'out',asset),'// replacement UI with changed bytes\n');
        f.assetChanged();f.assetCreated();
        await new Promise(resolve=>setImmediate(resolve));
        assert.equal((await fs.stat(hostFile)).mtimeMs,before.mtimeMs,'host code remains unchanged');
        assert.equal(f.notices.length,1,'repeated asset events offer one reload');
        assert.match(f.statusBar.text,/Reload to finish update/);
        const state=panel.posted.filter((entry:any)=>entry.type==='state').at(-1)?.state;
        assert.deepEqual(state.extensionUpdate,{runningVersion:'1.4.0',installedVersion:'1.4.0 (rebuilt)'});
        assert.equal(f.panels.filter(panel=>panel.viewType==='airk.explorer').length,1,'the existing panel receives the update');
        assert.equal(f.calls.includes('workbench.action.reloadWindow'),false,'replacement never reloads without a click');
        assert.deepEqual(await f.api.getServerDefinitions(),[],'old host is fenced after UI replacement');
    }finally{await f.close();}
    assert.ok(f.assetWatchers.every(watcher=>watcher.disposed),'extension asset watchers are disposed');
});

test('an incomplete asset replacement waits for the complete payload before offering reload',async context=>{
    const fs=await import('node:fs/promises');
    const f=await serviceTestHost(context,{codeFile:true,automatic:true});
    try{
        const file=path.join(f.root,'out','explorerWebview.css'),original=await fs.readFile(file);
        await fs.unlink(file);f.assetChanged();
        assert.equal(f.notices.length,0,'temporarily missing assets do not label a rebuild');
        await fs.writeFile(file,Buffer.concat([original,Buffer.from('/* replacement */')]));f.assetCreated();
        assert.equal(f.notices.length,1,'creation of the complete replacement is observed');
    }finally{await f.close();}
});


for(const [kind,roots,workspaceFile] of [['folder',1,false],['single-root workspace',1,true],['multi-root workspace',2,true]] as const){
    test(`opening a new unindexed ${kind} never creates or maintains an index`,async context=>{
        const f=await serviceTestHost(context,{automatic:true,registered:false,roots,workspaceFile});
        try{
            assert.equal(f.folders.length,roots);
            assert.equal(Boolean(f.stub.workspace.workspaceFile),workspaceFile,'activation receives a saved workspace file for workspace cases');
            if(workspaceFile)assert.match(f.stub.workspace.workspaceFile.fsPath,/\.code-workspace$/);
            assert.ok(f.state.ensured>0,'runtime setup can complete independently of index creation');
            const untouched=()=>{
                assert.equal(f.registrations.size,0,'opening sources cannot reserve a native index');
                assert.equal(f.writes.length,0,'opening sources cannot enqueue work, configure watching or plan index work');
                assert.deepEqual(f.nativeMethods.filter(name=>/^(reserve_project|enqueue|configure_watch|catalog_prepare|catalog_apply|catalog_index_setup_apply|catalog_index_setup_draft)$/.test(name)),[],'no index-creation or source-maintenance transport is called');
                assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.service.workspace.1.')),false,'no editor registration is invented');
            };
            untouched();
            assert.deepEqual(await f.api.getServerDefinitions(),[],'no automatic MCP evidence scope is fabricated');
            f.focus();f.extensionsChanged();await f.refreshFolders();
            untouched();
            assert.deepEqual(await f.api.getServerDefinitions(),[]);
            assert.ok(f.calls.some(line=>line.includes('Opening folders does not create indexes.')));
        }finally{await f.close();}
    });
}

/** Model the editor's real guarded file/save boundary; disk verification remains production. */
function migrationEditor(f:Awaited<ReturnType<typeof serviceTestHost>>,save=false){
    const documents=new Map<string,any>();
    f.stub.WorkspaceEdit=class{edits:any[]=[];replace(uri:any,_range:any,text:string){this.edits.push({uri,text});}};
    f.stub.Range=class{constructor(..._args:any[]){}};
    f.stub.workspace.openTextDocument=async(uri:any)=>{
        if(typeof uri.content==='string')return uri;
        const filename=uri.fsPath;let document=documents.get(filename);
        if(!document){let text=await fs.readFile(filename,'utf8');document={uri,version:1,eol:1,isDirty:false,getText:()=>text,positionAt:(offset:number)=>offset,
            replace:(next:string)=>{text=next;document.version++;document.isDirty=true;},
            save:async()=>{if(save)return false;await fs.writeFile(filename,text);document.isDirty=false;return true;}};
            documents.set(filename,document);f.stub.workspace.textDocuments.push(document);
        }return document;
    };
    f.stub.workspace.applyEdit=async(edit:any)=>{for(const row of edit.edits){const doc=await f.stub.workspace.openTextDocument(row.uri);doc.replace(row.text);}return true;};
    return documents;
}
function legacyConfiguration(f:Awaited<ReturnType<typeof serviceTestHost>>){
    const registration=[...f.registrations.values()][0],command=path.join(f.binding.installDir,process.platform==='win32'?'airk-launcher.exe':'airk-launcher');
    return {registration,command,configuration:{keep:'unrelated user setting',mcpServers:{['airk-'+registration.collection]:{command,args:['runtime','launch','--install-dir',f.binding.installDir,'--','--root',registration.root,'--data-dir',registration.data_root,'--collection',registration.collection,'--service-project',registration.binding,'mcp'],disabled:true},other:{command:'other-tool',args:['unchanged']}}}};
}
test('upgrade migrates exact owned configuration before retiring launchers and publishes completion only after verification',async context=>{
    const f=await serviceTestHost(context,{automatic:false});
    try{
        const {registration,command,configuration}=legacyConfiguration(f),filename=path.join(registration.root,'.mcp.json'),before=JSON.stringify(configuration);
        await fs.writeFile(filename,before);migrationEditor(f);
        context.mock.method(RuntimeHost.prototype,'inspectLegacyLaunchers',async()=>({schema:1,active:f.binding.runtimeSha256,launcher:{path:command,sha256:'ab'.repeat(32)},universal_launcher:null}));
        let removed=0;context.mock.method(RuntimeHost.prototype,'retireLegacyLaunchers',async()=>{
            removed++;const current=JSON.parse(await fs.readFile(filename,'utf8'));
            assert.equal(current.mcpServers.airk.url,f.httpUrl);assert.equal(current.mcpServers.airk.command,undefined);assert.equal(current.mcpServers.airk.disabled,true);
            assert.deepEqual(current.mcpServers.other,configuration.mcpServers.other);assert.equal(current.keep,configuration.keep);
            assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')),false,'No premature successful-upgrade marker');
            return {schema:1,active:f.binding.runtimeSha256,legacy_launchers_retired:true,launcher:null,universal_launcher:null,processes:{scope:'verified_launcher_images',remaining:0,observations:[{fixture:'held native image audit'}]}};
        });
        await f.commands.get('airk.installCli')!();assert.equal(removed,1);
        assert.ok([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')));
        const imported=f.state.httpConnections.find((value:any)=>value.client==='claude');assert.ok(imported);assert.equal(imported.selection.kind,'imported_legacy');
        assert.equal(imported.selection.selection.configuration_receipt,createHash('sha256').update(before).digest('hex'));
        assert.deepEqual(imported.selection.selection.registration,{binding:registration.binding,root:registration.root,data_root:registration.data_root,collection:registration.collection});
        assert.equal(f.calls.some(value=>value.includes('upgrade completion blocked')),false);
    }finally{await f.close();}
});
test('one install leaves a standalone sibling unchanged for review and still migrates managed stdio',async context=>{
    // Simplification S2, decision Q1: standalone saved indexes are never adopted.
    // The entry, its cached image and the user's other entries stay unchanged;
    // no cache retirement journal is prepared while the entry remains.
    const f=await serviceTestHost(context,{automatic:false});
    try{
        const {registration,configuration}=legacyConfiguration(f),destination=path.join(registration.root,'.mcp.json'),alias='old_standalone',digest='ba'.repeat(32),cached=path.join(f.root,'private-editor','User','globalStorage','iokaio.airk','helpers','1.3.1-windows-x64-'+digest.slice(0,16),'airk.exe');
        const fields={command:cached,args:['--root',registration.root,'--data-dir',path.join(f.root,'old-standalone-data'),'--collection','old-saved','mcp'],disabled:true,alwaysAllow:['airk_search']};
        const initial={...configuration,mcpServers:{...configuration.mcpServers,[alias]:fields}};await fs.writeFile(destination,JSON.stringify(initial));migrationEditor(f);
        let inspections=0,preparations=0,retirements=0,adoptions=0;
        context.mock.method(RuntimeHost.prototype,'inspectLegacyCaches',async(selected:typeof f.binding,storage:string,input?:string)=>{
            inspections++;const locators=input?JSON.parse(await fs.readFile(input,'utf8')):[];
            assert.equal(locators.length,1,'the scanned standalone declaration is offered to native inspection');assert.equal(locators[0].server,alias);
            return {schema:1,active:selected.runtimeSha256,scope:'context-only',context_global_storage:storage,global_storage_roots:[storage],caches:[{executable:cached}],
                inspection_sha256:'ad'.repeat(32),origin_locators:locators,adoption_locators:locators,managed_locators:[],origin_proofs:[],
                origin_classifications:locators.map((locator:unknown)=>({locator,origin_sha256:'10'.repeat(32),kind:'standalone',authority:'not_observed'}))};
        });
        context.mock.method(RuntimeHost.prototype,'prepareLegacyCaches',async()=>{preparations++;throw new Error('no cache retirement journal while a standalone entry remains');});
        context.mock.method(RuntimeHost.prototype,'retireLegacyCaches',async()=>{retirements++;throw new Error('retained standalone caches are not retired');});
        context.mock.method(RuntimeHost.prototype,'adoptLegacyIndexes',async()=>{adoptions++;throw new Error('standalone adoption is not supported');});
        await f.commands.get('airk.installCli')!();
        const after=JSON.parse(await fs.readFile(destination,'utf8')),managedKey='airk';
        // The single managed stdio entry becomes the one HTTP entry; the standalone and unrelated entries keep their keys.
        assert.deepEqual(Object.keys(after.mcpServers).sort(),['airk',alias,'other'].sort());
        assert.deepEqual(after.mcpServers[alias],fields,'The standalone entry is byte-for-byte unchanged');
        assert.equal(after.mcpServers[managedKey].url,f.httpUrl,'The managed sibling still migrates to HTTP');assert.equal(after.mcpServers[managedKey].command,undefined);
        assert.deepEqual(after.mcpServers.other,initial.mcpServers.other);
        assert.equal(adoptions,0);assert.equal(preparations,0);assert.equal(retirements,0);assert.ok(inspections>=1);
        assert.ok(f.calls.some(value=>value.includes(`Standalone AIRK agent entry "${alias}"`)&&value.includes('left unchanged for review')),'The user is told which entry needs review');
        assert.ok(f.calls.some(value=>value.includes('AIRK kept old helper caches: 1 standalone agent entry')),'Cache retention is reported');
        assert.ok([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')),'Installation completes with the entry retained');
    }finally{await f.close();}
});

test('a retained standalone entry does not waive native proof for another entry that launches a cached image',async context=>{
    const f=await serviceTestHost(context,{automatic:false});
    try{
        const {registration,configuration}=legacyConfiguration(f),destination=path.join(registration.root,'.mcp.json'),alias='old_standalone',digest='ba'.repeat(32),cached=path.join(f.root,'private-editor','User','globalStorage','iokaio.airk','helpers','1.3.1-windows-x64-'+digest.slice(0,16),'airk.exe');
        const standalone={command:cached,args:['--root',registration.root,'--data-dir',path.join(f.root,'old-standalone-data'),'--collection','old-saved','mcp']};
        const unproven={command:cached,args:['--root',registration.root,'--data-dir',path.join(f.root,'other-data'),'--collection','other','mcp']};
        const initial={...configuration,mcpServers:{...configuration.mcpServers,[alias]:standalone,'airk-unproven':unproven}},before=JSON.stringify(initial);await fs.writeFile(destination,before);migrationEditor(f);
        context.mock.method(RuntimeHost.prototype,'inspectLegacyCaches',async(selected:typeof f.binding,storage:string,input?:string)=>{
            const locators=(input?JSON.parse(await fs.readFile(input,'utf8')):[]) as {server:string}[];
            return {schema:1,active:selected.runtimeSha256,scope:'context-only',context_global_storage:storage,global_storage_roots:[storage],caches:[{executable:cached}],inspection_sha256:'af'.repeat(32),
                origin_locators:locators,adoption_locators:locators.filter(locator=>locator.server===alias),managed_locators:[],origin_proofs:[],origin_classifications:[]};
        });
        await f.commands.get('airk.installCli')!();
        assert.ok(f.calls.some(value=>value.includes('standalone_index_adoption_required')),'The unproven cached-image entry is refused with its existing typed error');
        assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')),false,'Installation does not complete over an unproven entry');
        const after=JSON.parse(await fs.readFile(destination,'utf8'));
        assert.deepEqual(after.mcpServers[alias],standalone);assert.deepEqual(after.mcpServers['airk-unproven'],unproven);
    }finally{await f.close();}
});

test('a standalone locator whose entry was already removed does not hold the caches back',async context=>{
    // A pending native inspection can still list a reviewed-and-removed entry.
    // Only declarations that exist now are retained; otherwise retirement proceeds.
    const f=await serviceTestHost(context,{automatic:false});
    try{
        const {registration,configuration}=legacyConfiguration(f),destination=path.join(registration.root,'.mcp.json');
        await fs.writeFile(destination,JSON.stringify(configuration));migrationEditor(f);
        const removed={path:destination,format:'mcp_servers_json',server:'old_standalone',container_path:['mcpServers']};
        context.mock.method(RuntimeHost.prototype,'inspectLegacyCaches',async(selected:typeof f.binding,storage:string)=>({schema:1,active:selected.runtimeSha256,scope:'context-only',context_global_storage:storage,
            global_storage_roots:[storage],caches:[],inspection_sha256:'ae'.repeat(32),origin_locators:[removed],adoption_locators:[removed],managed_locators:[],origin_proofs:[],origin_classifications:[]}));
        await f.commands.get('airk.installCli')!();
        assert.ok(f.calls.includes('legacy-caches-prepared'),'Cache retirement is prepared once no standalone entry remains');
        assert.ok(f.calls.includes('legacy-caches-retired'));
        assert.equal(f.calls.some(value=>value.includes('left unchanged for review')),false);
        assert.ok([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')));
    }finally{await f.close();}
});

test('cache native preimages precede original config edits and cache uncertainty prevents launcher/runtime completion',async context=>{
    const f=await serviceTestHost(context,{automatic:false});
    try{
        const {registration,configuration}=legacyConfiguration(f),destination=path.join(registration.root,'.mcp.json'),before=JSON.stringify(configuration);await fs.writeFile(destination,before);migrationEditor(f);
        let accepted:any,attempts=0;
        context.mock.method(RuntimeHost.prototype,'prepareLegacyCaches',async(selected:typeof f.binding,storage:string)=>{
            assert.equal(selected.runtimeSha256,f.binding.runtimeSha256);
            if(!accepted){assert.equal(await fs.readFile(destination,'utf8'),before,'Original complete bytes exist when native accepts its cleanup plan');accepted={schema:1,active:selected.runtimeSha256,scope:'context-only',context_global_storage:storage,global_storage_roots:[storage],caches:[],origin_proofs:[],origin_locators:[],adoption_locators:[],managed_locators:[],origin_classifications:[],inspection_sha256:'ab'.repeat(32)};}
            return accepted;
        });
        context.mock.method(RuntimeHost.prototype,'retireLegacyCaches',async(selected:typeof f.binding,prepared:Record<string,unknown>)=>{
            assert.equal(prepared,accepted);assert.equal(selected.runtimeSha256,prepared.active);
            const current=JSON.parse(await fs.readFile(destination,'utf8'));assert.equal(current.mcpServers.airk.url,f.httpUrl,'Migration is saved before native can retire old executable images');
            if(++attempts===1)throw new ServiceError('legacy_cache_process_identity_unknown',true,{scope:'context-only',processes:{remaining:null,observations:[{unobservable:true}]}});
            return {scope:'context-only',global_storage_roots:prepared.global_storage_roots,caches_retired:true,remaining:0};
        });
        await f.commands.get('airk.installCli')!();
        assert.equal(f.calls.includes('legacy-launchers-retired'),false);assert.equal(f.calls.includes('previous-runtime-retired'),false);assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')),false);
        const migrated=f.state.httpConnections.filter(value=>value.client==='claude');assert.equal(migrated.length,1);await f.commands.get('airk.installCli')!();
        assert.equal(attempts,2);assert.deepEqual(f.state.httpConnections.filter(value=>value.client==='claude'),migrated,'Resumption verifies already migrated HTTP authority instead of enrolling it again');
        assert.ok(f.calls.includes('legacy-launchers-retired'));assert.ok(f.calls.includes('previous-runtime-retired'));assert.ok([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')));
    }finally{await f.close();}
});

for(const obstruction of ['dirty','save_failed','unsupported_client','native_unknown'] as const){
    test(`upgrade stays incomplete and never retires launchers before ${obstruction} is resolved`,async context=>{
        const f=await serviceTestHost(context,{automatic:false});
        try{
            const {registration,configuration}=legacyConfiguration(f);
            const filename=obstruction==='unsupported_client'?path.join(registration.root,'.gemini','settings.json'):path.join(registration.root,'.mcp.json');
            await fs.mkdir(path.dirname(filename),{recursive:true});const before=JSON.stringify(configuration);await fs.writeFile(filename,before);
            migrationEditor(f,obstruction==='save_failed');
            if(obstruction==='dirty'){const document=await f.stub.workspace.openTextDocument(f.stub.Uri.file(filename));document.isDirty=true;}
            let removed=0;context.mock.method(RuntimeHost.prototype,'retireLegacyLaunchers',async()=>{removed++;throw new ServiceError('legacy_launcher_process_identity_unknown',true,{observations:[{unknown:true}]});});
            await f.commands.get('airk.installCli')!();
            assert.equal(removed,obstruction==='native_unknown'?1:0);
            assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')),false);
            assert.ok(f.calls.some(value=>value.includes('upgrade completion blocked')));
            if(obstruction!=='native_unknown')assert.equal(await fs.readFile(filename,'utf8'),before,'Unconfirmed source file preserved');
        }finally{await f.close();}
    });
}

for(const flag of ['--data-dir','--service-project'] as const)test(`generated legacy migration refuses a crossed original ${flag} instead of selecting by source root`,async context=>{
    const f=await serviceTestHost(context,{automatic:false});
    try{
        const {registration,configuration}=legacyConfiguration(f),destination=path.join(registration.root,'.mcp.json'),entry=configuration.mcpServers['airk-'+registration.collection],index=entry.args.indexOf(flag);entry.args[index+1]=flag==='--data-dir'?path.join(f.root,'different-index-data'):'fe'.repeat(32);
        const before=JSON.stringify(configuration);await fs.writeFile(destination,before);migrationEditor(f);await f.commands.get('airk.installCli')!();
        assert.equal(await fs.readFile(destination,'utf8'),before);assert.equal(f.state.httpConnections.filter(value=>value.client==='claude').length,0);assert.equal(f.calls.includes('legacy-launchers-retired'),false);assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')),false);
    }finally{await f.close();}
});
for(const secondFailure of [false,true])test(`cache inventory recovery performs one native reprepare and complete migration review (${secondFailure?'second refusal remains incomplete':'new inventory completes'})`,async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false});
    try{
        let current:any,retirements=0,replans=0,preparations=0;
        context.mock.method(RuntimeHost.prototype,'prepareLegacyCaches',async(selected:typeof f.binding,storage:string)=>{preparations++;current??={schema:1,active:selected.runtimeSha256,scope:'context-only',context_global_storage:storage,global_storage_roots:[storage],caches:[],origin_proofs:[],origin_locators:[],adoption_locators:[],managed_locators:[],origin_classifications:[],inspection_sha256:'a1'.repeat(32)};return current;});
        context.mock.method(RuntimeHost.prototype,'retireLegacyCaches',async(_binding:typeof f.binding,prepared:Record<string,unknown>)=>{retirements++;assert.equal(prepared,current);if(retirements===1||secondFailure)throw new ServiceError('legacy_cache_retirement_unconfirmed',true,{original_native_failure:{remaining:null}});return {scope:'context-only',caches_retired:true,remaining:0};});
        context.mock.method(RuntimeHost.prototype,'reprepareLegacyCaches',async(_binding:typeof f.binding,previous:Record<string,unknown>)=>{replans++;assert.equal(previous,current);current={...current,inspection_sha256:'a2'.repeat(32)};return current;});
        await f.commands.get('airk.installCli')!();assert.equal(replans,1);assert.equal(retirements,2);assert.equal(preparations,2,'Replacement inventory triggers the full guarded migration phase');assert.equal(f.state.httpConnections.length,0);
        assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')),!secondFailure);assert.equal(f.calls.includes('legacy-launchers-retired'),!secondFailure);
    }finally{await f.close();}
});

test('inactive native roots with unrelated agent settings neither enroll nor block launcher retirement',async context=>{
    const f=await serviceTestHost(context,{automatic:false});
    try{
        const registration=[...f.registrations.values()][0],filename=path.join(registration.root,'.mcp.json'),bytes='{"account":"keep","mcpServers":{"unrelated":{"command":"other"}}}';
        await fs.writeFile(filename,bytes);f.registrations.delete(registration.binding);f.inactiveRegistrations.set(registration.binding,registration);
        await f.commands.get('airk.installCli')!();
        assert.equal(await fs.readFile(filename,'utf8'),bytes);assert.equal(f.state.httpConnections.length,0);
        assert.ok(f.calls.includes('legacy-launchers-retired'));assert.ok([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')));
    }finally{await f.close();}
});
for(const shape of ['nested_owned','owned_custom_alias','owned_unknown_scope','foreign'] as const){
    test(`global ${shape} declaration is migrated or blocks cleanup using original native authority`,async context=>{
        const realInventory=realGlobalInventory,f=await serviceTestHost(context,{automatic:false});
        try{
            const {registration,configuration}=legacyConfiguration(f),home=path.join(f.root,'isolated-global-home');await fs.mkdir(home);
            const entry=configuration.mcpServers['airk-'+registration.collection];
            if(shape==='owned_unknown_scope')entry.args=entry.args.map(arg=>arg===registration.root?path.join(f.root,'unregistered-source'):arg);
            if(shape==='foreign'){entry.command=path.join(f.root,'foreign-install','airk-launcher');entry.args=entry.args.map(arg=>arg===f.binding.installDir?path.dirname(entry.command):arg);}
            const key=shape==='owned_custom_alias'?'custom_named_airk':'airk-'+registration.collection;
            const value={account:'preserve',projects:{'this key is never source authority':{mcpServers:{[key]:entry,unrelated:{command:'other'}}},foreign:{mcpServers:{other:{command:'foreign'}}}}};
            const filename=path.join(home,'.claude.json'),before=JSON.stringify(value);await fs.writeFile(filename,before);migrationEditor(f);
            context.mock.method(globalInventory,'inventoryGlobalAgentConfigs',async(options:globalInventory.GlobalInventoryOptions)=>realInventory({...options,home,env:{},globalStoragePath:undefined,vscodeUserDirectories:[]}));
            let cleanup=0;context.mock.method(RuntimeHost.prototype,'retireLegacyLaunchers',async()=>{cleanup++;return {schema:1,active:f.binding.runtimeSha256,legacy_launchers_retired:true,launcher:null,universal_launcher:null,processes:{scope:'verified_launcher_images',remaining:0,observations:[]}};});
            await f.commands.get('airk.installCli')!();
            if(shape==='nested_owned'){
                assert.equal(cleanup,1);const after=JSON.parse(await fs.readFile(filename,'utf8'));
                const servers=after.projects['this key is never source authority'].mcpServers;assert.equal(servers.airk.url,f.httpUrl);assert.equal(servers.airk.disabled,true);assert.deepEqual(servers.unrelated,value.projects['this key is never source authority'].mcpServers.unrelated);
                assert.deepEqual(after.projects.foreign,value.projects.foreign);assert.equal(after.account,value.account);
                assert.equal((f.state.httpConnections[0].selection.selection as any).configuration_receipt,createHash('sha256').update(before).digest('hex'),'Actual complete original global bytes bind enrollment');
            }else{
                assert.equal(await fs.readFile(filename,'utf8'),before);assert.equal(cleanup,shape==='foreign'?1:0);assert.equal(f.state.httpConnections.filter((row:any)=>row.client==='claude').length,0);
            }
        }finally{await f.close();}
    });
}
test('interrupted native cleanup resumes without stale completion and repeated runtime replacement reruns finalization',async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false});
    try{
        let attempts=0;context.mock.method(RuntimeHost.prototype,'retireLegacyLaunchers',async()=>{attempts++;if(attempts===1)throw new ServiceError('legacy_launcher_identity_unknown',true,{phase:'removing'});return {schema:1,active:f.binding.runtimeSha256,legacy_launchers_retired:true,launcher:null,universal_launcher:null,processes:{scope:'verified_launcher_images',remaining:0,observations:[]}};});
        await f.commands.get('airk.installCli')!();assert.equal(attempts,1);assert.equal([...f.values.keys()].some(key=>key.startsWith('airk.installComplete.2.')),false);
        await f.commands.get('airk.installCli')!();assert.equal(attempts,2);const key='airk.installComplete.2.'+f.binding.installDir,first=f.values.get(key);assert.ok(first);
        f.binding.runtimeSha256='ef'.repeat(32);await f.commands.get('airk.installCli')!();assert.equal(attempts,3);assert.notEqual(f.values.get(key),first);assert.equal(f.values.get(key),f.binding.version+':'+f.binding.runtimeSha256);
        assert.ok(f.calls.filter(value=>value==='previous-runtime-retired').length>=2,'Each completed distinct runtime retires previous payloads after the gate');
    }finally{await f.close();}
});

test('future modern upgrade completes an older fully occupied cleanup before admitting another payload',async context=>{
    const f=await serviceTestHost(context,{automatic:false,registered:false,bundledVersion:'1.8.15',runtimeVersion:'1.8.14',installedVersion:'1.8.15'});
    try{
        f.binding.launcher=null;const order:string[]=[];
        context.mock.method(RuntimeHost.prototype,'retireLegacyLaunchers',async(selected:typeof f.binding)=>{order.push('launchers-'+selected.version);return {schema:1,active:selected.runtimeSha256,legacy_launchers_retired:true,launcher:null,universal_launcher:null,processes:{scope:'verified_launcher_images',remaining:0,observations:[]}};});
        context.mock.method(RuntimeHost.prototype,'retirePrevious',async(selected:typeof f.binding)=>{order.push('runtimes-'+selected.version);return {active:selected.runtimeSha256,retired_previous:null,retired_runtimes:[],complete:true,selected_service_restarted:false,processes:{scope:'verified_superseded_runtime_images',remaining:0,observations:[]}};});
        const transition=RuntimeHost.prototype.transition;
        context.mock.method(RuntimeHost.prototype,'transition',async function(this:RuntimeHost,...args:Parameters<RuntimeHost['transition']>){order.push('activate-next');return transition.apply(this,args);});
        await f.commands.get('airk.installCli')!();
        assert.deepEqual(order,['launchers-1.8.14','runtimes-1.8.14','activate-next','launchers-1.8.15','runtimes-1.8.15']);
        assert.ok(f.calls.includes('selected-modern-metadata-verified'),'Older selected binding uses its narrowly verified metadata adapter');
        assert.equal(f.values.get('airk.installComplete.2.'+f.binding.installDir),'1.8.15:'+f.binding.runtimeSha256);
        assert.equal(f.registrations.size,0);assert.equal(f.writes.filter(value=>value.reserve_project||value.enqueue).length,0);
    }finally{await f.close();}
});

for(const [transientFailures,directoryHint] of [[0,false],[2,false],[0,true]] as const)test(`live native owner replacement refreshes owned HTTP port after ${transientFailures} transient failures (${directoryHint?'closed UI directory watcher':'console observation'}) without installing or enrolling again`,async context=>{
    const f=await serviceTestHost(context,{automatic:false});
    try{
        const definitions=await f.api.getServerDefinitions();assert.equal(definitions.length,1);
        const id=String(definitions[0].headers.Authorization).split('.')[1],info=await RuntimeHost.prototype.httpConnectionInfo.call({} as RuntimeHost,f.binding,id),selection=structuredClone(info.selection);
        const row=[...f.registrations.values()][0],destination=path.join(row.root,'.vscode','mcp.json');await fs.mkdir(path.dirname(destination));
        await fs.writeFile(destination,JSON.stringify({servers:{airk:{type:'http',url:info.endpoint.url,headers:{Authorization:'Bearer ${input:airkToken_'+id+'}','X-AIRK-Response-Mode':'full'},disabled:true},other:{command:'keep'}}}));migrationEditor(f);
        const control=(f.views.get('airk.workbench') as any).host.control;
        const owner=f.context.subscriptions.find((entry:any)=>typeof entry.syncHttpEndpointWatcher==='function');assert.ok(owner);
        const initialDeadline=Date.now()+3000;
        while((owner.observedHttpInstance!==info.endpoint.service_instance||owner.httpConfigurationRefresh||owner.httpWatchReading)&&Date.now()<initialDeadline)await new Promise(resolve=>setTimeout(resolve,10));
        assert.equal(owner.observedHttpInstance,info.endpoint.service_instance,'Initial native owner discovery settles before replacement');
        assert.equal(owner.httpConfigurationRefresh,undefined);
        if(directoryHint){
            assert.equal((control as any).consumers,0,'No Workbench or Explorer consumer polls the service');
            await fs.writeFile(path.join(f.binding.controlDir,'http-1','endpoint.json'),'old notification hint');owner.syncHttpEndpointWatcher();
            const limit=Date.now()+3000;while(owner.observedHttpInstance!=='12'.repeat(16)&&Date.now()<limit)await new Promise(resolve=>setTimeout(resolve,10));
            assert.equal(owner.observedHttpInstance,'12'.repeat(16),'Initial authenticated discovery closes the missed-event gap');
        }
        const next={...info,endpoint:{...info.endpoint,url:'https://127.0.0.1:57423/mcp',service_instance:'98'.repeat(16),generation:info.endpoint.generation+1}};
        let lookups=0,interleavedHealthReads=0;context.mock.method(RuntimeHost.prototype,'lookupHttpConnection',async(binding:typeof f.binding,id:string)=>{
            lookups++;if(lookups<=transientFailures)throw new ServiceError('native_management_busy',false,{safe_read:true});
            // Force the watcher/console overlap that otherwise depends on the
            // initial 200 ms notification timer and the speed of this host.
            if(transientFailures){assert.equal(owner.httpWatchReading,false);await owner.readHttpOwnerHint();interleavedHealthReads++;}
            assert.equal(binding.runtimeSha256,f.binding.runtimeSha256);assert.equal(id,info.connection_id);return {schema:1,active:binding.runtimeSha256,connection_id:id,status:'owned',connection:next};
        });
        context.mock.method(RuntimeHost.prototype,'httpConnectionInfo',async()=>next);
        let changes=0;f.providers[0].onDidChangeMcpServerDefinitions(()=>changes++);
        const before=f.state.httpConnections.length,retirements=f.calls.filter(value=>value==='legacy-launchers-retired').length;
        // Console observations and native watcher reads describe the same
        // replacement owner, including when a retry overlaps a health read.
        f.state.nativeInstance=next.endpoint.service_instance;
        if(!directoryHint)control.controller.state.health={state:'live',checkedAt:Date.now(),value:{status:'health',observation:{version:f.binding.version,runtime_sha256:f.binding.runtimeSha256,instance:next.endpoint.service_instance}}};
        const realNow=Date.now.bind(Date);let offset=0;
        if(transientFailures)context.mock.method(Date,'now',()=>realNow()+offset);
        const emit=()=>{(control as any).stateChanged.fire(control.controller.state);};
        const waitFor=async(predicate:()=>boolean)=>{const limit=realNow()+3000;while(!predicate()&&realNow()<limit)await new Promise(resolve=>setTimeout(resolve,10));assert.ok(predicate());};
        const noisyReads=f.nativeMethods.filter(method=>['resources','workers','events','embedding_status','source_status'].includes(method)).length;
        if(directoryHint){
            f.state.nativeHealthFailures=1;
            const temporary=path.join(f.binding.controlDir,'http-1','endpoint.next');await fs.writeFile(temporary,'invalid descriptor bytes are only a notification hint');
            await fs.rename(temporary,path.join(f.binding.controlDir,'http-1','endpoint.json'));
        }else emit();
        for(let failure=1;failure<=transientFailures;failure++){
            await waitFor(()=>f.calls.filter(value=>value.includes('saved HTTPS connection refresh deferred')).length===failure);
            assert.equal(JSON.parse(await fs.readFile(destination,'utf8')).servers.airk.url,info.endpoint.url);
            emit();await new Promise(resolve=>setTimeout(resolve,15));assert.equal(lookups,failure,'Backoff prevents repeated state events from replaying even the safe read');
            // The next attempt must read the present file and preserve new edits.
            const current=JSON.parse(await fs.readFile(destination,'utf8'));current.recentEdit=failure;await fs.writeFile(destination,JSON.stringify(current));
            offset+=30000;emit();
        }
        const deadline=Date.now()+3000;let actual:any;
        do{actual=JSON.parse(await fs.readFile(destination,'utf8'));if(actual.servers.airk.url===next.endpoint.url&&changes>0)break;await new Promise(resolve=>setTimeout(resolve,10));}while(Date.now()<deadline);
        assert.equal(actual.servers.airk.url,next.endpoint.url,JSON.stringify({lookups,interleavedHealthReads,nativeInstance:f.state.nativeInstance,latestHttpOwner:owner.latestHttpOwner,retry:owner.httpRefreshRetry,failures:f.calls.filter(value=>value.includes('saved HTTPS connection refresh deferred'))}));assert.equal(actual.servers.airk.disabled,true);assert.deepEqual(actual.servers.other,{command:'keep'});
        assert.equal(interleavedHealthReads,transientFailures?1:0);
        assert.ok(changes>0);assert.equal(lookups,1+transientFailures);if(transientFailures){assert.equal(actual.recentEdit,transientFailures);assert.equal(f.warnings.filter(value=>value.includes('Some saved HTTPS connections still need refreshing')).length,1);}assert.equal(f.state.httpConnections.length,before,'Port discovery never reenrolls a credential or changes its grant');
        assert.equal(f.calls.filter(value=>value==='legacy-launchers-retired').length,retirements,'Port refresh performs no installer retirement');assert.deepEqual(next.selection,selection);
        if(directoryHint){assert.equal(f.state.nativeHealthFailures,0,'A transient management failure after publication is retried without a second file event');assert.equal((control as any).consumers,0);assert.equal(f.nativeMethods.filter(method=>['resources','workers','events','embedding_status','source_status'].includes(method)).length,noisyReads,'Endpoint observation does not start general console polling');}
    }finally{await f.close();}
});
