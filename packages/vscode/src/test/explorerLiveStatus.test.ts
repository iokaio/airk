// SPDX-License-Identifier: Apache-2.0
// U103: live index status, everyday actions, usage and activity in the
// Explorer. Native responses are canned literals and every expected argv,
// row and header word is written out here, never derived from the code under
// test.
import { strict as assert } from 'node:assert';
import Module from 'node:module';
import { test } from 'node:test';
import { commandOutcome, LiveStatus, liveRow, LOGS_ARGV, usageArgv, workActionArgv, workActive, type LiveTimers } from '../airkctl/features/liveStatus';
import { projectSummaries } from '../explorerControl';
import { managementView } from '../explorerManagement';
import { headline, headlineWork, statusSummary } from '../explorerStatus';
import type { ServiceConsoleState } from '../serviceConsoleView';

const binding = 'ab'.repeat(32), job = 'cd'.repeat(32), queued = 'ef'.repeat(32);
const registration = { binding, root: '/src/demo', data_root: '/data/demo', collection: 'demo' };
const runningJob = { id: job, project: binding, kind: 'reconcile', state: 'running', fencing_generation: 3, attempts: 1, outcome: null };
const progressResponse = {
    status: 'progress', project: binding,
    observation: {
        worker: { project: binding, worker: '11'.repeat(16), service: '22'.repeat(16), fencing_generation: 3 }, current_fencing_generation: 3,
        progress: { job, grant: null, sequence: 9, phase: 'publishing', paused: false, stalled: false, elapsed_ms: 60000, phase_elapsed_ms: 2000,
            phase_totals_ms: { publishing: 2000 }, last_advance_age_ms: 100, unpaused_idle_ms: 0, captured_files: 2016, extracted_files: 2016,
            prepared_changes: 1200, committed_changes: 992, publications: 4, verified_objects: 0, indexed_files: 992, total_files: 2016,
            completed_semantic_units: 0, publication: null },
        observed_unix_ms: 1_700_000_000_000, job: runningJob,
    },
    pending_job: { id: queued, project: binding, kind: 'index', state: 'pending', fencing_generation: 3, attempts: 0, outcome: null },
    queued_jobs: [{ id: queued, project: binding, kind: 'index', state: 'pending', fencing_generation: 3, attempts: 0, outcome: null }],
    queue_truncated: false, project_paused: false, pending_paused: false, observed_unix_ms: 1_700_000_000_000,
};
const watchResponse = { status: 'watch', project: binding, observation: { enabled: true, requested_revision: 5, completed_revision: 5, automatic_retries_exhausted: false,
    automatic_retry_canceled: false, configuration_observed: true, configured_roots: 1, native_roots: 1, backend: 'native', failure: null, observed_unix_ms: 1_700_000_000_000, last_reconciled_unix_ms: 1_699_999_999_000 } };

function consoleWith(rows: Record<string, unknown>[]): ServiceConsoleState {
    const unavailable = { state: 'unavailable' as const, checkedAt: 1 };
    return { revision: '1', trusted: true, busy: false, authority: 'airkctl', version: '1.8.34', connected: true, health: { state: 'live', checkedAt: 1, value: { observation: {} } },
        resources: unavailable, model: unavailable, sources: unavailable, bootstrap: unavailable, events: [], historyOmitted: false, selected: new Set(), pending: [], includePaths: false,
        page: { revision: 1, cached: true, observed_unix_ms: 1, after: null, projects: rows } };
}
/** A manual clock: timers fire only when the test advances it. */
function manualTimers() {
    let now = 0, next = 0;
    const pending = new Map<number, { at: number; callback: () => void }>();
    const timers: LiveTimers = { set: (callback, ms) => { const id = ++next; pending.set(id, { at: now + ms, callback }); return id; }, clear: handle => { pending.delete(handle as number); }, now: () => now };
    return { timers, pending, advance(ms: number) { now += ms; for (const [id, entry] of [...pending]) { if (entry.at <= now) { pending.delete(id); entry.callback(); } } } };
}
// Drain queued promise work (not a timed sleep): setImmediate runs after every pending microtask.
const settle = async () => { for (let i = 0; i < 3; i++) { await new Promise(resolve => setImmediate(resolve)); } };

test('Explorer actions send exactly these airkctl commands, each selecting the index with --service-project', () => {
    const select = ['--service-project', binding, '--output', 'json'];
    assert.deepEqual(workActionArgv('index', binding), ['index', ...select]);
    assert.deepEqual(workActionArgv('pause', binding), ['pause', ...select]);
    assert.deepEqual(workActionArgv('resume', binding), ['resume', ...select]);
    assert.deepEqual(workActionArgv('watch_on', binding), ['watch', 'on', ...select]);
    assert.deepEqual(workActionArgv('watch_off', binding), ['watch', 'off', ...select]);
    assert.deepEqual(workActionArgv('retry', binding, job), ['jobs', 'retry', job, ...select]);
    assert.deepEqual(workActionArgv('cancel', binding, job), ['jobs', 'cancel', job, ...select]);
    assert.throws(() => workActionArgv('retry', binding), (error: any) => error.code === 'invalid_job');
    assert.throws(() => workActionArgv('pause', 'not-a-binding'), (error: any) => error.code === 'invalid_project');
    assert.deepEqual(usageArgv(binding, 30), ['usage', 'report', '--days', '30', '--format', 'json', '--service-project', binding, '--output', 'json']);
    assert.deepEqual([...LOGS_ARGV], ['service', 'logs', '--limit', '32', '--format', 'json', '--output', 'json']);
});

test('a command outcome keeps the parsed result, the error line and every channel, refused or not', () => {
    const argv = ['pause', '--service-project', binding, '--output', 'json'];
    assert.deepEqual(commandOutcome(argv, { exitCode: 0, class: 'success', stdout: '{"status":"paused","project":"x"}', stderr: '' }),
        { ok: true, argv, class: 'success', exitCode: 0, result: { status: 'paused', project: 'x' } });
    const error = { code: 'session_method_refused', message: 'Not on the session surface.', retryable: false } as any;
    assert.deepEqual(commandOutcome(argv, { exitCode: 3, class: 'refused', stdout: 'not json', stderr: 'detail', error }),
        { ok: false, argv, class: 'refused', exitCode: 3, stdout: 'not json', stderr: 'detail', error });
});

test('live Progress and Watch reads fill the console row: summaries, work progress and the overview facts show them', async () => {
    const controls: unknown[] = [];
    const session = { control: async (method: any) => { controls.push(structuredClone(method)); return method.progress ? structuredClone(progressResponse) : structuredClone(watchResponse); } };
    const clock = manualTimers(), live = new LiveStatus(session, () => {}, clock.timers);
    live.track([binding, 'not-a-binding']);
    await live.read();
    assert.deepEqual(controls, [{ progress: { project: binding } }, { watch: { project: binding } }]);
    const row = liveRow(registration, live.observation(binding));
    assert.deepEqual(row.project, { registration, paused: false });
    assert.deepEqual(row.progress, progressResponse.observation, 'the row carries the complete native ProgressObservation');
    assert.deepEqual(row.pending_job, progressResponse.pending_job);
    assert.deepEqual(row.queued_jobs, progressResponse.queued_jobs);
    assert.deepEqual(row.watch, watchResponse.observation);
    assert.deepEqual((row.live as any).progress, progressResponse, 'the complete Progress response stays available');
    const [summary] = projectSummaries(consoleWith([row]));
    assert.equal(summary.jobState, 'running'); assert.equal(summary.jobKind, 'reconcile'); assert.equal(summary.phase, 'publishing');
    assert.equal(summary.watching, true); assert.equal(summary.paused, false);
    assert.equal(summary.committedChanges, 992); assert.equal(summary.capturedFiles, 2016);
    assert.equal(summary.jobId, job); assert.equal(summary.pendingJobId, queued); assert.equal(summary.pendingJobState, 'pending');
    const managed = managementView(consoleWith([row])).projects[0];
    assert.equal(managed.work.committed, 992); assert.equal(managed.work.captured, 2016); assert.equal(managed.work.phase, 'publishing');
    assert.equal(managed.jobId, job); assert.equal(managed.jobState, 'running'); assert.equal(managed.pendingJobId, queued);
    assert.equal(workActive(live.observation(binding)), true);
});

test('a refused live read keeps its code beside the previous complete response and never invents an empty one', async () => {
    let refuse = false;
    const session = { control: async (method: any) => refuse ? { status: 'failure', code: 'session_method_refused' } : method.progress ? structuredClone(progressResponse) : structuredClone(watchResponse) };
    const live = new LiveStatus(session, () => {}, manualTimers().timers);
    live.track([binding]); await live.read();
    refuse = true; await live.read();
    const observed = live.observation(binding)!;
    assert.equal(observed.progressError, 'session_method_refused'); assert.equal(observed.watchError, 'session_method_refused');
    assert.deepEqual(observed.progress, progressResponse);
});

test('progress is polled every 2 s while a job runs and the Explorer is visible; it stops when hidden or idle', async () => {
    let state = 'running', reads = 0;
    const session = { control: async (method: any) => {
        if (method.watch) { return structuredClone(watchResponse); }
        reads++;
        const response = structuredClone(progressResponse) as any;
        response.observation.job.state = state; if (state !== 'running') { response.pending_job = null; }
        return response;
    } };
    const clock = manualTimers(), live = new LiveStatus(session, () => {}, clock.timers);
    live.track([binding]); await live.read();
    assert.equal(reads, 1); assert.equal(clock.pending.size, 0, 'nothing polls while the Explorer is hidden');
    const visible = live.retain();
    assert.equal(clock.pending.size, 1); assert.equal([...clock.pending.values()][0].at, 2000);
    clock.advance(1999); await settle(); assert.equal(reads, 1);
    clock.advance(1); await settle(); assert.equal(reads, 2, 'one read at the interval');
    clock.advance(2000); await settle(); assert.equal(reads, 3);
    visible.dispose(); assert.equal(clock.pending.size, 0, 'hidden stops the poll');
    const again = live.retain(); state = 'succeeded';
    clock.advance(2000); await settle(); assert.equal(reads, 4);
    clock.advance(10_000); await settle(); assert.equal(reads, 4, 'idle work stops the poll'); assert.equal(live.polling, false);
    again.dispose();
});

test('session status and job events are recorded as activity and refresh the live reads', async () => {
    let reads = 0;
    const session = { control: async (method: any) => { reads++; return method.progress ? structuredClone(progressResponse) : structuredClone(watchResponse); } };
    const clock = manualTimers(), live = new LiveStatus(session, () => {}, clock.timers);
    live.track([binding]);
    const cursor = { service_instance: '33'.repeat(16), window_id: '44'.repeat(16), native: {} } as any;
    live.event({ kind: 'job', cursor, job: { ...runningJob, state: 'failed', outcome: { code: 'work_stalled' } } });
    await settle();
    assert.equal(reads, 0, 'hidden: an event reads nothing');
    const visible = live.retain();
    await settle();
    assert.equal(reads, 2, 'showing the Explorer reads the outdated observations once (Progress and Watch)');
    live.event({ kind: 'status', cursor, status: { project: binding } } as any);
    await settle();
    assert.equal(reads, 4, 'visible: an event reads Progress and Watch again');
    live.event({ kind: 'assistance_draft', cursor, draft: {} });
    await settle();
    assert.equal(reads, 4, 'drafts do not refresh status');
    visible.dispose();
    assert.equal(live.events.length, 2);
    const record = live.events[0];
    assert.deepEqual({ ...record, native: undefined }, { kind: 'job', observed_unix_ms: 0, project: binding, job_kind: 'reconcile', state: 'failed', job, code: 'work_stalled', native: undefined });
    assert.deepEqual(record.native, { kind: 'job', cursor, job: { ...runningJob, state: 'failed', outcome: { code: 'work_stalled' } } }, 'the complete event is kept');
});

test('the header names running work, a pause and a missing index set; index unavailable stays for a native unavailable state', () => {
    const ready = statusSummary({ index_state: 'available', semantic_readiness: { state: 'ready' }, freshness: { state: 'idle' } });
    const running = { jobState: 'running', committedChanges: 992, capturedFiles: 2016 };
    assert.deepEqual(headline('ready', ready, 1, headlineWork(running)), { word: 'indexing 992/2,016 files', severity: 'info' });
    assert.deepEqual(headline('ready', undefined, 1, headlineWork({ jobState: 'running' })), { word: 'indexing…', severity: 'info' });
    assert.deepEqual(headline('ready', ready, 1, headlineWork({ ...running, paused: true })), { word: 'paused', severity: 'info' });
    const notReadable = statusSummary({ index_state: 'unknown', index_error: { code: 'index_read_unavailable', message: 'No index set.' },
        work: { schema: 1, project_paused: false, progress: progressResponse.observation } });
    assert.equal(notReadable.indexError, 'index_read_unavailable');
    assert.equal(notReadable.workJobState, 'running'); assert.equal(notReadable.workCommitted, 992); assert.equal(notReadable.workCaptured, 2016); assert.equal(notReadable.workPhase, 'publishing');
    assert.deepEqual(notReadable.work?.progress, progressResponse.observation, 'the complete native work observation is kept');
    assert.deepEqual(headline('ready', notReadable, 1), { word: 'indexing 992/2,016 files', severity: 'info' }, 'running work in the status reply is named');
    const idleNotReadable = statusSummary({ index_state: 'unknown', index_error: { code: 'index_read_unavailable' } });
    assert.deepEqual(headline('ready', idleNotReadable, 1), { word: 'not readable: no index set', severity: 'warn' });
    assert.deepEqual(headline('ready', undefined, 1, headlineWork(undefined, { state: 'not_readable', indexId: 'idx_' + '12'.repeat(16), code: 'index_read_unavailable' })),
        { word: 'not readable: no index set', severity: 'warn' }, 'the Explorer route names the cause before any status read');
    assert.deepEqual(headline('ready', statusSummary({ index_state: 'unavailable' }), 1), { word: 'index unavailable', severity: 'warn' });
    assert.deepEqual(headline('ready', statusSummary({ index_state: 'unknown', index_error: { code: 'index_worker_unavailable' } }), 1),
        { word: 'not readable (index_worker_unavailable)', severity: 'warn' }, 'another known cause is named, not collapsed');
});

test('consoleState builds rows from the live reads and this window events (vscode stubbed)', async () => {
    const load = (Module as any)._load;
    (Module as any)._load = function (request: string, ...rest: unknown[]) { return request === 'vscode' ? {} : load.call(this, request, ...rest); };
    try {
        // eslint-disable-next-line @typescript-eslint/no-require-imports
        const { consoleState } = require('../airkctl/features/explorerHost') as typeof import('../airkctl/features/explorerHost');
        const session = { control: async (method: any) => method.progress ? structuredClone(progressResponse) : structuredClone(watchResponse) };
        const live = new LiveStatus(session, () => {}, manualTimers().timers);
        live.track([binding]); await live.read();
        const ready = { kind: 'ready', version: '1.8.34', health: { instance: '55'.repeat(16) }, capabilities: [] } as any;
        const events = [{ kind: 'job', project: binding, job_kind: 'reconcile', state: 'running', observed_unix_ms: 5 }];
        const state = consoleState(ready, [{ project: binding, root: '/src/demo', data_root: '/data/demo', collection: 'demo' }], true, 4, value => live.observation(value), events);
        assert.deepEqual(state.page!.projects[0].progress, progressResponse.observation);
        assert.deepEqual(state.page!.projects[0].watch, watchResponse.observation);
        assert.deepEqual(state.events, events);
        const [summary] = projectSummaries(state);
        assert.deepEqual([summary.jobState, summary.watching, summary.paused, summary.committedChanges], ['running', true, false, 992]);
        const stopped = consoleState({ kind: 'stopped', explicitlyStopped: false } as any, [], true, 5, value => live.observation(value), events);
        assert.deepEqual([stopped.page, stopped.events], [undefined, []], 'no rows or events are shown without a ready service');
    } finally { (Module as any)._load = load; }
});
