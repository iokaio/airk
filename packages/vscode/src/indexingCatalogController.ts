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
export interface CatalogHost { control?: CatalogControl; identity(): string; authorized(): boolean; changed(): void; evidenceEpoch?():number; windowContext?(): WindowContextDescriptor | undefined; windowContextObservation?(): CatalogObject | undefined; canReadWindowContext?(): boolean; useGrant?(choice:CatalogObject,namespace:string,revision:number,valid:()=>boolean):Promise<CatalogObject>;searchScopeSelection?():{index_ids:string[];project_ids:string[];managedProjectIds?:string[];active:boolean};applySearchScope?(selection:CatalogObject):Promise<CatalogObject>;saveIndexSet?(selection:CatalogObject):Promise<CatalogObject> }
/** Window memory only; the shared native console owns durable intents and transport. No polling or source scanning. */
export class IndexingCatalogController {
    private owner?: string;
    private scopeOwner?: string;
    private nativeRevision?: number;
    private scopeSequence = 0;
    private splitPreparedSequence?:number;
    private splitObservedSequence?:number;
    private sequence = 0;
    private adviceSequence = 0;
    private view: CatalogView = { generation: 0, busy: false };
    private pageRequest?: CatalogObject;
    private membershipRequest?: CatalogObject;
    private sourceRequest?: CatalogObject;
    private grantRequest?: CatalogObject;
    private scopeApplying=false;
    constructor(private readonly host: CatalogHost) {}
    snapshot(): CatalogView {
        this.sync();
        return this.host.authorized() ? { ...this.view, adviceGeneration:this.adviceSequence, busy:this.view.busy||this.scopeApplying, canReviewSplit:hasSplitCapability(this.view.capabilities)&&!!this.host.control?.catalogSplitDraft&&!!this.host.control?.catalogSplitApply&&!!this.host.control?.catalogSplitSourceState, canReadPolicy:!!this.host.control?.catalogPolicyStatus,canResumePolicy:!!this.host.control?.catalogPolicyResume,canReviewRetirement:!!this.host.control?.catalogRetirementReview&&!!this.host.control?.catalogStopTracking,windowDescriptor:this.descriptor(),canReadWindowContext:this.host.canReadWindowContext?.(),canReviewGrant:!!this.host.control?.catalogGrantDraft,canUseGrant:!!this.host.useGrant,canReviewSource:!!this.host.control?.catalogSourceDraft,canReadPendingSource:!!this.host.control?.catalogPendingSource,canReviewSetup:!!this.host.control?.catalogIndexSetupDraft&&!!this.host.control?.catalogIndexSetupApply,pending: this.host.control?.catalogPending() ?? [],pendingDetails:this.host.control?.catalogPendingDetails?.() } : { generation: this.view.generation, busy: false, error: 'Workspace trust or client authority is unavailable.' };
    }
    /** Advice belongs to the evidence selection and native owner, independently
     * of management's sequence, reviews and busy state. */
    async recommendations(read:()=>Promise<CatalogObject>):Promise<CatalogView>{
        this.sync();
        if(!this.host.authorized())throw new Error('runtime_client_no_longer_authorized');
        this.adviceSequence++;
        const current=this.recommendationGuard();
        const response=await read();current();catalogBound(response,65536);
        this.view={...this.view,recommendations:response};this.adviceSequence++;this.host.changed();
        return this.snapshot();
    }
    /** Fence copy/use through their native revalidation, including a new read
     * returning the same object or a dismissal while no advice is stored. */
    recommendationGuard():()=>void {
        this.sync();const owner=this.owner,sequence=this.adviceSequence,epoch=this.host.evidenceEpoch?.();
        return ()=>{this.sync();if(!this.host.authorized()||owner!==this.owner||sequence!==this.adviceSequence||epoch!==this.host.evidenceEpoch?.())throw new Error('stale_catalog_view');};
    }
    /** Evidence selection owns advice, independently of an in-flight native
     * management review or its complete receipt. Clearing advice cannot retire
     * that operation's generation, request sequence or busy state. */
    clearEvidenceRecommendations():void {this.adviceSequence++;if(this.view.recommendations!==undefined)this.view={...this.view,recommendations:undefined};}
    dismissRecommendations():CatalogView {this.sync();this.clearEvidenceRecommendations();this.host.changed();return this.snapshot();}
    invalidate(): void {
        this.invalidateView(false);
    }
    private invalidateView(retainScope:boolean): void {
        if(!retainScope)this.scopeSequence++;
        this.sequence++; this.adviceSequence++; this.view = { generation: this.view.generation + 1, busy: false };
        this.pageRequest = this.membershipRequest = this.sourceRequest = undefined;
        this.grantRequest=undefined;
    }
    private sync(): void {
        let native:unknown[]|undefined,capabilities:CatalogObject|undefined;
        try{const observation=this.host.windowContextObservation?.(),observed=observation?.descriptor;if(observed&&typeof observed==='object'&&!Array.isArray(observed)){const value=validateNativeHostContext(observed);native=[value.namespace,value.host,value.key,value.catalog_revision];
            if(observation?.status==='catalog_host_context'&&observation.capabilities&&typeof observation.capabilities==='object')capabilities=catalogObject(observation.capabilities);
        }}catch{/* An unavailable producer cannot retain a prior authority identity. */}
        const owner = JSON.stringify([this.host.authorized(),this.host.identity(),this.descriptor(),native]);
        const scopeOwner=JSON.stringify([this.host.authorized(),this.host.identity(),this.descriptor(),native?.slice(0,3)]),revision=native?.[3] as number|undefined;
        if (owner !== this.owner) {
            // Scope orchestration refreshes the catalog clock after its own
            // journaled writes. That invalidates old reviews, but cannot retire
            // this action while its runtime/window/native authority is unchanged.
            const retainScope=this.scopeApplying&&scopeOwner===this.scopeOwner&&revision!==undefined&&this.nativeRevision!==undefined&&revision>this.nativeRevision;
            this.owner = owner; this.scopeOwner=scopeOwner;this.nativeRevision=revision;this.invalidateView(retainScope);
        }
        // Automatic connection observations establish capability without loading
        // library rows, changing membership, restoring a grant or starting work.
        if(capabilities&&this.view.capabilities===undefined)this.view={...this.view,capabilities};
    }
    private descriptor(): WindowContextDescriptor | undefined {
        try{const value=this.host.windowContext?.();return value===undefined?undefined:validateWindowContext(value);}catch{return undefined;}
    }
    private async run(work: (control: CatalogControl, current: () => void) => Promise<void>): Promise<CatalogView> {
        this.sync();
        if (!this.host.authorized()) { throw new Error('runtime_client_no_longer_authorized'); }
        if (this.view.busy||this.scopeApplying) { throw new Error('catalog_request_pending'); }
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
            this.view = { ...this.view, [field]: response, review: undefined,setupReview:undefined, ...(field === 'page' ? { pageKind:kind, detail: undefined, sources: undefined, memberships: undefined,policyStatus:undefined,splitFormOpen:undefined,splitSourceState:undefined,splitReview:undefined } : {}) };
            if (field === 'page') { this.pageRequest = request; } else if (field === 'sources') { this.sourceRequest = request; } else if(field==='grants'){this.grantRequest=request;}else { this.membershipRequest = request; }
            this.view.generation++;
        });
    }
    scopeRead(kind?:'indexes'|'projects',next=false):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            const native=validateNativeHostContext(this.host.windowContextObservation?.()?.descriptor);
            const prior=this.view.searchScope,selection=this.host.searchScopeSelection?.()??{index_ids:[],project_ids:[],managedProjectIds:[],active:false};
            let scope:NonNullable<CatalogView['searchScope']>=prior??{indexes:[],projects:[],selectedIndexIds:[],selectedProjectIds:[],managedProjectIds:[],active:false,responses:[]};
            if(this.view.namespace&&this.view.namespace!==native.namespace)throw new Error('catalog_namespace_changed_refresh');
            for(const category of kind?[kind]:['indexes','projects'] as const){
                const continuation=category==='indexes'?scope.indexContinuation:scope.projectContinuation;
                if(next&&!continuation)throw new Error('catalog_continuation_unavailable');
                const response=await control.catalogRead({catalog_page:{request:{kind:category,index_id:null,project_id:null,after:next?continuation:null,expected_catalog_revision:native.catalog_revision,limit:16,max_bytes:CATALOG_PAGE_BYTES}}});current();
                this.accept(response,'page',native.catalog_revision);
                const rows=catalogItems(response),expected=category==='indexes'?'index':'project';
                if(rows.some(row=>row.kind!==expected))throw new Error('crossed_search_scope_page');
                const field=category==='indexes'?'indexes':'projects',cursorField=category==='indexes'?'indexContinuation':'projectContinuation';
                const values=next?[...scope[field],...rows.map(row=>row.value)]:rows.map(row=>row.value);
                if(new Set(values.map(value=>value.id)).size!==values.length)throw new Error('search_scope_duplicate_items');
                const cursor=catalogObject(response.page).continuation;
                if(cursor!==null){const token=catalogObject(cursor);if(token.namespace!==native.namespace||token.revision!==native.catalog_revision||token.kind!==category||token.index_id!==null||token.project_id!==null||next&&JSON.stringify(cursor)===JSON.stringify(continuation))throw new Error('crossed_search_scope_continuation');}
                if(values.length>512)throw new Error('search_scope_display_limit');
                const responseField=category==='indexes'?'indexResponses':'projectResponses';
                const retained=next?(scope[responseField]??[]):[];
                scope={...scope,[field]:values,[cursorField]:cursor,[responseField]:[...retained,response]};
                scope.responses=[...(scope.indexResponses??[]),...(scope.projectResponses??[])];
            }
            this.view={...this.view,searchScope:{...scope,selectedIndexIds:selection.index_ids,selectedProjectIds:selection.project_ids,managedProjectIds:selection.managedProjectIds,active:selection.active},generation:this.view.generation+1};
        });
    }
    async scopeApply(selection:CatalogObject,generation:number,set=false):Promise<CatalogObject>{
        this.sync();
        const work=set?this.host.saveIndexSet:this.host.applySearchScope;
        if(!this.host.authorized()||!work)throw new Error('native_search_scope_unavailable');
        if(this.view.busy||this.scopeApplying||generation!==this.view.generation)throw new Error('stale_catalog_view');
        const owner=this.scopeOwner,sequence=++this.scopeSequence;this.sequence++;
        const current=()=>{this.sync();if(!this.host.authorized()||owner!==this.scopeOwner||sequence!==this.scopeSequence)throw new Error('stale_catalog_view');};
        this.scopeApplying=true;this.host.changed();
        try{
            const result=await work(selection);current();
            const selected=this.host.searchScopeSelection?.();
            this.view={...this.view,outcome:result,generation:this.view.generation+1,...(this.view.searchScope&&selected?{searchScope:{...this.view.searchScope,selectedIndexIds:selected.index_ids,selectedProjectIds:selected.project_ids,managedProjectIds:selected.managedProjectIds,active:selected.active}}:{})};
            return result;
        }catch(error){this.sync();if(owner===this.scopeOwner&&sequence===this.scopeSequence){const native=error&&typeof error==='object'?(error as {nativeResponse?:unknown}).nativeResponse:undefined;this.view={...this.view,...(native&&typeof native==='object'&&!Array.isArray(native)?{outcome:catalogObject(native)}:{}),error:error instanceof Error?error.message:'search_scope_failed'};}throw error;}
        finally{this.scopeApplying=false;this.host.changed();}
    }
    setRead(id:string):Promise<CatalogView>{
        return this.run(async(control,current)=>{
            catalogId(id,'prj_');const native=validateNativeHostContext(this.host.windowContextObservation?.()?.descriptor),responses:CatalogObject[]=[];
            const response=await control.catalogRead({catalog_detail:{entity:{kind:'project',id},expected_catalog_revision:native.catalog_revision}});current();this.accept(response,'detail',native.catalog_revision);responses.push(response);
            const item=catalogItems(response,'detail')[0];if(item?.kind!=='project'||item.value.id!==id)throw new Error('index_set_unavailable');
            const indexes:string[]=[];let after:unknown=null;const seen=new Set<string>();
            for(let attempt=0;attempt<16;attempt++){
                const result=await control.catalogRead({catalog_page:{request:{kind:'memberships',index_id:null,project_id:id,after,expected_catalog_revision:native.catalog_revision,limit:32,max_bytes:CATALOG_PAGE_BYTES}}});current();this.accept(result,'page',native.catalog_revision);responses.push(result);
                for(const row of catalogItems(result)){if(row.kind!=='membership'||row.value.project_id!==id)throw new Error('crossed_index_set_membership');indexes.push(catalogId(row.value.index_id,'idx_'));}
                if(indexes.length>32||new Set(indexes).size!==indexes.length)throw new Error('index_set_membership_limit');
                after=catalogObject(result.page).continuation;if(after===null){this.view={...this.view,detail:response,setEditor:{id,name:String(item.value.name),indexIds:indexes,complete:true,responses},generation:this.view.generation+1};return;}
                const token=JSON.stringify(after);if(seen.has(token))throw new Error('index_set_continuation_repeated');seen.add(token);
            }
            throw new Error('index_set_membership_limit');
        });
    }
    detail(kind: 'project' | 'index' | 'grant', id: string): Promise<CatalogView> {
        return this.run(async (control, current) => {
            catalogId(id, kind === 'grant'?'grt_':kind === 'index' ? 'idx_' : 'prj_');
            const expected = this.view.revision;
            const response = await control.catalogRead({ catalog_detail: { entity: { kind, id }, expected_catalog_revision: expected ?? null } });
            current(); this.accept(response, 'detail', expected);
            const item=catalogItems(response,'detail')[0];if(item&&(item.kind!==kind||(kind==='grant'?catalogObject(item.value.grant).id:item.value.id)!==id))throw new Error('crossed_catalog_detail');
            this.view = { ...this.view, ...(kind==='grant'?{grantDetail:response}:{detail:response,sources:undefined,memberships:undefined,policyStatus:undefined,splitFormOpen:undefined,splitSourceState:undefined,splitReview:undefined}), review: undefined,setupReview:undefined, generation: this.view.generation + 1 };
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
            this.view={generation:this.view.generation+1,busy:true,namespace:this.view.namespace,revision:this.view.revision,capabilities:this.view.capabilities,context:response};
        });
    }
    adoptHostContext(): Promise<CatalogView> {
        return this.run(async(_control,current)=>{
            const response=this.host.windowContextObservation?.(),descriptor=this.descriptor();
            if(!response||!descriptor||response.status!=='catalog_host_context')throw new Error('native_window_context_unavailable');
            const native=validateNativeHostContext(response.descriptor);current();
            if(native.host!==descriptor.host||native.key!==descriptor.key)throw new Error('crossed_window_context');
            // Capabilities belong to this exact authenticated observation, not a
            // package version or a prior connection that happened to support split.
            this.view={generation:this.view.generation+1,busy:true,namespace:native.namespace,revision:native.catalog_revision,
                ...(response.capabilities&&typeof response.capabilities==='object'?{capabilities:catalogObject(response.capabilities)}:{}),hostContext:response};
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
            if(this.host.canReadWindowContext?.()&&(!this.descriptor()||this.host.windowContextObservation?.()?.status!=='catalog_host_context'))throw new Error('read_native_window_context_before_setup_review');
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
            const advised=advice?.schema==='airk.recommendations/1'&&Array.isArray(catalogObject(advice.selected).index_ids)&&(catalogObject(advice.selected).index_ids as unknown[]).includes(index);
            const listed=this.view.pageKind==='indexes'&&catalogItems(this.view.page).some(row=>row.kind==='index'&&row.value.id===index&&row.value.tombstone===false);
            if(!advised&&!listed)throw new Error('select_a_listed_or_advised_index_for_split_review');
            catalogId(index,'idx_');const response=await control.catalogRead({catalog_detail:{entity:{kind:'index',id:index},expected_catalog_revision:this.view.revision}});current();this.accept(response,'detail',this.view.revision);
            const item=catalogItems(response,'detail')[0];if(item?.kind!=='index'||item.value.id!==index)throw new Error('crossed_split_index');
            if(item.value.tombstone!==false)throw new Error('split_requires_live_index');
            this.view={...this.view,detail:response,sources:undefined,memberships:undefined,policyStatus:undefined,splitSourceState:undefined,splitFormOpen:true,splitReview:undefined,review:undefined,setupReview:undefined,generation:this.view.generation+1};
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
