// SPDX-License-Identifier: Apache-2.0
//! Supplemental consumer checks; native transport producer parity is covered
//! by airk-cli/tests/combined_reads/profile_families.rs, not inferred here.
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import {object,toolFormArguments,toolFormFields,type WorkbenchTurn} from '../workbenchContracts';
import {discoveryDetails,renderExport} from '../workbenchRender';
import {newSession,SessionStore} from '../workbenchSessions';

const families=['saved_identity','saved_text','lexical_candidates','vectors','declarations','imports','written_calls','framework_structure','manifest_packages_dependencies','requirements_design_decisions','headings_sections','standalone_dates_bounds','tables_rows','mapped_regions','declaration_continuity','document_unit_continuity','imported_assertions','ocr_text_facts'];
function report(){return {schema:'airk.profile-families/1',basis:'metadata_only',file:{publication:'one-head',path:'src/a.rs',source_hash:'ab'.repeat(32)},applied_file:{adapter_id:'rust',adapter_version:'fixture',effective_profile_digest:'cd'.repeat(32),legacy_detail:'agent_lean'},configured_collection_policy:{state:'unavailable',reason:'selected_scope'},published_collection_policy:{state:'unavailable',reason:'selected_scope'},pending_collection_reindex:null,families:families.map(family=>({family,support:'unknown',retention:family==='manifest_packages_dependencies'?'not_retained':'selected',reply_inclusion:'omitted'})),future_native_field:{identity:'preserve opaque metadata'}};}
type ReportShape="flat"|"historical_nested";
function turn(shape:ReportShape="flat"):WorkbenchTurn{return {id:'families',at:1,input:'/info src/a.rs --include-profile-families',format:'structured',request:{plan:{steps:[{id:'info',tool:'file_info',args:{path:'src/a.rs',include_profile_families:true}}]}},envelope:{status:'complete',executed:true,publication:'one-head',steps:[{id:'info',tool:'file_info',status:'complete',result:shape==='flat'?{read:{status:'content'},path:'src/a.rs',profile_families:report()}:{read:{status:'content'},content:{path:'src/a.rs',profile_families:report()}}}],evidence:[{step:'info',text:'selected evidence',publication:'one-head',citation:{path:'src/a.rs',start_line:1,end_line:1,content_hash:'ab'.repeat(32)}}]}};}

test('native-schema form keeps Default absent and explicit false/true typed',()=>{
    const schema={type:'object',required:['path'],properties:{path:{type:'string'},include_profile_families:{type:'boolean',default:false,description:'Native metadata-only family report'}}};
    const field=toolFormFields(schema).find(field=>field.name==='include_profile_families')!;
    assert.equal(field.type,'boolean');assert.equal(field.required,false);assert.equal(field.defaultValue,false);
    assert.deepEqual(toolFormArguments(schema,{path:'src/a.rs',include_profile_families:''}),{path:'src/a.rs'});
    for(const choice of ['false','true'])assert.equal(toolFormArguments(schema,{path:'src/a.rs',include_profile_families:choice}).include_profile_families,choice==='true');
    assert.throws(()=>toolFormArguments(schema,{path:'src/a.rs',include_profile_families:'yes'}));
});
test('flat native and historical nested details and full/selected exports preserve every family',()=>{
    for(const shape of ['flat','historical_nested'] as const){
        const entry=turn(shape),session=newSession('ab'.repeat(32));session.turns=[entry];const original=structuredClone(entry.envelope);
        const result=object((entry.envelope!.steps as Record<string,unknown>[])[0].result);
        assert.equal(Object.hasOwn(result,'content'),shape==='historical_nested');
        assert.deepEqual(discoveryDetails(entry.envelope!)[0].profileFamilies,report());
        for(const format of ['json','markdown'] as const)for(const selected of [undefined,['record-0']]){
            const output=renderExport(session,format,entry.id,selected);
            for(const family of families)assert.ok(output.includes(family));
            assert.ok(output.includes('preserve opaque metadata'));assert.ok(output.includes('selected_scope'));assert.ok(output.includes('not_retained'));assert.ok(output.includes('omitted'));
            if(format==='json'&&selected){const coverage=JSON.parse(output).turns[0].envelope.source_coverage;assert.deepEqual(coverage[0].profile_families,report());}
        }
        assert.deepEqual(entry.envelope,original);
    }
});

test('absent family report does not invent details or suppress other native metadata',()=>{
    const legacy=turn(),content=object((legacy.envelope!.steps as Record<string,unknown>[])[0].result);
    assert.deepEqual(content.profile_families,report());assert.equal(Object.hasOwn(content,'content'),false);
    delete content.profile_families;
    assert.deepEqual(discoveryDetails(legacy.envelope!),[]);
    content.publication='legacy-metadata-head';
    const details=discoveryDetails(legacy.envelope!);
    assert.equal(details.length,1);
    assert.equal(details[0].publication,'legacy-metadata-head');
    assert.equal(Object.hasOwn(details[0],'profileFamilies'),false);
    const session=newSession('ab'.repeat(32));session.turns=[legacy];
    for(const format of ['json','markdown'] as const){
        const output=renderExport(session,format,legacy.id);
        assert.ok(output.includes('legacy-metadata-head'));
        assert.ok(!output.includes('airk.profile-families/1'));
    }
});
test('actual session save/load preserves opt-in draft and full report without rebuilding it',async()=>{
    const directory=await fs.mkdtemp(path.join(os.tmpdir(),'airk-family-session-'));
    try{
        const store=new SessionStore(directory);
        for(const shape of ['flat','historical_nested'] as const){
        const session=newSession('ab'.repeat(32));session.turns=[turn(shape)];
        session.formDraft={activeTool:'file_info',values:{file_info:{path:'src/a.rs',include_profile_families:'true'}},revision:1,scope:'',recipe:''};
        await store.save(session,true);const restored=await store.load(session.project,session.id);
        assert.deepEqual(restored.turns[0].envelope,session.turns[0].envelope);assert.deepEqual(restored.formDraft,session.formDraft);
        assert.deepEqual(discoveryDetails(restored.turns[0].envelope!)[0].profileFamilies,report());
        for(const format of ['json','markdown'] as const)for(const selected of [undefined,['record-0']])assert.ok(renderExport(restored,format,restored.turns[0].id,selected).includes('preserve opaque metadata'));
        }
    }finally{await fs.rm(directory,{recursive:true,force:true});}
});


test('Selected retained identity survives canonical flat and historical nested full/selected details and exports without legacy alias',()=>{
    for(const shape of ['flat','historical_nested'] as const){
    const entry=turn(shape),value=object(report()),applied=object(value.applied_file);
    delete applied.legacy_detail;delete applied.legacy_detail_was_absent;
    applied.applied_retention={kind:'selected',profile:'rich',family:'manifest',algorithm:'airk/file-aware-retention/1'};
    value.configured_collection_policy={state:'known',revision:{retained_profile:'rich',legacy_fallback_detail:'agent_lean',policy_digest:'ab'.repeat(32)}};
    value.published_collection_policy={state:'known',revision:{retained_profile:'lean',legacy_fallback_detail:'agent_lean',policy_digest:'cd'.repeat(32)}};value.pending_collection_reindex=true;
    const step=object((entry.envelope!.steps as Record<string,unknown>[])[0]),result=object(step.result);
    assert.equal(Object.hasOwn(result,'content'),shape==='historical_nested');
    const content=shape==='flat'?result:object(result.content);content.profile_families=value;
    const session=newSession('ab'.repeat(32));session.turns=[entry];const original=structuredClone(entry.envelope);
    assert.deepEqual(discoveryDetails(entry.envelope!)[0].profileFamilies,value);
    for(const format of ['json','markdown'] as const)for(const selected of [undefined,['record-0']]){
        const output=renderExport(session,format,entry.id,selected);assert.ok(output.includes('airk/file-aware-retention/1'));assert.ok(output.includes('legacy_fallback_detail'));assert.ok(output.includes('pending_collection_reindex'));assert.ok(output.includes('rich'));assert.ok(output.includes('lean'));
        if(format==='json'&&selected){const coverage=JSON.parse(output).turns[0].envelope.source_coverage;assert.deepEqual(coverage[0].profile_families,value);}
    }
    assert.deepEqual(entry.envelope,original);assert.equal(object(value.applied_file).legacy_detail,undefined);
    }
});
