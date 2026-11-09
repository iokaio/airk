// SPDX-License-Identifier: Apache-2.0
// Native extension acceptance with private profiles; accepts a reviewed stage or VSIX.
const fs=require('node:fs/promises'),path=require('node:path'),net=require('node:net');
const {spawn,execFile}=require('node:child_process');
const {promisify}=require('node:util');
const execute=promisify(execFile),services=[];
let bundle;
const profiles=new Map();
const initialIndexObservations=new Map();
const receiptPlans=new Map(),retainedProfileFolders=new Set();
const {prepareOwnedReceipts,cleanupOwnedReceipts,verifyOwnedReceiptsAbsent}=require('./native-editor-receipts.cjs');
const {verifyHealth:verifyIdleHealth,waitForIndexIdle,idleEvidencePrefix}=require('./native-editor-idle.cjs');
const {stopOwnedFixture,invokeNativeJson}=require('./native-editor-failure.cjs');
const {editorUi,privateEditorEnvironment,privateClientEnvironment}=require('./editor-ui.cjs');
const {publishMailbox,readMailbox}=require('./editor-mailbox.cjs');
const root=path.resolve(__dirname,'..');
const [editor,cli,source]=process.argv.slice(2).map(value=>path.resolve(value));
if(!source)throw new Error('Expected editor, its cli.js, and an approved extension stage or VSIX.');
const scenarios=(process.env.AIRK_EDITOR_SCENARIOS||'same,different,multi,usage,untrusted').split(',');
if(scenarios.some(value=>!['same','different','multi','usage','untrusted'].includes(value)))throw new Error('Unknown native editor scenario');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms)),active=new Map();
const environment=privateEditorEnvironment(process.env); // Child only: never inherit the normal editor's bootstrap/cache/IPC role.
async function privateProfile(run,label){
    const directory=process.platform==='win32'?path.join(run,'profile-'+label):await fs.realpath(await fs.mkdtemp('/tmp/airk-native-'));
    if(process.platform==='win32')await fs.mkdir(directory);else await fs.chmod(directory,0o700);
    profiles.set(directory,{run,label});await fs.writeFile(path.join(run,'profile-'+label+'-owned.json'),JSON.stringify({path:directory,label,private_user_data_dir:true},null,2));return directory;
}
async function finishProfile(profile,run,closed){
    const receipt={path:profile,owned_editors_closed:closed,removed:false,logs_copied:0,log_bytes:0,log_directories:0};
    if(process.platform==='win32'){receipt.retained_reason='Artifact-local Windows profile retained';return receipt;}
    try{
        if(!closed)throw new Error('Owned editor exit was not observed');
        const resolved=await fs.realpath(profile),info=await fs.lstat(profile);
        if(info.isSymbolicLink()||!info.isDirectory()||resolved!==profile||path.dirname(profile)!==await fs.realpath('/tmp')||!/^airk-native-[A-Za-z0-9]+$/.test(path.basename(profile)))throw new Error('Owned short profile identity changed');
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
        await copyLogs(path.join(profile,'logs'),path.join(run,'profile-logs-'+path.basename(profile)));
        await fs.rm(profile,{recursive:true,maxRetries:2,retryDelay:100});receipt.removed=true;
    }catch(error){receipt.retained_reason=String(error);}
    return receipt;
}
async function cleanupProfiles(){
    const receipts=[];
    for(const [profile,owner]of profiles){
        const closed=!retainedProfileFolders.has(owner.run)&&![...active.values()].some(item=>item.profile===profile);
        const receipt=await finishProfile(profile,owner.run,closed);receipt.label=owner.label;
        await fs.writeFile(path.join(owner.run,'profile-'+owner.label+'-cleanup.json'),JSON.stringify(receipt,null,2));receipts.push(receipt);
        if(receipt.removed||process.platform==='win32')profiles.delete(profile);
    }
    if(receipts.some(receipt=>process.platform!=='win32'&&!receipt.removed))throw new Error('Owned short profile cleanup remains uncertain: '+JSON.stringify(receipts));
    return receipts;
}
async function runProcess(args,logfile,extra={}){
    const log=await fs.open(logfile,'w'),deadline=Number(extra.AIRK_EDITOR_DEADLINE)||Date.now()+600000;
    return new Promise((resolve,reject)=>{
        const child=spawn(editor,args,{windowsHide:true,env:{...environment,...extra},stdio:['ignore',log.fd,log.fd]});
        let failure,forced=false,killTimer,hardTimer;
        const closed=new Promise(done=>child.once('close',(code,signal)=>done({code,signal})));
        const profileAt=args.indexOf('--user-data-dir');active.set(child,{closed,logfile,profile:profileAt>=0?args[profileAt+1]:undefined});
        const timer=setTimeout(()=>{forced=true;failure=new Error(`Private editor timed out: ${logfile}`);child.kill();killTimer=setTimeout(()=>child.kill('SIGKILL'),3000);hardTimer=setTimeout(()=>reject(new Error('Owned editor close was not observed after bounded force: '+logfile)),6000);},Math.max(1,deadline-Date.now()));
        child.once('error',error=>{failure=error;});
        void closed.then(async result=>{clearTimeout(timer);clearTimeout(killTimer);clearTimeout(hardTimer);active.delete(child);await log.close();
            const receipt={pid:child.pid,...result,forced,error:failure?String(failure):undefined};await fs.writeFile(logfile+'.cleanup.json',JSON.stringify(receipt,null,2));
            if(failure||forced||result.code!==0)reject(failure??new Error(`Private editor exited ${result.code}: ${logfile}`));else resolve(receipt);
        }).catch(reject);
    });
}
async function closeOwnedEditors(){
    const results=[];
    for(const [child,owned]of [...active]){
        child.kill();let timer;let result;
        try{result=await Promise.race([owned.closed,new Promise(resolve=>{timer=setTimeout(()=>resolve(undefined),3000);})]);}finally{clearTimeout(timer);}
        if(!result){child.kill('SIGKILL');try{result=await Promise.race([owned.closed,new Promise(resolve=>{timer=setTimeout(()=>resolve(undefined),2000);})]);}finally{clearTimeout(timer);}}
        const receipt={pid:child.pid,closed:!!result,result,forced:true};results.push(receipt);await fs.writeFile(owned.logfile+'.forced-cleanup.json',JSON.stringify(receipt,null,2));
    }
    return results;
}
async function unusedPort(){const server=net.createServer();await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const value=server.address().port;await new Promise(resolve=>server.close(resolve));return value;}
async function exists(file){return fs.access(file).then(()=>true,()=>false);}
async function observeIndexIdle(folder,role,request,scenarioDeadline){
    const fixture=await fs.realpath(folder),install=await canonicalOwned(fixture,path.join(folder,'native-service'));
    if(!/^idx_[a-f0-9]{32}$/.test(request.index)||!/^[a-f0-9]{64}$/.test(request.binding))throw new Error('Invalid exact indexing precondition binding');
    await canonicalOwned(fixture,request.root);await canonicalOwned(fixture,request.data);
    const prefix=idleEvidencePrefix(role,request.sequence,request.index);
    const deadline=Math.min(scenarioDeadline,Date.now()+90000),calls=[],samples=[],evidenceFile=path.join(folder,prefix+'-index-idle.json');
    let count=0;
    async function run(args){
        if(++count>128||Date.now()>=deadline)throw new Error('Index idle native read budget/deadline exhausted');
        const budget=path.join(folder,'native-request-budget');await fs.appendFile(budget,'x');if((await fs.stat(budget)).size>512)throw new Error('Shared native scenario request budget exhausted');
        const record={args};calls.push(record);
        try{const response=await execute(bundle.main,args,{cwd:folder,env:environment,windowsHide:true,timeout:Math.min(10000,deadline-Date.now()),maxBuffer:65536});record.stdout=response.stdout;record.stderr=response.stderr;return JSON.parse(response.stdout);}
        catch(error){record.error=String(error);record.stdout=error.stdout;record.stderr=error.stderr;throw error;}
    }
    async function rpc(control,method){
        const file=path.join(folder,prefix+'-idle-request-'+calls.length+'.json');await fs.writeFile(file,JSON.stringify(method),{flag:'wx'});
        const args=['service','rpc','--runtime-dir',control,'--request-file',file];
        // Native control owns exact registry_busy retries. Idle polling below
        // observes separate dated snapshots; never multiply a failed read's budget.
        return run(args);
    }
    try{
        const before=await run(['runtime','inspect','--install-dir',install]);
        if(before.payload_verified!==true||before.approval?.manifest_sha256!==bundle.digest||!/^[a-f0-9]{32}$/.test(before.registry_namespace))throw new Error('Index idle runtime approval unavailable');
        const control=await canonicalOwned(fixture,before.control_dir),pair=JSON.parse(await fs.readFile(path.join(bundle.source,'airk-runtime.json'),'utf8'));
        const health=await rpc(control,'health');
        const verifyHealth=value=>verifyIdleHealth(value,{version:bundle.version,runtimeSha256:bundle.digest,executableSha256:pair.airk_sha256,namespace:before.registry_namespace});
        const service=verifyHealth(health);
        async function backing(){
            const response=await rpc(control,{catalog_detail:{entity:{kind:'index',id:request.index},expected_catalog_revision:null}}),detail=response.detail,index=detail?.item?.value;
            if(response.status!=='catalog_detail'||detail.namespace!==before.registry_namespace||detail.item.kind!=='index'||index?.id!==request.index||index.tombstone!==false||index.backing?.binding!==request.binding||index.backing.root!==request.root||index.backing.data_root!==request.data||index.backing.collection!==request.collection)throw new Error('Index idle observation crossed the exact setup backing');
            return response;
        }
        const original=await backing(),initialKey=JSON.stringify([fixture,role,request.index,request.binding,request.root,request.data,request.collection]);
        const remembered=initialIndexObservations.get(initialKey),prior=remembered?.service_instance===service.instance&&remembered.namespace===before.registry_namespace?remembered:undefined;
        const idle=await waitForIndexIdle(()=>rpc(control,{progress:{project:request.binding}}),{project:request.binding,service:service.instance},{deadline,requireIndexWork:!prior,record:async value=>samples.push(value)});
        const confirmedHealth=verifyHealth(await rpc(control,'health'));if(confirmedHealth.instance!==service.instance)throw new Error('Service restarted during indexing precondition observation');
        const confirmed=await backing(),after=await run(['runtime','inspect','--install-dir',install]);
        if(after.payload_verified!==true||after.approval?.manifest_sha256!==bundle.digest||after.registry_namespace!==before.registry_namespace||after.control_dir!==before.control_dir)throw new Error('Index idle runtime changed during observation');
        const result={...idle,index:request.index,binding:request.binding,namespace:before.registry_namespace,runtime_sha256:bundle.digest,service_instance:service.instance,evidence:evidenceFile,initial_index_evidence:prior??null,original,confirmed};
        await fs.writeFile(evidenceFile,JSON.stringify({request,calls,samples,result},null,2),{flag:'wx'});
        if(!prior)initialIndexObservations.set(initialKey,{evidence:evidenceFile,index:request.index,binding:request.binding,namespace:before.registry_namespace,service_instance:service.instance});
        return result;
    }catch(error){try{await fs.writeFile(evidenceFile,JSON.stringify({request,calls,samples,error:String(error)},null,2),{flag:'wx'});}catch(retention){error.cause??=retention;}throw error;}
}
const sha=bytes=>require('node:crypto').createHash('sha256').update(bytes).digest('hex');
function inside(root,value){const normalize=value=>process.platform==='win32'?path.toNamespacedPath(value).toLowerCase():value;const relative=path.relative(normalize(root),normalize(value));return relative!==''&&!relative.startsWith('..'+path.sep)&&relative!=='..'&&!path.isAbsolute(relative);}
async function ownedPath(root,value,missingLeaf=false){
    if(typeof value!=='string'||!path.isAbsolute(value))throw new Error('Expected absolute owned fixture path');
    let resolved;try{resolved=await fs.realpath(value);}catch(error){if(!missingLeaf||error.code!=='ENOENT')throw error;resolved=path.join(await fs.realpath(path.dirname(value)),path.basename(value));}
    if(!inside(root,resolved))throw new Error('Private editor request escaped its owned fixture');return resolved;
}
async function canonicalOwned(root,value){const actual=await ownedPath(root,value);const normalize=p=>process.platform==='win32'?path.toNamespacedPath(p).toLowerCase():p;if(normalize(path.resolve(value))!==normalize(actual))throw new Error('Owned fixture path is not canonical');return actual;}
async function canonicalLocation(value){
    let current=path.resolve(value);const suffix=[];
    for(let depth=0;depth<64;depth++){
        try{return path.join(await fs.realpath(current),...suffix.reverse());}
        catch(error){if(error.code!=='ENOENT')throw error;const parent=path.dirname(current);if(parent===current)throw error;suffix.push(path.basename(current));current=parent;}
    }
    throw new Error('Fixture location ancestor bound exceeded');
}
async function verifyFreshFixtureLayout(folder){
    const fixture=await fs.realpath(folder),install=await ownedPath(fixture,path.join(folder,'native-service'),true);
    if(await exists(install))throw new Error('Fresh copied-install fixture already has runtime state');
    const result=await execute(bundle.main,['runtime','location','--channel','stable'],{windowsHide:true,env:environment,cwd:folder,timeout:30000,maxBuffer:65536});
    const location=JSON.parse(result.stdout);if(location.protocol!==1||location.channel!=='stable'||typeof location.install_dir!=='string')throw new Error('Native default-location preflight failed');
    const nativeInstall=await canonicalLocation(location.install_dir),normalize=p=>process.platform==='win32'?path.toNamespacedPath(p).toLowerCase():p;
    if(normalize(nativeInstall)===normalize(install))throw new Error('Fixture would adopt the ambient native installation/state/log layout');
    // runtime_layout::planned_state uses this fresh non-default install itself;
    // initialize then chooses state/logs. Only the owned child client home is used.
    const expected={installDir:install,stateDir:install,controlDir:path.join(install,'control'),logsDir:path.join(install,'logs')};
    if(Object.values(expected).some(value=>!inside(fixture,value)))throw new Error('Planned fixture runtime state escaped the owned scenario');
    await fs.writeFile(path.join(folder,'fixture-layout.json'),JSON.stringify({nativeLocation:location,expected,source_contract:'runtime_layout.rs planned_state(non-default fresh install) + initialize logs=state/logs'},null,2));
}
/** Installed AIRK Explorer: Search shows a cited fixture hit and Management lists the registered project. */
async function checkExplorer(ui){
    const inExplorer=expression=>ui.document("!!document.querySelector('.airk-explorer')",expression);
    const hits=await ui.until(async()=>(await inExplorer("(()=>{const paths=[...document.querySelectorAll('.hit code.path')].map(n=>n.textContent);return paths.length?{paths,meta:document.querySelector('.result-meta')?.innerText}:false;})()"))?.value,'Explorer search hits');
    if(!hits.paths.some(value=>value.startsWith('fixture.ts')))throw new Error('Explorer search did not cite the fixture: '+JSON.stringify(hits));
    const click=async(label,selector='button')=>{const find=`[...document.querySelectorAll(${JSON.stringify(selector)})].find(n=>n.textContent.trim()===${JSON.stringify(label)})`;return ui.dispatch("!!document.querySelector('.airk-explorer')",`!!(${find})&&!(${find}).disabled`,`(()=>{const b=${find};if(!b||b.disabled)return false;b.click();return true;})()`);};
    const contains=async(text)=>ui.until(async()=>(await inExplorer(`document.querySelector('.help-topic')?.innerText.includes(${JSON.stringify(text)})`))?.value,`Explorer help ${text}`);
    await ui.dispatch("!!document.querySelector('.airk-explorer')","[...document.querySelectorAll('.topbar button')].some(n=>n.textContent.trim().startsWith('Help'))","(()=>{const b=[...document.querySelectorAll('.topbar button')].find(n=>n.textContent.trim().startsWith('Help'));if(!b)return false;b.click();return true;})()");
    await ui.quickPick('Browse all help','AIRK Help');
    await ui.dispatch("!!document.querySelector('.airk-explorer')","!!document.querySelector('input[aria-label=\"Search help\"]')","(()=>{const i=document.querySelector('input[aria-label=\"Search help\"]');if(!i)return false;i.value='tester PDF';i.dispatchEvent(new Event('input',{bubbles:true}));return true;})()");
    await click('Tester workflow','.help-index ul button');await contains('scanned requirements');
    await click('Choose indexing by purpose','.help-index .chips button');await contains('Search Only (essential)');
    await click('Engine guide: indexing-choices');await contains('Choose evidence detail, vector work and extraction separately.');
    // Live help remains read-only, including the exact tool schema selector.
    await ui.dispatch("!!document.querySelector('.airk-explorer')","[...document.querySelectorAll('.help-index input')].some(i=>i.value==='airk_search'&&i.closest('form'))","(()=>{const i=[...document.querySelectorAll('.help-index input')].find(i=>i.value==='airk_search');const form=i?.closest('form');if(!form)return false;form.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));return true;})()");
    await contains('inputSchema');
    await click('About','.topbar button');await contains('Ioka LLC');await contains('Munarium');await contains('https://discord.gg/sJase5FbE');
    const about=await ui.until(async()=>(await inExplorer("(()=>{const labels=[...document.querySelectorAll('[aria-label=\"AIRK public links\"] button')].map(b=>b.textContent.trim());return labels.some(x=>x.includes('Report an issue'))&&labels.some(x=>x.includes('Contribute to AIRK'))?labels:false;})()"))?.value,'Explorer About community links');
    await ui.click('Indexing');await ui.click('Index sets');
    const native=await ui.indexing({action:'page',kind:'projects'}),rows=native.page.page.items;
    if(!rows.some(row=>row.kind==='project'&&String(row.value.name).includes('Fixture index set')))throw new Error('Indexing did not list the native index set');
    return {hits,rows,help:['role search','purpose and format guide','offline native engine guide','live tool schema','About credits/history/community links'],about};
}
async function stopOwnedServicesOnFailure(editorCleanup){
    const results=[];
    // This is disposal after a failed journey, never UI Stop acceptance. Keep
    // services in place so ordinary cleanup still reports its original failure.
    for(const folder of services){
        retainedProfileFolders.add(folder);
        const install=path.join(folder,'native-service');
        try{
            if(!Array.isArray(editorCleanup)||editorCleanup.some(receipt=>receipt.closed!==true))throw Error('Owned editor closure is unconfirmed; no failure Stop submitted');
            try{await fs.lstat(install);}catch(error){if(error.code==='ENOENT'){results.push({folder,absent:true,acceptance:false});continue;}throw error;}
            if(!bundle)throw Error('Verified runtime bundle unavailable for failure disposal');
            const fixture=folder,control=path.join(install,'control');
            let normalStop=null;
            try{normalStop=JSON.parse(await fs.readFile(path.join(folder,'native-stop.json'),'utf8'));}
            catch(error){if(error.code!=='ENOENT')throw error;}
            const invoke=(command,args,timeout,input,record)=>invokeNativeJson(command,args,{timeout,input,record,environment});
            const receipt=await stopOwnedFixture({fixture,install,control,bundle,invoke,normalStop,editorsClosed:true});
            results.push({folder,acceptance:false,receipt});
        }catch(error){results.push({folder,acceptance:false,complete:false,error:String(error)});}
    }
    return results;
}
async function cleanupServices(){
    const results=[];
    for(const folder of services.splice(0)){
        const install=path.join(folder,'native-service'),plan=receiptPlans.get(folder);
        if(!await exists(install)){
            try{if(plan){const absent=await verifyOwnedReceiptsAbsent(plan);await fs.writeFile(path.join(folder,'uninstall-receipt-cleanup.json'),JSON.stringify(absent,null,2));receiptPlans.delete(folder);}}
            catch(error){retainedProfileFolders.add(folder);results.push({qualified:false,error:String(error),retained:true});}
            continue;
        }
        let receipt;
        try{
            const stop=JSON.parse(await fs.readFile(path.join(folder,'native-stop.json'),'utf8'));
            const captured=stop.capture,record=captured?.response?.record,request=record?.request;
            if(captured?.qualification!=='historical_native_lifecycle_response'||captured.response.status!=='lifecycle'||record?.phase!=='complete'||request?.action!=='stop'||captured.force!==false||captured.before_health?.status!=='health'||request.expected_instance!==captured.before_health.observation?.instance)throw new Error('Complete native Stop/no-force/authenticated-health receipt unavailable');
            if(!Number.isSafeInteger(captured.capturedAt)||captured.capturedAt<stop.started||record.id===stop.previous_capture?.response?.record?.id||!record.id||stop.dispatch?.dispatch===stop.confirmation?.dispatch)throw new Error('Captured native receipt does not identify this one-dispatch Stop');
            if(captured.runtime.installDir!==install||captured.runtime.runtimeSha256!==bundle.digest||request.runtime!==bundle.digest||captured.namespace!==request.key?.namespace||captured.namespace!==stop.requested_binding?.namespace)throw new Error('Crossed native Stop runtime/namespace binding');
            const result=await execute(bundle.main,['runtime','inspect','--install-dir',install],{windowsHide:true,timeout:30000,maxBuffer:65536});
            const observation=JSON.parse(result.stdout),owned=await fs.realpath(folder),control=await canonicalOwned(owned,observation.control_dir);
            if(await canonicalOwned(owned,captured.runtime.controlDir)!==control||observation.approval?.manifest_sha256!==bundle.digest||observation.registry_namespace!==captured.namespace||observation.payload_verified!==true)throw new Error('Native stopped inspection crossed the captured installation');
            if(observation.lifecycle?.explicitly_stopped!==true||observation.lifecycle?.pending!==null)throw new Error('Native lifecycle has not settled stopped');
            receipt={qualified:true,stop,observation};
            if(plan){
                const profileRoots=[...profiles].filter(([,owner])=>owner.run===folder).map(([profile])=>profile);
                receipt.uninstallReceipts=await cleanupOwnedReceipts(plan,{profileRoots,editorsClosed:active.size===0,stop:receipt,evidenceDirectory:path.join(folder,'removed-uninstall-receipts')});
                await fs.writeFile(path.join(folder,'uninstall-receipt-cleanup.json'),JSON.stringify(receipt.uninstallReceipts,null,2));receiptPlans.delete(folder);
            }
        }catch(error){receipt={...receipt,qualified:false,error:String(error),retained:true};retainedProfileFolders.add(folder);}
        results.push(receipt);await fs.writeFile(path.join(folder,'native-cleanup.json'),JSON.stringify(receipt,null,2));
    }
    if(results.some(value=>!value.qualified))throw new Error('Owned native service cleanup remains unqualified: '+JSON.stringify(results));
    return results;
}
async function main(){
    const harnessPaths=['scripts/check-native-editor.cjs','scripts/editor-ui.cjs','scripts/native-editor-workbench.cjs','scripts/editor-mailbox.cjs','scripts/native-editor-receipts.cjs','scripts/native-editor-idle.cjs','scripts/native-editor-failure.cjs','packages/vscode/out/test/installedEditor.js','packages/vscode/out/test/installedCompletion.js','packages/vscode/out/test/installedControlTrace.js','packages/vscode/out/test/installedEditorC8.js','packages/vscode/out/test/installedEditorContracts.js','packages/vscode/out/test/installedEditorHttp.js'];
    const harness=Object.fromEntries(await Promise.all(harnessPaths.map(async name=>[name,sha(await fs.readFile(path.join(root,name)))])));
    const stage=(await fs.lstat(source)).isDirectory();
    if(stage){const version=JSON.parse(await fs.readFile(path.join(source,'package.json'),'utf8')).version;
        bundle=await require(path.join(root,'packages/vscode/out/serviceBootstrap.js')).verifyBundledRuntime(source,version);}
    const base=path.join(root,'artifacts/editor-qualification');await fs.mkdir(base,{recursive:true});
    const run=await fs.realpath(await fs.mkdtemp(path.join(base,stage?'native-stage-':'native-installed-')));
    const clientHome=path.join(run,'client-home');await fs.mkdir(clientHome,{mode:0o700});Object.assign(environment,privateClientEnvironment(environment,clientHome));
    const extensions=path.join(run,'extensions');await fs.mkdir(extensions);
    if(!stage)await runProcess([cli,'--user-data-dir',await privateProfile(run,'install'),'--extensions-dir',extensions,
        '--shared-data-dir',path.join(run,'shared-editor-data'),'--install-extension',source,'--force'],path.join(run,'install.log'),{ELECTRON_RUN_AS_NODE:'1'});
    if(!stage){const installed=(await fs.readdir(extensions)).filter(name=>name.startsWith('iokaio.airk-'));
        if(installed.length!==1)throw new Error('Expected one private installed AIRK extension');
        const directory=path.join(extensions,installed[0]),version=JSON.parse(await fs.readFile(path.join(directory,'package.json'),'utf8')).version;
        bundle=await require(path.join(root,'packages/vscode/out/serviceBootstrap.js')).verifyBundledRuntime(directory,version);}
    const driver=path.join(extensions,'local-test.airk-driver-0.0.1');await fs.mkdir(driver);
    await fs.writeFile(path.join(driver,'package.json'),JSON.stringify({name:'airk-driver',publisher:'local-test',version:'0.0.1',engines:{vscode:'^1.140.0'},main:'driver.js',activationEvents:['onStartupFinished'],capabilities:{untrustedWorkspaces:{supported:true}}}));
    await fs.writeFile(path.join(driver,'driver.js'),`const vscode=require('vscode'),path=require('node:path'),{publishMailbox}=require(${JSON.stringify(path.join(root,'scripts/editor-mailbox.cjs'))});
exports.activate=()=>{if(!process.env.AIRK_EDITOR_RUN)return;void require(${JSON.stringify(path.join(root,'packages/vscode/out/test/installedEditor.js'))}).run()
.catch(error=>publishMailbox(path.join(process.env.AIRK_EDITOR_RUN,process.env.AIRK_EDITOR_SCENARIO,process.env.AIRK_EDITOR_ROLE+'-error.json'),{error:String(error),stack:error.stack}))
.finally(()=>vscode.commands.executeCommand('workbench.action.quit'));};`);
    const summary={schema:'airk-native-editor/2',source,staged:stage,nativeInstallDigest:bundle.digest,harness,selectedScenarios:scenarios,scenarios:[]};
    console.log(`Native editor output: ${run}`);
    for(const scenario of scenarios){
        const folder=path.join(run,scenario);await fs.mkdir(folder);const scenarioDeadline=Date.now()+600000;
        services.push(folder);const storage=path.join(folder,'storage');await fs.mkdir(storage);
        await verifyFreshFixtureLayout(folder);
        for(const [name,symbol] of [['workspace-a','WindowAlphaEvidence'],['workspace-b','WindowBetaEvidence']]){const workspace=path.join(folder,name);await fs.mkdir(workspace);await fs.writeFile(path.join(workspace,'fixture.ts'),`export function ${symbol}() { return 42; }\n`);await fs.writeFile(path.join(workspace,'hidden.ts'),'export function HiddenPrivateEvidence() {}\n');}
        for(const name of ['shared-a','shared-b']){await fs.mkdir(path.join(folder,name));await fs.writeFile(path.join(folder,name,'shared.ts'),'export function SharedAllowedEvidence() { return 19; }\n');}
        const location=JSON.parse((await execute(bundle.main,['runtime','location','--channel','stable'],{windowsHide:true,env:environment,cwd:folder,timeout:30000,maxBuffer:65536})).stdout);
        // This legacy-named directory remains the native account receipt base;
        // observing it does not claim a modern launcher executable exists.
        if(location.protocol!==1||typeof location.user_paths?.launcher!=='string')throw new Error('Actual native account receipt location unavailable for exact fixture cleanup');
        const receiptSources=[await fs.realpath(path.join(folder,'workspace-a')),await fs.realpath(path.join(folder,'workspace-b'))];
        // Windows native metadata can retain the namespaced spelling. Both
        // exact hashes remain bound to the same verified canonical fixture root.
        const receiptSourceSpellings=[...new Set(receiptSources.flatMap(root=>process.platform==='win32'?[root,path.toNamespacedPath(root)]:[root]))];
        const receiptPlan=await prepareOwnedReceipts({launcher:location.user_paths.launcher,fixture:folder,installDir:path.join(folder,'native-service'),sourceRoots:receiptSourceSpellings});
        receiptPlans.set(folder,receiptPlan);await fs.writeFile(path.join(folder,'uninstall-receipt-baseline.json'),JSON.stringify({nativeLocation:location,plan:receiptPlan},null,2));
        const originalSources={};
        for(const name of ['workspace-a/fixture.ts','workspace-a/hidden.ts','workspace-b/fixture.ts','workspace-b/hidden.ts','shared-a/shared.ts','shared-b/shared.ts'])originalSources[name]=await fs.readFile(path.join(folder,name),'utf8');
        const roles=['same','different'].includes(scenario)?['a','b']:['a'],children=[],uis={},ports={};
        for(const role of roles){const profile=await privateProfile(folder,role);await fs.mkdir(path.join(profile,'User'),{recursive:true});
            // All new bindings come from the real reviewed Indexing forms.
            await fs.writeFile(path.join(profile,'User/settings.json'),JSON.stringify({'airk.service.channel':'stable','airk.service.installDir':path.join(folder,'native-service'),'airk.storage.path':storage,'airk.cli.autoInstall':false,
                'airk.cli.shellIntegration':'off','workbench.startupEditor':'none','window.restoreWindows':'none','security.workspace.trust.startupPrompt':'never','extensions.autoUpdate':false,'extensions.autoCheckUpdates':false,
                'telemetry.telemetryLevel':'off','update.mode':'none','files.simpleDialog.enable':true,'window.dialogStyle':'custom',
}));
            let workspace=path.join(folder,scenario==='different'&&role==='b'?'workspace-b':'workspace-a');
            if(['multi','usage'].includes(scenario)){workspace=path.join(folder,'two-roots.code-workspace');await fs.writeFile(workspace,JSON.stringify({folders:[{path:'workspace-a'},{path:'workspace-b'}]}));}
            ports[role]=await unusedPort();
            const args=[workspace,'--new-window','--user-data-dir',profile,'--extensions-dir',extensions,'--shared-data-dir',path.join(run,'shared-editor-data'),
                `--remote-debugging-port=${ports[role]}`,'--remote-debugging-address=127.0.0.1','--skip-welcome','--skip-release-notes','--disable-updates','--disable-telemetry','--disable-gpu'];
            if(stage)args.push(`--extensionDevelopmentPath=${source}`);
            // VSIX installation creates extensions.json; a subsequently copied
            // driver is not registered there. Develop only the separate driver:
            // installed AIRK must still resolve from the verified VSIX directory.
            else args.push(`--extensionDevelopmentPath=${driver}`);
            if(scenario!=='untrusted')args.push('--disable-workspace-trust');
            children.push(runProcess(args,path.join(folder,`${role}-editor.log`),{AIRK_EDITOR_RUN:run,AIRK_EDITOR_ROLE:role,AIRK_EDITOR_SCENARIO:scenario,AIRK_EDITOR_DEADLINE:String(scenarioDeadline),AIRK_EDITOR_RUNTIME_SHA256:bundle.digest,
                AIRK_EDITOR_DEVELOPMENT_STAGE:stage?source:'',AIRK_USAGE_MODE:scenario==='usage'?'aggregates-only':'disabled'}));
        }
        let failure,explorerTask,allExited=false;const exited=Promise.all(children).then(()=>{allExited=true;}).catch(error=>{failure=error;});const handled=new Map();
        try{
            const end=scenarioDeadline;
            while(true){if(failure)throw failure;
                for(const role of roles){const error=await readMailbox(path.join(folder,`${role}-error.json`));if(error)throw new Error(JSON.stringify(error));
                    const request=await readMailbox(path.join(folder,`${role}-ui-request.json`));
                    if(request&&handled.get(role)!==request.sequence){
                        if(!Number.isSafeInteger(request.sequence)||request.sequence<1||request.sequence>256)throw new Error('Private UI action budget/sequence invalid');
                        uis[role]??=await editorUi(ports[role],scenarioDeadline);
                        const actions=['workbenchRead','workbenchCopy','providerStart','providerStop','serviceStop','connect','startService','page','inspect','setupDraft','setupApply','setCreate','scopeSelect','sources','indexIdle','blockDraft','deleteSetDraft','renameDraft','apply','read','reconcile','staleApply','restricted','usageScoped','usageAdmin','usageJson','usageClear'];
                        if(!actions.includes(request.action))throw new Error('Unknown private Indexing action');
                        const fixture=await fs.realpath(folder);
                        if(request.action==='serviceStop'){
                            await canonicalOwned(fixture,request.installDir);
                            if(request.installDir!==path.join(folder,'native-service')||request.runtimeSha256!==bundle.digest)throw new Error('Crossed owned Stop fixture binding');
                            const observed=await execute(bundle.main,['runtime','inspect','--install-dir',request.installDir],{windowsHide:true,env:environment,timeout:30000,maxBuffer:65536});
                            const metadata=JSON.parse(observed.stdout);
                            if(metadata.payload_verified!==true||metadata.approval?.manifest_sha256!==bundle.digest||!/^[a-f0-9]{32}$/.test(metadata.registry_namespace))throw new Error('Owned service namespace unavailable before Stop');
                            await fs.writeFile(path.join(folder,request.action==='serviceStop'?'native-pre-stop-inspect.json':'native-pre-start-inspect.json'),JSON.stringify(metadata,null,2));
                            request.namespace=metadata.registry_namespace;
                        }
                        if(request.action==='setupDraft'){await ownedPath(fixture,request.source);await ownedPath(fixture,request.storage,true);if(request.additional)await ownedPath(fixture,request.additional.path);}
                        if(request.action==='blockDraft'&&(!/^src_[a-f0-9]{32}$/.test(request.root)||request.path!=='hidden.ts'))throw new Error('Invalid exact reviewed fixture exclusion');
                        if(['scopeSelect','setCreate'].includes(request.action)&&typeof(request.setName??request.name)!=='string')throw new Error('Exact native index-set name required');
                        for(const key of ['index','project','id'])if(request[key]!==undefined&&!/^(idx_|prj_|grt_)[a-f0-9]{32}$/.test(request[key]))throw new Error('Invalid exact native catalog ID');
                        // Mark dispatch before invoking the UI; unknown outcomes are never replayed.
                        handled.set(role,request.sequence);
                        const value=request.action==='indexIdle'?await observeIndexIdle(folder,role,request,scenarioDeadline):await uis[role].indexing(request);
                        if(Buffer.byteLength(JSON.stringify(value))>1048576)throw new Error('Complete private UI receipt exceeds 1 MiB');
                        await publishMailbox(path.join(folder,`${role}-ui-done-${request.sequence}.json`),value);}
                }
                if(scenario==='multi'&&await exists(path.join(folder,'a-explorer-ready.json'))&&!explorerTask){uis.a??=await editorUi(ports.a,scenarioDeadline);
                    explorerTask=checkExplorer(uis.a).then(value=>publishMailbox(path.join(folder,'explorer-checked.json'),value)).catch(error=>{failure=error;});}
                if((await Promise.all(roles.map(role=>exists(path.join(folder,`${role}-capture.json`))))).every(Boolean))break;
                if(allExited)throw new Error(`Private editor exited before acceptance completed: ${scenario}`);
                if(Date.now()>end)throw new Error(`Native editor scenario timed out: ${scenario}`);await delay(150);
            }
            if(explorerTask)await explorerTask;if(failure)throw failure;
            for(const role of roles){uis[role]??=await editorUi(ports[role],scenarioDeadline);const ui=uis[role];
                const body=await ui.until(async()=>(await ui.service('document.body.innerText'))?.value,'AIRK Explorer Management');
                await fs.writeFile(path.join(folder,`${role}-service.txt`),body);
                if(scenario==='untrusted'){if(!(await ui.service("![...document.querySelectorAll('button')].some(b=>!b.disabled&&['Review new index','Apply reviewed index setup','Create index set','Apply selection','Save index set'].includes(b.textContent.trim()))"))?.value)throw new Error('Untrusted native controls were enabled');}
                const page=await ui.workbench('document.title');const image=await page.client.call('Page.captureScreenshot',{format:'png'});
                await fs.writeFile(path.join(folder,`${role}-workbench.png`),Buffer.from(image.data,'base64'));
            }
        }finally{for(const [role,ui]of Object.entries(uis)){await fs.writeFile(path.join(folder,role+'-ui-dispatch-receipts.json'),JSON.stringify(ui.dispatchReceipts(),null,2));try{await fs.writeFile(path.join(folder,role+'-ui-diagnostics.json'),JSON.stringify(ui.diagnostics(),null,2));}catch(error){console.error('Private UI diagnostic retention failed:',String(error));}ui.close();}await publishMailbox(path.join(folder,'capture-complete.json'),{});}
        await exited;if(failure)throw failure;
        const sourcePreservation=[];
        for(const [name,original]of Object.entries(originalSources)){
            const expected=original+(['same','different'].includes(scenario)&&name==='workspace-a/fixture.ts'?'\nexport function SharedEditEvidence() { return 7; }\n':'');
            const actual=await fs.readFile(path.join(folder,name));if(!actual.equals(Buffer.from(expected)))throw new Error('Unexpected source mutation: '+name);
            sourcePreservation.push({path:name,before_sha256:sha(original),after_sha256:sha(actual),intentional_peer_edit:expected!==original});}
        await fs.writeFile(path.join(folder,'source-preservation.json'),JSON.stringify(sourcePreservation,null,2));
        const results=await Promise.all(roles.map(async role=>({...JSON.parse(await fs.readFile(path.join(folder,`${role}-result.json`),'utf8')),sourcePreservation})));
        if(new Set(results.map(value=>value.extensionHostPid)).size!==roles.length)throw new Error('Each test window must have its own extension host');
        summary.scenarios.push(...results);
        await cleanupServices();await cleanupProfiles();
        await fs.writeFile(path.join(run,'summary.json'),JSON.stringify(summary,null,2));console.log(`${scenario}: native editor passed`);
    }
    for(const [name,digest]of Object.entries(harness))if(sha(await fs.readFile(path.join(root,name)))!==digest)throw new Error('Installed-editor harness changed during qualification: '+name);
    summary.status=summary.scenarios.some(row=>row.status==='unqualified')?'partial':'passed';
    summary.qualifiesFullScenarioSelection=summary.status==='passed';
    await fs.writeFile(path.join(run,'summary.json'),JSON.stringify(summary,null,2));
    if(summary.status==='partial'){console.log(`Native editor coverage remains unqualified: ${run}`);process.exitCode=2;}
    else console.log(`Native editor selected scenarios passed: ${run}`);
}
main().catch(async error=>{let editorCleanup,failureNativeStop,serviceCleanup,profileCleanup;try{editorCleanup=await closeOwnedEditors();}catch(cleanupError){editorCleanup={error:String(cleanupError)};}try{failureNativeStop=await stopOwnedServicesOnFailure(editorCleanup);}catch(cleanupError){failureNativeStop={error:String(cleanupError),acceptance:false};}try{serviceCleanup=await cleanupServices();}catch(cleanupError){serviceCleanup=String(cleanupError);}try{profileCleanup=await cleanupProfiles();}catch(cleanupError){profileCleanup=String(cleanupError);}console.error(JSON.stringify({error:String(error),editorCleanup,failureNativeStop,serviceCleanup,profileCleanup}));process.exitCode=1;});
