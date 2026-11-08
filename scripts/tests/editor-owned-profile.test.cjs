// SPDX-License-Identifier: Apache-2.0
const test=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs/promises'),path=require('node:path'),os=require('node:os');
const {createOwnedProfile,finishOwnedProfile}=require('../editor-owned-profile.cjs');
test('Darwin profiles use a short canonical owned root and preserve logs before exact confirmed cleanup',async()=>{
    const root=await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(),'airk-profile-test-'))),run=path.join(root,'long-artifact-campaign-'.repeat(5));await fs.mkdir(run);
    let proof;
    try{
        // Inject only the temporary parent in this cross-platform filesystem test.
        // The actual Darwin runner uses the helper default /tmp, never os.tmpdir.
        proof=await createOwnedProfile(run,{platform:'darwin',temporaryRoot:os.tmpdir()});
        assert.equal(path.dirname(proof.profile),await fs.realpath(os.tmpdir()));assert.ok(Buffer.byteLength(proof.profile)<=70);assert.equal(proof.profile.startsWith(run+path.sep),false);
        if(process.platform!=='win32')assert.equal((await fs.stat(proof.profile)).mode&0o777,0o700);
        const pending=await finishOwnedProfile(proof,false);assert.equal(pending.removed,false);assert.ok(await fs.stat(proof.profile));
        await fs.mkdir(path.join(proof.profile,'logs'));await fs.writeFile(path.join(proof.profile,'logs','owned.log'),'actual owned profile log');
        const done=await finishOwnedProfile(proof,true);assert.equal(done.removed,true);assert.equal(done.logs_copied,1);assert.equal(await fs.readFile(path.join(run,'profile-logs','owned.log'),'utf8'),'actual owned profile log');
        await assert.rejects(fs.stat(proof.profile),{code:'ENOENT'});assert.equal(JSON.parse(await fs.readFile(path.join(run,'profile-owned.json'),'utf8')).profile,proof.profile);
    }finally{if(proof&&await fs.stat(proof.profile).catch(()=>undefined)){const result=await finishOwnedProfile(proof,true);assert.equal(result.removed,true);}await fs.rm(root,{recursive:true,force:true});}
});
test('malformed or changed ownership refuses removal, while Windows profiles remain artifact local',async()=>{
    const root=await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(),'airk-profile-proof-')));let proof,original;
    try{
        const run=path.join(root,'darwin');await fs.mkdir(run);proof=await createOwnedProfile(run,{platform:'darwin',temporaryRoot:os.tmpdir()});
        for(const profile of [proof.parent,path.parse(proof.profile).root,path.join(proof.parent,'unowned-profile')])assert.equal((await finishOwnedProfile({...proof,profile},true)).removed,false);
        const marker=path.join(proof.profile,'.airk-owned-profile.json');original=await fs.readFile(marker);await fs.writeFile(marker,'{}');
        const changed=await finishOwnedProfile(proof,true);assert.equal(changed.removed,false);assert.match(changed.retained_reason,/marker/);assert.ok(await fs.stat(proof.profile));await fs.writeFile(marker,original);
        const win=path.join(root,'windows');await fs.mkdir(win);const windows=await createOwnedProfile(win,{platform:'win32'});assert.equal(windows.profile,path.join(win,'profile'));assert.equal((await finishOwnedProfile(windows,true)).removed,false);assert.ok(await fs.stat(windows.profile));
    }finally{if(proof){if(original)await fs.writeFile(path.join(proof.profile,'.airk-owned-profile.json'),original);assert.equal((await finishOwnedProfile(proof,true)).removed,true);}await fs.rm(root,{recursive:true,force:true});}
});
