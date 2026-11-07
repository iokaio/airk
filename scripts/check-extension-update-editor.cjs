// SPDX-License-Identifier: Apache-2.0
// Actual VS Code lifecycle preflight with fictional managed code and no native service.
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const {spawn}=require('node:child_process');
const {privateEditorEnvironment}=require('./editor-ui.cjs');
const repo=path.resolve(__dirname,'..');
const editor=process.argv[2];
if(!editor)throw new Error('Expected the installed Code.exe path.');
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const service=`const vscode=require('vscode');const {ExplorerPanel}=require('./explorerPanel');let panel,route;
exports.activate=async context=>{let open=false;panel=new ExplorerPanel(context.extensionUri,{message(){},visible(value){open=value;}});context.subscriptions.push(panel);
context.subscriptions.push(vscode.commands.registerCommand('airk.explorer.open',next=>{route=next||route||'overview';panel.reveal();}));
return {getServerDefinitions:async()=>[{label:TAG,version:context.extension.packageJSON.version,args:[route,panel.open,open]}],whenReady:async()=>{},explorerHandoff:()=>({open:panel.open,route})};};
exports.deactivate=()=>panel?.dispose();`;
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
async function main(){
    const base=path.join(repo,'artifacts','editor-update-preflight');await fs.mkdir(base,{recursive:true});
    const run=await fs.mkdtemp(path.join(base,'run-')),extensions=path.join(run,'extensions'),profile=await privateProfile(run),driver=path.join(run,'driver');
    let ownedEditorsClosed=true;
    try{
    await fs.mkdir(extensions);await fs.mkdir(driver);
    const old=path.join(extensions,'iokaio.airk-1.8.12'),candidate=path.join(run,'candidate'),result=path.join(run,'result.json');
    const source=path.join(repo,'packages','vscode');
    async function stage(root,tag,version){
        await fs.mkdir(path.join(root,'out'),{recursive:true});
        for(const name of await fs.readdir(path.join(source,'out')))if(/\.(js|css)$/.test(name))await fs.copyFile(path.join(source,'out',name),path.join(root,'out',name));
        await fs.cp(path.join(source,'node_modules','jsonc-parser'),path.join(root,'node_modules','jsonc-parser'),{recursive:true});
        await fs.mkdir(path.join(root,'images'));await fs.copyFile(path.join(source,'images','airk.svg'),path.join(root,'images','airk.svg'));
        await fs.mkdir(path.join(root,'native'));await fs.writeFile(path.join(root,'native','manifest.json'),JSON.stringify({version}));
        await fs.writeFile(path.join(root,'package.json'),JSON.stringify({name:'airk',publisher:'iokaio',version,engines:{vscode:'^1.140.0'},main:'out/extension.js',activationEvents:['onStartupFinished'],capabilities:{untrustedWorkspaces:{supported:true}}}));
        await fs.writeFile(path.join(root,'out','serviceExtension.js'),`const TAG=${JSON.stringify(tag)};`+service);
        await seal(root,version);
        // Installed VSIX manifests are reformatted and receive VS Code's own
        // private top-level metadata; the reviewed product fields stay intact.
        const installed=JSON.parse(await fs.readFile(path.join(root,'package.json'),'utf8'));
        installed.__metadata={installedTimestamp:1794000000000,source:'vsix',targetPlatform:process.platform};
        await fs.writeFile(path.join(root,'package.json'),JSON.stringify(installed,null,4));
    }
    async function seal(root,version){
        const files={};
        for(const name of ['package.json','native/manifest.json','images/airk.svg'])files[name]=sha(await fs.readFile(path.join(root,name)));
        for(const name of await fs.readdir(path.join(root,'out')))if(/\.(js|css)$/.test(name))files['out/'+name]=sha(await fs.readFile(path.join(root,'out',name)));
        for(const name of ['package.json','lib/umd/main.js','lib/umd/impl/edit.js','lib/umd/impl/format.js','lib/umd/impl/parser.js','lib/umd/impl/scanner.js','lib/umd/impl/string-intern.js'])files['node_modules/jsonc-parser/'+name]=sha(await fs.readFile(path.join(root,'node_modules','jsonc-parser',name)));
        await fs.writeFile(path.join(root,'airk-update.json'),JSON.stringify({schema:'airk-extension-update/1',version,files,package_manifest:await fs.readFile(path.join(root,'package.json'),'utf8')}));
    }
    await stage(old,'initial','1.8.12');await stage(candidate,'updated','1.8.13');
    const uriRoot=value=>({scheme:'file',path:process.platform==='win32'?'/'+value.replaceAll('\\','/'):value});
    await fs.writeFile(path.join(extensions,'extensions.json'),JSON.stringify([{identifier:{id:'iokaio.airk'},version:'1.8.12',location:uriRoot(old),relativeLocation:path.basename(old)}]));
    await fs.mkdir(path.join(profile,'User'),{recursive:true});
    await fs.writeFile(path.join(profile,'User','settings.json'),JSON.stringify({'security.workspace.trust.enabled':false,'extensions.autoCheckUpdates':false,'extensions.autoUpdate':false,'update.mode':'none'}));
    await fs.writeFile(path.join(driver,'package.json'),JSON.stringify({name:'airk-update-driver',publisher:'local-test',version:'0.0.1',engines:{vscode:'^1.140.0'},main:'driver.js'}));
    await fs.writeFile(path.join(driver,'driver.js'),'exports.activate=()=>{};');
    const test=`const vscode=require('vscode'),fs=require('node:fs/promises'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const run=${JSON.stringify(run)},extensions=${JSON.stringify(extensions)},candidate=${JSON.stringify(candidate)},result=${JSON.stringify(result)};
const pause=ms=>new Promise(r=>setTimeout(r,ms));
async function until(get,label){const deadline=Date.now()+30000;while(Date.now()<deadline){const value=await get();if(value)return value;await pause(100);}throw Error('Timed out: '+label);}
const tabs=()=>vscode.window.tabGroups.all.flatMap(group=>group.tabs).filter(tab=>tab.input instanceof vscode.TabInputWebview&&tab.input.viewType.endsWith('airk.explorer'));
exports.run=async()=>{const evidence={schema:'airk-extension-update-editor/1',native_service_exercised:false,profile:${JSON.stringify(profile)}};try{
const extension=await until(()=>vscode.extensions.getExtension('iokaio.airk'),'AIRK extension discovery'),api=await extension.activate();await api.whenReady();
assert.equal((await api.getServerDefinitions())[0].label,'initial');
await vscode.commands.executeCommand('airk.explorer.open','indexing/library');await until(()=>tabs().length===1,'initial Explorer');
const initialTab=tabs()[0];
evidence.initialViewType=initialTab.input.viewType;
const next=path.join(extensions,'iokaio.airk-1.8.13');await fs.cp(candidate,next,{recursive:true});
const registry=JSON.parse(await fs.readFile(path.join(extensions,'extensions.json'),'utf8'));const entry=registry.find(entry=>entry.identifier.id==='iokaio.airk');entry.version='1.8.13';entry.relativeLocation=path.basename(next);entry.location={scheme:'file',path:process.platform==='win32'?'/'+next.replaceAll('\\\\','/'):next};await fs.writeFile(path.join(extensions,'extensions.json'),JSON.stringify(registry));
await until(async()=>(await api.getServerDefinitions())[0]?.label==='updated','new-version graph');await until(()=>tabs().length===1&&tabs()[0]!==initialTab,'updated singleton Explorer');const updatedTab=tabs()[0];
assert.equal((await api.getServerDefinitions())[0].args[0],'indexing/library');
await Promise.all(Array.from({length:12},()=>vscode.commands.executeCommand('airk.explorer.open','search/query')));assert.equal(tabs().length,1);
const hostFile=path.join(next,'out','serviceExtension.js');await fs.writeFile(hostFile,(await fs.readFile(hostFile,'utf8')).replace('TAG="updated"','TAG="rebuilt"'));
const inventoryPath=path.join(next,'airk-update.json'),inventory=JSON.parse(await fs.readFile(inventoryPath,'utf8'));inventory.files['out/serviceExtension.js']=crypto.createHash('sha256').update(await fs.readFile(hostFile)).digest('hex');await fs.writeFile(inventoryPath,JSON.stringify(inventory));
await until(async()=>(await api.getServerDefinitions())[0]?.label==='rebuilt','same-version graph');await until(()=>tabs().length===1&&tabs()[0]!==updatedTab,'rebuilt singleton Explorer');
assert.equal((await api.getServerDefinitions())[0].args[0],'search/query');
await vscode.window.tabGroups.close(tabs(),true);assert.equal(tabs().length,0);
await until(async()=>(await api.getServerDefinitions())[0]?.args[1]===false,'closed panel disposition');
await fs.writeFile(hostFile,(await fs.readFile(hostFile,'utf8')).replace('TAG="rebuilt"','TAG="closed"'));inventory.files['out/serviceExtension.js']=crypto.createHash('sha256').update(await fs.readFile(hostFile)).digest('hex');await fs.writeFile(inventoryPath,JSON.stringify(inventory));
await until(async()=>(await api.getServerDefinitions())[0]?.label==='closed','closed Explorer handoff');assert.equal(tabs().length,0);
Object.assign(evidence,{status:'passed',initial:'1.8.12',updated:'1.8.13',same_version_replacement:true,installed_metadata_rewrite:true,route_preserved:true,singleton:true,closed_stays_closed:true,stable_api_forwarded:true,window_reloaded:false});
await fs.writeFile(result,JSON.stringify(evidence,null,2));
}catch(error){evidence.status='failed';evidence.error=String(error);evidence.stack=error.stack;await fs.writeFile(result,JSON.stringify(evidence,null,2));throw error;}};`;
    await fs.writeFile(path.join(driver,'test.js'),test);
    const log=await fs.open(path.join(run,'editor.log'),'w');
    ownedEditorsClosed=false;
    const child=spawn(path.resolve(editor),['--new-window','--user-data-dir',profile,'--extensions-dir',extensions,'--shared-data-dir',path.join(run,'shared'),`--extensionDevelopmentPath=${driver}`,`--extensionTestsPath=${path.join(driver,'test.js')}`,'--skip-welcome','--skip-release-notes','--disable-updates'],{windowsHide:true,env:privateEditorEnvironment(process.env),stdio:['ignore',log.fd,log.fd]});
    let timedOut=false,killTimer,hardTimer,hardReject;const timer=setTimeout(()=>{timedOut=true;child.kill();killTimer=setTimeout(()=>child.kill('SIGKILL'),3000);hardTimer=setTimeout(()=>hardReject(new Error('Owned editor close was not observed: '+run)),6000);},120000);
    let code;
    try { code=await new Promise((resolve,reject)=>{hardReject=reject;child.once('error',reject);child.once('close',value=>{ownedEditorsClosed=true;resolve(value);});}); }
    finally { clearTimeout(timer);clearTimeout(killTimer);clearTimeout(hardTimer);await log.close(); }
    let evidence;try{evidence=JSON.parse(await fs.readFile(result,'utf8'));}catch{}
    await fs.writeFile(path.join(run,'editor.log.process.json'),JSON.stringify({code,timedOut,close_observed:ownedEditorsClosed,pid:child.pid},null,2));
    if(code!==0||timedOut||evidence?.status!=='passed')process.exitCode=1;
    }catch(error){let evidence;try{evidence=JSON.parse(await fs.readFile(path.join(run,'result.json'),'utf8'));}catch{evidence={schema:'airk-extension-update-editor/1',native_service_exercised:false};}
        Object.assign(evidence,{status:'failed',profile,harness_error:String(error)});await fs.writeFile(path.join(run,'result.json'),JSON.stringify(evidence,null,2));process.exitCode=1;
    }finally{
        const cleanup=await finishProfile(profile,run,ownedEditorsClosed);let evidence;try{evidence=JSON.parse(await fs.readFile(path.join(run,'result.json'),'utf8'));}catch{evidence={status:'failed',profile};}
        evidence.profile_cleanup=cleanup;if(process.platform!=='win32'&&!cleanup.removed){evidence.cleanup_uncertain=true;process.exitCode=1;}
        await fs.writeFile(path.join(run,'result.json'),JSON.stringify(evidence,null,2));console.log(JSON.stringify({run,evidence},null,2));
    }
}
main().catch(error=>{console.error(error);process.exitCode=1;});
