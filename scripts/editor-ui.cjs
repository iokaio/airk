// SPDX-License-Identifier: Apache-2.0
// Real UI automation for the explicitly launched private editor profiles only.
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function until(action,label,timeout=180000){const end=Date.now()+timeout;while(Date.now()<end){const value=await action();if(value)return value;await delay(120);}throw new Error(`Editor UI timeout: ${label}`);}
const pollUntil=until;
async function connect(target,deadline){
    const socket=new WebSocket(target.webSocketDebuggerUrl),contexts=new Set(),pending=new Map();let sequence=0;
    const budget=max=>{const left=Math.min(max,deadline-Date.now());if(left<=0)throw new Error('Shared editor scenario deadline exhausted');return left;};
    await new Promise((resolve,reject)=>{const timer=setTimeout(()=>{socket.close();reject(new Error('CDP connection timeout'));},budget(10000));socket.addEventListener('open',()=>{clearTimeout(timer);resolve();},{once:true});socket.addEventListener('error',error=>{clearTimeout(timer);reject(error);},{once:true});});
    socket.addEventListener('close',()=>{contexts.clear();for(const item of pending.values()){clearTimeout(item.timer);item.reject(new Error('CDP socket closed'));}pending.clear();});
    socket.addEventListener('message',event=>{let value;try{if(String(event.data).length>8388608)throw new Error('CDP frame bound exceeded');value=JSON.parse(event.data);}catch(error){for(const item of pending.values()){clearTimeout(item.timer);item.reject(error);}pending.clear();socket.close();return;}
        if(value.method==='Runtime.executionContextCreated')contexts.add(value.params.context.id);
        if(value.method==='Runtime.executionContextDestroyed')contexts.delete(value.params.executionContextId);
        if(value.method==='Runtime.executionContextsCleared')contexts.clear();
        const request=pending.get(value.id);if(request){pending.delete(value.id);clearTimeout(request.timer);value.error?request.reject(new Error(JSON.stringify(value.error))):request.resolve(value.result);}});
    const call=(method,params={})=>new Promise((resolve,reject)=>{if(pending.size>=16){reject(new Error('CDP pending bound exhausted'));return;}const id=++sequence,timer=setTimeout(()=>{pending.delete(id);reject(new Error(`CDP timeout: ${method}`));},budget(method==='Runtime.evaluate'?2000:15000));
        pending.set(id,{resolve,reject,timer});socket.send(JSON.stringify({id,method,params}));});
    await call('Runtime.enable');
    return {contexts,call,close:()=>socket.close(),async evaluate(contextId,expression){const result=await call('Runtime.evaluate',{contextId,expression,returnByValue:true});if(result.exceptionDetails)throw new Error('CDP expression failed: '+JSON.stringify(result.exceptionDetails));return result.result?.value;}};
}
async function editorUi(port,deadline=Date.now()+600000){
    const clients=new Map();
    let reads=0,dispatches=0;const dispatchLog=[];
    const budget=max=>{const left=Math.min(max,deadline-Date.now());if(left<=0)throw new Error('Shared editor scenario deadline exhausted');return left;};
    const boundedUntil=(action,label,timeout=180000)=>pollUntil(action,label,budget(timeout));
    const until=boundedUntil;
    async function document(predicate,expression,type='iframe'){
        if(/\.click\(|\.dispatchEvent\(|\.set\.call\(|\.value\s*=(?!=)|\.checked\s*=(?!=)/.test(expression))throw new Error('Mutating expression requires captured-context dispatch');
        if(++reads>12000)throw new Error('Private editor DOM observation budget exhausted');
        let targets;
        // The DevTools listing can refuse or reset a connection transiently; callers retry through until().
        try{targets=await(await fetch(`http://127.0.0.1:${port}/json/list`,{signal:AbortSignal.timeout(budget(10000))})).json();}
        catch(error){if(/ECONNREFUSED|ECONNRESET|UND_ERR_SOCKET|UND_ERR_CONNECT_TIMEOUT/.test(String(error?.cause?.code??'')))return;throw error;}
        for(const target of targets.filter(value=>value.type===type&&(type==='page'?value.url.includes('workbench'):value.url.includes('extensionId=iokaio.airk')))){
            let client=clients.get(target.id);if(!client){client=await connect(target,deadline);clients.set(target.id,client);}
            for(const context of client.contexts){try{if(await client.evaluate(context,predicate))return {value:await client.evaluate(context,expression),client,context,target};}
                catch(error){if(!/CDP timeout: Runtime.evaluate|Cannot find context|navigated or closed/.test(String(error)))throw error;}}
        }
    }
    const workbench=expression=>document('!!document.querySelector(".monaco-workbench")',expression,'page');
    // AIRK Explorer › Management drives the same native controller the Service Console used.
    const service=expression=>document("!!document.querySelector('.airk-explorer')",expression);
    async function dispatch(predicate,ready,expression,type='iframe'){
        const target=await until(async()=>{const found=await document(predicate,ready,type);return found?.value?found:undefined;},'exact mutation target');
        const number=++dispatches;
        if(number>1024)throw new Error('UI mutation dispatch budget exhausted');
        const receipt={dispatch:number,context:target.context,target:target.target.id,started:Date.now(),expression_sha256:require('node:crypto').createHash('sha256').update(expression).digest('hex')};dispatchLog.push(receipt);
        // Never put this evaluation inside document's context-discovery retry loop.
        try{const value=await target.client.evaluate(target.context,expression);if(value!==true)throw new Error('Unknown UI dispatch outcome; no replay: '+number);receipt.completed=Date.now();return {...receipt};}
        catch(error){receipt.error=String(error);throw error;}
    }
    const explorerPredicate="!!document.querySelector('.airk-explorer')",workbenchPredicate='!!document.querySelector(".monaco-workbench")';
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
        await delay(budget(100));
        const stillPicking=await workbench(`(()=>{const w=document.querySelector('.quick-input-widget');return !!w?.getBoundingClientRect().height&&w.innerText.includes(${JSON.stringify(title)})&&[...w.querySelectorAll('.monaco-list-row.focused')].some(row=>row.innerText.includes(${JSON.stringify(label)}));})()`);
        if(stillPicking?.value)throw new Error('Quick pick click has an unknown outcome; no fallback dispatch');
    }
    async function confirm(label,required){
        const find=`(()=>{const dialogs=[...document.querySelectorAll('.monaco-dialog-box')].filter(n=>n.getBoundingClientRect().height);if(dialogs.length!==1)return;const dialog=dialogs[0];if(!${JSON.stringify(required)}.every(value=>dialog.innerText.includes(value)))return;const buttons=[...dialog.querySelectorAll('button,.monaco-button')].filter(n=>n.textContent.replace(/&/g,'').trim()===${JSON.stringify(label)});return buttons.length===1?buttons[0]:null;})()`;
        return dispatch(workbenchPredicate,`!!${find}`,`(()=>{const button=${find};if(!button)return false;button.click();return true;})()`,'page');
    }
    // Waits inspect the DOM only. Dispatch a mutating click exactly once: an
    // unknown CDP/native outcome must never resubmit Apply on a polling loop.
    async function click(label,card){
        const find=`(()=>{const roots=${card?`[...document.querySelectorAll('article')].filter(a=>[...a.children].some(n=>/^H[23]$/.test(n.tagName)&&n.textContent.trim()===${JSON.stringify(card)}))`:`[document]`};const b=roots.flatMap(a=>[...a.querySelectorAll('button')]).filter(b=>b.textContent.trim()===${JSON.stringify(label)});return b.length===1&&!b[0].disabled?b[0]:null;})()`;
        return dispatch(explorerPredicate,`!!${find}`,`(()=>{const b=${find};if(!b||b.disabled)return false;b.click();return true;})()`);
    }
    async function fill(button,values){
        await dispatch(explorerPredicate,`(()=>{const b=[...document.querySelectorAll('button')].filter(b=>b.textContent.trim()===${JSON.stringify(button)});return b.length===1&&!!b[0].closest('form');})()`,`(()=>{const buttons=[...document.querySelectorAll('button')].filter(b=>b.textContent.trim()===${JSON.stringify(button)});const form=buttons.length===1?buttons[0].closest('form'):null;if(!form)return false;const values=${JSON.stringify(values)};for(const [label,value]of Object.entries(values)){const node=[...form.querySelectorAll('label')].find(n=>[...n.childNodes].filter(c=>c.nodeType===Node.TEXT_NODE).map(c=>c.textContent).join('').trim()===label)?.querySelector('input,select');if(!node)return false;const proto=node.tagName==='SELECT'?HTMLSelectElement.prototype:HTMLInputElement.prototype;Object.getOwnPropertyDescriptor(proto,node.type==='checkbox'?'checked':'value').set.call(node,value);node.dispatchEvent(new Event(node.tagName==='SELECT'||node.type==='checkbox'?'change':'input',{bubbles:true}));}return true;})()`);
    }
    async function catalog(){
        return until(async()=>{const value=(await service("(()=>{const d=[...document.querySelectorAll('details')].find(d=>d.querySelector('summary')?.textContent==='Complete response');if(!d)return;try{return JSON.parse(d.querySelector('pre').textContent);}catch{return;}})()"))?.value;return value||undefined;},'complete native catalog details');
    }
    async function settled(predicate){
        return until(async()=>{const value=await catalog();if(value.error)throw new Error(value.error);return !value.busy&&predicate(value)?value:undefined;},'native catalog outcome');
    }
    async function inspect(kind,id){
        const label=kind==='grant'?'Inspect grant':'Inspect';
        const query=`(()=>{const cards=[...document.querySelectorAll('article')].filter(a=>[...a.children].some(n=>n.tagName==='P'&&[...n.querySelectorAll('code')].some(c=>c.textContent===${JSON.stringify(id)})));const b=cards.flatMap(a=>[...a.children].filter(n=>n.tagName==='BUTTON'&&n.textContent.trim()===${JSON.stringify(label)}));return b.length===1&&!b[0].disabled?b[0]:null;})()`;
        await dispatch(explorerPredicate,`!!${query}`,`(()=>{const b=${query};if(!b)return false;b.click();return true;})()`);
        return settled(v=>!!(kind==='grant'?v.grantDetail:v.detail));
    }
    async function indexing(request){
        if(request.action==='serviceStop'){
            await click('Check status now');
            const before=await until(async()=>{const found=await service("(()=>{const b=[...document.querySelectorAll('button')].find(n=>n.textContent.trim()==='Stop service');return b&&!b.disabled?{text:document.body.innerText}:false;})()");return found?.value;},'healthy owned fixture Stop control');
            const captureExpression="(()=>{const text=document.querySelector('[data-testid=\"captured-native-lifecycle\"] pre')?.textContent;if(!text)return;return JSON.parse(text);})()";
            const previous=(await service(captureExpression))?.value,started=Date.now();
            const receipt=await click('Stop service'),confirmation=await confirm('Continue',['Stop AIRK on','Instance','native work will drain']);
            const capture=await until(async()=>{const value=(await service(captureExpression))?.value;return value&&value.capturedAt>=started&&value.response?.record?.id!==previous?.response?.record?.id?value:undefined;},'new complete captured native lifecycle response');
            return {preStop:before,dispatch:receipt,confirmation,started,previous_capture:previous??null,capture,requested_binding:{installDir:request.installDir,runtimeSha256:request.runtimeSha256,namespace:request.namespace}};
        }
        if(request.action==='restricted'){
            const result=await until(async()=>(await service("(()=>{if(!document.body.innerText.includes('Restricted Mode'))return;const labels=['Prepare native setup review','Apply reviewed index setup','Persist reviewed grant','Use Grant for this window'];return {blocked:![...document.querySelectorAll('button')].some(b=>!b.disabled&&labels.includes(b.textContent.trim())),text:document.body.innerText};})()"))?.value,'actual Restricted Mode view');
            if(!result.blocked)throw new Error('Restricted Mode exposed a native mutation');
            await click('Manage Workspace Trust');return result;
        }
        if(request.action.startsWith('usage')){
            const scoped="document.visibilityState==='visible'&&document.body.innerText.includes('Usage requires Index administration')";
            const admin="document.visibilityState==='visible'&&document.body.innerText.includes('Entire Index history and shared data-root totals')&&document.body.innerText.includes("+JSON.stringify(request.collection??'')+")&&!!document.querySelector('[data-usage=clear]')";
            const report=(predicate,expression)=>document(predicate,expression);
            const reportClick=async(predicate,action)=>{
                const selector='[data-usage="'+action+'"]';
                return dispatch(predicate,`!!document.querySelector(${JSON.stringify(selector)})&&!document.querySelector(${JSON.stringify(selector)}).disabled`,`(()=>{const b=document.querySelector(${JSON.stringify(selector)});if(!b||b.disabled)return false;b.click();return true;})()`);
            };
            if(request.action==='usageScoped'){
                const result=await until(async()=>(await report(scoped,"({text:document.body.innerText,writeControls:document.querySelectorAll('[data-usage=clear],[data-usage=recording],[data-usage=import]').length})"))?.value,'scoped usage unavailable');
                if(result.writeControls!==0)throw new Error('Scoped Usage exposed administration without explicit choice');return result;
            }
            if(request.action==='usageAdmin'){await reportClick(scoped,'openIndexAdministration');return until(async()=>(await report(admin,'({text:document.body.innerText})'))?.value,'explicit broad Index administration');}
            if(request.action==='usageJson'){await reportClick(admin,'json');return {requested:true};}
            if(request.action==='usageClear'){
                await reportClick(admin,'clear');await confirm('Clear history',['Clear all usage history',request.name,'source files and index stay intact']);
                return until(async()=>(await report(admin,"document.body.innerText.includes('No usage recorded yet.')?{cleared:true,text:document.body.innerText}:false"))?.value,'selected entire-Index usage clear');
            }
        }
        switch(request.action){
            case 'connect':await click('Connect / reconnect');return {connected:true};
            case 'page':await click(request.kind==='projects'?'Refresh Projects':request.kind==='grants'?'Read grants':'Refresh indexes');return settled(v=>!!(request.kind==='grants'?v.grants:v.page));
            case 'inspect':return inspect(request.kind,request.id);
            case 'setupDraft':
                for(let count=Number((await service("[...document.querySelectorAll('form')].find(f=>[...f.querySelectorAll('button')].some(b=>b.textContent==='Prepare native setup review'))?.querySelectorAll('fieldset').length??0"))?.value);count>0;count--)await click('Remove additional source '+count);
                if(request.additional){await click('Add source location');}
                await fill('Prepare native setup review',{'Index name':request.name,'Primary source location':request.source,'Index storage location':request.storage,'Collection':request.collection,'Enable native watching after setup':false,...(request.additional?{'Alias':request.additional.alias,'Source location':request.additional.path}:{})});
                await click('Prepare native setup review');return settled(v=>!!v.setupReview);
            case 'setupApply':await click('Apply reviewed index setup');return settled(v=>!!v.outcome);
            case 'projectDraft':await fill('Preview create Project',{'New Project name':request.name});await click('Preview create Project');return settled(v=>!!v.review);
            case 'renameDraft':await fill('Preview rename',{'New name':request.name});await click('Preview rename');return settled(v=>!!v.review);
            case 'membershipDraft':{
                await fill('Preview membership',{'Index ID':request.index,'Current membership revision (blank for new membership)':''});await click('Read Sources for this index');const sources=await settled(v=>!!v.sources);
                const rows=sources.sources.page.items.map(row=>row.value);
                const selected=request.roots.map(root=>{const row=rows.find(row=>row.canonical_path===root.path&&row.index_id===request.index&&row.enabled===true);if(!row)throw new Error('Native Source identity unavailable');return {...root,id:row.id};});
                let count=Number((await service("[...document.querySelectorAll('form')].find(f=>[...f.querySelectorAll('button')].some(b=>b.textContent==='Preview membership'))?.querySelectorAll('fieldset').length??0"))?.value);
                while(count>selected.length){await click('Remove scope '+count);count--;}
                while(count<selected.length){await click('Add source scope');count++;}
                for(let at=0;at<selected.length;at++){
                    const root=selected[at];
                    await dispatch(explorerPredicate,`!![...document.querySelectorAll('form')].find(f=>[...f.querySelectorAll('button')].some(b=>b.textContent==='Preview membership'))?.querySelectorAll('fieldset')[${at}]`,`(()=>{const form=[...document.querySelectorAll('form')].find(f=>[...f.querySelectorAll('button')].some(b=>b.textContent==='Preview membership'));const field=form?.querySelectorAll('fieldset')[${at}];if(!field)return false;for(const [label,value]of Object.entries(${JSON.stringify({'Source ID':root.id,'Canonical relative path':root.relative,'Scope':root.kind})})){const node=[...field.querySelectorAll('label')].find(n=>[...n.childNodes].filter(c=>c.nodeType===Node.TEXT_NODE).map(c=>c.textContent).join('').trim()===label)?.querySelector('input,select');if(!node)return false;Object.getOwnPropertyDescriptor(node.tagName==='SELECT'?HTMLSelectElement.prototype:HTMLInputElement.prototype,'value').set.call(node,value);node.dispatchEvent(new Event(node.tagName==='SELECT'?'change':'input',{bubbles:true}));}return true;})()`);
                }
                await click('Preview membership');return settled(v=>!!v.review);
            }
            case 'apply':await click(request.grant?'Persist reviewed grant':'Apply reviewed changes');return settled(v=>!!v.outcome);
            case 'context':await click('Read saved association');return settled(v=>!!v.hostContext||!!v.context);
            case 'associationDraft':await fill('Preview saved association',{'Project IDs (comma separated; blank selects zero Projects)':request.project,'Default index IDs (comma separated; blank selects no index defaults)':request.index});await click('Preview saved association');return settled(v=>!!v.review);
            case 'grantDraft':await fill('Prepare native grant review',{'Project IDs (1–32, comma separated)':request.project,"Optional index subset (1–32 IDs; blank asks native to include the selected Projects' memberships)":request.index});await click('Prepare native grant review');return settled(v=>!!v.review);
            case 'useGrant':await fill('Use Grant for this window',{'Index ID from this grant':request.index,'Project IDs from this grant (1–32, comma separated)':request.project});await click('Use Grant for this window');return settled(v=>!!v.outcome?.selected_subset);
            case 'revokeDraft':await click('Preview revoke grant');return settled(v=>!!v.review);
            case 'read':return catalog();
            case 'reconcile':await click('Check native receipts');return settled(v=>Array.isArray(v.outcome?.receipts));
            case 'staleApply':{
                const original=await catalog(),receipt=await click('Apply reviewed changes');
                const value=await until(async()=>{const v=await catalog();return !v.busy&&(v.error||v.outcome)?v:undefined;},'stale native preview refusal');
                return {...value,ui_dispatch:receipt,original_review:original.review,dispatch_count:1};
            }
            default:throw new Error('Unknown bounded Indexing action');
        }
    }
    async function responseMode(mode){
        await click('Choose terse / full responses');
        await quickPick(mode==='full'?'Full':'Terse (default)','');
        await until(async()=>(await service(`document.body.innerText.includes(${JSON.stringify('MCP response mode: '+mode)})`))?.value,`response mode ${mode}`);
    }
    async function approveRepair(project){
        const ready=`(()=>{const b=[...document.querySelectorAll('button')].find(n=>n.textContent.trim()==='Approve this exact preview');const rows=[...document.querySelectorAll('tr')].filter(r=>r.cells[0]?.textContent===${JSON.stringify(project)});return !!b&&!b.disabled&&rows.length===1&&rows[0].innerText.includes('rebuild');})()`;
        await dispatch(explorerPredicate,ready,clickButton('Approve this exact preview'));
        await confirm('Continue',['Execute this exact native repair preview']);
        await until(async()=>(await service(`(()=>{
            const phase=[...document.querySelectorAll('dt')].find(node=>node.textContent==='Phase'&&node.parentElement.querySelector('dt')?.textContent==='Plan');
            return phase?.nextElementSibling?.textContent==='finished';})()`))?.value,'native repair completion');
        await click('Refresh cached plan rows');
        const result=await until(async()=>(await service(`(()=>{
            const rows=[...document.querySelectorAll('tr')].filter(row=>row.cells[0]?.textContent===${JSON.stringify(project)});
            return rows.length===1&&['succeeded','failed','stale','skipped','canceled'].includes(rows[0].cells[1].textContent)?{phase:rows[0].cells[1].textContent,receipt:rows[0].innerText}:false;})()`))?.value,'native repair final receipt');
        if(result.phase!=='succeeded')throw new Error(`Native editor repair failed: ${result.receipt}`);
    }
    return {dispatch,dispatchReceipts:()=>structuredClone(dispatchLog),document,workbench,service,quickPick,confirm,indexing,catalog,click,approveRepair,responseMode,until,close(){for(const client of clients.values())client.close();}};
}
module.exports={editorUi,until};
