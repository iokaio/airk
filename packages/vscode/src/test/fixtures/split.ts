// SPDX-License-Identifier: Apache-2.0
import { CatalogObject } from '../../indexingCatalog';
import { SPLIT_STAGES } from '../../indexingSplit';

/** Transport/consumer fixture only; native policy/hash authority is tested natively. */
export function splitFixture() {
    const namespace='34'.repeat(16),operation='56'.repeat(16),index='idx_'+'91'.repeat(16),destinationIndex='idx_'+'92'.repeat(16),source='src_'+'93'.repeat(16),destinationSource='src_'+'94'.repeat(16);
    const registration={root:'/source',data_root:'/private/data',collection:'original',binding:'12'.repeat(32)},destinationRegistration={root:'/source',data_root:'/private/destination',collection:'destination',binding:'13'.repeat(32)};
    const before:CatalogObject={schema:'airk.source-policy/1',index_id:index,owner_binding:registration.binding,namespace:'collections-1.4',collection:registration.collection,epoch:1,roots:[{id:source,canonical_path:'/source',aliases:[],owner:true}],positive_includes:null,denies:[]};
    const selectors=[{source_root_id:source,kind:'subtree',path:'docs'}];
    const destination={index_id:destinationIndex,owner_binding:destinationRegistration.binding,collection:destinationRegistration.collection,source_ids:{[source]:destinationSource}};
    const capacity={destination_growth:1048576,destination_temporary:65536,original_rewrite:1048576,cleanup_allowance:65536};
    const choice:CatalogObject={selectors,destination_name:'Documents',destination_data_root:destinationRegistration.data_root,destination_collection:destinationRegistration.collection,projects:[],capacity,require_semantic_ready:false,destination_watch_enabled:false};
    const request:CatalogObject={schema:1,required_policy_contract:2,caller:'10'.repeat(16),key:{namespace,epoch:7,nonce:'55'.repeat(16)},expected_catalog_revision:1,original_index:index,original_registration:registration,expected_definition_revision:1,expected_source_revision:1,original_incarnation:'70'.repeat(16),original_settings_digest:'71'.repeat(32),source_configuration:{primary_root:'/source',aliases:{}},baseline:{baseline_kind:'virtual_unrestricted'},before_policy_digest:'72'.repeat(32),selectors,destination_name:'Documents',destination_registration:destinationRegistration,destination,compiled_plan_digest:'73'.repeat(32),projects:[],capacity:{...capacity,original_quota_digest:'74'.repeat(32),destination_quota_digest:'75'.repeat(32)},require_semantic_ready:false,original_watch_digest:'76'.repeat(32),destination_watch_enabled:false};
    const plan:CatalogObject={before_digest:request.before_policy_digest,operation_id:operation,destination_binding:destination,selectors,remainder:{...structuredClone(before),epoch:2,denies:[{...selectors[0],rule_id:'deny_'+'95'.repeat(16),reason:'split_transfer',created_epoch:2}]},destination:{...structuredClone(before),index_id:destinationIndex,owner_binding:destinationRegistration.binding,collection:destinationRegistration.collection,roots:[{id:destinationSource,canonical_path:'/source',aliases:[],owner:true}],positive_includes:[{...selectors[0],source_root_id:destinationSource}]}};
    const draft:CatalogObject={schema:1,request,request_digest:'78'.repeat(32),before_policy:before,plan,destination_storage_action:'create_missing_leaf',control_inputs:{basis:'metadata_only',future_control:'retained'},review_digest:'77'.repeat(32),future_native_draft:{retained:true}};
    const snapshot:CatalogObject={request:structuredClone(request),baseline:structuredClone(before),plan:structuredClone(plan),receipt:{schema:1,operation_id:operation,request_digest:'78'.repeat(32),revision:1,previous_receipt_digest:null,catalog_revision:2,stage:'accepted',state:{state:'pending'},cleanup_attention:null,terminal_publication_slots:{},remainder_witness:null,capacity_receipt:null,funding_receipts:{},destination_storage:null,destination_publication:null,destination_validation:null,original_fence:null,remainder_publication:null,membership_receipt:null,physical_cleanup_authorization:null,funding_release_receipts:{},cleanup_receipt:null,cancellation_cleanup_receipt:null,jobs:[]},receipt_digest:'79'.repeat(32),future_native_snapshot:{retained:true}};
    return {namespace,operation,index,destinationIndex,source,destinationSource,choice,draft,snapshot};
}
export function splitAtStage(input:CatalogObject,stage:typeof SPLIT_STAGES[number],revision=2):CatalogObject {
    const snapshot=structuredClone(input),receipt=snapshot.receipt as CatalogObject,request=snapshot.request as CatalogObject,at=SPLIT_STAGES.indexOf(stage);
    const rootA=(request.original_registration as CatalogObject).data_root as string,rootB=(request.destination_registration as CatalogObject).data_root as string;
    receipt.stage=stage;receipt.revision=revision;receipt.previous_receipt_digest=input.receipt_digest;receipt.catalog_revision=Number((input.receipt as CatalogObject).catalog_revision)+1;
    receipt.state={state:at===8?'complete':'pending'};snapshot.receipt_digest='80'.repeat(32);
    if(at>=1){receipt.capacity_receipt='81'.repeat(32);receipt.funding_receipts={[rootA]:'82'.repeat(32),[rootB]:'83'.repeat(32)};receipt.destination_storage='84'.repeat(32);}
    if(at>=3){receipt.destination_publication=`airk-r1:${'85'.repeat(16)}:1:${'86'.repeat(32)}`;receipt.destination_validation='87'.repeat(32);}
    if(at>=4){receipt.original_fence='88'.repeat(32);receipt.remainder_witness={job:null,index_id:request.original_index,before_policy_digest:request.before_policy_digest,publication:`airk-r1:${'96'.repeat(16)}:1:${'97'.repeat(32)}`,source_root_id:Object.keys((request.destination as CatalogObject).source_ids as CatalogObject)[0],logical_path:'src/main.rs',source_relative_path:'src/main.rs',expected_hash:'98'.repeat(32),observed_from_unix_ms:1,observed_to_unix_ms:2,bytes_read:12};}
    if(at>=5)receipt.remainder_publication=`airk-r1:${'89'.repeat(16)}:2:${'90'.repeat(32)}`;
    if(at>=6)receipt.membership_receipt='91'.repeat(32);
    if(at===8){receipt.physical_cleanup_authorization='92'.repeat(32);receipt.funding_release_receipts={[rootA]:'93'.repeat(32),[rootB]:'94'.repeat(32)};receipt.cleanup_receipt='95'.repeat(32);}
    return snapshot;
}
export function splitSourceResponse(f:ReturnType<typeof splitFixture>):CatalogObject {
    const request=f.draft.request as CatalogObject;
    return {status:'catalog_split_source_state',capabilities:{policy_contract:2,authority_contract:2,future_native_capability:true},state:{catalog_revision:1,index:{id:f.index,name:'Original',definition_revision:1,source_revision:1,tombstone:false,backing:request.original_registration},policy:f.draft.before_policy,baseline:request.baseline,source_configuration:request.source_configuration,watch_digest:request.original_watch_digest,affected_projects:[],future_native_census:'retained'}};
}

export function splitFrozenFixture() {
    const f=splitFixture(),reviewed=splitAtStage(f.snapshot,'cleanup_pending',7),terminal=splitAtStage(reviewed,'complete',8);
    terminal.receipt_digest='da'.repeat(32);
    reviewed.finalization={schema:1,revision:1,terminal_receipt_digest:terminal.receipt_digest,terminal_receipt_revision:8,slots:{},progress:'publishing',attention:'publish_failed',funding_acknowledged:[],future_finalization:'retained'};
    const request={caller:'db'.repeat(16),key:{namespace:f.namespace,epoch:7,nonce:'dc'.repeat(16)},operation_id:f.operation,request_digest:(reviewed.receipt as CatalogObject).request_digest,expected_receipt_digest:reviewed.receipt_digest,expected_finalization_revision:1,action:'resume'};
    const acknowledgment:CatalogObject={request,request_digest:'dd'.repeat(32),outcome:{outcome:'finalization_accepted',receipt_digest:reviewed.receipt_digest,terminal_receipt_digest:terminal.receipt_digest,terminal_receipt:terminal.receipt,finalization_revision:2},future_frozen_ack:'retained'};
    return {...f,reviewed,terminal,acknowledgment,intent:{reviewed,catalog_split_recover:{request}}};
}
