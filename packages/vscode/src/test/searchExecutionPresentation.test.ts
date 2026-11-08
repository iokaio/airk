// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import test from 'node:test';
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {Value,WorkbenchSession,WorkbenchTurn} from '../workbenchContracts';
import {discoveryDetails,markdown,renderExport,searchExecutionLabel} from '../workbenchRender';
import {verificationLabel} from '../sourceVerification';

function sourceReceipt():Value {
    const fixtures=JSON.parse(readFileSync(resolve(__dirname,'../../../../docs/research/contracts-v190/fixtures.json'),'utf8'));
    return structuredClone(fixtures.receipt_cases.find((entry:any)=>entry.name==='single_index_match_and_supplement').value);
}
function sample(execution:Value):{turn:WorkbenchTurn;session:WorkbenchSession;result:Value} {
    const receipt=sourceReceipt(),source=(receipt.sources as Value[])[0],origin=(receipt.origins as Value[])[0];
    const result:Value={execution,publication:{token:origin.publication},source_verification:receipt,
        semantic_readiness:{state:'ready',default_search_policy:'partial_hybrid',selected_artifacts:1,completed_artifacts:1,
            selected_units:1,completed_units:1,first_ready_unix_ms:1,fully_ready_unix_ms:2,selection_digest:'ab'.repeat(32),pipeline_digest:'cd'.repeat(32),failure:null},
        discovery_readiness:{version:1,state:'ready',epoch:'retained-epoch',processed_artifacts:1,committed_artifacts:1,total_artifacts:1,failure:null}};
    const citation={path:source.path,content_hash:source.expected_hash,start_line:1,end_line:2};
    const turn:WorkbenchTurn={id:'execution',at:1,input:'Find evidence',format:'structured',envelope:{status:execution.incomplete?'partial':'complete',executed:true,
        publication:origin.publication,plan:{path_prefix:''},steps:[{id:'chosen',tool:'search',status:'complete',result},
            {id:'other',tool:'search',status:'complete',result:{hits:[{text:'unselected-result'}]}}],
        evidence:[{step:'chosen',publication:origin.publication,citation,text:'Selected indexed evidence'},
            {step:'other',publication:origin.publication,citation:{...citation,path:'other.ts'},text:'unselected-evidence'}]}};
    return {turn,result,session:{schema:1,id:'session',project:'ab'.repeat(32),title:'Execution',createdAt:1,updatedAt:1,revision:1,omitted:0,turns:[turn]}};
}

test('search modes, execution limitations and indexed coverage survive details and all exports',()=>{
    for(const execution of [
        {version:1,requested_mode:'default',served_mode:'hybrid',incomplete:false},
        {version:1,requested_mode:'lexical',served_mode:'lexical',incomplete:false},
        {version:1,requested_mode:'default',served_mode:'lexical',incomplete:true,incomplete_reason:'helper_unavailable'},
        {version:1,requested_mode:'default',served_mode:'hybrid',incomplete:true,incomplete_reason:'semantic_coverage_incomplete'},
        {version:1,requested_mode:'hybrid',served_mode:'lexical',incomplete:true,incomplete_reason:'semantic_upgrade_pending'},
    ]){
        const {turn,session,result}=sample(execution),before=structuredClone(turn);
        const details=discoveryDetails(turn.envelope!)[0];
        assert.deepEqual(details.execution,execution);
        assert.deepEqual(details.semanticReadiness,result.semantic_readiness);
        assert.deepEqual(details.discoveryReadiness,result.discovery_readiness);
        assert.deepEqual(details.publication,result.publication);
        assert.match(verificationLabel(details.sourceVerification),/Saved sources verified at/);
        assert.equal(searchExecutionLabel(execution)?.includes('semantic component is incomplete'),execution.incomplete);
        const full=JSON.parse(renderExport(session,'json')).turns[0].envelope;
        assert.deepEqual(full,turn.envelope,'complete export retains the exact native response');
        const selected=JSON.parse(renderExport(session,'json',turn.id,['record-0'])).turns[0].envelope;
        assert.equal(selected.selection,true);assert.equal(selected.evidence.length,1);assert.deepEqual(selected.steps,[]);
        const coverage=selected.source_coverage[0];
        assert.equal(coverage.scope,'original_source_request');
        for(const key of ['execution','semantic_readiness','discovery_readiness','publication'])assert.deepEqual(coverage[key],result[key]);
        assert.equal(selected.source_verification_observations[0].original_status,'verified');
        assert.equal(selected.source_verification_observations[0].verification_performed,false);
        assert.equal(selected.selected_verification_accounting.selected_citations,1);
        assert.equal(JSON.stringify(selected).includes('unselected-evidence'),false);
        assert.equal(JSON.stringify(selected).includes('unselected-result'),false);
        for(const selectedIds of [undefined,new Set(['record-0'])]){
            const output=markdown(turn,selectedIds);
            assert.ok(output.includes('"requested_mode"'));
            assert.ok(output.includes('"served_mode"'));
            assert.ok(output.includes('"semantic_readiness"'));
            assert.ok(output.includes('"discovery_readiness"'));
            if(execution.incomplete)assert.match(output,/does not establish semantic completeness/);
        }
        assert.deepEqual(turn,before,'viewing and selection never mutate native observations');
    }
});

test('strict helper and required-verification refusals retain complete contextual metadata',()=>{
    const execution={version:1,requested_mode:'default',served_mode:'lexical',incomplete:true,incomplete_reason:'helper_unavailable'};
    const {turn,session,result}=sample(execution);
    for(const error of [
        {code:'helper_unavailable',message:'The embedding helper is unavailable for this search.',retryable:false,action:'retry_later_without_replay'},
        {code:'verification_required_unmet',message:'Required saved-source verification did not match every returned file reference.',retryable:false},
    ]){
        const strict=error.code==='helper_unavailable';
        const body:Value=strict?{error,requested_mode:'vector',publication:result.publication,
            semantic_readiness:result.semantic_readiness,discovery_readiness:result.discovery_readiness}
            :{...result,error,source_verification:{...(result.source_verification as Value),mode:'require',status:'requirements_unmet',requirements_satisfied:false,inventory_complete:false}};
        for(const wrapper of [{isError:true,content:[{type:'text',text:JSON.stringify(body)}]},
            {isError:true,content:[],structuredContent:body}]){
            (turn.envelope!.steps as Value[])[0].result=wrapper;
            const details=discoveryDetails(turn.envelope!)[0];
            assert.deepEqual(details.readError,error);
            assert.equal(details.requestedMode,strict?'vector':undefined);
            assert.deepEqual(details.execution,strict?undefined:execution);
            assert.deepEqual(details.semanticReadiness,result.semantic_readiness);
            assert.deepEqual(details.publication,result.publication);
            assert.match(markdown(turn),new RegExp(error.code));
            const complete=JSON.parse(renderExport(session,'json')).turns[0].envelope;
            assert.deepEqual(complete.steps[0].result,wrapper);
            if(!strict){
                const subset=JSON.parse(renderExport(session,'json',turn.id,['record-0'])).turns[0].envelope;
                assert.deepEqual(subset.source_coverage[0].error,error);
                assert.deepEqual(subset.source_coverage[0].execution,execution);
                assert.equal(subset.source_verification_observations[0].original_status,'requirements_unmet');
            }
        }
    }
});

test('unknown execution metadata remains visible without inventing modes or helper health',()=>{
    const unknown={version:2,requested_mode:'future',served_mode:'future',incomplete:true,incomplete_reason:'future_reason',future:{complete:'retained'}};
    const {turn,session}=sample(unknown),before=structuredClone(turn);
    assert.equal(searchExecutionLabel(unknown),undefined);
    assert.deepEqual(discoveryDetails(turn.envelope!)[0].execution,unknown);
    const subset=JSON.parse(renderExport(session,'json',turn.id,['record-0'])).turns[0].envelope;
    assert.deepEqual(subset.source_coverage[0].execution,unknown);
    assert.ok(markdown(turn,new Set(['record-0'])).includes('future_reason'));
    assert.equal(markdown(turn).includes('does not establish semantic completeness'),false);
    assert.deepEqual(turn,before);
    (turn.envelope!.steps as Value[])[0].result={hits:[]};
    assert.equal(discoveryDetails(turn.envelope!).some(value=>value.execution!==undefined),false);
    assert.equal(searchExecutionLabel(undefined),undefined);
    assert.equal(searchExecutionLabel({version:1,requested_mode:'default',served_mode:'unrecognized',incomplete:true}),undefined);
});


test('unavailable legacy semantic readiness and compatibility survive full and selected exports',()=>{
    const {turn,session,result}=sample({version:1,requested_mode:'lexical',served_mode:'lexical',incomplete:false});
    result.semantic_readiness={state:'unavailable',failure:'legacy_saved_semantic_conversion_unavailable',future:{retained:true}};
    result.legacy_adoption={semantic_available:false,future:{archive:'original-archive-observation'}};
    const before=structuredClone(turn),details=discoveryDetails(turn.envelope!)[0];
    assert.deepEqual(details.semanticReadiness,result.semantic_readiness);assert.deepEqual(details.legacyAdoption,result.legacy_adoption);
    assert.deepEqual(JSON.parse(renderExport(session,'json')).turns[0].envelope,turn.envelope);
    const subset=JSON.parse(renderExport(session,'json',turn.id,['record-0'])).turns[0].envelope;
    assert.deepEqual(subset.source_coverage[0].semantic_readiness,result.semantic_readiness);assert.deepEqual(subset.source_coverage[0].legacy_adoption,result.legacy_adoption);
    for(const selection of [undefined,new Set(['record-0'])]){const text=markdown(turn,selection);assert.match(text,/legacy_saved_semantic_conversion_unavailable/);assert.match(text,/original-archive-observation/);}
    assert.deepEqual(turn,before);
});
