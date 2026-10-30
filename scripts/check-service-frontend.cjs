// SPDX-License-Identifier: Apache-2.0
// Real reviewed registration/stable-launcher test; only a private installation is changed.
const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const path=require('node:path');
const crypto=require('node:crypto');
const {RuntimeHost,nativeRunner}=require('../packages/vscode/out/serviceBootstrap.js');
const {probeMcp,runText}=require('../packages/vscode/out/native.js');

(async()=>{
    assert.equal(process.argv[2],'--runtime');
    const cli=process.argv[4]==='--cli';assert.ok(process.argv[4]===undefined||cli);
    const payload=await fs.realpath(process.argv[3]),parent=await fs.realpath(path.join(__dirname,'../target'));
    const fixture=await fs.mkdtemp(path.join(parent,'airk-frontend-fixture-'));
    const install=path.join(fixture,'installation'),native=path.join(fixture,'native'),sourceParent=path.join(fixture,'source-parent'),source=path.join(sourceParent,'source');
    const data=path.join(fixture,'new','data'),main=process.platform==='win32'?'airk.exe':process.platform==='darwin'?'AIRK.app/Contents/MacOS/airk':'airk',helper=process.platform==='win32'?'airk-embed.exe':process.platform==='darwin'?'AIRK.app/Contents/MacOS/airk-embed':'airk-embed';
    await fs.mkdir(native);await fs.cp(payload,path.join(native,'runtime'),{recursive:true,errorOnExist:true,force:false});
    const bytes=await fs.readFile(path.join(native,'runtime','airk-install.json')),inventory=JSON.parse(bytes);
    const digest=crypto.createHash('sha256').update(bytes).digest('hex');
    await fs.writeFile(path.join(native,'manifest.json'),JSON.stringify({schema:2,version:inventory.version,vsix_target:`${process.platform}-${process.arch}`,
        runtime_dir:'runtime',install_manifest_sha256:digest,payload:{[main]:inventory.files[main],[helper]:inventory.files[helper]}}));
    const runtime=new RuntimeHost(fixture,inventory.version,{channel:'stable',installDir:install});
    let staged=false,client;
    try{
        const bundled=(await runtime.discover()).bundle;
        await nativeRunner(bundled.main,['runtime','stage','--source',bundled.source,'--install-dir',install,'--state-dir',path.join(fixture,'state'),
            '--approved-sha256',digest,'--verification-basis','trusted-distribution'],180000);staged=true;
        await nativeRunner(bundled.main,['runtime','activate','--install-dir',install,'--runtime-sha256',digest],180000);
        const control=await fs.realpath(path.join(fixture,'state','control'));
        const binding={installDir:install,controlDir:control,executable:path.join(install,'runtimes',digest,main),
            launcher:path.join(install,process.platform==='win32'?'airk-launcher.exe':'airk-launcher'),runtimeSha256:digest,
            version:inventory.version,lifecycle:'on-demand',verificationBasis:'trusted_distribution'};
        await fs.mkdir(source,{recursive:true});const original='Original ivory signal evidence.\n';await fs.writeFile(path.join(source,'memo.txt'),original);
        const review=await runtime.reviewProject(binding,source,data,'frontend-fixture');
        await assert.rejects(fs.stat(data),{code:'ENOENT'});
        await runtime.registerProject(binding,review);assert.equal((await fs.stat(data)).isDirectory(),true);
        console.log('Reviewed registration created only its reserved data path.');
        const beforeIndex=await nativeRunner(binding.executable,['--service-install-dir',install,'--service-project',review.binding,'tools','call','airk_diagnostics'],180000);
        assert.notEqual(beforeIndex.isError,true);const startup=JSON.parse(beforeIndex.content.find(part=>part.type==='text').text);
        assert.equal(startup.index_state,'unavailable');assert.equal(startup.publication,null);assert.equal(startup.files,null);
        assert.equal(startup.runtime.service.version,inventory.version);assert.ok(startup.work);
        console.log('Service/job diagnostics remain available before the first index publication.');
        client=runtime.client(binding);
        const intent=await client.prepareJob(review.binding,'index'),accepted=await client.submitJob(intent);
        const deadline=Date.now()+90000;
        for(;;){
            const result=await client.rpc({job:{id:accepted.job.id}},'job');
            if(result.job.state==='succeeded')break;
            assert.ok(!['failed','canceled'].includes(result.job.state),JSON.stringify(result));
            assert.ok(Date.now()<deadline,'Fixture index deadline exceeded');await new Promise(resolve=>setTimeout(resolve,100));
        }
        const spec={command:binding.launcher,root:review.root,dataDir:review.data_root,collection:review.collection,name:'Frontend fixture',
            service:{installDir:install,project:review.binding}};
        const first=await probeMcp(spec,28,inventory.version);assert.equal(first.tools.length,28);assert.notEqual(first.status.state,'not_indexed');
        const quick=JSON.parse(first.status.content.find(part=>part.type==='text').text);
        assert.equal(quick.runtime,undefined);assert.equal(quick.work,undefined);
        const detailed=await nativeRunner(binding.executable,['--service-install-dir',install,'--service-project',review.binding,'tools','call','airk_diagnostics'],180000);
        assert.notEqual(detailed.isError,true);
        const observed=JSON.parse(detailed.content.find(part=>part.type==='text').text);
        assert.equal(observed.index_state,'available');assert.ok(observed.publication.token);
        assert.equal(observed.runtime.service.version,inventory.version);assert.equal(observed.runtime.versions_match,true);
        assert.equal(observed.runtime.service.responsive,true);assert.equal(observed.work.serving_worker.project,review.binding);
        assert.equal(typeof observed.work.watch.enabled,'boolean');assert.ok(Array.isArray(observed.work.queued_jobs));
        const build=JSON.parse(await fs.readFile(path.join(payload,'airk-build.json'),'utf8'));
        assert.equal(observed.runtime.service.build.source_sha256,build.source.sha256);
        console.log('Runtime/build identities and scoped work/watcher observations verified.');
        if(cli){
            const context=['--service-install-dir',install,'--service-project',review.binding];
            const help=await nativeRunner(binding.executable,['tools','call','airk_help','--args',JSON.stringify({topic:'job-management'})],180000);
            assert.notEqual(help.isError,true);const guidance=JSON.parse(help.content.find(part=>part.type==='text').text);
            assert.equal(guidance.version,inventory.version);assert.ok(guidance.content.includes('airk pause'));
            const commandHelp=await nativeRunner(binding.executable,['tools','call','airk_help','--args',JSON.stringify({command:'runtime retire-previous'})],180000);
            assert.ok(JSON.parse(commandHelp.content.find(part=>part.type==='text').text).content.includes('--expected-active'));
            const result=await nativeRunner(binding.executable,[...context,'tools','call','airk_status'],180000);
            assert.notEqual(result.isError,true);assert.ok(result.content.some(part=>part.type==='text'));
            const prepared=await nativeRunner(binding.executable,['service','requests','--runtime-dir',control,'prepare','--project',review.binding],180000);
            assert.equal(prepared.submitted,false);assert.match(prepared.prepared,/^[0-9a-f]{32}$/);
            const pendingPath=path.join(control,'client-requests',`${prepared.prepared}.json`);
            const pending=JSON.parse(await fs.readFile(pendingPath,'utf8'));
            const accepted=await client.rpc(pending.request,'job');
            // The service accepted it, while the prepared client's saved record still exists:
            // this is the exact durable state after losing its acknowledgment.
            const recovered=await nativeRunner(binding.executable,['service','requests','--runtime-dir',control,'retry',prepared.prepared],180000);
            assert.equal(recovered.job.id,accepted.job.id);await assert.rejects(fs.stat(pendingPath),{code:'ENOENT'});
            const shell=await runText(binding.executable,[...context,'tools','shell'],path.dirname(binding.executable),180000,undefined,undefined,'call airk_status\nexit\n');
            assert.match(shell,/Result:/);assert.ok(shell.includes(review.collection));
            console.log('Direct CLI tool/shell calls passed; saved acknowledgment recovery returned the original native job.');
        }
        assert.equal(await fs.readFile(path.join(source,'memo.txt'),'utf8'),original);
        // An unavailable ancestor is not a confirmed deleted source leaf.
        await fs.rename(sourceParent,path.join(fixture,'temporarily-offline-parent'));
        const retained=await probeMcp(spec,28,inventory.version);assert.notEqual(retained.status.state,'not_indexed');
        const offline=await nativeRunner(binding.executable,['--service-install-dir',install,'--service-project',review.binding,'tools','call','airk_diagnostics'],180000);
        assert.notEqual(offline.isError,true);
        const saved=JSON.parse(offline.content.find(part=>part.type==='text').text);
        assert.equal(saved.index_state,'available');assert.equal(saved.publication.token,observed.publication.token);
        console.log('Stable-launcher MCP served the registered publication with sources present and unavailable.');
    }finally{
        client?.dispose();
        if(staged)await nativeRunner(path.join(native,'runtime',main),['runtime','uninstall','--install-dir',install],180000);
        assert.equal(path.dirname(path.resolve(fixture)),parent);assert.ok(path.basename(fixture).startsWith('airk-frontend-fixture-'));
        await fs.rm(fixture,{recursive:true,force:true});
    }
})().catch(error=>{console.error(error);process.exitCode=1;});
