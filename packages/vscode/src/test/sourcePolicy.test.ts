// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { CatalogObject, catalogExport } from '../indexingCatalog';
import { canReviewPolicyUnblock, policyStatusLabels, validatePolicyChange, validatePolicyImpact, validatePolicyStatus, validateSourcePolicy } from '../indexingCatalog';
import { ServiceClient } from '../serviceClient';

const index='idx_'+'11'.repeat(16),root='src_'+'22'.repeat(16),rule='deny_'+'33'.repeat(16),digest='44'.repeat(32);
function policy():CatalogObject{return {
    schema:'airk.source-policy/1',index_id:index,owner_binding:digest,namespace:'collections-1.4',collection:'fixture',epoch:1,
    roots:[{id:root,canonical_path:'/fictional/source',aliases:[],owner:true}],positive_includes:null,denies:[],
};}
function blockedPolicy():CatalogObject{
    const before=policy(),change={operation:'trim',rule_id:rule,selector:{source_root_id:root,kind:'subtree',path:'private'}};
    return {...before,epoch:2,denies:[{...change.selector,rule_id:rule,reason:'trim',created_epoch:2}]};
}

test('source-policy checks agree with approved native positive and negative fixtures',()=>{
    const fixtures=JSON.parse(readFileSync(resolve(__dirname,'../../../..','docs/research/contracts-v190/fixtures.json'),'utf8'));
    for(const entry of fixtures.policy_cases as {name:string;valid:boolean;value:unknown}[]){
        if(entry.valid)assert.doesNotThrow(()=>validateSourcePolicy(entry.value),entry.name);
        else assert.throws(()=>validateSourcePolicy(entry.value),Error,entry.name);
    }
});

test('trim and unblock syntax rejects invented authority and malformed selectors',()=>{
    for(const path of ['../private','a/../private','/private','a\\private','a//private','C:private']){
        assert.throws(()=>validatePolicyChange({operation:'trim',rule_id:rule,selector:{source_root_id:root,kind:'subtree',path}}),Error,path);
    }
    assert.throws(()=>validatePolicyChange({operation:'trim',rule_id:rule,selector:{source_root_id:root,kind:'exact',path:''}}));
    assert.throws(()=>validatePolicyChange({operation:'trim',rule_id:rule,selector:{source_root_id:root,kind:'exact',path:'a'},owner_binding:digest}));
    assert.deepEqual(validatePolicyChange({operation:'unblock',rule_id:rule}),{operation:'unblock',rule_id:rule});
    assert.throws(()=>validatePolicyChange({operation:'unblock',rule_id:'src_'+'33'.repeat(16)}));
});

test('policy roots, deny identities and byte capacities never silently collapse',()=>{
    const value=policy(),descriptor=(value.roots as CatalogObject[])[0];
    assert.throws(()=>validateSourcePolicy({...value,roots:[descriptor,descriptor]}),/roots/);
    assert.throws(()=>validateSourcePolicy({...value,roots:[{...descriptor,aliases:['café','café']}]}),/alias/);
    assert.throws(()=>validateSourcePolicy({...value,roots:[{...descriptor,aliases:['é'.repeat(65)]}]}),/alias/);
    assert.throws(()=>validateSourcePolicy({...value,positive_includes:[{source_root_id:'src_'+'99'.repeat(16),kind:'subtree',path:''}]}),/root/);
    const blocked=blockedPolicy(),deny=(blocked.denies as CatalogObject[])[0];
    assert.throws(()=>validateSourcePolicy({...blocked,denies:[deny,deny]}),/deny/);
    assert.throws(()=>validateSourcePolicy({...blocked,denies:[{...deny,created_epoch:3}]}),/deny/);
    assert.throws(()=>validateSourcePolicy({...blocked,denies:[{...deny,inherited_from:{index_id:index,rule_id:rule,epoch:1}}]}),/inherited/);
});

function status(application:CatalogObject|null=null):CatalogObject{return {
    status:'catalog_policy_status',index_id:index,
    snapshot:{schema:1,operation_id:'66'.repeat(16),policy:blockedPolicy(),job:digest,application,application_job:application?digest:null,last_error:null,future_snapshot:{retained:true}},
    job:{id:digest,project:digest,kind:'configure',state:'pending',fencing_generation:0,attempts:0,outcome:null,future_job:{retained:true}},future_response:{retained:true},
};}

test('policy status preserves native full jobs and separates removal from physical cleanup',async()=>{
    let response=status();const calls:unknown[]=[],client=new ServiceClient({dispose(){},async request(method){calls.push(method);return response;}});
    assert.equal(await client.catalogPolicyStatus(index),response);assert.deepEqual(calls,[{catalog_policy_status:{index_id:index}}]);
    assert.match(policyStatusLabels(response).join(' '),/exclusions active.*Removal.*pending/);
    const application={policy_digest:digest,operation_id:'66'.repeat(16),observed_unix_ms:1793858000000,removal_publication:`airk-r1:${'77'.repeat(16)}:2:${digest}`,no_index:false,removed_artifacts:4,physical_cleanup_pending:true,reclaimed_files:1,reclaimed_bytes:128,retired_components:[]};
    response=status(application);assert.equal(validatePolicyStatus(response,index),response);
    assert.match(policyStatusLabels(response).join(' '),/Removed.*Physical cleanup pending/);
    response=status({...application,physical_cleanup_pending:false});assert.match(policyStatusLabels(response).join(' '),/Physical cleanup complete/);
    assert.match(catalogExport({generation:1,busy:false,policyStatus:response}),/future_job/);
    assert.throws(()=>validatePolicyStatus({...response,index_id:'idx_'+'88'.repeat(16)},index),/crossed/);
    const crossed=structuredClone(response);(crossed.job as CatalogObject).id='99'.repeat(32);assert.throws(()=>validatePolicyStatus(crossed,index),/job/);
    const invalid=status({...application,no_index:true});assert.throws(()=>validatePolicyStatus(invalid,index),/application/);
    const empty=status({...application,removal_publication:null,no_index:true,removed_artifacts:0});assert.equal(validatePolicyStatus(empty,index),empty);
    assert.throws(()=>validatePolicyStatus(status({...application,removal_publication:'airk-r1:wrong'}),index),/publication/);
    const absent={status:'catalog_policy_status',index_id:index,snapshot:null,job:null};assert.equal(validatePolicyStatus(absent,index),absent);assert.match(policyStatusLabels(absent).join(' '),/unknown/);
    assert.throws(()=>validatePolicyStatus({...absent,job:response.job},index),/job/);
    const pending=status({...application,reclaimed_bytes:null,cleanup_error:'storage_busy',retired_components:['analysis','embedding_cache']});assert.equal(validatePolicyStatus(pending,index),pending);assert.match(policyStatusLabels(pending).join(' '),/entire index.*not been regenerated.*embedding cache.*storage_busy/);
    assert.throws(()=>validatePolicyStatus(status({...application,physical_cleanup_pending:false,cleanup_error:'storage_busy'}),index),/cleanup_error/);
    assert.throws(()=>validatePolicyStatus(status({...application,retired_components:['analysis','analysis']}),index),/retired_components/);
    assert.match(policyStatusLabels(response).join(' '),/2026-11-05T/);
    assert.throws(()=>validatePolicyStatus(status({...application,observed_unix_ms:0}),index),/observation_time/);
});

test('native policy impact preserves unknown counts, complete member IDs and overlap limitations',()=>{
    const impact={target:{source_root_id:root,kind:'subtree',path:'private'},canonical_root:'/fictional/source',affected_artifacts:null,affected_vectors:null,affected_facts:null,affected_relationships:null,counts_basis:'Unknown: no retained evidence read.',affected_source_root_ids:[root],affected_project_ids:['prj_'+'99'.repeat(16)],other_covering_index_ids:[index],overlap_basis:'Configured-root overlap candidates only; eligibility and aliases are unknown.'};
    assert.equal(validatePolicyImpact(impact),impact);assert.equal(impact.affected_artifacts,null);
    assert.throws(()=>validatePolicyImpact({...impact,affected_artifacts:-1}),/count/);
    assert.throws(()=>validatePolicyImpact({...impact,affected_project_ids:Array(33).fill('prj_'+'99'.repeat(16))}),/selection/);
    assert.throws(()=>validatePolicyImpact({...impact,affected_source_root_ids:[root,root]}),/selection/);
    assert.throws(()=>validatePolicyImpact({...impact,counts_basis:''}),/basis/);
});

test('last-rule unblock and failed-trim escape keep exclusion and removal states independent',()=>{
    const application={policy_digest:digest,operation_id:'66'.repeat(16),observed_unix_ms:1793858000000,removal_publication:`airk-r1:${'77'.repeat(16)}:2:${digest}`,no_index:false,removed_artifacts:0,physical_cleanup_pending:false,reclaimed_files:0,reclaimed_bytes:0,retired_components:[]};
    const unblocked=status(application);(unblocked.snapshot as CatalogObject).policy=policy();
    (unblocked.job as CatalogObject).state='succeeded';
    assert.equal(validatePolicyStatus(unblocked,index),unblocked);
    assert.match(policyStatusLabels(unblocked).join(' '),/No permanent exclusions.*recorded publication.*Physical cleanup complete/);
    assert.doesNotMatch(policyStatusLabels(unblocked).join(' '),/exclusions active|Blocked/);
    (unblocked.snapshot as CatalogObject).last_error='cleanup_failed';
    assert.match(policyStatusLabels(unblocked).join(' '),/no permanent exclusions are active/);
    for(const state of ['failed','canceled','pending','running']){
        const failed=status();(failed.job as CatalogObject).state=state;
        assert.equal(canReviewPolicyUnblock(failed),state==='failed'||state==='canceled');
        if(state==='failed'||state==='canceled')assert.match(policyStatusLabels(failed).join(' '),/no recorded publication.*exclusions remain active.*separate native unblock review/);
        assert.doesNotMatch(policyStatusLabels(failed).join(' '),/Removed from|cleanup complete/);
    }
});
