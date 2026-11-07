// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import * as path from 'node:path';
import {serviceTestHost} from './serviceTestHost';

test('saved local selection restores only exact native membership and stale metadata deactivates it',async context=>{
    const f=await serviceTestHost(context,{registered:false}),controller=f.context.subscriptions.find((item:any)=>typeof item.useGrant==='function');
    const index='idx_'+'22'.repeat(16),project='prj_'+'33'.repeat(16),source='src_'+'44'.repeat(16),grantId='grt_'+'11'.repeat(16);let revision=1,membershipRevision=1;
    try{
        await controller.refresh();await controller.console.controller.refresh();await controller.prepareWindowContext();
        const native=controller.currentWindowContext().response.descriptor;
        const membership={project_id:project,index_id:index,revision:1,scope:[{source_root_id:source,kind:'subtree',path:''}]};
        const grant={grant:{id:grantId,host:native.host,revision:1,revoked:false,memberships:[{membership,source_revision:1,policy_epoch:0,policy_digest:null}]},provenance:{kind:'user_reviewed_project_grant'}};
        const projectRow={id:project,name:'My set',definition_revision:1,scope_revision:1,defaults_revision:1,default_indexes:[]};
        const indexRow={id:index,name:'Owned index',backing:{binding:'77'.repeat(32),root:path.join(f.root,'source-0'),data_root:path.join(f.root,'private'),collection:'fixture'},definition_revision:1,source_revision:1,tombstone:false,source_import:'complete',policy:null};
        f.state.catalogDetailReply=args=>{const entity=args.entity as any;return {status:'catalog_detail',detail:{namespace:native.namespace,revision,item:{kind:entity.kind,value:entity.kind==='grant'?grant:entity.kind==='project'?projectRow:entity.kind==='membership'?{...membership,revision:membershipRevision}:indexRow}}};};
        f.values.set('airk.localScope.selection.1',{schema:1,namespace:native.namespace,host:native.host,context_key:native.key,selection:{index_ids:[index],project_ids:[]},grant:{grant_id:grantId,expected_grant_revision:1,project_ids:[project]}});
        controller.attachWorkbench({async useScope(){},async refresh(){}});
        const writes=f.writes.length;await controller.restoreLocalSearchScope();
        assert.equal(controller.searchScopeSelection().active,true);assert.equal(f.writes.length,writes,'restoration reads native metadata without creating grants or memberships');
        assert.equal(controller.currentCatalogScope().spec.name,'1 index','internal managed group count stays out of the user label');
        revision=2;membershipRevision=2;f.state.hostContextReply=key=>({status:'catalog_host_context',descriptor:{...native,key,catalog_revision:revision}});
        await controller.prepareWindowContext();assert.equal(controller.searchScopeSelection().active,false,'changed current membership cannot reactivate a frozen older grant');
        assert.deepEqual(controller.searchScopeSelection().index_ids,[index],'saved checkbox choices remain available for explicit Apply');
        f.values.set('airk.localScope.selection.1',{schema:1,selection:'corrupt'});assert.doesNotThrow(()=>controller.searchScopeSelection());assert.equal(controller.searchScopeSelection().active,false);
    }finally{await f.close();}
});

test('a delayed old restoration cannot clear a newer explicitly applied scope',async context=>{
    const f=await serviceTestHost(context,{registered:false}),controller=f.context.subscriptions.find((item:any)=>typeof item.useGrant==='function');
    let release!:(value:unknown)=>void;
    try{
        await controller.refresh();await controller.console.controller.refresh();await controller.prepareWindowContext();
        const native=controller.currentWindowContext().response.descriptor;
        f.values.set('airk.localScope.selection.1',{schema:1,namespace:native.namespace,host:native.host,context_key:native.key,selection:{index_ids:[],project_ids:['prj_'+'33'.repeat(16)]},grant:{grant_id:'grt_'+'11'.repeat(16),expected_grant_revision:1,project_ids:['prj_'+'33'.repeat(16)]}});
        const held=new Promise(resolve=>{release=resolve;});f.state.catalogDetailReply=()=>held;
        const restoring=controller.restoreLocalSearchScope();await new Promise(resolve=>setImmediate(resolve));
        controller.attachWorkbench({async refresh(){}});
        await controller.applySearchScope({index_ids:[],project_ids:[]});
        const newer={newExplicitSelection:true};controller.explicitCatalogScope=newer;
        release({status:'catalog_detail',detail:{namespace:native.namespace,revision:1,item:null}});await restoring;
        assert.equal(controller.explicitCatalogScope,newer,'the old restore failure cannot erase a later selection');
        assert.equal((f.values.get('airk.localScope.selection.1') as any).appliedEmpty,true,'explicit empty scope is persisted as applied');
    }finally{release?.({});await f.close();}
});

for(const refreshFails of [false,true])test(`a partially failed index-set save deactivates the old scope when metadata refresh ${refreshFails?'fails':'succeeds'}`,async context=>{
    const f=await serviceTestHost(context,{registered:false}),controller=f.context.subscriptions.find((item:any)=>typeof item.useGrant==='function');
    const index='idx_'+'22'.repeat(16),project='prj_'+'33'.repeat(16),source='src_'+'44'.repeat(16),grantId='grt_'+'11'.repeat(16);
    let revision=1,membershipRevision=1;
    try{
        await controller.refresh();await controller.console.controller.refresh();await controller.prepareWindowContext();
        const native=controller.currentWindowContext().response.descriptor;
        const membership={project_id:project,index_id:index,revision:1,scope:[{source_root_id:source,kind:'subtree',path:''}]};
        const grant={grant:{id:grantId,host:native.host,revision:1,revoked:false,memberships:[{membership,source_revision:1,policy_epoch:0,policy_digest:null}]},provenance:{kind:'user_reviewed_project_grant'}};
        const projectRow={id:project,name:'My set',definition_revision:1,scope_revision:1,defaults_revision:1,default_indexes:[]};
        const indexRow={id:index,name:'Owned index',backing:{binding:'77'.repeat(32),root:path.join(f.root,'source-0'),data_root:path.join(f.root,'private'),collection:'fixture'},definition_revision:1,source_revision:1,tombstone:false,source_import:'complete',policy:null};
        f.state.catalogDetailReply=args=>{const entity=args.entity as any;return {status:'catalog_detail',detail:{namespace:native.namespace,revision,item:{kind:entity.kind,value:entity.kind==='grant'?grant:entity.kind==='project'?projectRow:entity.kind==='membership'?{...membership,revision:membershipRevision}:indexRow}}};};
        f.values.set('airk.localScope.selection.1',{schema:1,namespace:native.namespace,host:native.host,context_key:native.key,selection:{index_ids:[],project_ids:[project]},grant:{grant_id:grantId,expected_grant_revision:1,project_ids:[project]}});
        controller.attachWorkbench({async useScope(){},async refresh(){}});
        await controller.restoreLocalSearchScope();assert.equal(controller.searchScopeSelection().active,true);
        const failure=new Error('later index-set step failed');
        context.mock.method(require('../localSearchScope'),'saveLocalIndexSet',async()=>{
            revision++;membershipRevision++;
            f.state.hostContextReply=key=>{if(refreshFails)throw new Error('metadata refresh failed');return {status:'catalog_host_context',descriptor:{...native,key,catalog_revision:revision}};};
            throw failure;
        });
        await assert.rejects(controller.saveIndexSet({id:project,name:'Changed set',index_ids:[index]}),error=>error===failure,'the original mutation failure remains authoritative');
        assert.equal(controller.searchScopeSelection().active,false,'partially changed membership cannot retain an active older grant');
        assert.equal(controller.currentCatalogScope(),undefined);
        assert.deepEqual(controller.searchScopeSelection().project_ids,[project],'checkbox choices remain available for explicit Apply');
        if(refreshFails)assert.ok(f.calls.some(message=>message.includes('Could not refresh search scope after the failed index-set change')));
    }finally{await f.close();}
});
