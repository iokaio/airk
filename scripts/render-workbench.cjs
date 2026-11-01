// SPDX-License-Identifier: Apache-2.0
// Real Workbench browser bundle, fictional bridge; no native work or editor qualification.
const fs=require('node:fs/promises'),path=require('node:path'),assert=require('node:assert/strict');
const {spawn}=require('node:child_process'),{pathToFileURL}=require('node:url');
const root=path.resolve(__dirname,'..'),output=path.join(root,process.argv.some(arg=>['--mcp-navigation','--form-drafts'].includes(arg))?'artifacts/mcp-optimizations-1.8.4/browser':process.argv.some(arg=>['--scroll','--selection'].includes(arg))?'artifacts/workbench-review-1.8.2/browser':'artifacts/workbench-review-1.7.1/browser');
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
/** Fictional host session ownership, including the real scalar form acknowledgement shape. */
function installFixtureHost(){
    const clone=value=>structuredClone(value),defaults=()=>({activeTool:'search',values:{},scope:'',recipe:'',revision:0});
    window.fixture.sessionEpoch??=0;
    window.fixture.session.formDraft??=defaults();
    window.fixtureSessions=new Map([[window.fixture.session.id,clone(window.fixture.session)]]);
    let sequence=0;
    const remember=()=>window.fixtureSessions.set(window.fixture.session.id,clone(window.fixture.session));
    const emit=()=>window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:clone(window.fixture)}}));
    const recent=()=>window.fixture.recent=[...window.fixtureSessions.values()].map(session=>({id:session.id,title:session.title,updatedAt:session.updatedAt,turns:session.turns.length}));
    window.fixtureSelectSession=id=>{
        remember();const session=window.fixtureSessions.get(id);if(!session)return;
        window.fixture.session=clone(session);window.fixture.sessionEpoch++;window.fixture.cards={};window.fixture.continuations={};recent();emit();
    };
    window.acquireVsCodeApi=()=>({postMessage(message){
        window.messages.push(clone(message));
        if(message.type==='ready'){setTimeout(emit,0);return;}
        if(message.type==='recent'){window.fixtureSelectSession(message.id);return;}
        if(['new','clear','fork'].includes(message.type)){
            remember();const before=window.fixture.session;
            if(message.type==='clear')for(const [id,session]of window.fixtureSessions)if(session.project===before.project)window.fixtureSessions.delete(id);
            window.fixture.session={...clone(before),id:`browser-new-session-${++sequence}`,title:'New session',revision:0,formDraft:defaults(),turns:message.type==='fork'?clone(before.turns):[]};
            window.fixture.sessionEpoch++;window.fixture.cards={};window.fixture.continuations={};remember();recent();emit();return;
        }
        if(message.type==='formDraft'||message.type==='resetForm'){
            const session=window.fixture.session,current=session.formDraft??defaults();
            if(message.sessionId!==session.id||message.project!==session.project||message.epoch!==window.fixture.sessionEpoch||message.revision<=current.revision)return;
            if(message.type==='resetForm'&&message.tool!==current.activeTool)return;
            const draft=clone(current);draft.activeTool=message.tool;draft.revision=message.revision;
            if(message.type==='resetForm')delete draft.values[message.tool];
            else {if(message.values!==undefined)draft.values[message.tool]=clone(message.values);if(message.scope!==undefined)draft.scope=message.scope;if(message.recipe!==undefined)draft.recipe=message.recipe;}
            session.formDraft=draft;remember();
            window.dispatchEvent(new MessageEvent('message',{data:{type:'formDraft',sessionId:session.id,project:session.project,epoch:window.fixture.sessionEpoch,formDraft:clone(draft),save:'Choices saved in browser fixture'}}));
        }
    }});
}
function html(scene,dark){
    const script=pathToFileURL(path.join(root,'packages/vscode/out/workbenchWebview.js')).href,css=pathToFileURL(path.join(root,'packages/vscode/out/workbenchWebview.css')).href;
    const foreground=dark?'#ddd':'#333',surface=dark?'#2d2d30':'#fff';
    return `<!doctype html><html><head><meta charset='utf-8'><link rel='stylesheet' href='${css}'><style>:root{--vscode-font-family:Arial,sans-serif;--vscode-font-size:13px;--vscode-foreground:${foreground};--vscode-sideBar-background:${dark?'#252526':'#f3f3f3'};--vscode-input-background:${surface};--vscode-input-foreground:${foreground};--vscode-editorWidget-background:${surface};--vscode-descriptionForeground:${dark?'#aaa':'#666'};--vscode-panel-border:${dark?'#555':'#ccc'};--vscode-button-secondaryBackground:${dark?'#3a3d41':'#e1e1e1'};--vscode-button-secondaryHoverBackground:${dark?'#4a4d51':'#ddd'};--vscode-button-background:#007acc;--vscode-button-foreground:#fff;--vscode-focusBorder:#007fd4;--vscode-textLink-foreground:${dark?'#4daafc':'#005fb8'};--vscode-textCodeBlock-background:${dark?'#181818':'#eee'};--vscode-editorWarning-foreground:#cca700;--vscode-errorForeground:#f44747;--vscode-editor-font-family:Consolas,monospace;--vscode-editor-font-size:12px}</style></head><body><div id='root'></div><script>window.fixture=${JSON.stringify(fixture(scene)).replaceAll('<','\\u003c')};window.messages=[];(${installFixtureHost.toString()})();</script><script src='${script}'></script></body></html>`;
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
        if(process.argv.includes('--form-drafts')){
            await call('Emulation.setDeviceMetricsOverride',{width:640,height:1200,deviceScaleFactor:1,mobile:false});
            const file=path.join(scratch,'form-drafts.html');await fs.writeFile(file,html('welcome',true));
            await call('Page.navigate',{url:pathToFileURL(file).href});await until(`!!document.querySelector('.composer textarea')`);
            const operations={search:{type:'object',required:['query'],properties:{query:{type:'string'},language:{type:'string'}}},
                symbols:{type:'object',properties:{name:{type:'string'},name_match:{type:'string',enum:['exact','suffix','contains']},compact:{type:'boolean'},include_count:{type:'boolean'}}},
                references:{type:'object',required:['name'],properties:{name:{type:'string'},order:{type:'string',enum:['index','path','code_first']},include_files:{type:'boolean'}}}};
            await evaluate(`window.fixture.operations=${JSON.stringify(operations)};window.fixture.session.title='Session alpha';window.fixture.sessionEpoch=0;window.alphaId=window.fixture.session.id;window.fixtureSessions.set(window.alphaId,structuredClone(window.fixture.session));window.betaId='browser-session-beta';window.fixtureSessions.set(window.betaId,{...structuredClone(window.fixture.session),id:window.betaId,title:'Session beta',turns:[],formDraft:{activeTool:'search',values:{},revision:0,scope:'',recipe:''}});window.fixture.recent=[...window.fixtureSessions.values()].map(session=>({id:session.id,title:session.title,updatedAt:session.updatedAt,turns:0}));window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:structuredClone(window.fixture)}}));`);
            const openForm=async()=>{if(!await evaluate(`!!document.querySelector('#evidence-options')`))await evaluate(`document.querySelector('[aria-label="Add scope, recipe or tool"]').click()`);await until(`!!document.querySelector('.tool-form form')`);};
            const fieldValue=label=>evaluate(`document.querySelector('.tool-form [aria-label=${JSON.stringify(label)}]')?.value`);
            const setField=async(label,value)=>{await evaluate(`(()=>{const field=document.querySelector('.tool-form [aria-label=${JSON.stringify(label)}]');if(!field)throw new Error('Missing field');field.value=${JSON.stringify(value)};field.dispatchEvent(new Event(field.tagName==='SELECT'?'change':'input',{bubbles:true}));})()`);await delay(25);};
            const selectTool=async tool=>{await setField('Operation',tool);await until(`document.querySelector('.tool-form [aria-label="Operation"]').value===${JSON.stringify(tool)}`);};
            const closeForm=async()=>{await evaluate(`document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}))`);await until(`!document.querySelector('#evidence-options')`);};
            const selectSession=async title=>{await closeForm();await evaluate(`document.querySelector('[aria-label="Session history"]').click()`);await until(`!!document.querySelector('#session-history')`);await evaluate(`[...document.querySelectorAll('#session-history .history-item')].find(button=>button.textContent.includes(${JSON.stringify(title)})).click()`);await until(`!document.querySelector('#session-history')`);await openForm();};
            await openForm();await setField('query','alpha query');await setField('language','rust');
            await evaluate(`(()=>{const scope=document.querySelector('[aria-label="Literal collection-relative path prefix"]');scope.value='src/alpha/';scope.dispatchEvent(new Event('input',{bubbles:true}));})()`);await delay(25);
            await evaluate(`(()=>{const recipe=document.querySelector('[aria-label="Recipe"]');recipe.value='component';recipe.dispatchEvent(new Event('change',{bubbles:true}));})()`);await delay(25);
            await selectTool('symbols');await setField('name','SessionAlpha');await setField('Name matching','suffix');await setField('Compact results','false');
            await closeForm();await openForm();
            assert.equal(await fieldValue('Operation'),'symbols');assert.equal(await fieldValue('name'),'SessionAlpha');assert.equal(await fieldValue('Name matching'),'suffix');assert.equal(await fieldValue('Compact results'),'false');
            assert.equal(await evaluate(`document.querySelector('[aria-label="Literal collection-relative path prefix"]').value`),'src/alpha/');
            assert.equal(await evaluate(`document.querySelector('[aria-label="Recipe"]').value`),'component');
            await selectTool('search');assert.equal(await fieldValue('query'),'alpha query');assert.equal(await fieldValue('language'),'rust');
            checks.push({check:'Close/reopen, operator maps and shared scope/recipe restore within the session',passed:true});

            await selectSession('Session beta');assert.equal(await fieldValue('Operation'),'search');assert.equal(await fieldValue('query'),'');
            assert.equal(await evaluate(`document.querySelector('[aria-label="Literal collection-relative path prefix"]').value`),'');
            assert.equal(await evaluate(`document.querySelector('[aria-label="Recipe"]').value`),'');
            await setField('query','beta query');await selectTool('references');await setField('name','SessionBeta');await setField('order','path');
            await selectSession('Session alpha');assert.equal(await fieldValue('Operation'),'search');assert.equal(await fieldValue('query'),'alpha query');
            await selectTool('symbols');assert.equal(await fieldValue('name'),'SessionAlpha');
            checks.push({check:'History A to B and back restores each session own active tool and values',passed:true});

            const beforeNative=await evaluate(`window.messages.filter(message=>['run','continue','followTarget','followDefinition'].includes(message.type)).length`);
            await evaluate(`document.querySelector('[aria-label="Reset this tool form"]').focus({preventScroll:true})`);
            assert.equal(await evaluate(`document.activeElement.title`),'Reset this tool form');
            assert.equal(await evaluate(`document.activeElement.type`),'button');
            for(const type of ['keyDown','keyUp'])await call('Input.dispatchKeyEvent',{type,key:' ',code:'Space',windowsVirtualKeyCode:32,nativeVirtualKeyCode:32});
            await until(`document.querySelector('.tool-form [aria-label="name"]').value===''`);
            assert.equal(await fieldValue('Operation'),'symbols');assert.equal(await fieldValue('Name matching'),'');
            assert.equal(await evaluate(`document.querySelector('[aria-label="Literal collection-relative path prefix"]').value`),'src/alpha/');
            assert.equal(await evaluate(`document.querySelector('[aria-label="Recipe"]').value`),'component');
            await selectTool('search');assert.equal(await fieldValue('query'),'alpha query');
            assert.equal(await evaluate(`window.messages.filter(message=>['run','continue','followTarget','followDefinition'].includes(message.type)).length`),beforeNative,'editing, switching and keyboard reset never emit evidence execution');
            const beforeAck=await evaluate(`JSON.stringify(window.fixture.session.formDraft)`);
            await evaluate(`(()=>{const current=structuredClone(window.fixture.session.formDraft),stale={...current,revision:Math.max(0,current.revision-1),values:{search:{query:'stale injection'}}};window.dispatchEvent(new MessageEvent('message',{data:{type:'formDraft',sessionId:window.fixture.session.id,project:window.fixture.session.project,epoch:window.fixture.sessionEpoch,formDraft:stale,save:'stale'}}));})()`);await delay(25);
            assert.equal(await fieldValue('query'),'alpha query');assert.equal(await evaluate(`JSON.stringify(window.fixture.session.formDraft)`),beforeAck);
            checks.push({check:'Accessible Reset icon clears active fields only; stale acknowledgement ignored; no query side effects',passed:true});

            await closeForm();await evaluate(`document.querySelector('[aria-label="New session"]').click()`);await openForm();
            assert.equal(await fieldValue('Operation'),'search');assert.equal(await fieldValue('query'),'');
            assert.equal(await evaluate(`document.querySelector('[aria-label="Literal collection-relative path prefix"]').value`),'');
            assert.equal(await evaluate(`document.querySelector('[aria-label="Recipe"]').value`),'');
            assert.deepEqual(await evaluate(`window.fixture.session.formDraft.values`),{});
            checks.push({check:'New session starts with defaults and inherits no previous form/scope/recipe',passed:true});
            const png=await call('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});await fs.writeFile(path.join(output,'session-form-defaults-reset.png'),Buffer.from(png.data,'base64'));
            await fs.writeFile(path.join(output,'form-drafts-summary.json'),JSON.stringify({basis:'Actual compiled Workbench component with fictional session-owning host and scalar acknowledgements; separate real host/store tests cover persistence and fences; no installed/native qualification',checks},null,2)+'\n');
            console.log('Workbench per-session choices, operator maps, new defaults and keyboard Reset browser checks passed.');await call('Browser.close');return;
        }
        if(process.argv.includes('--mcp-navigation')){
            await call('Emulation.setDeviceMetricsOverride',{width:480,height:900,deviceScaleFactor:1,mobile:false});
            const file=path.join(scratch,'mcp-navigation.html');await fs.writeFile(file,html('results',true));
            await call('Page.navigate',{url:pathToFileURL(file).href});await until(`!!document.querySelector('.result')`);
            const nativeFields={name:{type:'string'},name_match:{type:'string',enum:['exact','suffix','contains']},include_count:{type:'boolean'},include_files:{type:'boolean'},
                language:{type:'string',maxLength:128},evidence_kind:{type:'string',enum:['code','document']},exclude_path_prefix:{type:'string'},
                limit:{type:'integer',minimum:1,maximum:100},cursor:{type:'string',minLength:1,maxLength:32768}};
            const discoverySteps=[
                {id:'refs',tool:'references',status:'complete',arguments:{name:'Session',path_prefix:'src/'},result:{publication:{token:publication},evidence:{truncated:true,resolution:'textual',coverage:'Candidate scan remains incomplete.'},page:{cursor:'retained-reference-page',more:true,required_record_bytes:12000,count_unit:'matching_lines',files:[{path:'src/session.rs',count:2}],match_count:{total_matches:null,lower_bound:2,complete:false,scope:'scanned_candidates'}}}},
                {id:'find',tool:'search',status:'complete',result:{publication:{token:publication},definitions:[{name:'Session',total_matches:null,lower_bound:1,complete:false,evidence_scope:'native_definition_records',matches:[{symbol_id:'syntax:session',citation:{path:'src/session.rs',content_hash:hash,start_line:3,end_line:5}}]}]}},
                {id:'links',tool:'relationships',status:'complete',arguments:{path_prefix:'src/'},result:{publication:{token:publication},suggested_targets:[{target:'Session::spawn',citation:{path:'src/session.rs'},coverage:'Written target only; not resolved callers.'}]}},
            ];
            await evaluate(`window.fixture.session.turns[0].envelope={...window.fixture.session.turns[0].envelope,executed:true,status:'partial',steps:${JSON.stringify(discoverySteps)}};window.fixture.operations={search:{type:'object',properties:{query:{type:'string'}}},symbols:{type:'object',required:['name'],properties:${JSON.stringify(nativeFields)}}};window.fixture.continuations={'turn-1':[{id:'page-0',tool:'references',callsRemaining:9,requiredRecordBytes:12000}]};window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:structuredClone(window.fixture)}}))`);
            await until(`document.querySelectorAll('.discovery-details').length===3`);
            assert.equal(await evaluate(`document.body.textContent.includes('At least 2 verified matching lines')&&document.body.textContent.includes('total unknown')`),true,'lower bounds and unknown totals are visible');
            assert.equal(await evaluate(`document.querySelector('[aria-label="references discovery details"]').textContent.includes('12000 bytes')`),true,'oversized next record is explained');
            assert.equal(await evaluate(`document.body.textContent.includes('not compiler-resolved callers')`),true,'candidate relationship limits remain explicit');
            await evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Find callers of Session::spawn').click()`);
            assert.deepEqual(await evaluate(`window.messages.filter(message=>message.type==='followTarget').at(-1)`),{type:'followTarget',turn:'turn-1',step:'links',target:'Session::spawn'});
            await evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Inspect Session — src/session.rs').click()`);
            assert.deepEqual(await evaluate(`window.messages.filter(message=>message.type==='followDefinition').at(-1)`),{type:'followDefinition',turn:'turn-1',step:'find',name:'Session',path:'src/session.rs'});
            await evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent.startsWith('Fetch next references page')).click()`);
            assert.deepEqual(await evaluate(`window.messages.filter(message=>message.type==='continue').at(-1)`),{type:'continue',turn:'turn-1',step:'page-0'});
            checks.push({check:'structured discoveries, qualified follow-ups and guarded pagination',passed:true});

            for(const count of [0,2]){
                const coverage={basis:'verified_lexical_candidates',candidate_scan_complete:true,query_complete:false,query_total_matches:null,
                    token_boundary_complete:false,line_completeness:count===0?'no_returned_lines':'complete_source_lines'};
                await evaluate(`(()=>{const page=window.fixture.session.turns[0].envelope.steps[0].result.page;Object.assign(page,{more:false,cursor:undefined,required_record_bytes:undefined,count_unit:'matching_lines',coverage:${JSON.stringify(coverage)},match_count:{total_matches:${count},lower_bound:${count},complete:true,scope:'verified_lexical_candidates'}});window.fixture.continuations={'turn-1':[]};window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:structuredClone(window.fixture)}}));})()`);
                await until(`document.querySelector('[aria-label="references discovery details"] .counts')?.textContent.includes('${count} matching lines in verified lexical candidates; whole-query total unknown')`);
                assert.equal(await evaluate(`document.querySelector('[aria-label="references discovery details"]').textContent.includes('identifier tokens split across indexed chunk boundaries can be missed')`),true,`candidate/global distinction remains visible at ${count} matches`);
                assert.equal(await evaluate(`[...document.querySelectorAll('button')].some(button=>button.textContent.startsWith('Fetch next references page'))`),false,'terminal candidate coverage must not invent a fetch cursor');
            }
            await evaluate(`(()=>{const page=window.fixture.session.turns[0].envelope.steps[0].result.page;Object.assign(page,{count_unit:'matching_line_fragments',incomplete_reason:'original_line_completeness_unverified',match_count:{total_matches:null,lower_bound:1,complete:false,scope:'verified_lexical_candidates'}});page.coverage.line_completeness='includes_unverified_fragments';window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:structuredClone(window.fixture)}}));})()`);
            await until(`document.querySelector('[aria-label="references discovery details"]').textContent.includes('fragments whose completeness as original source lines is unverified')`);
            assert.equal(await evaluate(`document.querySelector('[aria-label="references discovery details"] .counts').textContent.includes('matching line fragments')`),true,'partial fragments use their qualified count unit');
            const unchangedNative=await evaluate(`JSON.stringify(window.fixture.session.turns[0].envelope)`);
            await evaluate(`[...document.querySelectorAll('.result header button')].find(button=>button.textContent==='markdown').click()`);
            await until(`document.querySelector('.result>pre')?.textContent.includes('"query_complete": false')`);
            assert.equal(await evaluate(`document.querySelector('.result>pre').textContent.includes('includes_unverified_fragments')`),true,'Markdown preserves fragment qualification and unknown whole-query coverage');
            assert.equal(await evaluate(`JSON.stringify(window.fixture.session.turns[0].envelope)`),unchangedNative,'changing output format cannot rewrite native coverage');
            await evaluate(`[...document.querySelectorAll('.result header button')].find(button=>button.textContent==='structured').click()`);
            checks.push({check:'candidate-scope exact zero/nonzero totals, unknown whole query, token boundary gaps and terminal fragment coverage',passed:true});

            await evaluate(`(()=>{const envelope=window.fixture.session.turns[0].envelope;envelope.steps=[{id:'legacy',tool:'references',status:'complete',result:{resolution:'textual',evidence:{hits:[],truncated:false}}}];envelope.evidence=[];window.fixture.cards={'turn-1':[]};window.fixture.continuations={'turn-1':[]};window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:structuredClone(window.fixture)}}));})()`);
            await until(`document.querySelector('[aria-label="references discovery details"]').textContent.includes('Textual candidate matches do not establish complete reference coverage; the whole-query total is unknown.')`);
            assert.equal(await evaluate(`document.querySelector('[aria-label="references discovery details"]').textContent.includes('identifier tokens split across indexed chunk boundaries')`),false,'legacy results cannot manufacture a typed coverage claim');
            assert.equal(await evaluate(`!!document.querySelector('[aria-label="references discovery details"] .counts')`),false,'legacy zero-hit result does not invent an exact or lower-bound count');
            assert.equal(await evaluate(`[...document.querySelectorAll('button')].some(button=>button.textContent.startsWith('Fetch next references page'))`),false,'legacy zero-hit result has no invented continuation');
            assert.equal(await evaluate(`Object.hasOwn(window.fixture.session.turns[0].envelope,'source_coverage')`),false,'display warning does not manufacture native coverage metadata');
            checks.push({check:'legacy textual zero-hit fallback warns whole-query unknown without invented typed metadata/count/continuation',passed:true});

            await evaluate(`document.querySelector('[aria-label="Add scope, recipe or tool"]').click()`);await until(`!!document.querySelector('.tool-form form')`);
            await evaluate(`(()=>{const select=document.querySelector('.tool-form [aria-label=\"Operation\"]');select.value='symbols';select.dispatchEvent(new Event('change',{bubbles:true}));})()`);
            await until(`!!document.querySelector('.tool-form [aria-label="Name matching"]')`);
            const setField=async(label,value)=>{
                await evaluate(`(()=>{const field=[...document.querySelectorAll('.tool-form input,.tool-form select')].find(item=>item.getAttribute('aria-label')===${JSON.stringify(label)});if(!field)throw new Error('Missing field');field.value=${JSON.stringify(value)};field.dispatchEvent(new Event(field.tagName==='SELECT'?'change':'input',{bubbles:true}));})()`);
                await delay(20);
            };
            await setField('name','Session');
            await evaluate(`document.querySelector('.tool-form form').requestSubmit()`);
            assert.deepEqual(await evaluate(`window.messages.filter(message=>message.type==='run').at(-1).plan.steps[0].args`),{name:'Session'},'unset optional fields do not inject defaults');
            await setField('Name matching','suffix');await setField('Show count precision','false');await setField('Summarize files','true');
            await setField('language','rust');await setField('Evidence type','code');await setField('Exclude path prefix','src/generated/');
            await setField('limit','12');await setField('cursor','a'.repeat(8192));
            await evaluate(`[...document.querySelectorAll('.tool-form button')].find(button=>button.textContent==='Run form').click()`);
            const request=await evaluate(`window.messages.filter(message=>message.type==='run').at(-1)`),args=request.plan.steps[0].args;
            assert.equal(request.prepare,false);assert.equal(request.plan.steps[0].tool,'symbols');
            assert.deepEqual({...args,cursor:undefined},{name:'Session',name_match:'suffix',include_count:false,include_files:true,language:'rust',evidence_kind:'code',exclude_path_prefix:'src/generated/',limit:12,cursor:undefined});
            assert.equal(args.cursor.length,8192,'opaque cursor is not limited to an arbitrary4096 characters');
            const before=await evaluate(`window.messages.filter(message=>message.type==='run').length`);
            await setField('limit','101');await evaluate(`[...document.querySelectorAll('.tool-form button')].find(button=>button.textContent==='Run form').click()`);
            assert.equal(await evaluate(`window.messages.filter(message=>message.type==='run').length`),before,'invalid numeric range never emits a native request');
            checks.push({check:'actual tool controls preserve boolean/enum/integer/long cursor types, omission and validity',passed:true});
            assert.equal(await evaluate(`document.documentElement.scrollWidth<=innerWidth`),true,'navigation/forms remain bounded horizontally');
            await evaluate(`document.dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true}))`);
            const png=await call('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});await fs.writeFile(path.join(output,'mcp-navigation.png'),Buffer.from(png.data,'base64'));
            await fs.writeFile(path.join(output,'mcp-navigation-summary.json'),JSON.stringify({basis:'Actual compiled Workbench browser bundle with original fictional host/data; no installed editor or native qualification',checks},null,2)+'\n');
            console.log('Workbench MCP navigation/forms browser checks passed (compiled fictional bridge, not installed qualification).');
            await call('Browser.close');return;
        }
        if(process.argv.includes('--selection')){
            await call('Emulation.setDeviceMetricsOverride',{width:480,height:800,deviceScaleFactor:1,mobile:false});
            const file=path.join(scratch,'selection.html');await fs.writeFile(file,html('results',true));
            await call('Page.navigate',{url:pathToFileURL(file).href});await until(`!!document.querySelector('.result')`);
            await evaluate(`window.fixture.cards['turn-1']=Array.from({length:25},(_,i)=>({...window.fixture.cards['turn-1'][0],id:'bulk-'+i,path:'src/row-'+i+'.rs',text:(i<10?'alpha':'beta')+' source line '+i}));window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:structuredClone(window.fixture)}}))`);
            const masterState=`Array.from(document.querySelectorAll('.select-all input')).map(input=>({label:input.parentElement.textContent,checked:input.checked,mixed:input.indeterminate,aria:input.getAttribute('aria-checked'),disabled:input.disabled}))`;
            const copiedIds=`window.messages.filter(message=>message.type==='copy'&&message.cards).at(-1)?.cards`;
            const copySelection=async(position=0)=>{await evaluate(`document.querySelectorAll('.selection-toolbar button')[${position}].click()`);};
            const setFilter=async value=>{await evaluate(`(()=>{document.querySelector('.result-tools').open=true;const input=document.querySelector('.filter input');input.value=${JSON.stringify(value)};input.dispatchEvent(new Event('input',{bubbles:true}));})()`);};
            await until(`document.querySelectorAll('.card').length===20&&document.querySelectorAll('.select-all input').length===2`);
            assert.deepEqual(await evaluate(masterState),Array.from({length:2},()=>({label:'Select all 25',checked:false,mixed:false,aria:'false',disabled:false})),'matching count appears at top and bottom before all cards are rendered');
            assert.equal(await evaluate(`document.querySelectorAll('.selection-toolbar').length===2&&Array.from(document.querySelectorAll('.selection-toolbar')).every(row=>row.querySelector('.select-all input')&&row.querySelectorAll('button').length===1&&row.querySelector('button').textContent==='Copy selected (0)')`),true,'both evidence-list rows pair select-all with one selected-copy action');
            assert.equal(await evaluate(`[...document.querySelectorAll('button')].some(button=>button.textContent==='Copy all')`),false,'no redundant Copy all action remains');
            assert.equal(await evaluate(`Array.from(document.querySelector('.result footer').querySelectorAll('button')).map(button=>button.textContent).join('|')`),'Save export…','structured evidence footer retains only Save export');
            assert.equal(await evaluate(`[...document.querySelectorAll('.selection-toolbar button')].find(button=>button.textContent==='Copy selected (0)').disabled`),true,'top Copy selected is visible but disabled without a selection');
            await copySelection();assert.equal(await evaluate(`window.messages.some(message=>message.type==='copy')`),false,'disabled empty selection sends no copy request');
            await evaluate(`document.querySelector('.transcript').scrollTop=170;window.position=document.querySelector('.transcript').scrollTop;document.querySelector('.select-all input').click()`);
            await until(`document.querySelector('.selection-toolbar')?.textContent.includes('Copy selected (25)')`);
            assert.equal(await evaluate(`document.querySelector('.transcript').scrollTop`),await evaluate('window.position'),'selecting all preserves manual transcript position');
            assert.equal(await evaluate(masterState+`.every(state=>state.checked&&!state.mixed)`),true,'top control selects both masters');
            assert.equal(await evaluate(`Array.from(document.querySelectorAll('.card-title input')).every(input=>input.checked)`),true,'all displayed cards selected');
            await copySelection();assert.deepEqual(await evaluate(copiedIds),Array.from({length:25},(_,i)=>'bulk-'+i),'bulk selection copies every matching result including five behind Show more');
            await copySelection(1);assert.deepEqual(await evaluate(copiedIds),Array.from({length:25},(_,i)=>'bulk-'+i),'bottom selected-copy action sends the same exact selected IDs');
            await evaluate(`[...document.querySelectorAll('button')].find(button=>button.textContent==='Show 20 more retained records').click()`);
            await until(`document.querySelectorAll('.card').length===25`);
            assert.equal(await evaluate(`Array.from(document.querySelectorAll('.card-title input')).every(input=>input.checked)`),true,'previously hidden cards retain bulk selection when shown');
            await evaluate(`document.querySelectorAll('.card-title input')[4].click()`);
            await until(`document.querySelector('.selection-toolbar')?.textContent.includes('Copy selected (24)')`);
            assert.equal(await evaluate(masterState+`.every(state=>!state.checked&&state.mixed&&state.aria==='mixed')`),true,'individual deselection sets both native controls to mixed');
            await evaluate(`document.querySelectorAll('.select-all input')[1].focus({preventScroll:true})`);
            for(const type of ['keyDown','keyUp'])await call('Input.dispatchKeyEvent',{type,key:' ',code:'Space',windowsVirtualKeyCode:32});
            await until(`document.querySelector('.selection-toolbar')?.textContent.includes('Copy selected (25)')`);
            assert.equal(await evaluate(masterState+`.every(state=>state.checked&&!state.mixed)`),true,'native Space on bottom mixed checkbox selects all and synchronizes top');
            await evaluate(`document.querySelector('.select-all input').focus({preventScroll:true})`);
            for(const type of ['keyDown','keyUp'])await call('Input.dispatchKeyEvent',{type,key:'Tab',code:'Tab',windowsVirtualKeyCode:9});
            assert.equal(await evaluate(`document.activeElement===document.querySelector('.selection-toolbar button')`),true,'native keyboard tab order reaches top Copy selected after master');
            for(const type of ['keyDown','keyUp'])await call('Input.dispatchKeyEvent',{type,key:'Tab',code:'Tab',windowsVirtualKeyCode:9});
            assert.equal(await evaluate(`document.activeElement===document.querySelector('.card-title input')`),true,'native keyboard tab order continues through Copy selected to first result checkbox');
            await evaluate(`document.querySelectorAll('.select-all input')[1].click()`);
            await until(`Array.from(document.querySelectorAll('.selection-toolbar button')).every(button=>button.textContent==='Copy selected (0)'&&button.disabled)`);
            assert.equal(await evaluate(masterState+`.every(state=>!state.checked&&!state.mixed)`),true,'bottom control clears all results and both masters');
            await evaluate(`document.querySelectorAll('.card-title input')[15].click();document.querySelector('.transcript').scrollTop=170;window.position=document.querySelector('.transcript').scrollTop`);
            await setFilter('alpha');await until(`document.querySelectorAll('.card').length===10`);
            assert.equal(await evaluate(`document.querySelector('.transcript').scrollTop`),await evaluate('window.position'),'filtering lower cards does not jump transcript to the query');
            assert.equal(await evaluate(masterState+`.every(state=>state.label==='Select all 10'&&!state.checked&&!state.mixed)`),true,'both controls count only current matching results');
            await evaluate(`document.querySelector('.select-all input').click()`);await until(`document.querySelector('.selection-toolbar')?.textContent.includes('Copy selected (11)')`);
            await copySelection();assert.deepEqual(await evaluate(copiedIds),['bulk-15',...Array.from({length:10},(_,i)=>'bulk-'+i)],'selecting matching results preserves a selection outside the filter');
            await evaluate(`document.querySelectorAll('.select-all input')[1].click()`);await until(`document.querySelector('.selection-toolbar')?.textContent.includes('Copy selected (1)')`);
            await copySelection(1);assert.deepEqual(await evaluate(copiedIds),['bulk-15'],'clearing matching results preserves selection outside the filter');
            await setFilter('no-matching-result');await until(`document.querySelectorAll('.card').length===0`);
            assert.equal(await evaluate(masterState+`.every(state=>state.label==='Select all 0'&&state.disabled&&!state.checked&&!state.mixed)`),true,'both zero-match controls are disabled');
            await setFilter('');await until(`document.querySelectorAll('.card').length===25`);
            assert.equal(await evaluate(`document.querySelectorAll('.card-title input')[15].checked`),true,'hidden selected result is restored when filter clears');
            assert.equal(await evaluate(masterState+`.every(state=>state.mixed&&!state.checked)`),true,'restored matching set updates both mixed states');
            assert.equal(await evaluate(`window.messages.some(message=>['run','continue'].includes(message.type))`),false,'selection/filter/show-more never fetch another native page');
            await setFilter('src/row-0.rs');await until(`document.querySelectorAll('.card').length===1`);
            await evaluate(`document.querySelector('.select-all input').click();document.querySelector('.transcript').scrollTop=0`);
            const png=await call('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});await fs.writeFile(path.join(output,'selection-top-bottom.png'),Buffer.from(png.data,'base64'));
            await call('Emulation.setDeviceMetricsOverride',{width:320,height:800,deviceScaleFactor:1,mobile:false});await delay(50);
            assert.equal(await evaluate(`(()=>{const row=document.querySelector('.selection-toolbar'),r=row.getBoundingClientRect();return document.documentElement.scrollWidth<=innerWidth&&[...row.children].every(child=>{const b=child.getBoundingClientRect();return b.left>=r.left&&b.right<=r.right&&b.top>=r.top&&b.bottom<=r.bottom;});})()`),true,'select-all and Copy selected fit one row at narrow width');
            await evaluate(`window.fixture.session.turns[0].id='limit';window.fixture.cards={limit:Array.from({length:1001},(_,i)=>({...window.fixture.cards['turn-1'][0],id:'limit-'+i,path:'src/limit.rs',text:'bounded selection fixture'}))};window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:structuredClone(window.fixture)}}))`);
            await until(`document.querySelector('.select-all')?.textContent==='Select all 1001'`);
            await evaluate(`document.querySelector('.select-all input').click()`);await until(`document.querySelector('.selection-toolbar')?.textContent.includes('Copy selected (1001)')`);
            assert.equal(await evaluate(masterState+`.every(state=>state.checked&&!state.mixed)`),true,'bulk selection preserves every result beyond host copy-selection count limit');
            assert.equal(await evaluate(`[...document.querySelectorAll('.selection-toolbar button,.result footer button')].filter(button=>button.textContent.startsWith('Copy selected')).every(button=>button.disabled)`),true,'both selected-copy actions reject an oversized ID count');
            assert.equal(await evaluate(`document.querySelector('.result [role=status]')?.textContent.includes('Select fewer results or use Save export')`),true,'oversized selection explains how to export without silently dropping results');
            await evaluate(`window.copyCount=window.messages.filter(message=>message.type==='copy').length`);await copySelection();assert.equal(await evaluate(`window.messages.filter(message=>message.type==='copy').length`),await evaluate('window.copyCount'),'oversized selection sends no rejected host request');
            await evaluate(`window.fixture.session.turns[0].id='message-limit';window.fixture.cards={'message-limit':Array.from({length:400},(_,i)=>({...window.fixture.cards.limit[0],id:String(i).padStart(4,'0')+'x'.repeat(124)}))};window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:structuredClone(window.fixture)}}))`);
            await until(`document.querySelector('.select-all')?.textContent==='Select all 400'`);
            await evaluate(`document.querySelector('.select-all input').click()`);await until(`document.querySelector('.selection-toolbar')?.textContent.includes('Copy selected (400)')`);
            assert.equal(await evaluate(`[...document.querySelectorAll('.selection-toolbar button')].find(button=>button.textContent.startsWith('Copy selected')).disabled`),true,'existing 40k message bound is checked even below the 1000-ID count limit');
            await evaluate(`[...document.querySelectorAll('.result header button')].find(button=>button.textContent==='json').click()`);await until(`!document.querySelector('.selection-toolbar')`);
            assert.equal(await evaluate(`[...document.querySelectorAll('.result footer button')].some(button=>button.textContent==='Copy JSON'&&!button.disabled)`),true,'raw JSON retains its full-output copy action');
            await evaluate(`[...document.querySelectorAll('.result footer button')].find(button=>button.textContent==='Copy JSON').click()`);
            assert.deepEqual(await evaluate(`window.messages.filter(message=>message.type==='copy').at(-1)`),{type:'copy',turn:'message-limit',format:'json'},'raw copy sends the whole-response JSON request without a selected-card list');
            await evaluate(`window.fixture.session.turns[0]={id:'status-only',at:Date.now(),input:'/status',format:'structured',envelope:{schema:'airk.workbench.result/1',status:'complete',steps:[],evidence:[]}};window.fixture.cards={'status-only':[]};window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:structuredClone(window.fixture)}}))`);
            await until(`document.querySelector('.turn .input')?.textContent==='/status'`);
            assert.equal(await evaluate(`!document.querySelector('.selection-toolbar')&&[...document.querySelectorAll('.result footer button')].some(button=>button.textContent==='Copy Markdown'&&!button.disabled)`),true,'status output without selectable evidence retains full-output Markdown copy');
            console.log('Workbench linked top/bottom select-all and selected-copy rows, mixed/keyboard/filter/pagination/copy-ID/bounds, raw-output copy and scroll checks passed (browser fixture, not installed editor).');
            await call('Browser.close');return;
        }
        if(process.argv.includes('--scroll')){
            await call('Emulation.setDeviceMetricsOverride',{width:480,height:800,deviceScaleFactor:1,mobile:false});
            const file=path.join(scratch,'scroll.html');await fs.writeFile(file,html('results',true));
            await call('Page.navigate',{url:pathToFileURL(file).href});await until(`!!document.querySelector('.result')`);
            await evaluate(`window.fixture.session.turns=Array.from({length:6},(_,i)=>({...window.fixture.session.turns[0],id:'turn-'+i}));window.fixture.cards=Object.fromEntries(window.fixture.session.turns.map(turn=>[turn.id,Array.from({length:8},(_,i)=>({...window.fixture.cards['turn-1'][0],id:'record-'+i,text:'source line\\n'.repeat(30)}))]));window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:structuredClone(window.fixture)}}))`);
            await until(`document.querySelectorAll('.turn').length===6`);
            await evaluate(`window.fixture.session.turns.push({id:'pending',at:Date.now(),input:'/symbols QueueClient',format:'structured'});window.fixture.busy=true;window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:structuredClone(window.fixture)}}))`);
            await until(`document.querySelector('.turn:last-child [role=status]')?.textContent.includes('Collecting')`);
            assert.equal(await evaluate(`(()=>{const t=document.querySelector('.transcript'),r=t.lastElementChild.getBoundingClientRect();return r.top>=t.getBoundingClientRect().top&&r.bottom<=t.getBoundingClientRect().bottom;})()`),true,'pending request visible');
            await evaluate(`window.fixture.session.turns.at(-1).envelope=window.fixture.session.turns[0].envelope;window.fixture.cards.pending=window.fixture.cards['turn-0'];window.fixture.busy=false;window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:structuredClone(window.fixture)}}))`);
            await until(`!!document.querySelector('.turn:last-child .result')`);
            assert.equal(await evaluate(`(()=>{const t=document.querySelector('.transcript'),turn=t.lastElementChild,r=turn.querySelector('.input').getBoundingClientRect(),bounds=t.getBoundingClientRect();return Math.abs(r.top-bounds.top)<2&&r.bottom<=bounds.bottom&&turn.querySelector('time').getBoundingClientRect().top>=r.bottom&&turn.querySelector('details').getBoundingClientRect().top>=r.bottom&&turn.querySelector('.card').getBoundingClientRect().top<bounds.bottom;})()`),true,'completed response keeps query visible above timestamp, details and first result');
            const queryPng=await call('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});await fs.writeFile(path.join(output,'scroll-query-first.png'),Buffer.from(queryPng.data,'base64'));
            await evaluate(`document.querySelector('.transcript').scrollTop-=200;document.querySelector('.turn:last-child .source-details').open=true;const pre=document.querySelector('.turn:last-child .card pre');pre.scrollTop=60;window.position=document.querySelector('.transcript').scrollTop;window.nestedPosition=pre.scrollTop;document.querySelector('#root').style.display='none';window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:structuredClone(window.fixture)}}));`);
            assert.ok(await evaluate('window.position')>0,'fixture has a nonzero transcript position');
            assert.ok(await evaluate('window.nestedPosition')>0,'fixture has a nonzero nested result position');
            await delay(100);
            await evaluate(`document.querySelector('#root').style.display='';window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:structuredClone(window.fixture)}}));window.dispatchEvent(new MessageEvent('message',{data:{type:'status',sessionId:window.fixture.session.id,project:window.fixture.project,status:{health:'ok'}}}));`);
            await delay(100);
            assert.equal(await evaluate(`document.querySelector('.transcript').scrollTop`),await evaluate('window.position'),'tab return and status preserve exact transcript position');
            assert.equal(await evaluate(`document.querySelector('.turn:last-child .card pre').scrollTop`),await evaluate('window.nestedPosition'),'nested scroll preserved');
            assert.equal(await evaluate(`document.querySelector('.turn:last-child .source-details').open`),true,'expanded source preserved');
            await evaluate(`window.dispatchEvent(new MessageEvent('message',{data:{type:'saved',sessionId:window.fixture.session.id,project:window.fixture.project,revision:1,recent:[],save:'Auto-saved'}}));`);
            await delay(100);
            assert.equal(await evaluate(`document.querySelector('.transcript').scrollTop`),await evaluate('window.position'),'save completion preserves position');
            await evaluate(`window.fixture.session.turns.push({id:'long-query',at:Date.now(),input:Array.from({length:80},(_,i)=>'Long query line '+i).join('\\n'),format:'structured'});window.fixture.busy=true;window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:structuredClone(window.fixture)}}))`);
            await until(`document.querySelector('.turn:last-child .input')?.textContent.includes('Long query line 79')`);
            assert.equal(await evaluate(`(()=>{const t=document.querySelector('.transcript'),q=t.lastElementChild.querySelector('.input').getBoundingClientRect();return q.height>t.clientHeight&&Math.abs(q.top-t.getBoundingClientRect().top)<2;})()`),true,'oversized pending query reveals its start');
            await evaluate(`window.fixture.session.turns.at(-1).envelope=window.fixture.session.turns[0].envelope;window.fixture.cards['long-query']=window.fixture.cards['turn-0'];window.fixture.busy=false;window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:structuredClone(window.fixture)}}))`);
            await until(`!!document.querySelector('.turn:last-child .result')`);
            assert.equal(await evaluate(`(()=>{const t=document.querySelector('.transcript'),turn=t.lastElementChild,q=turn.querySelector('.input').getBoundingClientRect();return Math.abs(q.top-t.getBoundingClientRect().top)<2&&turn.querySelector('.result').getBoundingClientRect().top>t.getBoundingClientRect().bottom;})()`),true,'oversized completed query retains its start rather than jumping below it to results');
            const tallPng=await call('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});await fs.writeFile(path.join(output,'scroll-tall-query.png'),Buffer.from(tallPng.data,'base64'));
            await evaluate(`window.fixture.session.turns.push({id:'failed-query',at:Date.now(),input:'Find the symbol PacketClient',format:'structured'});window.fixture.busy=true;window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:structuredClone(window.fixture)}}))`);
            await until(`document.querySelector('.turn:last-child [role=status]')?.textContent.includes('Collecting')`);
            await evaluate(`window.fixture.session.turns.at(-1).error='Fictional failed read';window.fixture.busy=false;window.dispatchEvent(new MessageEvent('message',{data:{type:'state',state:structuredClone(window.fixture)}}))`);
            await until(`document.querySelector('.turn:last-child .error')?.textContent.includes('Fictional failed read')`);
            assert.equal(await evaluate(`(()=>{const t=document.querySelector('.transcript'),q=t.lastElementChild.querySelector('.input').getBoundingClientRect(),bounds=t.getBoundingClientRect();return q.top>=bounds.top-2&&q.bottom<=bounds.bottom;})()`),true,'failed response keeps its query visible');
            console.log('Workbench query-first pending/completion reveal and retained-view scroll checks passed (browser fixture, not installed editor).');
            await call('Browser.close');return;
        }
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
