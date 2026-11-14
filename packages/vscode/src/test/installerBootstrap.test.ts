// SPDX-License-Identifier: Apache-2.0
import { test } from 'node:test';
import * as assert from 'node:assert/strict';
import * as fs from 'node:fs';
import * as fsp from 'node:fs/promises';
import * as os from 'node:os';
import { spawnBounded } from '../airkctl/runner';
import * as path from 'node:path';
import { InstallerBootstrap, nativeCompletionMatches, type BootstrapHost, type InstallerOffer } from '../airkctl/bootstrap/controller';
import { selectRelease, selectInstaller, targetOf, validatePublication, approvedRedirect } from '../airkctl/bootstrap/release';

const offer: InstallerOffer = {version:'1.8.35',target:'x86_64-pc-windows-msvc',name:'fixture.exe',url:'https://example.invalid/fixture.exe',sha256:'a'.repeat(64),bytes:5,signature:'authenticode',headlessArguments:['--silent']};
function fixture() {
    const calls: string[] = [];
    const host: BootstrapHost = {
        admission:async () => {calls.push('admission'); return {discovery:'absent',supported:true,trusted:true,nativeAllowed:true,setupActive:false};},
        manual:async () => {calls.push('manual');},
        discover:async () => {calls.push('metadata');return offer;},
        review:async () => {calls.push('review');return true;},
        downloadAndVerify:async selected => {calls.push('payload+verify');return {offer:selected,token:'private',images:{serviceSha256:'b'.repeat(64),cliSha256:'c'.repeat(64)}};},
        execute:async () => {calls.push('execute');return {exitCode:0,receipt:'private/receipt'};},
        confirmNativeSuccess:async () => {calls.push('native-receipt');return true;},
        rediscoverAndHealth:async () => {calls.push('verified+health');return true;},
        result:outcome => {calls.push('result:'+outcome.kind);},
    };
    return {host,calls};
}

test('manual and defer never discover or download',async () => {
    const {host,calls}=fixture();const bootstrap=new InstallerBootstrap(host);
    assert.equal((await bootstrap.choose('manual')).kind,'manual');
    assert.equal((await bootstrap.choose('defer')).kind,'deferred');
    assert.deepEqual(calls,['manual']);
});
test('explicit review precedes payload and fresh admission precedes execution',async () => {
    const {host,calls}=fixture();assert.equal((await new InstallerBootstrap(host).choose('automatic')).kind,'ready');
    assert.deepEqual(calls,['admission','metadata','review','payload+verify','admission','execute','native-receipt','verified+health','result:ready']);
});
test('unknown, installed, unsupported, untrusted, disabled and active setup refuse before network',async () => {
    for (const changed of [{discovery:'unknown' as const},{discovery:'installed' as const},{supported:false},{trusted:false},{nativeAllowed:false},{setupActive:true}]) {
        const {host,calls}=fixture();const admission=host.admission;
        host.admission=async () => ({...await admission(),...changed});
        assert.equal((await new InstallerBootstrap(host).choose('automatic')).kind,'refused');
        assert.deepEqual(calls,['admission','result:refused']);
    }
});
test('rejected review and cancellation never execute',async () => {
    const {host,calls}=fixture();host.review=async () => false;
    assert.equal((await new InstallerBootstrap(host).choose('automatic')).kind,'cancelled');
    assert.deepEqual(calls,['admission','metadata','result:cancelled']);
});
test('installation appearing while downloading refuses replacement',async () => {
    const {host,calls}=fixture();let count=0;const admission=host.admission;
    host.admission=async () => ({...await admission(),discovery:++count===1?'absent':'installed'});
    assert.equal((await new InstallerBootstrap(host).choose('automatic')).kind,'refused');assert.ok(!calls.includes('execute'));
});
test('nonzero installer and zero without verified health preserve failure evidence',async () => {
    for (const exitCode of [0,11]) {
        const {host}=fixture();host.execute=async () => ({exitCode,receipt:'private/recovery'});host.rediscoverAndHealth=async () => false;
        const result=await new InstallerBootstrap(host).choose('automatic');
        assert.equal(result.kind,'failed');assert.equal('receipt' in result&&result.receipt,'private/recovery');
    }
});
test('single window refuses concurrent attempts',async () => {
    const {host}=fixture();let unblock!:()=>void;let entered!:()=>void;
    const reviewed=new Promise<void>(resolve=>{entered=resolve;});
    host.review=() => new Promise(resolve => {unblock=()=>resolve(false);entered();});
    const bootstrap=new InstallerBootstrap(host);const pending=bootstrap.choose('automatic');
    await reviewed;
    assert.equal((await bootstrap.choose('automatic')).kind,'refused');
    bootstrap.cancel();unblock();
    await pending;
});
test('release selection pins official exact compatible stable release',() => {
    const asset=(name:string,size=5)=>({name,size,digest:'sha256:'+'a'.repeat(64),browser_download_url:`https://github.com/iokaio/airk/releases/download/v1.8.35/${name}`});
    const target='x86_64-pc-windows-msvc';const name=`airk-1.8.35-${target}-installer.exe`,manifest=`airk-1.8.35-${target}-installer.manifest.json`;
    const release=selectRelease([{tag_name:'v1.8.35',draft:false,prerelease:false,assets:[asset(name),asset(manifest)]}],'1.8.35');
    const record={commit:'c'.repeat(40),tree:'d'.repeat(40),run_id:'1',attempt:'1',producer:{commit:'c'.repeat(40)},schema:'airk-github-release/2',tag:'v1.8.35',package_version:'1.8.35',channel:'stable',assets:{[name]:'a'.repeat(64),[manifest]:'a'.repeat(64)},installers:{[target]:{artifact:name,manifest,headless:{arguments:['--silent'],stdin:'closed',prompts:false,certificate_trust:'skip',requires_reset_acknowledgement:true,success_requires:'native_setup_and_readiness'}}}};
    const completeAssets: Record<string,string> = {[`airk-1.8.35.vsix`]:'a'.repeat(64),[`airk-1.8.35.vsix-manifest.json`]:'a'.repeat(64)};
    const completeInstallers: Record<string,unknown> = {};
    for (const [platform,suffix] of [['aarch64-apple-darwin','dmg'],['x86_64-apple-darwin','dmg'],['x86_64-pc-windows-msvc','exe'],['x86_64-unknown-linux-gnu','tar.gz']]) {
        const prefix=`airk-1.8.35-${platform}`;
        for (const suffixName of ['.zip','.native-manifest.json','-portable.zip','-portable.manifest.json',`-installer.${suffix}`,'-installer.manifest.json']) {completeAssets[prefix+suffixName]='a'.repeat(64);}
        completeInstallers[platform]={...record.installers[target],artifact:prefix+`-installer.${suffix}`,manifest:prefix+'-installer.manifest.json'};
    }
    const valid={...record,assets:completeAssets,installers:completeInstallers};
    assert.equal(selectInstaller(release,valid,target).name,name);
    for (const mutation of [{...valid,schema:'airk-github-release/1'},{...valid,channel:'preview'},{...valid,tag:'v1.8.36'},{...valid,assets:{}}]) {assert.throws(()=>selectInstaller(release,mutation,target));}
    assert.throws(()=>selectRelease([{tag_name:'v1.8.35',draft:true,prerelease:false,assets:[]}],'1.8.35'));
    assert.equal(targetOf('linux','arm64'),undefined);assert.equal(targetOf('darwin','arm64'),'aarch64-apple-darwin');
});

test('publication decoder agrees with all independent Python-owner release cases',()=>{
    const source=JSON.parse(fs.readFileSync(path.resolve(__dirname,'../../../../fixtures/split/receipts/publication_2.json'),'utf8')) as {cases:{kind:string;name:string;input:unknown;accepted:boolean}[]};
    const cases=source.cases.filter(row=>row.kind==='release');
    assert.equal(cases.length,15);
    for (const row of cases) {
        let accepted=true;try {validatePublication(row.input);}catch {accepted=false;}
        assert.equal(accepted,row.accepted,row.name);
    }
});
test('cancellation after download prevents launching any installer',async()=>{
    const {host,calls}=fixture();const bootstrap=new InstallerBootstrap(host);
    const original=host.downloadAndVerify;
    host.downloadAndVerify=async (offer,signal)=>{const result=await original(offer,signal);bootstrap.cancel();return result;};
    assert.equal((await bootstrap.choose('automatic')).kind,'cancelled');
    assert.ok(!calls.includes('execute'));
});

test('healthy files cannot substitute for missing native completion receipt',async()=>{
    const {host,calls}=fixture();host.confirmNativeSuccess=async()=>false;
    assert.equal((await new InstallerBootstrap(host).choose('automatic')).kind,'failed');
    assert.ok(!calls.includes('verified+health'));
});

test('approved GitHub redirects reject downgrade, arbitrary hosts and wrong repository paths',()=>{
    const initial='https://github.com/iokaio/airk/releases/download/v1.8.35/release.json';
    assert.equal(approvedRedirect(initial,initial),true);
    assert.equal(approvedRedirect(initial,'https://release-assets.githubusercontent.com/github-production-release-asset/123/456?token=opaque'),true);
    for(const candidate of ['http://release-assets.githubusercontent.com/github-production-release-asset/123/456','https://evil.invalid/file','https://github.com/other/airk/releases/download/v1.8.35/release.json','https://release-assets.githubusercontent.com/arbitrary/file','https://user:password@release-assets.githubusercontent.com/github-production-release-asset/1/2','https://objects.githubusercontent.com/arbitrary/file']) {assert.equal(approvedRedirect(initial,candidate),false,candidate);}
    assert.equal(approvedRedirect('https://evil.invalid/file','https://evil.invalid/file'),false);
});
test('process output observation limit preserves an admitted native installer',async()=>{
    const root=await fsp.mkdtemp(path.join(os.tmpdir(),'airk-bootstrap-process-'));
    const trigger=path.join(root,'trigger'),proof=path.join(root,'proof');
    const cancellation=new AbortController();
    const events=fsp.watch(root,{signal:AbortSignal.any([cancellation.signal,AbortSignal.timeout(5000)])});
    const observed=(async()=>{for await(const event of events){if(event.filename==='proof'){return await fsp.readFile(proof,'utf8');}}throw new Error('No process ownership proof');})();
    const script=`const fs=require('node:fs');const watcher=fs.watch(${JSON.stringify(root)},(_event,name)=>{if(name==='trigger'){watcher.close();fs.writeFileSync(${JSON.stringify(proof)},'native process retained');process.exit(0);}});process.stdout.write('admitted');`;
    try{
        const result=await spawnBounded(process.execPath,['-e',script],{env:process.env,timeoutMs:5000,maxStdout:1,maxStderr:1024,preserveProcessOnLimit:true});
        assert.equal(result.overflow,true);assert.equal(result.exitCode,null);
        await fsp.writeFile(trigger,'continue');
        assert.equal(await observed,'native process retained');
    }finally{cancellation.abort();await fsp.writeFile(trigger,'continue').catch(()=>{});await fsp.rm(root,{recursive:true,force:true});}
});

test('native-owned successful receipt summary binds both current installation identities',{skip:!process.env.AIRK_SETUP_RECEIPT_CASE_INPUT},()=>{
    const native=JSON.parse(fs.readFileSync(process.env.AIRK_SETUP_RECEIPT_CASE_INPUT!,'utf8')) as {cases:{name:string;observation:unknown;expected_successful:boolean}[]};
    assert.equal(native.cases.length,7);
    const expected={operationId:'01'.repeat(16),installId:'02'.repeat(16)};
    for(const row of native.cases){assert.equal(nativeCompletionMatches(row.observation,expected),row.expected_successful,row.name);}
    const success=native.cases.find(row=>row.expected_successful)!.observation;
    assert.equal(nativeCompletionMatches(success,{...expected,operationId:'03'.repeat(16)}),false);
    assert.equal(nativeCompletionMatches(success,{...expected,installId:'04'.repeat(16)}),false);
});

test('publication /3 matches exact opaque native owner and preserves historical /2 semantics',()=>{
    const source=JSON.parse(fs.readFileSync(path.resolve(__dirname,'../../../../fixtures/split/receipts/publication_3.json'),'utf8')) as {owner:unknown;cases:{kind:string;name:string;input:unknown;accepted:boolean}[]};
    const cases=source.cases.filter(row=>row.kind==='release');
    assert.equal(cases.length,10);
    for(const row of cases){let accepted=true;try{validatePublication(row.input,source.owner);}catch{accepted=false;}assert.equal(accepted,row.accepted,row.name);}
    const accepted=cases.find(row=>row.accepted && (row.input as {channel:string}).channel==='stable')!.input;
    assert.throws(()=>validatePublication(accepted,null),'missing native production capture refuses automatic /3');
    const row=accepted as {tag:string;package_version:string;assets:Record<string,string>};
    const target='x86_64-pc-windows-msvc',name=`airk-${row.package_version}-${target}-installer.exe`;
    const release=selectRelease([{tag_name:row.tag,draft:false,prerelease:false,assets:[{name,size:5,digest:'sha256:'+row.assets[name],browser_download_url:`https://github.com/iokaio/airk/releases/download/${row.tag}/${name}`}]}],row.package_version);
    assert.equal(selectInstaller(release,accepted,target,source.owner).name,name);
    assert.throws(()=>selectInstaller(release,accepted,target,null));
});

test('release bundle binds exact owning source bytes and refuses absent production authority',()=>{
    const builder=require('../../scripts/build-bootstrap.cjs') as {compileRelease():Buffer;verifyReleaseBundle(bytes:Buffer,required?:boolean):void};
    const bytes=builder.compileRelease();
    builder.verifyReleaseBundle(bytes,false);
    const {spawnSync}=require('node:child_process') as typeof import('node:child_process');
    const file=path.resolve(__dirname,'../../scripts/build-bootstrap.cjs');
    const other=spawnSync(process.execPath,['-e',`process.stdout.write(require(${JSON.stringify(file)}).compileRelease())`],{cwd:path.resolve(__dirname,'../..'),maxBuffer:4*1024*1024});
    assert.equal(other.status,0,other.stderr.toString());assert.deepEqual(other.stdout,bytes,'npm and repository working directories produce identical sealed bytes');
    assert.throws(()=>builder.verifyReleaseBundle(Buffer.concat([bytes,Buffer.from('\n/* changed after compilation */\n')]),false));
    if(!fs.existsSync(path.resolve(__dirname,'../../../../fixtures/arbiter/publication_catalog.json'))){assert.throws(()=>builder.verifyReleaseBundle(bytes,true));}
});

test('provenance owner rejects valid-JavaScript body changes and static imports after compilation',()=>{
    const builder=require('../../scripts/build-bootstrap.cjs') as {verifyBundle(bytes:Buffer):void};
    const bytes=fs.readFileSync(path.resolve(__dirname,'../airkctl/bootstrap/provenance.js'));
    builder.verifyBundle(bytes);
    const {spawnSync}=require('node:child_process') as typeof import('node:child_process');
    const verified=spawnSync(process.execPath,[path.resolve(__dirname,'../../scripts/build-bootstrap.cjs'),'--verify-stdin'],{cwd:path.resolve(__dirname,'../..'),input:bytes,maxBuffer:4*1024*1024});
    assert.equal(verified.status,0,verified.stderr.toString());assert.equal(verified.stdout.toString(),'verified\n');
    assert.throws(()=>builder.verifyBundle(Buffer.concat([bytes,Buffer.from('\n/* changed after sealed compilation */\n')])));
    assert.throws(()=>builder.verifyBundle(Buffer.concat([bytes,Buffer.from('\nimport "node:child_process";\n')])));
});
