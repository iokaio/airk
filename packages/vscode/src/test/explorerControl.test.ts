// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { connectionOf, ExplorerControl, projectSummaries } from '../explorerControl';
import { ServiceConsoleState } from '../serviceConsoleView';

const project = '12'.repeat(32), other = '34'.repeat(16) + '56'.repeat(16);
function state(overrides: Partial<ServiceConsoleState> = {}): ServiceConsoleState {
    const observation = (value?: any) => ({ state: value ? 'live' as const : 'unavailable' as const, checkedAt: 1, ...(value ? { value } : {}) });
    return {
        revision: 'r', trusted: true, busy: false, authority: 'local', version: '1.5.0', connected: true,
        health: observation({ observation: { policy: { accepting: true } } }), resources: observation(), model: observation(), sources: observation(),
        bootstrap: observation({ selectedVersion: '1.5.0' }), events: [], historyOmitted: false, selected: new Set(), pending: [], includePaths: false,
        currentProjects: new Set([project]),
        page: { revision: 4, cached: true, observed_unix_ms: 99, after: null, projects: [
            { project: { registration: { binding: project, root: '/work/atlas-service', data_root: '/data', collection: 'atlas' }, paused: false },
                watch: { enabled: true }, progress: { job: { state: 'running', kind: 'index' }, progress: { phase: 'extract' } },
                inspection: { observed_unix_ms: 42, snapshot: { publication: 'airk-r1:tok', indexed_files: 0, semantic: { state: 'partial' }, index_state: 'ready' } } },
            { project: { registration: { binding: other, root: '', collection: 'shared' } } },
            { project: { registration: { binding: '../escape', root: '/x', collection: 'bad' } } },
        ] },
        ...overrides,
    } as ServiceConsoleState;
}

test('cached console rows become project summaries without inventing values', () => {
    const [first, second, ...rest] = projectSummaries(state());
    assert.equal(rest.length, 0, 'rows without a valid native binding are dropped');
    assert.deepEqual(first, { project, collection: 'atlas', name: 'atlas-service', root: '/work/atlas-service', current: true, paused: false,
        watching: true, publication: 'airk-r1:tok', indexedFiles: 0, semantic: 'partial', indexState: 'ready', jobState: 'running',
        jobKind: 'index', phase: 'extract', observedAt: 42 });
    assert.deepEqual(second, { project: other, collection: 'shared', name: 'shared', root: '', current: false });
    assert.equal(second.indexedFiles, undefined, 'unknown counts stay unknown, never zero');
    assert.deepEqual(projectSummaries(state({ inactiveProjects: true })), []);
});

test('connection state separates trust, version, acceptance and reachability', () => {
    assert.equal(connectionOf(state(), '1.5.0'), 'ready');
    assert.equal(connectionOf(state({ trusted: false }), '1.5.0'), 'untrusted');
    assert.equal(connectionOf(state(), '1.6.0'), 'incompatible');
    assert.equal(connectionOf(state({ health: { state: 'live', checkedAt: 1, value: { observation: { policy: { accepting: false } } } } }), '1.5.0'), 'degraded');
    assert.equal(connectionOf(state({ connected: false, busy: true, health: { state: 'unavailable', checkedAt: 1 } }), '1.5.0'), 'connecting');
    assert.equal(connectionOf(state({ connected: false, health: { state: 'failed', checkedAt: 1, code: 'x' } }), '1.5.0'), 'disconnected');
});

test('control actions run through the shared console controller', async () => {
    const runs: unknown[] = [];
    let retained = 0;
    const control = new ExplorerControl({ controller: { state: state() }, run: async (action, fields) => { runs.push([action, fields]); },
        retain: () => { retained++; return { dispose() { retained--; } }; }, onDidChangeState: () => ({ dispose() {} }) }, '1.5.0');
    await control.run('index', { project });
    await control.refresh();
    assert.deepEqual(runs, [['index', { project }], ['refresh', undefined]]);
    const handle = control.retain();
    assert.equal(retained, 1);
    handle.dispose();
    assert.equal(retained, 0);
    assert.equal(control.observedAt(), 99);
    assert.equal(control.connection(), 'ready');
});
