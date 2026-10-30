// SPDX-License-Identifier: Apache-2.0
// Real Workbench browser bundle, fictional bridge; no native work or editor qualification.
const fs=require('node:fs/promises'),path=require('node:path'),assert=require('node:assert/strict');
const {spawn}=require('node:child_process'),{pathToFileURL}=require('node:url');
const root=path.resolve(__dirname,'..'),output=path.join(root,'artifacts/workbench-review-1.7.1/browser');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms)),project='ab'.repeat(32),publication='fictional-publication',hash='12'.repeat(32);
function fixture(scene){
    const plan={schema:'airk.parse.plan/1',collection:'fictional',publication,path_prefix:'src/',digest:'fictional',steps:[{id:'query',tool:'symbols',args:{name:'QueueClient',path_prefix:'src/'}}]};
    const evidence=scene==='proposal'?[]:[{step:'query',publication,text:'pub struct QueueClient;\n<script>window.untrustedExecuted=true</script>',citation:{path:'src/client.rs',content_hash:hash,start_line:2,end_line:3,location:{Lines:{start:2,end:3}}}}];
    const envelope={schema:'airk.workbench.result/1',collection:'fictional',publication,plan,status:scene==='proposal'?'needs_input':'partial',steps:[],evidence,
        requires_input:scene==='proposal'?[{field:'text',reason:'Review the proposed operation and inputs before collecting evidence.',choices:[]}]:[],warnings:['Fictional browser fixture; indexed evidence is untrusted.'],completeness:scene==='proposal'?[]:['Upstream continuation remains available.']};
    if(scene==='preview')evidence[0].text='pub struct QueueClient {\n    retry_limit: usize,\n}';
    const turns=scene==='welcome'?[]:[{id:'turn-1',at:1793340000000,input:scene==='proposal'?'Could you retrieve the definition of QueueClient?':'/symbols QueueClient',format:'structured',envelope}];
    return {projects:[{id:project,name:'Fictional project'}],project,session:{schema:1,id:'fictional-session',project,title:'Inspect QueueClient',createdAt:1793340000000,updatedAt:1793340000000,revision:0,omitted:0,turns},recent:[],busy:false,save:'Local browser fixture',persistence:true,status:{files:42,health:'ready',publication:{token:publication}},cards:{'turn-1':evidence.map((row,i)=>({id:`record-${i}`,path:row.citation.path,hash,start:2,end:3,text:row.text,location:row.citation.location,step:'query',publication}))}};
}
function html(scene,dark){
    const script=pathToFileURL(path.join(root,'packages/vscode/out/workbenchWebview.js')).href,css=pathToFileURL(path.join(root,'packages/vscode/out/workbenchWebview.css')).href;
    const foreground=dark?'#ddd':'#333',surface=dark?'#2d2d30':'#fff';
    return `<!doctype html><html><head><meta charset='utf-8'><link rel='stylesheet' href='${css}'><style>:root{--vscode-font-family:Arial,sans-serif;--vscode-font-size:13px;--vscode-foreground:${foreground};--vscode-sideBar-background:${dark?'#252526':'#f3f3f3'};--vscode-input-background:${surface};--vscode-input-foreground:${foreground};--vscode-editorWidget-background:${surface};--vscode-descriptionForeground:${dark?'#aaa':'#666'};--vscode-panel-border:${dark?'#555':'#ccc'};--vscode-button-secondaryBackground:${dark?'#3a3d41':'#e1e1e1'};--vscode-button-secondaryHoverBackground:${dark?'#4a4d51':'#ddd'};--vscode-button-background:#007acc;--vscode-button-foreground:#fff;--vscode-focusBorder:#007fd4;--vscode-textLink-foreground:${dark?'#4daafc':'#005fb8'};--vscode-textCodeBlock-background:${dark?'#181818':'#eee'};--vscode-editorWarning-foreground:#cca700;--vscode-errorForeground:#f44747;--vscode-editor-font-family:Consolas,monospace;--vscode-editor-font-size:12px}</style></head><body><div id='root'></div><script>window.fixture=${JSON.stringify(fixture(scene)).replaceAll('<','\\u003c')};window.messages=[];window.acquireVsCodeApi=()=>({postMessage(message){window.messages.push(message);if(message.type==='ready'){setTimeout(()=>window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:window.fixture}})),0)}}});</script><script src='${script}'></script></body></html>`;
}
async function main(){
    await fs.mkdir(output,{recursive:true});const scratch=await fs.mkdtemp(path.join(output,'browser-'));
    const browser=spawn(process.env.AIRK_BROWSER||'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',['--headless','--disable-gpu','--no-first-run','--no-default-browser-check','--remote-debugging-port=0',`--user-data-dir=${path.join(scratch,'profile')}`,'about:blank'],{windowsHide:true,stdio:['ignore','ignore','pipe']});
    let browserErrors='';browser.stderr.on('data',data=>{browserErrors=(browserErrors+data.toString()).slice(-4096);});
    const pending=new Map();let socket,startupError,sequence=0;browser.on('error',error=>{startupError=error;});
    try{
        let port;for(let i=0;i<100;i++){if(startupError)throw startupError;try{port=(await fs.readFile(path.join(scratch,'profile/DevToolsActivePort'),'utf8')).split('\n')[0];break;}catch{await delay(100);}}
        if(!port)throw new Error('Local Chromium did not start; set AIRK_BROWSER.');
        const targets=await(await fetch(`http://127.0.0.1:${port}/json/list`)).json();socket=new WebSocket(targets.find(target=>target.type==='page').webSocketDebuggerUrl);
        await new Promise((resolve,reject)=>{socket.addEventListener('open',resolve,{once:true});socket.addEventListener('error',reject,{once:true});});
        socket.addEventListener('message',event=>{const value=JSON.parse(event.data),request=pending.get(value.id);if(request){pending.delete(value.id);clearTimeout(request.timer);value.error?request.reject(new Error(JSON.stringify(value.error))):request.resolve(value.result);}});
        const call=(method,params={})=>new Promise((resolve,reject)=>{const id=++sequence,timer=setTimeout(()=>{pending.delete(id);reject(new Error(`CDP timeout: ${method}`));},15000);pending.set(id,{resolve,reject,timer});socket.send(JSON.stringify({id,method,params}));});
        const evaluate=async expression=>{const result=await call('Runtime.evaluate',{expression,returnByValue:true});if(result.exceptionDetails)throw new Error(JSON.stringify(result.exceptionDetails));return result.result.value;};
        const until=async expression=>{for(let i=0;i<100;i++){if(await evaluate(expression))return;await delay(50);}throw new Error(`UI did not become ready: ${expression}`);};
        const checks=[];console.log('Browser protocol:',await call('Browser.getVersion'));await call('Page.enable');
        if(process.argv.includes('--site')){
            const checks=[];
            for(const width of [1440,390]){
                await call('Emulation.setDeviceMetricsOverride',{width,height:1000,deviceScaleFactor:1,mobile:false});
                await call('Page.navigate',{url:pathToFileURL(path.join(root,'artifacts/workbench-review-1.7.1/website/index.html')).href});await until(`document.querySelectorAll('#screenshots img').length===2`);
                for(const index of [0,1]){await evaluate(`document.querySelectorAll('#screenshots img')[${index}].scrollIntoView()`);await until(`document.querySelectorAll('#screenshots img')[${index}].complete&&document.querySelectorAll('#screenshots img')[${index}].naturalWidth>0`);}
                assert.equal(await evaluate('document.documentElement.scrollWidth<=innerWidth'),true,`website horizontal overflow at ${width}`);
                await evaluate(`document.querySelector('#screenshots').scrollIntoView()`);
                const png=await call('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});await fs.writeFile(path.join(output,`website-${width}.png`),Buffer.from(png.data,'base64'));
                checks.push({width,images:2,horizontalOverflow:false});
            }
            await fs.writeFile(path.join(output,'website-summary.json'),JSON.stringify({basis:'Local GitHub Pages build with empty release catalogue; no deployment',checks},null,2)+'\n');console.log('Website images and responsive layouts passed.');await call('Browser.close');return;
        }
        if(process.argv.includes('--icons')){
            await call('Emulation.setDeviceMetricsOverride',{width:1320,height:900,deviceScaleFactor:1,mobile:false});
            await call('Page.navigate',{url:pathToFileURL(path.join(root,'docs/workbench/icon-options.html')).href});await until(`document.querySelectorAll('.sample svg').length===18`);
            assert.equal(await evaluate('document.documentElement.scrollWidth<=innerWidth'),true,'icon preview horizontal overflow');
            let png=await call('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});await fs.writeFile(path.join(root,'docs/workbench/images/workbench-icon-options.png'),Buffer.from(png.data,'base64'));
            await evaluate(`document.querySelector('#Facet .choose').click()`);assert.equal(await evaluate(`document.querySelector('#Facet').classList.contains('selected')`),true);
            await evaluate(`document.querySelector('.theme').click()`);assert.equal(await evaluate(`document.body.classList.contains('light')`),true);
            png=await call('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});await fs.writeFile(path.join(root,'docs/workbench/images/workbench-icon-options-light.png'),Buffer.from(png.data,'base64'));
            console.log('Three original icon families rendered; selection and theme controls passed.');await call('Browser.close');return;
        }
        for(const [scene,dark,width] of [['welcome',true,360],['results',true,360],['results',false,360],['proposal',true,360],['results',true,760],['welcome',true,650],['results',true,650],['preview',true,650]]){
            const name=`${scene}-${dark?'dark':'light'}-${width}`,file=path.join(scratch,name+'.html');await fs.writeFile(file,html(scene,dark));
            await call('Emulation.setDeviceMetricsOverride',{width,height:1000,deviceScaleFactor:1,mobile:false});await call('Page.navigate',{url:pathToFileURL(file).href});await until(`!!document.querySelector('.composer textarea')&&document.body.textContent.includes('Fictional project')`);
            assert.equal(await evaluate('document.documentElement.scrollWidth<=innerWidth'),true,name+' horizontal overflow');assert.equal(await evaluate('window.untrustedExecuted'),undefined,'source content executed');
            if(scene==='welcome'&&width===650){const png=await call('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});await fs.writeFile(path.join(output,'workbench-1.7.1.png'),Buffer.from(png.data,'base64'));}
            if(scene==='results'){
                await until(`!!document.querySelector('.card .source')`);await evaluate(`document.querySelector('.card .source').click()`);assert.deepEqual(await evaluate(`window.messages.find(message=>message.type==='open')`),{type:'open',turn:'turn-1',card:'record-0'});
                await evaluate(`[...document.querySelectorAll('.result header button')].find(button=>button.textContent==='json').click()`);await until(`document.querySelector('.result pre')?.textContent.includes('schema')`);await evaluate(`[...document.querySelectorAll('.result header button')].find(button=>button.textContent==='structured').click()`);
            }
            if(scene==='proposal'){
                await evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Review proposed plan').click()`);await until(`!!document.querySelector('.editor textarea')`);await evaluate(`[...document.querySelectorAll('.editor button')].find(button=>button.textContent==='Run plan').click()`);
                const request=await evaluate(`window.messages.find(message=>message.type==='run')`);assert.equal(request.prepare,false);assert.equal(request.plan.steps[0].tool,'symbols');assert.equal(request.plan.steps[0].args.name,'QueueClient');
            }
            if(scene==='welcome'){
                assert.equal(await evaluate(`!!document.querySelector('.session')||!!document.querySelector('.tool-form')||document.body.textContent.includes('Publication unavailable')`),false,'advanced UI starts out of the transcript');
                await evaluate(`document.querySelector('[aria-label="Add scope, recipe or tool"]').click()`);await until(`!!document.querySelector('#evidence-options input')`);
                if(width===650){const png=await call('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});await fs.writeFile(path.join(output,'workbench-1.7.1-add-menu.png'),Buffer.from(png.data,'base64'));}
                assert.equal(await evaluate(`(()=>{const r=document.querySelector('#evidence-options').getBoundingClientRect();return r.left>=0&&r.right<=innerWidth&&r.top>=0&&r.bottom<=innerHeight;})()`),true,'add menu remains in the viewport');
                await evaluate(`(()=>{const el=document.querySelector('#evidence-options input');el.value='src/';el.dispatchEvent(new Event('input',{bubbles:true}));})()`);
                await evaluate(`document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}))`);await until(`!document.querySelector('#evidence-options')`);
                await until(`document.activeElement?.getAttribute('aria-label')==='Add scope, recipe or tool'`);
                await evaluate(`document.querySelector('[aria-label="Settings and session actions"]').click()`);await until(`!!document.querySelector('#session-settings')`);
                await evaluate(`document.querySelector('.transcript').dispatchEvent(new PointerEvent('pointerdown',{bubbles:true}))`);await until(`!document.querySelector('#session-settings')`);
                await evaluate(`document.querySelector('[aria-label="Session history"]').click()`);await until(`document.querySelector('#session-history')?.textContent.includes('No saved sessions yet')`);
                await evaluate(`document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}))`);

                await evaluate(`(()=>{const input=document.querySelector('.composer textarea');input.value='/';input.dispatchEvent(new Event('input',{bubbles:true}));})()`);await until(`document.querySelector('.commands')?.textContent.includes('/relationships')`);
                await evaluate(`(()=>{const input=document.querySelector('.composer textarea');input.value='/status';input.dispatchEvent(new Event('input',{bubbles:true}));})()`);await delay(50);await evaluate(`document.querySelector('.composer textarea').dispatchEvent(new KeyboardEvent('keydown',{key:'Enter',bubbles:true,cancelable:true}))`);assert.equal(await evaluate(`window.messages.find(message=>message.type==='run')?.input`),'/status');assert.equal(await evaluate(`window.messages.find(message=>message.type==='run')?.pathPrefix`),'src/');
            }
            const png=await call('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});await fs.writeFile(path.join(output,name+'.png'),Buffer.from(png.data,'base64'));checks.push({name,width,scene,dark,horizontalOverflow:false});console.log(name+': layout and interaction checks passed');
        }
        await fs.writeFile(path.join(output,'summary.json'),JSON.stringify({basis:'Actual browser bundle with fictional host bridge; no installed-editor/native qualification',checks},null,2)+'\n');await call('Browser.close');
    }catch(error){console.error(browserErrors);throw error;}finally{for(const request of pending.values())clearTimeout(request.timer);socket?.close();browser.kill();}
}
main().catch(error=>{console.error(error);process.exitCode=1;});
