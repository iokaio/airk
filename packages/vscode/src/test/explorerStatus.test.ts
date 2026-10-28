// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { headline, indexContents, semanticExplanation, sourceWorkExplanation, statusSummary, workProgressDisplay } from '../explorerStatus';
import type { ProjectSummary, ProjectWorkView } from '../explorerContracts';

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

test('status versions keep frontend and worker provenance separate and never fill missing versions', () => {
    const result = statusSummary({ runtime: { mcp_frontend: { basis: 'client_reported', process: { version: '1.5.5', process_id: 25 } },
        index_worker: { basis: 'running_process', process: { process_id: 45 } } } });
    assert.deepEqual(result.runtimeVersions, [['MCP frontend (airk)', '1.5.5 · client_reported · PID 25'], ['Index worker (airk)', 'not observed · running_process · PID 45']]);
});

test('disabled coverage does not imply saved opt-out and enabling is not readiness', () => {
    assert.match(semanticExplanation({ semantic:'disabled', vectorsEnabled:true, localModelReady:false }).detail,/no model configuration/);
    assert.match(semanticExplanation({ semantic:'disabled', vectorsEnabled:false }).title,/turned off/);
    assert.match(semanticExplanation({ semantic:'pending', vectorsEnabled:true }).title,/pending/);
    assert.match(semanticExplanation({ semantic:'disabled' }).detail,/does not establish/);
});

const vectorWork:ProjectWorkView={current:true,worker:'connected',phase:'preparing',captured:0,extracted:0,committed:32,publications:1,
    semanticUnits:10511,elapsedMs:185585,lastAdvanceMs:3571,wait:'No matching native wait observation',phaseTimes:[]};

test('semantic preparation shows real semantic work rather than zero source-file counters',()=>{
    const view=workProgressDisplay(vectorWork,'vectors','running');
    assert.match(view.title,/semantic index batch/);
    assert.deepEqual(view.metrics,[['Semantic units completed (reported)',10511],['Files committed this attempt',32],['Batches published this attempt',1]]);
    assert.equal(view.stages.find(stage=>stage.active)?.label,'Prepare & build');
    assert.match(view.explanation!,/already indexed content/);
    assert.match(view.explanation!,/earlier attempts/);
    assert.equal(workProgressDisplay({...vectorWork,semanticUnits:undefined},'vectors','running').metrics[0][1],undefined,'unknown semantic work is not zero');
});

test('cleanup retains published work and is active for both source and vector jobs',()=>{
    for(const kind of ['index','vectors']){
        const view=workProgressDisplay({...vectorWork,phase:'retiring'},kind,'running');
        assert.equal(view.title,'Cleaning up after publication');
        assert.equal(view.stages.find(stage=>stage.active)?.label,'Clean up');
        assert.ok(view.metrics.every(([label])=>!label.includes('Captured')&&!label.includes('extracted')));
        assert.match(view.explanation!,/Published results remain separate/);
    }
    assert.ok(workProgressDisplay({...vectorWork,current:false,phase:'retiring'},'vectors','succeeded').stages.every(stage=>!stage.active));
});

test('source preparation and completed or failed attempts retain their observed counters',()=>{
    const view=workProgressDisplay({...vectorWork,captured:10,extracted:8,total:12},'reconcile','running');
    assert.equal(view.stages.find(stage=>stage.active)?.label,'Prepare');
    assert.deepEqual(view.metrics[0],['Captured files',10]);
    for(const state of ['failed','succeeded']){
        const finished=workProgressDisplay({...vectorWork,phase:'finished'},'vectors',state);
        assert.equal(finished.title,state==='failed'?'Attempt failed':'Latest attempt completed');assert.ok(finished.stages.every(stage=>!stage.active));
        assert.equal(finished.metrics[0][1],10511);
    }
    assert.equal(workProgressDisplay(vectorWork,'model_setup','running').metrics.length,0);
    assert.equal(workProgressDisplay({...vectorWork,phase:'finished'},'vectors','running').title,'Finishing this attempt','a final worker sample does not invent a successful job outcome');
});

test('published contents remain distinct from a newer attempt or partial console snapshot',()=>{
    const project:ProjectSummary={project:'a'.repeat(64),collection:'demo',name:'demo',root:'/demo',current:true,
        publication:'newer-console',indexedFiles:9000,semanticCompleted:200,semanticSelected:8990,observedAt:20};
    const status=statusSummary({index_state:'available',files:8158,chunks:65404,indexed_vectors:9805,publication:{token:'mcp-snapshot'},
        discovery_readiness:{state:'ready'},semantic_readiness:{state:'partial',completed_artifacts:160,selected_artifacts:8156},freshness:{state:'pending'}},10);
    const saved=indexContents(project,status);
    assert.equal(saved.files,8158);assert.equal(saved.vectors,9805);assert.equal(saved.semanticCompleted,160);
    assert.equal(saved.observedAt,10);assert.equal(saved.publication,'mcp-snapshot');assert.equal(saved.discoveryState,'ready');
    const fallback=indexContents(project);assert.equal(fallback.files,9000);assert.equal(fallback.vectors,undefined);assert.equal(fallback.semanticSelected,8990);
    assert.equal(statusSummary({indexed_vectors:null}).vectors,undefined);
    assert.match(sourceWorkExplanation(saved.freshness),/waiting/);
    assert.match(sourceWorkExplanation('idle'),/does not verify/);
});
