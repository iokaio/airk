// SPDX-License-Identifier: Apache-2.0
'use strict';
/** Native handler/transport qualification. VS Code hosting is stubbed; no renderer
 * or native response is mocked. This is NOT an installed-editor qualification.
 * The foreground service fixture uses an explicit real-process opener because
 * its control directory is not a managed runtime installation. */
const fs=require('node:fs/promises'),path=require('node:path'),Module=require('node:module');
const assert=require('node:assert/strict'),{createHash}=require('node:crypto'),{spawn}=require('node:child_process');
const repo=path.resolve(__dirname,'../..');
const hash=bytes=>createHash('sha256').update(bytes).digest('hex');
const clone=value=>structuredClone(value);
const arg=name=>{const i=process.argv.indexOf(name);assert.ok(i>=0&&process.argv[i+1],`Missing ${name}`);return path.resolve(process.argv[i+1]);};
const tick=()=>new Promise(resolve=>setTimeout(resolve,10));
const started=Date.now(),workDeadline=started+90000;
const remaining=()=>Math.max(0,workDeadline-Date.now());
function requireBudget(){assert.ok(remaining()>0,'Global native Workbench work deadline exceeded');}
async function until(check,label){while(!check()){requireBudget();await tick();}}
const BASE_POINTERS=['/cache_age_ms','/cached','/discovery_observation/observed_unix_ms','/discovery_readiness/observed_unix_ms','/freshness/observed_unix_ms','/indexing/observed_unix_ms','/observed_unix_ms','/request_id','/semantic_readiness/observed_unix_ms','/status_observation/cache_age_ms','/status_observation/cached','/status_observation/observed_unix_ms'];
const WORKBENCH_POINTERS=[...BASE_POINTERS,'/steps/0/elapsed_ms',...BASE_POINTERS.map(pointer=>'/steps/0/result'+pointer)];
const allowedPointers=new Set(WORKBENCH_POINTERS);
class Emitter{listeners=new Set();event=listener=>{this.listeners.add(listener);return {dispose:()=>this.listeners.delete(listener)};};fire(value){for(const listener of this.listeners)listener(value);}dispose(){this.listeners.clear();}}
/** Exact JSON pointers only, never a recursive field-name blacklist. */
function normalized(input,pointers){const value=clone(input);for(const pointer of pointers??[]){
    assert.ok(allowedPointers.has(pointer),'Normalization must use the shared exact Python pointer allowlist');
    const keys=pointer.slice(1).split('/').map(key=>key.replaceAll('~1','/').replaceAll('~0','~'));
    const last=keys.at(-1);
    let cursor=value;for(const key of keys.slice(0,-1)){cursor=cursor?.[key];if(cursor===undefined)break;}
    if(cursor&&typeof cursor==='object')delete cursor[last];
}return value;}
function markdownObjects(text){
    const lines=text.split('\n'),objects=[];
    for(let i=0;i<lines.length;i++)if(/^`{3,}$/.test(lines[i])){
        const fence=lines[i],body=[];while(++i<lines.length&&lines[i]!==fence)body.push(lines[i]);
        try{objects.push(JSON.parse(body.join('\n')));}catch{/* Evidence text can be non-JSON. */}
    }
    return objects;
}
function assertMarkdownObject(text,expected,label){assert.ok(markdownObjects(text).some(value=>{
    try{assert.deepEqual(value,expected);return true;}catch{return false;}
}),label);}
async function main(){
    const casesPath=arg('--cases'),reportPath=arg('--report'),input=await fs.readFile(casesPath),manifest=JSON.parse(input);
    assert.equal(manifest.schema,1);assert.equal(manifest.fixture_kind,'compiled_schema12_authority1_handler_transport_only');
    assert.ok(Array.isArray(manifest.cases)&&manifest.cases.length>0&&manifest.cases.length<=8);
    for(const key of ['control','runtime','source_root'])assert.ok(path.isAbsolute(manifest[key]),`Absolute ${key} required`);
    assert.ok(!manifest.cases.some(item=>Object.keys(item).some(key=>/bearer|password|secret/i.test(key))),'Manifest must not contain credentials');
    const out=path.join(repo,'packages/vscode/out'),load=name=>require(path.join(out,name));
    const modules=['workbenchView','workbenchContracts','workbenchRender','workbenchSessions','explorerEvidence','scopeSelection','model','mcpStdioSession'];
    const sourceHashes={},compiledHashes={};
    for(const name of modules){sourceHashes[name]=hash(await fs.readFile(path.join(repo,'packages/vscode/src',name+'.ts')));compiledHashes[name]=hash(await fs.readFile(path.join(out,name+'.js')));}
    const directory=await fs.mkdtemp(path.join(path.dirname(reportPath),'workbench-native-'));
    const messages=[],clipboard=[],exports=[],changes=new Emitter(),config=new Emitter();
    let receiver,specs=[],state={projects:[],recent:[],cards:{},busy:false,save:'',persistence:true},saveOrdinal=0;
    const stub={EventEmitter:Emitter,Uri:{file:fsPath=>({fsPath}),joinPath:(base,...parts)=>({fsPath:path.join(base.fsPath,...parts)})},
        env:{clipboard:{writeText:async text=>{clipboard.push(text);}}},
        workspace:{getConfiguration:()=>({get:(_key,fallback)=>fallback,inspect:()=>({globalValue:true})}),onDidChangeConfiguration:config.event,
            fs:{writeFile:async(uri,bytes)=>{await fs.writeFile(uri.fsPath,bytes,{flag:'wx'});exports.push(Buffer.from(bytes).toString('utf8'));}}},
        window:{showSaveDialog:async()=>({fsPath:path.join(directory,`export-${++saveOrdinal}.txt`)}),showWarningMessage:async()=>undefined},
        commands:{executeCommand:async()=>undefined}};
    const original=Module._load;
    const nativeCounts={connections:0,calls:0},children=[];
    const cleanup={schema:1,owned_children:[],all_exited:false};
    function launchOwned(command,args,options){
        requireBudget();assert.ok(children.length<16,'Native process connection budget exceeded');
        const child=spawn(command,args,{...options,windowsHide:true,stdio:'pipe'});
        const record={child,pid:child.pid??null,closed:false,exit_code:null,signal:null,spawn_error:undefined};
        record.done=new Promise(resolve=>{
            child.once('error',error=>{record.spawn_error=error.code??error.name;});
            child.once('close',(code,signal)=>{record.closed=true;record.exit_code=code;record.signal=signal;resolve();});
        });children.push(record);return child;
    }
    async function drainOwned(){
        const wait=async ms=>new Promise(resolve=>{const timer=setTimeout(resolve,ms);void Promise.all(children.map(record=>record.done)).then(()=>{clearTimeout(timer);resolve();});});
        await wait(3000);
        for(const record of children)if(!record.closed){record.forced=true;record.child.kill('SIGKILL');}
        await wait(5000);
        cleanup.owned_children=children.map(({pid,closed,exit_code,signal,spawn_error,forced})=>({pid,closed,exit_code,signal,spawn_error,forced:forced===true}));
        cleanup.all_exited=children.every(record=>record.closed);cleanup.elapsed_ms=Date.now()-started;
    }
    let WorkbenchView,workbenchProjectKey,applyWorkbenchUpdate;
    Module._load=function(name,...args){
        if(name==='vscode')return stub;
        const actual=original.call(this,name,...args);
        if(name==='./explorerEvidence'){
            // Inject ONLY the process opener; retain actual production queue,
            // parsing, retries, error capture, cancellation and Workbench handlers.
            class ForegroundEvidenceClient extends actual.EvidenceClient{
                constructor(spec,version,options){super(spec,version,{...options,open:async(launch,timeout)=>{
                    requireBudget();assert.ok(nativeCounts.connections<16,'Native connection budget exceeded');
                    const {McpStdioSession}=load('mcpStdioSession');nativeCounts.connections++;
                    const opened=await McpStdioSession.open({...launch,command:manifest.binary??spec.command,cwd:manifest.source_root,
                        args:['--service-runtime-dir',manifest.control,'--catalog-session',JSON.stringify(spec.catalogSession),'mcp'],launch:launchOwned},Math.min(timeout,remaining()));
                    const request=opened.session.request.bind(opened.session);
                    opened.session.request=(method,params,deadline)=>{requireBudget();if(method==='tools/call'){assert.ok(nativeCounts.calls<64,'Native tools/call budget exceeded');nativeCounts.calls++;}return request(method,params,Math.min(deadline,remaining()));};
                    return opened;
                }});}
            }
            return {...actual,EvidenceClient:ForegroundEvidenceClient};
        }
        return actual;
    };
    try{({WorkbenchView,workbenchProjectKey}=load('workbenchView'));({applyWorkbenchUpdate}=load('workbenchContracts'));}finally{Module._load=original;}
    const host={trusted:()=>true,specs:()=>specs,activeProject:()=>undefined,onDidChangeSpecs:changes.event,version:'1.8.8'};
    const provider=new WorkbenchView({globalStorageUri:{fsPath:directory},extensionUri:{fsPath:path.join(repo,'packages/vscode')}},host);
    const disposed=new Emitter(),visibility=new Emitter();
    provider.resolveWebviewView({visible:true,onDidDispose:disposed.event,onDidChangeVisibility:visibility.event,webview:{options:{},html:'',cspSource:'fixture',
        asWebviewUri:uri=>uri.fsPath,onDidReceiveMessage:callback=>{receiver=callback;return {dispose(){receiver=undefined;}};},
        postMessage:async message=>{const captured=clone(message);messages.push(captured);state=applyWorkbenchUpdate(state,captured);return true;}}});
    const send=value=>{assert.ok(receiver,'Actual Workbench receive handler is installed');receiver(clone(value));};
    async function copy(turn,format,cards){const before=clipboard.length;send({type:'copy',turn,format,...(cards?{cards}:{})});await until(()=>clipboard.length>before,'actual clipboard handler');return clipboard.at(-1);}
    async function save(turn,format,cards){const before=exports.length;send({type:'export',turn,format,...(cards?{cards}:{})});await until(()=>exports.length>before,'actual save/export handler');return exports.at(-1);}
    const results=[],retained=[],preservation=new Map();let currentCase,currentResult;
    const execute=async()=>{
        for(const item of manifest.cases){
            requireBudget();currentCase=item.id;currentResult=undefined;
            assert.ok(Number(item.workbench_request?.plan?.budgets?.max_bytes)<=65536,'Fixture native response byte budget exceeds 65536');
            assert.ok(item.workbench_request&&item.scopeSpec?.catalogSession,'Canonical parse request and production selector required');
            const spec=clone(item.scopeSpec),key=workbenchProjectKey(spec);
            assert.ok(key&&key!==spec.service.project,'Scoped session history must differ from native backing binding');
            specs=[spec];changes.fire();await provider.useScope(spec);
            await until(()=>state?.session?.project===key&&!state.busy,'selected native scope');
            for(const prior of retained){assert.deepEqual(prior.turn.envelope,prior.envelope,'Scope change cannot relabel old evidence');assert.deepEqual(prior.turn.nativeFailure,prior.nativeFailure);}
            const before=state.session.turns.length;
            const request=item.workbench_request;
            assert.equal(request.schema,'airk.parse.request/1');assert.equal(request.mode,'execute');assert.ok(request.plan);
            send({type:'run',input:item.id,prepare:false,plan:request.plan,...(request.path_prefix?{pathPrefix:request.path_prefix}:{})});
            await until(()=>state?.session?.turns.length>before&&!state.busy,'actual native parse completion');
            const turn=state.session.turns.at(-1),session=state.session;
            assert.deepEqual(turn.request,request,'Workbench accepted exact supported canonical parse inputs');
            const excluded=item.normalization?.excluded_paths??manifest.normalization?.excluded_paths??[];
            if(item.workbench_expected_native_failure){
                assert.equal(turn.envelope,undefined);assert.deepEqual(normalized(turn.nativeFailure,excluded),normalized(item.workbench_expected_native_failure,excluded));
            }else{
                assert.ok(turn.envelope,`Actual native Workbench response missing for ${item.id}: ${turn.error??'unknown'}`);
                assert.ok(item.workbench_expected,'Full outer native parse baseline required');
                assert.deepEqual(normalized(turn.envelope,excluded),normalized(item.workbench_expected,excluded));
                if(item.expected!==undefined){const steps=turn.envelope.steps;assert.ok(Array.isArray(steps)&&steps.length===1);assert.deepEqual(normalized(steps[0].result,item.direct_normalization?.excluded_paths??[]),normalized(item.expected,item.direct_normalization?.excluded_paths??[]));}
            }
            const result=currentResult={id:item.id,native_body:turn.envelope,native_failure:turn.nativeFailure,actual_request:turn.request,comparisons:{full_native_response:true,full_native_request:true}};
            result.json_copy=await copy(turn.id,'json');result.markdown_copy=await copy(turn.id,'markdown');
            result.json_export=await save(turn.id,'json');result.markdown_export=await save(turn.id,'markdown');result.session_json_copy=await copy(undefined,'json');
            for(const key of ['json_copy','json_export']){const captured=JSON.parse(result[key]).turns[0];assert.deepEqual(captured.envelope,turn.envelope);assert.deepEqual(captured.nativeFailure,turn.nativeFailure);}
            assert.equal(result.json_copy,result.json_export);assert.equal(result.markdown_copy,result.markdown_export);
            if(turn.nativeFailure)assertMarkdownObject(result.markdown_copy,turn.nativeFailure,'Markdown preserves full original native refusal');
            if(turn.envelope)assertMarkdownObject(result.markdown_copy,turn.envelope,'Markdown preserves full native parse envelope');
            const cards=state.cards?.[turn.id]??[];
            if(cards.length){
                const selected=[cards[0].id];result.selected_json_copy=await copy(turn.id,'json',selected);result.selected_markdown_copy=await copy(turn.id,'markdown',selected);
                result.selected_json_export=await save(turn.id,'json',selected);result.selected_markdown_export=await save(turn.id,'markdown',selected);
                assert.equal(result.selected_json_copy,result.selected_json_export);assert.equal(result.selected_markdown_copy,result.selected_markdown_export);
                const selectedBody=JSON.parse(result.selected_json_copy).turns[0].envelope;
                assert.equal(selectedBody.selection,true);assert.deepEqual(selectedBody.evidence,[turn.envelope.evidence[Number(cards[0].id.slice('record-'.length))]]);
                for(const key of Object.keys(turn.envelope)){if(!['steps','evidence','source_verification'].includes(key))assert.deepEqual(selectedBody[key],turn.envelope[key],`Selected copy preserves complete ${key}`);}
                assertMarkdownObject(result.selected_markdown_copy,selectedBody,'Selected Markdown preserves the complete canonical selected response');
                const selectedObjects=markdownObjects(result.selected_markdown_copy);
                assert.ok(selectedObjects.some(value=>{try{assert.deepEqual(value.evidence,selectedBody.evidence);return true;}catch{return false;}}),'Selected Markdown retains full canonical evidence rows and citations');
                if(selectedBody.scope_observation!==undefined)assertMarkdownObject(result.selected_markdown_copy,selectedBody.scope_observation,'Selected Markdown retains complete ScopeObservation');
                if(selectedBody.source_coverage!==undefined)assertMarkdownObject(result.selected_markdown_copy,selectedBody.source_coverage,'Selected Markdown retains full applicable source coverage');
                const sameStep=cards.filter(card=>card.step===cards[0].step).map(card=>card.id);
                result.step_selected_json_copy=await copy(turn.id,'json',sameStep);result.comparisons.selected_handlers=true;
                result.step_selection_kind='existing card-selection handler; no dedicated step-copy action exists';
            }else{result.comparisons.selected_handlers='not_applicable_no_evidence_cards';}
            const hostTurn=provider.session.turns.find(value=>value.id===turn.id);
            assert.ok(hostTurn,'Observe actual host-owned turn without injecting or replacing state');
            retained.push({turn:hostTurn,envelope:clone(turn.envelope),nativeFailure:clone(turn.nativeFailure)});
            await provider.saveQueue;
            const savedPath=path.join(directory,'workbench',session.project,session.id+'.json');
            const savedBytes=await fs.readFile(savedPath);preservation.set(savedPath,{id:item.id,path:savedPath,sha256:hash(savedBytes)});
            results.push(result);
        }
        for(const saved of preservation.values())assert.equal(hash(await fs.readFile(saved.path)),saved.sha256,'Changing active scope must not delete or mutate saved old histories');
        for(const name of modules){assert.equal(hash(await fs.readFile(path.join(repo,'packages/vscode/src',name+'.ts'))),sourceHashes[name]);assert.equal(hash(await fs.readFile(path.join(out,name+'.js'))),compiledHashes[name]);}
        return {schema:1,status:'passed',fixture_kind:manifest.fixture_kind,qualification:{vscode_host:'stub',native_handlers:true,installed_editor:false,managed_launcher:false},input_sha256:hash(input),runtime_manifest_sha256:manifest.runtime_manifest_sha256,source_hashes:sourceHashes,compiled_hashes:compiledHashes,node:process.version,native_counts:nativeCounts,cases:results,history_preservation:[...preservation.values()],
            limitations:['Foreground fixture injects only actual MCP process opener; managed runtime launch remains separate.','Actual installed VS Code UI has not been exercised.','Source and compiled hashes identify bytes; matching compilation provenance requires the lead build receipt.','Native revocation and deterministic admitted late-response boundaries require separate owner-coordinated cases; no synthetic response is used.']};
    };
    let report,failure,timer,wroteFailure=false;
    const persist=async(flag)=>{
        const text=JSON.stringify(report,null,2)+'\n';
        assert.ok(Buffer.byteLength(text)<=64*1024*1024,'Native parity report exceeds bounded 64MiB budget');
        await fs.writeFile(reportPath,text,{flag});
    };
    try{
        report=await Promise.race([execute(),new Promise((_resolve,reject)=>{timer=setTimeout(()=>reject(new Error('Global 90s Workbench work deadline exceeded')),remaining());})]);
    }catch(error){
        failure=error;
        report={schema:1,status:'failed',fixture_kind:manifest.fixture_kind,input_sha256:hash(input),runtime_manifest_sha256:manifest.runtime_manifest_sha256,
            source_hashes:sourceHashes,compiled_hashes:compiledHashes,native_counts:nativeCounts,cases:results,current_case:currentCase,
            failure:{name:error.name,message:error.message,stack:error.stack},partial_case:currentResult,
            current_native_turn:clone(provider.session?.turns.at(-1)),history_preservation:[...preservation.values()],cleanup:{pending:true},
            qualification:{vscode_host:'stub',native_handlers:true,installed_editor:false,managed_launcher:false}};
        // Preserve observed complete native evidence BEFORE disposal/teardown.
        await persist('wx');wroteFailure=true;
    }finally{
        clearTimeout(timer);provider.dispose();await drainOwned();
        if(!cleanup.all_exited){failure??=new Error('Owned native MCP children did not exit within the cleanup budget');report.status='failed';report.cleanup_failure=failure.message;}
        report.cleanup=cleanup;await persist(wroteFailure?'w':'wx');
    }
    process.stdout.write(JSON.stringify({schema:1,status:report.status,report:reportPath,cases:results.length,native_counts:nativeCounts,cleanup_all_exited:cleanup.all_exited})+'\n');
    if(failure)throw failure;
}
main().catch(error=>{process.stderr.write(`Workbench native parity failed; full evidence is in --report: ${error.name}\n`);process.exitCode=1;});
