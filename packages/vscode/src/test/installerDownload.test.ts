// SPDX-License-Identifier: Apache-2.0
/** Deterministic HTTP/filesystem/signature/process observations, not installed qualification. */
import {test} from 'node:test';
import * as assert from 'node:assert/strict';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import {createHash} from 'node:crypto';
import {SystemInstaller,assessMacInstaller,type InstallerSystemPorts} from '../airkctl/bootstrap/system';
import {SIGNING_IDENTITY} from '../airkctl/signingIdentity';
import {systemVerifyHost,type VerifyHost} from '../airkctl/verify';

const repository=path.resolve(__dirname,'../../../..');
const sha=(raw:Buffer)=>createHash('sha256').update(raw).digest('hex');
const json=(value:unknown)=>Buffer.from(JSON.stringify(value));
async function fixture(){
    const directory=await fs.mkdtemp(path.join(repository,'artifacts/bootstrap-adapter-'));
    const cases=JSON.parse(await fs.readFile(path.join(repository,'fixtures/split/receipts/publication_2.json'),'utf8'));
    const publication=structuredClone(cases.cases.find((row:any)=>row.kind==='release'&&row.accepted&&row.input.channel==='stable').input);
    const target='x86_64-pc-windows-msvc',name='airk-1.8.35-x86_64-pc-windows-msvc-installer.exe';
    const manifestName='airk-1.8.35-x86_64-pc-windows-msvc-installer.manifest.json';
    const payload=Buffer.from('inert signed-installer observation');
    const manifest={schema:'airk-standalone-installer/1',commit:publication.commit,version:'1.8.35',target,artifact:name,artifact_sha256:sha(payload),payload:{'payload/service/airk.exe':'ab'.repeat(32),'payload/bin/airkctl.exe':'cd'.repeat(32)}};
    const manifestRaw=json(manifest);publication.assets[name]=sha(payload);publication.assets[manifestName]=sha(manifestRaw);
    const releaseRaw=json(publication),prefix='https://github.com/iokaio/airk/releases/download/v1.8.35/';
    const asset=(name:string,raw:Buffer)=>({name,size:raw.length,digest:'sha256:'+sha(raw),browser_download_url:prefix+name});
    const releases=[{tag_name:'v1.8.35',draft:false,prerelease:false,assets:[asset('release.json',releaseRaw),asset(manifestName,manifestRaw),asset(name,payload)]}];
    const responses=new Map<string,()=>Response>([
        ['https://api.github.com/repos/iokaio/airk/releases?per_page=20&page=1',()=>new Response(json(releases))],
        [prefix+'release.json',()=>new Response(releaseRaw)],
        [prefix+manifestName,()=>new Response(manifestRaw)],
        [prefix+name,()=>new Response(payload)],
        [`https://api.github.com/repos/iokaio/airk/attestations/sha256:${sha(payload)}?per_page=5&predicate_type=https%3A%2F%2Fslsa.dev%2Fprovenance%2Fv1`,()=>new Response(json({attestations:[{repository_id:1418150478,bundle:{fixture:true}}]}))],
    ]);
    const calls:string[]=[];let signatureValid=true;
    const verifier:VerifyHost={...systemVerifyHost(false),windows:async(paths:readonly string[],signed:string)=>{calls.push('signature:'+path.basename(signed));return{user:'S-1-5-21-fixture',acl:paths.map(item=>({path:item,owner:'S-1-5-21-fixture',allowed:['S-1-5-21-fixture']})),signature:{status:signatureValid?'Valid':'HashMismatch',subject:SIGNING_IDENTITY.windows.publisher,ekus:[SIGNING_IDENTITY.windows.identity_eku]}};},sha256:async(file:string)=>sha(await fs.readFile(file))};
    const ports:InstallerSystemPorts={platform:'win32',stagingRoot:directory,verifier,requester:async(input,init)=>{const url=String(input);calls.push('fetch:'+url);if(init?.signal?.aborted){throw new Error('cancelled');}const response=responses.get(url);if(!response){throw new Error('Unexpected fixture URL');}return response();},spawner:async(file,args)=>{calls.push('process:'+path.basename(file));return{exitCode:0,signal:null,stdout:Buffer.alloc(0),stderr:Buffer.alloc(0),timedOut:false,overflow:false};},verifyProvenance:(bundle,expected)=>{assert.deepEqual(bundle,{fixture:true});assert.equal(expected.sha256,sha(payload));assert.equal(expected.commit,publication.commit);calls.push('provenance');}};
    const system=new SystemInstaller('1.8.35',target,()=>{},()=>{},ports);
    return{directory,system,ports,calls,responses,name,prefix,payload,signature:(valid:boolean)=>{signatureValid=valid;},close:()=>fs.rm(directory,{recursive:true,force:true})};
}
const signal=()=>new AbortController().signal;

test('metadata/provenance discovery never downloads the installer; execution verifies same private bytes again',async()=>{
    const f=await fixture();try{
        const offer=await f.system.discover(signal());assert.ok(f.calls.includes('provenance'));assert.ok(!f.calls.includes('fetch:'+offer.url));
        const staged=await f.system.downloadAndVerify(offer,signal());
        assert.deepEqual(staged.images,{serviceSha256:'ab'.repeat(32),cliSha256:'cd'.repeat(32)});
        let admitted=0;assert.equal((await f.system.execute(staged,signal(),async()=>{admitted++;})).exitCode,0);
        assert.equal(admitted,1);assert.equal(f.calls.filter(call=>call==='signature:'+f.name).length,2);
        assert.equal(f.calls.filter(call=>call==='process:'+f.name).length,1);
    }finally{await f.close();}
});
test('changed private installer and platform signature refuse before native launch',async()=>{
    for(const changed of ['bytes','signature']){
        const f=await fixture();try{
            const offer=await f.system.discover(signal()),staged=await f.system.downloadAndVerify(offer,signal());
            if(changed==='bytes'){await fs.writeFile(path.join(staged.token,f.name),'changed');}else{f.signature(false);}
            let admitted=0;await assert.rejects(f.system.execute(staged,signal(),async()=>{admitted++;}));
            assert.equal(admitted,0);assert.ok(!f.calls.includes('process:'+f.name));
        }finally{await f.close();}
    }
});
test('wrong bytes and unexpected HTTPS redirect cannot become executable tokens',async()=>{
    for(const changed of ['digest','redirect']){
        const f=await fixture();try{
            const offer=await f.system.discover(signal());
            f.responses.set(offer.url,()=>changed==='digest'?new Response(Buffer.alloc(f.payload.length)):Response.redirect('https://evil.invalid/installer',302));
            await assert.rejects(f.system.downloadAndVerify(offer,signal()));
            assert.ok(!f.calls.some(call=>call.startsWith('fetch:https://evil.invalid')));
            assert.ok(!f.calls.includes('process:'+f.name));
        }finally{await f.close();}
    }
});
test('offline/rate-limit and disk-full observations retain an actionable failure without launch',async()=>{
    const f=await fixture();try{
        f.responses.set('https://api.github.com/repos/iokaio/airk/releases?per_page=20&page=1',()=>new Response('rate limited',{status:403}));
        await assert.rejects(f.system.discover(signal()),/403/);
        f.responses.delete('https://api.github.com/repos/iokaio/airk/releases?per_page=20&page=1');
        await assert.rejects(f.system.discover(signal()),/Unexpected fixture URL/);
        assert.ok(!f.calls.some(call=>call==='process:'+f.name));
    }finally{await f.close();}
    const full=await fixture();try{
        const ports={...full.ports,openFile:async()=>{throw Object.assign(new Error('fixture disk full'),{code:'ENOSPC'});}} as InstallerSystemPorts;
        const system=new SystemInstaller('1.8.35','x86_64-pc-windows-msvc',()=>{},()=>{},ports);
        const offer=await system.discover(signal());await assert.rejects(system.downloadAndVerify(offer,signal()),/disk full/);
        assert.ok(!full.calls.includes('process:'+full.name));
    }finally{await full.close();}
});

test('cancelled partial download removes only its originally created file and never launches',async()=>{
    const f=await fixture(),cancel=new AbortController();
    try{
        const ports={...f.ports,openFile:async(...args:Parameters<typeof fs.open>)=>{
            const handle=await fs.open(...args);
            return new Proxy(handle,{get(target,key){
                if(key==='writeFile'){return async(data:Uint8Array)=>{await target.writeFile(data);cancel.abort();};}
                const value=Reflect.get(target,key,target);return typeof value==='function'?value.bind(target):value;
            }});
        }} as InstallerSystemPorts;
        const system=new SystemInstaller('1.8.35','x86_64-pc-windows-msvc',()=>{},()=>{},ports);
        const offer=await system.discover(signal());
        f.responses.set(offer.url,()=>new Response(new ReadableStream({start(stream){stream.enqueue(f.payload.subarray(0,4));stream.enqueue(f.payload.subarray(4));stream.close();}})));
        await assert.rejects(system.downloadAndVerify(offer,cancel.signal),/cancelled/);
        const directories=await fs.readdir(f.directory);
        assert.equal(directories.length,1);
        assert.deepEqual(await fs.readdir(path.join(f.directory,directories[0])),[]);
        assert.ok(!f.calls.includes('process:'+f.name));
    }finally{await f.close();}
});
test('metadata response budget refuses before creating a staging directory',async()=>{
    const f=await fixture();try{
        f.responses.set('https://api.github.com/repos/iokaio/airk/releases?per_page=20&page=1',()=>new Response(Buffer.alloc(2*1024*1024+1)));
        await assert.rejects(f.system.discover(signal()),/bound/);assert.deepEqual(await fs.readdir(f.directory),[]);
    }finally{await f.close();}
});

test('Mac assessment requires a notarized Developer ID result with no timeout or output overflow',async()=>{
    for(const row of [
        {exitCode:0,timedOut:false,overflow:false,source:'source=Notarized Developer ID',accepted:true},
        {exitCode:0,timedOut:false,overflow:false,source:'source=Developer ID',accepted:false},
        {exitCode:0,timedOut:true,overflow:false,source:'source=Notarized Developer ID',accepted:false},
        {exitCode:1,timedOut:false,overflow:false,source:'source=Notarized Developer ID',accepted:false},
        {exitCode:0,timedOut:false,overflow:true,source:'source=Notarized Developer ID',accepted:false},
    ]){
        assert.equal(await assessMacInstaller(async(file,args,options)=>{
            assert.equal(file,'/usr/sbin/spctl');
            assert.deepEqual(args,['--assess','--type','open','--context','context:primary-signature','--verbose=2','/fixture/installer.dmg']);
            assert.equal(options.env.LC_ALL,'C');
            return{...row,signal:null,stdout:Buffer.alloc(0),stderr:Buffer.from('accepted\n'+row.source+'\n')};
        },'/fixture/installer.dmg','/fixture'),row.accepted);
    }
});
