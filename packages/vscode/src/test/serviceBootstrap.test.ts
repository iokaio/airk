// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { NativeRunner, RuntimeHost, RuntimeOptions, verifyBundledRuntime } from '../serviceBootstrap';
import { RecordValue, ServiceClient, ServiceError, record } from '../serviceClient';
import { nativePayloadDirectory } from './nativeServiceFixture';
const sha=(bytes:Buffer|string)=>createHash('sha256').update(bytes).digest('hex');

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
    await fs.mkdir(target.root);await fs.mkdir(target.data_dir);await fs.mkdir(path.dirname(target.executable));await fs.writeFile(target.executable,'Old command is data only.');
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

async function fixture(host:{platform:string;arch:string}={platform:process.platform,arch:process.arch}){
    const root=await fs.mkdtemp(path.join(os.tmpdir(),'airk-runtime-bootstrap-'));
    const source=path.join(root,'native','runtime');await fs.mkdir(source,{recursive:true});
    const prefix=host.platform==='darwin'?'AIRK.app/Contents/MacOS/':'';
    const main=prefix+(host.platform==='win32'?'airk.exe':'airk'),helper=prefix+(host.platform==='win32'?'airk-embed.exe':'airk-embed');
    const files:Record<string,string>={};
    for(const [name,body]of [[main,'Original native main fixture.'],[helper,'Original native inference fixture.'],['LICENSE','Original fixture license notice.']]){
        await fs.mkdir(path.dirname(path.join(source,name)),{recursive:true});await fs.writeFile(path.join(source,name),body);files[name]=sha(body);
    }
    const inventory=Buffer.from(JSON.stringify({schema:1,version:'1.4.0',os:host.platform==='win32'?'windows':host.platform==='darwin'?'macos':'linux',
        arch:host.arch==='arm64'?'aarch64':'x86_64',files}));
    const digest=sha(inventory);await fs.writeFile(path.join(source,'airk-install.json'),inventory);
    const wrapper={schema:2,version:'1.4.0',vsix_target:`${host.platform}-${host.arch}`,runtime_dir:'runtime',install_manifest_sha256:digest,payload:{[main]:files[main],[helper]:files[helper]}};
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

async function transitionFixture(){
    const f=await fixture(),install=path.join(f.root,'install');await fs.mkdir(install);
    await fs.mkdir(path.join(install,'control'));
    const calls:string[][]=[];let active='ab'.repeat(32),pending:RecordValue|null=null,candidateVersion='1.4.0',selectedVersion='1.4.0';
    const inspection=()=>({selection:{active,previous:'cd'.repeat(32)},executable:path.join(install,'runtimes',active,f.main),launcher:path.join(install,'airk-launcher'),
        control_dir:path.join(install,'control'),managed_policy:null,approval:{version:selectedVersion,manifest_sha256:active,verification_basis:'trusted_distribution'}});
    const run:NativeRunner=async(command,args)=>{
        assert.equal(command,path.join(f.source,f.main));calls.push([...args]);
        switch(args[1]){
            case 'self-test':return {protocol:1,version:'1.4.0',self_test:'passed',metadata_console:1};
            case 'location':return {protocol:1,install_dir:install};
            case 'inspect':return inspection();
            case 'diagnose':return {pending_activation:pending};
            case 'inspect-candidate':{const target=args[args.indexOf('--runtime-sha256')+1];return {runtime_sha256:target,metadata_only:true,selection_changed:false,executed:false,
                approval:{version:candidateVersion,manifest_sha256:target,verification_basis:'trusted_distribution'}};}
            case 'stage':return {};
            case 'activate':{
                const expected=args[args.indexOf('--expected-active')+1],target=args[args.indexOf('--runtime-sha256')+1];
                if(active!==expected&&!(pending?.candidate===target&&record(pending.prior).active===expected))throw new ServiceError('runtime_selection_changed_since_review');
                active=target;return {schema:1,active,previous:expected};
            }
            default:throw new Error('Unexpected transition action');
        }
    };
    const host=new RuntimeHost(f.root,'1.4.0',{channel:'stable',installDir:install},run);
    return {f,host,calls,install,binding:()=>host.binding(inspection(),install),setActive:(value:string)=>{active=value;},
        setPending:(value:RecordValue)=>{pending=value;},setCandidateVersion:(value:string)=>{candidateVersion=value;},setSelectedVersion:(value:string)=>{selectedVersion=value;}};
}

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

test('runtime upgrade uses only its host-owned review and supplies the exact native selection guard',async()=>{
    const t=await transitionFixture();
    try{
        const binding=t.binding(),review=await t.host.reviewTransition(binding,'upgrade');assert.ok(Object.isFrozen(review));
        const before=t.calls.length;
        await assert.rejects(t.host.transition(binding,{...review}),/runtime_transition_not_reviewed/);
        await assert.rejects(t.host.reviewTransition({...binding,lifecycle:'external'},'upgrade'),/runtime_payload_managed_externally/);
        assert.equal(t.calls.length,before);
        const result=await t.host.transition(binding,review);assert.equal(result.active,t.f.digest);
        const mutations=t.calls.slice(before);assert.deepEqual(mutations.map(args=>args[1]),['stage','activate']);
        assert.equal(mutations[0][mutations[0].indexOf('--approved-sha256')+1],t.f.digest);
        assert.equal(mutations[1][mutations[1].indexOf('--expected-active')+1],binding.runtimeSha256);
        await assert.rejects(t.host.transition(binding,review),/runtime_transition_not_reviewed/);
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
    const f=await fixture(),install=path.join(f.root,'install'),calls:string[][]=[];await fs.mkdir(install);
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
        case'stage':assert.equal(args[args.indexOf('--approved-sha256')+1],f.digest);await fs.mkdir(install);return {};
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
    const f=await fixture(),install=path.join(f.root,'install');await fs.mkdir(install);const calls:string[][]=[];
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
    const f=await fixture(),install=path.join(f.root,'install');await fs.mkdir(install);
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
    const f=await fixture(),install=path.join(f.root,'install');await fs.mkdir(install);
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
