// SPDX-License-Identifier: Apache-2.0
import { CatalogKind, CatalogObject, CatalogReview, CatalogView, WindowContextDescriptor, catalogBound, catalogHex, catalogId, catalogItems, catalogObject, catalogRevision, CATALOG_PAGE_BYTES, validateCatalogOperation, validateWindowContext } from './indexingCatalog';
import { randomUUID } from 'node:crypto';

export interface CatalogControl {
    catalogRead(method: CatalogObject): Promise<CatalogObject>;
    catalogPrepare(operation: CatalogObject, revision: number): Promise<CatalogReview>;
    catalogApply(review: CatalogReview, valid: () => boolean): Promise<CatalogObject>;
    catalogReconcile(): Promise<CatalogObject[]>;
    catalogPending(): readonly string[];
}
export interface CatalogHost { control?: CatalogControl; identity(): string; authorized(): boolean; changed(): void; windowContext?(): WindowContextDescriptor | undefined }
/** Window memory only; the shared native console owns durable intents and transport. No polling or source scanning. */
export class IndexingCatalogController {
    private owner?: string;
    private sequence = 0;
    private view: CatalogView = { generation: 0, busy: false };
    private pageRequest?: CatalogObject;
    private membershipRequest?: CatalogObject;
    private sourceRequest?: CatalogObject;
    constructor(private readonly host: CatalogHost) {}
    snapshot(): CatalogView {
        this.sync();
        return this.host.authorized() ? { ...this.view, windowDescriptor:this.descriptor(),pending: this.host.control?.catalogPending() ?? [] } : { generation: this.view.generation, busy: false, error: 'Workspace trust or client authority is unavailable.' };
    }
    invalidate(): void {
        this.sequence++; this.view = { generation: this.view.generation + 1, busy: false };
        this.pageRequest = this.membershipRequest = this.sourceRequest = undefined;
    }
    private sync(): void {
        const owner = JSON.stringify([this.host.authorized(),this.host.identity(),this.descriptor()]);
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
                this.view = { ...this.view, review: undefined, error: error instanceof Error ? error.message : 'catalog_operation_failed' };
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
        this.view = { ...this.view, namespace, revision };
    }
    page(kind: CatalogKind, next = false, index?: string, project?: string): Promise<CatalogView> {
        return this.run(async (control, current) => {
            const field = kind === 'sources' ? 'sources' : kind === 'memberships' ? 'memberships' : 'page';
            const previous = field === 'sources' ? this.sourceRequest : field === 'memberships' ? this.membershipRequest : this.pageRequest;
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
            this.view = { ...this.view, [field]: response, review: undefined, ...(field === 'page' ? { detail: undefined, sources: undefined, memberships: undefined } : {}) };
            if (field === 'page') { this.pageRequest = request; } else if (field === 'sources') { this.sourceRequest = request; } else { this.membershipRequest = request; }
            this.view.generation++;
        });
    }
    detail(kind: 'project' | 'index', id: string): Promise<CatalogView> {
        return this.run(async (control, current) => {
            catalogId(id, kind === 'index' ? 'idx_' : 'prj_');
            const expected = this.view.revision;
            const response = await control.catalogRead({ catalog_detail: { entity: { kind, id }, expected_catalog_revision: expected ?? null } });
            current(); this.accept(response, 'detail', expected);
            this.view = { ...this.view, detail: response, sources: undefined, memberships: undefined, review: undefined, generation: this.view.generation + 1 };
        });
    }
    preview(operation: CatalogObject, generation: number): Promise<CatalogView> {
        return this.preparePreview(operation,generation,false);
    }
    private preparePreview(operation: CatalogObject, generation: number, contextReview: boolean): Promise<CatalogView> {
        return this.run(async (control, current) => {
            if (generation !== this.view.generation || !this.view.revision) { throw new Error('stale_catalog_view'); }
            validateCatalogOperation(operation);
            if(['set_context','delete_context'].includes(String(operation.operation))&&!contextReview)throw new Error('window_context_requires_host_review');
            if ((this.host.control?.catalogPending().length ?? 0) > 0) { throw new Error('catalog_outcome_unconfirmed_reconcile_first'); }
            const review = await control.catalogPrepare(operation, this.view.revision); current();
            if (review.request.key.namespace !== this.view.namespace) { throw new Error('catalog_namespace_changed_refresh'); }
            this.view = { ...this.view, review, outcome: undefined };
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
    previewContext(selection: CatalogObject, generation: number, remove=false): Promise<CatalogView> {
        this.sync();const descriptor=this.descriptor(),response=this.view.context;
        if(!descriptor||!response) return Promise.reject(new Error('read_window_context_before_review'));
        if(Object.keys(selection).sort().join(',')!=='index_ids,project_ids')return Promise.reject(new Error('invalid_context_selection_fields'));
        const prior=catalogItems(response,'detail')[0]?.value;
        if(remove){
            if(!prior)return Promise.reject(new Error('window_context_not_saved'));
            return this.preparePreview({operation:'delete_context',id:prior.id,expected_revision:prior.revision},generation,true);
        }
        return this.preparePreview({operation:'set_context',expected_revision:prior?.revision??null,context:{
            id:prior?.id??`ctx_${randomUUID().replaceAll('-','')}`,host:descriptor.host,key:descriptor.key,kind:descriptor.kind,name:descriptor.name,
            revision:prior?catalogRevision(prior.revision)+1:1,project_ids:selection.project_ids,index_ids:selection.index_ids,
        }},generation,true);
    }
    apply(generation: number): Promise<CatalogView> {
        return this.run(async (control, current) => {
            const review = this.view.review;
            if (!review || generation !== this.view.generation) { throw new Error('stale_catalog_review'); }
            // Consume before dispatch. Any interrupted reply is reconciled, never replayed.
            this.view = { ...this.view, review: undefined };
            const outcome = await control.catalogApply(review, () => { try { current(); return generation === this.view.generation; } catch { return false; } });
            current(); this.view = { generation: this.view.generation + 1, busy: true, outcome };
        });
    }
    reconcile(): Promise<CatalogView> {
        return this.run(async (control, current) => {
            const results = await control.catalogReconcile(); current();
            this.view = { ...this.view, review: undefined, outcome: { receipts: results }, generation: this.view.generation + 1 };
        });
    }
}
