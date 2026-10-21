// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { collectDiagnostics, diagnosticError, DiagnosticHistory, DiagnosticRunner, verifyDiagnosticIntegrity } from '../diagnostics';
import { LaunchSpec } from '../model';
import { NativeProcessError } from '../native';

const spec: LaunchSpec = { command: path.resolve('fixture/airk'), root: path.resolve('fixture/project'), dataDir: path.resolve('fixture/state'), collection: 'project-fixture', name: 'Fixture' };
const discovery = { state: 'ready', processed_artifacts: 100, committed_artifacts: 100, total_artifacts: 100, failure: null };
const semantic = { state: 'ready', selected_artifacts: 100, completed_artifacts: 100, completed_units: 800, failure: null };
function replies(): Record<string, unknown> {
    return {
        '--version': 'airk 1.2.4\n',
        status: { collection: spec.collection, generation: 9, files: 100, indexed_vectors: 800, publication: { token: 'fixture-revision', generation: 9 },
            discovery_readiness: { ...discovery }, semantic_readiness: { ...semantic }, source_roots: { private: 'PRIVATE_SOURCE_ROOT' }, source: 'PRIVATE_SOURCE_TEXT' },
        'schedule status': { collection: spec.collection, root: spec.root, paused: false, blocked: false, error: null, requested_revision: 1, completed_revision: 1, last_report: 'PRIVATE_SOURCE_REPORT' },
        'collections vectors show': { enabled: true, published: true, indexed_vectors: 800, pending_reindex: false, discovery_readiness: { ...discovery }, semantic_readiness: { ...semantic } },
        'collections extraction show': { configured_detail: 'agent_lean', published_detail: 'agent_lean', pending_reindex: false, policy: { private: 'PRIVATE_POLICY_CONTENT' } },
        'collections list': [{ collection: 'other-private-collection', disk_bytes: 999, private: 'PRIVATE_OTHER_COLLECTION' }, { collection: spec.collection, disk_bytes: 1000, indexed_files: 100, status: 'indexed' }],
        'collections diagnose': { publication: { collection: spec.collection, token: 'fixture-revision', generation: 9 }, from_start: true, total_files: 100,
            checked_files: 100, verified_sources: 100, verified_extractions: 100, checked_source_bytes: 1000, checked_extraction_bytes: 2000,
            failed_objects: 0, failures: [], failures_truncated: 0, skipped_large: 0, complete: true, next_after: null }
    };
}
const fakeRunner = (responses: Record<string, unknown>): DiagnosticRunner => async (_spec, command, extra) => {
    const result = responses[[command, ...extra].join(' ')]; if (result instanceof Error) { throw result; } return result;
};
const processError = (stderr: string, code: number = 1) => new NativeProcessError(spec.command, Object.assign(new Error('Command failed'), { code }), stderr, 25, 10000);
const gate = 'Error: current index is busy applying an update\n\nCaused by:\n    lock acquisition failed because the operation would block';

test('diagnostics use bounded read-only collection probes and whitelist metadata', async () => {
    const responses = replies(), calls: string[] = []; let active = 0, peak = 0;
    const report = await collectDiagnostics(spec, { expectedVersion: '1.2.4', runner: async (selected, command, extra, timeout) => {
        assert.equal(selected, spec); assert.equal(timeout, 10000); calls.push([command, ...extra].join(' '));
        active++; peak = Math.max(peak, active); await new Promise(resolve => setImmediate(resolve)); active--;
        return responses[[command, ...extra].join(' ')];
    }, events: [{ at: '2026-10-20T20:00:00Z', kind: 'index_failed', helperPath: '/older/airk', error: { kind: 'error', message: 'historical retained object missing' } }] });
    assert.deepEqual(calls, Object.keys(responses)); assert.equal(peak, 3);
    assert.equal(report.assessment.state, 'ready'); assert.equal(report.assessment.recommendedAction, 'none');
    assert.equal(report.recentEvents.length, 1, 'Historical failure is retained without overriding a healthy current observation');
    assert.equal(report.probes.status.value?.files, 100); assert.equal(report.probes.storage.value?.disk_bytes, 1000);
    assert.ok(!JSON.stringify(report).includes('PRIVATE_')); assert.ok(!JSON.stringify(report).includes('other-private-collection'));
    assert.equal(report.binding.root, spec.root); assert.equal(report.binding.helperPath, spec.command);
});

test('native scheduler canonical Windows paths match the selected Node workspace binding', { skip: process.platform !== 'win32' }, async () => {
    const responses = replies();
    (responses['schedule status'] as any).root = path.toNamespacedPath(spec.root);
    const report = await collectDiagnostics(spec, { runner: fakeRunner(responses) });
    assert.equal(report.probes.schedule.state, 'ok'); assert.equal(report.assessment.state, 'ready');
});

test('diagnostic cancellation stops queued probes when trust or collection binding changes', async () => {
    const responses = replies(), calls: string[] = [];
    let allowed = true;
    const report = await collectDiagnostics(spec, { shouldContinue: () => allowed, runner: async (_selected, command, extra) => {
        const key = [command, ...extra].join(' '); calls.push(key);
        await new Promise(resolve => setImmediate(resolve));
        allowed = false;
        return responses[key];
    } });
    assert.deepEqual(calls, ['--version', 'status', 'schedule status']);
    assert.equal(report.probes.helper.state, 'ok'); assert.equal(report.probes.status.state, 'ok');
    for (const name of ['vectors', 'extraction', 'storage', 'integrity'] as const) { assert.equal(report.probes[name].state, 'unknown'); }
    assert.equal(report.assessment.state, 'unknown');
    await collectDiagnostics(spec, { shouldContinue: () => false, runner: async () => { assert.fail('Closed collection must not launch a helper'); } });
});

test('diagnostics distinguish exact contention from permission errors, missing objects and unknown observations', async () => {
    for (const scenario of ['busy', 'permission', 'missing', 'unindexed', 'invalid', 'paused', 'blocked', 'partial', 'unregistered', 'policy-pending', 'mismatch', 'editor-failed', 'editor-paused'] as const) {
        const responses = replies();
        if (scenario === 'busy') { responses.status = processError(gate); }
        if (scenario === 'permission') { responses.status = processError(gate.replace('lock acquisition failed because the operation would block', 'Permission denied (os error 13)')); }
        if (scenario === 'missing') { responses.status = processError('Error: retained object missing; previous generation cannot be reconstructed'); }
        if (scenario === 'unindexed') { responses.status = processError('Error: workspace is not indexed; run airk index first'); }
        if (scenario === 'invalid') { responses.status = { collection: 'foreign', generation: 7, files: 1 }; }
        if (scenario === 'paused') { (responses['schedule status'] as any).paused = true; }
        if (scenario === 'blocked') { Object.assign(responses['schedule status'] as object, { blocked: true, error: 'fixture failure' }); }
        if (scenario === 'partial' || scenario === 'unregistered') { (responses.status as any).semantic_readiness = { ...semantic, state: 'partial', completed_artifacts: 50 }; }
        if (scenario === 'unregistered') { responses['schedule status'] = null; }
        if (scenario === 'policy-pending') { (responses['collections extraction show'] as any).pending_reindex = true; }
        const report = await collectDiagnostics(spec, { expectedVersion: scenario === 'mismatch' ? '1.2.5' : '1.2.4', runner: fakeRunner(responses),
            ...(scenario === 'editor-failed' ? { editor: { paused: false, state: 'failed', busy: false, pending: true } } : {}),
            ...(scenario === 'editor-paused' ? { editor: { paused: true, busy: false, pending: true } } : {}) });
        const expected = { busy: 'busy', permission: 'failed', missing: 'failed', unindexed: 'not_indexed', invalid: 'unknown', paused: 'paused', blocked: 'blocked',
            partial: 'building', unregistered: 'blocked', 'policy-pending': 'building', mismatch: 'failed', 'editor-failed': 'blocked', 'editor-paused': 'paused' }[scenario];
        assert.equal(report.assessment.state, expected, scenario);
        if (scenario === 'missing') { assert.equal(report.assessment.recommendedAction, 'retry'); }
        if (scenario === 'permission') { assert.equal(report.assessment.recommendedAction, 'review'); }
        if (scenario === 'unregistered' || scenario === 'policy-pending') { assert.equal(report.assessment.recommendedAction, 'retry'); }
    }
});

test('unverified helpers never run diagnostic subprocesses and process metadata survives bounded formatting', async () => {
    let calls = 0;
    const report = await collectDiagnostics(spec, { helperError: new Error('cached helper digest mismatch'), runner: async () => { calls++; throw new Error('Unexpected subprocess'); } });
    assert.equal(calls, 0); assert.equal(report.probes.helper.state, 'failed'); assert.equal(report.probes.status.state, 'unknown');
    const error = new NativeProcessError(spec.command, Object.assign(new Error('Command failed'), { code: -1073741819, killed: false }), 'x'.repeat(20000), 1234, 10000);
    const formatted = diagnosticError(error);
    assert.equal(formatted.kind, 'native_process'); assert.equal(formatted.code, -1073741819); assert.equal(formatted.elapsedMs, 1234); assert.equal(formatted.timeoutMs, 10000);
    assert.equal(formatted.timedOut, false); assert.ok(formatted.message.length < 1100); assert.ok(formatted.stderr!.length < 2100);
    const responses = replies(); responses.status = new Error(gate);
    assert.equal((await collectDiagnostics(spec, { runner: fakeRunner(responses) })).probes.status.state, 'unknown', 'An arbitrary Error string must not authorize a busy retry or establish a native failure');
    responses.status = new NativeProcessError(spec.command, Object.assign(new Error('Command failed'), { killed: true }), '', 10001, 10000);
    const timedOut = await collectDiagnostics(spec, { runner: fakeRunner(responses) });
    assert.equal(timedOut.probes.status.state, 'unknown'); assert.equal(timedOut.assessment.state, 'unknown', 'A diagnostic time budget cannot establish a broken index');
});

test('retained input verification distinguishes missing inputs, partial checks and mismatched publications', async () => {
    for (const scenario of ['missing', 'invalid', 'partial', 'head-changed', 'unbound-ready', 'bad-count'] as const) {
        const responses = replies(), integrity = responses['collections diagnose'] as any;
        if (scenario === 'missing' || scenario === 'invalid') {
            Object.assign(integrity, { verified_sources: 99, failed_objects: 1, failures: [{ path: 'fictional.txt', component: 'source', kind: scenario }] });
        }
        if (scenario === 'partial') { Object.assign(integrity, { checked_files: 50, verified_sources: 50, verified_extractions: 50, complete: false, next_after: 'file_50.txt' }); }
        if (scenario === 'head-changed') { integrity.publication.token = 'different-publication'; }
        if (scenario === 'unbound-ready') { (responses.status as any).semantic_readiness = { ...semantic, state: 'partial', completed_artifacts: 50 }; }
        if (scenario === 'bad-count') { integrity.checked_files = NaN; }
        const report = await collectDiagnostics(spec, { runner: fakeRunner(responses) });
        assert.equal(report.assessment.state, scenario === 'missing' || scenario === 'invalid' ? 'failed' : scenario === 'unbound-ready' ? 'building' : 'unknown', scenario);
        if (scenario === 'missing') { assert.equal(report.assessment.recommendedAction, 'retry'); }
        if (scenario === 'invalid') { assert.equal(report.assessment.recommendedAction, 'review'); }
    }
});

test('explicit retained verification completes bounded pages and stops safely on cancellation or revision expiry', async () => {
    const responses = replies(), first = responses['collections diagnose'] as any;
    Object.assign(first, { total_files: 3, checked_files: 1, verified_sources: 1, verified_extractions: 1, complete: false, next_after: 'a.txt' });
    (responses.status as any).files = 3;
    const report = await collectDiagnostics(spec, { runner: fakeRunner(responses) });
    assert.equal(report.assessment.state, 'unknown');
    const progress: number[] = []; let calls = 0;
    const verified = await verifyDiagnosticIntegrity(spec, report, { runner: async (selected, command, extra, timeout) => {
        assert.equal(selected, spec); assert.equal(command, 'collections'); assert.ok(timeout <= 10000);
        assert.deepEqual(extra, ['diagnose', '--revision', 'fixture-revision', '--after', calls ? 'b.txt' : 'a.txt']);
        calls++;
        return { ...first, from_start: false, complete: calls === 2, next_after: calls === 2 ? null : 'b.txt' };
    }, onProgress: value => { progress.push(Number(value.probes.integrity.value?.checked_files)); } });
    assert.equal(calls, 2); assert.deepEqual(progress, [2, 3]); assert.equal(verified.assessment.state, 'ready');
    assert.equal(verified.probes.integrity.value?.checked_source_bytes, 3000);
    const cancelled = await verifyDiagnosticIntegrity(spec, report, { shouldContinue: () => false, runner: async () => { throw new Error('Must not run'); } });
    assert.equal(cancelled.probes.integrity.value?.interrupted_reason, 'cancelled'); assert.equal(cancelled.assessment.state, 'unknown');
    const expired = await verifyDiagnosticIntegrity(spec, report, { runner: async () => { throw processError('Error: retained integrity revision changed; restart verification at the current head'); } });
    assert.equal(expired.probes.integrity.state, 'unknown'); assert.equal(expired.probes.integrity.value?.next_after, null);
    assert.equal(expired.probes.integrity.value?.interrupted_reason, 'revision_changed');
    await assert.rejects(verifyDiagnosticIntegrity({ ...spec, collection: 'other' }, report), /current collection/);
});

test('diagnostic history survives helper upgrades, isolates bindings and bounds concurrent event writes', async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-diagnostics-'));
    try {
        const history = new DiagnosticHistory(directory);
        await Promise.all(Array.from({ length: 35 }, (_, index) => history.append(spec, { kind: 'index_failed', helperVersion: '1.2.3', action: String(index), error: processError('x'.repeat(20000)) })));
        const upgraded = { ...spec, command: '/new-helper/airk' };
        await new DiagnosticHistory(directory).append(upgraded, { kind: 'helper', helperVersion: '1.2.4', error: processError('fixture native crash', -1073741819) });
        const events = await new DiagnosticHistory(directory).read(upgraded);
        assert.ok(events.length <= 24); assert.equal(events.at(-1)?.helperVersion, '1.2.4'); assert.equal(events.at(-1)?.helperPath, upgraded.command);
        assert.equal(events.at(-1)?.error?.code, -1073741819, 'Signed Windows exception status must survive reload');
        assert.ok(events.some(event => event.helperVersion === '1.2.3'));
        for (const changed of [{ ...spec, collection: 'other' }, { ...spec, root: path.resolve('other-root') }, { ...spec, dataDir: path.resolve('other-state') }]) { assert.deepEqual(await history.read(changed), []); }
        const files = await fs.readdir(directory); assert.equal(files.length, 1);
        assert.ok((await fs.stat(path.join(directory, files[0]))).size <= 128 * 1024);
        await fs.writeFile(path.join(directory, files[0]), 'x'.repeat(128 * 1024 + 1));
        await assert.rejects(history.read(spec), /bounded regular file/);
    } finally {
        const resolved = await fs.realpath(directory); assert.equal(path.dirname(resolved), await fs.realpath(os.tmpdir())); assert.ok(path.basename(resolved).startsWith('airk-diagnostics-'));
        await fs.rm(resolved, { recursive: true, force: true });
    }
});

test('closing diagnostic history drains accepted writes and prevents late events from recreating files', async () => {
    const directory = await fs.mkdtemp(path.join(os.tmpdir(), 'airk-diagnostics-close-'));
    const history = new DiagnosticHistory(directory);
    try {
        const writes = Array.from({ length: 8 }, (_, index) => history.append(spec, { kind: 'repair', action: String(index) }));
        await history.close(); await Promise.all(writes);
        const events = await history.read(spec);
        assert.equal(events.length, 8); assert.equal(events.at(-1)?.action, '7');
        const files = await fs.readdir(directory);
        assert.equal(files.length, 1); assert.ok(files[0].endsWith('.json'));
        await fs.unlink(path.join(directory, files[0]));
        await history.append(spec, { kind: 'index_completed' });
        await history.flush();
        assert.deepEqual(await fs.readdir(directory), [], 'A late completion after disposal cannot race fixture or editor teardown');
    } finally {
        await history.close();
        const resolved = await fs.realpath(directory); assert.equal(path.dirname(resolved), await fs.realpath(os.tmpdir())); assert.ok(path.basename(resolved).startsWith('airk-diagnostics-close-'));
        await fs.rm(resolved, { recursive: true, force: true });
    }
});
