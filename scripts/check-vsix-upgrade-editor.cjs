// SPDX-License-Identifier: Apache-2.0
// Explicit qualification only. Real installed extensions; separate private UI driver.
// Usage: node scripts/check-vsix-upgrade-editor.cjs <Code> <cli.js> <1.8.7.vsix> <current.vsix> <current-sha256>
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const {spawn,execFile}=require('node:child_process'),{promisify,isDeepStrictEqual}=require('node:util'),net=require('node:net');
const execute=promisify(execFile),root=path.resolve(__dirname,'..');
const {editorUi,privateEditorEnvironment,privateClientEnvironment}=require('./editor-ui.cjs');
const {prepareOwnedReceipts,cleanupOwnedReceipts}=require('./native-editor-receipts.cjs');
const CURRENT_VERSION=require('../packages/vscode/package.json').version;
const OLD_SHA='71518c9d89fd0fb4f54e0d046b2f85948a361c7303ad7c49a60df568dfb4219d';
const LEAF_SHA='4260bd90b8623e45da6d67144a2bff4fd8fa0a8186c7281c0125e39877c3f5c1';
const OLD_MAIN_SHA='d28828af6eb8275c55748bc5202e9a7e642e8568ff915eaa7284e343bb56a066';
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const exists=file=>fs.access(file).then(()=>true,()=>false);
function inside(base,value){const relative=path.relative(base,value);return !!relative&&relative!=='..'&&!relative.startsWith('..'+path.sep)&&!path.isAbsolute(relative);}
function assert(value,message){if(!value)throw Error(message);}
async function boundedSafeRead({request,retryDelay,deadline,perReadMs=30000,now=Date.now,sleep=delay}){
    const readEnd=Math.min(deadline,now()+perReadMs);
    while(true){
        const timeout=Math.min(30000,deadline-now(),readEnd-now());assert(timeout>0,'Typed safe read exceeded per-read deadline before dispatch');
        const wire=await request(timeout),retry=retryDelay(wire);if(retry===undefined)return wire;
        assert(now()+retry<readEnd&&now()+retry<deadline,'Typed safe read retry exceeded per-read deadline');await sleep(retry);
    }
}
async function waitOwnedExit(child,closed,timeoutMs=5000){
    let timer;const race=async ms=>{try{return await Promise.race([closed,new Promise(resolve=>{timer=setTimeout(()=>resolve(undefined),ms);})]);}finally{clearTimeout(timer);}};
    let value=await race(timeoutMs);if(!value){child.kill('SIGKILL');value=await race(2000);}
    assert(value,'Owned process OS exit was not observed');return {pid:child.pid,closed:true,...value};
}
async function boundedJsonProcess(command,args,input,{cwd,env,timeout=30000,maxBytes=2097152,onExit=async()=>{}}){
    const child=spawn(command,args,{cwd,env,stdio:'pipe'}),closed=new Promise(resolve=>child.once('close',(code,signal)=>resolve({code,signal})));
    let stdout='',stderr='',failure,timer,primary;
    const exited=new Promise((resolve,reject)=>{child.once('error',reject);closed.then(resolve);timer=setTimeout(()=>{failure=Error('Native JSON read exceeded deadline');child.kill();resolve(undefined);},timeout);});
    child.stdout.on('data',bytes=>{if(Buffer.byteLength(stdout)+bytes.length>maxBytes){failure=Error('Native JSON read exceeded byte bound');child.kill();}else stdout+=bytes;});
    child.stderr.on('data',bytes=>{stderr=(stderr+bytes).slice(-8192);});
    try{
        child.stdin.on('error',error=>{failure??=error;});child.stdin.end(JSON.stringify(input));const value=await exited;
        if(failure)throw failure;assert(value?.code===0,'Native JSON read failed: '+stderr);return JSON.parse(stdout);
    }catch(error){primary=error;child.kill();throw error;}finally{
        clearTimeout(timer);try{await onExit(await waitOwnedExit(child,closed));}catch(error){if(!primary)throw error;await onExit({pid:child.pid,closed:false,error:String(error),primary:String(primary)});}
    }
}
async function gatePrivateProfile(profile,install){
    const filename=path.join(profile,'User/settings.json'),stat=await fs.lstat(filename);
    assert(stat.isFile()&&!stat.isSymbolicLink()&&stat.size<=65536,'Private settings are redirected or oversized');
    const settings=JSON.parse(await fs.readFile(filename,'utf8'));assert(settings['airk.service.installDir']===install,'Private installation setting changed');
    settings['airk.runtime.nativeExecution']=false;await fs.writeFile(filename,JSON.stringify(settings));
    assert(JSON.parse(await fs.readFile(filename,'utf8'))['airk.runtime.nativeExecution']===false,'Private native gate did not persist');
}
function failedActivation(receipt,install,candidate,started){
    assert(receipt?.schema===1&&receipt.install===install&&receipt.candidate===candidate&&receipt.preceding===null&&receipt.outcome==='aborted_or_failed'&&Number.isSafeInteger(receipt.observed_unix_ms)&&receipt.observed_unix_ms>=started,'No typed native failed-activation receipt bound to the exact old Setup attempt');
    return receipt;
}
function qualifyOldLauncherCopy({install,launcher,canonical,regular,nlink,actualHash,authenticatedMainHash,inventoryMainHash,strict}){
    assert(launcher===path.join(install,'airk-launcher')&&canonical===launcher&&regular===true&&nlink===1,'Old copied launcher is not one exact owned regular native file');
    assert(actualHash===OLD_MAIN_SHA&&authenticatedMainHash===OLD_MAIN_SHA&&inventoryMainHash===OLD_MAIN_SHA,'Old copied launcher differs from authenticated sealed application main');
    assert(strict?.command==='/usr/bin/codesign'&&isDeepStrictEqual(strict.args,['--verify','--strict','--verbose=4',launcher])&&Number.isInteger(strict.status)&&strict.status!==0&&typeof strict.stdout==='string'&&typeof strict.stderr==='string'&&/invalid Info\.plist/i.test(strict.stderr),'Old copied launcher did not reproduce the strict invalid Info.plist signature defect');
    return {qualified:true,defect:'authenticated sealed application main copied as standalone launcher; strict invalid Info.plist',launcher,sha256:actualHash,strict};
}
function completeStop(stop,inspection,bundle,install){
    const capture=stop?.capture,record=capture?.response?.record,request=record?.request;
    assert(capture?.qualification==='historical_native_lifecycle_response'&&capture.response.status==='lifecycle'&&record.phase==='complete'&&request.action==='stop'&&capture.force===false,'Incomplete public native Stop');
    assert(capture.before_health?.status==='health'&&request.expected_instance===capture.before_health.observation?.instance,'Stop lacks authenticated instance binding');
    assert(Number.isSafeInteger(capture.capturedAt)&&capture.capturedAt>=stop.started&&record.id&&record.id!==stop.previous_capture?.response?.record?.id&&Number.isSafeInteger(stop.dispatch?.dispatch)&&Number.isSafeInteger(stop.confirmation?.dispatch)&&stop.dispatch.dispatch!==stop.confirmation.dispatch,'Stop capture is stale or missing one-dispatch evidence');
    assert(stop.requested_binding?.installDir===install&&stop.requested_binding.runtimeSha256===bundle.digest&&capture.runtime.installDir===install&&capture.runtime.runtimeSha256===bundle.digest&&request.runtime===bundle.digest&&capture.namespace===request.key?.namespace&&capture.namespace===stop.requested_binding?.namespace,'Stop crossed native ownership');
    assert(inspection.payload_verified===true&&inspection.approval?.manifest_sha256===bundle.digest&&inspection.registry_namespace===capture.namespace&&inspection.control_dir===capture.runtime.controlDir&&inspection.lifecycle?.explicitly_stopped===true&&inspection.lifecycle?.pending===null,'Stopped native inspection is incomplete');
    return {qualified:true,stop,observation:inspection};
}
function toolBody(value){
    assert(value&&!value.isError,'MCP returned an error');
    const text=value.content?.filter(row=>row.type==='text').map(row=>row.text).join('\n');
    const parsed=text?JSON.parse(text):undefined;
    if(parsed&&value.structuredContent)assert(isDeepStrictEqual(parsed,value.structuredContent),'MCP structured/text contract differs');
    const body=value.structuredContent??parsed;assert(body&&typeof body==='object','Missing full MCP result');return body;
}
function citations(value,result=[]){
    if(!value||typeof value!=='object')return result;
    if(value.citation)result.push(value.citation);
    for(const [key,child]of Object.entries(value))if(key!=='citation')citations(child,result);
    return result;
}
function assertEvidence(body,hash){assert(citations(body).some(row=>row.path==='fixture.ts'&&row.content_hash===hash),'MCP did not cite exact original source bytes');}
/** A public snapshot never persists the native bearer. Material came from its
 * exact owner-private native references and is checked against the actual API. */
function httpDefinitionSnapshot(definition,info,material){
    const {installedHttpIdentity}=require(path.join(root,'packages/vscode/out/test/installedEditorHttp.js'));
    const {validateHttpConnection}=require(path.join(root,'packages/vscode/out/serviceHttp.js'));
    const identity=installedHttpIdentity(definition),connection=validateHttpConnection(info);
    assert(identity.credentialId===connection.connection_id&&identity.url===connection.endpoint.url&&material.url===identity.url,'HTTP definition crossed native connection');
    const authorization=Object.entries(identity.headers).find(([name])=>name.toLowerCase()==='authorization')?.[1];
    assert(authorization==='Bearer '+material.credential,'Actual provider bearer differs from protected native credential');
    return {transport:'https',uri:identity.url,version:definition.version,headers:Object.fromEntries(Object.entries(identity.headers).map(([name,value])=>[name,name.toLowerCase()==='authorization'?'Bearer [redacted]':value])),authorization_sha256:sha(Buffer.from(authorization)),nativeConnection:connection};
}
async function restoreHttpDefinition(snapshot,material){
    assert(snapshot?.transport==='https'&&typeof snapshot.uri==='string'&&snapshot.uri===material.url,'Current evidence requires exact HTTPS transport');
    assert(sha(Buffer.from('Bearer '+material.credential))===snapshot.authorization_sha256,'Native credential changed since actual provider observation');
    assert(Object.entries(snapshot.headers).filter(([name])=>name.toLowerCase()==='authorization').length===1&&Object.entries(snapshot.headers).find(([name])=>name.toLowerCase()==='authorization')?.[1]==='Bearer [redacted]','Exactly one redacted native authorization');
    const headers=Object.fromEntries(Object.entries(snapshot.headers).map(([name,value])=>[name,name.toLowerCase()==='authorization'?'Bearer '+material.credential:value]));
    return {uri:{toString:()=>snapshot.uri},headers,nativeConnection:snapshot.nativeConnection,version:snapshot.version};
}
// This function is serialized into the sole development extension. AIRK itself
// is installed by the real Code CLI and is never loaded as a development stage.
async function privateDriver(){
    const vscode=require('vscode'),fs=require('node:fs/promises'),path=require('node:path');
    const folder=process.env.AIRK_UPGRADE_PHASE,config=JSON.parse(await fs.readFile(path.join(folder,'config.json'),'utf8'));
    const write=(name,value)=>fs.writeFile(path.join(folder,name+'.json'),JSON.stringify(value,null,2));
    const ext=vscode.extensions.getExtension('iokaio.airk');let api,bootError;
    try{
        if(!ext||ext.packageJSON.version!==config.version||!ext.extensionPath.startsWith(config.extensions+path.sep))throw Error('Wrong actual installed extension');
        if(vscode.workspace.getConfiguration('airk').inspect('service.installDir')?.globalValue!==config.install)throw Error('Native installation configuration escaped fixture');
        // Native execution starts disabled, including automatic extension
        // activation racing this driver. This independent, verified helper graph
        // performs discovery only; it never calls ensure/stage/repair.
        const options=require(path.join(ext.extensionPath,'out/serviceConsole.js')).serviceRuntimeOptions();
        if(options.installDir!==config.install||options.managed)throw Error('Actual installed runtime options escaped fixture');
        const {RuntimeHost}=require(config.helper);
        const observation=await new RuntimeHost(ext.extensionPath,config.version,options).discover();
        if(observation.installDir!==config.install)throw Error('Actual native discovery selected an ambient installation');
        await write('pre-activation-location',{options,installDir:observation.installDir,location:observation.location,installed:observation.installed});
        await vscode.workspace.getConfiguration('airk').update('runtime.nativeExecution',true,vscode.ConfigurationTarget.Global);
        api=await ext.activate();await api.whenReady();
    }catch(error){bootError=String(error);}
    await write('ready',{version:ext?.packageJSON.version,path:ext?.extensionPath,editor:vscode.version,trusted:vscode.workspace.isTrusted,bootError,pid:process.pid});
    let sequence=0;
    while(Date.now()<config.deadline){
        const filename=path.join(folder,'request-'+(sequence+1)+'.json');let request;
        try{request=JSON.parse(await fs.readFile(filename,'utf8'));}catch(error){if(error.code!=='ENOENT')throw error;await new Promise(resolve=>setTimeout(resolve,80));continue;}
        sequence++;let value,error;
        try{
            if(request.action==='definitions'){
                value=[];for(const row of await api.getServerDefinitions()){
                    if(row.uri){
                        const helpers=path.dirname(config.helper),{httpMaterial}=require(path.join(helpers,'serviceHttp.js'));
                        const {installedHttpIdentity}=require(config.httpHelper),identity=installedHttpIdentity(row),host=new (require(config.helper).RuntimeHost)(ext.extensionPath,config.version,require(path.join(ext.extensionPath,'out/serviceConsole.js')).serviceRuntimeOptions());
                        const observed=await host.discover();if(observed.installDir!==config.install||!observed.inspection)throw Error('HTTP discovery escaped actual installed fixture');
                        const info=await host.httpConnectionInfo(host.binding(observed.inspection,observed.installDir),identity.credentialId);
                        value.push(require(config.contracts).httpDefinitionSnapshot(row,info,await httpMaterial(info)));
                    }else value.push({command:row.command,args:row.args,env:row.env,cwd:row.cwd?.fsPath??row.cwd});
                }
            }
            else if(request.action==='open')value=await vscode.commands.executeCommand('airk.explorer.open',request.route);
            else if(request.action==='search')value=await vscode.commands.executeCommand('airk.explorer.search','UpgradeOriginalEvidence');
            else if(request.action==='index'){await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(config.source,'fixture.ts'))));value=await vscode.commands.executeCommand('airk.index');}
            else if(request.action==='tabs')value=vscode.window.tabGroups.all.flatMap(group=>group.tabs).filter(tab=>tab.label==='AIRK Explorer').map(tab=>({label:tab.label,isActive:tab.isActive}));
            else if(request.action==='disableNative'){await vscode.workspace.getConfiguration('airk').update('runtime.nativeExecution',false,vscode.ConfigurationTarget.Global);value={disabled:true};}
            else if(request.action==='quit'){await write('reply-'+sequence,{value:{quitting:true}});await vscode.commands.executeCommand('workbench.action.quit');return;}
            else throw Error('Unknown private driver action');
        }catch(problem){error=String(problem);}
        await write('reply-'+sequence,{value:value??null,error});
    }
    throw Error('Private driver exceeded shared deadline');
}
async function main(argv=process.argv.slice(2)){
    assert(process.platform==='darwin'&&process.arch==='arm64','This pinned public ARM VSIX qualification requires an Apple Silicon Aqua Code session');
    assert(argv.length===5&&/^[a-f0-9]{64}$/.test(argv[4]),'Expected Code, cli.js, old VSIX, current VSIX, current SHA256');
    const [editor,cli,oldVsix,newVsix]=argv.slice(0,4).map(value=>path.resolve(value)),currentSha=argv[4];
    const environment=privateEditorEnvironment(process.env),deadline=Date.now()+1200000;
    const artifacts=path.join(root,'artifacts/editor-qualification');await fs.mkdir(artifacts,{recursive:true});
    const run=await fs.realpath(await fs.mkdtemp(path.join(artifacts,'actual-vsix-upgrade-')));await fs.chmod(run,0o700);
    const clientHome=path.join(run,'client-home');await fs.mkdir(clientHome,{mode:0o700});Object.assign(environment,privateClientEnvironment(environment,clientHome));
    const result={schema:1,scope:'existing OS UID; private client home and Code profile; actual installed VSIXes',run,deadline,oldVersion:'1.8.7',currentVersion:CURRENT_VERSION,uiLimit:256,mcpLimit:128,legacyRestart:'Only the owned private 1.8.7 Code process is closed/reopened. 1.8.7 cannot acquire a future bootstrap before activation.',checks:[],phases:[]};
    const save=async(name,value)=>{const bytes=JSON.stringify(value,null,2);assert(Buffer.byteLength(bytes)<=8388608,'Qualification evidence exceeds 8 MiB');await fs.writeFile(path.join(run,name+'.json'),bytes);};
    const active=new Set(),ownedExits=new Map();let ui,stopProof,receiptPlan,profile,uiCount=0,mcpCount=0,editorCloseObserved=true,sourceBaseline,uncertainMcp=false,verificationBundle;
    function remaining(max=30000){assert(Date.now()<deadline,'Shared 20 minute deadline exceeded');return Math.max(1,Math.min(max,deadline-Date.now()));}
    async function command(program,args,input){const output=await execute(program,args,{cwd:run,env:environment,timeout:remaining(),maxBuffer:2097152,input});return output;}
    async function native(bundle,args,input){
        if(input===undefined)return JSON.parse((await command(bundle.main,args)).stdout);
        return boundedJsonProcess(bundle.main,args,input,{cwd:run,env:environment,timeout:remaining(),onExit:receipt=>save('native-json-exit-'+receipt.pid,receipt)});
    }
    async function processCode(args,label,extra={}){
        const log=await fs.open(path.join(run,label+'.log'),'wx');
        const child=spawn(editor,args,{cwd:run,env:{...environment,...extra},stdio:['ignore',log.fd,log.fd]});active.add(child);editorCloseObserved=false;
        const observedClose=new Promise(resolve=>child.once('close',(code,signal)=>resolve({code,signal})));ownedExits.set(child,observedClose);
        const closed=new Promise((resolve,reject)=>{const timer=setTimeout(()=>{child.kill();reject(Error('Owned Code process deadline: '+label));},remaining(1200000));child.once('error',reject);child.once('close',async(code,signal)=>{clearTimeout(timer);active.delete(child);await log.close();await save(label+'-exit',{pid:child.pid,code,signal});resolve({code,signal});});});
        closed.catch(()=>{});return {child,closed};
    }
    async function install(vsix,label,extensions,shared){
        const task=await processCode([cli,'--user-data-dir',profile,'--extensions-dir',extensions,'--shared-data-dir',shared,'--install-extension',vsix,'--force'],label,{ELECTRON_RUN_AS_NODE:'1'});
        assert((await task.closed).code===0,'Actual Code CLI installation failed');editorCloseObserved=active.size===0;
    }
    const extract=String.raw`import zipfile,sys,os,stat
with zipfile.ZipFile(sys.argv[1]) as z:
 entries=z.infolist()
 if len(entries)>4096 or sum(e.file_size for e in entries)>1073741824: raise RuntimeError('VSIX extraction bound')
 seen=set()
 for e in entries:
  if not e.filename.startswith('extension/'): continue
  name=e.filename[len('extension/'):]
  if not name: continue
  parts=name.split('/')
  if any(p in ('','..','.') for p in parts[:-1]) or '..' in parts or '\\' in name or name.startswith('/') or ':' in name or name in seen or stat.S_ISLNK(e.external_attr>>16): raise RuntimeError('Unsafe VSIX path')
  seen.add(name); dest=os.path.join(sys.argv[2],*parts)
  if e.is_dir(): os.makedirs(dest,exist_ok=True); continue
  if e.file_size>536870912: raise RuntimeError('VSIX file bound')
  os.makedirs(os.path.dirname(dest),exist_ok=True)
  with z.open(e) as src,open(dest,'xb') as out:
   total=0
   while True:
    block=src.read(1048576)
    if not block: break
    total+=len(block)
    if total>e.file_size: raise RuntimeError('VSIX stream bound')
    out.write(block)
  if e.external_attr>>16 & 0o111: os.chmod(dest,0o755)
`;
    async function authenticatedBundle(vsix,version,digest,label){
        const info=await fs.lstat(vsix);assert(info.isFile()&&!info.isSymbolicLink()&&info.size<268435456,'VSIX must be one bounded regular file');
        assert(sha(await fs.readFile(vsix))===digest,label+' VSIX checksum mismatch');
        const stage=path.join(run,label+'-verified-stage');await fs.mkdir(stage);
        await command(process.env.PYTHON||'python3',['-c',extract,vsix,stage]);
        const manifest=JSON.parse(await fs.readFile(path.join(stage,'package.json'),'utf8'));assert(manifest.publisher==='iokaio'&&manifest.name==='airk'&&manifest.version===version,'Unexpected VSIX identity');
        const bundle=await require(path.join(root,'packages/vscode/out/serviceBootstrap.js')).verifyBundledRuntime(stage,version);
        const app=path.join(bundle.source,'AIRK.app');await command('/usr/bin/codesign',['--verify','--deep','--strict','--verbose=2',app]);
        const cert=path.join(run,label+'-public-cert');await command('/usr/bin/codesign',['-d','--extract-certificates='+cert,app]);
        assert(sha(await fs.readFile(cert+'0'))===LEAF_SHA,'Signed application leaf differs from independently verified Ioka certificate');
        await command('/usr/bin/xcrun',['stapler','validate',app]);
        await save(label+'-payload',{vsix,sha256:digest,version,bundle,leaf_sha256:LEAF_SHA});return bundle;
    }
    async function boundedUi(action){
        // Frozen editor-ui branches used below dispatch at most 32 mutations.
        // Reserve the whole branch before entry, then charge actual dispatches.
        assert(uiCount+32<=256,'256 UI action budget exhausted');const before=ui.dispatchReceipts().length;
        try{return await action();}finally{const dispatched=ui.dispatchReceipts().length-before;uiCount+=dispatched;assert(dispatched<=32&&uiCount<=256,'Frozen UI branch exceeded reserved mutation budget');}
    }
    async function waitRead(read,label,end=deadline){while(Date.now()<Math.min(end,deadline)){const value=await read();if(value!==undefined)return value;await delay(100);}throw Error(label+' exceeded observation deadline');}
    let phaseSequence=0;
    async function startPhase(version,extensions,shared,source,driver,install){
        const folder=path.join(run,'phase-'+version);await fs.mkdir(folder);await fs.writeFile(path.join(folder,'config.json'),JSON.stringify({version,extensions,install,source,deadline,helper:path.join(root,'packages/vscode/out/serviceBootstrap.js'),contracts:__filename,httpHelper:path.join(root,'packages/vscode/out/test/installedEditorHttp.js')}));
        const server=net.createServer();await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const port=server.address().port;await new Promise(resolve=>server.close(resolve));
        const task=await processCode([source,'--new-window','--user-data-dir',profile,'--extensions-dir',extensions,'--shared-data-dir',shared,'--remote-debugging-port',String(port),'--remote-debugging-address','127.0.0.1','--skip-welcome','--skip-release-notes','--disable-updates','--disable-telemetry','--disable-gpu','--disable-workspace-trust','--extensionDevelopmentPath='+driver],'editor-'+version,{AIRK_UPGRADE_PHASE:folder});
        ui=await editorUi(port,deadline);phaseSequence=0;
        async function rpc(action,fields={}){
            if(['open','search','index','quit'].includes(action))assert(++uiCount<=256,'256 UI action budget exhausted');
            const id=++phaseSequence;await fs.writeFile(path.join(folder,'request-'+id+'.json'),JSON.stringify({action,...fields}));
            const reply=await waitRead(async()=>{try{return JSON.parse(await fs.readFile(path.join(folder,'reply-'+id+'.json'),'utf8'));}catch(error){if(error.code==='ENOENT')return;throw error;}},'Driver '+action);
            assert(!reply.error,reply.error);return reply.value;
        }
        return {folder,task,rpc,location:()=>waitRead(async()=>{try{return JSON.parse(await fs.readFile(path.join(folder,'pre-activation-location.json'),'utf8'));}catch(error){if(error.code==='ENOENT')return;throw error;}},'Verified native installation binding before execution'),ready:()=>waitRead(async()=>{try{return JSON.parse(await fs.readFile(path.join(folder,'ready.json'),'utf8'));}catch(error){if(error.code==='ENOENT')return;throw error;}},'Actual installed extension activation')};
    }
    async function closePhase(phase){await save('ui-'+path.basename(phase.folder),{diagnostics:ui.diagnostics(),dispatches:ui.dispatchReceipts()});await phase.rpc('quit');assert((await phase.task.closed).code===0,'Private editor did not quit normally');editorCloseObserved=active.size===0;await ui.close();ui=undefined;}
    async function mcp(definition,check){
        if(definition?.transport==='https'){
            assert(!uncertainMcp,'Previous owned transport close remains uncertain');
            const install=path.join(run,'native-service'),info=definition.nativeConnection;
            const {validateHttpConnection,httpMaterial,sameHttpAuthority}=require(path.join(root,'packages/vscode/out/serviceHttp.js'));
            validateHttpConnection(info);
            const metadata=await native(verificationBundle,['runtime','inspect','--install-dir',install]);
            const pair=JSON.parse(await fs.readFile(path.join(verificationBundle.source,'airk-runtime.json'),'utf8'));
            assert(pair.schema===2&&metadata.launcher===null&&metadata.payload_verified===true&&metadata.approval?.manifest_sha256===verificationBundle.digest,'Modern HTTP requires the exact verified main-only native payload');
            const currentInfo=validateHttpConnection(await native(verificationBundle,['runtime','http-connection-info','--install-dir',install,'--connection-id',info.connection_id]));
            assert(sameHttpAuthority(currentInfo.selection,info.selection)&&isDeepStrictEqual(currentInfo.endpoint,info.endpoint)&&currentInfo.endpoint.executable_sha256===pair.airk_sha256&&currentInfo.endpoint.namespace===metadata.registry_namespace,'Native HTTP authority/endpoint changed since observation');
            const {InstalledHttpTransport}=require(path.join(root,'packages/vscode/out/test/installedEditorHttp.js'));
            const material=await httpMaterial(currentInfo),actual=await restoreHttpDefinition(definition,material),transport=new InstalledHttpTransport(actual,material.ca);
            let primary;
            try{
                const initialized=await transport.request('initialize',{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'airk-installed-upgrade-test',version:CURRENT_VERSION}},remaining(90000));
                assert(!initialized.error,'Actual HTTPS initialize refused');await transport.notify('notifications/initialized',remaining());
                const catalog=await transport.request('tools/list',{},remaining()),{UNIFIED_TOOLS}=require(path.join(root,'packages/vscode/out/model.js'));
                assert(!catalog.error&&isDeepStrictEqual(catalog.result.tools.map(tool=>tool.name).sort(),[...UNIFIED_TOOLS].sort()),'Actual HTTPS tool catalog differs');
                await save('https-catalog-'+info.connection_id,{definition,initialized,catalog});
                return await check(async(name,args)=>{
                    assert(['status','symbols','search'].includes(name),'Only bounded native reads may retry');const {readRetryDelay}=require(path.join(root,'packages/vscode/out/readRetry.js'));
                    return toolBody(await boundedSafeRead({deadline,retryDelay:wire=>readRetryDelay(wire,info.selection.kind==='scoped'?info.selection.selection:undefined),request:async timeout=>{
                        assert(++mcpCount<=128,'128 MCP read budget exhausted');const id=mcpCount,wire=await transport.request('tools/call',{name,arguments:{...args,response_mode:'full'}},timeout);
                        await save('mcp-read-'+id,{definition,name,args,wire});assert(!wire.error,'Actual HTTPS RPC refusal');return wire.result;
                    }}));
                });
            }catch(error){primary=error;await save('https-failure-'+info.connection_id,{error:String(error),detail:error.detail,nativeResponse:error.nativeResponse});throw error;}finally{
                try{await save('https-close-'+info.connection_id,{receipt:await transport.close(),observations:transport.observations});}
                catch(error){uncertainMcp=true;await save('https-close-uncertain-'+info.connection_id,{error:String(error),primary:primary?String(primary):null});if(!primary)throw error;}
            }
        }
        assert(verificationBundle.version==='1.8.7','A modern candidate cannot claim retrieval success from retired stdio');
        assert(typeof definition?.command==='string'&&inside(run,definition.command)&&Array.isArray(definition.args)&&definition.args.includes('mcp'),'MCP definition is outside owned native fixture');
        assert(!uncertainMcp,'Previous owned MCP exit remains uncertain');
        const install=path.join(run,'native-service'),stat=await fs.lstat(definition.command),actual=await fs.realpath(definition.command);
        assert(stat.isFile()&&!stat.isSymbolicLink()&&stat.nlink===1&&stat.size<=536870912&&actual===definition.command&&inside(install,actual),'MCP executable redirects outside exact owned native installation');
        const metadata=await native(verificationBundle,['runtime','inspect','--install-dir',install]);assert(metadata.payload_verified===true&&metadata.approval?.manifest_sha256===verificationBundle.digest,'MCP native approval differs from phase payload');
        if(actual===path.join(install,'airk-launcher')){const launcher=JSON.parse(await fs.readFile(path.join(install,'launcher.json'),'utf8'));assert(launcher.schema===1&&launcher.sha256===sha(await fs.readFile(actual)),'MCP launcher differs from native receipt');}
        else{const pair=JSON.parse(await fs.readFile(path.join(verificationBundle.source,'airk-runtime.json'),'utf8'));assert(actual===metadata.executable&&sha(await fs.readFile(actual))===pair.airk_sha256,'MCP executable differs from verified native selected pair');}
        const {McpStdioSession}=require(path.join(root,'packages/vscode/out/mcpStdioSession.js'));
        let child,closed,opened,primary;
        try{
            opened=await McpStdioSession.open({command:definition.command,args:definition.args,env:definition.env,cwd:definition.cwd??run,clientName:'AIRK actual installed VSIX upgrade qualification',clientVersion:CURRENT_VERSION,launch:(command,args,options)=>{
                child=spawn(command,args,{...options,stdio:'pipe'});closed=new Promise(resolve=>child.once('close',(code,signal)=>resolve({code,signal})));return child;
            }},remaining(90000));
            return await check(async(name,args)=>{
                assert(['status','symbols','search'].includes(name),'Only bounded native reads may retry');const {readRetryDelay}=require(path.join(root,'packages/vscode/out/readRetry.js'));
                return toolBody(await boundedSafeRead({deadline,retryDelay:readRetryDelay,request:async timeout=>{
                    assert(++mcpCount<=128,'128 MCP read budget exhausted');const id=mcpCount,wire=await opened.session.request('tools/call',{name:'airk_'+name,arguments:{...args,response_mode:'full'}},timeout);await save('mcp-read-'+id,{definition,name,args,wire});return wire;
                }}));
            });
        }catch(error){primary=error;throw error;}finally{
            opened?.session.close();if(child){try{if(!opened)child.kill();await save('mcp-exit-'+child.pid,await waitOwnedExit(child,closed,5000));}catch(error){uncertainMcp=true;await save('mcp-exit-uncertain-'+child.pid,{error:String(error),primary:primary?String(primary):null});if(!primary)throw error;}}
        }
    }
    try{
        // API metadata is public provenance, not executable material or a secret.
        const release=JSON.parse((await command('gh',['api','repos/iokaio/airk/releases/tags/v1.8.7'])).stdout);
        const asset=release.assets?.find(row=>row.name==='airk-1.8.7-darwin-arm64.vsix');assert(asset?.digest==='sha256:'+OLD_SHA,'Public old VSIX release digest no longer matches reviewed checksum');await save('old-release-provenance',{release_id:release.id,tag:release.tag_name,published_at:release.published_at,asset});
        const old=await authenticatedBundle(oldVsix,'1.8.7',OLD_SHA,'old'),current=await authenticatedBundle(newVsix,CURRENT_VERSION,currentSha,'current');
        verificationBundle=old;
        const source=path.join(run,'source'),storage=path.join(run,'storage'),installDir=path.join(run,'native-service'),extensions=path.join(run,'extensions'),shared=path.join(run,'shared');
        for(const folder of [source,storage,extensions,shared])await fs.mkdir(folder);
        const fixture=Buffer.from('export function UpgradeOriginalEvidence() { return 42; }\n');await fs.writeFile(path.join(source,'fixture.ts'),fixture,{flag:'wx'});const sourceHash=sha(fixture);sourceBaseline={source,hash:sourceHash};
        profile=await fs.realpath(await fs.mkdtemp('/tmp/airk-vsix-'));await fs.chmod(profile,0o700);await save('private-profile',{profile,source,storage,installDir});
        await fs.mkdir(path.join(profile,'User'));await fs.writeFile(path.join(profile,'User/settings.json'),JSON.stringify({'airk.service.channel':'stable','airk.service.installDir':installDir,'airk.runtime.nativeExecution':false,'airk.cli.autoInstall':true,'airk.cli.shellIntegration':'off','airk.storage.path':storage,'extensions.autoUpdate':false,'update.mode':'none','telemetry.telemetryLevel':'off'}));
        const location=await native(current,['runtime','location','--channel','stable']);assert(location.protocol===1&&location.install_dir!==installDir&&!await exists(installDir),'Custom native layout is not fresh');
        receiptPlan=await prepareOwnedReceipts({launcher:location.user_paths.launcher,fixture:run,installDir,sourceRoots:[source]});await save('native-location',{location,expected:{state:installDir,logs:path.join(installDir,'logs'),control:path.join(installDir,'control')}});
        const driver=path.join(run,'driver');await fs.mkdir(driver);await fs.writeFile(path.join(driver,'package.json'),JSON.stringify({name:'actual-vsix-upgrade-driver',publisher:'local-test',version:'0.0.1',engines:{vscode:'^1.140.0'},main:'driver.cjs',activationEvents:['onStartupFinished']}));
        await fs.writeFile(path.join(driver,'driver.cjs'),'exports.activate=()=>('+privateDriver.toString()+')().catch(error=>{require("node:fs").writeFileSync(require("node:path").join(process.env.AIRK_UPGRADE_PHASE,"driver-failure.json"),JSON.stringify({error:String(error),stack:error.stack}));});');
        await install(oldVsix,'install-old',extensions,shared);
        const previous=await startPhase('1.8.7',extensions,shared,source,driver,installDir);
        const previousLocation=await previous.location();assert(previousLocation.installDir===installDir&&previousLocation.options.installDir===installDir&&!previousLocation.options.managed,'Old observed runtime escaped canonical fixture before Setup');await save('old-location-before-setup',previousLocation);
        const setupStarted=Date.now();await boundedUi(()=>ui.confirm('Set up',['Set up AIRK on this computer?','1.8.7',source,storage,'No shell startup file or user PATH is changed']));
        const oldReady=await previous.ready();assert(!oldReady.bootError,oldReady.bootError);await save('old-activation',oldReady);
        await previous.rpc('open',{route:'#overview'});
        const oldDefinitions=await waitRead(async()=>{
            const definitions=await previous.rpc('definitions');if(definitions.length)return definitions;
            const diagnosis=await native(old,['runtime','diagnose','--install-dir',installDir]);
            return diagnosis.activation_result?.outcome==='aborted_or_failed'?[]:undefined;
        },'Actual old Setup completion or explicit failure',Math.min(deadline,Date.now()+240000));
        const oldObservation={definitions:oldDefinitions};try{oldObservation.inspect=await native(old,['runtime','inspect','--install-dir',installDir]);}catch(error){oldObservation.inspectError=String(error);}await save('old-observation',oldObservation);
        let baseline,oldFailure;
        if(oldObservation.definitions.length===1){
            try{baseline=await mcp(oldObservation.definitions[0],async call=>{
                const until=Math.min(deadline,Date.now()+180000);let symbols;
                do{symbols=await call('symbols',{name:'UpgradeOriginalEvidence',compact:false});if(citations(symbols).length){assertEvidence(symbols,sourceHash);return {symbols,status:await call('status',{}),search:await call('search',{query:'UpgradeOriginalEvidence',mode:'lexical',limit:5})};}await delay(1000);}while(Date.now()<until);
                throw Error('Old native indexing never cited original fixture');
            });}catch(error){oldFailure=String(error);}
        }
        const oldRendered=(await ui.document("!!document.querySelector('.airk-explorer')","({body:document.body.innerText.slice(0,32768),notices:[...document.querySelectorAll('.notice')].map(n=>n.innerText.slice(0,8192))})"))?.value;
        oldObservation.diagnose=await native(old,['runtime','diagnose','--install-dir',installDir]);await save('old-native-diagnose',oldObservation.diagnose);
        if(!baseline){
            // A deadline alone is not a qualified failed-install cause.
            const actualFailure=failedActivation(oldObservation.diagnose.activation_result,installDir,old.digest,setupStarted);
            const launcher=path.join(installDir,'airk-launcher'),before=await fs.lstat(launcher),canonical=await fs.realpath(launcher);
            assert(before.isFile()&&!before.isSymbolicLink()&&before.nlink===1&&before.size<=536870912,'Actual old Setup did not produce one bounded regular standalone launcher');
            const actualHash=sha(await fs.readFile(launcher)),inventory=JSON.parse(await fs.readFile(path.join(old.source,'airk-install.json'),'utf8'));
            let strict;const args=['--verify','--strict','--verbose=4',launcher];
            try{const output=await command('/usr/bin/codesign',args);strict={command:'/usr/bin/codesign',args,status:0,stdout:output.stdout,stderr:output.stderr};}
            catch(error){strict={command:'/usr/bin/codesign',args,status:error.code,signal:error.signal??null,stdout:error.stdout??'',stderr:error.stderr??''};}
            await save('old-copy-strict-codesign',strict);
            const after=await fs.lstat(launcher);assert(after.dev===before.dev&&after.ino===before.ino&&after.size===before.size&&after.mtimeMs===before.mtimeMs&&sha(await fs.readFile(launcher))===actualHash,'Old copied launcher changed during strict signature observation');
            const copyDefect=qualifyOldLauncherCopy({install:installDir,launcher,canonical,regular:before.isFile()&&!before.isSymbolicLink(),nlink:before.nlink,actualHash,authenticatedMainHash:sha(await fs.readFile(old.main)),inventoryMainHash:inventory.files?.['AIRK.app/Contents/MacOS/airk'],strict});
            result.journey='failed-install-recovery';result.oldFailure={error:oldFailure??null,actualFailure,copyDefect,rendered:oldRendered};
        }else{assertEvidence(baseline.search,sourceHash);result.journey='indexed-upgrade';result.oldBaseline=baseline;}
        await save('old-outcome',result);assert((await previous.rpc('tabs')).length===1,'Exactly one old Explorer must be left open');
        // Real VSIX replacement while the old Explorer is still open.
        await previous.rpc('disableNative');await install(newVsix,'install-current',extensions,shared);await closePhase(previous);await gatePrivateProfile(profile,installDir);
        const next=await startPhase(CURRENT_VERSION,extensions,shared,source,driver,installDir),nextLocation=await next.location();assert(nextLocation.installDir===installDir&&nextLocation.options.installDir===installDir&&!nextLocation.options.managed,'Current observed runtime escaped canonical fixture');await save('current-location-before-start',nextLocation);
        const ready=await next.ready();assert(!ready.bootError,ready.bootError);await save('current-activation',ready);
        verificationBundle=current;
        await next.rpc('open',{route:'#manage/service'});await boundedUi(()=>ui.indexing({action:'connect'}));
        const observation=await native(current,['runtime','inspect','--install-dir',installDir]),diagnose=await native(current,['runtime','diagnose','--install-dir',installDir]);assert(observation.payload_verified===true&&observation.approval?.manifest_sha256===current.digest&&diagnose.pending_activation===null&&diagnose.activation_error===null&&observation.control_dir===path.join(installDir,'control'),'Current native installation/activation is not verified');
        const health=await native(current,['service','rpc','--runtime-dir',observation.control_dir,'--stdin'],'health'),pair=JSON.parse(await fs.readFile(path.join(current.source,'airk-runtime.json'),'utf8'));
        assert(health.status==='health'&&health.observation?.version===CURRENT_VERSION&&health.observation.protocol===1&&health.observation.runtime_sha256===current.digest&&health.observation.executable_sha256===pair.airk_sha256&&/^[a-f0-9]{64}$/.test(health.observation.build?.source_sha256)&&Number.isSafeInteger(health.observation.build?.source_files),'Missing authenticated current native version/pair/build health');await save('current-native',{observation,diagnose,health});
        await next.rpc('open',{route:'#indexing/library'});let library=await boundedUi(()=>ui.indexing({action:'page',kind:'indexes'})),index,name;
        if(baseline){const rows=library.page?.page?.items??[];const matches=rows.filter(row=>row.kind==='index'&&row.value?.backing?.root===source);assert(matches.length===1,'Original imported source index must be unique');index=matches[0].value.id;name=matches[0].value.name;}
        else{
            await boundedUi(()=>ui.indexing({action:'setupDraft',name:'Recovered actual VSIX fixture',source,storage:path.join(storage,'reviewed-recovery'),collection:'actual-vsix-recovery'}));
            const applied=await boundedUi(()=>ui.indexing({action:'setupApply'}));assert(applied.outcome?.status==='catalog_index_setup_applied'&&applied.outcome.receipt.stage==='ready','Explicit recovery setup did not become ready');index=applied.outcome.receipt.index_id;
            const detail=await boundedUi(()=>ui.indexing({action:'inspect',kind:'index',id:index}));assert(detail.detail.detail.item.value.backing.root===source,'Reviewed recovery changed source');name=detail.detail.detail.item.value.name;
        }
        await boundedUi(()=>ui.indexing({action:'inspect',kind:'index',id:index}));
        const sourceView=await boundedUi(()=>ui.indexing({action:'sources'})),sourceIds=sourceView.sources.page.items.map(row=>row.value).filter(row=>row.index_id===index&&row.enabled===true&&row.canonical_path===source).map(row=>row.id);assert(sourceIds.length===1,'Selected index did not retain exact configured originating source');
        await next.rpc('open',{route:'#indexing/projects'});const setName='Actual VSIX upgrade selection',saved=await boundedUi(()=>ui.indexing({action:'setCreate',name:setName,indexName:name}));const project=saved.outcome?.id;assert(/^prj_[a-f0-9]{32}$/.test(project),'Public index set not created');
        await next.rpc('open',{route:'#indexing/window'});const selection=await boundedUi(()=>ui.indexing({action:'scopeSelect',setName,project}));assert(selection.outcome?.status==='search_scope_selected','Reviewed scope did not apply');await save('current-selection',selection);
        const definitions=await next.rpc('definitions');assert(definitions.length===1,'Current selected native definition must be unique');
        const {installedSelector,installedMember}=require(path.join(root,'packages/vscode/out/test/installedEditorContracts.js'));
        const selected={index,project,grant:selection.outcome.activation.selection.session.grant_id,source_ids:sourceIds};assert(definitions[0].transport==='https','Current actual provider must advertise authenticated HTTPS');
        const {httpMaterial}=require(path.join(root,'packages/vscode/out/serviceHttp.js')),actualDefinition=await restoreHttpDefinition(definitions[0],await httpMaterial(definitions[0].nativeConnection));selected.session=installedSelector(actualDefinition,selection,selected);
        await next.rpc('index');const reads=await mcp(definitions[0],async call=>{
            const end=Math.min(deadline,Date.now()+180000);let symbols;
            do{symbols=await call('symbols',{name:'UpgradeOriginalEvidence',compact:false});if(citations(installedMember(symbols,selected)).length)break;await delay(1000);}while(Date.now()<end);
            assertEvidence(installedMember(symbols,selected),sourceHash);return {status:await call('status',{}),symbols,search:await call('search',{query:'UpgradeOriginalEvidence',mode:'lexical',limit:5})};
        });const materialized=Object.fromEntries(Object.entries(reads).map(([name,body])=>[name,installedMember(body,selected)]));assertEvidence(materialized.symbols,sourceHash);assertEvidence(materialized.search,sourceHash);await save('current-mcp',{definitions,selected,reads,materialized});
        await next.rpc('search');const rendered=await ui.until(async()=>(await ui.document("!!document.querySelector('.airk-explorer')","(()=>{const paths=[...document.querySelectorAll('.hit code.path')].map(n=>n.textContent);return paths.some(p=>p.startsWith('fixture.ts'))?{paths,text:document.body.innerText.slice(0,32768)}:undefined;})()"))?.value,'Current Explorer cited original fixture');await save('current-explorer',rendered);
        await next.rpc('open',{route:'#indexing/window'});assert((await next.rpc('tabs')).length===1,'Repeated navigation created duplicate Explorer');
        await next.rpc('open',{route:'#manage/service'});const beforeStop=await native(current,['runtime','inspect','--install-dir',installDir]);
        assert(!uncertainMcp,'Native Stop cannot run before all owned stdio processes or HTTPS sockets have observed closure');
        const stop=await boundedUi(()=>ui.indexing({action:'serviceStop',installDir,runtimeSha256:current.digest,namespace:beforeStop.registry_namespace}));await save('native-stop',stop);
        stopProof=completeStop(stop,await native(current,['runtime','inspect','--install-dir',installDir]),current,installDir);await closePhase(next);
        assert(sha(await fs.readFile(path.join(source,'fixture.ts')))===sourceHash&&isDeepStrictEqual(await fs.readdir(source),['fixture.ts']),'Original source changed');
        result.source={path:path.join(source,'fixture.ts'),before_sha256:sourceHash,after_sha256:sourceHash};
        result.receiptCleanup=await cleanupOwnedReceipts(receiptPlan,{profileRoots:[profile],editorsClosed:editorCloseObserved,stop:stopProof,evidenceDirectory:path.join(run,'uninstall-receipt-evidence')});
        result.checks.push('authenticated public old VSIX and exact signed native leaf','actual old Setup UI review and native ownership','private legacy Code restart only','verified current native activation and authenticated health','full native MCP source citations and explicit modern scope','one Explorer and public captured native Stop','original source bytes preserved','only exact newly owned account receipts removed');
        result.status='passed';
    }catch(error){result.status='failed';result.error=String(error);result.stack=error.stack;throw error;}
    finally{
        if(sourceBaseline){try{result.sourceFinally={before:sourceBaseline.hash,after:sha(await fs.readFile(path.join(sourceBaseline.source,'fixture.ts'))),files:await fs.readdir(sourceBaseline.source)};assert(result.sourceFinally.after===sourceBaseline.hash&&isDeepStrictEqual(result.sourceFinally.files,['fixture.ts']),'Original source changed on failed/successful path');}catch(error){result.sourcePreservationFailure=String(error);result.status='failed';process.exitCode=1;}}
        if(ui){try{await save('last-ui-diagnostics',{diagnostics:ui.diagnostics(),dispatches:ui.dispatchReceipts()});await ui.close();}catch(error){result.uiCleanupError=String(error);}}
        // Kill only children created here. Never an ambient Code/native process.
        result.editorExitCleanup=[];
        for(const child of active){child.kill();try{result.editorExitCleanup.push(await waitOwnedExit(child,ownedExits.get(child)));}catch(error){result.editorExitCleanup.push({pid:child.pid,closed:false,error:String(error)});result.status='failed';process.exitCode=1;}}
        result.uiActions=uiCount;result.mcpReads=mcpCount;result.profile=profile;result.profileRetained=true;result.fixtureRetained=true;
        result.cleanupScope='Profile and native fixture retained as evidence; failed/unknown service or receipt cleanup is never hidden by deleting its profile.';
        await save('result',result);process.stdout.write(path.join(run,'result.json')+'\n');
    }
    return result;
}
module.exports={CURRENT_VERSION,httpDefinitionSnapshot,restoreHttpDefinition,OLD_SHA,LEAF_SHA,OLD_MAIN_SHA,inside,toolBody,citations,assertEvidence,completeStop,failedActivation,qualifyOldLauncherCopy,gatePrivateProfile,waitOwnedExit,boundedJsonProcess,boundedSafeRead,privateDriver,main};
if(require.main===module)main().catch(error=>{process.stderr.write(String(error)+'\n');process.exitCode=1;});
