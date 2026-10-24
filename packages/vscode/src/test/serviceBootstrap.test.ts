// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { NativeRunner, RuntimeHost, RuntimeOptions, verifyBundledRuntime } from '../serviceBootstrap';
import { RecordValue, ServiceError } from '../serviceClient';
const sha=(bytes:Buffer|string)=>createHash('sha256').update(bytes).digest('hex');
async function fixture(){
    const root=await fs.mkdtemp(path.join(os.tmpdir(),'airk-runtime-bootstrap-'));
    const source=path.join(root,'native','runtime');await fs.mkdir(source,{recursive:true});
    const main=process.platform==='win32'?'airk.exe':'airk',helper=process.platform==='win32'?'airk-embed.exe':'airk-embed';
    const files:Record<string,string>={};
    for(const [name,body]of [[main,'Original native main fixture.'],[helper,'Original native inference fixture.'],['LICENSE','Original fixture license notice.']]){
        await fs.writeFile(path.join(source,name),body);files[name]=sha(body);
    }
    const inventory=Buffer.from(JSON.stringify({schema:1,version:'1.4.0',os:process.platform==='win32'?'windows':process.platform==='darwin'?'macos':'linux',
        arch:process.arch==='arm64'?'aarch64':'x86_64',files}));
    const digest=sha(inventory);await fs.writeFile(path.join(source,'airk-install.json'),inventory);
    const wrapper={schema:2,version:'1.4.0',vsix_target:`${process.platform}-${process.arch}`,runtime_dir:'runtime',install_manifest_sha256:digest,payload:{[main]:files[main],[helper]:files[helper]}};
    await fs.writeFile(path.join(root,'native','manifest.json'),JSON.stringify(wrapper));
    async function cleanup(){const target=path.resolve(root),base=path.resolve(os.tmpdir());assert.equal(path.dirname(target),base);assert.ok(path.basename(target).startsWith('airk-runtime-bootstrap-'));await fs.rm(target,{recursive:true,force:true});}
    return {root,source,main,helper,digest,wrapper,cleanup};
}
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
