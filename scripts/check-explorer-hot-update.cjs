// SPDX-License-Identifier: Apache-2.0
// Opt-in actual Code UI gate. No native execution or ambient editor profiles.
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto'),net=require('node:net');
const {spawn,execFile}=require('node:child_process'),{promisify,isDeepStrictEqual}=require('node:util');
const {editorUi,privateEditorEnvironment,privateClientEnvironment}=require('./editor-ui.cjs');
const {createOwnedProfile,finishOwnedProfile}=require('./editor-owned-profile.cjs');
const execute=promisify(execFile),sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const assert=(condition,message)=>{if(!condition)throw Error(message);};
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const predicate="!!document.querySelector('.airk-explorer')";
const noticeText='AIRK native execution is turned off in User settings.';
const noticeExists=`[...document.querySelectorAll('aside.dismissible-notice')].some(n=>n.innerText.includes(${JSON.stringify(noticeText)})||n.innerText.includes('Enable native execution in AIRK settings to load and manage indexes.'))`;
const hasValue=value=>value!==undefined&&value!==null&&value!==false;
const cleanOwnedExit=(exit,quitError,forced)=>!!exit&&exit.code===0&&exit.signal===null&&!quitError&&!forced;
async function readMailbox(filename){try{return JSON.parse(await fs.readFile(filename,'utf8'));}catch(error){if(error.code==='ENOENT')return undefined;throw error;}}
async function publishMailbox(filename,value){const stage=filename+'.stage-'+crypto.randomBytes(8).toString('hex');try{await fs.writeFile(stage,JSON.stringify(value),{flag:'wx',mode:0o600});await fs.rename(stage,filename);}finally{await fs.rm(stage,{force:true});}}
function qualify(before,after,baseline,candidate){
    assert(baseline.identity!==candidate.identity,'Two distinct sealed update inventories are required');
    assert(before.host.pid===after.host.pid&&before.host.boot===after.host.boot,'The actual extension host restarted');
    assert(isDeepStrictEqual(before.host.profile,after.host.profile),'The actual private profile changed');
    assert(before.tabs===1&&after.tabs===1,'Explorer singleton was not retained');
    assert(before.asset.includes(baseline.identity)&&after.asset.includes(candidate.identity),'Actual webview assets do not match both sealed packages');
    assert(before.route===after.route,'The open Explorer route was lost during automatic replacement');
    assert(after.noticeAbsent===true&&after.reopenedNoticeAbsent===true,'A saved notice dismissal reappeared');
    return {qualified:true,scope:'actual same-window UI only; native execution disabled',baseline,candidate,before,after};
}
// The separate development driver only calls the actual installed extension API.
async function privateDriver(){
    const vscode=require('vscode'),fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
    const directory=process.env.AIRK_HOT_UI_RUN,config=JSON.parse(await fs.readFile(path.join(directory,'driver-config.json'),'utf8'));
    const boot=crypto.randomBytes(16).toString('hex'),profile=vscode.env.appRoot+'|'+config.profile;
    const write=async(name,value)=>{const filename=path.join(directory,name+'.json'),stage=filename+'.stage-'+crypto.randomBytes(8).toString('hex');try{await fs.writeFile(stage,JSON.stringify(value),{flag:'wx',mode:0o600});await fs.rename(stage,filename);}finally{await fs.rm(stage,{force:true});}};
    try{
        const extension=vscode.extensions.getExtension('iokaio.airk');
        if(!extension||!extension.extensionPath.startsWith(config.extensions+path.sep))throw Error('Installed AIRK escaped the private extension root');
        if(vscode.workspace.getConfiguration('airk').get('runtime.nativeExecution')!==false)throw Error('Native execution must stay disabled');
        if(typeof require(path.join(extension.extensionPath,'out/extensionUpdateLoader.js')).ExtensionUpdateLoader!=='function')throw Error('Baseline does not implement the update loader');
        await extension.activate();await write('driver-ready',{pid:process.pid,boot,profile});
        for(let sequence=1;Date.now()<config.deadline;){
            let request;try{request=JSON.parse(await fs.readFile(path.join(directory,'request-'+sequence+'.json'),'utf8'));}
            catch(error){if(error.code!=='ENOENT')throw error;await new Promise(resolve=>setTimeout(resolve,60));continue;}
            let value,error;
            try{
                if(vscode.workspace.getConfiguration('airk').get('runtime.nativeExecution')!==false)throw Error('Native execution was unexpectedly enabled');
                if(request.action==='open')value=await vscode.commands.executeCommand('airk.explorer.open',request.route);
                else if(request.action==='close'){
                    const tabs=vscode.window.tabGroups.all.flatMap(group=>group.tabs).filter(tab=>tab.label==='AIRK Explorer');
                    value=await vscode.window.tabGroups.close(tabs,true);
                }else if(request.action==='identity')value={pid:process.pid,boot,profile};
                else if(request.action==='tabs')value=vscode.window.tabGroups.all.flatMap(group=>group.tabs).filter(tab=>tab.label==='AIRK Explorer').length;
                else if(request.action==='quit'){await write('reply-'+sequence,{value:true});await vscode.commands.executeCommand('workbench.action.quit');return;}
                else throw Error('Unknown private UI action');
            }catch(problem){error=String(problem);}
            await write('reply-'+sequence,{value:value??null,error});sequence++;
        }
        throw Error('Private UI driver deadline expired');
    }catch(error){await write('driver-error',{error:String(error),stack:error.stack});}
}
const verifyArchive=String.raw`import hashlib,json,sys,zipfile
p,expected=sys.argv[1:]
with open(p,'rb') as f:
 h=hashlib.file_digest(f,'sha256').hexdigest()
assert h==expected,'VSIX SHA256 differs from reviewed input'
with zipfile.ZipFile(p) as z:
 rows=z.infolist();assert len(rows)<8192 and len({r.filename for r in rows})==len(rows),'VSIX inventory bound/duplicates'
 assert z.getinfo('extension/airk-update.json').file_size<=262144,'Update inventory too large'
 raw=z.read('extension/airk-update.json');v=json.loads(raw);m=json.loads(v['package_manifest'])
 assert v['schema']=='airk-extension-update/1' and m['name']=='airk' and m['publisher']=='iokaio' and m['version']==v['version'],'Not a canonical AIRK update package'
 assert m['main']=='./out/extension.js' and hashlib.sha256(v['package_manifest'].encode()).hexdigest()==v['files']['package.json'],'Entry/manifest seal mismatch'
 assert 0<len(v['files'])<=4096 and sum(z.getinfo('extension/'+n).file_size for n in v['files'])<=33554432,'Update payload budget'
 for n,d in v['files'].items():
  assert not n.startswith('/') and '\\' not in n and all(c not in ('','.','..') for c in n.split('/')),'Unsafe update inventory path'
  assert z.getinfo('extension/'+n).file_size<=8388608 and hashlib.sha256(z.read('extension/'+n)).hexdigest()==d,'Update payload hash mismatch'
 assert 'out/extensionUpdateLoader.js' in v['files'] and b'extensionUpdateLoader' in z.read('extension/out/extension.js') and b'ExtensionUpdateLoader' in z.read('extension/out/extensionUpdateLoader.js'),'Package lacks bootstrap capability'
 print(json.dumps({'archive_sha256':h,'identity':hashlib.sha256(raw).hexdigest(),'version':v['version']}))
`;
async function main(args=process.argv.slice(2)){
    assert(args.length===6&&[args[3],args[5]].every(value=>/^[a-f0-9]{64}$/.test(value)),
        'Usage: node scripts/check-explorer-hot-update.cjs <Code> <cli.js> <baseline.vsix> <baseline-sha256> <candidate.vsix> <candidate-sha256>');
    const [editor,cli,baselineFile,,candidateFile]=args.map((value,index)=>[0,1,2,4].includes(index)?path.resolve(value):value);
    const deadline=Date.now()+300000,remaining=()=>{assert(Date.now()<deadline,'Actual UI gate exceeded five-minute deadline');return deadline-Date.now();};
    const baseline=JSON.parse((await execute(process.env.PYTHON??'python',['-c',verifyArchive,baselineFile,args[3]],{timeout:60000,maxBuffer:1048576})).stdout);
    const candidate=JSON.parse((await execute(process.env.PYTHON??'python',['-c',verifyArchive,candidateFile,args[5]],{timeout:60000,maxBuffer:1048576})).stdout);
    assert(baseline.identity!==candidate.identity,'Packages must have distinct sealed identities');
    const artifacts=path.resolve(__dirname,'../artifacts/editor-qualification');await fs.mkdir(artifacts,{recursive:true});
    const run=await fs.realpath(await fs.mkdtemp(path.join(artifacts,'hot-explorer-'))),extensions=path.join(run,'extensions'),driver=path.join(run,'driver'),source=path.join(run,'workspace');
    const profileOwner=await createOwnedProfile(run),profile=profileOwner.profile;
    const clientHome=path.join(run,'client-home');let env=privateClientEnvironment(privateEditorEnvironment(),clientHome),child,ui,sequence=0,closed,ownedExit,launchFailure,finalResult,cleanupConfirmed=false;
    for(const directory of [clientHome,extensions,driver,source,path.join(profile,'User')])await fs.mkdir(directory,{recursive:true,mode:0o700});
    await fs.writeFile(path.join(profile,'User/settings.json'),JSON.stringify({'airk.runtime.nativeExecution':false,'airk.cli.autoInstall':false,'airk.explorer.assistance.enabled':false,'airk.service.installDir':path.join(run,'native-disabled'),'window.restoreWindows':'none','security.workspace.trust.enabled':false,'extensions.autoUpdate':false,'update.mode':'none'}));
    await fs.writeFile(path.join(driver,'package.json'),JSON.stringify({name:'airk-owned-hot-ui-driver',publisher:'local-test',version:'0.0.1',engines:{vscode:'*'},activationEvents:['onStartupFinished'],main:'driver.cjs'}));
    await fs.writeFile(path.join(driver,'driver.cjs'),'exports.activate=()=>('+privateDriver.toString()+')();');
    await fs.writeFile(path.join(run,'driver-config.json'),JSON.stringify({profile,extensions,deadline:deadline+20000}));
    const save=(name,value)=>fs.writeFile(path.join(run,name+'.json'),JSON.stringify(value,null,2));
    async function install(file,label){const result=await execute(editor,[cli,'--user-data-dir',profile,'--extensions-dir',extensions,'--install-extension',file,'--force'],{env:{...env,ELECTRON_RUN_AS_NODE:'1'},timeout:remaining(),maxBuffer:1048576});await fs.writeFile(path.join(run,label+'.log'),result.stdout+result.stderr);}
    async function poll(action,label,end=deadline,cleanup=false){while(Date.now()<end){assert(!launchFailure&&!ownedExit,'Owned Code exited before '+label+': '+String(launchFailure??JSON.stringify(ownedExit)));if(!cleanup){const failure=await readMailbox(path.join(run,'driver-error.json'));if(failure)throw Error(JSON.stringify(failure));}const value=await action();if(hasValue(value))return value;await sleep(100);}throw Error(label+' deadline expired');}
    async function rpc(action,extra={},cleanup=false){const number=++sequence;await publishMailbox(path.join(run,'request-'+number+'.json'),{action,...extra});const reply=await poll(()=>readMailbox(path.join(run,'reply-'+number+'.json')),'Private driver response',cleanup?Date.now()+5000:deadline,cleanup);assert(!reply.error,reply.error);return reply.value;}
    const routeExpression="JSON.stringify([document.querySelector('main.content')?.getAttribute('aria-label'),document.querySelector('button[role=tab][aria-selected=true]')?.id??null])";
    async function observation(identity){const dom=await ui.until(async()=>(await ui.document(predicate,`(()=>{const asset=[...document.scripts].find(s=>s.src.includes('explorerWebview.js'))?.src;return asset?.includes(${JSON.stringify(identity)})?{asset,route:${routeExpression},noticeAbsent:!(${noticeExists})}:false;})()`))?.value,'Exact sealed webview asset');return {...dom,host:await rpc('identity'),tabs:await rpc('tabs')};}
    try{
        await install(baselineFile,'install-baseline');
        const server=net.createServer();await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const port=server.address().port;await new Promise(resolve=>server.close(resolve));
        const log=await fs.open(path.join(run,'owned-editor.log'),'wx');
        child=spawn(editor,[source,'--new-window','--wait','--user-data-dir',profile,'--extensions-dir',extensions,'--extensionDevelopmentPath='+driver,'--remote-debugging-port='+port,'--remote-debugging-address','127.0.0.1','--skip-welcome','--skip-release-notes','--disable-updates','--disable-telemetry','--disable-gpu','--disable-workspace-trust'],{env:{...env,AIRK_HOT_UI_RUN:run},stdio:['ignore',log.fd,log.fd]});
        closed=new Promise(resolve=>{child.once('error',error=>{launchFailure=error;});child.once('close',(code,signal)=>{void log.close();ownedExit={code,signal};resolve(ownedExit);});});
        await poll(()=>readMailbox(path.join(run,'driver-ready.json')),'Actual installed extension activation');
        ui=await editorUi(port,deadline);await rpc('open',{route:'#indexing/window'});
        await ui.until(async()=>(await ui.document(predicate,"!!document.querySelector('#tab-indexing-window[aria-selected=true]')"))?.value,'Requested original Indexing route');
        const before=await observation(baseline.identity);await save('before',before);
        await ui.dispatch(predicate,noticeExists,`(()=>{const n=[...document.querySelectorAll('aside.dismissible-notice')].find(n=>n.innerText.includes(${JSON.stringify(noticeText)}));n.querySelector('button[aria-label="Dismiss notice"]').click();return true;})()`);
        await ui.until(async()=>(await ui.document(predicate,`!(${noticeExists})`))?.value,'Persisted native-disabled notice dismissal');
        await install(candidateFile,'install-candidate-with-editor-open');
        const after=await observation(candidate.identity);await save('after',after);
        for(const [route,label,tab]of [['#overview','Overview',null],['#manage/service','Management','tab-manage-service'],['#indexing/window','Indexing','tab-indexing-window']]){await rpc('open',{route});await ui.until(async()=>(await ui.document(predicate,`${routeExpression}===${JSON.stringify(JSON.stringify([label,tab]))}&&!(${noticeExists})`))?.value,'Dismissal on requested screen');assert(await rpc('tabs')===1,'Navigation duplicated Explorer');}
        await rpc('close');await rpc('open',{route:'#indexing/window'});await ui.until(async()=>(await ui.document(predicate,"!!document.querySelector('#tab-indexing-window[aria-selected=true]')"))?.value,'Reopened requested Indexing route');const reopened=await observation(candidate.identity);
        after.reopenedNoticeAbsent=reopened.noticeAbsent;assert(isDeepStrictEqual(after.host,reopened.host),'Reopening restarted the host');
        finalResult=qualify(before,after,baseline,candidate);finalResult.run=run;finalResult.dispatches=ui.dispatchReceipts();
    }catch(error){await save('failure',{error:String(error),stack:error.stack,baseline,candidate});throw error;}
    finally{
        let profileCanRemove=!child;
        if(child){let quitError,forced=false;try{await rpc('quit',{},true);}catch(error){quitError=String(error);}let timer;const wait=async ms=>{try{return await Promise.race([closed,new Promise(resolve=>{timer=setTimeout(()=>resolve(undefined),ms);})]);}finally{clearTimeout(timer);}};let exit=await wait(5000);if(!exit){forced=true;child.kill();exit=await wait(2000);}profileCanRemove=cleanOwnedExit(exit,quitError,forced);await save('owned-editor-exit',{exit:exit??null,quitError,forced,owned_child_pid:child.pid,graceful_quit_acknowledged:!quitError,clean_exit:profileCanRemove});if(!profileCanRemove)process.exitCode=1;}
        ui?.close();
        const profileCleanup=await finishOwnedProfile(profileOwner,profileCanRemove);await save('profile-cleanup',profileCleanup);cleanupConfirmed=profileCanRemove&&(profileOwner.platform!=='darwin'||profileCleanup.removed);if(!cleanupConfirmed)process.exitCode=1;
    }
    assert(cleanupConfirmed,'Owned Code/profile cleanup was not confirmed; no successful result published');
    await save('result',finalResult);console.log(JSON.stringify(finalResult,null,2));
}
module.exports={qualify,verifyArchive,noticeExists,hasValue,readMailbox,publishMailbox,cleanOwnedExit};
if(require.main===module)main().catch(error=>{console.error(error);process.exitCode=1;});
