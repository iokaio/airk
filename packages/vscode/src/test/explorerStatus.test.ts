// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { headline, statusSummary } from '../explorerStatus';

test('airk_status maps to separate readiness, freshness and semantic observations', () => {
    const summary = statusSummary({
        index_state: 'available', files: 12, chunks: 40,
        publication: { token: 'airk-r1:tok', generation: 3, collection: 'demo' },
        semantic_readiness: { state: 'partial', completed_artifacts: 4, selected_artifacts: 12 },
        freshness: { state: 'idle', live_source_verified: false },
        vectors_enabled: true, local_model_ready: true,
        runtime: { versions_match: null }, work: { project_paused: false, progress: { phase: 'extract' } },
    }, 5);
    assert.deepEqual(summary, { indexState: 'available', semantic: 'partial', freshness: 'idle', observedAt: 5, publication: 'airk-r1:tok',
        files: 12, chunks: 40, semanticCompleted: 4, semanticSelected: 12, liveSourceVerified: false, vectorsEnabled: true,
        localModelReady: true, versionsMatch: null, workPhase: 'extract', workPaused: false });
    const unavailable = statusSummary({ index_state: 'unavailable', index_error: { code: 'index_unavailable', message: 'private detail' }, files: null, publication: null }, 1);
    assert.deepEqual(unavailable, { indexState: 'unavailable', semantic: 'unknown', freshness: 'unknown', observedAt: 1, indexError: 'index_unavailable' });
    assert.equal(unavailable.files, undefined, 'null counts stay unknown, never zero');
    assert.equal(statusSummary({ index_state: 'odd', semantic_readiness: { state: 'bogus' }, freshness: { state: 'great' } }).semantic, 'unknown');
});

test('the headline reports the worst condition and never merges states into OK', () => {
    const ready = statusSummary({ index_state: 'available', semantic_readiness: { state: 'ready' }, freshness: { state: 'idle' } });
    assert.deepEqual(headline('ready', ready, 1), { word: 'ready', severity: 'ok' });
    assert.equal(headline('untrusted', ready, 1).word, 'untrusted');
    assert.equal(headline('incompatible', ready, 1).word, 'incompatible');
    assert.equal(headline('disconnected', ready, 1).word, 'disconnected');
    assert.equal(headline('ready', undefined, 0).word, 'no projects');
    assert.equal(headline('ready', statusSummary({ index_state: 'unavailable' }), 1).word, 'index unavailable');
    assert.equal(headline('ready', { ...ready, semantic: 'failed' }, 1).severity, 'error');
    assert.equal(headline('ready', { ...ready, freshness: 'running' }, 1).word, 'indexing');
    assert.equal(headline('degraded', ready, 1).word, 'degraded');
});
