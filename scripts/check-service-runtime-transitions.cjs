// SPDX-License-Identifier: Apache-2.0
// Real trusted controller upgrade/rollback fixture. No VSIX, login or default user installation.
const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const path=require('node:path');
const crypto=require('node:crypto');
const {execFile}=require('node:child_process');
const {RuntimeHost,nativeRunner}=require('../packages/vscode/out/serviceBootstrap.js');
const {ServiceConsoleController}=require('../packages/vscode/out/serviceConsoleController.js');

(async()=>{
    assert.equal(process.argv[2],'--runtime');assert.equal(process.argv[4],'--previous-runtime');
    const payload=await fs.realpath(process.argv[3]),previous=await fs.realpath(process.argv[5]);
    const parent=await fs.realpath(path.join(__dirname,'../target'));
    const fixture=await fs.mkdtemp(path.join(parent,'airk-transition-fixture-'));
    const install=path.join(fixture,'installation'),native=path.join(fixture,'native'),source=path.join(fixture,'source'),data=path.join(fixture,'data');
    const main=process.platform==='win32'?'airk.exe':'airk',helper=process.platform==='win32'?'airk-embed.exe':'airk-embed';
    const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
    await fs.mkdir(native);await fs.cp(payload,path.join(native,'runtime'),{recursive:true,errorOnExist:true,force:false});
    const bytes=await fs.readFile(path.join(native,'runtime','airk-install.json')),inventory=JSON.parse(bytes),target=hash(bytes);
    const priorBytes=await fs.readFile(path.join(previous,'airk-install.json')),prior=JSON.parse(priorBytes),preceding=hash(priorBytes);
    assert.equal(prior.version,inventory.version);assert.notEqual(preceding,target);
    await fs.writeFile(path.join(native,'manifest.json'),JSON.stringify({schema:2,version:inventory.version,vsix_target:`${process.platform}-${process.arch}`,
        runtime_dir:'runtime',install_manifest_sha256:target,payload:{[main]:inventory.files[main],[helper]:inventory.files[helper]}}));
    const values=new Map(),confirmations=[];let interrupted=false;
    const fault=process.argv[6]==='--interrupt-upgrade';
    const runner=async(command,args,timeout,input)=>{
        if(!fault||interrupted||args[1]!=='activate'||!args.includes('--expected-active')||args[args.indexOf('--runtime-sha256')+1]!==target){
            return nativeRunner(command,args,timeout,input);
        }
        assert.equal(args[args.indexOf('--install-dir')+1],install);
        let child,done=false;
        const completion=new Promise((resolve,reject)=>{
            child=execFile(command,args,{cwd:path.dirname(command),windowsHide:true,timeout:timeout??180000,maxBuffer:2*1024*1024},(error,stdout)=>{
                done=true;if(error)reject(error);else{try{resolve(JSON.parse(stdout));}catch(failure){reject(failure);}}
            });
            child.stdin?.end(input);
        });
        completion.catch(()=>{});
        try{
            const deadline=Date.now()+180000;
            while(Date.now()<deadline){
                if(done){await completion;throw new Error('Owned installer completed before fault injection');}
                let pending;try{pending=JSON.parse(await fs.readFile(path.join(install,'activation.json'),'utf8'));}catch(error){if(error.code!=='ENOENT')throw error;}
                if(pending?.phase==='probe'&&pending.candidate===target){
                    interrupted=true;child.kill();await assert.rejects(completion);throw new Error('Owned installer interrupted at its saved Probe checkpoint');
                }
                await new Promise(resolve=>setTimeout(resolve,25));
            }
            throw new Error('Saved activation Probe checkpoint was not observed');
        }finally{if(!done){child.kill();await completion.catch(()=>{});}}
    };
    const runtime=new RuntimeHost(fixture,inventory.version,{channel:'stable',installDir:install},runner);
    const controller=new ServiceConsoleController({version:inventory.version,authority:'Owned runtime transition fixture',runtime,trusted:()=>true,
        storage:{keys:()=>[...values.keys()],get:key=>values.get(key),update:async(key,value)=>{if(value===undefined)values.delete(key);else values.set(key,structuredClone(value));}},
        changed(){},confirm:async message=>{confirmations.push(message);return true;},copy:async()=>{},logs:async()=>{},resources:async()=>undefined,
        lifecycle:(binding,intent,force)=>runtime.lifecycle(binding,intent,force),abandonLifecycle:(binding,intent,id)=>runtime.abandonLifecycle(binding,intent,id)});
    const action=async(name,extra={})=>{
        await controller.action({type:'service',revision:controller.state.revision,action:name,...extra});
        assert.equal(controller.state.message,undefined,controller.state.message);
    };
    const selection=async()=>JSON.parse(await fs.readFile(path.join(install,'selected.json'),'utf8'));
    let staged=false,failure;
    try{
        const bundled=(await runtime.discover()).bundle;
        await nativeRunner(bundled.main,['runtime','stage','--source',previous,'--install-dir',install,'--state-dir',path.join(fixture,'state'),
            '--approved-sha256',preceding,'--verification-basis','trusted-distribution','--channel','stable'],180000);staged=true;
        await nativeRunner(bundled.main,['runtime','activate','--install-dir',install,'--runtime-sha256',preceding],180000);
        await controller.refresh();assert.equal(controller.state.connected,true);
        assert.equal((await selection()).active,preceding,'same-version pin was automatically replaced');
        await fs.mkdir(source);await fs.mkdir(data);const original='Original copper beacon transition evidence.\n';await fs.writeFile(path.join(source,'memo.txt'),original);
        const control=await fs.realpath(path.join(fixture,'state','control'));
        const registered=await nativeRunner(bundled.main,['--root',source,'--data-dir',data,'--collection','transition-fixture','service','register','--runtime-dir',control]);
        const project=registered.project.registration.binding;
        await controller.refresh();await action('index',{project});
        const deadline=Date.now()+90000;
        while(controller.state.page?.projects[0]?.progress?.job?.state!=='succeeded'){
            assert.ok(Date.now()<deadline,'Fixture index did not finish');await new Promise(resolve=>setTimeout(resolve,150));await controller.refresh();
        }
        const job=controller.state.page.projects[0].progress.job.id;
        if(fault){
            await controller.action({type:'service',revision:controller.state.revision,action:'upgradeRuntime'});
            assert.equal(interrupted,true);assert.ok(controller.state.message);
            assert.equal(controller.state.bootstrap.value.diagnosis.pending_activation.candidate,target);
            await action('resumeActivation');
            console.log('Controller recovered the exact saved activation after its owned installer exited at Probe.');
        }else await action('upgradeRuntime');
        assert.equal((await selection()).active,target);
        assert.equal(controller.state.connected,true);assert.equal(controller.state.page.projects[0].progress.job.id,job);
        assert.ok(confirmations.some(message=>message.includes(preceding)&&message.includes(target)));
        await action('rollbackRuntime');assert.equal((await selection()).active,preceding);
        assert.equal(controller.state.page.projects[0].progress.job.id,job);
        assert.equal(await fs.readFile(path.join(source,'memo.txt'),'utf8'),original);
        console.log('Reviewed controller upgrade/rollback preserved the exact accepted project receipt and original source.');
        await assert.rejects(nativeRunner(bundled.main,['runtime','activate','--install-dir',install,'--runtime-sha256',target,'--expected-active','ef'.repeat(32)],180000));
        assert.equal((await selection()).active,preceding);
        await action('stop');
        const replay=await nativeRunner(bundled.main,['runtime','activate','--install-dir',install,'--runtime-sha256',preceding,'--expected-active',target],180000);
        assert.equal(replay.active,preceding);
        const stopped=await nativeRunner(bundled.main,['runtime','snapshot','--install-dir',install]);assert.equal(stopped.lifecycle.explicitly_stopped,true);
        await assert.rejects(nativeRunner(bundled.main,['service','health','--runtime-dir',control]));
        await action('start');assert.equal(controller.state.connected,true);
        console.log('Stale review was refused; completed activation replay preserved explicit Stop and did not restart the service.');
    }catch(error){failure=error;throw error;}finally{
        controller.dispose();
        try{
            if(staged){
                let pending;try{pending=JSON.parse(await fs.readFile(path.join(install,'activation.json'),'utf8'));}catch(error){if(error.code!=='ENOENT')throw error;}
                if(pending){
                    assert.ok([target,preceding].includes(pending.candidate));
                    const args=['runtime','activate','--install-dir',install,'--runtime-sha256',pending.candidate];
                    if(pending.prior){assert.ok([target,preceding].includes(pending.prior.active));args.push('--expected-active',pending.prior.active);}
                    await nativeRunner(path.join(native,'runtime',main),args,180000);
                }
                await nativeRunner(path.join(native,'runtime',main),['runtime','uninstall','--install-dir',install],180000);
            }
            assert.equal(path.dirname(path.resolve(fixture)),parent);assert.ok(path.basename(fixture).startsWith('airk-transition-fixture-'));
            await fs.rm(fixture,{recursive:true,force:true});
        }catch(cleanup){console.error('Owned transition fixture retained after cleanup failure:',fixture,cleanup);throw failure??cleanup;}
    }
})().catch(error=>{console.error(error);process.exitCode=1;});
