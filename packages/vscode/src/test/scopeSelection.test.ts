// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as path from 'node:path';
import { observedScopeSelectionIdentity, reviewScopeSelection, scopePreferenceKey, scopeSelectionIdentity, scopeSelectionPreference, validateCatalogSession, WindowScopeSelection } from '../scopeSelection';
import { launchArgs, unifiedEntry } from '../model';
import { validateCatalogSession as pureValidator } from '../catalogSession';

function fixture(){
    const namespace='01'.repeat(16),host='02'.repeat(32),key='03'.repeat(32);
    const grant='grt_'+'04'.repeat(16),index='idx_'+'05'.repeat(16),project='prj_'+'06'.repeat(16);
    const source='src_'+'07'.repeat(16),binding='08'.repeat(32);
    const context={schema:1,namespace,host,key,catalog_revision:7,context:null};
    const grantDetail={namespace,revision:7,item:{kind:'grant',value:{provenance:{kind:'user_reviewed_project_grant'},grant:{
        id:grant,host,revision:2,revoked:false,memberships:[{membership:{project_id:project,index_id:index,revision:1,
            scope:[{source_root_id:source,kind:'subtree',path:'public'}]},source_revision:3,policy_epoch:0,policy_digest:null}]}}}};
    const indexDetail={namespace,revision:7,item:{kind:'index',value:{id:index,name:'Native index',definition_revision:1,source_revision:3,
        policy:null,tombstone:false,source_import:'complete',backing:{binding,root:path.resolve('native','source'),data_root:path.resolve('native','data'),collection:'native-index'}}}};
    const choice={grant_id:grant,expected_grant_revision:2,index_id:index,project_ids:[project]};
    return {context,grantDetail,indexDetail,choice};
}

test('selection refuses crossed namespace/revision/host, revoked/imported and changed source policy',()=>{
    const f=fixture();
    for(const change of [
        (v:ReturnType<typeof fixture>)=>{v.grantDetail.namespace='10'.repeat(16);},
        (v:ReturnType<typeof fixture>)=>{v.indexDetail.revision=8;},
        (v:ReturnType<typeof fixture>)=>{v.grantDetail.item.value.grant.host='11'.repeat(32);},
        (v:ReturnType<typeof fixture>)=>{v.grantDetail.item.value.grant.revoked=true;},
        (v:ReturnType<typeof fixture>)=>{v.grantDetail.item.value.provenance.kind='imported_legacy_snapshot';},
        (v:ReturnType<typeof fixture>)=>{v.indexDetail.item.value.source_revision=4;},
        (v:ReturnType<typeof fixture>)=>{v.grantDetail.item.value.grant.memberships[0].policy_epoch=1;},
        (v:ReturnType<typeof fixture>)=>{v.indexDetail.item.value.source_import='pending_metadata';},
        (v:ReturnType<typeof fixture>)=>{v.indexDetail.item.value.tombstone=true;},
    ]){const crossed=structuredClone(f);change(crossed);assert.throws(()=>reviewScopeSelection(crossed.choice,crossed.context,crossed.grantDetail,crossed.indexDetail));}
});

test('context defaults cannot select projects outside the reviewed grant or choose a second index',()=>{
    const f=fixture(),other='prj_'+'12'.repeat(16);
    assert.throws(()=>reviewScopeSelection({...f.choice,project_ids:[other]},f.context,f.grantDetail,f.indexDetail),/outside_reviewed_grant/);
    assert.throws(()=>reviewScopeSelection({...f.choice,project_ids:[]},f.context,f.grantDetail,f.indexDetail));
    assert.throws(()=>reviewScopeSelection({...f.choice,index_id:'idx_'+'13'.repeat(16)},f.context,f.grantDetail,f.indexDetail));
    const selected=reviewScopeSelection(f.choice,f.context,f.grantDetail,f.indexDetail);
    assert.throws(()=>validateCatalogSession({...selected.session,index_ids:[f.choice.index_id]}),/fields/);
    assert.throws(()=>validateCatalogSession({...selected.session,host:f.context.host}),/fields/);
    assert.throws(()=>validateCatalogSession({...selected.session,project_ids:[...f.choice.project_ids,...f.choice.project_ids]}));
});

test('semantic scope identity excludes only catalog clock and binds native fingerprint when supplied',()=>{
    const f=fixture(),session=reviewScopeSelection(f.choice,f.context,f.grantDetail,f.indexDetail).session;
    const id=scopeSelectionIdentity(session);
    assert.equal(id,scopeSelectionIdentity({...session,expected_catalog_revision:8}));
    for(const changed of [{...session,expected_grant_revision:3},{...session,index_id:'idx_'+'14'.repeat(16)},
        {...session,project_ids:['prj_'+'15'.repeat(16)]},{...session,namespace:'16'.repeat(16)},
        {...session,context_key:'17'.repeat(32)}])assert.notEqual(id,scopeSelectionIdentity(changed));
    assert.notEqual(scopeSelectionIdentity(session,'18'.repeat(32)),scopeSelectionIdentity(session,'19'.repeat(32)));
    const observation={schema:1,qualifier:'selected_subset',provenance:'user_reviewed_project_grant',fingerprint:'18'.repeat(32),
        grant:{id:session.grant_id,revision:session.expected_grant_revision},members:[{index_id:session.index_id,memberships:[{project_id:session.project_ids[0],membership_revision:1}]}]};
    assert.equal(observedScopeSelectionIdentity(session,observation),scopeSelectionIdentity(session,observation.fingerprint));
    assert.throws(()=>observedScopeSelectionIdentity({...session,expected_grant_revision:3},observation),/crossed_scope_observation/);
    assert.throws(()=>observedScopeSelectionIdentity({...session,project_ids:['prj_'+'15'.repeat(16)]},observation),/crossed_scope_observation/);
});

test('saved preference is inert, isolated by native context, and explicit switches preserve captured old selections',()=>{
    const f=fixture(),selection=reviewScopeSelection(f.choice,f.context,f.grantDetail,f.indexDetail),window=new WindowScopeSelection();
    const preference=scopeSelectionPreference(selection);
    assert.equal(window.current,undefined);assert.equal(window.generation,0);
    assert.ok(!('expected_catalog_revision' in preference));assert.ok(!('root' in preference));
    const key=scopePreferenceKey(preference.namespace,preference.host,preference.context_key);
    assert.notEqual(key,scopePreferenceKey('20'.repeat(16),preference.host,preference.context_key));
    assert.notEqual(key,scopePreferenceKey(preference.namespace,'21'.repeat(32),preference.context_key));
    assert.notEqual(key,scopePreferenceKey(preference.namespace,preference.host,'22'.repeat(32)));
    window.use(selection);const captured=window.current!;selection.session.project_ids.length=0;
    assert.equal(window.current!.session.project_ids.length,1);
    window.clear();assert.equal(window.current,undefined);assert.equal(window.generation,2);
    assert.equal(captured.session.project_ids.length,1);
    assert.equal(preference.project_ids.length,1);
});
