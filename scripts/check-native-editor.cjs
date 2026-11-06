// SPDX-License-Identifier: Apache-2.0
// Native extension acceptance with private profiles; accepts a reviewed stage or VSIX.
const fs=require('node:fs/promises'),path=require('node:path'),net=require('node:net');
const {spawn,execFile}=require('node:child_process');
const {promisify}=require('node:util');
const execute=promisify(execFile),services=[],legacyServices=new Map();
let bundle;
const {editorUi,privateEditorEnvironment}=require('./editor-ui.cjs');
const root=path.resolve(__dirname,'..');
const [editor,cli,source]=process.argv.slice(2).map(value=>path.resolve(value));
if(!source)throw new Error('Expected editor, its cli.js, and an approved extension stage or VSIX.');
const scenarios=(process.env.AIRK_EDITOR_SCENARIOS||'same,different,multi,usage,untrusted,legacy').split(',');
if(scenarios.some(value=>!['same','different','multi','usage','untrusted','legacy'].includes(value)))throw new Error('Unknown native editor scenario');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms)),active=new Map();
const environment=privateEditorEnvironment(process.env); // Child only: never inherit the normal editor's bootstrap/cache/IPC role.
async function runProcess(args,logfile,extra={}){
    const log=await fs.open(logfile,'w'),deadline=Number(extra.AIRK_EDITOR_DEADLINE)||Date.now()+600000;
    return new Promise((resolve,reject)=>{
        const child=spawn(editor,args,{windowsHide:true,env:{...environment,...extra},stdio:['ignore',log.fd,log.fd]});
        let failure,forced=false,killTimer,hardTimer;
        const closed=new Promise(done=>child.once('close',(code,signal)=>done({code,signal})));
        active.set(child,{closed,logfile});
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
const sha=bytes=>require('node:crypto').createHash('sha256').update(bytes).digest('hex');
function inside(root,value){const normalize=value=>process.platform==='win32'?path.toNamespacedPath(value).toLowerCase():value;const relative=path.relative(normalize(root),normalize(value));return relative!==''&&!relative.startsWith('..'+path.sep)&&relative!=='..'&&!path.isAbsolute(relative);}
async function ownedPath(root,value,missingLeaf=false){
    if(typeof value!=='string'||!path.isAbsolute(value))throw new Error('Expected absolute owned fixture path');
    let resolved;try{resolved=await fs.realpath(value);}catch(error){if(!missingLeaf||error.code!=='ENOENT')throw error;resolved=path.join(await fs.realpath(path.dirname(value)),path.basename(value));}
    if(!inside(root,resolved))throw new Error('Private editor request escaped its owned fixture');return resolved;
}
async function canonicalOwned(root,value){const actual=await ownedPath(root,value);const normalize=p=>process.platform==='win32'?path.toNamespacedPath(p).toLowerCase():p;if(normalize(path.resolve(value))!==normalize(actual))throw new Error('Legacy fixture path is not canonical');return actual;}
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
    // initialize then chooses state/logs. No ambient HOME/APPDATA is changed.
    const expected={installDir:install,stateDir:install,controlDir:path.join(install,'control'),logsDir:path.join(install,'logs')};
    if(Object.values(expected).some(value=>!inside(fixture,value)))throw new Error('Planned fixture runtime state escaped the owned scenario');
    await fs.writeFile(path.join(folder,'fixture-layout.json'),JSON.stringify({nativeLocation:location,expected,source_contract:'runtime_layout.rs planned_state(non-default fresh install) + initialize logs=state/logs'},null,2));
}
async function legacyFixture(){
    if(!process.env.AIRK_EDITOR_LEGACY_FIXTURE)return undefined;
    const filename=path.resolve(process.env.AIRK_EDITOR_LEGACY_FIXTURE),bytes=await fs.readFile(filename);
    if(bytes.length>1048576)throw new Error('Legacy fixture receipt exceeds 1 MiB');
    const value=JSON.parse(bytes),artifacts=await fs.realpath(path.join(root,'artifacts')),fixture=await fs.realpath(value.fixture_root);
    if(value.schema!==1||value.fixture_kind!=='managed_native_catalog_upgrade'||!inside(artifacts,fixture))throw new Error('Legacy receipt must name a native-owned artifacts fixture');
    const normalize=p=>process.platform==='win32'?path.toNamespacedPath(p).toLowerCase():p;
    if(normalize(path.resolve(value.fixture_root))!==normalize(fixture))throw new Error('Legacy fixture root is not canonical');
    await canonicalOwned(fixture,filename);
    for(const key of ['install_dir','control_dir','source_root','data_root'])await canonicalOwned(fixture,value[key]);
    const config=value.legacy_config;if(config?.kind!=='vscode_servers'||typeof config.entry_key!=='string')throw new Error('Unsupported legacy configuration fixture');
    for(const key of ['path','command','cwd'])await canonicalOwned(fixture,config[key]);
    if(!['stable_launcher','pinned_predecessor'].includes(config.binding_kind))throw new Error('Legacy command binding kind is required');
    if(config.binding_kind==='pinned_predecessor'&&(!Array.isArray(value.expected_refusal_codes)||!value.expected_refusal_codes.length||value.expected_refusal_codes.some(code=>typeof code!=='string'||['registry_busy','status_timeout','native_read_failed'].includes(code))))throw new Error('Pinned predecessor needs actual native incompatible-format refusal codes');
    const original=await fs.readFile(config.path);if(sha(original)!==config.sha256)throw new Error('Original legacy configuration changed');
    const parseErrors=[],parsed=require('../packages/vscode/node_modules/jsonc-parser').parse(original.toString('utf8'),parseErrors,{allowTrailingComma:true}),entry=parsed?.servers?.[config.entry_key];
    const same=require('node:util').isDeepStrictEqual;
    if(parseErrors.length||!entry||entry.command!==config.command||!same(entry.args,config.args)||!same(entry.env??{},config.env??{}))throw new Error('Legacy receipt does not match the actual original configuration');
    if(!Array.isArray(config.args)||!config.args.includes('mcp')||Object.keys(config.env??{}).some(key=>!['AIRK_CLIENT','AIRK_MCP_RESPONSE_MODE'].includes(key)))throw new Error('Unsupported legacy native entry/environment');
    const candidate=value.candidate;if(candidate?.install_manifest_sha256!==bundle.digest)throw new Error('Legacy fixture candidate differs from the verified installed-editor payload');
    const runtime=await canonicalOwned(fixture,candidate.runtime_dir),installBytes=await fs.readFile(path.join(runtime,'airk-install.json'));
    if(sha(installBytes)!==candidate.install_manifest_sha256)throw new Error('Legacy candidate installation manifest changed');
    const inventory=JSON.parse(installBytes),names=Object.keys(inventory.files);if(names.length>4096)throw new Error('Legacy payload inventory exceeds fixture bound');
    for(const name of names){const file=await ownedPath(runtime,path.resolve(runtime,name));if(sha(await fs.readFile(file))!==inventory.files[name])throw new Error('Legacy candidate payload changed: '+name);}
    const pair=JSON.parse(await fs.readFile(path.join(runtime,'airk-runtime.json'),'utf8'));
    if(pair.schema!==1||pair.airk_sha256!==candidate.engine_sha256||pair.embed_sha256!==candidate.helper_sha256)throw new Error('Legacy candidate pair differs from actual native receipt');
    if(config.binding_kind==='pinned_predecessor'){
        const owner=value.predecessor,commandRuntime=await canonicalOwned(fixture,owner.runtime_dir),manifest=await fs.readFile(path.join(commandRuntime,'airk-install.json'));
        if(sha(manifest)!==owner.install_manifest_sha256)throw new Error('Actual predecessor payload manifest changed');
        const inventory=JSON.parse(manifest),payload=owner.command_payload;
        if(!payload||inventory.files[payload.relative_path]!==payload.sha256||sha(await fs.readFile(config.command))!==payload.sha256||await canonicalOwned(commandRuntime,path.resolve(commandRuntime,payload.relative_path))!==config.command)throw new Error('Pinned predecessor command is not its exact verified payload');
    }else{
        const binding=config.launcher_binding,installation=binding?.installation_launcher;
        if(!binding||!['installation','universal'].includes(binding.kind)||binding.command_path!==config.command||sha(await fs.readFile(config.command))!==binding.command_sha256||!installation)throw new Error('Actual stable launcher binding is missing or changed');
        await canonicalOwned(fixture,installation.path);await canonicalOwned(fixture,installation.receipt.path);
        const bytes=await fs.readFile(installation.receipt.path),receipt=JSON.parse(bytes);
        if(installation.receipt.path!==path.join(value.install_dir,'launcher.json')||sha(bytes)!==installation.receipt.sha256||receipt.schema!==1||Object.keys(receipt).sort().join(',')!=='schema,sha256'||receipt.sha256!==installation.sha256||sha(await fs.readFile(installation.path))!==installation.sha256)throw new Error('Actual installation launcher receipt changed');
        if(binding.kind==='installation'){if(config.command!==installation.path||binding.command_sha256!==installation.sha256)throw new Error('Configuration does not name the verified installation launcher');}
        else{
            const record=binding.universal_pointer;if(!record)throw new Error('Actual universal launcher pointer is missing');await canonicalOwned(fixture,record.path);
            const bytes=await fs.readFile(record.path),pointer=JSON.parse(bytes);
            if(path.basename(record.path)!=='airk-launcher.json'||sha(bytes)!==record.sha256||pointer.schema!==1||Object.keys(pointer).sort().join(',')!=='install,launcher_sha256,schema'||pointer.install!==value.install_dir||pointer.launcher_sha256!==binding.command_sha256||path.dirname(record.path)!==path.dirname(config.command))throw new Error('Actual universal launcher pointer does not bind this installation');
        }
    }
    const native=value.native;if(!native?.activation_receipt||!native.authority_v1_readiness||!native.post_health||!Array.isArray(native.evidence_files)||!native.evidence_files.length)throw new Error('Actual native activation/readiness/upgrade evidence is required');
    if(native.evidence_files.length>64)throw new Error('Legacy native evidence file bound exceeded');
    let nativeBytes=0;native.parsed_evidence=[];
    for(const record of native.evidence_files){await canonicalOwned(fixture,record.path);const bytes=await fs.readFile(record.path);nativeBytes+=bytes.length;if(bytes.length>1048576||nativeBytes>4194304||sha(bytes)!==record.sha256)throw new Error('Legacy native evidence changed or exceeds the disclosed 4 MiB bound');native.parsed_evidence.push({path:record.path,sha256:record.sha256,value:JSON.parse(bytes.toString('utf8'))});}
    const inspection=native.inspect;
    if(!inspection||!native.parsed_evidence.some(record=>same(record.value,inspection))||inspection.selection?.active!==candidate.install_manifest_sha256||inspection.approval?.manifest_sha256!==candidate.install_manifest_sha256||inspection.payload_verified!==true)throw new Error('Actual native candidate inspection evidence is missing or does not select the candidate');
    if(config.binding_kind==='stable_launcher'&&inspection.launcher!==config.launcher_binding.installation_launcher.path)throw new Error('Actual native inspection disagrees with the installation launcher binding');
    const executable=await canonicalOwned(runtime,inspection.executable),relative=path.relative(runtime,executable).split(path.sep).join('/');
    if(inventory.files[relative]!==candidate.engine_sha256||sha(await fs.readFile(executable))!==candidate.engine_sha256)throw new Error('Actual native inspection did not select the verified candidate engine');
    if(!Object.keys(value.source_hashes_before??{}).length)throw new Error('Original native fixture source inventory is missing');
    for(const [name,digest]of Object.entries(value.source_hashes_before??{})){const file=await ownedPath(value.source_root,path.resolve(value.source_root,name));if(value.source_hashes_after?.[name]!==digest||sha(await fs.readFile(file))!==digest)throw new Error('Legacy original source changed');}
    if(!value.probe?.symbol||!value.probe.path||!/^[a-f0-9]{64}$/.test(value.probe.content_sha256))throw new Error('Native legacy fixture probe is missing');
    const probe=await canonicalOwned(value.source_root,path.resolve(value.source_root,value.probe.path));
    if(sha(await fs.readFile(probe))!==value.probe.content_sha256)throw new Error('Actual original legacy probe bytes changed');
    const previous=value.predecessor;
    if(!previous)throw new Error('Actual predecessor inventory is required');
    const previousRuntime=await canonicalOwned(fixture,previous.runtime_dir),previousManifest=await fs.readFile(path.join(previousRuntime,'airk-install.json'));
    if(sha(previousManifest)!==previous.install_manifest_sha256)throw new Error('Predecessor manifest changed');
    const previousInventory=JSON.parse(previousManifest),previousNames=Object.keys(previousInventory.files);
    if(previousNames.length>4096)throw new Error('Predecessor payload inventory exceeds fixture bound');
    for(const name of previousNames){const file=await ownedPath(previousRuntime,path.resolve(previousRuntime,name));if(sha(await fs.readFile(file))!==previousInventory.files[name])throw new Error('Predecessor payload changed: '+name);}
    const previousPair=JSON.parse(await fs.readFile(path.join(previousRuntime,'airk-runtime.json'),'utf8'));
    const previousMain=process.platform==='darwin'?'AIRK.app/Contents/MacOS/airk':process.platform==='win32'?'airk.exe':'airk';
    const previousHelper=process.platform==='darwin'?'AIRK.app/Contents/MacOS/airk-embed':process.platform==='win32'?'airk-embed.exe':'airk-embed';
    if(previousInventory.files[previousMain]!==previousPair.airk_sha256||previousInventory.files[previousHelper]!==previousPair.embed_sha256)throw new Error('Predecessor native pair differs from its verified inventory');
    const {pairIdentity,qualifyLegacyNative}=require('./native-editor-legacy.cjs');
    return {...value,qualification:qualifyLegacyNative(value,{candidateIdentity:pairIdentity(pair),predecessorIdentity:pairIdentity(previousPair)})};
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
    await ui.click('Indexing');await ui.click('Projects');
    const native=await ui.indexing({action:'page',kind:'projects'}),rows=native.page.page.items;
    if(!rows.some(row=>row.kind==='project'&&String(row.value.name).includes('Fixture Project')))throw new Error('Indexing did not list the native metadata Project');
    return {hits,rows,help:['role search','purpose and format guide','offline native engine guide','live tool schema','About credits/history/community links'],about};
}
async function cleanupServices(){
    const results=[];
    for(const folder of services.splice(0)){
        const legacy=legacyServices.get(folder),install=legacy?.install_dir??path.join(folder,'native-service');if(!await exists(install))continue;
        let receipt;
        try{
            const stop=JSON.parse(await fs.readFile(path.join(folder,'native-stop.json'),'utf8'));
            const captured=stop.capture,record=captured?.response?.record,request=record?.request;
            if(captured?.qualification!=='historical_native_lifecycle_response'||captured.response.status!=='lifecycle'||record?.phase!=='complete'||request?.action!=='stop'||captured.force!==false||captured.before_health?.status!=='health'||request.expected_instance!==captured.before_health.observation?.instance)throw new Error('Complete native Stop/no-force/authenticated-health receipt unavailable');
            if(!Number.isSafeInteger(captured.capturedAt)||captured.capturedAt<stop.started||record.id===stop.previous_capture?.response?.record?.id||!record.id||stop.dispatch?.dispatch===stop.confirmation?.dispatch)throw new Error('Captured native receipt does not identify this one-dispatch Stop');
            if(captured.runtime.installDir!==install||captured.runtime.runtimeSha256!==bundle.digest||request.runtime!==bundle.digest||captured.namespace!==request.key?.namespace||captured.namespace!==stop.requested_binding?.namespace)throw new Error('Crossed native Stop runtime/namespace binding');
            const result=await execute(bundle.main,['runtime','inspect','--install-dir',install],{windowsHide:true,timeout:30000,maxBuffer:65536});
            const observation=JSON.parse(result.stdout),owned=await fs.realpath(legacy?.fixture_root??folder),control=await canonicalOwned(owned,observation.control_dir);
            if(legacy){
                if(legacy.qualification?.status!=='qualified_native_fixture'||install!==legacy.install_dir||control!==await canonicalOwned(owned,legacy.control_dir)||captured.namespace!==legacy.native.inspect.registry_namespace)throw new Error('Crossed qualified legacy Stop installation');
                if(sha(await fs.readFile(legacy.legacy_config.path))!==legacy.legacy_config.sha256)throw new Error('Legacy configuration changed during lifecycle');
                for(const [name,digest]of Object.entries(legacy.source_hashes_before)){const file=await ownedPath(legacy.source_root,path.resolve(legacy.source_root,name));if(sha(await fs.readFile(file))!==digest)throw new Error('Legacy source changed during lifecycle');}
            }
            if(await canonicalOwned(owned,captured.runtime.controlDir)!==control||observation.approval?.manifest_sha256!==bundle.digest||observation.registry_namespace!==captured.namespace||observation.payload_verified!==true)throw new Error('Native stopped inspection crossed the captured installation');
            if(observation.lifecycle?.explicitly_stopped!==true||observation.lifecycle?.pending!==null)throw new Error('Native lifecycle has not settled stopped');
            receipt={qualified:true,stop,observation};
        }catch(error){receipt={qualified:false,error:String(error),retained:true};}
        results.push(receipt);await fs.writeFile(path.join(folder,'native-cleanup.json'),JSON.stringify(receipt,null,2));
    }
    if(results.some(value=>!value.qualified))throw new Error('Owned native service cleanup remains unqualified: '+JSON.stringify(results));
    return results;
}
async function main(){
    const harnessPaths=['scripts/check-native-editor.cjs','scripts/editor-ui.cjs','scripts/native-editor-legacy.cjs','packages/vscode/out/test/installedEditor.js'];
    const harness=Object.fromEntries(await Promise.all(harnessPaths.map(async name=>[name,sha(await fs.readFile(path.join(root,name)))])));
    const stage=(await fs.lstat(source)).isDirectory();
    if(stage){const version=JSON.parse(await fs.readFile(path.join(source,'package.json'),'utf8')).version;
        bundle=await require(path.join(root,'packages/vscode/out/serviceBootstrap.js')).verifyBundledRuntime(source,version);}
    const base=path.join(root,'artifacts/editor-qualification');await fs.mkdir(base,{recursive:true});
    const run=await fs.mkdtemp(path.join(base,stage?'native-stage-':'native-installed-'));
    const extensions=path.join(run,'extensions');await fs.mkdir(extensions);
    if(!stage)await runProcess([cli,'--user-data-dir',path.join(run,'install-profile'),'--extensions-dir',extensions,
        '--shared-data-dir',path.join(run,'shared-editor-data'),'--install-extension',source,'--force'],path.join(run,'install.log'),{ELECTRON_RUN_AS_NODE:'1'});
    if(!stage){const installed=(await fs.readdir(extensions)).filter(name=>name.startsWith('iokaio.airk-'));
        if(installed.length!==1)throw new Error('Expected one private installed AIRK extension');
        const directory=path.join(extensions,installed[0]),version=JSON.parse(await fs.readFile(path.join(directory,'package.json'),'utf8')).version;
        bundle=await require(path.join(root,'packages/vscode/out/serviceBootstrap.js')).verifyBundledRuntime(directory,version);}
    const driver=path.join(extensions,'local-test.airk-driver-0.0.1');await fs.mkdir(driver);
    await fs.writeFile(path.join(driver,'package.json'),JSON.stringify({name:'airk-driver',publisher:'local-test',version:'0.0.1',engines:{vscode:'^1.140.0'},main:'driver.js',activationEvents:['onStartupFinished'],capabilities:{untrustedWorkspaces:{supported:true}}}));
    await fs.writeFile(path.join(driver,'driver.js'),`const vscode=require('vscode'),fs=require('node:fs/promises'),path=require('node:path');
exports.activate=()=>{if(!process.env.AIRK_EDITOR_RUN)return;void require(${JSON.stringify(path.join(root,'packages/vscode/out/test/installedEditor.js'))}).run()
.catch(error=>fs.writeFile(path.join(process.env.AIRK_EDITOR_RUN,process.env.AIRK_EDITOR_SCENARIO,process.env.AIRK_EDITOR_ROLE+'-error.json'),JSON.stringify({error:String(error),stack:error.stack})))
.finally(()=>vscode.commands.executeCommand('workbench.action.quit'));};`);
    const summary={schema:'airk-native-editor/2',source,staged:stage,nativeInstallDigest:bundle.digest,harness,selectedScenarios:scenarios,legacyUpgradeCoverage:false,scenarios:[]};
    console.log(`Native editor output: ${run}`);
    for(const scenario of scenarios){
        const folder=path.join(run,scenario);await fs.mkdir(folder);const scenarioDeadline=Date.now()+600000;
        const legacy=scenario==='legacy'?await legacyFixture():undefined;
        if(scenario==='legacy'&&!legacy){
            summary.scenarios.push({scenario,status:'unqualified',coverage:false,reason:legacy?.qualification.reason??'AIRK_EDITOR_LEGACY_FIXTURE from an actual native managed upgrade is unavailable'});
            if(legacy)await fs.writeFile(path.join(folder,'legacy-fixture-unqualified.json'),JSON.stringify(legacy,null,2));
            await fs.writeFile(path.join(run,'summary.json'),JSON.stringify(summary,null,2));continue;}
        services.push(folder);if(legacy)legacyServices.set(folder,legacy);const storage=path.join(folder,'storage');await fs.mkdir(storage);
        if(!legacy)await verifyFreshFixtureLayout(folder);
        if(legacy)await fs.writeFile(path.join(folder,'legacy-fixture.json'),JSON.stringify(legacy,null,2));
        for(const [name,symbol] of [['workspace-a','WindowAlphaEvidence'],['workspace-b','WindowBetaEvidence']]){const workspace=path.join(folder,name);await fs.mkdir(workspace);await fs.writeFile(path.join(workspace,'fixture.ts'),`export function ${symbol}() { return 42; }\n`);await fs.writeFile(path.join(workspace,'hidden.ts'),'export function HiddenPrivateEvidence() {}\n');}
        for(const name of ['shared-a','shared-b']){await fs.mkdir(path.join(folder,name));await fs.writeFile(path.join(folder,name,'shared.ts'),'export function SharedAllowedEvidence() { return 19; }\n');}
        const originalSources={};
        for(const name of ['workspace-a/fixture.ts','workspace-a/hidden.ts','workspace-b/fixture.ts','workspace-b/hidden.ts','shared-a/shared.ts','shared-b/shared.ts'])originalSources[name]=await fs.readFile(path.join(folder,name),'utf8');
        const roles=['same','different'].includes(scenario)?['a','b']:['a'],children=[],uis={},ports={};
        for(const role of roles){const profile=path.join(folder,`profile-${role}`);await fs.mkdir(path.join(profile,'User'),{recursive:true});
            // All new bindings come from the real reviewed Indexing forms. A
            // separately supplied legacy fixture keeps its original config.
            await fs.writeFile(path.join(profile,'User/settings.json'),JSON.stringify({'airk.service.channel':'stable','airk.service.installDir':legacy?.install_dir??path.join(folder,'native-service'),'airk.storage.path':legacy?.data_root??storage,'airk.cli.autoInstall':false,
                'workbench.startupEditor':'none','window.restoreWindows':'none','security.workspace.trust.startupPrompt':'never','extensions.autoUpdate':false,'extensions.autoCheckUpdates':false,
                'telemetry.telemetryLevel':'off','update.mode':'none','files.simpleDialog.enable':true,'window.dialogStyle':'custom',
}));
            let workspace=path.join(folder,scenario==='different'&&role==='b'?'workspace-b':'workspace-a');
            if(['multi','usage'].includes(scenario)){workspace=path.join(folder,'two-roots.code-workspace');await fs.writeFile(workspace,JSON.stringify({folders:[{path:'workspace-a'},{path:'workspace-b'}]}));}
            if(legacy)workspace=legacy.source_root;
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
                for(const role of roles){const error=await fs.readFile(path.join(folder,`${role}-error.json`),'utf8').catch(()=>undefined);if(error)throw new Error(error);
                    const request=JSON.parse(await fs.readFile(path.join(folder,`${role}-ui-request.json`),'utf8').catch(()=>'null'));
                    if(request&&handled.get(role)!==request.sequence){
                        if(!Number.isSafeInteger(request.sequence)||request.sequence<1||request.sequence>256)throw new Error('Private UI action budget/sequence invalid');
                        uis[role]??=await editorUi(ports[role],scenarioDeadline);
                        const actions=['serviceStop','connect','startService','page','inspect','setupDraft','setupApply','projectDraft','renameDraft','membershipDraft','apply','context','associationDraft','grantDraft','useGrant','revokeDraft','read','reconcile','staleApply','restricted','usageScoped','usageAdmin','usageJson','usageClear'];
                        if(!actions.includes(request.action))throw new Error('Unknown private Indexing action');
                        const fixture=await fs.realpath(folder);
                        if(request.action==='serviceStop'||(legacy&&request.action==='startService')){
                            if(legacy){
                                if(legacy.qualification?.status!=='qualified_native_fixture'||request.installDir!==legacy.install_dir||request.runtimeSha256!==legacy.candidate.install_manifest_sha256||request.runtimeSha256!==bundle.digest)throw new Error('Crossed qualified legacy lifecycle binding');
                                await canonicalOwned(await fs.realpath(legacy.fixture_root),request.installDir);
                            }else{
                                await canonicalOwned(fixture,request.installDir);
                                if(request.installDir!==path.join(folder,'native-service')||request.runtimeSha256!==bundle.digest)throw new Error('Crossed owned Stop fixture binding');
                            }
                            const observed=await execute(bundle.main,['runtime','inspect','--install-dir',request.installDir],{windowsHide:true,env:environment,timeout:30000,maxBuffer:65536});
                            const metadata=JSON.parse(observed.stdout);
                            if(metadata.payload_verified!==true||metadata.approval?.manifest_sha256!==bundle.digest||!/^[a-f0-9]{32}$/.test(metadata.registry_namespace))throw new Error('Owned service namespace unavailable before Stop');
                            if(legacy&&(metadata.registry_namespace!==legacy.native.inspect.registry_namespace||await canonicalOwned(await fs.realpath(legacy.fixture_root),metadata.control_dir)!==await canonicalOwned(await fs.realpath(legacy.fixture_root),legacy.control_dir)))throw new Error('Legacy lifecycle changed native namespace/control');
                            await fs.writeFile(path.join(folder,request.action==='serviceStop'?'native-pre-stop-inspect.json':'native-pre-start-inspect.json'),JSON.stringify(metadata,null,2));
                            request.namespace=metadata.registry_namespace;
                        }
                        if(request.action==='setupDraft'){await ownedPath(fixture,request.source);await ownedPath(fixture,request.storage,true);if(request.additional)await ownedPath(fixture,request.additional.path);}
                        if(request.action==='membershipDraft')for(const atom of request.roots??[])await ownedPath(fixture,atom.path);
                        for(const key of ['index','project','id'])if(request[key]!==undefined&&!/^(idx_|prj_|grt_)[a-f0-9]{32}$/.test(request[key]))throw new Error('Invalid exact native catalog ID');
                        // Mark dispatch before invoking the UI; unknown outcomes are never replayed.
                        handled.set(role,request.sequence);
                        const value=await uis[role].indexing(request);
                        if(Buffer.byteLength(JSON.stringify(value))>1048576)throw new Error('Complete private UI receipt exceeds 1 MiB');
                        await fs.writeFile(path.join(folder,`${role}-ui-done-${request.sequence}.json`),JSON.stringify(value));}
                }
                if(scenario==='multi'&&await exists(path.join(folder,'a-explorer-ready.json'))&&!explorerTask){uis.a??=await editorUi(ports.a,scenarioDeadline);
                    explorerTask=checkExplorer(uis.a).then(value=>fs.writeFile(path.join(folder,'explorer-checked.json'),JSON.stringify(value,null,2))).catch(error=>{failure=error;});}
                if((await Promise.all(roles.map(role=>exists(path.join(folder,`${role}-capture.json`))))).every(Boolean))break;
                if(allExited)throw new Error(`Private editor exited before acceptance completed: ${scenario}`);
                if(Date.now()>end)throw new Error(`Native editor scenario timed out: ${scenario}`);await delay(150);
            }
            if(explorerTask)await explorerTask;if(failure)throw failure;
            for(const role of roles){uis[role]??=await editorUi(ports[role],scenarioDeadline);const ui=uis[role];
                const body=await ui.until(async()=>(await ui.service('document.body.innerText'))?.value,'AIRK Explorer Management');
                await fs.writeFile(path.join(folder,`${role}-service.txt`),body);
                if(scenario==='untrusted'){if(!(await ui.service("![...document.querySelectorAll('button')].some(b=>!b.disabled&&['Prepare native setup review','Apply reviewed index setup','Use Grant for this window','Persist reviewed grant'].includes(b.textContent.trim()))"))?.value)throw new Error('Untrusted native controls were enabled');}
                const page=await ui.workbench('document.title');const image=await page.client.call('Page.captureScreenshot',{format:'png'});
                await fs.writeFile(path.join(folder,`${role}-workbench.png`),Buffer.from(image.data,'base64'));
            }
        }finally{for(const [role,ui]of Object.entries(uis)){await fs.writeFile(path.join(folder,role+'-ui-dispatch-receipts.json'),JSON.stringify(ui.dispatchReceipts(),null,2));try{await fs.writeFile(path.join(folder,role+'-ui-diagnostics.json'),JSON.stringify(ui.diagnostics(),null,2));}catch(error){console.error('Private UI diagnostic retention failed:',String(error));}ui.close();}await fs.writeFile(path.join(folder,'capture-complete.json'),'{}');}
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
        if(legacy){if(sha(await fs.readFile(legacy.legacy_config.path))!==legacy.legacy_config.sha256)throw new Error('Legacy config changed after installed-editor probe');
            for(const [name,digest]of Object.entries(legacy.source_hashes_before)){const file=await ownedPath(legacy.source_root,path.resolve(legacy.source_root,name));if(sha(await fs.readFile(file))!==digest)throw new Error('Legacy source changed during installed-editor probe');}
            summary.legacyUpgradeCoverage=true;}
        await cleanupServices();
        await fs.writeFile(path.join(run,'summary.json'),JSON.stringify(summary,null,2));console.log(`${scenario}: native editor passed`);
    }
    for(const [name,digest]of Object.entries(harness))if(sha(await fs.readFile(path.join(root,name)))!==digest)throw new Error('Installed-editor harness changed during qualification: '+name);
    summary.status=summary.scenarios.some(row=>row.status==='unqualified')?'partial':'passed';
    summary.qualifiesFullScenarioSelection=summary.status==='passed';
    await fs.writeFile(path.join(run,'summary.json'),JSON.stringify(summary,null,2));
    if(summary.status==='partial'){console.log(`Native editor coverage remains unqualified: ${run}`);process.exitCode=2;}
    else console.log(`Native editor selected scenarios passed: ${run}`);
}
main().catch(async error=>{const editorCleanup=await closeOwnedEditors();let serviceCleanup;try{serviceCleanup=await cleanupServices();}catch(cleanupError){serviceCleanup=String(cleanupError);}console.error(JSON.stringify({error:String(error),editorCleanup,serviceCleanup}));process.exitCode=1;});
