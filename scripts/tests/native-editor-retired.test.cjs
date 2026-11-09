// SPDX-License-Identifier: Apache-2.0
const {test}=require('node:test'),assert=require('node:assert/strict');
const {createHash}=require('node:crypto'),path=require('node:path');
const {qualifyRetiredLauncher,verifyRetiredPaths,requireLegacyInput}=require('../native-editor-legacy.cjs');
const fs=require('node:fs/promises'),os=require('node:os');
const sha=text=>createHash('sha256').update(text).digest('hex');
function fixture(){
    const install=path.resolve('artifacts/fictional/install'),old='a'.repeat(64),candidate='b'.repeat(64);
    const metadata={schema:1,version:'1.8.8',os:'windows',arch:'x86_64',airk_sha256:'c'.repeat(64),embed_sha256:'d'.repeat(64)};
    const runtimeText=JSON.stringify(metadata),manifestText=JSON.stringify({files:{'airk.exe':metadata.airk_sha256,'airk-embed.exe':metadata.embed_sha256,'airk-runtime.json':sha(runtimeText)}});
    const previous=sha(manifestText),command=path.join(install,'airk-launcher.exe'),commandSha='e'.repeat(64);
    const snapshot={schema:1,manifest_text:manifestText,runtime_text:runtimeText,launcher_receipt_text:JSON.stringify({schema:1,sha256:commandSha}),command_sha256:commandSha,
        native_inspect:{payload_verified:true,approval:{manifest_sha256:previous},selection:{active:previous},launcher:command,executable:path.join(install,'runtimes',previous,'airk.exe')}};
    const processes={remaining:0,scope:'verified_launcher_images'};
    const retirement={launchers:{active:candidate,legacy_launchers_retired:true,launcher:null,universal_launcher:null,processes},
        runtimes:{active:candidate,complete:true,selected_service_restarted:false,retired_runtimes:[previous],processes:{remaining:0,scope:'verified_superseded_runtime_images'}}};
    const value={install_dir:install,candidate:{install_manifest_sha256:candidate},predecessor:{install_manifest_sha256:previous},legacy_config:{binding_kind:'retired_installation_launcher',command},native:{obsolete_cleanup:retirement,parsed_evidence:[{value:structuredClone(retirement)}]}};
    return {value,snapshot,metadata};
}
test('retired positive provenance uses JSON ownership evidence without a launcher executable',()=>{
    const {value,snapshot,metadata}=fixture();assert.deepEqual(qualifyRetiredLauncher(value,snapshot),metadata);
});
for(const mutation of [
    f=>f.snapshot.command_sha256='f'.repeat(64),
    f=>f.snapshot.native_inspect.launcher=path.resolve('foreign/airk-launcher.exe'),
    f=>f.snapshot.native_inspect.selection.active='f'.repeat(64),
    f=>f.snapshot.native_inspect.executable=path.resolve('foreign/airk.exe'),
    f=>f.snapshot.manifest_text+=' ',
    f=>f.value.native.obsolete_cleanup.launchers.processes.remaining=1,
    f=>f.value.native.obsolete_cleanup.runtimes.selected_service_restarted=true,
    f=>f.value.native.obsolete_cleanup.runtimes.retired_runtimes=[],
])test('changed provenance or incomplete native retirement fails closed '+mutation.toString(),()=>{
    const f=fixture();mutation(f);assert.throws(()=>qualifyRetiredLauncher(f.value,f.snapshot));
});


test('missing selected legacy input fails before any editor work',()=>{
    assert.throws(()=>requireLegacyInput(['same','legacy'],{}),/before any editor run/);
    requireLegacyInput(['same'],{});
    requireLegacyInput(['legacy'],{AIRK_EDITOR_LEGACY_FIXTURE:'owned/fixture.json'});
});

test('actual filesystem must contain only the candidate and no retired launcher or receipt',async()=>{
    const owner=await fs.mkdtemp(path.join(os.tmpdir(),'airk-retired-contract-'));
    try{
        const f=fixture(),install=path.join(owner,'install');f.value.install_dir=install;
        f.value.legacy_config.command=path.join(install,'airk-launcher.exe');
        const runtimes=path.join(install,'runtimes');
        await fs.mkdir(path.join(runtimes,f.value.candidate.install_manifest_sha256),{recursive:true});
        await verifyRetiredPaths(f.value);
        for(const file of [f.value.legacy_config.command,path.join(install,'launcher.json'),path.join(runtimes,f.value.predecessor.install_manifest_sha256)]){
            await fs.writeFile(file,'retained executable or receipt');
            await assert.rejects(verifyRetiredPaths(f.value),/still exists/);
            await fs.unlink(file);
        }
        const extra=path.join(runtimes,'f'.repeat(64));await fs.mkdir(extra);
        await assert.rejects(verifyRetiredPaths(f.value),/Superseded payload/);
    }finally{await fs.rm(owner,{recursive:true,force:true});}
});


test('unretained or changed cleanup JSON cannot stand in for hashed native retirement evidence',()=>{
    for(const mutate of [f=>f.value.native.parsed_evidence=[],f=>f.value.native.parsed_evidence[0].value.runtimes.retired_runtimes=[]]){
        const f=fixture();mutate(f);assert.throws(()=>qualifyRetiredLauncher(f.value,f.snapshot));
    }
});
