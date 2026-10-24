// SPDX-License-Identifier: Apache-2.0
import { randomBytes } from 'node:crypto';
import { ConsolePage, RecordValue, RepairOperation, identifier, record } from './serviceClient';
export interface ConsoleObservation { state: 'live'|'cached'|'failed'|'unavailable'; value?:RecordValue; code?:string; checkedAt:number }
export interface ServiceConsoleState {
    revision:string; trusted:boolean; busy:boolean; authority:string; version:string; connected:boolean;
    lifecycle?:'on-demand'|'external'|'user-on-demand';
    responseMode?:'terse'|'full';
    health:ConsoleObservation; resources:ConsoleObservation; model:ConsoleObservation; sources:ConsoleObservation;
    bootstrap:ConsoleObservation; page?:ConsolePage; workers?:RecordValue; events:RecordValue[]; historyOmitted:boolean;
    serviceEvents?:ConsoleObservation;
    selected:Set<string>; plan?:{value:RecordValue;items:RecordValue[];after:string|null};
    pending:{id:string;title:string;unknown:boolean;lifecycle?:boolean;forceAvailable?:boolean}[]; preview?:string; includePaths:boolean; message?:string;
}
export type ConsoleAction = {type:'service';revision:string;action:string;project?:string;id?:string;selected?:boolean;scope?:'one'|'selected'|'all';operation?:RepairOperation;includePaths?:boolean;allowFallback?:boolean};
const simple=['refresh','connect','start','stop','restart','logs','resources','responseMode','recoverLifecycle','abandonLifecycle','startEmbedding','stopEmbedding','forceEmbedding','firstPage','nextPage','morePlan','refreshPlan','pausePlan','resumePlan','cancelPlan','approvePlan','previewReport','copyReport'];
const projects=['inspect','index','pause','resume','restartProject','watch','modelSetup'];
const operations=['auto','verify','recover','reconcile','vectors','rebuild'];
/** Messages select fixed host-owned operations; no paths, commands, tokens or RPC bodies are accepted. */
export function consoleAction(value:unknown,current:ServiceConsoleState):ConsoleAction|undefined {
    try {
        const message=record(value);
        if(message.type!=='service'||message.revision!==current.revision||typeof message.action!=='string')return;
        const keys=new Set(['type','revision','action']);const action=message.action;
        if(simple.includes(action)){
            if(action==='previewReport'){keys.add('includePaths');if(typeof message.includePaths!=='boolean')return;}
            if(action==='approvePlan'){keys.add('allowFallback');if(typeof message.allowFallback!=='boolean')return;}
        }else if(projects.includes(action)||action==='select'){
            keys.add('project');const project=identifier(message.project,32);
            if(!current.page?.projects.some(row=>record(record(row.project).registration).binding===project))return;
            if(action==='watch'||action==='select'){keys.add('selected');if(typeof message.selected!=='boolean')return;}
        }else if(action==='repair'){
            keys.add('scope');keys.add('operation');if(!['one','selected','all'].includes(String(message.scope))||!operations.includes(String(message.operation)))return;
            if(message.scope==='one'){keys.add('project');const project=identifier(message.project,32);
                if(!current.page?.projects.some(row=>record(record(row.project).registration).binding===project))return;}
        }else if(['retryJob','cancelJob'].includes(action)){
            keys.add('id');const id=identifier(message.id,32);
            const jobs=new Set(current.events.map(event=>event.job));
            for(const row of current.page?.projects??[]){const progress=row.progress?record(row.progress):undefined;if(progress?.job)jobs.add(record(progress.job).id);}
            if(!jobs.has(id))return;
        }else if(action==='retryIntent'||action==='forgetIntent'||action==='forceIntent'){
            keys.add('id');const id=identifier(message.id,16);if(!current.pending.some(intent=>intent.id===id))return;
            if(action==='forceIntent'&&!current.pending.some(intent=>intent.id===id&&intent.lifecycle&&intent.forceAvailable))return;
        }else return;
        if(Object.keys(message).some(key=>!keys.has(key))||Object.keys(message).length!==keys.size)return;
        return message as unknown as ConsoleAction;
    }catch{return;}
}
const scalar=(value:unknown):string=>value==null?'Not observed':typeof value==='boolean'?value?'Yes':'No'
    :typeof value==='number'?Number.isSafeInteger(value)&&value>=0?value.toLocaleString():'Unknown'
    :typeof value==='string'?value.slice(0,4096):'See details';
const e=(value:unknown):string=>scalar(value).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'})[c]!);
const obj=(value:unknown):RecordValue=>value&&typeof value==='object'&&!Array.isArray(value)?value as RecordValue:{};
const facts=(values:[string,unknown][])=>`<dl>${values.map(([name,value])=>`<dt>${e(name)}</dt><dd>${e(value)}</dd>`).join('')}</dl>`;
function button(action:string,label:string,extra='',disabled=false):string{return `<button type="button" data-action="${action}" ${extra}${disabled?' disabled':''}>${e(label)}</button>`;}
function observation(name:string,value:ConsoleObservation):string {
    return `<p><strong>${e(name)}</strong>: ${e(value.state)}${value.code?` · ${e(value.code)}`:''} · checked ${e(new Date(value.checkedAt).toISOString())}</p>`;
}
function projectRow(row:RecordValue,state:ServiceConsoleState):string {
    const project=obj(row.project),registration=obj(project.registration),binding=String(registration.binding);
    const inspection=obj(row.inspection),snapshot=obj(inspection.snapshot),progress=obj(row.progress),work=obj(progress.progress),job=obj(progress.job),watch=obj(row.watch);
    const workers=Array.isArray(state.workers?.observations)?state.workers!.observations:[];
    const worker=obj(workers.find(value=>obj(value).project===binding)),identity=obj(worker.identity);
    const sampled=obj(progress.worker);
    const current=state.connected&&typeof sampled.worker==='string'&&typeof sampled.service==='string'&&sampled.worker===identity.worker&&sampled.service===identity.service
        &&sampled.project===identity.project
        &&sampled.fencing_generation===identity.fencing_generation&&sampled.fencing_generation===progress.current_fencing_generation;
    const semantic=obj(snapshot.semantic),discovery=obj(snapshot.discovery);
    const attr=`data-project="${e(binding)}"`;
    const unavailable=state.busy||!state.connected||!state.trusted;
    return `<article><h3><label><input type="checkbox" data-action="select" ${attr}${state.selected.has(binding)?' checked':''}${unavailable?' disabled':''}> ${e(registration.collection)}</label></h3>
        ${facts([['Source',registration.root],['Index data',registration.data_root],['Project identity',binding],
            ['Worker',identity.worker??(worker.restarting?'Restart queued':worker.identity===null?'No resident worker observed':'Worker identity not observed')],['Worker fence',identity.fencing_generation],
            ['Worker age at sample (ms)',state.workers?.age_ms],['Worker observation',state.connected?'Connected sample':'Cached sample'],['Startup circuit blocked',worker.automatic_start_blocked],
            ['Paused',project.paused],['Source watcher',watch.enabled],['Source observation',watch.failure??watch.backend],
            ['Pending source changes',typeof watch.requested_revision==='number'&&typeof watch.completed_revision==='number'?Math.max(0,watch.requested_revision-watch.completed_revision):undefined],
            ['Last inspected revision',snapshot.publication],['Inspection time (cached)',inspection.observed_unix_ms],['Source availability (cached)',snapshot.source_state],
            ['Discovery (cached)',discovery.state],['Indexed files (cached)',snapshot.indexed_files],['Semantic coverage (cached)',semantic.state],
            ['Completed semantic artifacts',semantic.completed_artifacts],['Selected semantic artifacts',semantic.selected_artifacts],
            ['Work observation',current?'Observed worker generation':progress.worker?'Cached worker sample':'Not observed'],['Phase',work.phase],
            ['Captured files',work.captured_files],['Extracted files',work.extracted_files],['Prepared changes',work.prepared_changes],['Committed changes',work.committed_changes],
            ['Published batches',work.publications],['Total files',work.total_files],['Last real work advance age at sample (ms)',work.last_advance_age_ms],
            ['Unpaused time without progress (ms)',work.unpaused_idle_ms],['Phase elapsed (ms)',work.phase_elapsed_ms],['Job state',job.state],
            ['Last job failure code',obj(job.outcome).code],['Inspection failure (cached)',snapshot.failure],
            ['Last observed publication',work.publication],['Unavailable cached fields',Array.isArray(row.failed_observations)?row.failed_observations.join(', '):undefined]])}
        <div class="controls">${button('inspect','Inspect',attr,unavailable)}${button('index','Index now',attr,unavailable)}
        ${button(project.paused?'resume':'pause',project.paused?'Resume project':'Pause project',attr,unavailable)}
        ${button('restartProject','Restart project worker',attr,unavailable)}${button('modelSetup','Install bundled model',attr,unavailable)}
        ${button('repair','Diagnose & repair',`${attr} data-scope="one" data-operation="auto"`,unavailable)}</div>
        <label><input type="checkbox" data-action="watch" ${attr}${watch.enabled?' checked':''}${unavailable?' disabled':''}> Continue automatic source watching when workspaces are closed</label>
        ${job.state==='failed'||job.state==='canceled'?button('retryJob','Retry job',`data-id="${e(job.id)}"`,unavailable):''}
        ${job.state==='running'||job.state==='pending'?button('cancelJob','Cancel job',`data-id="${e(job.id)}"`,unavailable):''}
    </article>`;
}
function planView(state:ServiceConsoleState):string {
    if(!state.plan)return '<p>Create a plan for one project, selected projects, or all registered projects. Planning inspects state before choosing actions.</p>';
    const plan=state.plan.value,counts=obj(plan.counts),unavailable=state.busy||!state.connected||!state.trusted;
    const rows=state.plan.items.map(item=>{const inspection=obj(item.inspection),snapshot=obj(inspection.snapshot),storage=obj(snapshot.storage);
        return `<tr><td>${e(item.project)}</td><td>${e(item.phase)}</td><td>${e(item.action)}</td><td>${e(item.availability)}</td>
            <td>${e(storage.replacement_estimate_bytes)}</td><td>${e(snapshot.source_state)}</td><td>${e(item.explicit_fallback_required)}</td>
            <td>${e(item.code??obj(item.outcome).state)}</td><td>${e(item.cleanup_pending)}</td></tr>`;}).join('');
    return facts([['Plan',plan.id],['Operation',plan.operation],['All registered projects',plan.all_registered],['Phase',plan.phase],
        ['Paused',plan.paused],['Total projects',counts.selected],['Ready',counts.ready],['Running',counts.running],['Succeeded',counts.succeeded],
        ['Failed',counts.failed],['Stale',counts.stale],['Skipped',counts.skipped],['Canceled',counts.canceled],['Cleanup pending',counts.cleanup_pending]])+
        `<div class="scroll"><table><thead><tr><th>Project</th><th>Phase</th><th>Action</th><th>Availability impact</th><th>Temporary disk estimate (bytes)</th><th>Source</th><th>Fallback approval required</th><th>Result</th><th>Cleanup pending</th></tr></thead><tbody>${rows}</tbody></table></div>
        ${state.plan.after?button('morePlan','More plan projects','',unavailable):''}
        ${button('refreshPlan','Refresh cached plan rows','',unavailable)}
        <p>Plan rows are cached observations. Read every preview page before approving the complete plan.</p>
        <p>Disk estimates are observations; native admission reserves space before work. A canceled plan may contain completed project receipts.</p>
        <label><input id="allowFallback" type="checkbox"> Approve any explicitly identified fresh/offline fallback in this exact plan</label>
        ${button('approvePlan','Approve this exact preview','',unavailable||plan.phase!=='ready'||typeof plan.preview_digest!=='string'||!!state.plan.after||state.plan.items.length!==counts.selected)}
        ${button(plan.paused?'resumePlan':'pausePlan',plan.paused?'Resume plan':'Pause plan','',unavailable)}${button('cancelPlan','Cancel plan','',unavailable)}`;
}
export function consoleContent(state:ServiceConsoleState):string {
    const health=obj(state.health.value?.observation),resources=obj(state.resources.value?.observation),configuration=obj(resources.configuration),model=state.model.value;
    const unavailable=state.busy||!state.trusted;
    const projects=state.page?.projects.map(row=>projectRow(row,state)).join('')??'<p>No project page has been observed.</p>';
    const lifecycle=obj(state.bootstrap.value?.lifecycle),pending=obj(lifecycle.pending);
    const manager=obj(state.bootstrap.value?.startupManager);
    return `<h1>AIRK Service Console</h1><p>${e(state.authority)} · extension ${e(state.version)} · ${state.connected?'Connected':'Disconnected'}${state.busy?' · Working':''}</p>
        ${!state.trusted?'<p role="alert">Trust the workspace before connecting AIRK or requesting work.</p>':''}
        ${state.message?`<p role="status">${e(state.message)}</p>`:''}
        <div class="controls">${button('refresh','Refresh','',state.busy)}${button('connect','Connect / reconnect','',unavailable)}
        ${button('start','Start service','',unavailable||state.lifecycle==='external')}${button('restart','Restart service','',unavailable||!state.connected||state.lifecycle==='external')}
        ${button('stop','Stop service','',unavailable||(!state.connected||state.health.state!=='live')&&typeof lifecycle.recorded_instance!=='string'||state.lifecycle==='external')}${button('logs','Open service logs','',state.busy)}${button('resources','Resource settings','',unavailable||!state.connected)}</div>
        ${state.lifecycle==='external'?'<p>Startup, stop and restart are controlled by the external service manager.</p>':''}
        <p>MCP response mode: <strong>${e(state.responseMode??'terse')}</strong>. Full adds diagnostic provenance, readiness and coverage details. Agents can request Full for one call when those details are needed.</p>
        ${button('responseMode','Choose terse / full responses','',unavailable)}
        <h2>Runtime and observations</h2>${observation('Service',state.health)}${observation('Bootstrap',state.bootstrap)}
        ${facts([['Lifecycle',state.lifecycle],['Installation',state.bootstrap.value?.installDir],['Bundled runtime',state.bootstrap.value?.bundleDigest],['Execution platform',state.bootstrap.value?.platform],['Bootstrap failure',state.bootstrap.value?.error]])}
        ${facts([['Selected payload verified',state.bootstrap.value?.payloadVerified],['Cached registry failure',state.bootstrap.value?.registryError],
            ['Startup manager observation',manager.mode],['Login registration installed',manager.installed],['Manager reports running',manager.running],
            ['Registration definition matches',manager.registered_definition_matches],['Registration verification failed',manager.verification_failed]])}
        ${facts([['Explicitly stopped',lifecycle.explicitly_stopped],['Pending native lifecycle request',pending.id],['Native lifecycle phase',pending.phase]])}
        ${pending.id?button('recoverLifecycle','Resume saved native lifecycle','',unavailable)+button('abandonLifecycle','Abandon saved native lifecycle intent','',unavailable):''}
        ${facts([['Service instance',health.instance],['Native version',health.version],['Protocol',health.protocol],['Service process',health.process_id],
            ['Executable identity',health.executable_sha256],['Runtime identity',health.runtime_sha256],['Native user',health.user],['Uptime (ms)',health.uptime_ms],
            ['Native log available',obj(health.logs).available],['Dropped diagnostic records',obj(health.logs).dropped_records],['Diagnostic write failures',obj(health.logs).write_failures]])}
        <p>Health confirms responsiveness. It does not prove indexing progress, source freshness, or complete semantic coverage.</p>
        <h2>Resources and inference</h2>${observation('Resources',state.resources)}${observation('Inference',state.model)}${observation('Source work',state.sources)}
        ${facts([['Preset',configuration.preset],['Resident workers',resources.resident_workers],['Resident limit',resources.resident_limit],
            ['Running jobs',resources.running_jobs],['Waiting projects',resources.waiting_projects],['Reserved memory (bytes)',resources.memory_reserved_bytes],
            ['Logical CPUs',resources.logical_cpus],['Extraction threads admitted',resources.extraction_threads_reserved],['Extraction thread limit',resources.extraction_threads_limit],
            ['Inference thread limit',resources.inference_threads_limit],['Configured background I/O override (bytes/s)',configuration.background_io_bytes_per_second??'Preset default'],['Available host memory (bytes)',resources.available_memory_bytes],
            ['Observed RSS (bytes)',resources.observed_rss_bytes],['Memory allowance (bytes)',resources.memory_limit_bytes],['Memory pressure',resources.pressure],
            ['Telemetry complete',resources.telemetry_complete],['Pending native drains',resources.pending_provider_drains],
            ['Inference process available',model?.available],['Inference worker',model?.worker],['Loaded-model events',model?.loads],['Compatible reuse events',model?.reuses],['Inference sessions',model?.sessions]])}
        ${button('startEmbedding','Start inference helper','',unavailable||!state.connected)}
        ${button('stopEmbedding','Stop this inference helper','',unavailable||!state.connected||typeof model?.worker!=='string')}
        ${button('forceEmbedding','Force-stop this owned inference helper','',unavailable||!state.connected||typeof model?.worker!=='string')}
        <h2>Registered projects</h2><p>Includes closed workspaces. Index and progress values are cached observations; missing values remain unknown.</p>
        <div class="controls">${button('firstPage','First page','',state.busy)}${button('nextPage','Next page','',state.busy||!state.page?.after)}
        ${button('repair','Repair selected',`data-scope="selected" data-operation="auto"`,unavailable||!state.connected||!state.selected.size)}
        ${button('repair','Repair all registered',`data-scope="all" data-operation="auto"`,unavailable||!state.connected)}</div>
        <p>${state.page?`Showing ${state.page.projects.length} projects at registry revision ${e(state.page.revision)}${state.page.after?'; more projects remain':''}.`:''} ${state.selected.size} selected across pages. All-project repair includes every registered project.</p>
        ${projects}<h2>Repair preview and receipts</h2>${planView(state)}
        <h2>Recent transitions</h2>${state.historyOmitted?'<p>Older transitions are omitted from this recent-history view.</p>':''}
        <ol>${state.events.slice(-32).reverse().map(event=>`<li>${e(event.kind)} · ${e(event.project)} · ${e(event.job_kind)} · ${e(event.code??event.observed_unix_ms)}</li>`).join('')}</ol>
        <h2>Recent process transitions</h2><p>Cached, bounded native logs; older or rotated records may be unavailable.</p>
        ${state.serviceEvents?observation('Process logs',state.serviceEvents):''}
        <ol>${(Array.isArray(state.serviceEvents?.value?.records)?state.serviceEvents!.value!.records:[]).slice(-32).reverse().map(value=>{const event=obj(value);return `<li>${e(event.kind)} · ${e(event.instance)} · ${e(event.project)} · ${e(event.observed_unix_ms)}</li>`;}).join('')}</ol>
        <h2>Requests awaiting confirmation</h2><p>An unknown outcome retains its original request key. Rechecking sends that exact request; forgetting it does not cancel native work.</p>
        <ul>${state.pending.map(intent=>`<li>${e(intent.title)} · ${intent.unknown?'Outcome unknown':'Prepared'} ${button('retryIntent','Check / retry exact request',`data-id="${e(intent.id)}"`,state.busy||!state.trusted)} ${intent.lifecycle&&intent.forceAvailable?button('forceIntent','Approve owned manager force stop',`data-id="${e(intent.id)}"`,state.busy||!state.trusted):''} ${button('forgetIntent','Forget local receipt request',`data-id="${e(intent.id)}"`,state.busy||!state.trusted)}</li>`).join('')}</ul>
        <h2>Diagnostic preview</h2><p>Reports contain metadata only and are never uploaded. Paths and user names are redacted by default. Copy preserves this captured preview; choose Preview again to refresh its observations.</p>
        <label><input type="checkbox" id="includePaths"${state.includePaths?' checked':''}> Include paths and user/machine names</label>
        ${button('previewReport','Preview report','',state.busy)}${button('copyReport','Copy this preview','',!state.preview||state.busy)}
        ${state.preview?`<pre tabindex="0">${e(state.preview)}</pre>`:''}`;
}
export function renderConsole(state:ServiceConsoleState):string {
    const nonce=randomBytes(24).toString('hex');
    return `<!DOCTYPE html><html><head><meta charset="UTF-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'nonce-${nonce}'; script-src 'nonce-${nonce}';"><meta name="viewport" content="width=device-width, initial-scale=1.0">
    <style nonce="${nonce}">body{font:var(--vscode-font-size) var(--vscode-font-family);color:var(--vscode-foreground);background:var(--vscode-editor-background);padding:1rem;max-width:1200px;margin:auto}h1{font-size:1.6rem}h2{margin-top:1.6rem}article{border:1px solid var(--vscode-panel-border);padding:1rem;margin:1rem 0}dl{display:grid;grid-template-columns:minmax(12rem,1fr) 3fr;gap:.35rem}dt{font-weight:600}dd{margin:0;overflow-wrap:anywhere}button{font:inherit;padding:.4rem .6rem;margin:.2rem;color:var(--vscode-button-foreground);background:var(--vscode-button-background);border:1px solid var(--vscode-button-border,transparent);cursor:pointer}button:disabled{opacity:.5;cursor:default}button:focus-visible,input:focus-visible{outline:2px solid var(--vscode-focusBorder);outline-offset:2px}.controls{display:flex;flex-wrap:wrap}.scroll{overflow:auto}table{border-collapse:collapse}td,th{border:1px solid var(--vscode-panel-border);padding:.4rem;text-align:left;overflow-wrap:anywhere}pre{white-space:pre-wrap;overflow-wrap:anywhere;border:1px solid var(--vscode-panel-border);padding:1rem}label{display:inline-block;margin:.25rem 0}@media(max-width:650px){dl{grid-template-columns:1fr}dd{margin-bottom:.6rem}}</style></head>
    <body data-revision="${e(state.revision)}"><main id="content">${consoleContent(state)}</main>
    <script nonce="${nonce}">const vscode=acquireVsCodeApi();document.addEventListener('click',event=>{const button=event.target.closest('button[data-action]');if(!button||button.disabled)return;const message={type:'service',revision:document.body.dataset.revision,action:button.dataset.action};for(const key of ['project','id','scope','operation'])if(button.dataset[key])message[key]=button.dataset[key];if(message.action==='previewReport')message.includePaths=document.getElementById('includePaths').checked;if(message.action==='approvePlan')message.allowFallback=document.getElementById('allowFallback').checked;vscode.postMessage(message);});document.addEventListener('change',event=>{const input=event.target;if(!input.matches('input[data-action]'))return;vscode.postMessage({type:'service',revision:document.body.dataset.revision,action:input.dataset.action,project:input.dataset.project,selected:input.checked});});window.addEventListener('message',event=>{const value=event.data;if(value&&value.type==='render'&&typeof value.html==='string'&&typeof value.revision==='string'){document.body.dataset.revision=value.revision;document.getElementById('content').innerHTML=value.html;}});</script></body></html>`;
}
export function diagnosticPreview(state:ServiceConsoleState,includePaths:boolean):string {
    const fields=(value:RecordValue,keys:string[])=>Object.fromEntries(keys.filter(key=>['string','number','boolean'].includes(typeof value[key])||value[key]===null).map(key=>[key,typeof value[key]==='string'?String(value[key]).slice(0,512):value[key]]));
    const health=obj(state.health.value?.observation),resources=obj(state.resources.value?.observation),model=obj(state.model.value);
    const report:RecordValue={schemaVersion:1,capturedAt:new Date().toISOString(),authority:includePaths?state.authority:'selected execution host',extensionVersion:state.version,
        observations:Object.fromEntries(['health','resources','model','sources','bootstrap'].map(name=>{const observation=state[name as 'health'];return [name,{state:observation.state,checkedAt:observation.checkedAt,code:observation.code}];})),
        service:fields(health,['instance','version','protocol','process_id','executable_sha256','runtime_sha256','uptime_ms',...(includePaths?['user']:[])]),
        resources:fields(resources,['resident_workers','resident_limit','running_jobs','waiting_projects','logical_cpus','extraction_threads_limit','extraction_threads_reserved','inference_threads_limit','memory_limit_bytes','memory_reserved_bytes','observed_rss_bytes','available_memory_bytes','telemetry_complete','pressure','pending_provider_drains','provider_drain_error']),
        resourceConfiguration:fields(obj(resources.configuration),['preset','extraction_threads','inference_threads','memory_bytes','background_io_bytes_per_second']),
        inference:fields(model,['available','worker','loads','reuses','sessions','observation_age_ms']),
        projects:(state.page?.projects??[]).map(row=>{const project=obj(row.project),registration=obj(project.registration),inspection=obj(row.inspection),snapshot=obj(inspection.snapshot),progress=obj(row.progress),work=obj(progress.progress),job=obj(progress.job);return {
            project:registration.binding,collection:includePaths?registration.collection:'[redacted]',root:includePaths?registration.root:'[redacted]',dataRoot:includePaths?registration.data_root:'[redacted]',
            fencingGeneration:project.fencing_generation,paused:project.paused,inspectionObservedAt:inspection.observed_unix_ms,
            inspection:fields(snapshot,['incarnation','publication','index_state','source_state','recovery_required','indexed_files','indexed_vectors','vectors_enabled','failure']),
            progress:fields(work,['phase','paused','stalled','elapsed_ms','phase_elapsed_ms','last_advance_age_ms','unpaused_idle_ms','captured_files','extracted_files','prepared_changes','committed_changes','publications','total_files','completed_semantic_units','publication']),
            job:fields(job,['id','kind','state','attempts','fencing_generation']),failedObservations:row.failed_observations};}),
        events:state.events.slice(-32).map(event=>fields(event,['sequence','observed_unix_ms','project','kind','job','job_kind','fencing_generation','attempts','code'])),
        processLogs:{state:state.serviceEvents?.state??'unavailable',checkedAt:state.serviceEvents?.checkedAt,
            malformedRecords:state.serviceEvents?.value?.malformed_records,tailTruncated:state.serviceEvents?.value?.tail_truncated,
            records:(Array.isArray(state.serviceEvents?.value?.records)?state.serviceEvents!.value!.records:[]).slice(-32).map(value=>fields(obj(value),['instance','kind','project','job','count','observed_unix_ms']))},
        historyOmitted:state.historyOmitted,moreProjects:!!state.page?.after,pendingRequestCount:state.pending.length,pathsRedacted:!includePaths,truncated:false};
    let text=JSON.stringify(report,null,2);
    while(Buffer.byteLength(text)>65536){report.truncated=true;const projects=report.projects as unknown[],events=report.events as unknown[];
        if(events.length)events.shift();else if(projects.length)projects.pop();else break;text=JSON.stringify(report,null,2);}
    return text;
}
