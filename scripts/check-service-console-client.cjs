// SPDX-License-Identifier: Apache-2.0
// Real native/client/controller fixture. No editor, login registration, VSIX or user installation.
const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const path=require('node:path');
const crypto=require('node:crypto');
const {RuntimeHost,nativeRunner}=require('../packages/vscode/out/serviceBootstrap.js');
const {ServiceConsoleController}=require('../packages/vscode/out/serviceConsoleController.js');

(async()=>{
    assert.equal(process.argv[2],'--runtime','Supply --runtime <host-native pair>');
    const payload=await fs.realpath(process.argv[3]);
    const parent=await fs.realpath(path.join(__dirname,'../target'));
    const fixture=await fs.mkdtemp(path.join(parent,'airk-console-client-fixture-'));
    assert.equal(path.dirname(fixture),parent);
    assert.ok(path.basename(fixture).startsWith('airk-console-client-fixture-'));
    const native=path.join(fixture,'native'),source=path.join(fixture,'source'),data=path.join(fixture,'data');
    await fs.mkdir(native);await fs.cp(payload,path.join(native,'runtime'),{recursive:true,errorOnExist:true,force:false});
    const bytes=await fs.readFile(path.join(native,'runtime','airk-install.json')),inventory=JSON.parse(bytes);
    const main=process.platform==='win32'?'airk.exe':'airk',helper=process.platform==='win32'?'airk-embed.exe':'airk-embed';
    const digest=crypto.createHash('sha256').update(bytes).digest('hex');
    await fs.writeFile(path.join(native,'manifest.json'),JSON.stringify({schema:2,version:inventory.version,vsix_target:`${process.platform}-${process.arch}`,
        runtime_dir:'runtime',install_manifest_sha256:digest,payload:{[main]:inventory.files[main],[helper]:inventory.files[helper]}}));
    const values=new Map(),copies=[],install=path.join(fixture,'installation');let trusted=true,binding,removed=false,missingHelper;
    const runtime=new RuntimeHost(fixture,inventory.version,{channel:'stable',installDir:install},undefined,()=>trusted);
    const controller=new ServiceConsoleController({version:inventory.version,authority:'Owned native controller fixture',runtime,
        trusted:()=>trusted,storage:{keys:()=>[...values.keys()],get:key=>values.get(key),update:async(key,value)=>{if(value===undefined)values.delete(key);else values.set(key,structuredClone(value));}},
        changed(){},confirm:async()=>true,copy:async text=>{copies.push(text);},logs:async()=>{},resources:async()=>undefined,
        lifecycle:(selected,intent,force)=>runtime.lifecycle(selected,intent,force),abandonLifecycle:(selected,intent,id)=>runtime.abandonLifecycle(selected,intent,id)});
    const action=async(name,extra={})=>{
        await controller.action({type:'service',revision:controller.state.revision,action:name,...extra});
        if(controller.state.message&&!controller.state.message.startsWith('Repair preview changed'))throw new Error(controller.state.message);
    };
    const wait=async(predicate,label)=>{
        const deadline=Date.now()+90000;
        while(!predicate()){
            assert.ok(Date.now()<deadline,label);await new Promise(resolve=>setTimeout(resolve,150));await controller.refresh();
        }
    };
    try{
        await controller.refresh();assert.equal(controller.state.connected,false);
        await action('start');assert.equal(controller.state.connected,true);
        binding=runtime.binding((await runtime.discover()).inspection,install);
        await fs.mkdir(source);await fs.mkdir(data);
        const original='Original copper beacon console controller fixture.\n';await fs.writeFile(path.join(source,'memo.txt'),original);
        const registered=await nativeRunner(binding.executable,['--root',source,'--data-dir',data,'--collection','console-controller-fixture','service','register','--runtime-dir',binding.controlDir]);
        const project=registered.project.registration.binding;
        await controller.refresh();await action('index',{project});
        await wait(()=>controller.state.page?.projects[0]?.progress?.job?.state==='succeeded','Native source receipt did not complete');
        assert.equal(controller.state.pending.length,0);
        await action('repair',{project,scope:'one',operation:'auto'});
        await wait(()=>controller.state.plan?.value.phase==='ready','Native repair preview did not become ready');
        assert.equal(controller.state.plan.items.length,1);assert.equal(typeof controller.state.plan.items[0].action,'string');
        await action('approvePlan',{allowFallback:false});
        await wait(()=>controller.state.plan?.value.phase==='finished','Approved repair did not finish');
        assert.equal(controller.state.plan.value.counts.succeeded,1);assert.equal(controller.state.plan.value.outcome,'succeeded');
        await action('refreshPlan');assert.equal(controller.state.plan.items[0].phase,'succeeded');
        console.log('Real controller indexing, persisted native requests, guarded repair preview and terminal receipts passed.');

        await action('previewReport',{includePaths:false});const preview=controller.state.preview;
        assert.ok(preview);assert.ok(!preview.includes(source)&&!preview.includes(original));
        await controller.refresh();assert.equal(controller.state.preview,preview);
        await action('copyReport');assert.equal(copies[0],preview);
        trusted=false;await controller.action({type:'service',revision:controller.state.revision,action:'index',project});
        assert.equal(controller.state.message,'runtime_client_no_longer_authorized');trusted=true;
        const oldInstance=controller.state.health.value.observation.instance;
        await action('stop');assert.equal(controller.state.connected,false);assert.equal(controller.state.pending.length,0);
        await assert.rejects(runtime.ensure(),/service_explicitly_stopped/);
        await action('start');assert.equal(controller.state.connected,true);
        assert.notEqual(controller.state.health.value.observation.instance,oldInstance);
        assert.equal(await fs.readFile(path.join(source,'memo.txt'),'utf8'),original);
        assert.equal([...values.keys()].filter(key=>key.startsWith('airk.service.intent.')).length,0);
        console.log('Captured redacted report, trust refusal, native stop acknowledgement, explicit start and original-source preservation passed.');

        const selectedHelper=path.join(path.dirname(binding.executable),helper),backup=path.join(fixture,'saved-helper');
        assert.equal(path.relative(fixture,selectedHelper).startsWith('..'),false);
        await fs.rename(selectedHelper,backup);missingHelper={selectedHelper,backup};
        await action('connect');assert.equal(controller.state.connected,false);
        assert.equal(controller.state.bootstrap.value.payloadVerified,false);
        assert.equal(controller.state.page.projects[0].project.registration.binding,project);
        assert.equal(controller.state.resources.state,'cached');
        await action('stop');assert.equal(controller.state.pending.length,0);
        await assert.rejects(nativeRunner(path.join(native,'runtime',main),['service','health','--runtime-dir',binding.controlDir]));
        await fs.rename(backup,selectedHelper);missingHelper=undefined;
        await assert.rejects(runtime.ensure(),/service_explicitly_stopped/);
        await action('start');assert.equal(controller.state.connected,true);
        assert.equal(await fs.readFile(path.join(source,'memo.txt'),'utf8'),original);
        console.log('Missing-helper discovery, cached projects/resources, saved-instance Stop and repaired explicit Start passed.');
    }finally{
        controller.dispose();
        if(missingHelper)await fs.rename(missingHelper.backup,missingHelper.selectedHelper);
        try{
            await fs.stat(install);
            await nativeRunner(path.join(native,'runtime',main),['runtime','uninstall','--install-dir',install],180000);
            removed=true;
        }catch(error){if(error.code==='ENOENT')removed=true;else throw error;}
        if(removed){
            const resolved=await fs.realpath(fixture);
            assert.equal(path.dirname(resolved),parent);assert.ok(path.basename(resolved).startsWith('airk-console-client-fixture-'));
            await fs.rm(resolved,{recursive:true,force:true});
        }
    }
})().catch(error=>{console.error(error);process.exitCode=1;});
