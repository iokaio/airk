// SPDX-License-Identifier: Apache-2.0
// Actual packaged product/controller and UI handoff, with native execution disabled.
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const {spawn}=require('node:child_process');
const {privateEditorEnvironment}=require('./editor-ui.cjs');
const repo=path.resolve(__dirname,'..'),sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const args=process.argv.slice(2);
if(args.length!==3)throw new Error('Expected Code.exe, its matching resources/app/out/cli.js, and an actual AIRK VSIX.');
const [editor,cli,vsix]=args.map(value=>path.resolve(value));
const environment=privateEditorEnvironment(process.env);
let ownedEditorsClosed=true;
async function privateProfile(run){
    if(process.platform==='win32'){const directory=path.join(run,'profile');await fs.mkdir(directory);return directory;}
    const directory=await fs.mkdtemp('/tmp/airk-explorer-');await fs.chmod(directory,0o700);return directory;
}
async function finishProfile(profile,run,closed){
    const receipt={path:profile,owned_editors_closed:closed,removed:false,logs_copied:0,log_bytes:0,log_directories:0};
    if(process.platform==='win32'){receipt.retained_reason='Artifact-local Windows profile retained';return receipt;}
    try{
        if(!closed)throw new Error('Owned editor exit was not observed');
        const resolved=await fs.realpath(profile),info=await fs.lstat(profile);
        if(info.isSymbolicLink()||!info.isDirectory()||resolved!==path.join(await fs.realpath('/tmp'),path.basename(profile))||path.dirname(profile)!=='/tmp'||!/^airk-explorer-[A-Za-z0-9]+$/.test(path.basename(profile)))throw new Error('Owned short profile identity changed');
        async function copyLogs(source,destination,depth=0){
            if(depth>12||++receipt.log_directories>256)throw new Error('Profile log directory bound exceeded');
            let entries;try{entries=await fs.readdir(source,{withFileTypes:true});}catch(error){if(error.code==='ENOENT')return;throw error;}
            await fs.mkdir(destination,{recursive:true});
            for(const entry of entries){const input=path.join(source,entry.name),output=path.join(destination,entry.name);
                if(entry.isSymbolicLink())throw new Error('Profile log symlink cannot be copied safely');
                if(entry.isDirectory())await copyLogs(input,output,depth+1);
                else if(entry.isFile()){const stat=await fs.stat(input);if(++receipt.logs_copied>256||(receipt.log_bytes+=stat.size)>33554432)throw new Error('Profile log copy bound exceeded');await fs.copyFile(input,output);}
            }
        }
        await copyLogs(path.join(profile,'logs'),path.join(run,'profile-logs'));
        await fs.rm(profile,{recursive:true,maxRetries:2,retryDelay:100});receipt.removed=true;
    }catch(error){receipt.retained_reason=String(error);}
    return receipt;
}
async function runOwned(commandArgs,logfile,timeout,extra={}){
    const log=await fs.open(logfile,'w'),started=Date.now();
    let child,timer,killTimer,hardTimer;
    try{return await new Promise((resolve,reject)=>{
        ownedEditorsClosed=false;child=spawn(editor,commandArgs,{windowsHide:true,env:{...environment,...extra},stdio:['ignore',log.fd,log.fd]});
        let timedOut=false,spawnError;
        child.once('error',error=>{spawnError=error;});
        timer=setTimeout(()=>{timedOut=true;child.kill();killTimer=setTimeout(()=>child.kill('SIGKILL'),3000);hardTimer=setTimeout(()=>reject(new Error('Owned editor close was not observed: '+logfile)),6000);},timeout);
        child.once('close',async(code,signal)=>{
            ownedEditorsClosed=true;
            clearTimeout(timer);clearTimeout(killTimer);clearTimeout(hardTimer);
            const receipt={pid:child.pid,code,signal,timed_out:timedOut,elapsed_ms:Date.now()-started,close_observed:true,error:spawnError?String(spawnError):undefined};
            try{await fs.writeFile(logfile+'.process.json',JSON.stringify(receipt,null,2));
                if(spawnError||timedOut||code!==0)reject(spawnError??new Error('Private editor process failed: '+JSON.stringify(receipt)));else resolve(receipt);
            }catch(error){reject(error);}
        });
    });}finally{clearTimeout(timer);clearTimeout(killTimer);clearTimeout(hardTimer);await log.close();}
}
async function main(){
    for(const file of [editor,cli,vsix])if(!(await fs.stat(file)).isFile())throw new Error('Expected a file: '+file);
    if(path.extname(vsix).toLowerCase()!=='.vsix')throw new Error('An actual VSIX archive is required.');
    const base=path.join(repo,'artifacts','installed-explorer-preflight');await fs.mkdir(base,{recursive:true});
    const run=await fs.mkdtemp(path.join(base,'run-')),extensions=path.join(run,'extensions'),profile=await privateProfile(run),driver=path.join(run,'driver'),folder=path.join(run,'workspace'),result=path.join(run,'result.json');
    const evidence={schema:'airk-installed-explorer-preflight/1',status:'running',native_service_exercised:false,profile,vsix:{path:vsix,sha256:sha(await fs.readFile(vsix))},harness_sha256:sha(await fs.readFile(__filename)),run};
    try{
        for(const directory of [extensions,driver,folder])await fs.mkdir(directory);
        evidence.install=await runOwned([cli,'--user-data-dir',profile,'--extensions-dir',extensions,'--shared-data-dir',path.join(run,'shared'),'--install-extension',vsix,'--force'],path.join(run,'install.log'),120000,{ELECTRON_RUN_AS_NODE:'1'});
        const installed=(await fs.readdir(extensions)).filter(name=>name.startsWith('iokaio.airk-'));
        if(installed.length!==1)throw new Error('Expected exactly one privately installed AIRK extension.');
        const installedRoot=path.join(extensions,installed[0]),manifest=JSON.parse(await fs.readFile(path.join(installedRoot,'package.json'),'utf8'));
        evidence.vsix.version=manifest.version;evidence.installed_root=installedRoot;
        const inventory=JSON.parse(await fs.readFile(path.join(installedRoot,'airk-update.json'),'utf8'));
        if(inventory.version!==manifest.version||!inventory.package_manifest)throw new Error('Candidate lacks the sealed packaged manifest required for installed metadata validation.');
        await fs.mkdir(path.join(profile,'User'),{recursive:true});
        await fs.writeFile(path.join(profile,'User','settings.json'),JSON.stringify({'security.workspace.trust.enabled':false,'airk.runtime.nativeExecution':false,'airk.cli.autoInstall':false,'airk.explorer.assistance.enabled':false,'extensions.autoCheckUpdates':false,'extensions.autoUpdate':false,'update.mode':'none','workbench.startupEditor':'none'}));
        await fs.writeFile(path.join(folder,'fictional.txt'),'Fictional Explorer UI preflight; no native indexing.\n');
        await fs.writeFile(path.join(driver,'package.json'),JSON.stringify({name:'airk-installed-explorer-driver',publisher:'local-test',version:'0.0.1',engines:{vscode:'^1.140.0'},main:'driver.js'}));
        await fs.writeFile(path.join(driver,'driver.js'),'exports.activate=()=>{};');
        const test=`const vscode=require('vscode'),fs=require('node:fs/promises'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const result=${JSON.stringify(result)},installedRoot=${JSON.stringify(installedRoot)},evidence=${JSON.stringify(evidence)};
const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function until(get,label){const deadline=Date.now()+40000;while(Date.now()<deadline){const value=await get();if(value)return value;await pause(100);}throw Error('Timed out: '+label);}
const tabs=()=>vscode.window.tabGroups.all.flatMap(group=>group.tabs).filter(tab=>tab.input instanceof vscode.TabInputWebview&&tab.input.viewType.endsWith('airk.explorer'));
exports.run=async()=>{try{
assert.equal(vscode.workspace.isTrusted,true);assert.equal(vscode.workspace.getConfiguration('airk').inspect('runtime.nativeExecution').globalValue,false);
const extension=await until(()=>vscode.extensions.getExtension('iokaio.airk'),'installed product discovery');
assert.equal(path.resolve(extension.extensionPath),path.resolve(installedRoot));
assert.equal(extension.packageJSON.version,evidence.vsix.version);const installedManifest=JSON.parse(await fs.readFile(path.join(installedRoot,'package.json'),'utf8'));assert.ok(installedManifest.__metadata,'VS Code rewrites actual installed manifests on disk');
const api=await extension.activate();await api.whenReady();assert.deepEqual(await api.getServerDefinitions(),[]);
await vscode.commands.executeCommand('airk.explorer.open','indexing/library');await until(()=>tabs().length===1,'actual product Explorer');
await Promise.all(Array.from({length:12},()=>vscode.commands.executeCommand('airk.explorer.open','indexing/library')));assert.equal(tabs().length,1);
assert.equal(await vscode.window.tabGroups.close(tabs(),true),true);await until(()=>tabs().length===0,'closed Explorer');await pause(750);assert.equal(tabs().length,0);
await vscode.commands.executeCommand('airk.explorer.open','indexing/library');await until(()=>tabs().length===1,'reopened product Explorer');const initialTab=tabs()[0],extensionHostPid=process.pid;
const inventoryPath=path.join(installedRoot,'airk-update.json'),inventory=JSON.parse(await fs.readFile(inventoryPath,'utf8')),manifestBefore=inventory.package_manifest;
const css=path.join(installedRoot,'out','explorerWebview.css');await fs.appendFile(css,'\\n/* fictional installed Explorer replacement preflight */\\n');
inventory.files['out/explorerWebview.css']=crypto.createHash('sha256').update(await fs.readFile(css)).digest('hex');await fs.writeFile(inventoryPath,JSON.stringify(inventory));
await until(()=>tabs().length===1&&tabs()[0]!==initialTab,'same-version product/controller/UI handoff');await api.whenReady();assert.deepEqual(await api.getServerDefinitions(),[]);
assert.equal(vscode.workspace.getConfiguration('airk').inspect('runtime.nativeExecution').globalValue,false);
assert.equal(process.pid,extensionHostPid);assert.equal(JSON.parse(await fs.readFile(inventoryPath,'utf8')).package_manifest,manifestBefore);
await Promise.all(Array.from({length:12},()=>vscode.commands.executeCommand('airk.explorer.open','indexing/library')));assert.equal(tabs().length,1);
await vscode.window.tabGroups.close(tabs(),true);await until(()=>tabs().length===0,'replacement Explorer closes');await pause(750);assert.equal(tabs().length,0);
await vscode.commands.executeCommand('airk.explorer.open','indexing/library');await until(()=>tabs().length===1,'replacement Explorer reopens');assert.deepEqual(await api.getServerDefinitions(),[]);
Object.assign(evidence,{status:'passed',production_controller_graph:true,actual_vsix_installation:true,same_version_css_replacement:true,installed_metadata_rewrite:!!installedManifest.__metadata,original_api_forwarded:true,singleton:true,closed_stays_closed:true,reopen:true,extension_host_pid:extensionHostPid,window_reloaded:false,scope:'Installed production activation, Explorer tabs and UI asset handoff with native execution disabled; native runtime/process qualification is separate.'});
await fs.writeFile(result,JSON.stringify(evidence,null,2));
}catch(error){Object.assign(evidence,{status:'failed',error:String(error),stack:error.stack});await fs.writeFile(result,JSON.stringify(evidence,null,2));throw error;}};`;
        await fs.writeFile(path.join(driver,'test.js'),test);
        const execution=await runOwned(['--new-window','--user-data-dir',profile,'--extensions-dir',extensions,'--shared-data-dir',path.join(run,'shared'),
            '--extensionDevelopmentPath='+driver,'--extensionTestsPath='+path.join(driver,'test.js'),'--skip-welcome','--skip-release-notes','--disable-updates',folder],path.join(run,'editor.log'),150000);
        const outcome=JSON.parse(await fs.readFile(result,'utf8'));outcome.editor_process=execution;await fs.writeFile(result,JSON.stringify(outcome,null,2));
        if(outcome.status!=='passed')process.exitCode=1;
    }catch(error){let prior;try{prior=JSON.parse(await fs.readFile(result,'utf8'));}catch{}
        await fs.writeFile(result,JSON.stringify({...evidence,...prior,status:'failed',harness_error:String(error)},null,2));console.error(JSON.stringify({run,result,error:String(error)},null,2));process.exitCode=1;
    }finally{
        const cleanup=await finishProfile(profile,run,ownedEditorsClosed);let outcome;try{outcome=JSON.parse(await fs.readFile(result,'utf8'));}catch{outcome=evidence;}
        outcome.profile_cleanup=cleanup;if(process.platform!=='win32'&&!cleanup.removed){outcome.cleanup_uncertain=true;process.exitCode=1;}
        await fs.writeFile(result,JSON.stringify(outcome,null,2));console.log(JSON.stringify({run,result,evidence:outcome},null,2));
    }
}
main().catch(error=>{console.error(error);process.exitCode=1;});
