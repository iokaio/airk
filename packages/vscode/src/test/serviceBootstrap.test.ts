// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { automaticUpgrade, NativeRunner, RuntimeHost, RuntimeOptions, validateRuntimeInstallDirectory, verifyBundledRuntime, nativeRunner } from '../serviceBootstrap';
import { RecordValue, ServiceClient, ServiceError, record } from '../serviceClient';
import { nativePayloadDirectory } from './nativeServiceFixture';
import { LegacyRetirementSession } from '../runtimeLegacyRetirement';
import { httpAuthorityDigest } from '../serviceHttp';
import { ConsoleHost, ServiceConsoleController } from '../serviceConsoleController';
import { managementView, panelDetails } from '../explorerManagement';
import { diagnosticPreview } from '../serviceConsoleView';
import {predecessorHttpFixture} from './fixtures/predecessorHttpFixture';
const sha=(bytes:Buffer|string)=>createHash('sha256').update(bytes).digest('hex');

test('predecessor command inspection requests the disposal purpose without weakening ordinary inspection',async()=>{
    const f=await fixture(),install=path.join(f.root,'install'),calls:string[][]=[];
    const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable'},async(_command,args)=>{
        if(args[1]==='self-test')return {protocol:1,version:'1.4.0',self_test:'passed',metadata_console:1};
        calls.push([...args]);return {schema:1,active:f.digest,launcher:null,universal_launcher:null};
    });
    const binding={installDir:install,controlDir:path.join(install,'control'),executable:f.source,launcher:null,runtimeSha256:f.digest,version:'1.8.11',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    try{
        await (host as any).inspectLegacyLaunchers(binding,'predecessor-cleanup');
        assert.deepEqual(calls[0],['runtime','inspect-legacy-launchers','--install-dir',install,'--expected-active',f.digest,'--purpose','predecessor-cleanup']);
        await host.inspectLegacyLaunchers(binding);assert.deepEqual(calls[1],['runtime','inspect-legacy-launchers','--install-dir',install,'--expected-active',f.digest]);
    }finally{await f.cleanup();}
});

test('predecessor disposal lookup uses its metadata purpose before the baseline and retains the complete snapshot',async()=>{
    const f=await fixture(),install=path.join(f.root,'install'),control=path.join(install,'old-control'),id='29'.repeat(16),reply=predecessorHttpFixture(control,f.digest,id);let queried=false;
    const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable'},async(_command,args)=>{
        if(args[1]==='self-test')return {protocol:1,version:'1.4.0',self_test:'passed',metadata_console:1};
        assert.deepEqual(args,['runtime','lookup-http-connection','--install-dir',install,'--expected-active',f.digest,'--connection-id',id,'--purpose','predecessor-cleanup','--predecessor',f.digest]);queried=true;return reply;
    });
    const binding={installDir:install,controlDir:control,executable:f.source,launcher:null,runtimeSha256:f.digest,version:'1.8.11',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    try{
        const observed=await (host as any).lookupPredecessorHttpOwnership(binding,id,{predecessor:f.digest,controlDir:control,namespace:reply.namespace});
        assert.equal(queried,true);assert.deepEqual(observed,reply);assert.equal(observed.ownership,'expired');assert.equal(observed.qualification.live_authority,false);assert.equal('connection' in observed,false);
    }finally{await f.cleanup();}
});

test('predecessor disposal lookup rejects crossed namespace, control, predecessor and saved operation scope',async()=>{
    const f=await fixture(),install=path.join(f.root,'install'),control=path.join(install,'old-control'),id='29'.repeat(16),original=predecessorHttpFixture(control,'ab'.repeat(32),id);original.active=f.digest;original.operation_id='cd'.repeat(16);original.qualification.source='saved_predecessor_snapshot';original.qualification.current_namespace='78'.repeat(16);original.qualification.current_namespace_ownership='not_owned';let reply:any=original;
    const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable'},async(_command,args)=>args[1]==='self-test'?{protocol:1,version:'1.4.0',self_test:'passed',metadata_console:1}:reply);
    const binding={installDir:install,controlDir:path.join(install,'fresh-control'),executable:f.source,launcher:null,runtimeSha256:f.digest,version:'1.8.16',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    const expected={predecessor:original.predecessor,controlDir:control,namespace:original.namespace,operationId:original.operation_id};
    try{
        assert.deepEqual(await (host as any).lookupPredecessorHttpOwnership(binding,id,expected),original);
        for(const change of [(value:any)=>value.predecessor='ef'.repeat(32),(value:any)=>value.control_dir=path.join(install,'foreign-control'),(value:any)=>value.namespace='ef'.repeat(16),(value:any)=>value.operation_id='ef'.repeat(16),(value:any)=>value.snapshot.complete=false]){
            reply=structuredClone(original);change(reply);await assert.rejects((host as any).lookupPredecessorHttpOwnership(binding,id,expected),/predecessor_http_ownership_unconfirmed/);
        }
    }finally{await f.cleanup();}
});

test('installation retires previous native runtime with the exact current selection and validates its receipt',async()=>{
    const f=await fixture(),install=path.join(f.root,'install');let crossed=false;
    const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable'},async(_command,args)=>{
        if(args[1]==='self-test')return {protocol:1,version:'1.4.0',self_test:'passed',metadata_console:1};
        assert.deepEqual(args,['runtime','retire-previous','--install-dir',install,'--expected-active',f.digest]);
        return {active:crossed?'ab'.repeat(32):f.digest,retired_previous:'cd'.repeat(32)};
    });
    const binding={installDir:install,controlDir:path.join(install,'control'),executable:f.source,launcher:null,
        runtimeSha256:f.digest,version:'1.4.0',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    try{
        await host.retirePrevious(binding);crossed=true;
        await assert.rejects(host.retirePrevious(binding),/crossed_runtime_retirement_receipt/);
    }finally{await f.cleanup();}
});

test('legacy launcher finalization retains exact native process audit and rejects incomplete or crossed absence',async()=>{
    const f=await fixture(),install=path.join(f.root,'install');let receipt:any;
    const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable'},async(_command,args)=>{
        if(args[1]==='self-test')return {protocol:1,version:'1.4.0',self_test:'passed',metadata_console:1};
        assert.deepEqual(args,['runtime','retire-legacy-launchers','--install-dir',install,'--expected-active',f.digest]);return receipt;
    });
    const binding={installDir:install,controlDir:path.join(install,'control'),executable:f.source,launcher:null,runtimeSha256:f.digest,version:'1.4.0',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    const valid={schema:1,active:f.digest,legacy_launchers_retired:true,launcher:null,universal_launcher:null,processes:{scope:'verified_launcher_images',remaining:0,observations:[{complete:false,unknown_system_processes:3,owned_images_stopped:[7]}]},future:{retained:true}};
    try{receipt=valid;assert.deepEqual(await host.retireLegacyLaunchers(binding),valid);
        for(const mutate of [(v:any)=>v.active='a'.repeat(64),(v:any)=>v.legacy_launchers_retired=false,(v:any)=>v.launcher=f.source,(v:any)=>v.universal_launcher=f.source,(v:any)=>v.processes.remaining=1,(v:any)=>v.processes.scope='all_processes',(v:any)=>v.processes.observations=null,(v:any)=>{delete v.processes;}]){
            receipt=structuredClone(valid);mutate(receipt);await assert.rejects(host.retireLegacyLaunchers(binding),(error:unknown)=>error instanceof ServiceError&&error.outcomeUnknown&&error.nativeResponse===receipt);
        }
    }finally{await f.cleanup();}
});

test('native cache preparation freezes real cleanup scope and retirement requires the same complete accepted proof',async()=>{
    const f=await fixture(),install=path.join(f.root,'custom-install'),context=path.join(f.root,'Code','User','globalStorage'),locator=path.join(f.root,'protected-locators.json');
    const binding={installDir:install,controlDir:path.join(install,'control'),executable:f.source,launcher:null,runtimeSha256:f.digest,version:'1.4.0',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution',payloadVerified:true};
    const cacheDirectory=path.join(context,'iokaio.airk','helpers','0.2.1-windows-x64-'+f.digest.slice(0,16));
    const cache={schema:1,provenance:'owned_legacy_helper_cache',user:'fixture',global_storage:context,global_storage_identity:'global-native-id',helpers_identity:'helpers-native-id',cache_directory:cacheDirectory,cache_directory_identity:'cache-native-id',executable:path.join(cacheDirectory,'airk.exe'),executable_sha256:f.digest,executable_identity:'file-native-id',cache_version:'0.2.1',platform:'windows-x64',digest_prefix:f.digest.slice(0,16),artifact_approved:false,future:{retained:true}};
    const original={schema:1,active:f.digest,scope:'context-only',context_global_storage:context,global_storage_roots:[context],caches:[cache],inspection_sha256:'ef'.repeat(32),origin_proofs:[],origin_locators:[],adoption_locators:[],managed_locators:[],origin_classifications:[],future_origin_evidence:{full_original_config_sha256:'12'.repeat(32)},future:{retained:true}};
    let prepared:any=original,retired:any;
    const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable',installDir:install},async(_command,args)=>{
        if(args[1]==='self-test')return {protocol:1,version:'1.4.0',self_test:'passed',metadata_console:1};
        if(args[1]==='prepare-legacy-caches'){
            assert.deepEqual(args,['runtime','prepare-legacy-caches','--install-dir',install,'--expected-active',f.digest,'--global-storage',context,'--origin-locators-file',locator]);
            assert.equal(args.includes('--scope'),false,'Only native OS installation identity decides default versus custom scope');return prepared;
        }
        assert.deepEqual(args,['runtime','retire-legacy-caches','--install-dir',install,'--expected-active',f.digest,'--expected-inspection-sha256',original.inspection_sha256]);return retired;
    });
    const complete={schema:1,active:f.digest,scope:original.scope,global_storage_roots:original.global_storage_roots,caches_retired:true,remaining:0,inspection_sha256:original.inspection_sha256,retired_caches:original.caches,processes:{scope:'verified_legacy_cache_images',remaining:0,observations:[{complete:false,unknown_system_pids:3,owned_image_removed:true}],image_roster:[{original:'full native roster'}],image_exclusions:[{original:'held image proof'}]},future:{retained:true}};
    try{
        assert.equal(await host.prepareLegacyCaches(binding,context,locator),original);
        retired=complete;assert.equal(await host.retireLegacyCaches(binding,original),complete);
        for(const change of [(v:any)=>v.active='00'.repeat(32),(v:any)=>v.scope='guessed-global',(v:any)=>v.context_global_storage=install,(v:any)=>v.global_storage_roots=[],(v:any)=>v.caches[0].executable_sha256='00'.repeat(32),(v:any)=>delete v.caches[0].executable_identity]){
            prepared=structuredClone(original);change(prepared);await assert.rejects(host.prepareLegacyCaches(binding,context,locator),(error:unknown)=>error instanceof ServiceError&&error.outcomeUnknown&&error.nativeResponse===prepared);
        }
        for(const change of [(v:any)=>v.active='00'.repeat(32),(v:any)=>v.scope='native-defaults',(v:any)=>v.global_storage_roots.push(install),(v:any)=>v.inspection_sha256='00'.repeat(32),(v:any)=>v.caches_retired=false,(v:any)=>v.remaining=1,(v:any)=>v.processes.scope='all_processes',(v:any)=>v.processes.remaining=1,(v:any)=>delete v.processes.image_exclusions,(v:any)=>v.retired_caches=[]]){
            retired=structuredClone(complete);change(retired);await assert.rejects(host.retireLegacyCaches(binding,original),(error:unknown)=>error instanceof ServiceError&&error.outcomeUnknown&&error.nativeResponse===retired);
        }
        // Origin classification rules (formerly covered only through adoption):
        // equivalent literal locators share one complete native proof, a
        // native-origin SHA cannot collapse different observations, and an
        // explicit nested container never becomes a wildcard.
        const originLocator={path:path.join(f.root,'.mcp.json'),format:'mcp_servers_json' as const,server:'old-1'},explicit={...originLocator,container_path:['mcpServers']},origin='ab'.repeat(32);
        const proof={config_path:originLocator.path,server:originLocator.server,container_path:['mcpServers'],config_sha256:origin,executable:cache.executable,executable_sha256:f.digest,selected_entry_sha256:origin,future_native_proof:{retained:true}};
        const withOrigins:any={...structuredClone(original),origin_locators:[originLocator,explicit],origin_proofs:[proof],adoption_locators:[originLocator],managed_locators:[],
            origin_classifications:[originLocator,explicit].map(item=>({locator:item,origin_sha256:origin,kind:'standalone',authority:'not_observed'}))};
        prepared=withOrigins;assert.equal(await host.prepareLegacyCaches(binding,context,locator),withOrigins);
        const crossed=structuredClone(withOrigins);crossed.origin_locators[1]={...crossed.origin_locators[1],server:'different-server'};crossed.origin_classifications[1].locator=crossed.origin_locators[1];crossed.origin_proofs.push({...proof,server:'different-server'});
        const nested=structuredClone(withOrigins);nested.origin_locators[1]={...nested.origin_locators[1],container_path:['projects','different','mcpServers']};nested.origin_classifications[1].locator=nested.origin_locators[1];
        for(const changed of [crossed,nested]){
            prepared=changed;await assert.rejects(host.prepareLegacyCaches(binding,context,locator),(error:unknown)=>error instanceof ServiceError&&error.code==='legacy_cache_inspection_unconfirmed'&&error.outcomeUnknown&&error.nativeResponse===changed);
        }
        prepared={...original,scope:'native-defaults'};assert.equal((await host.prepareLegacyCaches(binding,context,locator)).scope,'native-defaults','Frontend never infers scope from HOME or a mock custom path');
    }finally{await f.cleanup();}
});

test('legacy HTTP enrollment accepts only native standalone original-to-adopted retargets and preserves full replies',async()=>{
    const f=await fixture(),install=path.join(f.root,'install'),namespace='ab'.repeat(16),id='cd'.repeat(16),receipt='ef'.repeat(32);
    const binding={installDir:install,controlDir:path.join(install,'control'),executable:f.source,launcher:null,runtimeSha256:f.digest,version:'1.4.0',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    const member=(number:number)=>{
        const suffix=String(number),original={binding:(number+10).toString(16).padStart(64,'0'),root:path.join(f.root,'source-'+suffix),data_root:path.join(f.root,'old-data-'+suffix),collection:'original-'+suffix},adopted={...original,binding:(number+20).toString(16).padStart(64,'0'),data_root:path.join(f.root,'adopted-data-'+suffix)};
        const setup={caller:id,key:{namespace,epoch:1,nonce:id},expected_catalog_revision:1,name:'Original '+suffix,registration:adopted,sources:[{canonical_path:original.root,aliases:[],owner:true}],setup_watch:false};
        const plan={schema:1,setup,original_registration:original,origin_sha256:receipt,config_sha256:receipt,executable_sha256:f.digest,quiescence_sha256:receipt,archive_inventory_sha256:receipt,original_publication_sha256:receipt,source_inventory_sha256:receipt};
        const adoption={schema:1,operation_id:id,plan,publication:{schema:1,plan_digest:receipt,archive_inventory_sha256:receipt,original_publication_sha256:receipt,current:`airk-r1:${namespace}:2:${receipt}`,resource_sha256:receipt,compatibility_sha256:receipt},original_server:'old-'+suffix,no_default_project:true,configuration_claims:[{schema:1,origin_sha256:plan.origin_sha256,config_sha256:plan.config_sha256,executable_sha256:plan.executable_sha256,selected_entry_sha256:receipt,container_path:['mcpServers'],original_server:'old-'+suffix,no_default_project:true}]};
        const selected={namespace,expected_catalog_revision:2,registration:adopted,configuration_receipt:receipt,grant:{grant:{id:'grt_'+'91'.repeat(16),host:receipt,revision:1,revoked:false,memberships:[{membership:{project_id:'prj_'+'11'.repeat(16),index_id:'idx_'+'22'.repeat(16),revision:1,scope:[{source_root_id:'src_'+'33'.repeat(16),kind:'subtree',path:''}]},source_revision:1,policy_epoch:0,policy_digest:null}]},provenance:{kind:'imported_standalone_adoption',adoption},future_native_field:{retained:true}}};
        return {original,adopted,selected};
    };
    const first=member(1),second=member(2),directory=path.join(binding.controlDir,'http-1');
    const base={schema:1,connection_id:id,endpoint:{schema:1,installation:f.digest,user:'fixture',namespace,generation:1,service_instance:id,executable_sha256:f.digest,url:'https://127.0.0.1:57421/mcp',ca_sha256:receipt,certificate_sha256:receipt,state:'ready',predecessor_digest:null},ca_certificate_file:path.join(directory,'ca.pem'),credential_file:path.join(directory,'credential-'+id+'.token')};
    const singleton={...base,selection:{kind:'imported_legacy',selection:first.selected}},union={...base,selection:{kind:'imported_legacy_union',selection:{namespace,selections:[first.selected,second.selected],no_default_project:true}}};
    let response:any=singleton,observedArgs:readonly string[]=[];
    const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable'},async(_command,args)=>{
        if(args[1]==='self-test')return {protocol:1,version:'1.4.0',self_test:'passed',metadata_console:1};
        assert.ok(['http-legacy-connection','http-legacy-union-connection'].includes(args[1]));observedArgs=args;return response;
    });
    const file=path.join(f.root,'protected-original-registration.json');
    try{
        assert.deepEqual(await host.httpLegacyConnection(binding,first.original,file,receipt,id,'claude'),singleton);
        const attachment=path.join(f.root,'explorer-assistance','windows','a'.repeat(32)+'.json'),oldId='f'.repeat(32);
        assert.deepEqual(await host.httpLegacyConnection(binding,first.original,file,receipt,id,'claude',attachment,receipt,oldId),singleton);
        assert.deepEqual(observedArgs.slice(-4),['--explorer-descriptor',attachment,'--reattach-connection-id',oldId],'Native mutation receives both the live replacement attachment and the exact retirement identity');
        await assert.rejects(host.httpLegacyConnection(binding,{...first.original,data_root:install},file,receipt,id,'claude'),(error:unknown)=>error instanceof ServiceError&&error.code==='crossed_http_legacy_connection'&&error.nativeResponse===response);
        response=union;assert.deepEqual(await host.httpLegacyUnionConnection(binding,[{registration:first.original,configuration_receipt:receipt},{registration:second.original,configuration_receipt:receipt}],file,id,'vscode'),union);
        await assert.rejects(host.httpLegacyUnionConnection(binding,[{registration:second.original,configuration_receipt:receipt},{registration:first.original,configuration_receipt:receipt}],file,id,'vscode'),/crossed_http_legacy_union/);
        response=structuredClone(singleton);response.selection.selection.grant.provenance={kind:'imported_legacy_snapshot',upgrade_id:id,plan_digest:receipt,legacy_binding:first.adopted.binding,configuration_digest:receipt};
        await assert.rejects(host.httpLegacyConnection(binding,first.original,file,receipt,id,'claude'),/crossed_http_legacy_connection/);
        response=structuredClone(singleton);response.selection.selection.grant.provenance.adoption.publication.archive_inventory_sha256=f.digest;
        await assert.rejects(host.httpLegacyConnection(binding,first.original,file,receipt,id,'claude'),(error:unknown)=>error instanceof ServiceError&&error.outcomeUnknown&&error.nativeResponse===response);
    }finally{await f.cleanup();}
});

test('native qualification locates pair metadata outside the exact Mac app layout',()=>{
    const root=path.resolve('native-payload-fixture');
    assert.equal(nativePayloadDirectory(path.join(root,'airk.exe')),root);
    assert.equal(nativePayloadDirectory(path.join(root,'AIRK.app','Contents','MacOS','airk')),root);
    const unknown=path.join(root,'Other.app','Contents','MacOS','airk');
    assert.equal(nativePayloadDirectory(unknown),path.dirname(unknown));
});
test('reviewed project setup reserves exact native identity before creating storage and refuses copied reviews',async()=>{
    const f=await fixture(),calls:string[][]=[],mutations:unknown[]=[];
    const registration={binding:'ab'.repeat(32),root:path.join(f.root,'source'),data_root:path.join(f.root,'new','data'),collection:'fixture'};
    let authorized=true,loseTrust=false;
    const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable'},async(command,args)=>{
        assert.equal(command,path.join(f.source,f.main));calls.push(args);
        if(args.includes('prepare-registration'))return registration;
        assert.equal(args[1],'prepare-storage');assert.ok(mutations.length);return registration;
    },()=>authorized);
    host.client=()=>new ServiceClient({async request(method){mutations.push(method);if(loseTrust)authorized=false;
        return {status:'registered',project:{registration,fencing_generation:0,paused:false}};},dispose(){}});
    const binding={installDir:f.root,controlDir:path.join(f.root,'control'),executable:path.join(f.root,'selected-main'),launcher:null,
        runtimeSha256:f.digest,version:'1.4.0',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    try{
        const review=await host.reviewProject(binding,registration.root,registration.data_root,'fixture');assert.ok(Object.isFrozen(review));
        await assert.rejects(fs.stat(registration.data_root),{code:'ENOENT'});
        await assert.rejects(host.registerProject(binding,{...review}),/project_registration_not_reviewed/);assert.equal(mutations.length,0);
        await host.registerProject(binding,review);
        assert.deepEqual(mutations,[{reserve_project:{project:registration}}]);
        assert.deepEqual(calls.at(-1),['service','prepare-storage','--runtime-dir',binding.controlDir,'--project',registration.binding]);
        await assert.rejects(host.registerProject(binding,review),/project_registration_not_reviewed/);
        const next=await host.reviewProject(binding,registration.root,registration.data_root,'fixture');const before=calls.length;loseTrust=true;
        await assert.rejects(host.registerProject(binding,next),/runtime_client_no_longer_authorized/);assert.equal(calls.length,before);
    }finally{await f.cleanup();}
});
test('configuration preparation uses bounded stdin and the verified bundled command without launching the selected image',async()=>{
    const f=await fixture(),calls:string[][]=[];const project='12'.repeat(32);
    const request={incarnation:'34'.repeat(16),settings:'56'.repeat(32),configuration:{operation:'vectors',enabled:false}};
    const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable'},async(command,args,_timeout,input)=>{
        assert.equal(command,path.join(f.source,f.main));assert.deepEqual(JSON.parse(input!),{operation:'vectors',enabled:false});calls.push(args);return request;
    });
    const binding={installDir:f.root,controlDir:path.join(f.root,'control'),executable:path.join(f.root,'selected-main'),launcher:null,
        runtimeSha256:f.digest,version:'1.4.0',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    try{
        assert.deepEqual(await host.prepareConfiguration(binding,project,{operation:'vectors',enabled:false}),request);
        assert.deepEqual(calls[0],['service','prepare-configuration','--runtime-dir',binding.controlDir,'--project',project]);
        await assert.rejects(host.prepareConfiguration(binding,project,{operation:'vectors',text:'x'.repeat(20000)}),/configuration_draft_exceeds_budget/);
        assert.equal(calls.length,1);
    }finally{await f.cleanup();}
});
test('legacy startup review executes only the verified new main and binds exact targets, approvals and runtime scope',async()=>{
    const f=await fixture(),calls:string[][]=[];let authorized=true,crossed=false;
    const target={root:path.join(f.root,'old-source'),data_dir:path.join(f.root,'old-data'),collection:'old-fixture',executable:path.join(f.root,'old',f.main)};
    await fs.mkdir(target.root);await fs.mkdir(target.data_dir);await fs.mkdir(path.dirname(target.executable),{recursive:true});await fs.writeFile(target.executable,'Old command is data only.');
    const canonical={...target,root:await fs.realpath(target.root),data_dir:await fs.realpath(target.data_dir),executable:await fs.realpath(target.executable)};
    const binding={installDir:f.root,controlDir:path.join(f.root,'control'),executable:path.join(f.root,'selected-main'),launcher:null,
        runtimeSha256:f.digest,version:'1.4.0',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    const approval='ab'.repeat(32);
    const value=()=>({preview:{version:1,namespace:'cd'.repeat(16),epoch:1,removal_available:true,spec:{id:'airk-'+'ef'.repeat(16),target:{...canonical,collection:crossed?'other':canonical.collection},definition:path.join(f.root,'old-startup.xml'),receipt:path.join(f.root,'old-owner.json')},observation:{owned:true,loaded:true,running:false,definition_matches:true}},approval,
        workers:{matching_executable_pids:[],complete:false,scanned:3,unobserved:1,failure:null}});
    const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable'},async(command,args)=>{
        assert.equal(command,path.join(f.source,f.main));assert.notEqual(command,target.executable);calls.push([...args]);
        if(args[2]==='review')return value();
        if(args[2]==='list')return {namespace:'cd'.repeat(16),epoch:1,items:[{approval,service_id:'airk-'+'ef'.repeat(16),phase:'definition_removed'}]};
        if(args[2]==='show')return {version:1,review:value(),phase:'definition_removed'};
        assert.equal(args[2],'remove-startup');return {approval,startup_removed:true,sources_preserved:true,project_settings_preserved:true,indexes_retained:true};
    },()=>authorized);
    try{
        const review=await host.reviewLegacyStartup(binding,target);assert.ok(Object.isFrozen(review)&&Object.isFrozen(review.target));
        assert.equal(calls.length,1);assert.ok(!calls[0].includes('remove-startup'));
        await assert.rejects(host.removeLegacyStartup(binding,{...review}),/legacy_startup_not_reviewed/);
        await assert.rejects(host.removeLegacyStartup({...binding,controlDir:path.join(f.root,'other-control')},review),/legacy_startup_not_reviewed/);assert.equal(calls.length,1);
        await host.removeLegacyStartup(binding,review);assert.equal(calls[1][calls[1].indexOf('--approval')+1],approval);
        await assert.rejects(host.removeLegacyStartup(binding,review),/legacy_startup_not_reviewed/);
        const receipts=await host.legacyStartupReceipts(binding);assert.equal((receipts.items as any[]).length,1);
        const resumed=await host.loadLegacyStartup(binding,approval);assert.equal(resumed.phase,'definition_removed');
        const before=calls.length;authorized=false;await assert.rejects(host.removeLegacyStartup(binding,resumed),/runtime_client_no_longer_authorized/);assert.equal(calls.length,before);
        authorized=true;crossed=true;await assert.rejects(host.reviewLegacyStartup(binding,target),/crossed_legacy_review/);
        assert.equal(await fs.readFile(target.executable,'utf8'),'Old command is data only.');
    }finally{await f.cleanup();}
});

async function fixture(host:{platform:string;arch:string}={platform:process.platform,arch:process.arch},version='1.4.0'){
    const root=await fs.mkdtemp(path.join(os.tmpdir(),'airk-runtime-bootstrap-'));
    const source=path.join(root,'native','runtime');await fs.mkdir(source,{recursive:true});
    const prefix=host.platform==='darwin'?'AIRK.app/Contents/MacOS/':'';
    const main=prefix+(host.platform==='win32'?'airk.exe':'airk'),helper=prefix+(host.platform==='win32'?'airk-embed.exe':'airk-embed');
    const files:Record<string,string>={};
    for(const [name,body]of [[main,'Original native main fixture.'],[helper,'Original native inference fixture.'],['LICENSE','Original fixture license notice.']]){
        await fs.mkdir(path.dirname(path.join(source,name)),{recursive:true});await fs.writeFile(path.join(source,name),body);files[name]=sha(body);
    }
    const inventory=Buffer.from(JSON.stringify({schema:1,version,os:host.platform==='win32'?'windows':host.platform==='darwin'?'macos':'linux',
        arch:host.arch==='arm64'?'aarch64':'x86_64',files}));
    const digest=sha(inventory);await fs.writeFile(path.join(source,'airk-install.json'),inventory);
    const wrapper={schema:2,version,vsix_target:`${host.platform}-${host.arch}`,runtime_dir:'runtime',install_manifest_sha256:digest,payload:{[main]:files[main],[helper]:files[helper]}};
    await fs.writeFile(path.join(root,'native','manifest.json'),JSON.stringify(wrapper));
    async function cleanup(){const target=path.resolve(root),base=path.resolve(os.tmpdir());assert.equal(path.dirname(target),base);assert.ok(path.basename(target).startsWith('airk-runtime-bootstrap-'));await fs.rm(target,{recursive:true,force:true});}
    return {root,source,main,helper,digest,wrapper,cleanup};
}

test('Mac bundle verification resolves the same two native roles and refuses an extra executable',async()=>{
    for(const arch of ['arm64','x64']){
        const host={platform:'darwin',arch},f=await fixture(host);
        try{
            const bundle=await verifyBundledRuntime(f.root,'1.4.0',host);
            assert.equal(bundle.main,path.join(f.source,'AIRK.app','Contents','MacOS','airk'));
            assert.equal(bundle.helper,path.join(f.source,'AIRK.app','Contents','MacOS','airk-embed'));
            await fs.writeFile(path.join(f.source,'airk'),'Unexpected third native executable.');
            await assert.rejects(verifyBundledRuntime(f.root,'1.4.0',host),/unknown_runtime_payload_file/);
        }finally{await f.cleanup();}
    }
});

async function transitionFixture(version='1.4.0',diagnosis:RecordValue={}){
    const [major,minor,patch]=version.split('.').map(Number),modern=major>1||major===1&&(minor>8||minor===8&&patch>=14);
    const f=modern?await modernBindingFixture(version):await fixture(undefined,version),install=path.join(f.root,'install');await fs.mkdir(install,{mode:0o700});
    await fs.mkdir(path.join(install,'control'));
    const calls:string[][]=[];let active='ab'.repeat(32),pending:RecordValue|null=null,candidateVersion=version,selectedVersion=version,cleanBreakPolicy:RecordValue|undefined;
    const inspection=()=>({selection:{active,previous:'cd'.repeat(32)},executable:path.join(install,'runtimes',active,f.main),launcher:path.join(install,'airk-launcher'),
        control_dir:path.join(install,'control'),managed_policy:null,approval:{version:selectedVersion,manifest_sha256:active,verification_basis:'trusted_distribution'},...(cleanBreakPolicy?{clean_break:cleanBreakPolicy}:{})});
    const run:NativeRunner=async(command,args)=>{
        assert.equal(command,path.join(f.source,f.main));calls.push([...args]);
        switch(args[1]){
            case 'self-test':return {protocol:1,version,self_test:'passed',metadata_console:1};
            case 'location':return {protocol:1,install_dir:install};
            case 'inspect':return inspection();
            case 'diagnose':return {...diagnosis,pending_activation:pending};
            case 'inspect-candidate':{const target=args[args.indexOf('--runtime-sha256')+1];return {runtime_sha256:target,metadata_only:true,selection_changed:false,executed:false,
                approval:{version:candidateVersion,manifest_sha256:target,verification_basis:'trusted_distribution'}};}
            case 'stage':return {};
            case 'abort-activation':{
                assert.equal(args[args.indexOf('--expected-candidate')+1],pending?.candidate);
                assert.equal(args[args.indexOf('--expected-active')+1],active);
                pending=null;return {activation_aborted:true,payloads_preserved:true};
            }
            case 'activate':{
                const expected=args[args.indexOf('--expected-active')+1],target=args[args.indexOf('--runtime-sha256')+1];
                if(active!==expected&&!(pending?.candidate===target&&record(pending.prior).active===expected))throw new ServiceError('runtime_selection_changed_since_review');
                active=target;return {schema:1,active,previous:expected};
            }
            default:throw new Error('Unexpected transition action');
        }
    };
    const host=new RuntimeHost(f.root,version,{channel:'stable',installDir:install},run);
    return {f,host,calls,install,binding:()=>host.binding(inspection(),install),setActive:(value:string)=>{active=value;},
        setPending:(value:RecordValue)=>{pending=value;},setCandidateVersion:(value:string)=>{candidateVersion=value;},setSelectedVersion:(value:string)=>{selectedVersion=value;},setCleanBreakPolicy:(value:RecordValue)=>{cleanBreakPolicy=value;}};
}

test('native clean break policy forces the current bundle through a prior pending activation without a migration option',async()=>{
    const t=await transitionFixture('1.8.16');
    try{
        t.setSelectedVersion('1.8.11');t.setCleanBreakPolicy({required:true,baseline_version:'1.8.16',state_contract:'airk-state/1',installed_contract:null,future_policy:{retained:true}});
        const binding=t.binding();assert.equal(binding.cleanBreak?.required,true);
        t.setPending({candidate:'ef'.repeat(32),phase:'drain',prior:{active:binding.runtimeSha256}});
        assert.equal(await t.host.supersedePending(binding,{pending_activation:{candidate:'ef'.repeat(32),phase:'drain'}}),false,'legacy supersession must not run before native reset');
        const review=await t.host.reviewTransition(binding,'resume');
        assert.equal(review.cleanBreak,true);assert.equal(review.rebuildIndexes,undefined);assert.equal(review.target,t.f.digest);
        assert.equal(t.calls.some(args=>args[1]==='inspect-candidate'||args[1]==='abort-activation'),false);
        await assert.rejects(t.host.reviewTransition(binding,'rollback'),/clean_break_required_before_runtime_rollback/);
        const stages:string[]=[];await t.host.transition(binding,review,message=>{stages.push(message);});
        assert.deepEqual(t.calls.filter(args=>['stage','activate'].includes(args[1])).map(args=>args[1]),['stage','activate']);
        const activation=t.calls.find(args=>args[1]==='activate')!;
        assert.equal(activation.includes('--rebuild-indexes'),false,'native clean break policy is unconditional, not an optional rebuild fallback');
        assert.equal(activation[activation.indexOf('--expected-active')+1],binding.runtimeSha256);
        assert.match(stages.at(-1)!,/clean installation/);
    }finally{await t.f.cleanup();}
});

test('a validated baseline state contract retains the normal compatible runtime upgrade',async()=>{
    const t=await transitionFixture('1.9.0');
    try{
        t.setSelectedVersion('1.8.16');t.setCleanBreakPolicy({required:false,baseline_version:'1.8.16',state_contract:'airk-state/1',installed_contract:{schema:1}});
        const binding=t.binding(),review=await t.host.reviewTransition(binding,'upgrade');
        assert.equal(review.cleanBreak,undefined);await t.host.transition(binding,review);
        assert.equal(t.calls.find(args=>args[1]==='activate')!.includes('--rebuild-indexes'),false);
    }finally{await t.f.cleanup();}
});

test('a same-version executable without the baseline state contract cannot open a native client or enroll a connection',async()=>{
    const t=await transitionFixture('1.8.16');
    try{
        t.setCleanBreakPolicy({required:true,baseline_version:'1.8.16',state_contract:'airk-state/1',installed_contract:null});
        const binding=t.binding(),before=t.calls.length;
        assert.throws(()=>t.host.client(binding),/clean_break_required_before_connection/);
        await assert.rejects(t.host.httpConnection(binding,{} as any,path.join(t.install,'scope.json'),'ab'.repeat(16),'codex'),/clean_break_required_before_connection/);
        assert.equal(t.calls.length,before,'contract refusal precedes native transport or credential mutation');
    }finally{await t.f.cleanup();}
});

test('native runtime diagnostic transport survives complete cached Management JSON exports without scalar copies',async()=>{
    const diagnosis={upgrade_checkpoint:{schema:1,entries:Array.from({length:200},(_,index)=>({index,future_native_field:'checkpoint-detail-'.repeat(30)})),future_manifest_field:{preserved:true}},future_diagnosis:{preserved:'native transport'}};
    const t=await transitionFixture('1.4.0',diagnosis),copied:string[]=[],opened:string[]=[],saved:string[]=[];
    let trusted=true;
    const host:ConsoleHost={authority:'fixture native runtime',version:'1.4.0',storage:{keys:()=>[],get:()=>undefined,update:async()=>{}},trusted:()=>trusted,
        runtime:t.host,changed(){},confirm:async()=>false,copy:async text=>{copied.push(text);},openReport:async text=>{opened.push(text);},saveReport:async text=>{saved.push(text);},
        logs:async()=>{},resources:async()=>undefined,lifecycle:async()=>{throw new Error('No lifecycle work allowed');}};
    // Discovery still uses the real RuntimeHost parsing/reshaping of native
    // command results. A refused subsequent service connection must not erase it.
    t.host.client=()=>{throw new ServiceError('fixture_service_unavailable');};
    const controller=new ServiceConsoleController(host);
    try{
        const canonical=await t.host.discover();await controller.refresh();
        assert.ok(controller.state.runtimeDiagnostics);
        assert.ok(t.calls.some(args=>args[1]==='diagnose'));
        const nativeCalls=t.calls.length;
        for(const action of ['copyRuntimeDiagnostics','openRuntimeDiagnostics','saveRuntimeDiagnostics'])await controller.action({type:'service',revision:controller.state.revision,action});
        const capture=JSON.parse(copied[0]);assert.equal(capture.qualification,'cached_native_bootstrap_observation');assert.equal(capture.capturedAt,controller.state.runtimeDiagnostics.capturedAt);
        assert.deepEqual(capture.observation,canonical);assert.deepEqual(capture.observation.diagnosis.upgrade_checkpoint,diagnosis.upgrade_checkpoint);
        assert.ok(Buffer.byteLength(copied[0])>65536,'complete JSON must exceed and bypass the summary copy limit');
        assert.deepEqual(opened,copied);assert.deepEqual(saved,copied);assert.equal(t.calls.length,nativeCalls,'cached exports issue no native commands or RPC');
        const view=managementView(controller.state);assert.deepEqual(view.runtimeDiagnostics,controller.state.runtimeDiagnostics);
        assert.doesNotMatch(JSON.stringify(view),/checkpoint-detail/,'scalar Management updates carry only capture metadata');
        assert.match(panelDetails(view,'service','1.4.0'),/Selected service\/runtime summary fields only/);
        const summary=JSON.parse(diagnosticPreview(controller.state,false));assert.equal(summary.qualification,'selected_diagnostic_summary');assert.ok(summary.completeRuntimeDiagnostics);
        trusted=false;await assert.rejects(controller.action({type:'service',revision:controller.state.revision,action:'copyRuntimeDiagnostics'}),/runtime_client_no_longer_authorized/);
        assert.equal(copied.length,1);assert.equal(controller.state.runtimeDiagnostics,undefined);
    }finally{controller.dispose();await t.f.cleanup();}
});

test('concurrent discovery shares verification and callers cannot mutate each other’s observations',async()=>{
    const t=await transitionFixture();
    try{
        const [first,second]=await Promise.all([t.host.discover(),t.host.discover()]);
        assert.equal(t.calls.filter(args=>args[1]==='inspect').length,1);
        assert.equal(t.calls.filter(args=>args[1]==='self-test').length,1);
        first.inspection!.selection={active:'ff'.repeat(32)};
        assert.notDeepEqual(first.inspection!.selection,second.inspection!.selection);
        await t.host.discover();
        assert.equal(t.calls.filter(args=>args[1]==='inspect').length,2,'a subsequent discovery observes external selection changes');
        assert.equal(t.calls.filter(args=>args[1]==='self-test').length,1,'the unchanged verified bundle tests once per host');
    }finally{await t.f.cleanup();}
});

test('warm reconnect checks live identity without restarting; missing health and pending updates use native lifecycle',async context=>{
    const t=await transitionFixture();let ensures=0,disposed=0,healthReads=0;
    let health:RecordValue={version:'1.4.0',runtime_sha256:t.binding().runtimeSha256,policy:{accepting:true}};
    context.mock.method(t.host,'ensure',async()=>{ensures++;return t.binding();});
    context.mock.method(t.host,'client',()=>new ServiceClient({dispose:()=>{disposed++;},request:async()=>{
        healthReads++;return {status:'health',observation:health};
    }}));
    try{
        const observation=await t.host.discover(),calls=t.calls.length;
        const binding=await t.host.reconnect(observation);assert.equal(binding.runtimeSha256,t.binding().runtimeSha256);
        assert.equal(ensures,0);assert.equal(t.calls.length,calls);assert.equal(disposed,1);
        observation.inspection!.lifecycle={explicitly_stopped:true};
        await assert.rejects(t.host.reconnect(observation),/service_explicitly_stopped/);
        assert.equal(healthReads,1);assert.equal(ensures,0);
        observation.inspection!.lifecycle={explicitly_stopped:false};
        health={...health,runtime_sha256:'ff'.repeat(32)};await t.host.reconnect(observation);assert.equal(ensures,1);
        observation.diagnosis={pending_activation:{phase:'probe'}};
        await t.host.reconnect(observation);assert.equal(ensures,2);assert.equal(healthReads,2);
        observation.diagnosis={};observation.inspection!.lifecycle={pending:{intent:'stop'}};
        await t.host.reconnect(observation);assert.equal(ensures,3);assert.equal(healthReads,2);
    }finally{await t.f.cleanup();}
});

test('new installation resolves an exact older unfinished upgrade before bundled activation',async()=>{
    const t=await transitionFixture();
    try{
        const binding=t.binding(),pending={candidate:'ef'.repeat(32),prior:{active:binding.runtimeSha256},
            install:t.install,control:binding.controlDir,phase:'drain'};
        assert.equal(await t.host.supersedePending(binding,undefined),false);
        t.setPending(pending);
        assert.equal(await t.host.supersedePending(binding,{pending_activation:pending}),true);
        assert.equal(t.calls.at(-1)?.[1],'abort-activation');
        const review=await t.host.reviewTransition(binding,'upgrade');
        await t.host.transition(binding,review);assert.equal(t.binding().runtimeSha256,t.f.digest);
        const before=t.calls.length;
        assert.equal(await t.host.supersedePending(binding,{pending_activation:{...pending,candidate:t.f.digest}}),false);
        assert.equal(await t.host.supersedePending(binding,{pending_activation:{...pending,phase:'probe'}}),false);
        assert.equal(t.calls.length,before);
    }finally{await t.f.cleanup();}
});

test('a selected different native version stays pinned until explicit review and cannot open a mismatched console',async()=>{
    const t=await transitionFixture();
    try{
        t.setSelectedVersion('1.4.1');const binding=t.binding();assert.equal(binding.version,'1.4.1');
        assert.throws(()=>t.host.client(binding),/service_runtime_version_mismatch/);
        await assert.rejects(t.host.ensure(),/explicit_runtime_upgrade_or_matching_extension_required/);
        await assert.rejects(t.host.ensure(true),/explicit_runtime_upgrade_or_matching_extension_required/);
        assert.ok(t.calls.every(args=>['self-test','location','inspect','diagnose'].includes(args[1])));
        const reviewed=await t.host.reviewTransition(binding,'upgrade');assert.equal(reviewed.target,t.f.digest);assert.equal(reviewed.version,'1.4.0');
        t.setCandidateVersion('1.4.1');const rollback=await t.host.reviewTransition(binding,'rollback');assert.equal(rollback.version,'1.4.1');
    }finally{await t.f.cleanup();}
});

test('1.6 supersedes an unfinished 1.5 upgrade without making that older candidate resumable',async()=>{
    const t=await transitionFixture('1.6.0');
    try{
        t.setSelectedVersion('1.5.11');t.setCandidateVersion('1.5.12');
        const binding=t.binding(),pending={candidate:'ef'.repeat(32),prior:{active:binding.runtimeSha256},
            install:t.install,control:binding.controlDir,phase:'drain'};
        t.setPending(pending);
        await assert.rejects(t.host.reviewTransition(binding,'resume'),/candidate_not_compatible_with_this_extension/);
        assert.equal(await t.host.supersedePending(binding,{pending_activation:pending}),true);
        assert.equal(t.calls.filter(args=>args[1]==='abort-activation').length,1);
        assert.equal(t.calls.filter(args=>['stage','activate'].includes(args[1])).length,0);
        const review=await t.host.reviewTransition(binding,'upgrade');
        await t.host.transition(binding,review);assert.equal(t.binding().runtimeSha256,t.f.digest);
    }finally{await t.f.cleanup();}
});

test('runtime upgrade uses only its host-owned review and supplies the exact native selection guard',async()=>{
    const t=await transitionFixture();
    try{
        const binding=t.binding(),review=await t.host.reviewTransition(binding,'upgrade');assert.ok(Object.isFrozen(review));
        const before=t.calls.length;
        await assert.rejects(t.host.transition(binding,{...review}),/runtime_transition_not_reviewed/);
        await assert.rejects(t.host.reviewTransition({...binding,lifecycle:'external'},'upgrade'),/runtime_payload_managed_externally/);
        assert.equal(t.calls.length,before);
        const stages:string[]=[];
        const result=await t.host.transition(binding,review,(message,stage)=>{
            assert.ok(message.length>0);stages.push(stage);
            if(stage==='activating')assert.equal(message,'Checking upgrade prerequisites and activating the selected runtime…');
            // Each observation precedes its actual native work, not a guessed percentage.
            assert.equal(t.calls.slice(before).filter(args=>args[1]==='activate').length,0);
        });assert.equal(result.active,t.f.digest);
        assert.deepEqual(stages,['staging','activating']);
        const mutations=t.calls.slice(before);assert.deepEqual(mutations.map(args=>args[1]),['stage','activate']);
        assert.equal(mutations[0][mutations[0].indexOf('--approved-sha256')+1],t.f.digest);
        assert.equal(mutations[1][mutations[1].indexOf('--expected-active')+1],binding.runtimeSha256);
        await assert.rejects(t.host.transition(binding,review),/runtime_transition_not_reviewed/);
    }finally{await t.f.cleanup();}
});

test('an early rebuild choice is bound to the exact host review and native predecessor guard',async()=>{
    const t=await transitionFixture();
    try{
        const binding=t.binding();
        await assert.rejects(t.host.reviewTransition(binding,'rollback',{rebuildIndexes:true}),/invalid_runtime_rebuild_choice/);
        const review=await t.host.reviewTransition(binding,'upgrade',{rebuildIndexes:true});
        assert.equal(review.rebuildIndexes,true);assert.ok(Object.isFrozen(review));
        await assert.rejects(t.host.transition(binding,{...review,rebuildIndexes:false}),/runtime_transition_not_reviewed/);
        await t.host.transition(binding,review);
        const activation=t.calls.filter(args=>args[1]==='activate').at(-1)!;
        assert.ok(activation.includes('--rebuild-indexes'));
        assert.equal(activation[activation.indexOf('--expected-active')+1],binding.runtimeSha256);
        assert.equal(activation[activation.indexOf('--runtime-sha256')+1],t.f.digest);
    }finally{await t.f.cleanup();}
});

test('selection changes invalidate reviewed activation and rollback/resume inspect exact approved candidates',async()=>{
    const t=await transitionFixture();
    try{
        const binding=t.binding(),upgrade=await t.host.reviewTransition(binding,'upgrade');t.setActive('ef'.repeat(32));
        await assert.rejects(t.host.transition(binding,upgrade),/runtime_selection_changed_since_review/);
        assert.equal(t.binding().runtimeSha256,'ef'.repeat(32));
        await assert.rejects(t.host.reviewTransition(binding,'rollback'),/runtime_selection_changed_review_again/);
        const current=t.binding();t.setCandidateVersion('1.3.2');
        await assert.rejects(t.host.reviewTransition(current,'rollback'),/candidate_not_compatible_with_this_extension/);
        t.setCandidateVersion('1.4.0');const rollback=await t.host.reviewTransition(current,'rollback');
        assert.equal(rollback.target,'cd'.repeat(32));const start=t.calls.length;
        await t.host.transition(current,rollback);assert.deepEqual(t.calls.slice(start).map(args=>args[1]),['activate']);
        const selected=t.binding();t.setPending({candidate:selected.runtimeSha256,prior:{active:current.runtimeSha256},install:path.toNamespacedPath(t.install),control:path.toNamespacedPath(selected.controlDir),phase:'probe'});
        await assert.rejects(t.host.reviewTransition(selected,'upgrade'),/resume_pending_runtime_activation/);
        const resumed=await t.host.reviewTransition(selected,'resume');assert.equal(resumed.expectedActive,current.runtimeSha256);
        assert.equal(resumed.observedActive,selected.runtimeSha256);const next=t.calls.length;
        await t.host.transition(selected,resumed);assert.deepEqual(t.calls.slice(next).map(args=>args[1]),['activate']);
        assert.equal(t.calls.at(-1)![t.calls.at(-1)!.indexOf('--expected-active')+1],current.runtimeSha256);
    }finally{await t.f.cleanup();}
});
test('bundled runtime verification binds both executables, complete topology and the installed VSIX pin',async()=>{
    const f=await fixture();
    try {
        const bundle=await verifyBundledRuntime(f.root,'1.4.0');assert.equal(bundle.digest,f.digest);assert.equal(bundle.main,path.join(f.source,f.main));
        await fs.writeFile(path.join(f.source,'unlisted.dll'),'Unknown fixture bytes.');
        await assert.rejects(verifyBundledRuntime(f.root,'1.4.0'),error=>error instanceof ServiceError&&error.code==='unknown_runtime_payload_file');
        await fs.unlink(path.join(f.source,'unlisted.dll'));
        await fs.writeFile(path.join(f.source,f.helper),'Changed helper fixture.');
        await assert.rejects(verifyBundledRuntime(f.root,'1.4.0'),error=>error instanceof ServiceError&&error.code==='runtime_payload_digest_mismatch');
    } finally {await f.cleanup();}
});
test('payload approval refuses foreign targets, changed inventories, unsafe paths and file aliases',async()=>{
    const f=await fixture();
    try {
        const wrapper=path.join(f.root,'native','manifest.json');
        await fs.writeFile(wrapper,JSON.stringify({...f.wrapper,vsix_target:'foreign-architecture'}));
        await assert.rejects(verifyBundledRuntime(f.root,'1.4.0'),/matching_service_vsix_required/);
        await fs.writeFile(wrapper,JSON.stringify(f.wrapper));
        const inventory=path.join(f.source,'airk-install.json'),original=await fs.readFile(inventory);
        await fs.appendFile(inventory,' ');await assert.rejects(verifyBundledRuntime(f.root,'1.4.0'),/runtime_inventory_digest_mismatch/);
        await fs.writeFile(inventory,original);
        const outside=path.join(f.root,'outside-main');await fs.link(path.join(f.source,f.main),outside);
        await assert.rejects(verifyBundledRuntime(f.root,'1.4.0'),/unknown_runtime_payload_file|unsafe_runtime_payload_file/);
        await fs.unlink(outside);
        const value=JSON.parse(original.toString());value.files['../outside-main']='f'.repeat(64);
        const bad=Buffer.from(JSON.stringify(value));await fs.writeFile(inventory,bad);
        await fs.writeFile(wrapper,JSON.stringify({...f.wrapper,install_manifest_sha256:sha(bad)}));
        await assert.rejects(verifyBundledRuntime(f.root,'1.4.0'),/unsafe_runtime_payload_path/);
    } finally {await f.cleanup();}
});
test('invalid trusted payloads fail before any native invocation or user installation write',async()=>{
    const f=await fixture();let calls=0;
    try {
        await fs.writeFile(path.join(f.source,f.main),'Changed native main fixture.');
        const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable'},async()=>{calls++;return {};});
        await assert.rejects(host.discover(),/runtime_payload_digest_mismatch/);assert.equal(calls,0);
    } finally {await f.cleanup();}
});

test('damaged selected payload exposes saved metadata but cannot be launched or silently restaged',async()=>{
    const f=await fixture(),install=path.join(f.root,'install'),calls:string[][]=[];await fs.mkdir(install,{mode:0o700});
    const run:NativeRunner=async(command,args)=>{
        assert.equal(command,path.join(f.source,f.main),'only the verified distribution runs');calls.push(args);
        switch(args[1]){
            case 'self-test':return {protocol:1,version:'1.4.0',self_test:'passed',metadata_console:1};
            case 'location':return {protocol:1,install_dir:install};
            case 'inspect':throw new Error('Selected helper missing');
            case 'snapshot':return {schema:1,metadata_only:true,payload_verified:false,selection:{active:f.digest},
                approval:{version:'1.4.0',manifest_sha256:f.digest,verification_basis:'trusted_distribution'},
                executable:path.join(install,'missing-main'),launcher:null,control_dir:path.join(install,'control')};
            case 'diagnose':return {};
            default:throw new Error('Unexpected repair or execution');
        }
    };
    try{
        const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable',installDir:install},run),saved=await host.discover();
        assert.equal(saved.error,'selected_runtime_payload_unverified');
        const binding=host.binding(saved.inspection!,install);assert.equal(binding.payloadVerified,false);
        assert.throws(()=>host.client(binding),/selected_runtime_payload_unverified/);
        await assert.rejects(host.ensure(),/selected_runtime_payload_unverified/);
        await assert.rejects(host.ensure(true),/selected_runtime_payload_unverified/);
        assert.ok(calls.every(args=>['self-test','location','inspect','snapshot','diagnose'].includes(args[1])));
    }finally{await f.cleanup();}
});

test('payload repair executes the trusted bundle with a reviewed guard and refuses a different approved pin',async()=>{
    const f=await fixture(),calls:string[][]=[],token='ab'.repeat(32),root=path.join(f.root,'install');
    const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable'},async(command,args)=>{
        assert.equal(command,path.join(f.source,f.main));calls.push(args);
        return args.includes('--preview')?{schema:1,runtime:f.digest,repaired:true,service_started:false}:{schema:1,runtime:f.digest,
            preview:token,preserved_directory:path.join(root,'preserved'),preserved_files:3,preserved_bytes:100,starts_service:false,requires_stopped_owners:true};
    });
    const binding={installDir:root,controlDir:path.join(root,'control'),executable:path.join(root,'missing-main'),launcher:null,
        runtimeSha256:f.digest,version:'1.4.0',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution',payloadVerified:false};
    try{
        assert.equal((await host.repair(binding)).preview,token);assert.equal((await host.repair(binding,token)).repaired,true);
        assert.equal(calls[1][calls[1].indexOf('--preview')+1],token);
        await assert.rejects(host.repair({...binding,runtimeSha256:'cd'.repeat(32)}),/matching_approved_distribution_required/);
        await assert.rejects(host.repair({...binding,lifecycle:'external'}),/runtime_payload_managed_externally/);
        assert.equal(calls.length,2);
    }finally{await f.cleanup();}
});
test('discovery does not install/start and concurrent first use shares one approved activation',async()=>{
    const f=await fixture(),install=path.join(f.root,'install'),calls:string[][]=[];let selected=false;
    const inspection=()=>({selection:{active:f.digest,previous:null},executable:path.join(install,'runtimes',f.digest,f.main),launcher:path.join(install,'airk-launcher'),
        control_dir:path.join(install,'control'),managed_policy:null,approval:{version:'1.4.0',manifest_sha256:f.digest,verification_basis:'trusted_distribution'}});
    const run:NativeRunner=async(_command,args)=>{calls.push([...args]);switch(args[1]){
        case'self-test':return {protocol:1,version:'1.4.0',self_test:'passed',metadata_console:1};
        case'location':return {protocol:1,install_dir:install};
        case'stage':assert.equal(args[args.indexOf('--approved-sha256')+1],f.digest);await fs.mkdir(install,{mode:0o700});return {};
        case'activate':selected=true;return {};
        case'inspect':assert.ok(selected);return inspection();
        case'diagnose':return {live_service_health:'not_checked'};
        case'ensure':return {state:'ready'};
        default:throw new Error('Unexpected native fixture action');
    }};
    try {
        const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable'},run);
        assert.equal((await host.discover()).installed,false);assert.ok(calls.every(args=>['self-test','location'].includes(args[1])));
        await assert.rejects(fs.stat(install));
        const first=host.ensure(),second=host.ensure();assert.equal(first,second);
        const binding=await first;assert.equal(binding.runtimeSha256,f.digest);assert.equal(binding.verificationBasis,'trusted_distribution');
        assert.equal(calls.filter(args=>args[1]==='stage').length,1);assert.equal(calls.filter(args=>args[1]==='activate').length,1);
        await host.ensure();assert.equal(calls.filter(args=>args[1]==='activate').length,1,'selected approved payload is reused');
    } finally {await f.cleanup();}
});
test('runtime bootstrap rechecks authorization after asynchronous native observations',async()=>{
    const f=await fixture();let authorized=true;const calls:string[][]=[];
    const run:NativeRunner=async(_command,args)=>{calls.push(args);authorized=false;return {protocol:1,version:'1.4.0',self_test:'passed',metadata_console:1};};
    try {
        const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable'},run,()=>authorized);
        await assert.rejects(host.ensure(),/runtime_client_no_longer_authorized/);assert.equal(calls.length,1);
        assert.ok(calls.every(args=>args[1]==='self-test'));
    }finally {await f.cleanup();}
});
test('managed policy conflicts do not stage payloads, change a pin or start a competing owner',async()=>{
    const f=await fixture(),install=path.join(f.root,'install');await fs.mkdir(install,{mode:0o700});const calls:string[][]=[];
    const options:RuntimeOptions={channel:'stable',installDir:install,managed:{source:f.source,approvedSha256:f.digest,lifecycle:'external'}};
    const run:NativeRunner=async(_command,args)=>{calls.push(args);if(args[1]==='self-test')return {protocol:1,version:'1.4.0',self_test:'passed',metadata_console:1};
        if(args[1]==='location')return {protocol:1,install_dir:install};if(args[1]==='inspect')return {selection:{active:f.digest},managed_policy:null};return {};};
    try {
        const host=new RuntimeHost(f.root,'1.4.0',options,run);
        await assert.rejects(host.ensure(),/managed_runtime_policy_mismatch/);
        assert.ok(!calls.some(args=>['stage','activate','managed','ensure'].includes(args[1])));
        options.managed!.approvedSha256='f'.repeat(64);assert.equal(host.options.managed!.approvedSha256,f.digest,'caller mutation must not retarget a prepared host');
    }finally {await f.cleanup();}
});

test('automatic bootstrap respects an approved same-version pin and a persistent explicit stop',async()=>{
    const f=await fixture(),install=path.join(f.root,'install');await fs.mkdir(install,{mode:0o700});
    const pinned='ef'.repeat(32),calls:string[][]=[];let stopped=true;
    const run:NativeRunner=async(_command,args)=>{
        calls.push(args);switch(args[1]){
            case 'self-test':return {protocol:1,version:'1.4.0',self_test:'passed',metadata_console:1};
            case 'location':return {protocol:1,install_dir:install};
            case 'inspect':return {selection:{active:pinned,previous:f.digest},approval:{version:'1.4.0',manifest_sha256:pinned,verification_basis:'trusted_distribution'},
                executable:path.join(install,'approved-main'),launcher:null,control_dir:path.join(install,'control'),lifecycle:{explicitly_stopped:stopped,pending:null}};
            case 'diagnose':return {};
            case 'start':stopped=false;return {};
            case 'ensure':return {};
            default:throw new Error('Bootstrap changed an explicit runtime pin');
        }
    };
    try{
        const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable',installDir:install},run);
        await assert.rejects(host.ensure(),/service_explicitly_stopped/);
        assert.ok(!calls.some(args=>['stage','activate','ensure','start'].includes(args[1])));
        const started=await host.ensure(true);assert.equal(started.runtimeSha256,pinned);
        assert.equal(calls.filter(args=>args[1]==='start').length,1);
        assert.equal((await host.ensure()).runtimeSha256,pinned);
        assert.ok(!calls.some(args=>['stage','activate'].includes(args[1])));
    }finally{await f.cleanup();}
});

test('managed agent setup prepares a user client launcher while preserving external lifecycle ownership',async()=>{
    const f=await fixture(),install=path.join(f.root,'install');await fs.mkdir(install,{mode:0o700});
    const calls:string[][]=[];let launcher:string|null=null;
    const approval={version:'1.4.0',manifest_sha256:f.digest,verification_basis:'organization_digest'};
    const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable',installDir:install,
        managed:{source:f.source,approvedSha256:f.digest,lifecycle:'external'}},async(_command,args)=>{
        calls.push(args);
        switch(args[1]){
            case 'self-test':return {protocol:1,version:'1.4.0',self_test:'passed',metadata_console:1};
            case 'location':return {protocol:1,install_dir:install};
            case 'inspect':return {selection:{active:f.digest},approval,executable:path.join(f.source,f.main),launcher,
                control_dir:path.join(install,'control'),managed_policy:{source:f.source,approval,lifecycle:'external'}};
            case 'diagnose':return {};
            case 'ensure':return {};
            case 'prepare-client':launcher=path.join(install,'airk-launcher');return {launcher,service_lifecycle_changed:false};
            default:throw new Error('Managed client setup changed runtime ownership');
        }
    });
    try{
        const binding=await host.ensure();assert.equal(binding.lifecycle,'external');assert.equal(binding.launcher,launcher);
        assert.ok(launcher);assert.equal(binding.executable,path.join(f.source,f.main));
        assert.deepEqual(calls.filter(args=>['ensure','prepare-client'].includes(args[1])).map(args=>args[1]),['ensure','prepare-client']);
        assert.ok(!calls.some(args=>['stage','activate','managed','start','login'].includes(args[1])));
    }finally{await f.cleanup();}
});

test('native lifecycle sends a bounded saved intent over stdin and refuses a crossed acknowledgement',async()=>{
    const f=await fixture(),install=path.join(f.root,'install');const inputs:string[]=[];let crossed=false;
    const intent={lifecycle:{action:'restart' as const,caller:'12'.repeat(16),expected_instance:'34'.repeat(16),runtime:f.digest,
        key:{namespace:'56'.repeat(16),epoch:8,nonce:'78'.repeat(16)}}};
    const binding={installDir:install,controlDir:path.join(install,'control'),executable:f.source,launcher:null,runtimeSha256:f.digest,
        version:'1.4.0',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable'},async(_command,args,_timeout,input)=>{
        assert.equal(args[1],'lifecycle');assert.ok(input);inputs.push(input!);
        const request=JSON.parse(input!);delete request.force;
        if(crossed)request.key.nonce='99'.repeat(16);
        return {status:'lifecycle',record:{id:'ab'.repeat(32),request,phase:'complete'}};
    });
    try{
        await host.lifecycle(binding,intent);assert.deepEqual(JSON.parse(inputs[0]),{...intent.lifecycle,force:false});
        crossed=true;await assert.rejects(host.lifecycle(binding,intent),/crossed_lifecycle_receipt/);
    }finally{await f.cleanup();}
});

test('upgrade direction allows skipped historical and future releases while native format checks remain authoritative', () => {
    for (const [from, to] of [['1.4.0', '1.4.4'], ['1.4.4', '1.4.4'], ['1.4.4', '1.5.0'], ['1.4.1', '1.5.3'], ['1.5.0', '1.5.2'], ['1.4.4','1.6.0'], ['1.5.12','1.6.0'], ['1.6.0','1.6.0'], ['1.6.0','1.6.1'], ['1.6.4','1.7.0'], ['1.7.0','1.7.0'], ['1.7.0','1.7.1'], ['1.4.4','1.8.0'], ['1.7.6','1.8.0'], ['1.8.0','1.8.0'], ['1.8.0','1.8.1'], ['1.8.7','1.8.8'], ['1.8.8','1.8.8'], ['1.4.4','1.9.0'], ['1.8.7','1.9.0'], ['1.9.0','1.9.0'], ['1.9.0','1.9.1'],['0.2.1','1.8.14'],['1.0.0','1.8.14'],['1.3.2','1.5.0'],['1.3.2','1.4.0'],['1.3.2','1.9.0'],['1.9.0','1.10.0'],['1.10.0','1.10.1'],['2.0.0','2.0.1']]) { assert.ok(automaticUpgrade(from, to), `${from} → ${to}`); }
    for (const [from, to] of [['1.5.0', '1.4.4'], ['1.4.4', '1.4.3'], ['1.5.1', '1.5.0'], ['1.6.0','1.5.12'], ['1.6.1','1.6.0'], ['1.7.0','1.6.4'], ['1.7.1','1.7.0'], ['1.8.0','1.7.6'], ['1.8.1','1.8.0'], ['1.9.0','1.8.7'], ['1.9.0','1.8.8'], ['1.8.8','1.8.7'], ['1.9.1','1.9.0'], ['1.4', '1.5.0'], ['1.4.x', '1.5.0'], ['1..0','1.6.0'], ['1.5.0','1.6.'], ['1.5.0','1.6.0-beta'], ['1.5.0','1.6.9007199254740992']]) {
        assert.equal(automaticUpgrade(from, to), false, `${from} → ${to}`);
    }
});


test('healthy authenticated connection discovery does not wait for optional diagnostics', {timeout:30000},async()=>{
    const f=await fixture(),install=path.join(f.root,'install');await fs.mkdir(install,{mode:0o700});
    const inspection={selection:{active:f.digest},approval:{manifest_sha256:f.digest,version:'1.4.0',verification_basis:'trusted_distribution'},
        executable:path.join(f.source,f.main),launcher:path.join(install,'launcher'),control_dir:path.join(install,'control'),payload_verified:true,
        lifecycle:{explicitly_stopped:false,pending:null},payload_repair:{phase:'none'},registry_namespace:'12'.repeat(16),
        connection_observation:{schema:1,activation_pending:false,launcher_repair_pending:false}};
    let release!:()=>void,entered!:()=>void,fullDone=false;
    const held=new Promise<void>(resolve=>{release=resolve;}),began=new Promise<void>(resolve=>{entered=resolve;}),calls:string[][]=[];
    const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable',installDir:install},async(_command,args)=>{
        calls.push(args);
        switch(args[1]){case 'self-test':return {protocol:1,version:'1.4.0',self_test:'passed',metadata_console:1};
            case 'location':return {protocol:1,install_dir:install};case 'inspect':return inspection;
            case 'diagnose':entered();await held;return {};default:throw new Error('Unexpected native effect');}
    });
    host.client=()=>new ServiceClient({async request(){return {status:'health',observation:{version:'1.4.0',runtime_sha256:f.digest,instance:'34'.repeat(16),policy:{accepting:true}}};},dispose(){}});
    const full=host.discover().then(value=>{fullDone=true;return value;});
    try{
        const connection=host.discoverForConnection();await began;
        const current=await connection;assert.equal(current.inspection?.payload_verified,true);assert.equal(fullDone,false);
        assert.equal(calls.filter(args=>args[1]==='self-test').length,1);assert.equal(calls.filter(args=>args[1]==='inspect').length,1);
        assert.equal(calls.filter(args=>args[1]==='ensure'||args[1]==='activate').length,0);
        release();assert.deepEqual((await full).diagnosis,{});
    }finally{release();await full.catch(()=>{});await f.cleanup();}
});

test('connection discovery refuses mandatory self-test failure before diagnostics',async()=>{
    const f=await fixture();let diagnostics=0;
    const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable'},async(_command,args)=>{
        if(args[1]==='self-test')return {protocol:1,version:'1.4.0',self_test:'failed',metadata_console:1};
        if(args[1]==='diagnose')diagnostics++;throw new Error('Mandatory failure must stop discovery');
    });
    try{await assert.rejects(host.discoverForConnection(),/service_runtime_self_test_failed/);assert.equal(diagnostics,0);}
    finally{await f.cleanup();}
});


test('pending or unavailable current recovery observation keeps diagnostics in the connection barrier',{timeout:30000},async()=>{
    for(const connection_observation of [undefined,{schema:1,activation_pending:true,launcher_repair_pending:false}]){
        const f=await fixture(),install=path.join(f.root,'install');await fs.mkdir(install,{mode:0o700});
        let release!:()=>void,entered!:()=>void,done=false,health=0;
        const held=new Promise<void>(resolve=>{release=resolve;}),began=new Promise<void>(resolve=>{entered=resolve;});
        const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable',installDir:install},async(_command,args)=>{
            if(args[1]==='self-test')return {protocol:1,version:'1.4.0',self_test:'passed',metadata_console:1};
            if(args[1]==='location')return {protocol:1,install_dir:install};
            if(args[1]==='inspect')return {selection:{active:f.digest},approval:{manifest_sha256:f.digest,version:'1.4.0',verification_basis:'trusted_distribution'},
                executable:path.join(f.source,f.main),launcher:null,control_dir:path.join(install,'control'),payload_verified:true,
                lifecycle:{explicitly_stopped:false,pending:null},payload_repair:{phase:'none'},registry_namespace:'12'.repeat(16),connection_observation};
            if(args[1]==='diagnose'){entered();await held;return {pending_activation:connection_observation?{phase:'probe'}:null};}
            throw new Error('Unexpected lifecycle execution');
        });
        host.client=()=>{health++;throw new Error('Unsettled observation must not probe a healthy shortcut');};
        const pending=host.discoverForConnection().then(value=>{done=true;return value;});
        try{await began;await Promise.resolve();assert.equal(done,false);assert.equal(health,0);release();await pending;}
        finally{release();await pending.catch(()=>{});await f.cleanup();}
    }
});


test('explicit Stop refuses the connection shortcut while optional diagnose is still blocked',{timeout:30000},async()=>{
    const f=await fixture(),install=path.join(f.root,'install');await fs.mkdir(install,{mode:0o700});let release!:()=>void,entered!:()=>void;
    const held=new Promise<void>(resolve=>{release=resolve;}),began=new Promise<void>(resolve=>{entered=resolve;});
    const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable',installDir:install},async(_command,args)=>{
        if(args[1]==='self-test')return {protocol:1,version:'1.4.0',self_test:'passed',metadata_console:1};
        if(args[1]==='location')return {protocol:1,install_dir:install};
        if(args[1]==='inspect')return {selection:{active:f.digest},approval:{manifest_sha256:f.digest,version:'1.4.0',verification_basis:'trusted_distribution'},
            executable:path.join(f.source,f.main),launcher:null,control_dir:path.join(install,'control'),payload_verified:true,lifecycle:{explicitly_stopped:true,pending:null}};
        if(args[1]==='diagnose'){entered();await held;return {};}
        throw new Error('Explicit Stop cannot admit native lifecycle work');
    });
    host.client=()=>{throw new Error('Stopped observation cannot admit health or registry query');};
    const full=host.discover();
    try{await began;await assert.rejects(host.discoverForConnection(),/service_explicitly_stopped/);}
    finally{release();await full.catch(()=>{});await f.cleanup();}
});


function lazySignal<T>(){let resolve!:(value:T)=>void;const promise=new Promise<T>(done=>{resolve=done;});return {promise,resolve};}
async function lazyDiscoveryFixture(){
    const f=await fixture(),install=path.join(f.root,'install');await fs.mkdir(install,{mode:0o700});
    const calls:string[][]=[];let healthCalls=0,disposed=0;
    const state={failSelfTest:false,stopped:false,healthError:undefined as Error|undefined,
        health:undefined as undefined|((number:number)=>Promise<void>)};
    const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable',installDir:install},async(_command,args)=>{
        calls.push(args);
        switch(args[1]){
            case 'self-test':return {protocol:1,version:'1.4.0',self_test:state.failSelfTest?'failed':'passed',metadata_console:1};
            case 'location':return {protocol:1,install_dir:install};
            case 'inspect':return {selection:{active:f.digest},approval:{manifest_sha256:f.digest,version:'1.4.0',verification_basis:'trusted_distribution'},
                executable:path.join(f.source,f.main),launcher:path.join(install,'launcher'),control_dir:path.join(install,'control'),payload_verified:true,
                lifecycle:{explicitly_stopped:state.stopped,pending:null},payload_repair:{phase:'none'},registry_namespace:'12'.repeat(16),
                connection_observation:{schema:1,activation_pending:false,launcher_repair_pending:false}};
            case 'diagnose':return {fixture:'full'};
            default:throw new Error('Unexpected native execution effect');
        }
    });
    host.client=()=>new ServiceClient({async request(){
        const number=++healthCalls;if(state.health)await state.health(number);if(state.healthError)throw state.healthError;
        return {status:'health',observation:{version:'1.4.0',runtime_sha256:f.digest,instance:'34'.repeat(16),policy:{accepting:true}}};
    },dispose(){disposed++;}});
    return {host,state,calls,count:(name:string)=>calls.filter(args=>args[1]===name).length,disposed:()=>disposed,cleanup:f.cleanup};
}
test('lone healthy connection runs zero diagnose and later full discovery obtains fresh inspect exactly once',{timeout:30000},async()=>{
    const f=await lazyDiscoveryFixture();
    try{
        const connection=await f.host.discoverForConnection();assert.equal(connection.diagnosis,undefined);
        assert.equal(f.count('inspect'),1);assert.equal(f.count('diagnose'),0);assert.equal(f.disposed(),1);
        const [first,second]=await Promise.all([f.host.discover(),f.host.discover()]);
        assert.deepEqual(first.diagnosis,{fixture:'full'});assert.deepEqual(second,first);
        assert.equal(f.count('inspect'),2,'finished connection-only attempt is not a persistent inspect cache');
        assert.equal(f.count('diagnose'),1);assert.equal(f.count('self-test'),1,'existing successful self-test cache retained');
    }finally{await f.cleanup();}
});
test('mandatory health and Stop failures retire connection-only attempts without diagnostics or poisoned reuse',{timeout:30000},async()=>{
    for(const failure of ['self-test','health','stop']){
        const f=await lazyDiscoveryFixture(),original=new Error('actual fixture health failure');
        try{
            f.state.failSelfTest=failure==='self-test';f.state.stopped=failure==='stop';f.state.healthError=failure==='health'?original:undefined;
            if(failure==='health')await assert.rejects(f.host.discoverForConnection(),error=>error===original);
            else await assert.rejects(f.host.discoverForConnection(),failure==='stop'?/service_explicitly_stopped/:/service_runtime_self_test_failed/);
            assert.equal(f.count('diagnose'),0);
            const before=f.count('inspect');f.state.failSelfTest=false;f.state.stopped=false;f.state.healthError=undefined;
            assert.equal((await f.host.discoverForConnection()).inspection?.payload_verified,true);
            assert.equal(f.count('inspect'),before+1);assert.equal(f.count('diagnose'),0);
        }finally{await f.cleanup();}
    }
});
test('connection waiters share mandatory work until the last health completes even after full settles',{timeout:30000},async()=>{
    const f=await lazyDiscoveryFixture(),entered=[lazySignal<void>(),lazySignal<void>(),lazySignal<void>()],release=[lazySignal<void>(),lazySignal<void>(),lazySignal<void>()];
    f.state.health=async number=>{if(number<=3){entered[number-1].resolve();await release[number-1].promise;}};
    let first:Promise<unknown>|undefined,second:Promise<unknown>|undefined,third:Promise<unknown>|undefined;
    try{
        first=f.host.discoverForConnection();second=f.host.discoverForConnection();
        await Promise.all([entered[0].promise,entered[1].promise]);release[0].resolve();await first;
        third=f.host.discoverForConnection();await entered[2].promise;
        assert.equal(f.count('inspect'),1,'third caller must share while second still owns health');assert.equal(f.count('diagnose'),0);
        release[2].resolve();await third;
        assert.deepEqual((await f.host.discover()).diagnosis,{fixture:'full'});assert.equal(f.count('inspect'),1);assert.equal(f.count('diagnose'),1);
        await f.host.discoverForConnection();assert.equal(f.count('inspect'),1,'settled full stays in active attempt until remaining connection finishes');
        release[1].resolve();await second;
        await f.host.discover();assert.equal(f.count('inspect'),2);assert.equal(f.count('diagnose'),2);
    }finally{release.forEach(gate=>gate.resolve());await Promise.allSettled([first,second,third]);await f.cleanup();}
});

test('legacy retirement Stop during RuntimeHost location preparation prevents later authorize or resume dispatch',async()=>{
    for(const action of ['authorize','resume']){
        const f=await fixture(),entered=lazySignal<void>(),release=lazySignal<void>();let held=false,mutations=0;
        const hash='ab'.repeat(32),index='idx_'+'cd'.repeat(16),who='ef'.repeat(16);
        const preview={schema:1,request_digest:hash,original_backup_digest:hash,original_legacy_data_digest:hash,
            source_configuration:'pending_metadata',effect:'retire_and_cancel_with_cleanup_pending',affected_work:{jobs:0,job_ids_digest:hash},
            request:{schema:1,caller:who,key:{namespace:who,epoch:0,nonce:who},upgrade_id:who,plan_digest:hash,installer_identity:hash,index_id:index,
                registration:{binding:hash,root:'/fictional/source',data_root:'/fictional/private',collection:'a'},expected_authorizations_digest:hash}};
        const review={status:'review',preview,review_digest:hash};
        const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable'},async(_command,args)=>{
            if(args[1]==='location'){if(held){entered.resolve();await release.promise;}return {protocol:1,install_dir:path.join(f.root,'install')};}
            if(args[1]==='legacy-retirement-review')return review;
            mutations++;throw new Error('mutation must not dispatch after Stop');
        });
        const session=new LegacyRetirementSession(host,()=>true);let pending:Promise<unknown>|undefined;
        try{
            if(action==='authorize')await session.prepare(index);
            else session.restore({status:'authorized',authorization:{preview,review_digest:hash}});
            held=true;pending=action==='authorize'?session.authorize(review):session.resume();
            await entered.promise;session.stop();release.resolve();
            await assert.rejects(pending,/review_stopped/);assert.equal(mutations,0);
        }finally{release.resolve();await Promise.allSettled([pending]);await f.cleanup();}
    }
});


test('POSIX runtime install metadata distinguishes ownership from privacy and quotes chmod paths',()=>{
    const directory={isDirectory:()=>true,isSymbolicLink:()=>false,uid:42,mode:0o40700};
    assert.doesNotThrow(()=>validateRuntimeInstallDirectory('/owned runtime',directory,'linux',42));
    assert.throws(()=>validateRuntimeInstallDirectory('/foreign',{...directory,uid:43,mode:0o40755},'darwin',42),error=>{
        assert.ok(error instanceof ServiceError);assert.equal(error.code,'runtime_install_directory_wrong_owner');
        assert.match(error.message,/directory you own|nonexistent directory/);assert.doesNotMatch(error.message,/chmod/);return true;
    });
    assert.throws(()=>validateRuntimeInstallDirectory("/owned/a'b $HOME",{...directory,mode:0o40750},'linux',42),error=>{
        assert.ok(error instanceof ServiceError);assert.equal(error.code,'runtime_install_directory_not_private');
        assert.ok(error.message.includes("chmod 700 '/owned/a'\"'\"'b $HOME'"));return true;
    });
    assert.doesNotThrow(()=>validateRuntimeInstallDirectory('C:\\runtime',{...directory,uid:43,mode:0o40777},'win32',undefined));
    assert.throws(()=>validateRuntimeInstallDirectory('/alias',{...directory,isSymbolicLink:()=>true},'linux',42),/unsafe_runtime_install_location/);
    assert.throws(()=>validateRuntimeInstallDirectory('/file',{...directory,isDirectory:()=>false},'win32',undefined),/unsafe_runtime_install_location/);
});

test('runtime discovery accepts a new root without creation and a private existing root without changing permissions',async()=>{
    const f=await fixture(),install=path.join(f.root,'private install'),calls:string[][]=[];
    const run:NativeRunner=async(_command,args)=>{
        calls.push(args);
        if(args[1]==='self-test')return {protocol:1,version:'1.4.0',self_test:'passed',metadata_console:1};
        if(args[1]==='location')return {protocol:1,install_dir:install};
        if(args[1]==='inspect'||args[1]==='diagnose')return {};
        throw new Error('Unexpected install mutation: '+args.join(' '));
    };
    try{
        const missing=await new RuntimeHost(f.root,'1.4.0',{channel:'stable'},run).discover();
        assert.equal(missing.installed,false);await assert.rejects(fs.lstat(install),{code:'ENOENT'});
        await fs.mkdir(install,{mode:0o700});const before=await fs.lstat(install);
        const existing=await new RuntimeHost(f.root,'1.4.0',{channel:'stable'},run).discover();
        assert.equal(existing.installed,true);assert.equal((await fs.lstat(install)).mode,before.mode);
        assert.ok(calls.every(args=>['self-test','location','inspect','diagnose'].includes(args[1])));
    }finally{await f.cleanup();}
});

test('POSIX discovery refuses an existing public install before native inspection without changing it', {skip:process.platform==='win32'},async()=>{
    const f=await fixture(),install=path.join(f.root,'public install'),calls:string[][]=[];
    try{
        await fs.mkdir(install,{mode:0o700});await fs.chmod(install,0o755);
        const run:NativeRunner=async(_command,args)=>{
            calls.push(args);
            if(args[1]==='self-test')return {protocol:1,version:'1.4.0',self_test:'passed',metadata_console:1};
            if(args[1]==='location')return {protocol:1,install_dir:install};
            throw new Error('Unsafe root reached native inspection');
        };
        await assert.rejects(new RuntimeHost(f.root,'1.4.0',{channel:'stable'},run).discover(),error=>
            error instanceof ServiceError&&error.code==='runtime_install_directory_not_private');
        assert.equal((await fs.lstat(install)).mode&0o777,0o755);
        assert.deepEqual(calls.map(args=>args[1]),['self-test','location']);
    }finally{await f.cleanup();}
});

test('1.8.14 platform bundles require one sealed native image and schema2 role metadata',async()=>{
    for(const host of [{platform:'win32',arch:'x64'},{platform:'linux',arch:'x64'},{platform:'darwin',arch:'arm64'},{platform:'darwin',arch:'x64'}]){
        const f=await fixture(host,'1.8.14');
        try{
            await assert.rejects(verifyBundledRuntime(f.root,'1.8.14',host),/single_image_runtime_required/);
            await fs.unlink(path.join(f.source,f.helper));
            const inventoryPath=path.join(f.source,'airk-install.json'),inventory=JSON.parse(await fs.readFile(inventoryPath,'utf8'));delete inventory.files[f.helper];
            const metadata={schema:2,version:'1.8.14',os:inventory.os,arch:inventory.arch,airk_sha256:inventory.files[f.main]},metadataBytes=JSON.stringify(metadata);
            await fs.writeFile(path.join(f.source,'airk-runtime.json'),metadataBytes);inventory.files['airk-runtime.json']=sha(metadataBytes);
            const bytes=JSON.stringify(inventory);await fs.writeFile(inventoryPath,bytes);
            const wrapper={...f.wrapper,install_manifest_sha256:sha(bytes),payload:{[f.main]:inventory.files[f.main]}};await fs.writeFile(path.join(f.root,'native','manifest.json'),JSON.stringify(wrapper));
            const bundle=await verifyBundledRuntime(f.root,'1.8.14',host);assert.equal(bundle.main,path.join(f.source,f.main));assert.equal(bundle.helper,undefined);
            metadata.airk_sha256='0'.repeat(64);const crossed=JSON.stringify(metadata);await fs.writeFile(path.join(f.source,'airk-runtime.json'),crossed);inventory.files['airk-runtime.json']=sha(crossed);
            const changed=JSON.stringify(inventory);await fs.writeFile(inventoryPath,changed);wrapper.install_manifest_sha256=sha(changed);await fs.writeFile(path.join(f.root,'native','manifest.json'),JSON.stringify(wrapper));
            await assert.rejects(verifyBundledRuntime(f.root,'1.8.14',host),/single_image_runtime_metadata_mismatch/);
        }finally{await f.cleanup();}
    }
});

async function modernBindingFixture(version:string){
    const f=await fixture(undefined,version);await fs.unlink(path.join(f.source,f.helper));
    const inventoryPath=path.join(f.source,'airk-install.json'),inventory=JSON.parse(await fs.readFile(inventoryPath,'utf8'));delete inventory.files[f.helper];
    const pair={schema:2,version,os:inventory.os,arch:inventory.arch,airk_sha256:inventory.files[f.main]},pairBytes=JSON.stringify(pair);
    await fs.writeFile(path.join(f.source,'airk-runtime.json'),pairBytes);inventory.files['airk-runtime.json']=sha(pairBytes);
    const bytes=JSON.stringify(inventory),digest=sha(bytes);await fs.writeFile(inventoryPath,bytes);
    await fs.writeFile(path.join(f.root,'native','manifest.json'),JSON.stringify({...f.wrapper,install_manifest_sha256:digest,payload:{[f.main]:inventory.files[f.main]}}));
    return {...f,digest,pair};
}
test('future host cleanup verifies exact selected modern format/capabilities without relaxing ordinary client version gate',async()=>{
    const current=await modernBindingFixture('1.8.15'),previous=await modernBindingFixture('1.8.14'),install=path.join(previous.root,'install');let fault='';const calls:string[][]=[];
    const binding={installDir:install,controlDir:path.join(install,'control'),executable:path.join(previous.source,previous.main),launcher:null,runtimeSha256:previous.digest,version:'1.8.14',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution',payloadVerified:true};
    const endpoint={schema:1,installation:'a'.repeat(64),user:'fixture',namespace:'b'.repeat(32),generation:1,service_instance:'c'.repeat(32),executable_sha256:previous.pair.airk_sha256,url:'https://127.0.0.1:57421/mcp',ca_sha256:'d'.repeat(64),certificate_sha256:'e'.repeat(64),state:'ready',predecessor_digest:null};
    const host=new RuntimeHost(current.root,'1.8.15',{channel:'stable'},async(command,args,_timeout,input)=>{
        calls.push([...args]);
        // Native metadata RPC/endpoint discovery require the predecessor's
        // compiled version/image. Accepting the new installer would hide the
        // actual service_rebind_required refusal.
        assert.equal(command,args[1]==='inspect'?path.join(current.source,current.main):binding.executable);
        if(args[1]==='inspect')return {payload_verified:fault!=='unverified',selection:{active:fault==='selection'?'0'.repeat(64):binding.runtimeSha256},approval:{version:binding.version,manifest_sha256:fault==='selection'?'0'.repeat(64):binding.runtimeSha256,verification_basis:'trusted_distribution'},executable:binding.executable,launcher:null,control_dir:fault==='control'?path.join(install,'foreign'):binding.controlDir};
        if(args[1]==='self-test'){assert.equal(command,binding.executable);return {version:fault==='version'?'1.8.13':binding.version,protocol:fault==='protocol'?2:1,metadata_console:fault==='capabilities'?0:1,self_test:'passed'};}
        if(args[1]==='rpc'&&input!==JSON.stringify('health'))return {status:'console',page:{revision:1,cached:true,observed_unix_ms:1,after:null,projects:[]}};
        if(args[1]==='rpc')return {status:'health',observation:{version:binding.version,protocol:1,runtime_sha256:binding.runtimeSha256,executable_sha256:fault==='hash'?'0'.repeat(64):previous.pair.airk_sha256,instance:fault==='live_instance'?'f'.repeat(32):endpoint.service_instance,catalog_clock:{namespace:endpoint.namespace,revision:1},policy:{accepting:true}}};
        if(args[1]==='http-endpoint')return {...endpoint,service_instance:fault==='endpoint'?'f'.repeat(32):endpoint.service_instance,namespace:fault==='endpoint_namespace'?'f'.repeat(32):endpoint.namespace,executable_sha256:fault==='endpoint_hash'?'f'.repeat(64):endpoint.executable_sha256,state:fault==='endpoint_state'?'recovering':endpoint.state};
        throw Error('Unexpected cleanup mutation');
    });
    try{
        assert.throws(()=>host.client(binding),/service_runtime_version_mismatch/);
        const reader=await host.migrationClient(binding);assert.deepEqual(Object.keys(reader).sort(),['dispose','projects']);
        assert.deepEqual(await reader.projects(),{revision:1,cached:true,observed_unix_ms:1,after:null,projects:[]});
        assert.deepEqual(calls.map(args=>args[1]),['inspect','self-test','rpc','http-endpoint','rpc','rpc']);
        fault='live_instance';await assert.rejects(reader.projects(),/legacy_cleanup_health_unconfirmed/);reader.dispose();await assert.rejects(reader.projects(),/legacy_cleanup_reader_disposed/);
        for(fault of ['unverified','selection','control','version','protocol','capabilities','hash','endpoint','endpoint_namespace','endpoint_hash','endpoint_state'])await assert.rejects(host.migrationClient(binding));
        await assert.rejects(host.migrationClient({...binding,version:'2.0.0'}),/legacy_cleanup_protocol_unverified/);
        assert.throws(()=>host.client(binding),/service_runtime_version_mismatch/);
    }finally{await current.cleanup();await previous.cleanup();}
});

test('native HTTP lookup distinguishes authenticated foreign absence from owned unavailable without enrolling',async()=>{
    const f=await modernBindingFixture('1.8.14'),install=path.join(f.root,'install'),id='a'.repeat(32);let reply:RecordValue;
    const binding={installDir:install,controlDir:path.join(install,'control'),executable:path.join(f.source,f.main),launcher:null,runtimeSha256:f.digest,version:'1.8.14',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    const host=new RuntimeHost(f.root,'1.8.14',{channel:'stable'},async(_command,args)=>{assert.deepEqual(args,['runtime','lookup-http-connection','--install-dir',install,'--expected-active',f.digest,'--connection-id',id]);return reply;});
    const base={schema:1,active:f.digest,connection_id:id};
    try{
        reply={...base,status:'not_owned',future:{retained:true}};assert.deepEqual(await host.lookupHttpConnection(binding,id),reply);
        for(const code of ['owned_connection_revoked','owned_connection_expired','owned_connection_unavailable','owned_connection_attachment_retired']){reply={...base,status:'unavailable',code,native_error:{exact:'kept'},guidance:'Review native owner'};await assert.rejects(host.lookupHttpConnection(binding,id),error=>error instanceof ServiceError&&!error.outcomeUnknown&&error.nativeResponse===reply&&error.code===code);}
        for(const value of [{...base,status:'not_owned',active:'0'.repeat(64)},{...base,status:'not_owned',connection_id:'0'.repeat(32)},{...base,status:'unavailable',code:'unknown'},{...base,status:'not_owned',connection:{}},{...base,status:'invented'}]){reply=value;await assert.rejects(host.lookupHttpConnection(binding,id));}
    }finally{await f.cleanup();}
});

test('only explicit standalone recovery consumes a native attachment retirement proof',async()=>{
    const f=await modernBindingFixture('1.8.14'),install=path.join(f.root,'install'),id='a'.repeat(32),namespace='b'.repeat(32);let reply:RecordValue;
    const binding={installDir:install,controlDir:path.join(install,'control'),executable:path.join(f.source,f.main),launcher:null,runtimeSha256:f.digest,version:'1.8.14',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    const selection:any={kind:'scoped',selection:{schema:1,context_key:'a'.repeat(64),namespace,expected_catalog_revision:7,grant_id:'grt_'+'c'.repeat(32),expected_grant_revision:1,index_id:'idx_'+'d'.repeat(32),project_ids:['prj_'+'e'.repeat(32)]}};
    const connection:any={schema:1,connection_id:id,endpoint:{schema:1,installation:f.digest,user:'fixture',namespace,generation:1,service_instance:'c'.repeat(32),executable_sha256:f.digest,url:'https://127.0.0.1:57421/mcp',ca_sha256:f.digest,certificate_sha256:f.digest,state:'ready',predecessor_digest:null},selection,ca_certificate_file:path.join(binding.controlDir,'http-1','ca.pem'),credential_file:path.join(binding.controlDir,'http-1','credential-'+id+'.token')};
    const proof={schema:1,connection_id:id,installation:f.digest,namespace,client:'claude_code',expires_unix_ms:Date.now()+60000,explorer_descriptor_sha256:'d'.repeat(64),authority_sha256:httpAuthorityDigest(selection)};
    const base={schema:1,active:f.digest,connection_id:id,status:'unavailable',code:'owned_connection_attachment_retired',attachment_retirement:proof};
    const host=new RuntimeHost(f.root,'1.8.14',{channel:'stable'},async(_command,args)=>{assert.deepEqual(args,['runtime','lookup-http-connection','--install-dir',install,'--expected-active',f.digest,'--connection-id',id]);return reply;});
    try{
        reply=base;await assert.rejects(host.lookupHttpConnection(binding,id),error=>error instanceof ServiceError&&error.code==='owned_connection_attachment_retired');
        assert.equal(await host.lookupHttpConnection(binding,id,{connection,client:'claude'}),reply,'Complete native proof result is retained only for the explicit recovery caller');
        for(const code of ['owned_connection_revoked','owned_connection_expired','owned_connection_unavailable']){reply={...base,code};await assert.rejects(host.lookupHttpConnection(binding,id,{connection,client:'claude'}),error=>error instanceof ServiceError&&error.code===code);}
        reply={...base,attachment_retirement:{...proof,authority_sha256:'0'.repeat(64)}};await assert.rejects(host.lookupHttpConnection(binding,id,{connection,client:'claude'}),/http_attachment_retirement_unconfirmed/);
        reply={...base,attachment_retirement:undefined};await assert.rejects(host.lookupHttpConnection(binding,id,{connection,client:'claude'}),/http_attachment_retirement_unconfirmed/);
        reply={...base,status:'owned',connection};await assert.rejects(host.lookupHttpConnection(binding,id,{connection,client:'claude'}),/http_connection_lookup_unconfirmed/);
    }finally{await f.cleanup();}
});
test('modern previous-runtime retirement proves all superseded images gone without restarting selected service',async()=>{
    const f=await modernBindingFixture('1.8.14'),install=path.join(f.root,'install');let reply:RecordValue;
    const binding={installDir:install,controlDir:path.join(install,'control'),executable:path.join(f.source,f.main),launcher:null,runtimeSha256:f.digest,version:'1.8.14',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
    const host=new RuntimeHost(f.root,'1.8.14',{channel:'stable'},async()=>reply);
    const valid={active:f.digest,complete:true,selected_service_restarted:false,retired_previous:'a'.repeat(64),retired_runtimes:['a'.repeat(64),'b'.repeat(64)],processes:{scope:'verified_superseded_runtime_images',remaining:0,observations:[{unknown_system_images:2,owned_images_absent:true}]},future:{kept:true}};
    try{reply=valid;assert.deepEqual(await host.retirePrevious(binding),valid);
        for(const mutate of [(v:any)=>v.complete=false,(v:any)=>v.selected_service_restarted=true,(v:any)=>v.retired_runtimes.push(f.digest),(v:any)=>v.processes.remaining=1,(v:any)=>{delete v.processes;}]){reply=structuredClone(valid);mutate(reply);await assert.rejects(host.retirePrevious(binding),error=>error instanceof ServiceError&&error.outcomeUnknown&&error.nativeResponse===reply);}
    }finally{await f.cleanup();}
});

test('every native bootstrap reply has one 2 MiB wire budget; the refused adoption command has no larger one',async context=>{
    const text=JSON.stringify({padding:'x'.repeat(2*1024*1024)});
    context.mock.method(require('../native'),'runText',async()=>text);
    for(const args of [['runtime','inspect'],['runtime','adopt-legacy-indexes']]){
        await assert.rejects(nativeRunner('fixture-native',args),(error:unknown)=>error instanceof ServiceError&&error.code==='bootstrap_response_oversize'&&!error.outcomeUnknown);
    }
});
