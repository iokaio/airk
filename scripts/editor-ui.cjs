// SPDX-License-Identifier: Apache-2.0
// Real UI automation for the explicitly launched private editor profiles only.
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function until(action,label,timeout=180000){const end=Date.now()+timeout;while(Date.now()<end){const value=await action();if(value)return value;await delay(120);}throw new Error(`Editor UI timeout: ${label}`);}
async function connect(target){
    const socket=new WebSocket(target.webSocketDebuggerUrl),contexts=new Set(),pending=new Map();let sequence=0;
    await new Promise((resolve,reject)=>{socket.addEventListener('open',resolve,{once:true});socket.addEventListener('error',reject,{once:true});});
    socket.addEventListener('message',event=>{const value=JSON.parse(event.data);
        if(value.method==='Runtime.executionContextCreated')contexts.add(value.params.context.id);
        if(value.method==='Runtime.executionContextDestroyed')contexts.delete(value.params.executionContextId);
        const request=pending.get(value.id);if(request){pending.delete(value.id);clearTimeout(request.timer);value.error?request.reject(new Error(JSON.stringify(value.error))):request.resolve(value.result);}});
    const call=(method,params={})=>new Promise((resolve,reject)=>{const id=++sequence,timer=setTimeout(()=>{pending.delete(id);reject(new Error(`CDP timeout: ${method}`));},method==='Runtime.evaluate'?2000:15000);
        pending.set(id,{resolve,reject,timer});socket.send(JSON.stringify({id,method,params}));});
    await call('Runtime.enable');
    return {contexts,call,close:()=>socket.close(),async evaluate(contextId,expression){const result=await call('Runtime.evaluate',{contextId,expression,returnByValue:true});if(result.exceptionDetails)return;return result.result?.value;}};
}
async function editorUi(port){
    const clients=new Map();
    async function document(predicate,expression,type='iframe'){
        let targets;
        // The DevTools listing can refuse or reset a connection transiently; callers retry through until().
        try{targets=await(await fetch(`http://127.0.0.1:${port}/json/list`,{signal:AbortSignal.timeout(10000)})).json();}
        catch(error){if(/ECONNREFUSED|ECONNRESET|UND_ERR_SOCKET|UND_ERR_CONNECT_TIMEOUT/.test(String(error?.cause?.code??'')))return;throw error;}
        for(const target of targets.filter(value=>value.type===type&&(type==='page'?value.url.includes('workbench'):value.url.includes('extensionId=iokaio.airk')))){
            let client=clients.get(target.id);if(!client){client=await connect(target);clients.set(target.id,client);}
            for(const context of client.contexts){try{if(await client.evaluate(context,predicate))return {value:await client.evaluate(context,expression),client,context,target};}
                catch(error){if(!/CDP timeout: Runtime.evaluate|Cannot find context|navigated or closed/.test(String(error)))throw error;}}
        }
    }
    const workbench=expression=>document('!!document.querySelector(".monaco-workbench")',expression,'page');
    // AIRK Explorer › Management drives the same native controller the Service Console used.
    const service=expression=>document("!!document.querySelector('.airk-explorer')",expression);
    const clickButton=label=>`(()=>{const b=[...document.querySelectorAll('button')].find(n=>n.textContent.trim()===${JSON.stringify(label)});if(!b||b.disabled)return false;b.click();return true;})()`;
    async function quickPick(label,title){
        const row=await until(async()=>{
            const result=await workbench(`(()=>{const widget=document.querySelector('.quick-input-widget');if(!widget||!widget.getBoundingClientRect().height||!widget.innerText.includes(${JSON.stringify(title)}))return;
                const rows=[...widget.querySelectorAll('.monaco-list-row')].filter(row=>row.getBoundingClientRect().height&&row.innerText.includes(${JSON.stringify(label)}));
                if(rows.length!==1)return;const r=rows[0].getBoundingClientRect();return {x:r.x+r.width/2,y:r.y+r.height/2};})()`);
            return result?.value?result:undefined;
        },`quick pick ${title}: ${label}`);
        await row.client.call('Input.dispatchMouseEvent',{type:'mousePressed',...row.value,button:'left',clickCount:1});
        await row.client.call('Input.dispatchMouseEvent',{type:'mouseReleased',...row.value,button:'left',clickCount:1});
        await delay(100);
        const stillPicking=await workbench(`(()=>{const w=document.querySelector('.quick-input-widget');return !!w?.getBoundingClientRect().height&&w.innerText.includes(${JSON.stringify(title)})&&[...w.querySelectorAll('.monaco-list-row.focused')].some(row=>row.innerText.includes(${JSON.stringify(label)}));})()`);
        if(stillPicking?.value){await row.client.call('Input.dispatchKeyEvent',{type:'keyDown',key:'Enter',code:'Enter',windowsVirtualKeyCode:13});
            await row.client.call('Input.dispatchKeyEvent',{type:'keyUp',key:'Enter',code:'Enter',windowsVirtualKeyCode:13});}
    }
    async function confirm(label,required){
        await until(async()=>{
            const result=await workbench(`(()=>{const dialogs=[...document.querySelectorAll('.monaco-dialog-box')].filter(node=>node.getBoundingClientRect().height);if(dialogs.length!==1)return false;
                const dialog=dialogs[0],text=dialog.innerText;if(!${JSON.stringify(required)}.every(value=>text.includes(value)))return false;
                const buttons=[...dialog.querySelectorAll('button,.monaco-button')].filter(node=>node.textContent.replace(/&/g,'').trim()===${JSON.stringify(label)});if(buttons.length!==1)return false;buttons[0].click();return true;})()`);
            return result?.value;
        },`scoped confirmation ${label}`);
    }
    async function register(request){
        await until(async()=>(await service(clickButton('Review workspace registration')))?.value,'workspace registration button');
        await quickPick(request.root,'Register a trusted source root with AIRK');
        await confirm('Review registration',['AIRK 1.4 uses a new service','fresh index namespace']);
        await quickPick('Manual indexing','AIRK indexing ownership');
        await confirm('Register',['Register this exact native project',request.source,request.storage,'Index data:','Automatic source watching is disabled']);
    }
    async function responseMode(mode){
        await until(async()=>(await service(clickButton('Choose terse / full responses')))?.value,'response mode button');
        await quickPick(mode==='full'?'Full':'Terse (default)','');
        await until(async()=>(await service(`document.body.innerText.includes(${JSON.stringify('MCP response mode: '+mode)})`))?.value,`response mode ${mode}`);
    }
    async function approveRepair(project){
        await until(async()=>(await service(`(()=>{const button=[...document.querySelectorAll('button')].find(n=>n.textContent.trim()==='Approve this exact preview');if(!button||button.disabled)return false;
            const rows=[...document.querySelectorAll('tr')].filter(row=>row.cells[0]?.textContent===${JSON.stringify(project)});
            if(rows.length!==1||!rows[0].innerText.includes('rebuild'))return false;button.click();return true;})()`))?.value,'exact one-project rebuild preview');
        await confirm('Continue',['Execute this exact native repair preview']);
        await until(async()=>(await service(`(()=>{
            const phase=[...document.querySelectorAll('dt')].find(node=>node.textContent==='Phase'&&node.parentElement.querySelector('dt')?.textContent==='Plan');
            return phase?.nextElementSibling?.textContent==='finished';})()`))?.value,'native repair completion');
        await until(async()=>(await service(clickButton('Refresh cached plan rows')))?.value,'refresh cached native repair receipts');
        const result=await until(async()=>(await service(`(()=>{
            const rows=[...document.querySelectorAll('tr')].filter(row=>row.cells[0]?.textContent===${JSON.stringify(project)});
            return rows.length===1&&['succeeded','failed','stale','skipped','canceled'].includes(rows[0].cells[1].textContent)?{phase:rows[0].cells[1].textContent,receipt:rows[0].innerText}:false;})()`))?.value,'native repair final receipt');
        if(result.phase!=='succeeded')throw new Error(`Native editor repair failed: ${result.receipt}`);
    }
    return {document,workbench,service,quickPick,confirm,register,approveRepair,responseMode,until,close(){for(const client of clients.values())client.close();}};
}
module.exports={editorUi,until};
