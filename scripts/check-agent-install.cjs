// SPDX-License-Identifier: Apache-2.0
// Install a VSIX into a private profile and exercise existing local agent migrations.
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const {spawn,execFile}=require('node:child_process');
const {promisify}=require('node:util');
const assert=require('node:assert/strict');
const net=require('node:net');
const {editorUi,until}=require('./editor-ui.cjs');
const execute=promisify(execFile);
const root=path.resolve(__dirname,'..');
const [editor,vsix,previousVsix]=process.argv.slice(2).map(value=>path.resolve(value));
if(!editor||!vsix)throw new Error('Expected Code.exe and the local VSIX path.');
const env={...process.env};delete env.ELECTRON_RUN_AS_NODE;delete env.VSCODE_IPC_HOOK_CLI;
async function removeFixtureTerminalPath(storage){
    if(process.platform!=='win32')return;
    // The real installer adds its command to the account PATH even in a private
    // editor profile. Remove only this run's exact entry, preserving other edits.
    const directory=path.join(storage,'service-cli','bin').replaceAll("'","''");
    const script=`$ErrorActionPreference='Stop'
$airkFixtureBin='${directory}'
$airkKey=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment',$true)
if($null -ne $airkKey){try{
 $airkValue=$airkKey.GetValue('Path',$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
 if($null -ne $airkValue){
  $airkParts=@(([string]$airkValue).Split(';') | Where-Object { -not [string]::Equals($_.Trim().Trim('"').TrimEnd('\\','/'),$airkFixtureBin,[StringComparison]::OrdinalIgnoreCase) })
  $airkNext=$airkParts -join ';'
  if($airkNext -cne [string]$airkValue){$airkKey.SetValue('Path',$airkNext,$airkKey.GetValueKind('Path'))}
 }
}finally{$airkKey.Dispose()}}
`;
    await execute(path.join(process.env.SystemRoot||'C:\\Windows','System32/WindowsPowerShell/v1.0/powershell.exe'),
        ['-NoLogo','-NoProfile','-NonInteractive','-EncodedCommand',Buffer.from(script,'utf16le').toString('base64')],
        {windowsHide:true,env,timeout:15000,maxBuffer:1024*1024});
}
async function main(){
    const candidate=JSON.parse(await fs.readFile(path.join(path.dirname(vsix),'manifest.json'),'utf8')),version=candidate.version;
    const previousVersion=previousVsix?JSON.parse(await fs.readFile(path.join(path.dirname(previousVsix),'manifest.json'),'utf8')).version:null;
    if(previousVersion&&previousVersion===version)throw new Error('Upgrade requires distinct installed package versions.');
    const listener=net.createServer();await new Promise(resolve=>listener.listen(0,'127.0.0.1',resolve));
    const port=listener.address().port;await new Promise(resolve=>listener.close(resolve));
    const base=path.join(root,'artifacts/editor-qualification');await fs.mkdir(base,{recursive:true});
    const run=await fs.mkdtemp(path.join(base,'agent-install-'));
    console.log(`Installer acceptance output: ${run}`);
    const profile=path.join(run,'profile'),extensions=path.join(run,'extensions'),workspace=path.join(run,'workspace');
    await fs.mkdir(path.join(profile,'User'),{recursive:true});await fs.mkdir(extensions);await fs.mkdir(workspace);
    const storage=path.join(profile,'User/globalStorage/iokaio.airk'),install=path.join(run,'native-service');
    await fs.writeFile(path.join(profile,'User/settings.json'),JSON.stringify({'airk.service.installDir':install,'airk.storage.path':path.join(run,'data'),
        'workbench.startupEditor':'none','extensions.autoUpdate':false,'extensions.autoCheckUpdates':false,'telemetry.telemetryLevel':'off','update.mode':'none'}));
    await fs.writeFile(path.join(workspace,'fixture.ts'),'export const InstallerEvidence = 42;\n');
    const canonical=await fs.realpath(workspace);
    const collection='project-'+crypto.createHash('sha256').update(process.platform==='win32'?canonical.toLowerCase():canonical).digest('hex').slice(0,24);
    const name='airk-'+collection,legacy={command:path.join(storage,'cli/bin',process.platform==='win32'?'airk.exe':'airk'),
        args:['--root',canonical,'--data-dir',path.join(run,'old-data'),'--collection',collection,'mcp'],env:{AIRK_CLIENT:'fixture'},disabled:true};
    const files=[path.join(workspace,'.codex/config.toml'),path.join(workspace,'.mcp.json'),path.join(workspace,'.vscode/mcp.json')];
    for(const file of files)await fs.mkdir(path.dirname(file),{recursive:true});
    await fs.writeFile(files[0],`[mcp_servers.${name}]\ncommand = ${JSON.stringify(legacy.command)}\nargs = ${JSON.stringify(legacy.args)}\nenabled = false\n[mcp_servers.${name}.env]\nAIRK_CLIENT = "fixture"\n`);
    await fs.writeFile(files[1],JSON.stringify({mcpServers:{[name]:legacy,other:{command:'preserve-other'}}}));
    await fs.writeFile(files[2],JSON.stringify({servers:{[name]:{...legacy,type:'stdio'},other:{command:'preserve-other'}},inputs:[]}));
    const editorRoot=path.dirname(editor),candidates=[editorRoot,...(await fs.readdir(editorRoot,{withFileTypes:true})).filter(value=>value.isDirectory()).map(value=>path.join(editorRoot,value.name))];
    const cliPaths=[];for(const candidate of candidates){const file=path.join(candidate,'resources/app/out/cli.js');if(await fs.access(file).then(()=>true,()=>false))cliPaths.push(file);}
    if(cliPaths.length!==1)throw new Error('Expected one installed VS Code CLI entry point.');
    const cli=cliPaths[0];
    const installed=await execute(editor,[cli,'--user-data-dir',profile,'--extensions-dir',extensions,'--install-extension',previousVsix||vsix,'--force'],
        {windowsHide:true,env:{...env,ELECTRON_RUN_AS_NODE:'1'},timeout:120000,maxBuffer:1024*1024});
    await fs.writeFile(path.join(run,'install.log'),installed.stdout+installed.stderr);
    const driver=path.join(run,'driver');await fs.mkdir(driver);
    await fs.writeFile(path.join(driver,'package.json'),JSON.stringify({name:'airk-installer-driver',publisher:'local-test',version:'0.0.1',engines:{vscode:'^1.140.0'},main:'driver.js',activationEvents:['onStartupFinished']}));
    await fs.writeFile(path.join(driver,'driver.js'),`const vscode=require('vscode'),fs=require('node:fs/promises'),path=require('node:path'),assert=require('node:assert/strict');
exports.activate=()=>{let reloading=false;void (async()=>{
 const extension=vscode.extensions.getExtension('iokaio.airk');assert.ok(extension);assert.ok(extension.extensionPath.startsWith(${JSON.stringify(extensions)}));
 const api=await extension.activate();let definitions=await api.getServerDefinitions();
 const files=${JSON.stringify(files)},name=${JSON.stringify(name)};
 const manual=JSON.parse(await fs.readFile(files[2],'utf8')).servers;
 const definition=manual.airk??definitions[0];assert.ok(definition);
 if(manual.airk)assert.equal(definitions.length,0,'manual configuration owns disabled state and suppresses a duplicate provider');
 const flag=(args,key)=>{const index=args.indexOf(key);assert.ok(index>=0,key);return args[index+1];};
 const same=(a,b)=>path.relative(process.platform==='win32'?path.toNamespacedPath(a):a,process.platform==='win32'?path.toNamespacedPath(b):b)==='';
 const {RuntimeHost}=require(path.join(extension.extensionPath,'out/serviceBootstrap.js'));
 const runtime=new RuntimeHost(extension.extensionPath,extension.packageJSON.version,{channel:'stable',installDir:${JSON.stringify(install)}});
 const installation=await runtime.discover(),binding=runtime.binding(installation.inspection,installation.installDir),client=runtime.client(binding);
 let registration;
 try{const page=await client.projects(null,null,32,false);assert.equal(page.after,null);const matches=page.projects.map(row=>row.project.registration).filter(value=>same(value.root,${JSON.stringify(canonical)})&&value.collection===${JSON.stringify(collection)});assert.equal(matches.length,1);registration=matches[0];}finally{client.dispose();}
 const spec={command:definition.command,root:registration.root,dataDir:registration.data_root,collection:registration.collection,name:'fixture',service:{installDir:binding.installDir,project:registration.binding,universal:!definition.args.includes('launch')}};
 async function diagnostics(){
  const {McpStdioSession}=require(path.join(extension.extensionPath,'out/mcpStdioSession.js'));
  const {session}=await McpStdioSession.open({command:definition.command,args:definition.args,cwd:path.dirname(definition.command),clientName:'installer-acceptance',clientVersion:extension.packageJSON.version},90000);
  try{const result=await session.request('tools/call',{name:'diagnostics',arguments:{response_mode:'full'}},90000);assert.notEqual(result.isError,true);return JSON.parse(result.content.find(part=>part.type==='text').text);}finally{session.close();}
 }
 if(${JSON.stringify(previousVersion)}&&!await fs.access(${JSON.stringify(path.join(run,'baseline.json'))}).then(()=>true,()=>false)){
  assert.equal(extension.packageJSON.version,${JSON.stringify(previousVersion)});
  await fs.writeFile(${JSON.stringify(path.join(run,'baseline.json'))},JSON.stringify({version:extension.packageJSON.version,command:definition.command,args:definition.args,project:spec.service.project,dataDir:spec.dataDir,root:spec.root,controlDir:binding.controlDir}));
  const deadline=Date.now()+180000;
  while(!await fs.access(${JSON.stringify(path.join(run,'upgrade-reload.json'))}).then(()=>true,()=>false)){if(Date.now()>deadline)throw new Error('Upgrade did not finish installing');await new Promise(resolve=>setTimeout(resolve,100));}
  reloading=true;await vscode.commands.executeCommand('workbench.action.reloadWindow');return;
 }
 assert.equal(extension.packageJSON.version,${JSON.stringify(version)});
 if(${!!previousVsix}){const old=JSON.parse(await fs.readFile(${JSON.stringify(path.join(run,'baseline.json'))},'utf8'));assert.equal(old.project,spec.service.project);assert.equal(old.dataDir,spec.dataDir);assert.equal(old.root,spec.root);}
 if(${!!previousVsix}){
  const live=await require(path.join(extension.extensionPath,'out/native.js')).probeMcp(spec,27,'upgrade-before-reconnect');
  assert.ok(live.tools.includes('diagnostics'));const status=await diagnostics();assert.equal(status.runtime.service.version,${JSON.stringify(version)});
  await fs.writeFile(${JSON.stringify(path.join(run,'new-runtime-active.json'))},JSON.stringify(status));
  const deadline=Date.now()+45000;while(!await fs.access(${JSON.stringify(path.join(run,'clients-disconnected.json'))}).then(()=>true,()=>false)){if(Date.now()>deadline)throw new Error('Installer did not terminate preceding clients');await new Promise(resolve=>setTimeout(resolve,100));}
 }
 const texts=await Promise.all(files.map(file=>fs.readFile(file,'utf8')));
 assert.ok(same(flag(definition.args,'--root'),spec.root));
 for(const text of texts){assert.ok(text.includes('airk-launcher'));assert.ok(text.includes('--root'));assert.ok(!text.includes('--service-project'));assert.ok(!text.includes('--data-dir'));}
 assert.ok(texts[0].includes('enabled = false'));
 for(const index of [1,2]){const value=JSON.parse(texts[index]),servers=value[index===1?'mcpServers':'servers'];assert.equal(servers[name],undefined);assert.equal(servers.airk.disabled,true);assert.equal(servers.airk.env.AIRK_CLIENT,'fixture');assert.equal(servers.other.command,'preserve-other');assert.equal(servers.airk.command,definition.command);assert.deepEqual(servers.airk.args,definition.args);}
 await vscode.commands.executeCommand('airk.installCli');assert.deepEqual(await Promise.all(files.map(file=>fs.readFile(file,'utf8'))),texts);
 await vscode.commands.executeCommand('airk.explorer.open','#manage/overview');
 await vscode.commands.executeCommand('notifications.showList');
 await fs.writeFile(${JSON.stringify(path.join(run,'notification-ready.json'))},JSON.stringify({project:spec.service.project}));
 const noticeDeadline=Date.now()+45000;
 while(!await fs.access(${JSON.stringify(path.join(run,'notification-verified.json'))}).then(()=>true,()=>false)){if(Date.now()>noticeDeadline)throw new Error('Installer notification was not verified');await new Promise(resolve=>setTimeout(resolve,100));}
 const probe=await require(path.join(extension.extensionPath,'out/native.js')).probeMcp(spec,27,'installer-acceptance');assert.ok(probe.tools.includes('status'));
 const quick=JSON.parse(probe.status.content[0].text);assert.equal(quick.runtime,undefined);assert.equal(quick.work,undefined);
 const observed=await diagnostics();
 for(const actual of [observed.runtime.mcp_frontend.process.version,observed.runtime.index_worker.process.version,observed.runtime.service.version])assert.equal(actual,${JSON.stringify(version)});
 assert.equal(observed.runtime.service.payload_sha256,${JSON.stringify(candidate.payload['native/runtime/airk-install.json'])});
 await fs.writeFile(${JSON.stringify(path.join(run,'result.json'))},JSON.stringify({passed:true,installed:true,version:extension.packageJSON.version,extensionPath:extension.extensionPath,files,command:definition.command,args:definition.args,controlDir:binding.controlDir,tools:probe.tools,status:probe.status,diagnostics:observed,policyPreserved:true,repairIdempotent:true,quietCompletionVerified:true,managementTabsVerified:true,installProgressVerified:await fs.access(${JSON.stringify(path.join(run,'progress-verified.json'))}).then(()=>true,()=>false)},null,2));
 })().catch(error=>{if(reloading&&String(error)==='Canceled: Canceled')return;return fs.writeFile(${JSON.stringify(path.join(run,'error.json'))},JSON.stringify({error:String(error),stack:error.stack}));}).finally(()=>{if(!reloading)return vscode.commands.executeCommand('workbench.action.quit');});};`);
    const log=await fs.open(path.join(run,'editor.log'),'w');
    const child=spawn(editor,[workspace,'--new-window','--user-data-dir',profile,'--extensions-dir',extensions,'--shared-data-dir',path.join(run,'shared'),
        `--extensionDevelopmentPath=${driver}`,`--remote-debugging-port=${port}`,'--remote-debugging-address=127.0.0.1','--disable-workspace-trust','--skip-welcome','--skip-release-notes','--disable-updates','--disable-telemetry','--disable-gpu'],
        {windowsHide:true,env,stdio:['ignore',log.fd,log.fd]});
    let failure;
    const retained=[];
    try{
        const exited=new Promise((resolve,reject)=>{
            const timer=setTimeout(()=>{child.kill();reject(new Error('Installer acceptance timed out.'));},240000);
            child.on('error',error=>{clearTimeout(timer);reject(error);});child.on('exit',code=>{clearTimeout(timer);code===0?resolve():reject(new Error('Editor exited '+code));});
        });
        const notification=(async()=>{
            if(previousVsix){
                await until(()=>fs.access(path.join(run,'baseline.json')).then(()=>true,()=>false),'running baseline installation',120000);
                const baseline=JSON.parse(await fs.readFile(path.join(run,'baseline.json'),'utf8'));
                const baselineStatuses=[];
                for(const label of ['codex','claude']){
                    const child=spawn(baseline.command,baseline.args,{windowsHide:true,env:{...env,AIRK_CLIENT:label},stdio:'pipe'});
                    retained.push(child);child.stderr.resume();
                    await new Promise((resolve,reject)=>{
                        const timer=setTimeout(()=>reject(new Error('Retained MCP initialize timed out')),60000);let buffer='';
                        child.on('error',reject);child.stdout.on('data',data=>{buffer+=data;let end;while((end=buffer.indexOf('\n'))>=0){const value=JSON.parse(buffer.slice(0,end));buffer=buffer.slice(end+1);if(value.id===1){clearTimeout(timer);value.error?reject(new Error(JSON.stringify(value.error))):resolve();}if(value.id===2)baselineStatuses.push({client:label,result:value});}});
                        child.stdin.write(JSON.stringify({jsonrpc:'2.0',id:1,method:'initialize',params:{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:label,version:'upgrade-fixture'}}})+'\n');
                    });
                    child.stdin.write(JSON.stringify({jsonrpc:'2.0',method:'notifications/initialized'})+'\n');
                    child.stdin.write(JSON.stringify({jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'airk_status',arguments:{}}})+'\n');
                }
                await until(()=>baselineStatuses.length===2,'baseline MCP status observations',90000);
                for(const value of baselineStatuses){if(value.result.error||value.result.result?.isError)throw new Error('Baseline MCP status failed');}
                await fs.writeFile(path.join(run,'baseline-status.json'),JSON.stringify(baselineStatuses,null,2));
                await fs.writeFile(path.join(run,'retained-clients.json'),JSON.stringify(retained.map(child=>({pid:child.pid})),null,2));
                const upgrade=await execute(editor,[cli,'--user-data-dir',profile,'--extensions-dir',extensions,'--install-extension',vsix,'--force'],
                    {windowsHide:true,env:{...env,ELECTRON_RUN_AS_NODE:'1'},timeout:150000,maxBuffer:1024*1024});
                await fs.writeFile(path.join(run,'upgrade-install.log'),upgrade.stdout+upgrade.stderr);
                await fs.writeFile(path.join(run,'upgrade-reload.json'),'{}');
                const progressUi=await editorUi(port);
                try{
                    const page=await until(async()=>{const value=await progressUi.workbench('document.body.innerText');return value&&value.value.includes(`Updating AIRK ${version} runtime`)?value:undefined;},'visible upgrade progress popup',30000);
                    await fs.writeFile(path.join(run,'install-progress.txt'),page.value);
                    const screenshot=await page.client.call('Page.captureScreenshot',{format:'png'});await fs.writeFile(path.join(run,'install-progress.png'),Buffer.from(screenshot.data,'base64'));
                    await fs.writeFile(path.join(run,'progress-verified.json'),'{}');
                }finally{progressUi.close();}
                await until(async()=>{
                    const error=await fs.readFile(path.join(run,'error.json'),'utf8').catch(()=>undefined);if(error)throw new Error(error);
                    return fs.access(path.join(run,'new-runtime-active.json')).then(()=>true,()=>false);
                },'new runtime activated while previous clients were connected',120000);
                await until(()=>retained.every(client=>client.exitCode!==null||client.signalCode!==null),'installer terminated preceding MCP clients',15000);
                await fs.writeFile(path.join(run,'terminated-clients.json'),JSON.stringify(retained.map(client=>({pid:client.pid,exitCode:client.exitCode,signal:client.signalCode})),null,2));
                await fs.writeFile(path.join(run,'clients-disconnected.json'),'{}');
            }
            await until(async()=>{
                const error=await fs.readFile(path.join(run,'error.json'),'utf8').catch(()=>undefined);if(error)throw new Error(error);
                return fs.access(path.join(run,'notification-ready.json')).then(()=>true,()=>false);
            },'installed extension notification ready',180000);
            const ui=await editorUi(port);
            try{
                const expected=JSON.parse(await fs.readFile(path.join(run,'notification-ready.json'),'utf8'));
                const notice=await until(async()=>{
                    const page=await ui.workbench('document.body.innerText');
                    if(page&&page.value.includes(`AIRK ${version} installed.`))throw new Error('Obsolete multi-choice completion notification is visible');
                    const view=await ui.service(`(()=>{const tabs=[...document.querySelectorAll('[role="tab"]')].map(n=>n.textContent.trim());return {version:document.querySelector('.brand')?.title,tabs,project:document.querySelector('.manage select[aria-label="Project"]')?.value,connection:document.querySelector('.manage > p')?.textContent};})()`);
                    return view&&view.value.version===`Running AIRK ${version}`&&view.value.project===expected.project&&view.value.connection.includes('· connected')&&['Indexing','Activity','Storage','Recovery','Service','Agents','Usage','Logs'].every(tab=>view.value.tabs.includes(tab))?page:undefined;
                },'quiet automatic connection, registered project and all Management tabs',90000);
                await fs.writeFile(path.join(run,'notification.txt'),notice.value);
                const screenshot=await notice.client.call('Page.captureScreenshot',{format:'png'});
                await fs.writeFile(path.join(run,'notification.png'),Buffer.from(screenshot.data,'base64'));
                await fs.writeFile(path.join(run,'notification-verified.json'),'{}');
            }finally{ui.close();}
        })();
        await Promise.all([exited,notification]);
        const error=await fs.readFile(path.join(run,'error.json'),'utf8').catch(()=>undefined);if(error)throw new Error(error);
        const result=JSON.parse(await fs.readFile(path.join(run,'result.json'),'utf8'));if(!result.passed)throw new Error('Missing success receipt.');
        const toml=await execute('python',['-c','import json,sys,tomllib; print(json.dumps(tomllib.load(open(sys.argv[1],"rb"))))',files[0]],{env,windowsHide:true,timeout:30000,maxBuffer:1024*1024});
        const codex=JSON.parse(toml.stdout).mcp_servers;
        assert.equal(codex[name],undefined);assert.equal(codex.airk.enabled,false);assert.equal(codex.airk.env.AIRK_CLIENT,'fixture');
        assert.equal(codex.airk.command,result.command);assert.deepEqual(codex.airk.args,result.args);
        console.log(`Installed VSIX migration passed for Codex, Claude Code and Copilot; ${result.tools.length} MCP tools verified.`);
    }catch(error){failure=error;}finally{
        if(failure)child.kill();
        for(const child of retained){child.stdin.end();child.kill();}
        await log.close();
        const launcher=path.join(install,process.platform==='win32'?'airk-launcher.exe':'airk-launcher');
        try{
            const saved=await fs.readFile(path.join(run,'result.json'),'utf8').catch(()=>fs.readFile(path.join(run,'baseline.json'),'utf8'));
            const control=JSON.parse(saved).controlDir;assert.ok(typeof control==='string'&&path.isAbsolute(control),'No observed fixture control directory');
            await execute(launcher,['service','shutdown','--runtime-dir',control],{windowsHide:true,env,timeout:30000});
        }
        catch(error){await fs.writeFile(path.join(run,'shutdown-error.json'),JSON.stringify({error:String(error)}));}
        try{await removeFixtureTerminalPath(storage);}
        catch(error){failure??=error;}
    }
    if(failure)throw failure;
}
main().catch(error=>{console.error(error);process.exitCode=1;});
