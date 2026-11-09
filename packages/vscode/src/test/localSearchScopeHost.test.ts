// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import * as path from 'node:path';
import {serviceTestHost} from './serviceTestHost';
import {record,type RecordValue} from '../serviceClient';
import {IntentJournal} from '../serviceIntents';
import {IndexingCatalogController} from '../indexingCatalogController';

for(const boundary of ['own','whole_snapshot_event','foreign_pending','identity_changed'] as const)test(`actual host index-set save retains journal and window fences at ${boundary} boundary`,async context=>{
    const f=await serviceTestHost(context,{registered:false}),controller=f.context.subscriptions.find((item:any)=>typeof item.useGrant==='function');
    const index='idx_'+'22'.repeat(16),source='src_'+'44'.repeat(16),digest='45'.repeat(32),operationId='46'.repeat(16);
    let revision=5,project:RecordValue|undefined,membership:RecordValue|undefined,injected=false;
    let snapshotDelivered=false,intentMementoWrites=0;
    const applied:string[]=[],pendingSnapshots:string[][]=[],contextReads:Promise<void>[]=[];
    let ui:IndexingCatalogController|undefined;
    const observe=()=>{if(ui)pendingSnapshots.push([...(ui.snapshot().pending??[])]);};
    try{
        await controller.refresh();await controller.console.controller.refresh();
        f.state.hostContextReply=key=>({status:'catalog_host_context',descriptor:{schema:1,namespace:'34'.repeat(16),host:'56'.repeat(32),key,catalog_revision:revision,context:null}});
        await controller.prepareWindowContext();
        const native=controller.currentWindowContext().response.descriptor,console=controller.console.controller;
        const client=console.client,request=client.transport.request.bind(client.transport),update=f.context.globalState.update.bind(f.context.globalState);
        if(boundary==='whole_snapshot_event'){
            // Model the installed VS Code Memento contract: local update is
            // synchronous, matching storage events replace the entire cache,
            // and the scheduled flush reads that cache when it actually runs.
            // A separate writer's outstanding event is NOT fenced by awaiting
            // this window's preceding update acknowledgement. This is a
            // permitted ordering model, not proof that R37 received that event.
            let cache:Record<string,unknown>=Object.fromEntries(f.values),otherWriterEvent:Record<string,unknown>|undefined;
            const acceptSnapshot=(snapshot:Record<string,unknown>)=>{cache=snapshot;snapshotDelivered=true;f.values.clear();for(const [key,value] of Object.entries(cache))if(value!==undefined)f.values.set(key,structuredClone(value));};
            context.mock.method(f.context.globalState,'keys',()=>Object.keys(cache).filter(key=>cache[key]!==undefined));
            context.mock.method(f.context.globalState,'get',(key:string,fallback?:unknown)=>cache[key]===undefined?fallback:cache[key]);
            context.mock.method(f.context.globalState,'update',async(key:string,saved:unknown)=>{
                const prior=cache[key];cache[key]=saved===undefined?undefined:structuredClone(saved);observe();
                if(key.startsWith('airk.service.intent.'))intentMementoWrites++;
                // Deliver the independent writer's already-emitted whole-state
                // event before this deletion's scheduled setValue flush.
                await Promise.resolve();
                if(saved===undefined&&otherWriterEvent&&record(record(record(prior??{}).intent??{}).catalog_apply??{}).request){acceptSnapshot(otherWriterEvent);otherWriterEvent=undefined;}
                await Promise.resolve();
                f.values.clear();for(const [stored,value] of Object.entries(cache))if(value!==undefined)f.values.set(stored,structuredClone(value));
                observe();
                const row=record(saved??{}),intent=record(row.intent??{}),apply=record(intent.catalog_apply??{}),operation=record(record(apply.request??{}).operation??{});
                if(operation.operation==='set_membership'&&row.nativeObservation&&!injected){
                    injected=true;otherWriterEvent={...structuredClone(cache),'fixture.independentWriter':true};
                }
                await Promise.resolve();
            });
            if(f.intentStorage.retire){
                const retire=f.intentStorage.retire.bind(f.intentStorage);
                context.mock.method(f.intentStorage,'retire',async(key:string)=>{
                    const prior=f.intentStorage.get(key),snapshot={...structuredClone(cache),'fixture.independentWriter':true};
                    const saved=record(prior??{}),apply=record(record(saved.intent??{}).catalog_apply??{}),operation=record(record(apply.request??{}).operation??{});
                    await retire(key);
                    if(operation.operation==='set_membership'){
                        // The same whole-cache event can still arrive, but the
                        // critical request/retirement is no longer in Memento.
                        acceptSnapshot(snapshot);
                    }
                });
            }
        }else context.mock.method(f.context.globalState,'update',async(key:string,saved:unknown)=>{observe();await Promise.resolve();await update(key,saved);observe();await Promise.resolve();});
        context.mock.method(client.transport,'request',async(input:unknown)=>{
            const method=record(typeof input==='string'?{}:input),name=typeof input==='string'?input:Object.keys(method)[0];
            if(name==='catalog_preview'){
                const apply=record(record(method.catalog_preview).request);
                return {status:name,preview_digest:digest,preview:{request_digest:digest,catalog_revision:revision,next_revision:revision+1,operation:apply.operation,before:[],after:[],authority_fence_required:record(apply.operation).operation==='set_membership',file_data_changes:false}};
            }
            if(name==='catalog_apply'){
                const apply=record(record(method.catalog_apply).request),operation=record(apply.operation),kind=String(operation.operation);assert.equal(apply.expected_catalog_revision,revision);applied.push(kind);revision++;
                if(kind==='create_project')project={id:operation.id,name:operation.name,definition_revision:1,scope_revision:1,defaults_revision:1,default_indexes:[]};
                if(kind==='set_membership'){membership=record(operation.membership);project!.scope_revision=2;}
                if(kind==='set_project_defaults'){project!.default_indexes=operation.index_ids;project!.defaults_revision=2;}
                const receipt={schema:1,operation_id:operationId,caller:apply.caller,key:apply.key,request_digest:digest,catalog_revision:revision,authority_fence_required:kind==='set_membership'};
                const fenced_receipt=kind==='set_membership'?{request:apply,metadata:receipt,preview_digest:digest,prepared_digest:digest,proof:{plan_digest:digest,service:f.state.nativeInstance,gate_generation:1,confirmations:[],worker_confirmations:[]}}:undefined;
                if(boundary==='identity_changed'&&!injected){injected=true;controller.generation++;}
                observe();return {status:'catalog_applied',receipt,...(fenced_receipt?{fenced_receipt}:{})};
            }
            if(name==='catalog_detail'){
                const detail=record(method.catalog_detail),entity=record(detail.entity);assert.equal(detail.expected_catalog_revision,revision);
                if(boundary==='foreign_pending'&&membership&&!injected){
                    injected=true;
                    await new IntentJournal(f.intentStorage,f.binding.controlDir,native.namespace).prepare('Sibling unknown',{catalog_apply:{request:{caller:'91'.repeat(16),key:{namespace:native.namespace,epoch:1,nonce:'92'.repeat(16)},expected_catalog_revision:revision,operation:{operation:'rename_project',id:String(project!.id),expected_definition_revision:1,name:'Sibling'}},preview_digest:digest},request_digest:digest});
                }
                return {status:name,detail:{namespace:native.namespace,revision,item:entity.kind==='index'?{kind:'index',value:{id:index,name:'Fixture index a',definition_revision:1,source_revision:1,source_import:'complete',tombstone:false,backing:{}}}:{kind:'project',value:project}}};
            }
            if(name==='catalog_page'){
                const page=record(record(method.catalog_page).request);assert.equal(page.expected_catalog_revision,revision);
                return {status:name,page:{namespace:native.namespace,revision,items:page.kind==='sources'?[{kind:'source',value:{id:source,index_id:index,canonical_path:'/src',aliases:[],owner:true,enabled:true}}]:membership?[{kind:'membership',value:membership}]:[],continuation:null}};
            }
            return request(input);
        });
        ui=new IndexingCatalogController({identity:()=>console.observationIdentity,authorized:()=>f.stub.workspace.isTrusted,changed:observe,control:console,windowContext:()=>controller.currentWindowContext()?.descriptor,windowContextObservation:()=>controller.currentWindowContext()?.response,saveIndexSet:selection=>controller.saveIndexSet(selection)});
        // Refreshes may observe a completed preceding write while the next
        // operation is durably pending, as in the retained editor observations.
        const subscription=controller.console.onDidChangeState(()=>{observe();if(console.state.connected)contextReads.push(controller.prepareWindowContext().catch(()=>{}));});
        try{
            const work=ui.scopeApply({name:'Fixture index set a',index_ids:[index]},ui.snapshot().generation,true);
            if(boundary==='own'||boundary==='whole_snapshot_event'){
                const result=await work;assert.equal(result.status,'index_set_saved');assert.deepEqual(applied,['create_project','set_membership','set_project_defaults']);assert.deepEqual(console.catalogPending(),[]);
                assert.equal(controller.currentWindowContext().response.descriptor.catalog_revision,8);assert.equal(controller.localScopeWorking,false);assert.equal(controller.searchScopeSelection().active,false,'saving does not enroll or activate a grant');
                if(boundary==='whole_snapshot_event'){assert.equal(snapshotDelivered,true);assert.equal(intentMementoWrites,0,'critical intents are never mirrored into whole-extension preference storage');}
            }else{
                await assert.rejects(work,boundary==='foreign_pending'?/catalog_outcome_unconfirmed_reconcile_first/:/stale_catalog_review/);
                assert.deepEqual(applied,boundary==='foreign_pending'?['create_project','set_membership']:['create_project']);assert.equal(console.catalogPending().length,1,'unknown or foreign work remains durable without replay');assert.equal(controller.localScopeWorking,false);assert.equal(controller.searchScopeSelection().active,false);
            }
            await Promise.all(contextReads);assert.ok(pendingSnapshots.some(rows=>rows.length===1),'actual host observers saw the durable own intent');
        }finally{subscription.dispose();}
    }finally{await f.close();}
});

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
