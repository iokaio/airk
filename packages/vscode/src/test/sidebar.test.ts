// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { Coverage } from '../coverage';
import { SidebarState, renderSidebar, sidebarAction, sidebarContent, sidebarControlGroups } from '../sidebar';

const report: Coverage = { collection: 'a', root: '/project/a', generation: 1, indexed: true, inventory_revision: 1,
    scanned_at_unix_ms: 1, scan_complete: true, eligible: 1000, current: 999, stale: 1, missing: 0, extraction_failed: 0,
    capacity_blocked: 0, unreadable: 0, excluded: 0, unsupported: 3, pending_removals: 0, budget_exhausted: false, percent: 99 };
const state: SidebarState = { trusted: true, paused: false, changingStorage: false, mode: 'debounced',
    groups: [{ label: 'Setup and reports', commands: [['Connect agent', 'airk.setupAgent', 'Preview configuration']] }],
    collections: [{ collection: 'a', name: 'Project A', root: '/project/a', dataDir: '/data', state: 'indexing',
        coverage: { report, scanning: false, pending: false }, diskBytes: 1048576 }] };

test('vector switch reflects persisted opt-out, readiness and trust and validates collection-bound actions', () => {
    const sample = structuredClone(state);
    sample.collections[0].state = 'current';
    sample.collections[0].vectors = { enabled: true, published: false, indexed_vectors: 0, pending_reindex: true, default_search_mode: 'hybrid' };
    assert.match(sidebarContent(sample), /data-vectors="true" checked>/);
    assert.match(sidebarContent(sample), /waiting for model setup/);
    sample.collections[0].vectors.semantic_readiness = {
        state: 'partial', selected_artifacts: 33, completed_artifacts: 32,
        selected_units: null, completed_units: 32, failure: null
    };
    assert.match(sidebarContent(sample), /Semantic indexing 97%: 32 of 33 selected files complete; 1 remaining/);
    assert.match(sidebarContent(sample), /value="32" aria-label="Published semantic indexing progress"/);
    sample.collections[0].vectors.semantic_readiness.selected_artifacts = 1000;
    sample.collections[0].vectors.semantic_readiness.completed_artifacts = 999;
    assert.match(sidebarContent(sample), /Semantic indexing 99%: 999 of 1000/);
    sample.collections[0].vectors.enabled = false;
    assert.doesNotMatch(sidebarContent(sample), /data-vectors="true" checked/);
    assert.match(sidebarContent(sample), /Off: keyword search/);
    assert.match(sidebarContent({ ...sample, trusted: false }), /data-vectors="true" disabled/);
    const action = { type: 'vectors', collection: 'a', dataDir: '/data', enabled: false };
    assert.deepEqual(sidebarAction(action, new Set()), action);
    assert.equal(sidebarAction({ ...action, enabled: 'false' }, new Set()), undefined);
    assert.equal(sidebarAction({ ...action, dataDir: undefined }, new Set()), undefined);
});

test('tools button opens a collection-bound console and is disabled without trust or during storage changes', () => {
    assert.match(sidebarContent(state), /data-collection="a" data-storage="\/data" data-action="tools">Open AIRK Tools/);
    assert.match(sidebarContent({ ...state, trusted: false }), /data-action="tools" disabled/);
    assert.match(sidebarContent({ ...state, changingStorage: true }), /data-action="tools" disabled/);
    const action = { type: 'collection', action: 'tools', collection: 'a', dataDir: '/data' };
    assert.deepEqual(sidebarAction(action, new Set()), action);
    assert.equal(sidebarAction({ ...action, dataDir: undefined }, new Set()), undefined);
});

test('semantic indexing remains visible independently of complete saved-file coverage', () => {
    const sample = structuredClone(state);
    sample.collections[0].state = 'enriching';
    sample.collections[0].semanticElapsedSeconds = 125;
    sample.collections[0].semanticStartedAt = 1000;
    sample.collections[0].vectors = { enabled: true, published: true, indexed_vectors: 416,
        pending_reindex: false, default_search_mode: 'hybrid', semantic_readiness: {
            state: 'partial', selected_artifacts: 745, completed_artifacts: 416,
            selected_units: null, completed_units: 5000, failure: null } };
    sample.collections[0].semanticProgress = { phase: 'enrichment_prepared', selected_artifacts: 745, completed_artifacts: 416 };
    sample.collections[0].coverage!.report!.current = 1000;
    const html = sidebarContent(sample);
    assert.match(html, /100%/);
    assert.match(html, /Building semantic index/);
    assert.match(html, /Semantic indexing 56%: 416 of 745 selected files complete; 329 remaining/);
    assert.match(html, /Computing vectors and publishing this batch/);
    assert.match(html, /data-semantic-started="1000">2m 5s/);
    assert.match(html, /value="416" aria-label="Published semantic indexing progress"/);
    assert.doesNotMatch(html, /stops after twenty minutes/);
    assert.doesNotMatch(html, /Indexing saved files/);
    assert.match(html, /data-action="index"[^>]*disabled/);
    sample.collections[0].state = 'semantic-pending';
    assert.match(sidebarContent(sample), /Semantic indexing pending/);
});

test('initial indexing shows committed MCP evidence and a quiet scan warning', () => {
    const sample = structuredClone(state);
    sample.collections[0].indexActivity = { phase: 'base_committed', revision: 'airk-r1:fixture',
        processed_artifacts: 64, committed_artifacts: 32, selected_artifacts: 32,
        completed_artifacts: 8, elapsedSeconds: 181, quietSeconds: 130 };
    sample.collections[0].mcp = { phase: 'verified', tools: Array(15).fill('tool'), indexState: 'published' };
    const html = sidebarContent(sample);
    assert.match(html, /32 files committed, 64 processed/);
    assert.match(html, /Published evidence is available to MCP now/);
    assert.match(html, /No new progress for 2 minutes/);
    assert.match(html, /Verified · 15 tools available/);
    sample.collections[0].indexActivity.revision = null;
    sample.collections[0].mcp.indexState = 'not_indexed';
    assert.match(sidebarContent(sample), /MCP can start; search awaits the first publication/);
    assert.match(sidebarContent(sample), /indexing pending/);
    sample.collections[0].indexActivity.phase = 'waiting';
    assert.match(sidebarContent(sample), /Waiting for the shared indexing worker/);
    assert.doesNotMatch(sidebarContent(sample), /No new progress|Published evidence remains available/);
});

test('CLI getting-started note reports setup failures and keeps repair behind workspace trust', () => {
    assert.match(sidebarContent({ ...state, cliStatus: 'ready' }), /<code>airk --help<\/code>/);
    assert.match(sidebarContent({ ...state, cliStatus: 'ready' }), /Restart external terminal apps/);
    assert.match(sidebarContent({ ...state, cliStatus: 'failed' }), /CLI setup needs attention/);
    assert.match(sidebarContent({ ...state, cliStatus: 'disabled' }), /Automatic CLI installation is off/);
    assert.match(sidebarContent({ ...state, trusted: false }), /data-command="airk.installCli"[^>]*disabled/);
    const allowed = new Set(sidebarControlGroups().flatMap(group => group.commands.map(([, command]) => command)));
    assert.deepEqual(sidebarAction({ type: 'command', command: 'airk.installCli' }, allowed), { type: 'command', command: 'airk.installCli' });
});

test('sidebar shows measured coverage, direct collection controls and no invented indexing percentage', () => {
    const html = sidebarContent(state);
    assert.match(html, /Indexing saved files/); assert.match(html, /99%/);
    assert.match(html, /data-collection="a" data-storage="\/data" data-action="index"[^>]*disabled/);
    assert.match(html, /data-action="status"[^>]*>Details/);
    assert.match(html, /<select id="mode"/); assert.match(html, /data-command="airk.pause"/);
    assert.match(html, /1\.0 MiB/); assert.match(html, /Current/); assert.match(html, /Eligible/);
    const changed = structuredClone(state); changed.collections[0].coverage!.pending = true;
    assert.doesNotMatch(sidebarContent(changed), /<progress/);
    assert.match(sidebarContent(changed), /previous scan/);
    changed.collections[0].coverage = undefined;
    assert.match(sidebarContent(changed), /Coverage unknown/); assert.doesNotMatch(sidebarContent(changed), /<progress/);
});

test('sidebar escapes workspace names, paths and errors and uses a nonce-only content policy', () => {
    const hostile = structuredClone(state);
    hostile.collections[0].name = '<img src=x onerror="attack()">';
    hostile.collections[0].dataDir = '" onmouseover="attack()';
    hostile.collections[0].error = '</p><script>attack()</script>';
    const html = renderSidebar(hostile);
    assert.doesNotMatch(html, /<img|<script>attack|data-storage="" onmouseover/);
    assert.match(html, /&lt;img/); assert.match(html, /&lt;script&gt;/);
    assert.match(html, /default-src 'none'/); assert.match(html, /script-src 'nonce-[0-9a-f]+'/);
    assert.doesNotMatch(html, /unsafe-inline|https:\/\//);
});

test('sidebar preserves unknown coverage and exposes paused, empty and untrusted states', () => {
    const paused = { ...state, paused: true, changingStorage: true };
    assert.match(sidebarContent(paused), /data-command="airk.resume"/);
    assert.match(sidebarContent(paused), /New indexing jobs are held/);
    assert.match(sidebarContent({ ...state, trusted: false, collections: [] }), /Trust this workspace/);
    assert.match(sidebarContent({ ...state, trusted: false, collections: [] }), /<select id="mode" disabled/);
    assert.match(sidebarContent({ ...state, collections: [] }), /No workspace collection/);
});

test('sidebar accepts only known actions and never executes arbitrary editor commands', () => {
    const allowed = new Set(['airk.pause', 'airk.chooseStorage']);
    for (const value of [null, {}, { type: 'command', command: 'workbench.action.terminal.sendSequence' },
        { type: 'mode', value: 'unbounded' }, { type: 'mode', value: ['idle'] }, { type: 'registration', enabled: 'false' },
        { type: 'collection', action: ['index'], collection: 'a', dataDir: '/data' },
        { type: 'collection', action: 'remove', collection: 'a', dataDir: '/data' },
        { type: 'collection', action: 'index', collection: 'a' }]) {
        assert.equal(sidebarAction(value, allowed), undefined);
    }
    assert.deepEqual(sidebarAction({ type: 'mode', value: 'idle' }, allowed), { type: 'mode', value: 'idle' });
    assert.deepEqual(sidebarAction({ type: 'command', command: 'airk.pause', extra: 'discard' }, allowed), { type: 'command', command: 'airk.pause' });
    assert.deepEqual(sidebarAction({ type: 'collection', action: 'index', collection: 'a', dataDir: '/data' }, allowed),
        { type: 'collection', action: 'index', collection: 'a', dataDir: '/data' });
});

test('MCP verification states are collection-scoped and separate from registration and agent connections', () => {
    const sample = structuredClone(state);
    sample.registrationEnabled = false;
    sample.collections[0].mcp = { phase: 'verified', checkedAt: 1000, tools: ['status', 'search'] };
    sample.collections.push({ collection: 'b', name: 'Other', root: '/other', dataDir: '/data', state: 'current' });
    let html = sidebarContent(sample);
    assert.match(html, /Verified · 2 tools available/); assert.match(html, /Last checked:/);
    assert.match(html, /Helper: <strong>Not checked/);
    assert.match(html, /VS Code registration: Off/);
    assert.doesNotMatch(html, /id="registration"[^>]* checked/);
    assert.match(html, /not whether an agent is connected/);
    assert.match(html, /data-command="airk.manageMcp"/);
    for (const phase of ['preparing', 'checking'] as const) {
        sample.collections[0].mcp = { phase };
        html = sidebarContent(sample);
        assert.match(html, /id="a-verify"[^>]* disabled/);
        assert.match(html, /id="a-mcpDetails"[^>]* disabled/);
    }
    sample.collections[0].mcp = { phase: 'failed', error: '<script>failure</script>', checkedAt: 1000 };
    html = sidebarContent(sample);
    assert.match(html, /Verification failed/); assert.match(html, /&lt;script&gt;/);
    assert.doesNotMatch(html, /id="a-verify"[^>]* disabled/);
    assert.deepEqual(sidebarAction({ type: 'registration', enabled: false }, new Set()), { type: 'registration', enabled: false });
});

// Capacity blocks need an immediate recovery control without a pause/resume detour.
test('capacity-blocked collections expose Resume and storage recovery controls', () => {
    const html = renderSidebar({ trusted: true, paused: false, changingStorage: false, mode: 'debounced', groups: sidebarControlGroups(),
        collections: [{ collection: 'fixture', name: 'Fixture', root: '/fixture', dataDir: '/state', state: 'capacity-limited' }] });
    assert.match(html, /data-command="airk.resume"/);
    assert.match(html, /data-command="airk.storageLimits"/);
    assert.match(html, /data-command="airk.currentMaintenance"/);
    assert.doesNotMatch(html, /airk\.(retention|history|checkpoints|pruneIndexes)/);
});


test('detail selection shows configured and published levels and validates messages', () => {
    const sample = structuredClone(state);
    sample.collections[0].state = 'current';
    sample.collections[0].detail = { configured_detail: 'balanced', published_detail: 'detailed', pending_reindex: true };
    const html = sidebarContent(sample);
    assert.match(html, /value="balanced" selected/);
    assert.match(html, /Published: <strong>Detailed \(advanced\)/);
    assert.match(html, /Agent Lean/);
    assert.match(html, /Rich Structure/);
    assert.match(html, /Search Only/);
    assert.match(html, /Change pending indexing/);
    assert.match(html, /Detail change pending/);
    assert.doesNotMatch(html, /Index current/);
    assert.match(html, /data-detail="true"/);
    assert.deepEqual(sidebarAction({ type: 'detail', collection: 'a', dataDir: '/data', level: 'essential' }, new Set()),
        { type: 'detail', collection: 'a', dataDir: '/data', level: 'essential' });
    assert.equal(sidebarAction({ type: 'detail', collection: 'a', dataDir: '/data', level: 'unbounded' }, new Set()), undefined);
    assert.deepEqual(sidebarAction({ type: 'detail', collection: 'a', dataDir: '/data', level: 'agent_lean' }, new Set()),
        { type: 'detail', collection: 'a', dataDir: '/data', level: 'agent_lean' });
    assert.deepEqual(sidebarAction({ type: 'collection', action: 'compression', collection: 'a', dataDir: '/data' }, new Set()),
        { type: 'collection', action: 'compression', collection: 'a', dataDir: '/data' });
});

test('successful indexing does not claim current coverage after new changes or an incomplete scan', () => {
    const sample = structuredClone(state);
    sample.collections[0].state = 'current';
    assert.match(sidebarContent(sample), /Changes detected/);
    sample.collections[0].coverage!.report = { ...report, stale: 0, current: 1000, percent: 100 };
    assert.match(sidebarContent(sample), /Index current/);
    sample.collections[0].coverage!.pending = true;
    assert.match(sidebarContent(sample), /Changes detected/);
    sample.collections[0].coverage!.pending = false;
    sample.collections[0].coverage!.report!.scan_complete = false;
    assert.match(sidebarContent(sample), /Indexed; coverage unverified/);
    sample.collections[0].coverage = undefined;
    assert.match(sidebarContent(sample), /Indexed; coverage unverified/);
});


test('semantic readiness does not imply discovery completed after a failed scan', () => {
    const sample = structuredClone(state);
    sample.collections[0].state = 'failed';
    sample.collections[0].vectors = { enabled: true, published: true, indexed_vectors: 17,
        pending_reindex: false, default_search_mode: 'hybrid',
        semantic_readiness: { state: 'ready', selected_artifacts: 17, completed_artifacts: 17,
            selected_units: null, completed_units: 17, failure: null },
        discovery_readiness: { state: 'failed', processed_artifacts: 18, committed_artifacts: 17,
            total_artifacts: null, failure: 'retained object missing' } };
    const html = sidebarContent(sample);
    assert.match(html, /Indexing failed/);
    assert.match(html, /Semantic evidence ready for committed files; discovery incomplete/);
    assert.match(html, /Discovery stopped after 17 committed files/);
    assert.match(html, /retained object missing/);
    assert.match(html, /data-action="reindex"[^>]*>Reindex/);
    sample.collections[0].vectors.discovery_readiness!.state = 'ready';
    assert.doesNotMatch(sidebarContent(sample), /discovery incomplete/);
});
