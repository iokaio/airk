// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { managementView } from '../explorerManagement';
import { ServiceConsoleState } from '../serviceConsoleView';

const project = '12'.repeat(32), job = '34'.repeat(32), pending = '56'.repeat(32), plan = '78'.repeat(32), intent = '9a'.repeat(16);
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
