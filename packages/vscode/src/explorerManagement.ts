// SPDX-License-Identifier: Apache-2.0
import type { FactRow, ManagedProject, ManagementView, PlanView, ProjectHealthView, StatusSummary } from './explorerContracts';
import { projectSummaries } from './explorerControl';
import type { RecordValue } from './serviceClient';
import { ConsoleObservation, ServiceConsoleState, waitLabel } from './serviceConsoleView';
import { indexContents, semanticExplanation } from './explorerStatus';

/** Copy cached diagnostic metadata only. No reads, source bodies, jobs or index inspection are triggered. */
export function panelDetails(view: ManagementView, section: string, version: string, projectId?: string, status?: StatusSummary, now = Date.now()): string {
    const entry=view.projects.find(project=>project.summary.project===projectId);
    if(section!=='service'&&!entry)throw new Error('Project details are no longer available. Refresh status and copy again.');
    const lines=[`AIRK ${version} — ${section}`,`Captured: ${new Date(now).toISOString()}`,`Connection: ${view.connected?'connected':'disconnected'} (${view.authority})`,`Service check: ${view.checkedAt===undefined?'Not observed':new Date(view.checkedAt).toISOString()}`,
        'Cached observations; capture time does not establish source freshness.'];
    const facts=(heading:string,rows:readonly FactRow[])=>{lines.push('',heading,...rows.map(([key,value])=>`${key}: ${value}`));};
    if(entry){
        lines.push(`Project: ${entry.summary.name}`,`Project identity: ${entry.summary.project}`,`Health: ${entry.health.title}`,entry.health.detail);
    }
    if(entry&&['index','health','progress','activity'].includes(section)){
        const saved=indexContents(entry.summary,status);
        facts('Published index',[
            ['Files',scalar(saved.files)],['Chunks',scalar(saved.chunks)],['Vectors',scalar(saved.vectors)],
            ['Semantic files completed / selected',`${scalar(saved.semanticCompleted)} / ${scalar(saved.semanticSelected)}`],
            ['Publication',saved.publication??'Not observed'],['Source discovery',saved.discoveryState??'Not observed'],['Source updates',saved.freshness??'Not observed'],
            ['Publication observed',saved.observedAt===undefined?'Not observed':new Date(saved.observedAt).toISOString()],
        ]);
    }
    if(entry&&['health','progress','activity'].includes(section)){
        facts('Worker progress',entry.activity);
        lines.push(`Progress observed: ${entry.work.observedAt===undefined?'Not observed':new Date(entry.work.observedAt).toISOString()}`,`Current worker sample: ${entry.work.current?'yes':'no'}`);
        facts('Phase durations (ms)',entry.work.phaseTimes);
        facts('Requests',entry.requests);
        lines.push('',...entry.events,view.historyOmitted?'Earlier events omitted.':'');
    }
    if(entry&&section==='semantic'){
        const explanation=semanticExplanation(status);
        lines.push(explanation.title,explanation.detail);
        facts('Semantic observations',[
            ['Saved semantic preference',entry.configuration.vectorsEnabled===undefined?'Not observed':entry.configuration.vectorsEnabled?'Enabled':'Disabled'],
            ['Requested semantic preference',entry.configuration.requestedVectors===undefined?'No pending change observed':entry.configuration.requestedVectors?'Enabled':'Disabled'],
            ['Published semantic state',status?.semantic??entry.summary.semantic??'Not observed'],
            ['Model configured in publication',status?.localModelReady===undefined?'Not observed':String(status.localModelReady)],
            ['Publication',status?.publication??'Not observed'],['Observed',status?new Date(status.observedAt).toISOString():'Not observed']]);
        facts('Requests',entry.requests);
    }
    if(section==='service'){
        facts('Service',view.service);facts('Runtime',view.runtime);facts('Running versions',view.runtimeVersions);facts('Resources',view.resources);facts('Inference',view.inference);
        lines.push('',...view.processEvents);
    }
    if(entry&&section==='storage')facts('Storage',entry.storage.rows);
    if(entry&&section==='recovery'){
        facts('Inspection and settings',entry.rows);facts('Source access',entry.watchDiagnostics??[]);
        if(view.plan){facts('Repair plan',view.plan.rows);lines.push(...view.plan.items.map(item=>Object.entries(item).map(([key,value])=>`${key}: ${value}`).join(' · ')),view.plan.morePages?'More plan rows are not captured.':'');}
        else lines.push('Repair plan: Not observed');
    }
    const encoded=new TextEncoder().encode(lines.join('\n').replace(/\u0000/g,''));
    return encoded.length<=65536?new TextDecoder().decode(encoded):new TextDecoder().decode(encoded.slice(0,65400))+'\n[Details truncated; use the full diagnostic report in Logs.]';
}

const obj = (value: unknown): RecordValue => value && typeof value === 'object' && !Array.isArray(value) ? value as RecordValue : {};
/** Same wording as the Service Console: unknown stays "Not observed", never zero. */
function scalar(value: unknown): string {
    if (value === null || value === undefined) { return 'Not observed'; }
    if (typeof value === 'boolean') { return value ? 'Yes' : 'No'; }
    if (typeof value === 'number') { return Number.isSafeInteger(value) && value >= 0 ? value.toLocaleString('en-US') : 'Unknown'; }
    if (typeof value === 'string') { return value.slice(0, 4096); }
    return 'See details';
}
const rows = (values: [string, unknown][]): FactRow[] => values.map(([name, value]) => [name, scalar(value)]);
const bytes = (value: unknown): number | undefined => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : undefined;
const observed = (name: string, value: ConsoleObservation): FactRow =>
    [name, `${value.state}${value.code ? ` · ${value.code}` : ''} · checked ${new Date(value.checkedAt).toISOString()}`];

function failureDetail(code: unknown): string {
    if (typeof code === 'string' && code.endsWith('quota_inventory_limit_exceeded')) {
        return `Storage accounting reached its file-count limit before indexing could start. Storage shows cleanup progress; the raw diagnostic log records the measured count and limit. This does not mean a source file is too large. Native reason: ${code}.`;
    }
    const explanations: Record<string,string> = {
        storage_quota_root: 'The shared storage quota is full. Review Storage and removal or raise the quota in Advanced settings.',
        storage_quota_collection: 'This project reached its storage quota. Review Storage and removal or its quota setting.',
        storage_quota_reservation: 'Storage admission could not reserve enough space. Review storage capacity before retrying.',
        physical_storage_headroom: 'The disk has too little free space for safe indexing. Free space before retrying.',
        filesystem_storage_full: 'The disk is full. Free space before retrying.',
        source_unavailable: 'A source folder is missing or unreadable. Restore access, or remove the project if it was deleted.',
        configuration_settings_or_engine_changed: 'Settings changed while this request waited. Check current settings and submit the intended change again.',
        configuration_failed_reinspect: 'The settings could not be applied. Check current settings and diagnostics before retrying.',
        quota_ledger_busy: 'Storage accounting is busy in another project. AIRK waits within its recovery budget; saved evidence is preserved.',
        work_stalled: 'The worker stopped making progress. Review the wait reason and diagnostics before retrying.',
        model_setup_failed: 'Offline model preparation failed. Text indexing is still scheduled; review the model and helper diagnostics before retrying semantic setup.',
        index_scan_sources_source_byte_limit_exceeded: 'The project reached its configured total source-byte limit. Review ingestion limits in Advanced settings before retrying. Any existing searchable index is preserved.',
        index_scan_sources_extracted_byte_limit_exceeded: 'The project reached its configured extracted-text byte limit. Review ingestion limits in Advanced settings before retrying. Any existing searchable index is preserved.',
        index_scan_sources_source_file_limit_exceeded: 'The project reached its configured file-count limit. Review ingestion limits in Advanced settings before retrying. Any existing searchable index is preserved.',
    };
    return typeof code === 'string' ? `${explanations[code] ?? 'Review diagnostics before retrying.'} Native reason: ${code}.` : 'No failure reason was observed.';
}

function project(row: RecordValue, state: ServiceConsoleState): { rows: FactRow[]; activity: FactRow[]; extra: Omit<ManagedProject, 'summary' | 'rows' | 'activity'> } {
    const formats = obj(obj(state.health.value?.observation).storage_formats);
    const registration = obj(obj(row.project).registration), binding = String(registration.binding);
    const inspection = obj(row.inspection), snapshot = obj(inspection.snapshot), progress = obj(row.progress), work = obj(progress.progress), job = obj(progress.job), watch = obj(row.watch);
    const workers = Array.isArray(state.workers?.observations) ? state.workers!.observations : [];
    const worker = obj(workers.find(value => obj(value).project === binding)), identity = obj(worker.identity), sampled = obj(progress.worker);
    const current = state.connected && sampled.project === binding && identity.project === binding && typeof sampled.worker === 'string' && sampled.worker === identity.worker && sampled.service === identity.service
        && sampled.project === identity.project && sampled.fencing_generation === identity.fencing_generation && sampled.fencing_generation === progress.current_fencing_generation;
    const pending = obj(row.pending_job), waiting = obj(worker.waiting), pendingGate=obj(row.pending_gate);
    const gate=pendingGate.job===pending.id?pendingGate:{};
    const matchingWait = (typeof pending.id === 'string' && waiting.job === pending.id) || (current && job.state === 'running' && waiting.job === job.id);
    const wait = row.pending_paused === true ? 'Queued work is paused by project policy' : current && work.paused === true ? 'Work is paused'
        : worker.automatic_start_blocked ? 'Worker startup requires explicit retry' : worker.restarting ? 'Worker restart is pending'
        : matchingWait ? waitLabel(waiting.reason) : 'No matching native wait observation';
    const semantic = obj(snapshot.semantic), discovery = obj(snapshot.discovery), configuration = obj(row.configuration);
    const preference=obj(row.preference),requested=preference.job===configuration.id&&['pending','running'].includes(String(configuration.state))?preference:{};
    const storage = obj(snapshot.storage), used = bytes(storage.collection_used_bytes), reserved = bytes(storage.collection_reserved_bytes), limit = bytes(storage.collection_limit_bytes);
    const result = state.configurationResults?.get(binding), resultValue = result && result.job === configuration.id ? result.value : undefined;
    const traversal = obj(watch.traversal);
    const unreadable = Array.isArray(traversal.unreadable) ? traversal.unreadable.slice(0, 8).map(obj) : undefined;
    const traversalTime = typeof watch.traversal_observed_unix_ms === 'number' ? new Date(watch.traversal_observed_unix_ms) : undefined;
    // A live service connection is not proof that an old worker's counters are current.
    const sampleCurrent = current && work.job === job.id;
    const recoveries=Array.isArray(row.recovery)?row.recovery.slice(0,4).map(obj):[];
    const wantsSemantic=typeof requested.vectors_enabled==='boolean'?requested.vectors_enabled:snapshot.vectors_enabled!==false;
    const scopedRecovery=(r:Record<string,unknown>)=>typeof r.job==='string'&&(r.job===pending.id||r.job===job.id);
    const helperRecovery=(r:Record<string,unknown>)=>r.reason==='helper_unavailable'&&wantsSemantic&&(scopedRecovery(r)||state.model.value?.available!==true);
    const blockedRecovery=recoveries.find(r=>helperRecovery(r)&&['needs_attention','stopped'].includes(String(r.state)));
    const recovery=recoveries.find(r=>['waiting','retrying'].includes(String(r.state))&&(r.reason==='helper_unavailable'?helperRecovery(r):scopedRecovery(r)||r.reason==='worker_unavailable'&&worker.identity===null));
    const recoveryTitle=recovery?.reason==='source_unavailable'?'Waiting for source folder':String(recovery?.reason).includes('storage')?'Waiting for storage capacity':recovery?.reason==='helper_unavailable'?'Recovering semantic indexing':recovery?.reason==='work_stalled'?'Recovering stalled indexing':'Recovering indexing';
    const health: ProjectHealthView = !state.connected
        ? state.busy && state.health.state === 'unavailable'
            ? { severity: 'info', title: 'Connecting to AIRK', detail: 'Loading current project status. Published index information is shown separately.' }
            : { severity: 'warn', title: 'Connection unavailable', detail: 'These are retained observations. Reconnect to check current project health.' }
        : state.inactiveProjects
        ? { severity: 'info', title: 'Removed from AIRK', detail: 'Automatic work is stopped. AIRK is deleting this project’s index data and saved settings. Source files are untouched. This entry disappears when removal completes.' }
        : worker.automatic_start_blocked === true
        ? { severity: 'error', title: 'Indexing cannot continue', detail: 'No project worker is available and automatic startup retries are exhausted. The saved job may still say running. Restart only this project worker; active requests may disconnect.', action: 'restartProject', actionLabel: 'Restart project worker' }
        : worker.restarting === true
        ? { severity: 'info', title: 'Restarting project worker', detail: 'The restart was requested. Recovery is confirmed only when a worker connects and work advances.' }
        : obj(row.project).paused === true || row.pending_paused === true || (sampleCurrent && work.paused === true)
        ? { severity: 'info', title: 'Project paused', detail: 'Queued work is retained. Resume the project to allow indexing to continue.', action: 'resume', actionLabel: 'Resume project' }
        : gate.code==='maintenance_plan_paused'
        ? {severity:'warn',title:'Repair plan paused',detail:`The project is enabled, but the repair plan owning this request is paused. Open Recovery to resume or cancel that exact plan. Plan: ${scalar(gate.plan)}.`,reviewTab:'repair'}
        : typeof gate.code==='string'
        ? {severity:'warn',title:'Repair request cannot run',detail:`The queued request is not eligible for its repair plan. Native reason: ${gate.code}. Plan: ${scalar(gate.plan)} (${scalar(gate.plan_phase)}). Obsolete requests are settled by the service without replaying an old approval. Review Recovery and copy these details if the request persists.`,reviewTab:'repair'}
        : blockedRecovery
        ? {severity:'warn',title:blockedRecovery.state==='stopped'?'Semantic helper stopped':'Semantic indexing needs attention',detail:'Automatic helper recovery has stopped. Published text remains separately available. Retry resumes the shared helper and follows the saved request.',action:'retryRecovery',actionLabel:'Retry semantic indexing'}
        : recovery
        ? {severity:'info',title:recoveryTitle,detail:`AIRK has retained the accepted request. Attempt ${scalar(recovery.attempts)} of ${scalar(recovery.limit)}. It will continue when its prerequisite is available. Published search availability is shown separately.`}
        : configuration.state === 'pending' || configuration.state === 'running'
        ? { severity: 'info', title: configuration.state === 'pending' ? 'Settings change queued' : 'Applying project settings', detail: 'The settings change has not completed. Published coverage still describes the earlier index. Existing work must reach a safe boundary before queued settings can run; semantic work follows source publication.' }
        : configuration.state === 'failed'
        ? { severity: 'warn', title: 'Settings change failed', detail: failureDetail(obj(configuration.outcome).code), action: 'inspect', actionLabel: 'Recheck settings' }
        : job.state === 'failed' || job.state === 'canceled'
        ? { severity: job.state === 'failed' ? 'error' : 'info', title: job.state === 'failed' ? 'Job failed' : 'Work canceled', detail: `The ${scalar(job.kind)} job ${job.state}. ${failureDetail(obj(job.outcome).code)}`, action: 'retryJob', actionLabel: 'Retry job' }
        : watch.enabled === true && watch.failure === 'backend_unavailable' && typeof watch.observed_unix_ms !== 'number'
        ? { severity: 'info', title: 'Checking live update status', detail: 'Waiting for the source watcher to report its current status. Published index information is shown separately.' }
        : typeof watch.failure === 'string'
        ? { severity: 'warn', title: 'Source watching needs attention', detail: `Reported reason: ${watch.failure}. Review source access and unreadable directories. A retained index may still be available.` }
        : snapshot.source_state === 'unavailable'
        ? { severity: 'warn', title: 'Source folder unavailable', detail: 'Restore access to the registered source folder, or remove this project under Storage and removal. Retained search evidence may still be available.', action: 'inspect', actionLabel: 'Recheck source access' }
        : semantic.state === 'failed'
        ? { severity: 'warn', title: 'Semantic indexing needs attention', detail: 'Review semantic setup and the vector job failure. Source indexing and published lexical evidence have separate status.' }
        : job.state !== 'running' && typeof pending.id === 'string'
        ? {severity:!current&&!matchingWait?'warn':'info',title:'Work queued',detail:`The ${scalar(pending.kind)} request has not started. ${matchingWait?wait:!current?'No current worker is confirmed, and the service has not reported a blocking reason. Copy Activity and review diagnostics if this persists.':'The service has not reported a blocking reason.'} The retained sample belongs to the previous ${scalar(job.kind)} job (${scalar(job.state)}).`,...(!matchingWait?{reviewTab:'logs' as const}:{})}
        : sampleCurrent && work.stalled === true
        ? { severity: 'warn', title: 'No work advancement reported', detail: 'The native worker reports stalled work. Review the wait and diagnostics; cancellation may be waiting for a safe boundary.' }
        : job.state === 'running' || job.state === 'pending' || typeof pending.id === 'string'
        ? { severity: 'info', routineWork:sampleCurrent&&job.state==='running'&&!matchingWait, title: sampleCurrent && job.state === 'running' ? work.phase==='retiring' ? 'Cleaning up index' : job.kind === 'model_setup' ? 'Preparing offline model' : job.kind === 'inspect' ? 'Checking index and settings' : job.kind === 'vectors' ? 'Building semantic index' : 'Indexing' : 'Waiting for work', detail: sampleCurrent ? matchingWait ? wait : 'Work is running. Progress below describes the latest worker sample.' : 'The saved job and progress are shown below. Current progress has not been confirmed for this worker.' }
        : semantic.state === 'disabled' && snapshot.vectors_enabled === true
        ? { severity: 'warn', title: 'Semantic setup incomplete', detail: 'Semantic mode is enabled, but the inspected publication has no semantic coverage and no active request was observed. Complete setup prepares the selected offline model and queues indexing.', action: 'enableSemantic', actionLabel: 'Complete semantic setup' }
        : snapshot.index_state === 'available' || snapshot.index_state === 'ready'
        ? { severity: 'ok', title: 'Published index available', detail: 'A published index is available. This does not prove that saved source files still match it.' }
        : { severity: 'info', title: 'Index status not checked', detail: 'Check index and storage to collect a native inspection. Refresh only reloads cached observations.', action: 'inspect', actionLabel: 'Check index & storage' };
    return {
        rows: rows([['Index format', formats.index === 1 ? 'Index format 1 (index-v1)' : undefined], ['Index data', registration.data_root], ['Project identity', binding], ['Source observation', watch.enabled === true && watch.failure === 'backend_unavailable' && typeof watch.observed_unix_ms !== 'number'
            ? 'Checking live update status'
            : watch.failure === 'root_unavailable'
            ? 'Source watching is unavailable: a configured root or directory is missing, unreadable or redirected. Remove a deleted project from Management › Projects.'
            : watch.failure ?? watch.backend],
            ['Pending source changes', typeof watch.requested_revision === 'number' && typeof watch.completed_revision === 'number' ? Math.max(0, watch.requested_revision - watch.completed_revision) : undefined],
            ['Last inspected revision', snapshot.publication], ['Source availability (cached)', snapshot.source_state], ['Discovery (cached)', discovery.state],
            ['Indexed files (cached)', snapshot.indexed_files], ['Semantic coverage (cached)', semantic.state], ['Completed / selected semantic artifacts',
                semantic.selected_artifacts === undefined ? undefined : `${scalar(semantic.completed_artifacts)} / ${scalar(semantic.selected_artifacts)}`],
            ['Inspection failure (cached)', snapshot.failure], ['Last configuration job', configuration.id], ['Configuration state (cached)', configuration.state],
            ['Configuration failure', obj(configuration.outcome).code], ['Configuration result failure', result?.code],
            ['Unavailable cached fields', Array.isArray(row.failed_observations) ? row.failed_observations.join(', ') : undefined]]),
        activity: rows([['Worker', identity.worker ?? (worker.restarting ? 'Restart queued' : worker.identity === null ? 'No resident worker observed' : 'Worker identity not observed')],
            ['Worker observation', state.connected ? 'Connected sample' : 'Cached sample'], ['Phase', work.phase], ['Job', job.id ? `${scalar(job.kind)} · ${scalar(job.state)}` : undefined],
            ['Pending job', pending.id ? `${scalar(pending.kind)} · ${scalar(pending.state)}` : undefined],
            ['Queued request eligibility',gate.code??(pending.id?'No plan gate observed':'No queued request')],['Owning repair plan',gate.plan],['Owning plan phase',gate.plan_phase],['Wait reason (sampled)', wait],
            ['Wait sample age (ms)', matchingWait ? waiting.age_ms : undefined], ['Captured / extracted files', work.captured_files === undefined ? undefined : `${scalar(work.captured_files)} / ${scalar(work.extracted_files)}`],
            ['Prepared changes', work.prepared_changes], ['Committed changes', work.committed_changes], ['Published batches', work.publications], ['Total files', work.total_files],
            ['Completed semantic units (reported)',work.completed_semantic_units],
            ['Last real advance (ms ago at sample)', work.last_advance_age_ms], ['Unpaused time without progress (ms)', work.unpaused_idle_ms],
            ['Phase elapsed (ms)', work.phase_elapsed_ms], ['Last job failure code', obj(job.outcome).code]]),
        extra: {
            health,
            lifecycle: !state.inactiveProjects ? 'Active' : configuration.state === 'pending' || configuration.state === 'running' ? 'Cleanup or removal request pending'
                : configuration.state === 'succeeded' && resultValue?.complete === true ? 'Cleanup completed; ready to review registration'
                : typeof resultValue?.guard === 'string' ? 'Cleanup captured; finish cleanup before adding again' : 'Removal pending; deleting project data',
            requests: rows([...recoveries.map((r):[string,unknown]=>['Automatic recovery',`${scalar(r.reason)} · ${scalar(r.state)} · attempt ${scalar(r.attempts)} / ${scalar(r.limit)}${r.state==='waiting'&&typeof r.next_check_unix_ms==='number'?` · next check ${new Date(r.next_check_unix_ms).toLocaleTimeString()}`:''}`]),['Latest settings request', configuration.id ? `${scalar(configuration.state)} · ${scalar(configuration.id)}` : 'None observed'],
                ['Settings result', obj(configuration.outcome).code ?? (configuration.state === 'succeeded' ? 'Applied; published coverage is observed separately' : undefined)],
                ['Current / latest job', job.id ? `${scalar(job.kind)} · ${scalar(job.state)} · ${scalar(job.id)}` : 'None observed'],
                ['Queued job', pending.id ? `${scalar(pending.kind)} · ${scalar(pending.state)} · ${scalar(pending.id)}` : 'None observed'],
                ['Job result', obj(job.outcome).code ?? obj(job.outcome).state], ['Observed publication', snapshot.publication]]),
            events: state.events.filter(event => event.project === binding).slice(-16).reverse().map(event =>
                `${scalar(event.kind)} · ${scalar(event.job_kind)}${event.code ? ` · ${scalar(event.code)}` : ''} · ${typeof event.observed_unix_ms === 'number' ? new Date(event.observed_unix_ms).toLocaleString() : 'time not observed'}`),
            configuration: { detail: typeof snapshot.index_detail === 'string' ? snapshot.index_detail : undefined, state: typeof configuration.state === 'string' ? configuration.state : undefined,
                requestedVectors:typeof requested.vectors_enabled==='boolean'?requested.vectors_enabled:undefined,
                requestedDetail:typeof requested.detail==='string'?requested.detail:undefined,
                pending: configuration.state === 'pending' || configuration.state === 'running',
                failure: typeof obj(configuration.outcome).code === 'string' ? String(obj(configuration.outcome).code) : undefined,
                vectorsEnabled: typeof snapshot.vectors_enabled === 'boolean' ? snapshot.vectors_enabled : undefined },
            work: {
                current: sampleCurrent,
                worker: !state.connected ? 'unknown' : worker.restarting === true ? 'restarting' : typeof identity.worker === 'string' ? 'connected' : worker.identity === null ? 'unavailable' : 'unknown',
                observedAt: bytes(progress.observed_unix_ms), phase: typeof work.phase === 'string' ? work.phase : undefined,
                elapsedMs: bytes(work.elapsed_ms), phaseElapsedMs: bytes(work.phase_elapsed_ms), lastAdvanceMs: bytes(work.last_advance_age_ms),
                captured: bytes(work.captured_files), extracted: bytes(work.extracted_files), prepared: bytes(work.prepared_changes),
                committed: bytes(work.committed_changes), publications: bytes(work.publications), verified: bytes(work.verified_objects),
                total: bytes(work.total_files), semanticUnits: bytes(work.completed_semantic_units), wait,
                phaseTimes: rows(Object.entries(obj(work.phase_totals_ms)).slice(0, 11)),
            },
            storage: {
                ...(typeof registration.data_root === 'string' ? { root: registration.data_root } : {}),
                usedBytes: used, reservedBytes: reserved, limitBytes: limit,
                observedAt: bytes(inspection.observed_unix_ms),
                availableBytes: used !== undefined && reserved !== undefined && limit !== undefined ? Math.max(0, limit - used - reserved) : undefined,
                rows: rows([
                    ['Index format', formats.index === 1 ? 'Index format 1 (index-v1)' : undefined],
                    ['Cleanup files removed (latest pass)', obj(row.maintenance).removed_files],
                    ['Old index instances awaiting cleanup', obj(row.maintenance).pending_incarnations],
                    ['Unused objects awaiting cleanup', obj(row.maintenance).pending_objects],
                    ['Cleanup failed', obj(row.maintenance).failed],
                    ['Accounted index bytes', used], ['Reserved bytes', reserved], ['Collection quota (bytes)', limit],
                    ['Storage-root quota remaining (bytes)', bytes(storage.root_available_bytes)],
                    ['Physical growth allowance (bytes)', bytes(storage.physical_available_growth_bytes)],
                    ['Replacement estimate (bytes)', bytes(storage.replacement_estimate_bytes)],
                    ['Indexed vectors (cached)', snapshot.indexed_vectors], ['Vectors enabled (cached)', snapshot.vectors_enabled],
                    ['Inspection failure', snapshot.failure],
                ]),
            },
            ...(unreadable ? { watchDiagnostics: rows([
                ['Last directory check', traversalTime && Number.isFinite(traversalTime.getTime()) ? traversalTime.toISOString() : undefined],
                ['Unreadable directories', unreadable.length || traversal.truncated === true ? 'Observed failures; coverage is incomplete'
                    : Array.isArray(traversal.incomplete_roots) && traversal.incomplete_roots.length ? 'No unreadable paths reported; traversal is incomplete' : 'None in the latest completed traversal'],
                ...unreadable.map((entry, index): [string, unknown] => [`Directory ${index + 1} · ${scalar(entry.reason)}`, entry.path]),
                ['Diagnostic list truncated', traversal.truncated],
            ]) } : {}),
            selected: state.selected.has(binding),
            ...(typeof job.id === 'string' ? { jobId: job.id, jobState: String(job.state) } : {}),
            ...(typeof pending.id === 'string' && pending.id !== job.id ? { pendingJobId: pending.id } : {}),
            configurationResult: !!resultValue,
            canDeleteDerived: configuration.state === 'succeeded' && typeof resultValue?.active === 'string' && typeof resultValue?.guard === 'string',
            uncertainConfiguration: obj(configuration.outcome).code === 'configuration_outcome_unknown_reinspect',
        },
    };
}

function plan(state: ServiceConsoleState): PlanView | undefined {
    if (!state.plan) { return undefined; }
    const value = state.plan.value, counts = obj(value.counts);
    return {
        ...(value.phase==='finished'?{outcome:typeof counts.failed==='number'&&counts.failed>0?'failed' as const
            :typeof counts.selected==='number'&&counts.selected>0&&counts.succeeded===counts.selected
                ?typeof counts.cleanup_pending==='number'&&counts.cleanup_pending>0?'cleanup_pending' as const:'succeeded' as const:'incomplete' as const}:{}),
        id:typeof value.id==='string'?value.id:undefined,preview:typeof value.preview_digest==='string'?value.preview_digest:undefined,phase:typeof value.phase==='string'?value.phase:undefined,
        rows: rows([['Plan', value.id], ['Operation', value.operation], ['All registered projects', value.all_registered], ['Phase', value.phase], ['Paused', value.paused],
            ['Total projects', counts.selected], ['Ready', counts.ready], ['Running', counts.running], ['Succeeded', counts.succeeded], ['Failed', counts.failed],
            ['Stale', counts.stale], ['Skipped', counts.skipped], ['Canceled', counts.canceled], ['Cleanup pending', counts.cleanup_pending]]),
        items: state.plan.items.map(item => {
            const snapshot = obj(obj(item.inspection).snapshot), storage = obj(snapshot.storage);
            return { project: scalar(item.project), phase: scalar(item.phase), action: scalar(item.action), availability: scalar(item.availability),
                estimate: scalar(storage.replacement_estimate_bytes), source: scalar(snapshot.source_state), fallback: scalar(item.explicit_fallback_required),
                result: scalar(item.code ?? obj(item.outcome).state), cleanup: scalar(item.cleanup_pending) };
        }),
        morePages: !!state.plan.after,
        paused: value.paused === true,
        approvable: value.phase === 'ready' && typeof value.preview_digest === 'string' && !state.plan.after && state.plan.items.length === counts.selected,
    };
}

/** Everything the nine Management tabs show, derived from the one shared console state. */
export function managementView(state: ServiceConsoleState): ManagementView {
    const health = obj(state.health.value?.observation), resources = obj(state.resources.value?.observation), configuration = obj(resources.configuration);
    const model = state.model.value, bootstrap = obj(state.bootstrap.value), lifecycle = obj(bootstrap.lifecycle), pendingLifecycle = obj(lifecycle.pending);
    const manager = obj(bootstrap.startupManager), repair = obj(bootstrap.payloadRepair), diagnosis = obj(bootstrap.diagnosis);
    const activation = obj(diagnosis.pending_activation), activationResult = obj(diagnosis.activation_result);
    const unavailable = state.busy || !state.trusted;
    const enabled: Record<string, boolean> = {
        connect: !unavailable, refresh: !state.busy, registerWorkspace: !unavailable && !!state.registrationAvailable,
        start: !unavailable && state.lifecycle !== 'external',
        restart: !unavailable && state.connected && state.lifecycle !== 'external',
        stop: !unavailable && state.lifecycle !== 'external' && ((state.connected && state.health.state === 'live') || typeof lifecycle.recorded_instance === 'string'),
        resources: !unavailable && state.connected,
        repairRuntime: !unavailable && state.lifecycle === 'on-demand' && lifecycle.explicitly_stopped === true,
        upgradeRuntime: !unavailable && state.lifecycle === 'on-demand' && lifecycle.explicitly_stopped !== true,
        rollbackRuntime: !unavailable && state.lifecycle === 'on-demand' && lifecycle.explicitly_stopped !== true,
        resumeActivation: !unavailable && state.lifecycle === 'on-demand' && !!activation.candidate,
        recoverLifecycle: !unavailable && !!pendingLifecycle.id, abandonLifecycle: !unavailable && !!pendingLifecycle.id,
        startEmbedding: !unavailable && state.connected, stopEmbedding: !unavailable && state.connected && typeof model?.worker === 'string',
        forceEmbedding: !unavailable && state.connected && typeof model?.worker === 'string',
        projectActions: !unavailable && state.connected, responseMode: !unavailable,
    };
    const summaries = new Map(projectSummaries(state).map(summary => [summary.project, summary]));
    const projects: ManagedProject[] = (state.page?.projects ?? []).flatMap(row => {
        const binding = String(obj(obj(row.project).registration).binding);
        const summary = summaries.get(binding) ?? (state.inactiveProjects ? { project: binding, collection: String(obj(obj(row.project).registration).collection ?? binding.slice(0, 12)),
            name: String(obj(obj(row.project).registration).collection ?? ''), root: String(obj(obj(row.project).registration).root ?? ''), current: false } : undefined);
        if (!summary || !/^[0-9a-f]{64}$/.test(binding)) { return []; }
        const detail = project(row, state);
        return [{ summary, rows: detail.rows, activity: detail.activity, ...detail.extra }];
    });
    const legacyReview = state.legacyReview, legacyValue = obj(legacyReview?.value), preview = obj(legacyValue.preview), spec = obj(preview.spec), target = obj(spec.target);
    return {
        busy: state.busy, connected: state.connected, checkedAt:state.health?.state==='unavailable'?undefined:state.health?.checkedAt, trusted: state.trusted, authority: state.authority,
        ...(state.lifecycle ? { lifecycle: state.lifecycle } : {}), responseMode: state.responseMode ?? 'terse', ...(state.message ? { message: state.message } : {}),
        service: [observed('Service', state.health), ...rows([['Service instance', health.instance], ['Native version', health.version], ['Index format', obj(health.storage_formats).index === 1 ? 'Index format 1 (index-v1)' : undefined], ['Service-state format', obj(health.storage_formats).service_state], ['Protocol', health.protocol],
            ['Service process', health.process_id], ['Uptime (ms)', health.uptime_ms], ['Accepting work', obj(health.policy).accepting],
            ['Executable identity', health.executable_sha256], ['Runtime identity', health.runtime_sha256], ['Native log available', obj(health.logs).available],
            ['Dropped diagnostic records', obj(health.logs).dropped_records], ['Diagnostic write failures', obj(health.logs).write_failures]])],
        runtime: [observed('Bootstrap', state.bootstrap), ...rows([['Lifecycle', state.lifecycle], ['Installation', bootstrap.installDir], ['Bundled runtime', bootstrap.bundleDigest],
            ['Execution platform', bootstrap.platform], ['Bootstrap failure', bootstrap.error], ['Selected runtime', bootstrap.selectedRuntime],
            ['Selected runtime version', bootstrap.selectedVersion], ['Payload approval basis', bootstrap.verificationBasis], ['Selected payload verified', bootstrap.payloadVerified],
            ['Cached registry failure', bootstrap.registryError], ['Startup manager', manager.mode], ['Login registration installed', manager.installed],
            ['Manager reports running', manager.running], ['Runtime repair phase', repair.phase], ['Explicitly stopped', lifecycle.explicitly_stopped],
            ['Pending native lifecycle request', pendingLifecycle.id], ['Native lifecycle phase', pendingLifecycle.phase], ['Pending runtime activation', activation.candidate],
            ['Activation phase', activation.phase], ['Runtime staging error', diagnosis.staging_error], ['Runtime activation error', diagnosis.activation_error],
            ['Last activation outcome', activationResult.outcome]])],
        runtimeVersions: rows([
            ['Native CLI (airk)', typeof bootstrap.selectedVersion === 'string' ? `${bootstrap.selectedVersion} · selected payload${bootstrap.payloadVerified === true ? ', verified' : ', verification not observed'}` : undefined],
            ['Inference helper (airk-embed)', bootstrap.payloadVerified === true && typeof bootstrap.selectedVersion === 'string' ? `${bootstrap.selectedVersion} · verified selected payload; running helper version not observed` : undefined],
            ['Service (airk)', typeof health.version === 'string' ? `${health.version} · ${state.health.state} health observation${typeof health.process_id === 'number' ? ` · PID ${health.process_id}` : ''}` : undefined],
        ]),
        resources: [observed('Resources', state.resources), observed('Source work', state.sources), ...rows([['Preset', configuration.preset],
            ['Resident workers / limit', resources.resident_workers === undefined ? undefined : `${scalar(resources.resident_workers)} / ${scalar(resources.resident_limit)}`],
            ['Running jobs', resources.running_jobs], ['Waiting projects', resources.waiting_projects], ['Logical CPUs', resources.logical_cpus],
            ['Extraction threads admitted / limit', resources.extraction_threads_reserved === undefined ? undefined : `${scalar(resources.extraction_threads_reserved)} / ${scalar(resources.extraction_threads_limit)}`],
            ['Inference thread limit', resources.inference_threads_limit], ['Reserved / allowed memory (bytes)', resources.memory_reserved_bytes === undefined ? undefined
                : `${scalar(resources.memory_reserved_bytes)} / ${scalar(resources.memory_limit_bytes)}`], ['Observed RSS (bytes)', resources.observed_rss_bytes],
            ['Available host memory (bytes)', resources.available_memory_bytes], ['Memory pressure', resources.pressure], ['Protected query headroom (bytes)', resources.query_headroom_bytes],
            ['Source pacing ceiling (bytes/s)', resources.background_io_limit_bytes_per_second], ['Telemetry complete', resources.telemetry_complete]])],
        inference: [observed('Inference', state.model), ...rows([['Inference process available', model?.available], ['Inference worker', model?.worker],
            ['Loaded-model events', model?.loads], ['Compatible reuse events', model?.reuses], ['Inference sessions', model?.sessions]])],
        enabled, projects, inactive: state.inactiveProjects === true, morePages: !!state.page?.after,
        pageNote: state.page ? `Showing ${state.page.projects.length} projects at registry revision ${state.page.revision}${state.page.after ? '; more projects remain' : ''}.` : 'No project page has been observed.',
        selectedCount: state.selected.size,
        ...(plan(state) ? { plan: plan(state)! } : {}),
        // Unacknowledged requests belong to the internal journal and diagnostics.
        pending: [],
        events: state.events.slice(-32).reverse().map(event => `${scalar(event.kind)} · ${scalar(event.project)} · ${scalar(event.job_kind)} · ${scalar(event.code ?? event.observed_unix_ms)}`),
        processEvents: (Array.isArray(state.serviceEvents?.value?.records) ? state.serviceEvents!.value!.records : []).slice(-32).reverse().map(value => {
            const event = obj(value), timestamp = bytes(event.observed_unix_ms);
            const time = timestamp !== undefined && timestamp <= 8640000000000000 ? new Date(timestamp).toISOString() : scalar(event.observed_unix_ms);
            return [scalar(event.kind), time, ...(event.project ? [`project ${scalar(event.project)}`] : []),
                ...(event.job ? [`job ${scalar(event.job)}`] : []), ...(event.code ? [`reason ${scalar(event.code)}`] : []),
                `service ${scalar(event.instance)}`].join(' · ');
        }),
        historyOmitted: state.historyOmitted,
        processLogStatus: state.serviceEvents ? observed('Process logs', state.serviceEvents)[1] : 'Not observed; refresh to request recent service events.',
        configurationResultsOmitted: state.configurationResultsOmitted === true,
        ...(state.legacyAvailable ? { legacy: {
            rows: legacyReview ? rows([['Captured review', new Date(legacyReview.checkedAt).toISOString()], ['Saved removal phase', legacyReview.phase], ['Old source', target.root],
                ['Old index data', target.data_dir], ['Old collection', target.collection], ['Configured executable', target.executable], ['OS registration', spec.id],
                ['Approval', legacyValue.approval]]) : [],
            reviewed: !!legacyReview, canRemove: !!legacyValue.approval && legacyReview?.phase !== 'complete',
            receipts: (Array.isArray(state.legacyReceipts?.items) ? state.legacyReceipts!.items : []).map(value => {
                const row = obj(value); return { id: String(row.approval), phase: scalar(row.phase), service: scalar(row.service_id) };
            }),
        } } : {}),
        ...(state.preview ? { preview: state.preview } : {}), includePaths: state.includePaths,
    };
}
