// SPDX-License-Identifier: Apache-2.0
import type { FactRow, ManagedProject, ManagementView, PlanView } from './explorerContracts';
import { projectSummaries } from './explorerControl';
import type { RecordValue } from './serviceClient';
import { ConsoleObservation, ServiceConsoleState, waitLabel } from './serviceConsoleView';

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

function project(row: RecordValue, state: ServiceConsoleState): { rows: FactRow[]; activity: FactRow[]; extra: Omit<ManagedProject, 'summary' | 'rows' | 'activity'> } {
    const registration = obj(obj(row.project).registration), binding = String(registration.binding);
    const inspection = obj(row.inspection), snapshot = obj(inspection.snapshot), progress = obj(row.progress), work = obj(progress.progress), job = obj(progress.job), watch = obj(row.watch);
    const workers = Array.isArray(state.workers?.observations) ? state.workers!.observations : [];
    const worker = obj(workers.find(value => obj(value).project === binding)), identity = obj(worker.identity), sampled = obj(progress.worker);
    const current = state.connected && typeof sampled.worker === 'string' && sampled.worker === identity.worker && sampled.service === identity.service
        && sampled.project === identity.project && sampled.fencing_generation === identity.fencing_generation && sampled.fencing_generation === progress.current_fencing_generation;
    const pending = obj(row.pending_job), waiting = obj(worker.waiting);
    const matchingWait = (typeof pending.id === 'string' && waiting.job === pending.id) || (current && job.state === 'running' && waiting.job === job.id);
    const wait = row.pending_paused === true ? 'Queued work is paused by project policy' : current && work.paused === true ? 'Work is paused'
        : worker.automatic_start_blocked ? 'Worker startup requires explicit retry' : worker.restarting ? 'Worker restart is pending'
        : matchingWait ? waitLabel(waiting.reason) : 'No matching native wait observation';
    const semantic = obj(snapshot.semantic), discovery = obj(snapshot.discovery), configuration = obj(row.configuration);
    const storage = obj(snapshot.storage), used = bytes(storage.collection_used_bytes), reserved = bytes(storage.collection_reserved_bytes), limit = bytes(storage.collection_limit_bytes);
    const result = state.configurationResults?.get(binding), resultValue = result && result.job === configuration.id ? result.value : undefined;
    const traversal = obj(watch.traversal);
    const unreadable = Array.isArray(traversal.unreadable) ? traversal.unreadable.slice(0, 8).map(obj) : undefined;
    const traversalTime = typeof watch.traversal_observed_unix_ms === 'number' ? new Date(watch.traversal_observed_unix_ms) : undefined;
    return {
        rows: rows([['Index data', registration.data_root], ['Project identity', binding], ['Source observation', watch.failure === 'root_unavailable'
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
            ['Pending job', pending.id ? `${scalar(pending.kind)} · ${scalar(pending.state)}` : undefined], ['Wait reason (sampled)', wait],
            ['Wait sample age (ms)', matchingWait ? waiting.age_ms : undefined], ['Captured / extracted files', work.captured_files === undefined ? undefined : `${scalar(work.captured_files)} / ${scalar(work.extracted_files)}`],
            ['Committed changes', work.committed_changes], ['Published batches', work.publications], ['Total files', work.total_files],
            ['Last real advance (ms ago at sample)', work.last_advance_age_ms], ['Unpaused time without progress (ms)', work.unpaused_idle_ms],
            ['Phase elapsed (ms)', work.phase_elapsed_ms], ['Last job failure code', obj(job.outcome).code]]),
        extra: {
            storage: {
                ...(typeof registration.data_root === 'string' ? { root: registration.data_root } : {}),
                usedBytes: used, reservedBytes: reserved, limitBytes: limit,
                observedAt: bytes(inspection.observed_unix_ms),
                availableBytes: used !== undefined && reserved !== undefined && limit !== undefined ? Math.max(0, limit - used - reserved) : undefined,
                rows: rows([
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
        busy: state.busy, connected: state.connected, trusted: state.trusted, authority: state.authority,
        ...(state.lifecycle ? { lifecycle: state.lifecycle } : {}), responseMode: state.responseMode ?? 'terse', ...(state.message ? { message: state.message } : {}),
        service: [observed('Service', state.health), ...rows([['Service instance', health.instance], ['Native version', health.version], ['Protocol', health.protocol],
            ['Service process', health.process_id], ['Uptime (ms)', health.uptime_ms], ['Accepting work', obj(health.policy).accepting],
            ['Executable identity', health.executable_sha256], ['Runtime identity', health.runtime_sha256], ['Native log available', obj(health.logs).available],
            ['Dropped diagnostic records', obj(health.logs).dropped_records]])],
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
        pending: state.pending.map(intent => ({ id: intent.id, title: intent.title, unknown: intent.unknown, lifecycle: !!intent.lifecycle, forceAvailable: !!intent.forceAvailable })),
        events: state.events.slice(-32).reverse().map(event => `${scalar(event.kind)} · ${scalar(event.project)} · ${scalar(event.job_kind)} · ${scalar(event.code ?? event.observed_unix_ms)}`),
        processEvents: (Array.isArray(state.serviceEvents?.value?.records) ? state.serviceEvents!.value!.records : []).slice(-32).reverse().map(value => {
            const event = obj(value); return `${scalar(event.kind)} · ${scalar(event.instance)} · ${scalar(event.project)} · ${scalar(event.observed_unix_ms)}`;
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
