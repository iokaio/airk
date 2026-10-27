// SPDX-License-Identifier: Apache-2.0
// Native extension acceptance with private profiles; accepts a reviewed stage or VSIX.
const fs=require('node:fs/promises'),path=require('node:path'),net=require('node:net');
const {spawn,execFile}=require('node:child_process');
const {promisify}=require('node:util');
const execute=promisify(execFile),services=[];
let bundle;
const {editorUi}=require('./editor-ui.cjs');
const root=path.resolve(__dirname,'..');
const [editor,cli,source]=process.argv.slice(2).map(value=>path.resolve(value));
if(!source)throw new Error('Expected editor, its cli.js, and an approved extension stage or VSIX.');
const scenarios=(process.env.AIRK_EDITOR_SCENARIOS||'same,different,multi,usage,untrusted').split(',');
if(scenarios.some(value=>!['same','different','multi','usage','untrusted'].includes(value)))throw new Error('Unknown native editor scenario');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms)),active=new Set();
const environment={...process.env,RUST_BACKTRACE:'1'};delete environment.ELECTRON_RUN_AS_NODE;delete environment.VSCODE_IPC_HOOK_CLI;
async function runProcess(args,logfile,extra={}){
    const log=await fs.open(logfile,'w');
    return new Promise((resolve,reject)=>{const child=spawn(editor,args,{windowsHide:true,env:{...environment,...extra},stdio:['ignore',log.fd,log.fd]});active.add(child);
        const timer=setTimeout(()=>{child.kill();reject(new Error(`Private editor timed out: ${logfile}`));},600000);
        child.on('error',error=>{active.delete(child);clearTimeout(timer);void log.close();reject(error);});
        child.on('exit',code=>{active.delete(child);clearTimeout(timer);void log.close();code===0?resolve():reject(new Error(`Private editor exited ${code}: ${logfile}`));});});
}
async function unusedPort(){const server=net.createServer();await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const value=server.address().port;await new Promise(resolve=>server.close(resolve));return value;}
async function exists(file){return fs.access(file).then(()=>true,()=>false);}
/** Installed AIRK Explorer: Search shows a cited fixture hit and Management lists the registered project. */
async function checkExplorer(ui){
    const inExplorer=expression=>ui.document("!!document.querySelector('.airk-explorer')",expression);
    const hits=await ui.until(async()=>(await inExplorer("(()=>{const paths=[...document.querySelectorAll('.hit code.path')].map(n=>n.textContent);return paths.length?{paths,meta:document.querySelector('.result-meta')?.innerText}:false;})()"))?.value,'Explorer search hits');
    if(!hits.paths.some(value=>value.startsWith('fixture.ts')))throw new Error('Explorer search did not cite the fixture: '+JSON.stringify(hits));
    const click=async(label,selector='button')=>ui.until(async()=>(await inExplorer(`(()=>{const b=[...document.querySelectorAll(${JSON.stringify(selector)})].find(n=>n.textContent.trim()===${JSON.stringify(label)});if(!b||b.disabled)return false;b.click();return true;})()`))?.value,`Explorer ${label}`);
    const contains=async(text)=>ui.until(async()=>(await inExplorer(`document.querySelector('.help-topic')?.innerText.includes(${JSON.stringify(text)})`))?.value,`Explorer help ${text}`);
    await click('Help','.topbar button');
    await ui.until(async()=>(await inExplorer("(()=>{const i=document.querySelector('input[aria-label=\"Search help\"]');if(!i)return false;i.value='tester PDF';i.dispatchEvent(new Event('input',{bubbles:true}));return true;})()"))?.value,'Explorer help search');
    await click('Tester workflow','.help-index ul button');await contains('scanned requirements');
    await click('Choose indexing by purpose','.help-index .chips button');await contains('Search Only (essential)');
    await click('Engine guide: indexing-choices');await contains('Choose evidence detail, vector work and extraction separately.');
    // Live help remains read-only, including the exact tool schema selector.
    await ui.until(async()=>(await inExplorer("(()=>{const i=[...document.querySelectorAll('.help-index input')].find(i=>i.value==='airk_search');const form=i?.closest('form');if(!form)return false;form.dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));return true;})()"))?.value,'Explorer live tool reference');
    await contains('inputSchema');
    await click('About','.topbar button');await contains('Ioka LLC');await contains('Munarium');await contains('https://discord.gg/sJase5FbE');
    const about=await ui.until(async()=>(await inExplorer("(()=>{const labels=[...document.querySelectorAll('[aria-label=\"AIRK public links\"] button')].map(b=>b.textContent.trim());return labels.some(x=>x.includes('Report an issue'))&&labels.some(x=>x.includes('Contribute to AIRK'))?labels:false;})()"))?.value,'Explorer About community links');
    await ui.until(async()=>(await inExplorer("(()=>{const b=[...document.querySelectorAll('.rail button')].find(n=>n.textContent.startsWith('Management'));if(!b)return false;b.click();return true;})()"))?.value,'Explorer Management rail');
    const rows=await ui.until(async()=>(await inExplorer("(()=>{const r=[...document.querySelectorAll('table.grid tbody tr')].map(x=>x.innerText);return r.length?r:false;})()"))?.value,'Explorer Management projects');
    if(!rows.some(row=>row.includes('workspace-a')||row.includes('workspace-b')))throw new Error('Explorer Management did not list the project: '+JSON.stringify(rows));
    return {hits,rows,help:['role search','purpose and format guide','offline native engine guide','live tool schema','About credits/history/community links'],about};
}
async function cleanupServices(){
    for(const folder of services.splice(0)){
        const control=path.join(folder,'native-service','control');
        if(!await exists(control))continue;
        try{const result=await execute(bundle.main,['service','shutdown','--runtime-dir',control],{windowsHide:true,timeout:30000,maxBuffer:65536});
            await fs.writeFile(path.join(folder,'native-shutdown.json'),JSON.stringify({requested:true,stdout:result.stdout,stderr:result.stderr}));}
        catch(error){await fs.writeFile(path.join(folder,'native-shutdown.json'),JSON.stringify({requested:false,error:String(error),retained:true}));}
    }
}
async function main(){
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
    const summary={schema:'airk-native-editor/1',source,staged:stage,nativeInstallDigest:bundle.digest,scenarios:[]};
    console.log(`Native editor output: ${run}`);
    for(const scenario of scenarios){
        const folder=path.join(run,scenario);await fs.mkdir(folder);services.push(folder);const storage=path.join(folder,'storage');await fs.mkdir(storage);
        for(const [name,symbol] of [['workspace-a','WindowAlphaEvidence'],['workspace-b','WindowBetaEvidence']]){const workspace=path.join(folder,name);await fs.mkdir(workspace);await fs.writeFile(path.join(workspace,'fixture.ts'),`export function ${symbol}() { return 42; }\n`);}
        const roles=['same','different'].includes(scenario)?['a','b']:['a'],children=[],uis={},ports={};
        for(const role of roles){const profile=path.join(folder,`profile-${role}`);await fs.mkdir(path.join(profile,'User'),{recursive:true});
            // These scenarios exercise explicit root-review dialogs. Automatic
            // onboarding has its own installed-artifact installer acceptance.
            await fs.writeFile(path.join(profile,'User/settings.json'),JSON.stringify({'airk.service.installDir':path.join(folder,'native-service'),'airk.storage.path':storage,'airk.cli.autoInstall':false,
                'workbench.startupEditor':'none','window.restoreWindows':'none','security.workspace.trust.startupPrompt':'never','extensions.autoUpdate':false,'extensions.autoCheckUpdates':false,
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
            children.push(runProcess(args,path.join(folder,`${role}-editor.log`),{AIRK_EDITOR_RUN:run,AIRK_EDITOR_ROLE:role,AIRK_EDITOR_SCENARIO:scenario,
                AIRK_EDITOR_DEVELOPMENT_STAGE:stage?source:'',AIRK_USAGE_MODE:scenario==='usage'?'aggregates-only':'disabled'}));
        }
        let failure,usageTask,explorerTask,allExited=false;const exited=Promise.all(children).then(()=>{allExited=true;}).catch(error=>{failure=error;});const handled=new Map(),uiChecks={};
        try{
            const end=Date.now()+600000;
            while(true){if(failure)throw failure;
                for(const role of roles){const error=await fs.readFile(path.join(folder,`${role}-error.json`),'utf8').catch(()=>undefined);if(error)throw new Error(error);
                    const request=JSON.parse(await fs.readFile(path.join(folder,`${role}-ui-request.json`),'utf8').catch(()=>'null'));
                    if(request&&handled.get(role)!==request.sequence){uis[role]??=await editorUi(ports[role]);if(!['register','approveRepair'].includes(request.action))throw new Error('Unknown private UI request');
                        const [fixture,sourceRoot,storageRoot]=await Promise.all([folder,request.source,request.storage].map(value=>fs.realpath(value)));
                        if(!['workspace-a','workspace-b'].includes(request.root)||path.dirname(sourceRoot)!==fixture||path.basename(sourceRoot)!==request.root||path.dirname(storageRoot)!==fixture||!['storage','selected-storage'].includes(path.basename(storageRoot)))throw new Error('UI request escaped fixture scope');
                        if(request.action==='register')await uis[role].register(request);
                        else{if(scenario!=='multi'||!/^[a-f0-9]{64}$/.test(request.project))throw new Error('Invalid private repair selection');await uis[role].approveRepair(request.project);}
                        handled.set(role,request.sequence);await fs.writeFile(path.join(folder,`${role}-ui-done-${request.sequence}.json`),'{}');}
                }
                if(scenario==='multi'&&await exists(path.join(folder,'a-explorer-ready.json'))&&!explorerTask){uis.a??=await editorUi(ports.a);
                    explorerTask=checkExplorer(uis.a).then(value=>fs.writeFile(path.join(folder,'explorer-checked.json'),JSON.stringify(value,null,2))).catch(error=>{failure=error;});}
                if(scenario==='usage'&&await exists(path.join(folder,'a-usage-ready.json'))&&!usageTask){usageTask=require('./check-usage-editor.cjs').checkUsage(ports.a,folder).then(value=>{uiChecks.a=value;}).catch(error=>{failure=error;});}
                if((await Promise.all(roles.map(role=>exists(path.join(folder,`${role}-capture.json`))))).every(Boolean))break;
                if(allExited)throw new Error(`Private editor exited before acceptance completed: ${scenario}`);
                if(Date.now()>end)throw new Error(`Native editor scenario timed out: ${scenario}`);await delay(150);
            }
            if(usageTask)await usageTask;if(explorerTask)await explorerTask;if(failure)throw failure;
            for(const role of roles){uis[role]??=await editorUi(ports[role]);const ui=uis[role];
                const body=await ui.until(async()=>(await ui.service('document.body.innerText'))?.value,'AIRK Explorer Management');
                await fs.writeFile(path.join(folder,`${role}-service.txt`),body);
                if(scenario==='untrusted'){if(!(await ui.service("document.body.innerText.includes('Trust this workspace')&&![...document.querySelectorAll('button')].some(b=>!b.disabled&&['Review workspace registration','Connect / reconnect','Start service','Index now'].includes(b.textContent.trim()))"))?.value)throw new Error('Untrusted native controls were enabled');}
                else {await ui.responseMode('full');await ui.responseMode('terse');}
                const page=await ui.workbench('document.title');const image=await page.client.call('Page.captureScreenshot',{format:'png'});
                await fs.writeFile(path.join(folder,`${role}-workbench.png`),Buffer.from(image.data,'base64'));
            }
        }finally{for(const ui of Object.values(uis))ui.close();await fs.writeFile(path.join(folder,'capture-complete.json'),'{}');}
        await exited;if(failure)throw failure;
        const results=await Promise.all(roles.map(async role=>({...JSON.parse(await fs.readFile(path.join(folder,`${role}-result.json`),'utf8')),uiChecks:uiChecks[role]||['native console','terse/full selector']})));
        if(new Set(results.map(value=>value.extensionHostPid)).size!==roles.length)throw new Error('Each test window must have its own extension host');
        summary.scenarios.push(...results);
        await cleanupServices();
        await fs.writeFile(path.join(run,'summary.json'),JSON.stringify(summary,null,2));console.log(`${scenario}: native editor passed`);
    }
    console.log(`Native editor acceptance passed: ${run}`);
}
main().catch(async error=>{for(const child of active)child.kill();await cleanupServices();console.error(error);process.exitCode=1;});
