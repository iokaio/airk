// SPDX-License-Identifier: Apache-2.0
import {test} from 'node:test';
import {strict as assert} from 'node:assert';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import {serviceTestHost} from './serviceTestHost';
import {serializeServers,unifiedEntry,launchArgs} from '../model';
import {migrateAgentConfig} from '../agentMigration';
import {vscodeCredentialConfiguration,syncVscodeHttpInput} from '../agentConfig';
import * as globalInventory from '../globalAgentInventory';
const actualGlobalInventory=globalInventory.inventoryGlobalAgentConfigs;

test('standalone credential aliases require the exact native server and VS Code container claim',()=>{
    const digest='f'.repeat(64),namespace='c'.repeat(32),id='a'.repeat(32),key='saved.alias';
    const original={binding:'1'.repeat(64),root:path.resolve('saved-source'),data_root:path.resolve('old-data'),collection:'saved'};
    const registration={...original,binding:'2'.repeat(64),data_root:path.resolve('adopted-data')};
    const setup={caller:'d'.repeat(32),key:{namespace,epoch:1,nonce:'e'.repeat(32)},expected_catalog_revision:1,name:'Saved original',registration,sources:[{canonical_path:original.root,owner:true,aliases:[]}],setup_watch:false};
    const plan={schema:1,setup,original_registration:original,origin_sha256:digest,config_sha256:digest,executable_sha256:digest,quiescence_sha256:digest,archive_inventory_sha256:digest,original_publication_sha256:digest,source_inventory_sha256:digest};
    const claim={schema:1,origin_sha256:digest,config_sha256:digest,executable_sha256:digest,selected_entry_sha256:digest,container_path:['servers'],original_server:key,no_default_project:false};
    const adoption={schema:1,operation_id:'d'.repeat(32),plan,publication:{schema:1,plan_digest:digest,archive_inventory_sha256:digest,original_publication_sha256:digest,current:`airk-r1:${namespace}:2:${digest}`,resource_sha256:digest,compatibility_sha256:digest},original_server:key,no_default_project:false,configuration_claims:[claim]};
    const info:any={schema:1,connection_id:id,endpoint:{schema:1,installation:digest,user:'fixture',namespace,generation:1,service_instance:'d'.repeat(32),executable_sha256:digest,url:'https://127.0.0.1:57421/mcp',ca_sha256:digest,certificate_sha256:digest,state:'ready',predecessor_digest:null},selection:{kind:'imported_legacy',selection:{namespace,expected_catalog_revision:2,registration,configuration_receipt:digest,grant:{grant:{id:'grt_'+'91'.repeat(16),host:digest,revision:1,revoked:false,memberships:[]},provenance:{kind:'imported_standalone_adoption',adoption}}}},ca_certificate_file:path.resolve('native-private','ca.pem'),credential_file:path.resolve('native-private','credential-'+id+'.token')};
    const source=serializeServers('vscode',[{key,label:key,command:'',args:[],http:info}]);
    assert.equal(vscodeCredentialConfiguration(source,info),true);
    claim.container_path=['mcpServers'];assert.equal(vscodeCredentialConfiguration(source,info),false);
    claim.container_path=['servers'];claim.original_server='other.alias';adoption.original_server='other.alias';assert.equal(vscodeCredentialConfiguration(source,info),false);
});

for(const location of ['workspace','user','profile'] as const)test(`preserved VS Code aliases resolve from ${location} configuration without duplicate providers`,async t=>{
    const f=await serviceTestHost(t);
    try{
        const controller=f.context.subscriptions.find((item:any)=>typeof item.resolveAgentSpec==='function');
        const spec=await controller.resolveAgentSpec(controller.getChildren()[0],'vscode'),info=spec.http!;
        assert.equal(info.selection.kind,'imported_legacy');const alias='airk-'+info.selection.selection.registration.collection;
        let file:string;
        if(location==='workspace'){
            const directory=path.join(f.folders[0].uri.fsPath,'.vscode');await fs.mkdir(directory);file=path.join(directory,'mcp.json');
        }else{
            const user=path.join(f.root,'roaming','Code','User'),profile=location==='profile'?path.join(user,'profiles','owned-profile'):user;
            f.context.globalStorageUri={fsPath:path.join(profile,'globalStorage','iokaio.airk')};await fs.mkdir(f.context.globalStorageUri.fsPath,{recursive:true});file=path.join(profile,'mcp.json');
            t.mock.method(globalInventory,'inventoryGlobalAgentConfigs',async(options:globalInventory.GlobalInventoryOptions)=>actualGlobalInventory({...options,home:f.root,
                env:{APPDATA:path.join(f.root,'roaming'),XDG_CONFIG_HOME:path.join(f.root,'config')}}));
        }
        const config=JSON.parse(serializeServers('vscode',[{...unifiedEntry(spec),key:alias}]));config.servers[alias].disabled=true;
        await fs.writeFile(file,JSON.stringify(config));
        const command=f.commands.get('airk.httpCredential')!,input={connection_id:info.connection_id};
        assert.equal(await command(input),'airk1.'+info.connection_id+'.'+'a'.repeat(64));
        assert.deepEqual(await f.api.getServerDefinitions(),[],'manual alias owns disabled policy');
        config.inputs.push(config.inputs[0]);await fs.writeFile(file,JSON.stringify(config));
        await assert.rejects(command(input),/matching_workspace_configuration/);
        assert.deepEqual(await f.api.getServerDefinitions(),[],'an invalid input must not bypass the disabled native alias');
        config.inputs.pop();config.servers.unconfirmed_alias=config.servers[alias];delete config.servers[alias];await fs.writeFile(file,JSON.stringify(config));
        await assert.rejects(command(input),/matching_workspace_configuration/,'a different name has no native alias claim');
        assert.equal(f.writes.length,0);
    }finally{await f.close();}
});

test('multiple exact native IDs in one VS Code file resolve individually while repeated references refuse',async t=>{
    const f=await serviceTestHost(t,{roots:2});
    try{
        const controller=f.context.subscriptions.find((item:any)=>typeof item.resolveAgentSpec==='function'),specs=[] as any[];
        for(const spec of controller.getChildren())specs.push(await controller.resolveAgentSpec(spec,'vscode'));
        const configs=specs.map(spec=>JSON.parse(serializeServers('vscode',[{...unifiedEntry(spec),key:'airk-'+spec.collection}])));
        const config={servers:Object.assign({},...configs.map(value=>value.servers)),inputs:configs.flatMap(value=>value.inputs)};
        const directory=path.join(f.folders[0].uri.fsPath,'.vscode');await fs.mkdir(directory);const file=path.join(directory,'mcp.json');await fs.writeFile(file,JSON.stringify(config));
        for(const spec of specs)assert.equal(await f.commands.get('airk.httpCredential')!({connection_id:spec.http.connection_id}),'airk1.'+spec.http.connection_id+'.'+'a'.repeat(64));
        assert.deepEqual(await f.api.getServerDefinitions(),[]);
        config.servers.reused={...(Object.values(config.servers)[0] as Record<string,unknown>)};await fs.writeFile(file,JSON.stringify(config));
        await assert.rejects(f.commands.get('airk.httpCredential')!({connection_id:specs[0].http.connection_id}),/matching_workspace_configuration/);
    }finally{await f.close();}
});

test('a credential duplicated between native user and workspace files is ambiguous and cannot be released',async t=>{
    const f=await serviceTestHost(t);
    try{
        const controller=f.context.subscriptions.find((item:any)=>typeof item.resolveAgentSpec==='function'),spec=await controller.resolveAgentSpec(controller.getChildren()[0],'vscode');
        const directory=path.join(f.folders[0].uri.fsPath,'.vscode');await fs.mkdir(directory);
        const source=serializeServers('vscode',[unifiedEntry(spec)]),local=path.join(directory,'mcp.json'),user=path.join(f.root,'User','mcp.json');await fs.mkdir(path.dirname(user));await fs.writeFile(local,source);await fs.writeFile(user,source);
        t.mock.method(globalInventory,'inventoryGlobalAgentConfigs',async()=>({files:[{agent:'copilot',destination:user,source,sha256:'fixture',declarations:[]}],checked:[user],blockers:[]}));
        await assert.rejects(f.commands.get('airk.httpCredential')!({connection_id:spec.http.connection_id}),/matching_workspace_configuration/);
    }finally{await f.close();}
});

test('native user configuration changes during protected reads cannot release its credential',async t=>{
    const f=await serviceTestHost(t);
    try{
        const controller=f.context.subscriptions.find((item:any)=>typeof item.resolveAgentSpec==='function'),spec=await controller.resolveAgentSpec(controller.getChildren()[0],'vscode'),info=spec.http!;
        const user=path.join(f.root,'User','mcp.json');await fs.mkdir(path.dirname(user));const source=serializeServers('vscode',[unifiedEntry(spec)]);await fs.writeFile(user,source);
        t.mock.method(globalInventory,'inventoryGlobalAgentConfigs',async()=>({files:[{agent:'copilot',destination:user,source:await fs.readFile(user,'utf8'),sha256:'fixture',declarations:[]}],checked:[user],blockers:[]}));
        const nativeFs=require('node:fs/promises'),open=nativeFs.open;
        t.mock.method(nativeFs,'open',async(filename:string,...args:unknown[])=>{
            if(filename===info.credential_file){const changed=JSON.parse(source);changed.servers.airk.url='https://127.0.0.1:1/mcp';await fs.writeFile(user,JSON.stringify(changed));}
            return open(filename,...args);
        });
        await assert.rejects(f.commands.get('airk.httpCredential')!({connection_id:info.connection_id}),/configuration_changed/);
    }finally{await f.close();}
});

test('actual extension input command resolves only the native-bound workspace HTTP credential',async t=>{
    const f=await serviceTestHost(t);
    try{
        const controller=f.context.subscriptions.find((item:any)=>typeof item.resolveAgentSpec==='function');
        const spec=await controller.resolveAgentSpec(controller.getChildren()[0],'vscode'),info=spec.http!;
        const directory=path.join(f.folders[0].uri.fsPath,'.vscode'),file=path.join(directory,'mcp.json');await fs.mkdir(directory);
        const source=serializeServers('vscode',[unifiedEntry(spec)]);await fs.writeFile(file,source);
        assert.ok(source.includes('${input:airkToken_'+info.connection_id+'}'));assert.ok(!source.includes('airk1.'));assert.ok(!source.includes('credential_file'));
        const result=await f.commands.get('airk.httpCredential')!({connection_id:info.connection_id});
        assert.equal(result,'airk1.'+info.connection_id+'.'+'a'.repeat(64));assert.ok(!f.calls.join('\n').includes(String(result)));
        assert.deepEqual(await f.api.getServerDefinitions(),[],'a manual HTTP configuration owns its policy without a duplicate provider');
        const disabled=JSON.parse(source);disabled.servers.airk.disabled=true;await fs.writeFile(file,JSON.stringify(disabled));assert.deepEqual(await f.api.getServerDefinitions(),[],'disabled manual server is not bypassed');
        const migrated=migrateAgentConfig('copilot',spec,f.context.globalStorageUri.fsPath,JSON.stringify(disabled));assert.equal(JSON.parse(migrated).servers.airk.disabled,true);assert.equal(vscodeCredentialConfiguration(migrated,info),true);
        assert.equal(syncVscodeHttpInput(migrated,info),migrated,'an exact input is not rewritten');
    }finally{await f.close();}
});

test('credential resolution refuses foreign targets, reused inputs, malformed arguments and lost trust',async t=>{
    const f=await serviceTestHost(t);
    try{
        const controller=f.context.subscriptions.find((item:any)=>typeof item.resolveAgentSpec==='function'),spec=await controller.resolveAgentSpec(controller.getChildren()[0],'vscode'),info=spec.http!;
        const directory=path.join(f.folders[0].uri.fsPath,'.vscode'),file=path.join(directory,'mcp.json');await fs.mkdir(directory);
        const source=serializeServers('vscode',[unifiedEntry(spec)]),input={connection_id:info.connection_id};
        for(const mutate of [(value:any)=>value.servers.airk.url='https://127.0.0.1:1/mcp',(value:any)=>value.servers.foreign={type:'http',url:'https://foreign.invalid',headers:{Authorization:value.servers.airk.headers.Authorization}},(value:any)=>value.inputs[0].command='foreign.command',(value:any)=>value.inputs.push(value.inputs[0])]){
            const value=JSON.parse(source);mutate(value);await fs.writeFile(file,JSON.stringify(value));await assert.rejects(f.commands.get('airk.httpCredential')!(input),/matching_workspace_configuration/);
        }
        await fs.writeFile(file,source);
        await assert.rejects(f.commands.get('airk.httpCredential')!({...input,file:'/arbitrary/secret'}),/invalid_http_credential_input/);
        await assert.rejects(f.commands.get('airk.httpCredential')!({connection_id:'../private'}),/invalid_http_credential_input/);
        f.stub.workspace.isTrusted=false;await assert.rejects(f.commands.get('airk.httpCredential')!(input),/workspace_untrusted/);
        assert.equal(f.writes.length,0,'resolving a credential does not create or index a project');
    }finally{await f.close();}
});

test('configuration changes during protected token read cannot release a credential',async t=>{
    const f=await serviceTestHost(t);
    try{
        const controller=f.context.subscriptions.find((item:any)=>typeof item.resolveAgentSpec==='function'),spec=await controller.resolveAgentSpec(controller.getChildren()[0],'vscode'),info=spec.http!;
        const directory=path.join(f.folders[0].uri.fsPath,'.vscode'),file=path.join(directory,'mcp.json');await fs.mkdir(directory);
        const source=serializeServers('vscode',[unifiedEntry(spec)]);await fs.writeFile(file,source);
        const nativeFs=require('node:fs/promises'),open=nativeFs.open;
        t.mock.method(nativeFs,'open',async(filename:string,...args:unknown[])=>{
            if(filename===info.credential_file){const changed=JSON.parse(source);changed.servers.airk.url='https://127.0.0.1:1/mcp';await fs.writeFile(file,JSON.stringify(changed));}
            return open(filename,...args);
        });
        await assert.rejects(f.commands.get('airk.httpCredential')!({connection_id:info.connection_id}),/configuration_changed/);
    }finally{await f.close();}
});

test('imported Workbench evidence and interactive CLI retain their original native scope without a launcher',async t=>{
    const f=await serviceTestHost(t);
    try{
        const controller=f.context.subscriptions.find((item:any)=>typeof item.resolveAgentSpec==='function'),spec=controller.getChildren()[0];
        const prepared=await controller.explorerHost().prepareEvidenceConnection(spec,'airk-workbench');
        assert.equal(prepared.http.selection.kind,'imported_legacy');assert.equal(f.state.httpConnections.at(-1)!.client,'airk-workbench');assert.deepEqual(prepared.http.selection.selection.registration,{binding:spec.service.project,root:spec.root,data_root:spec.dataDir,collection:spec.collection});
        assert.equal(prepared.command,f.binding.executable);const args=launchArgs(prepared,'tools');assert.deepEqual(args.slice(0,2),['--service-install-dir',f.binding.installDir]);assert.ok(!args.includes('runtime')&&!args.includes('launch'));assert.equal(args[args.indexOf('--service-project')+1],spec.service.project);
        f.state.legacyBindingReply=project=>({status:'catalog_legacy_binding',observation:{schema:1,namespace:'34'.repeat(16),catalog_revision:null,project,eligibility:{kind:'authority_zero',registration:{binding:spec.service.project,root:spec.root,data_root:spec.dataDir,collection:spec.collection}}}});
        await assert.rejects(controller.explorerHost().prepareEvidenceConnection(spec),/legacy_selection_review_required/,'saved preferences cannot manufacture an imported grant');assert.equal(f.writes.length,0);
    }finally{await f.close();}
});

test('external terminal credential values disable persistence before either environment mutation',async t=>{
    const f=await serviceTestHost(t);
    try{
        const collection=f.context.environmentVariableCollection;collection.persistent=true;const names:string[]=[];
        collection.replace=(name:string,_value:string)=>{assert.equal(collection.persistent,false,'credential/CA provisioning must be ephemeral before publishing');names.push(name);};
        const controller=f.context.subscriptions.find((item:any)=>typeof item.resolveAgentSpec==='function');await controller.resolveAgentSpec(controller.getChildren()[0],'codex');
        assert.ok(names.includes('CODEX_CA_CERTIFICATE'));assert.ok(names.some(name=>name.startsWith('AIRK_MCP_TOKEN_')));assert.equal(collection.persistent,false);
    }finally{await f.close();}
});

test('Explorer, Workbench and Copilot enrollment retain distinct immutable native usage families',async t=>{
    const f=await serviceTestHost(t);
    try{
        const controller=f.context.subscriptions.find((item:any)=>typeof item.resolveAgentSpec==='function'),spec=controller.getChildren()[0],host=controller.explorerHost();
        const explorer=await host.prepareEvidenceConnection(spec),workbench=await host.prepareEvidenceConnection(spec,'airk-workbench'),copilot=await controller.resolveAgentSpec(spec,'vscode');
        assert.deepEqual(f.state.httpConnections.slice(-3).map(item=>item.client),['airk-explorer','airk-workbench','vscode']);
        assert.equal(new Set([explorer.http.connection_id,workbench.http.connection_id,copilot.http.connection_id]).size,3,'credential identity must never cross attribution families');
        const again=await host.prepareEvidenceConnection(spec,'airk-explorer');assert.equal(again.http.connection_id,explorer.http.connection_id);
        assert.deepEqual(explorer.http.selection,workbench.http.selection);assert.deepEqual(workbench.http.selection,copilot.http.selection);
    }finally{await f.close();}
});

test('issuer rotation requires the exact human review and retains the complete native abort receipt',async t=>{
    const f=await serviceTestHost(t);
    try{
        const controller=f.context.subscriptions.find((item:any)=>typeof item.resolveAgentSpec==='function'),runtime=controller.runtime(),actions:string[]=[];
        const info=(await controller.resolveAgentSpec(controller.getChildren()[0],'airk-explorer')).http;
        const status={schema:1,current_endpoint:info.endpoint,current_descriptor_sha256:'11'.repeat(32),receipt_sha256:'22'.repeat(32),finalize_overdue:false,guidance:'Native rotation fixture',future_native:{retained:true},rotation:{schema:1,operation_id:'33'.repeat(16),phase:'prepared',revision:1,installation:info.endpoint.installation,user:info.endpoint.user,namespace:info.endpoint.namespace,old_ca_sha256:info.endpoint.ca_sha256,new_ca_sha256:'44'.repeat(32),old_ca_certificate_file:info.ca_certificate_file,new_ca_certificate_file:path.join(f.binding.controlDir,'http-1','staged-new.pem'),old_endpoint:info.endpoint,new_endpoint:null,overlap_seconds:86400,overlap_expires_unix_ms:null}};
        t.mock.method(runtime,'httpIssuerRotation',async(_binding:unknown,action:string,expected:any)=>{actions.push(action);if(action==='status')return status;assert.equal(action,'abort');assert.equal(expected,status);return {...status,rotation:{...status.rotation,phase:'aborted',revision:2}};});
        f.state.pick=items=>items.find((item:any)=>item.action==='abort');
        f.state.confirm=false;assert.equal(await f.commands.get('airk.rotateHttpIssuer')!(),undefined);assert.deepEqual(actions,['status']);
        f.state.confirm=true;const result:any=await f.commands.get('airk.rotateHttpIssuer')!();assert.deepEqual(actions,['status','status','abort']);assert.equal(result.rotation.phase,'aborted');assert.deepEqual(result.future_native,{retained:true});
        assert.match(f.warnings.at(-1)!,/Abort only the prepared/);assert.match(f.warnings.at(-1)!,new RegExp(status.rotation.operation_id));
        assert.deepEqual(JSON.parse(f.openedDocuments.at(-1)!.content).future_native,{retained:true});assert.equal(f.writes.length,0,'rotation UI never indexes or widens the native selection');
    }finally{await f.close();}
});
