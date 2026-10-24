// SPDX-License-Identifier: Apache-2.0
// Real reviewed registration/stable-launcher test; only a private installation is changed.
const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const path=require('node:path');
const crypto=require('node:crypto');
const {RuntimeHost,nativeRunner}=require('../packages/vscode/out/serviceBootstrap.js');
const {probeMcp}=require('../packages/vscode/out/native.js');

(async()=>{
    assert.equal(process.argv[2],'--runtime');
    const payload=await fs.realpath(process.argv[3]),parent=await fs.realpath(path.join(__dirname,'../target'));
    const fixture=await fs.mkdtemp(path.join(parent,'airk-frontend-fixture-'));
    const install=path.join(fixture,'installation'),native=path.join(fixture,'native'),source=path.join(fixture,'source');
    const data=path.join(fixture,'new','data'),main=process.platform==='win32'?'airk.exe':'airk',helper=process.platform==='win32'?'airk-embed.exe':'airk-embed';
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
        await fs.mkdir(source);const original='Original ivory signal evidence.\n';await fs.writeFile(path.join(source,'memo.txt'),original);
        const review=await runtime.reviewProject(binding,source,data,'frontend-fixture');
        await assert.rejects(fs.stat(data),{code:'ENOENT'});
        await runtime.registerProject(binding,review);assert.equal((await fs.stat(data)).isDirectory(),true);
        console.log('Reviewed registration created only its reserved data path.');
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
        const first=await probeMcp(spec,15,inventory.version);assert.equal(first.tools.length,15);assert.notEqual(first.status.state,'not_indexed');
        assert.equal(await fs.readFile(path.join(source,'memo.txt'),'utf8'),original);
        await fs.rename(source,path.join(fixture,'temporarily-offline-source'));
        const retained=await probeMcp(spec,15,inventory.version);assert.notEqual(retained.status.state,'not_indexed');
        console.log('Stable-launcher MCP served the registered publication with sources present and unavailable.');
    }finally{
        client?.dispose();
        if(staged)await nativeRunner(path.join(native,'runtime',main),['runtime','uninstall','--install-dir',install],180000);
        assert.equal(path.dirname(path.resolve(fixture)),parent);assert.ok(path.basename(fixture).startsWith('airk-frontend-fixture-'));
        await fs.rm(fixture,{recursive:true,force:true});
    }
})().catch(error=>{console.error(error);process.exitCode=1;});
