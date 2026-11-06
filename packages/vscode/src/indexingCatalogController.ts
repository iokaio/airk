// SPDX-License-Identifier: Apache-2.0
import { CatalogKind, CatalogObject, CatalogReview, CatalogView, IndexSetupReview, WindowContextDescriptor, catalogBound, catalogHex, catalogId, catalogItems, catalogObject, catalogRevision, catalogWindowAssociation, CATALOG_PAGE_BYTES, validateCatalogOperation, validateGrantSelection, validateNativeHostContext, validateWindowContext, validateIndexSetupSelection, validateSourceMutationSelection, sourceChangeObservation } from './indexingCatalog';
import { randomUUID } from 'node:crypto';
import { canReviewPolicyUnblock, CatalogRetirementReview } from './indexingCatalog';
import { SplitReview, hasSplitCapability, validateSplitSelection, validateSplitSnapshot, validateSplitSourceState } from './indexingSplit';

export interface CatalogControl {
    catalogSplitDraft?(index:string,selection:CatalogObject,revision:number):Promise<SplitReview>;
    catalogSplitSourceState?(index:string,revision:number):Promise<CatalogObject>;
    catalogSplitApply?(review:SplitReview,valid:()=>boolean):Promise<CatalogObject>;
    catalogSplitStatus?(operation:string):Promise<CatalogObject>;
    catalogSplitRecover?(observation:CatalogObject,action:'resume'|'cancel',valid:()=>boolean):Promise<CatalogObject>;
    catalogRead(method: CatalogObject): Promise<CatalogObject>;
    catalogPrepare(operation: CatalogObject, revision: number): Promise<CatalogReview>;
    catalogApply(review: CatalogReview, valid: () => boolean): Promise<CatalogObject>;
    catalogReconcile(): Promise<CatalogObject[]>;
    catalogPending(): readonly string[];
    catalogPendingDetails?(): readonly CatalogObject[];
    catalogGrantDraft?(contextKey:string,selection:CatalogObject,revision:number):Promise<CatalogReview>;
    catalogIndexSetupDraft?(selection:CatalogObject,revision:number):Promise<IndexSetupReview>;
    catalogIndexSetupApply?(review:IndexSetupReview,valid:()=>boolean):Promise<CatalogObject>;
    catalogSetupResume?(id:string,valid:()=>boolean):Promise<CatalogObject>;
    catalogSourceDraft?(index:string,change:CatalogObject,revision:number):Promise<CatalogReview>;
    catalogSourceObservation?(begin:string,namespace:string,prior?:CatalogObject):Promise<CatalogObject>;
    catalogPendingSource?(index:string,namespace:string):Promise<CatalogObject>;
    catalogSourceResolve?(observation:CatalogObject,valid:()=>boolean):Promise<CatalogObject>;
    catalogCanRetire?(index:string,backing:string,namespace:string):boolean;
    catalogPolicyStatus?(index:string):Promise<CatalogObject>;
    catalogPolicyResume?(observation:CatalogObject,valid:()=>boolean):Promise<CatalogObject>;
    catalogRetirementReview?(id:string):Promise<CatalogRetirementReview>;
    catalogStopTracking?(review:CatalogRetirementReview,valid:()=>boolean):Promise<CatalogObject>;
}
export interface CatalogHost { control?: CatalogControl; identity(): string; authorized(): boolean; changed(): void; windowContext?(): WindowContextDescriptor | undefined; windowContextObservation?(): CatalogObject | undefined; canReadWindowContext?(): boolean; useGrant?(choice:CatalogObject,namespace:string,revision:number,valid:()=>boolean):Promise<CatalogObject> }
/** Window memory only; the shared native console owns durable intents and transport. No polling or source scanning. */
export class IndexingCatalogController {
    private owner?: string;
    private splitPreparedSequence?:number;
    private splitObservedSequence?:number;
    private sequence = 0;
    private view: CatalogView = { generation: 0, busy: false };
    private pageRequest?: CatalogObject;
    private membershipRequest?: CatalogObject;
    private sourceRequest?: CatalogObject;
    private grantRequest?: CatalogObject;
    constructor(private readonly host: CatalogHost) {}
    snapshot(): CatalogView {
        this.sync();
        return this.host.authorized() ? { ...this.view, canReviewSplit:hasSplitCapability(this.view.capabilities)&&!!this.host.control?.catalogSplitDraft&&!!this.host.control?.catalogSplitApply&&!!this.host.control?.catalogSplitSourceState, canReadPolicy:!!this.host.control?.catalogPolicyStatus,canResumePolicy:!!this.host.control?.catalogPolicyResume,canReviewRetirement:!!this.host.control?.catalogRetirementReview&&!!this.host.control?.catalogStopTracking,windowDescriptor:this.descriptor(),canReadWindowContext:this.host.canReadWindowContext?.(),canReviewGrant:!!this.host.control?.catalogGrantDraft,canUseGrant:!!this.host.useGrant,canReviewSource:!!this.host.control?.catalogSourceDraft,canReadPendingSource:!!this.host.control?.catalogPendingSource,canReviewSetup:!!this.host.control?.catalogIndexSetupDraft&&!!this.host.control?.catalogIndexSetupApply,pending: this.host.control?.catalogPending() ?? [],pendingDetails:this.host.control?.catalogPendingDetails?.() } : { generation: this.view.generation, busy: false, error: 'Workspace trust or client authority is unavailable.' };
    }
    /** Complete native read, retained with the same owner fence as catalog data. */
    async recommendations(read:()=>Promise<CatalogObject>):Promise<CatalogView>{
        return this.run(async(_control,current)=>{
            const response=await read();current();catalogBound(response,65536);
            this.view={...this.view,recommendations:response,generation:this.view.generation+1};
        });
    }
    dismissRecommendations():CatalogView {this.sync();this.sequence++;this.view={...this.view,busy:false,recommendations:undefined,generation:this.view.generation+1};this.host.changed();return this.snapshot();}
    invalidate(): void {
        this.sequence++; this.view = { generation: this.view.generation + 1, busy: false };
        this.pageRequest = this.membershipRequest = this.sourceRequest = undefined;
        this.grantRequest=undefined;
    }
    private sync(): void {
        let native:unknown;
        try{const observed=this.host.windowContextObservation?.()?.descriptor;if(observed&&typeof observed==='object'&&!Array.isArray(observed)){const value=observed as CatalogObject;native=[value.namespace,value.host,value.key,value.catalog_revision];}}catch{/* An unavailable producer cannot retain a prior authority identity. */}
        const owner = JSON.stringify([this.host.authorized(),this.host.identity(),this.descriptor(),native]);
        if (owner !== this.owner) { this.owner = owner; this.invalidate(); }
    }
    private descriptor(): WindowContextDescriptor | undefined {
        try{const value=this.host.windowContext?.();return value===undefined?undefined:validateWindowContext(value);}catch{return undefined;}
    }
    private async run(work: (control: CatalogControl, current: () => void) => Promise<void>): Promise<CatalogView> {
        this.sync();
        if (!this.host.authorized()) { throw new Error('runtime_client_no_longer_authorized'); }
        if (this.view.busy) { throw new Error('catalog_request_pending'); }
        if (!this.host.control) { throw new Error('catalog_control_connection_unavailable'); }
        const owner = this.owner, sequence = ++this.sequence;
        const current = () => { this.sync(); if (!this.host.authorized() || owner !== this.owner || sequence !== this.sequence) { throw new Error('stale_catalog_view'); } };
        this.view = { ...this.view, busy: true, error: undefined }; this.host.changed();
        try { await work(this.host.control, current); current(); }
        catch (error) {
            if (owner === this.owner && sequence === this.sequence) {
                const native=error&&typeof error==='object'?(error as {nativeResponse?:unknown}).nativeResponse:undefined;
                this.view = { ...this.view, review: undefined,setupReview:undefined,...(native&&typeof native==='object'&&!Array.isArray(native)?{outcome:catalogObject(native)}:{}), error: error instanceof Error ? error.message : 'catalog_operation_failed' };
            }
            throw error;
        } finally {
            if (owner === this.owner && sequence === this.sequence) { this.view = { ...this.view, busy: false }; this.host.changed(); }
        }
        return this.snapshot();
    }
    private accept(response: CatalogObject, field: 'page' | 'detail', expected?: number): void {
        catalogBound(response,65536);
        const body = catalogObject(response[field]), namespace = catalogHex(body.namespace), revision = catalogRevision(body.revision);
        catalogBound(body);
        if (this.view.namespace && namespace !== this.view.namespace) { this.invalidate(); throw new Error('catalog_namespace_changed_refresh'); }
        if (expected !== undefined && revision !== expected) { this.invalidate(); throw new Error('catalog_revision_changed_refresh'); }
        catalogItems(response, field);
        this.view = { ...this.view, namespace, revision,...(response.capabilities&&typeof response.capabilities==='object'?{capabilities:catalogObject(response.capabilities)}:{}) };
    }
    page(kind: CatalogKind, next = false, index?: string, project?: string): Promise<CatalogView> {
        return this.run(async (control, current) => {
            const field = kind === 'grants'?'grants':kind === 'sources' ? 'sources' : kind === 'memberships' ? 'memberships' : 'page';
            const previous = field === 'grants'?this.grantRequest:field === 'sources' ? this.sourceRequest : field === 'memberships' ? this.membershipRequest : this.pageRequest;
            const retained = this.view[field];
            const cursor = next && retained ? catalogObject(retained.page).continuation : null;
            if (next && (!cursor || !previous || previous.kind !== kind || previous.index_id !== (index ?? null) || previous.project_id !== (project ?? null))) { throw new Error('catalog_continuation_unavailable'); }
            if (index) { catalogId(index, 'idx_'); } if (project) { catalogId(project, 'prj_'); }
            const expected = next || field !== 'page' ? this.view.revision : undefined;
            const request = { kind, index_id: index ?? null, project_id: project ?? null, after: cursor, expected_catalog_revision: expected ?? null, limit: 16, max_bytes: CATALOG_PAGE_BYTES };
            const response = await control.catalogRead({ catalog_page: { request } }); current(); this.accept(response, 'page', expected);
            const body=catalogObject(response.page),continuation=body.continuation;
            if(catalogItems(response).length>request.limit)throw new Error('invalid_catalog_page_limit');
            if(continuation!==null){
                const cursor=catalogObject(continuation);
                if(cursor.namespace!==body.namespace||cursor.revision!==body.revision||cursor.kind!==kind||cursor.index_id!==request.index_id||cursor.project_id!==request.project_id)throw new Error('crossed_catalog_continuation');
            }
            this.view = { ...this.view, [field]: response, review: undefined,setupReview:undefined, ...(field === 'page' ? { detail: undefined, sources: undefined, memberships: undefined,policyStatus:undefined } : {}) };
            if (field === 'page') { this.pageRequest = request; } else if (field === 'sources') { this.sourceRequest = request; } else if(field==='grants'){this.grantRequest=request;}else { this.membershipRequest = request; }
            this.view.generation++;
        });
    }
    detail(kind: 'project' | 'index' | 'grant', id: string): Promise<CatalogView> {
        return this.run(async (control, current) => {
            catalogId(id, kind === 'grant'?'grt_':kind === 'index' ? 'idx_' : 'prj_');
            const expected = this.view.revision;
            const response = await control.catalogRead({ catalog_detail: { entity: { kind, id }, expected_catalog_revision: expected ?? null } });
            current(); this.accept(response, 'detail', expected);
            const item=catalogItems(response,'detail')[0];if(item&&(item.kind!==kind||(kind==='grant'?catalogObject(item.value.grant).id:item.value.id)!==id))throw new Error('crossed_catalog_detail');
            this.view = { ...this.view, ...(kind==='grant'?{grantDetail:response}:{detail:response,sources:undefined,memberships:undefined,policyStatus:undefined}), review: undefined,setupReview:undefined, generation: this.view.generation + 1 };
        });
    }
    preview(operation: CatalogObject, generation: number): Promise<CatalogView> {
        return this.preparePreview(operation,generation,false);
    }
    private preparePreview(operation: CatalogObject, generation: number, contextReview: boolean,grantRevoke=false): Promise<CatalogView> {
        return this.run(async (control, current) => {
            if (generation !== this.view.generation || !this.view.revision) { throw new Error('stale_catalog_view'); }
            validateCatalogOperation(operation);
            if(operation.operation==='create_grant')throw new Error('native_grant_draft_required');
            if(['begin_index_source_change','retire_index','change_index_policy'].includes(String(operation.operation)))throw new Error('native_source_draft_required');
            if(operation.operation==='revoke_grant'&&!grantRevoke)throw new Error('inspect_native_grant_before_revocation');
            if(['set_context','delete_context'].includes(String(operation.operation))&&!contextReview)throw new Error('window_context_requires_host_review');
            if ((this.host.control?.catalogPending().length ?? 0) > 0) { throw new Error('catalog_outcome_unconfirmed_reconcile_first'); }
            const review = await control.catalogPrepare(operation, this.view.revision); current();
            if (review.request.key.namespace !== this.view.namespace) { throw new Error('catalog_namespace_changed_refresh'); }
            this.view = { ...this.view, review,setupReview:undefined, outcome: undefined };
        });
    }
    create(name: string, generation: number): Promise<CatalogView> {
        return this.preview({operation:'create_project',id:`prj_${randomUUID().replaceAll('-','')}`,name},generation);
    }
    readContext(): Promise<CatalogView> {
        return this.run(async(control,current)=>{
            const descriptor=this.descriptor();if(!descriptor)throw new Error('window_context_descriptor_unavailable');
            const response=await control.catalogRead({catalog_context:{host:descriptor.host,key:descriptor.key,expected_catalog_revision:null}});
            current();this.accept(response,'detail');
            const item=catalogItems(response,'detail')[0];
            if(item&&(item.kind!=='context'||item.value.host!==descriptor.host||item.value.key!==descriptor.key)){this.invalidate();throw new Error('crossed_window_context');}
            this.view={generation:this.view.generation+1,busy:true,namespace:this.view.namespace,revision:this.view.revision,context:response};
        });
    }
    adoptHostContext(): Promise<CatalogView> {
        return this.run(async(_control,current)=>{
            const response=this.host.windowContextObservation?.(),descriptor=this.descriptor();
            if(!response||!descriptor||response.status!=='catalog_host_context')throw new Error('native_window_context_unavailable');
            const native=validateNativeHostContext(response.descriptor);current();
            if(native.host!==descriptor.host||native.key!==descriptor.key)throw new Error('crossed_window_context');
            this.view={generation:this.view.generation+1,busy:true,namespace:native.namespace,revision:native.catalog_revision,hostContext:response};
        });
    }
    previewContext(selection: CatalogObject, generation: number, remove=false): Promise<CatalogView> {
        this.sync();const descriptor=this.descriptor();
        if(!descriptor||!this.view.context&&!this.view.hostContext) return Promise.reject(new Error('read_window_context_before_review'));
        if(Object.keys(selection).sort().join(',')!=='index_ids,project_ids')return Promise.reject(new Error('invalid_context_selection_fields'));
        const prior=catalogWindowAssociation(this.view);
        if(remove){
            if(!prior)return Promise.reject(new Error('window_context_not_saved'));
            return this.preparePreview({operation:'delete_context',id:prior.id,expected_revision:prior.revision},generation,true);
        }
        return this.preparePreview({operation:'set_context',expected_revision:prior?.revision??null,context:{
            id:prior?.id??`ctx_${randomUUID().replaceAll('-','')}`,host:descriptor.host,key:descriptor.key,kind:descriptor.kind,name:descriptor.name,
            revision:prior?catalogRevision(prior.revision)+1:1,project_ids:selection.project_ids,index_ids:selection.index_ids,
        }},generation,true);
    }
    grantDraft(selection:CatalogObject,generation:number):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            const descriptor=this.descriptor(),observation=this.view.hostContext;
            if(!descriptor||!observation||!control.catalogGrantDraft)throw new Error('read_native_window_context_before_grant_review');
            if(generation!==this.view.generation||!this.view.revision)throw new Error('stale_catalog_view');
            if(control.catalogPending().length)throw new Error('catalog_outcome_unconfirmed_reconcile_first');
            const native=validateNativeHostContext(observation.descriptor),chosen=validateGrantSelection(selection),revision=this.view.revision;
            if(native.namespace!==this.view.namespace||native.catalog_revision!==revision||native.key!==descriptor.key||native.host!==descriptor.host)throw new Error('read_current_window_context_before_grant_review');
            const review=await control.catalogGrantDraft(descriptor.key,chosen,revision);current();
            if(review.contextKey!==descriptor.key||review.request.key.namespace!==native.namespace||review.request.expected_catalog_revision!==revision||review.request.operation.operation!=='create_grant'||catalogObject(review.request.operation.grant).host!==native.host)throw new Error('crossed_native_grant_review');
            this.view={...this.view,review,setupReview:undefined,outcome:undefined,submittedReview:undefined};
        });
    }
    apply(generation: number): Promise<CatalogView> {
        return this.run(async (control, current) => {
            const review = this.view.review;
            if (!review || generation !== this.view.generation) { throw new Error('stale_catalog_review'); }
            // Consume before dispatch. Any interrupted reply is reconciled, never replayed.
            this.view = { ...this.view, review: undefined,submittedReview:review };
            const outcome = await control.catalogApply(review, () => { try { current(); return generation === this.view.generation; } catch { return false; } });
            current(); this.view = { generation: this.view.generation + 1, busy: true, outcome,submittedReview:review,namespace:review.request.key.namespace,revision:review.request.expected_catalog_revision+1 };
        });
    }
    setupDraft(selection:CatalogObject,generation:number):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            if(generation!==this.view.generation||!this.view.revision||!this.view.namespace)throw new Error('read_current_catalog_before_setup');
            if(!control.catalogIndexSetupDraft)throw new Error('native_index_setup_unavailable');
            if(control.catalogPending().length)throw new Error('catalog_outcome_unconfirmed_reconcile_first');
            const review=await control.catalogIndexSetupDraft(validateIndexSetupSelection(selection),this.view.revision);current();
            const request=catalogObject(review.draft.request);
            if(catalogObject(request.key).namespace!==this.view.namespace||request.expected_catalog_revision!==this.view.revision)throw new Error('crossed_native_setup_review');
            this.view={...this.view,review:undefined,setupReview:review,submittedSetupReview:undefined,outcome:undefined};
        });
    }
    setupApply(generation:number):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            const review=this.view.setupReview;
            if(!review||generation!==this.view.generation||!control.catalogIndexSetupApply)throw new Error('stale_setup_review');
            this.view={...this.view,setupReview:undefined,submittedSetupReview:review};
            const outcome=await control.catalogIndexSetupApply(review,()=>{try{current();return generation===this.view.generation;}catch{return false;}});current();
            this.view={generation:this.view.generation+1,busy:true,outcome,submittedSetupReview:review};
        });
    }
    async useGrant(choice:CatalogObject,generation:number):Promise<CatalogObject>{
        this.sync();const owner=this.owner,sequence=++this.sequence,namespace=this.view.namespace,revision=this.view.revision;
        if(!this.host.authorized()||!this.host.useGrant)throw new Error('native_scope_selection_unavailable');
        if(this.view.busy||generation!==this.view.generation||!namespace||!revision)throw new Error('stale_catalog_view');
        if(this.host.control?.catalogPending().length)throw new Error('catalog_outcome_unconfirmed_reconcile_first');
        catalogBound(choice,4096);this.view={...this.view,busy:true,review:undefined,setupReview:undefined,error:undefined};this.host.changed();
        const valid=()=>{this.sync();return this.host.authorized()&&owner===this.owner&&sequence===this.sequence&&generation===this.view.generation;};
        try{
            // The host validates through the last await, then publishes a deliberate authority change.
            const outcome=await this.host.useGrant(structuredClone(choice),namespace,revision,valid);this.sync();
            if(!this.host.authorized())throw new Error('runtime_client_no_longer_authorized');
            this.view={...this.view,busy:false,outcome,generation:this.view.generation+1};this.host.changed();return outcome;
        }catch(error){if(owner===this.owner&&sequence===this.sequence){this.view={...this.view,busy:false,error:error instanceof Error?error.message:'scope_selection_failed'};this.host.changed();}throw error;}
    }
    revokeGrant(generation:number):Promise<CatalogView>{
        this.sync();const detail=this.view.grantDetail,item=catalogItems(detail,'detail')[0];
        if(!detail||!item||item.kind!=='grant'||catalogObject(detail.detail).revision!==this.view.revision)return Promise.reject(new Error('inspect_current_native_grant_before_revocation'));
        const grant=catalogObject(item.value.grant);if(grant.revoked!==false)return Promise.reject(new Error('grant_already_revoked'));
        return this.preparePreview({operation:'revoke_grant',id:grant.id,expected_revision:grant.revision},generation,false,true);
    }
    sourceDraft(change:CatalogObject,generation:number):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            const item=catalogItems(this.view.detail,'detail')[0];
            if(generation!==this.view.generation||!this.view.revision||!this.view.namespace||item?.kind!=='index')throw new Error('inspect_current_index_before_source_review');
            if(!control.catalogSourceDraft)throw new Error('native_source_draft_unavailable');
            const selection=validateSourceMutationSelection(change);
            if(selection.operation==='unblock_path'){
                if(!canReviewPolicyUnblock(this.view.policyStatus))throw new Error('read_terminal_policy_job_before_unblock_review');
                const snapshot=this.view.policyStatus?.snapshot,policy=snapshot?catalogObject(catalogObject(snapshot).policy):undefined;
                if(!policy||policy.index_id!==item.value.id||!Array.isArray(policy.denies)||!policy.denies.some(value=>{const rule=catalogObject(value);return rule.rule_id===selection.rule_id&&rule.reason==='trim';}))throw new Error('read_recorded_exclusion_before_unblock_review');
            }
            if(control.catalogPending().length&&(selection.operation!=='retire_index'||!control.catalogCanRetire?.(String(item.value.id),String(catalogObject(item.value.backing).binding),this.view.namespace)))throw new Error('catalog_outcome_unconfirmed_reconcile_first');
            const review=await control.catalogSourceDraft(String(item.value.id),selection,this.view.revision);current();
            if(review.request.key.namespace!==this.view.namespace||review.request.expected_catalog_revision!==this.view.revision||review.request.operation.id!==item.value.id)throw new Error('crossed_native_source_review');
            this.view={...this.view,review,setupReview:undefined,outcome:undefined,submittedReview:undefined};
        });
    }
    policyRead(generation:number):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            const item=catalogItems(this.view.detail,'detail')[0];
            if(generation!==this.view.generation||!this.view.namespace||item?.kind!=='index')throw new Error('inspect_current_index_before_policy_observation');
            if(!control.catalogPolicyStatus)throw new Error('native_policy_observation_unavailable');
            const response=await control.catalogPolicyStatus(String(item.value.id));current();
            if(response.status!=='catalog_policy_status'||response.index_id!==item.value.id)throw new Error('crossed_native_policy_observation');
            this.view={...this.view,policyStatus:response,review:undefined,setupReview:undefined,generation:this.view.generation+1};
        });
    }
    policyResume(generation:number):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            const observation=this.view.policyStatus,item=catalogItems(this.view.detail,'detail')[0];
            if(!observation||generation!==this.view.generation||item?.kind!=='index'||item.value.tombstone===true||observation.index_id!==item.value.id||!control.catalogPolicyResume)throw new Error('read_current_native_policy_observation');
            const response=await control.catalogPolicyResume(observation,()=>{try{current();return generation===this.view.generation;}catch{return false;}});current();
            this.view={...this.view,policyStatus:response,outcome:response,review:undefined,setupReview:undefined,generation:this.view.generation+1};
        });
    }
    sourceRead(begin:string,generation:number):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            catalogHex(begin);if(generation!==this.view.generation||!this.view.namespace)throw new Error('read_current_catalog_before_source_observation');
            if(!control.catalogSourceObservation)throw new Error('native_source_observation_unavailable');
            const previous=this.view.sourceObservation,body=previous?sourceChangeObservation(previous):undefined,prior=body&&catalogObject(catalogObject(body.begin).metadata).operation_id===begin?previous:undefined;
            const response=await control.catalogSourceObservation(begin,this.view.namespace,prior);current();
            this.view={...this.view,sourceObservation:response,review:undefined,setupReview:undefined,generation:this.view.generation+1};
        });
    }
    sourcePending(generation:number):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            const item=catalogItems(this.view.detail,'detail')[0];
            if(generation!==this.view.generation||!this.view.namespace||item?.kind!=='index')throw new Error('inspect_current_index_before_source_observation');
            if(!control.catalogPendingSource)throw new Error('native_pending_source_observation_unavailable');
            const response=await control.catalogPendingSource(String(item.value.id),this.view.namespace);current();
            if(response.index_id!==item.value.id||response.status!=='catalog_pending_source_change')throw new Error('crossed_pending_source_index');
            this.view={...this.view,sourceObservation:response,review:undefined,setupReview:undefined,generation:this.view.generation+1};
        });
    }
    sourceResolve(generation:number):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            const observation=this.view.sourceObservation,body=observation?sourceChangeObservation(observation):undefined;
            if(!observation||!body||generation!==this.view.generation||!control.catalogSourceResolve)throw new Error('read_current_failed_source_observation');
            const begin=String(catalogObject(catalogObject(body.begin).metadata).operation_id);
            if(control.catalogPending().includes(begin))throw new Error('source_resolution_requires_observation');
            if(body.completion!==null||catalogObject(body.job).state!=='failed')throw new Error('native_failed_source_job_required');
            const response=await control.catalogSourceResolve(observation,()=>{try{current();return generation===this.view.generation;}catch{return false;}});current();
            this.view={...this.view,sourceObservation:response,outcome:response,review:undefined,setupReview:undefined,generation:this.view.generation+1};
        });
    }
    setupResume(id:string,generation:number):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            catalogHex(id);if(generation!==this.view.generation||!control.catalogSetupResume)throw new Error('native_setup_resume_unavailable');
            const response=await control.catalogSetupResume(id,()=>{try{current();return generation===this.view.generation;}catch{return false;}});current();
            this.view={...this.view,review:undefined,setupReview:undefined,outcome:response,generation:this.view.generation+1};
        });
    }
    retirementRead(id:string,generation:number):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            if(generation!==this.view.generation||!control.catalogRetirementReview)throw new Error('native_retirement_recovery_unavailable');catalogHex(id);
            const review=await control.catalogRetirementReview(id);current();this.view={...this.view,retirementReview:review,review:undefined,setupReview:undefined};
        });
    }
    stopTracking(generation:number):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            const review=this.view.retirementReview;if(!review||generation!==this.view.generation||!control.catalogStopTracking)throw new Error('read_current_native_retirement_evidence');
            this.view={...this.view,retirementReview:undefined};
            const outcome=await control.catalogStopTracking(review,()=>{try{current();return generation===this.view.generation;}catch{return false;}});current();
            this.view={...this.view,outcome,generation:this.view.generation+1};
        });
    }
    reconcile(): Promise<CatalogView> {
        return this.run(async (control, current) => {
            const results = await control.catalogReconcile(); current();
            this.view = { ...this.view, review: undefined, outcome: { receipts: results }, generation: this.view.generation + 1 };
        });
    }
    splitOpen(index:string,generation:number):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            if(generation!==this.view.generation||!hasSplitCapability(this.view.capabilities)||!this.view.revision)throw new Error('split_capability_or_review_stale');
            const advice=this.view.recommendations;
            if(!advice||advice.schema!=='airk.recommendations/1'||!Array.isArray(catalogObject(advice.selected).index_ids)||!(catalogObject(advice.selected).index_ids as unknown[]).includes(index))throw new Error('select_an_advised_index_for_split_review');
            catalogId(index,'idx_');const response=await control.catalogRead({catalog_detail:{entity:{kind:'index',id:index},expected_catalog_revision:this.view.revision}});current();this.accept(response,'detail',this.view.revision);
            const item=catalogItems(response,'detail')[0];if(item?.kind!=='index'||item.value.id!==index)throw new Error('crossed_split_index');
            this.view={...this.view,detail:response,splitFormOpen:true,splitReview:undefined,review:undefined,setupReview:undefined,generation:this.view.generation+1};
        });
    }
    splitDraft(selection:CatalogObject,generation:number):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            const item=catalogItems(this.view.detail,'detail')[0];
            if(generation!==this.view.generation||!this.view.revision||!this.view.namespace||!hasSplitCapability(this.view.capabilities)||item?.kind!=='index'||item.value.tombstone!==false||!control.catalogSplitDraft||!this.view.splitSourceState)throw new Error('inspect_capable_current_index_before_split_review');
            validateSplitSourceState(this.view.splitSourceState,String(item.value.id),this.view.revision);
            if(control.catalogPending().length)throw new Error('catalog_outcome_unconfirmed_reconcile_first');
            const review=await control.catalogSplitDraft(String(item.value.id),validateSplitSelection(selection),this.view.revision);current();const request=catalogObject(review.draft.request);
            if(request.original_index!==item.value.id||request.expected_catalog_revision!==this.view.revision||catalogObject(request.key).namespace!==this.view.namespace)throw new Error('crossed_split_review');
            this.splitPreparedSequence=this.sequence;
            this.view={...this.view,splitReview:review,submittedSplitReview:undefined,outcome:undefined,review:undefined,setupReview:undefined};
        });
    }
    splitSourceRead(generation:number):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            const item=catalogItems(this.view.detail,'detail')[0];
            if(generation!==this.view.generation||!this.view.revision||!this.view.namespace||!hasSplitCapability(this.view.capabilities)||item?.kind!=='index'||!control.catalogSplitSourceState)throw new Error('inspect_current_index_before_split_source_state');
            const response=await control.catalogSplitSourceState(String(item.value.id),this.view.revision);current();validateSplitSourceState(response,String(item.value.id),this.view.revision);
            this.view={...this.view,splitSourceState:response,splitReview:undefined,capabilities:catalogObject(response.capabilities),generation:this.view.generation+1};
        });
    }
    splitApply(generation:number):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            const review=this.view.splitReview;if(!review||this.splitPreparedSequence!==this.sequence-1||generation!==this.view.generation||!hasSplitCapability(this.view.capabilities)||!control.catalogSplitApply)throw new Error('stale_split_review');
            this.view={...this.view,splitReview:undefined,submittedSplitReview:review};
            const response=await control.catalogSplitApply(review,()=>{try{current();return generation===this.view.generation;}catch{return false;}});current();
            this.view={...this.view,outcome:response,generation:this.view.generation+1};
        });
    }
    splitRead(operation:string,generation:number):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            if(generation!==this.view.generation||!this.view.namespace||!hasSplitCapability(this.view.capabilities)||!control.catalogSplitStatus)throw new Error('split_status_unavailable');catalogHex(operation);
            const response=await control.catalogSplitStatus(operation);current();if(response.operation_id!==operation)throw new Error('crossed_split_status');if(response.snapshot!==null)validateSplitSnapshot(response.snapshot);
            this.splitObservedSequence=this.sequence;
            this.view={...this.view,splitStatus:response,reviewedSplitStatus:undefined,splitReview:undefined,generation:this.view.generation+1};
        });
    }
    splitRecover(action:'resume'|'cancel',generation:number):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            const observation=this.view.splitStatus;if(!observation||this.splitObservedSequence!==this.sequence-1||observation.snapshot===null||generation!==this.view.generation||!hasSplitCapability(this.view.capabilities)||!control.catalogSplitRecover)throw new Error('read_current_split_status_before_recovery');
            this.view={...this.view,splitStatus:undefined,reviewedSplitStatus:observation};
            const response=await control.catalogSplitRecover(observation,action,()=>{try{current();return generation===this.view.generation;}catch{return false;}});current();
            this.view={...this.view,outcome:response,generation:this.view.generation+1};
        });
    }
}
