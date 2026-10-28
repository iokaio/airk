// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { managementView, panelDetails } from '../explorerManagement';
import { ServiceConsoleState } from '../serviceConsoleView';

const project = '12'.repeat(32), job = '34'.repeat(32), pending = '56'.repeat(32), plan = '78'.repeat(32), intent = '9a'.repeat(16);
test('queued repair is distinct from a finished recovery sample',()=>{
    const current=state(),entry=current.page!.projects[0];
    (entry.progress as any).job={id:job,kind:'recover',state:'succeeded'};
    entry.pending_job={id:pending,kind:'repair',state:'pending'};
    const health=managementView(current).projects[0].health;
    assert.equal(health.title,'Work queued');
    assert.match(health.detail,/repair request has not started/);
    assert.match(health.detail,/not reported a blocking reason/);
    assert.equal(health.reviewTab,'logs');
    assert.match(health.detail,/previous recover job \(succeeded\)/);
    assert.doesNotMatch(health.detail,/Work is running/);
});
test('queued maintenance gates identify a paused plan separately from project pause and reject foreign job gates',()=>{
    const current=state(),row=current.page!.projects[0];
    (row as any).pending_gate={job:pending,code:'maintenance_plan_paused',plan,plan_phase:'running'};
    let entry=managementView(current).projects[0];
    assert.equal(entry.health.title,'Repair plan paused');assert.equal(entry.health.reviewTab,'repair');
    assert.ok(entry.activity.some(([label,value])=>label==='Owning repair plan'&&value===plan));
    (row as any).pending_gate.job=job;
    entry=managementView(current).projects[0];assert.notEqual(entry.health.title,'Repair plan paused');
});
test('copyable activity retains exact request identities and unknown progress without requesting native work',()=>{
    const view=managementView(state()),entry=view.projects[0];
    const text=panelDetails(view,'activity','1.5.8',entry.summary.project,undefined,0);
    assert.match(text,/AIRK 1.5.8 — activity/);assert.match(text,/Captured: 1970-01-01T00:00:00.000Z/);
    assert.ok(text.includes(pending));assert.match(text,/Worker progress/);assert.match(text,/Wait reason/);
    assert.throws(()=>panelDetails(view,'activity','1.5.8','ff'.repeat(32)),/no longer available/);
    const long={...view,projects:[{...entry,events:['x'.repeat(100000)]}]};
    const bounded=panelDetails(long,'activity','1.5.8',entry.summary.project);
    assert.ok(new TextEncoder().encode(bounded).length<=65536);assert.match(bounded,/Details truncated/);
});
test('recovery explains source waits and helper exhaustion while preserving pause and disconnected observations',()=>{
    const current=state(),row=current.page!.projects[0];
    row.recovery=[{reason:'source_unavailable',state:'waiting',attempts:1,limit:3,job:pending,next_check_unix_ms:10000}];
    assert.equal(managementView(current).projects[0].health.title,'Waiting for source folder');
    (row.project as any).paused=true;assert.equal(managementView(current).projects[0].health.title,'Project paused');
    (row.project as any).paused=false;current.model.value!.available=false;row.recovery=[{reason:'helper_unavailable',state:'needs_attention',attempts:3,limit:3}];
    assert.equal(managementView(current).projects[0].health.action,'retryRecovery');
    current.connected=false;assert.equal(managementView(current).projects[0].health.title,'Connection unavailable');
});
test('shared helper retry history does not obscure a healthy helper or text-only project',()=>{
    const current=state(),row=current.page!.projects[0];
    row.recovery=[{reason:'helper_unavailable',state:'retrying',attempts:1,limit:3,job:null}];
    assert.notEqual(managementView(current).projects[0].health.title,'Recovering semantic indexing');
    current.model.value!.available=false;(row.inspection as any).snapshot.vectors_enabled=false;
    assert.notEqual(managementView(current).projects[0].health.title,'Recovering semantic indexing');
    row.configuration={id:pending,state:'pending'};row.preference={job:pending,vectors_enabled:true};
    assert.equal(managementView(current).projects[0].health.title,'Recovering semantic indexing');
});
test('queued preferences distinguish requested and saved settings and reject another job preference',()=>{
    const current=state(),row=current.page!.projects[0];
    (row.inspection as any).snapshot.vectors_enabled=false;
    row.configuration={id:pending,state:'pending'};
    row.preference={job:pending,vectors_enabled:true};
    let settings=managementView(current).projects[0].configuration;
    assert.equal(settings.vectorsEnabled,false);assert.equal(settings.requestedVectors,true);
    row.preference={job,vectors_enabled:true};settings=managementView(current).projects[0].configuration;
    assert.equal(settings.requestedVectors,undefined);
});
function state(overrides: Partial<ServiceConsoleState> = {}): ServiceConsoleState {
    const live = (value: any) => ({ state: 'live' as const, checkedAt: 1, value });
    return {
        revision: 'r', trusted: true, busy: false, authority: 'Local execution host', version: '1.5.0', connected: true, lifecycle: 'on-demand', responseMode: 'terse',
        health: live({ observation: { instance: 'ab'.repeat(16), version: '1.5.0', protocol: 2, uptime_ms: 1200, policy: { accepting: true } } }),
        resources: live({ observation: { configuration: { preset: 'balanced' }, resident_workers: 1, resident_limit: 4 } }),
        model: live({ available: true, worker: 'cd'.repeat(16), loads: 1 }), sources: live({}),
        bootstrap: live({ lifecycle: { explicitly_stopped: false }, selectedVersion: '1.5.0', diagnosis: {} }),
        events: [{ kind: 'job_succeeded', project, job_kind: 'index', observed_unix_ms: 5 }], historyOmitted: false,
        selected: new Set([project]), pending: [{ id: intent, title: 'index project', unknown: true }], includePaths: false,
        registrationAvailable: true, currentProjects: new Set([project]),
        page: { revision: 3, cached: true, observed_unix_ms: 9, after: null, projects: [{
            project: { registration: { binding: project, root: '/src/demo', data_root: '/data', collection: 'demo' }, paused: false },
            watch: { enabled: true, requested_revision: 5, completed_revision: 3 },
            progress: { job: { id: job, kind: 'index', state: 'failed', outcome: { code: 'quota_exceeded' } }, progress: { phase: 'extract', captured_files: 4, extracted_files: 2 } },
            pending_job: { id: pending, kind: 'reconcile', state: 'pending' },
            inspection: { observed_unix_ms: 8, snapshot: { publication: 'airk-r1:t', indexed_files: 10, semantic: { state: 'partial', completed_artifacts: 2, selected_artifacts: 10 } } },
        }] },
        plan: { value: { id: plan, operation: 'verify', phase: 'ready', preview_digest: 'ef'.repeat(32), counts: { selected: 1 } }, items: [{ project, phase: 'ready', action: 'verify' }], after: null },
        ...overrides,
    } as ServiceConsoleState;
}

test('management rows mirror the console wording and keep unknown values unobserved', () => {
    const view = managementView(state());
    const [entry] = view.projects;
    const row = (rows: readonly (readonly [string, string])[], name: string) => rows.find(([label]) => label === name)?.[1];
    assert.equal(row(entry.rows, 'Pending source changes'), '2');
    assert.equal(row(entry.rows, 'Completed / selected semantic artifacts'), '2 / 10');
    assert.equal(row(entry.rows, 'Inspection failure (cached)'), 'Not observed');
    assert.equal(row(entry.activity, 'Last job failure code'), 'quota_exceeded');
    assert.equal(row(entry.activity, 'Wait reason (sampled)'), 'No matching native wait observation');
    assert.equal(entry.jobId, job); assert.equal(entry.pendingJobId, pending); assert.equal(entry.selected, true);
    assert.equal(row(view.service, 'Accepting work'), 'Yes');
    assert.equal(row(view.resources, 'Resident workers / limit'), '1 / 4');
    assert.equal(view.pending[0].id, intent);
    assert.match(view.events[0], /job_succeeded/);
    assert.equal(view.plan?.approvable, true, 'a ready plan with every page read can be approved');
});

test('enabled actions follow the console rules for lifecycle, trust and plan pages', () => {
    assert.equal(managementView(state()).enabled.stop, true);
    assert.equal(managementView(state({ lifecycle: 'external' })).enabled.stop, false);
    assert.equal(managementView(state({ busy: true })).enabled.projectActions, false);
    assert.equal(managementView(state({ connected: false })).enabled.resources, false);
    assert.equal(managementView(state()).enabled.repairRuntime, false, 'payload repair needs an explicitly stopped service');
    const morePages = managementView(state({ plan: { value: { id: plan, phase: 'ready', preview_digest: 'ef'.repeat(32), counts: { selected: 2 } }, items: [{ project }], after: 'ab'.repeat(32) } }));
    assert.equal(morePages.plan?.approvable, false, 'approval stays disabled until every preview page is read');
    assert.equal(morePages.plan?.morePages, true);
    const inactive = managementView(state({ inactiveProjects: true }));
    assert.equal(inactive.inactive, true);
    assert.equal(inactive.projects[0].summary.collection, 'demo');
});

test('management exposes bounded traversal diagnostics and clears the current list on recovery', () => {
    const current = state();
    const watch = current.page!.projects[0].watch as Record<string, unknown>;
    watch.traversal = { unreadable: [{ root: 0, path: '/src/demo/artifacts/unreadable', reason: 'permission_denied' }], incomplete_roots: [0], truncated: false };
    watch.traversal_observed_unix_ms = 1234;
    const failed = managementView(current).projects[0].watchDiagnostics!;
    assert.ok(failed.some(([label, value]) => label.includes('permission_denied') && value === '/src/demo/artifacts/unreadable'));
    assert.ok(failed.some(([label, value]) => label === 'Last directory check' && value === '1970-01-01T00:00:01.234Z'));
    watch.traversal = { unreadable: [], incomplete_roots: [], truncated: false };
    const recovered = managementView(current).projects[0].watchDiagnostics!;
    assert.ok(recovered.some(([, value]) => value === 'None in the latest completed traversal'));
    assert.ok(!recovered.some(([label]) => label.startsWith('Directory ')));
});

test('storage observations preserve missing measurements, zero, quota reservations and per-project scope', () => {
    const current = state(), snapshot = (current.page!.projects[0].inspection as any).snapshot;
    assert.equal(managementView(current).projects[0].storage.usedBytes, undefined);
    snapshot.storage = { collection_used_bytes: 1048576, collection_reserved_bytes: 512, collection_limit_bytes: 2097152,
        root_available_bytes: 4096, physical_available_growth_bytes: 8192, replacement_estimate_bytes: 68000000 };
    const entry = managementView(current).projects[0];
    assert.equal(entry.storage.root, '/data'); assert.equal(entry.storage.observedAt, 8);
    assert.equal(entry.storage.usedBytes, 1048576); assert.equal(entry.storage.availableBytes, 1048064);
    assert.ok(entry.storage.rows.some(([label, value]) => label === 'Physical growth allowance (bytes)' && value === '8,192'));
    snapshot.storage.collection_used_bytes = 0; assert.equal(managementView(current).projects[0].storage.usedBytes, 0);
    snapshot.storage.collection_reserved_bytes = 3000000; assert.equal(managementView(current).projects[0].storage.availableBytes, 0);
    snapshot.storage.collection_used_bytes = Number.MAX_SAFE_INTEGER + 1;
    assert.equal(managementView(current).projects[0].storage.usedBytes, undefined);
    assert.equal(managementView(current).projects[0].storage.availableBytes, undefined);
    current.inactiveProjects = true;
    assert.equal(managementView(current).projects[0].storage.observedAt, 8, 'unregistered storage retains its real observation time');
});

test('executable versions distinguish approved payloads from running health observations', () => {
    const current = state();
    current.bootstrap.value = { selectedVersion: '1.5.5', payloadVerified: true };
    const versions = managementView(current).runtimeVersions;
    assert.ok(versions.some(([name, value]) => name.includes('airk-embed') && value.includes('1.5.5') && value.includes('running helper version not observed')));
    assert.ok(versions.some(([name, value]) => name.startsWith('Service') && value.startsWith('1.5.0')));
    current.bootstrap.value.payloadVerified = false;
    assert.ok(managementView(current).runtimeVersions.some(([name, value]) => name.includes('airk-embed') && value === 'Not observed'));
});

test('exhausted worker startup exposes the project restart beside a retained running job', () => {
    const current = state();
    const progress = current.page!.projects[0].progress as any;
    progress.job.state = 'running';
    current.workers = { observations: [{ project, identity: null, automatic_start_blocked: true, restarting: false }] };
    const entry = managementView(current).projects[0];
    assert.equal(entry.health.title, 'Indexing cannot continue');
    assert.equal(entry.health.action, 'restartProject');
    assert.equal(entry.work.current, false);
    assert.equal(entry.work.worker, 'unavailable');
    assert.equal(entry.work.captured, 4, 'old counters are retained, not silently discarded');
    assert.equal(entry.work.total, undefined, 'no invented completion denominator');
    current.connected = false;
    assert.equal(managementView(current).projects[0].health.action, undefined, 'retained metadata cannot recommend an enabled restart offline');
});

test('progress matches project, service, worker generation and job before being called current', () => {
    const current = state(), progress = current.page!.projects[0].progress as any;
    const identity = { project, service: 'ab'.repeat(16), worker: 'cd'.repeat(16), fencing_generation: 4 };
    progress.worker = { ...identity }; progress.current_fencing_generation = 4;
    progress.job.state = 'running'; progress.progress.job = job;
    progress.progress.elapsed_ms = 32000; progress.progress.phase_totals_ms = { extracting: 25000 };
    current.workers = { observations: [{ project, identity, automatic_start_blocked: false, restarting: false }] };
    assert.equal(managementView(current).projects[0].work.current, true);
    assert.equal(managementView(current).projects[0].health.title, 'Indexing');
    progress.worker.fencing_generation = 3;
    assert.equal(managementView(current).projects[0].work.current, false);
    progress.worker = { ...identity, project: 'ff'.repeat(32) };
    assert.equal(managementView(current).projects[0].work.current, false);
    progress.worker = { ...identity }; progress.progress.job = pending;
    assert.equal(managementView(current).projects[0].work.current, false, 'a preceding job is not current progress');
});

test('health separates pause, restart acceptance, terminal jobs, removed projects and source failures', () => {
    const current = state(), row = current.page!.projects[0] as any;
    row.project.paused = true;
    assert.equal(managementView(current).projects[0].health.action, 'resume');
    row.project.paused = false;
    assert.equal(managementView(current).projects[0].health.action, 'retryJob');
    current.workers = { observations: [{ project, identity: null, restarting: true, automatic_start_blocked: false }] };
    assert.equal(managementView(current).projects[0].health.title, 'Restarting project worker');
    assert.equal(managementView(current).projects[0].health.action, undefined, 'acceptance is not recovery');
    current.inactiveProjects = true;
    assert.equal(managementView(current).projects[0].health.title, 'Removed from AIRK');
    current.inactiveProjects = false; current.workers = undefined;
    row.progress.job.state = 'succeeded'; delete row.pending_job;
    row.watch.failure = 'root_unavailable';
    assert.equal(managementView(current).projects[0].health.title, 'Source watching needs attention');
    delete row.watch.failure; row.inspection.snapshot.index_state = 'ready';
    assert.equal(managementView(current).projects[0].health.title, 'Published index available');
});

test('completed cleanup is distinguished from a captured preview and project history stays isolated', () => {
    const value=state({inactiveProjects:true});
    value.page!.projects[0].configuration={id:job,state:'succeeded'};
    value.events.push({project:'ff'.repeat(32),kind:'foreign'});
    value.configurationResults=new Map([[project,{job,jobState:'succeeded',checkedAt:1,value:{guard:'ef'.repeat(32),complete:true}}]]);
    let entry=managementView(value).projects[0];
    assert.match(entry.lifecycle,/Cleanup completed/);assert.equal(entry.canDeleteDerived,false);
    assert.equal(entry.events.some(event=>event.includes('foreign')),false);
    value.configurationResults.get(project)!.value={guard:'ef'.repeat(32),active:'cd'.repeat(16)};
    entry=managementView(value).projects[0];assert.match(entry.lifecycle,/finish cleanup/);assert.equal(entry.canDeleteDerived,true);
});

test('idle semantic mode without published coverage offers complete setup and unavailable sources get recheck guidance', () => {
    const value=state();const row=value.page!.projects[0];row.progress=null;row.pending_job=null;
    row.inspection={snapshot:{index_state:'ready',vectors_enabled:true,semantic:{state:'disabled'}}};
    let entry=managementView(value).projects[0];assert.equal(entry.health.action,'enableSemantic');
    (row.inspection as any).snapshot.source_state='unavailable';
    entry=managementView(value).projects[0];assert.equal(entry.health.title,'Source folder unavailable');assert.equal(entry.health.action,'inspect');
});
