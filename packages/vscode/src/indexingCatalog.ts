// SPDX-License-Identifier: Apache-2.0
/** Administrative metadata only. These IDs and preferences confer no evidence access. */
export type CatalogObject = Record<string, unknown>;
export type CatalogKind = 'projects' | 'indexes' | 'memberships' | 'contexts' | 'grants' | 'sources';
export interface CatalogKey { namespace: string; epoch: number; nonce: string }
export interface CatalogRequest { caller: string; key: CatalogKey; expected_catalog_revision: number; operation: CatalogObject }
/** request_digest is local journal metadata; only catalog_apply is sent on the native transport. */
export interface CatalogApplyIntent { catalog_apply: { request: CatalogRequest; preview_digest: string }; request_digest: string }
export interface CatalogReview { request: CatalogRequest; response: CatalogObject; previewDigest: string; requestDigest: string }
export interface CatalogScopeAtom { source_root_id: string; kind: 'exact' | 'subtree'; path: string }
export type WindowContextKind = 'folder' | 'workspace' | 'empty' | 'untitled';
/** Supplied by the trusted host adapter, never inferred from a webview selection. */
export interface WindowContextDescriptor { host: string; key: string; kind: WindowContextKind; name: string }
/** Complete native administrative result, including future fields; never an access credential. */
export interface NativeHostContextDescriptor extends CatalogObject {
    schema: 1; namespace: string; host: string; key: string; catalog_revision: number; context: CatalogObject | null;
}
export interface CatalogView {
    generation: number; busy: boolean; error?: string; namespace?: string; revision?: number;
    page?: CatalogObject; detail?: CatalogObject; sources?: CatalogObject; memberships?: CatalogObject;
    review?: CatalogReview; outcome?: CatalogObject; pending?: readonly string[];
    windowDescriptor?: WindowContextDescriptor; context?: CatalogObject;
}
export const CATALOG_PAGE_BYTES = 48 * 1024;
export const CATALOG_RECEIPT_BYTES = 16 * 1024;
export const CATALOG_UI_OPERATIONS = ['create_project', 'rename_project', 'delete_project', 'rename_index', 'set_membership', 'remove_membership', 'set_project_defaults', 'set_context', 'delete_context'] as const;
export function catalogObject(value: unknown): CatalogObject {
    if (!value || typeof value !== 'object' || Array.isArray(value)) { throw new Error('invalid_catalog_object'); }
    return value as CatalogObject;
}
export function catalogBound(value: unknown, maximum = CATALOG_PAGE_BYTES): void {
    if (new TextEncoder().encode(JSON.stringify(value)).length > maximum) { throw new Error('catalog_response_oversize'); }
}
export function catalogId(value: unknown, prefix: 'idx_' | 'prj_' | 'src_' | 'ctx_' | 'grt_'): string {
    if (typeof value !== 'string' || !new RegExp(`^${prefix}[0-9a-f]{32}$`).test(value)) { throw new Error('invalid_catalog_id'); }
    return value;
}
export function catalogHex(value: unknown, length = 32): string {
    if (typeof value !== 'string' || !new RegExp(`^[0-9a-f]{${length}}$`).test(value)) { throw new Error('invalid_catalog_identity'); }
    return value;
}
export function catalogRevision(value: unknown): number {
    if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 1) { throw new Error('invalid_catalog_revision'); }
    return value;
}
function exact(value: CatalogObject, fields: string): void {
    if (Object.keys(value).sort().join(',') !== fields.split(',').sort().join(',')) { throw new Error('invalid_catalog_fields'); }
}
function label(value: unknown): void {
    const blank=/^[\u0009-\u000d\u0020\u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]*$/u;
    if (typeof value !== 'string' || blank.test(value) || new TextEncoder().encode(value).length > 256 || /[\u0000-\u001f\u007f-\u009f]/.test(value)) { throw new Error('invalid_catalog_name'); }
}
/** Match Core ScopeAtom/atoms exactly: UTF-8 bytes, canonical segments and no duplicate atoms. */
export function validateCatalogScope(input: unknown): CatalogScopeAtom[] {
    if (!Array.isArray(input) || input.length > 128) { throw new Error('invalid_membership_scope'); }
    const seen = new Set<string>();
    for (const inputAtom of input) {
        const atom = catalogObject(inputAtom); exact(atom, 'source_root_id,kind,path'); catalogId(atom.source_root_id, 'src_');
        if (!['exact', 'subtree'].includes(String(atom.kind)) || typeof atom.path !== 'string' || new TextEncoder().encode(atom.path).length > 4096 || /[\\:\u0000]/.test(atom.path)
            || (atom.path !== '' && atom.path.split('/').some(part => part === '' || part === '.' || part === '..')) || (!atom.path && atom.kind !== 'subtree')) { throw new Error('invalid_membership_scope'); }
        const identity=JSON.stringify([atom.source_root_id,atom.kind,atom.path]);
        if(seen.has(identity))throw new Error('duplicate_membership_scope');
        seen.add(identity);
    }
    return input as CatalogScopeAtom[];
}
export function catalogMembershipDraft(input: unknown): { index: string; revision: string; scope: CatalogScopeAtom[] } {
    const member=catalogObject(input);catalogId(member.index_id,'idx_');catalogRevision(member.revision);
    const scope=validateCatalogScope(member.scope);
    return {index:String(member.index_id),revision:String(member.revision),scope:scope.map(atom=>({...atom}))};
}
function indexes(value: unknown): void {
    if (!Array.isArray(value) || value.length > 32 || new Set(value).size !== value.length) { throw new Error('invalid_catalog_selection'); }
    value.forEach(id => catalogId(id, 'idx_'));
}
export function validateWindowContext(input: unknown): WindowContextDescriptor {
    const value=catalogObject(input);exact(value,'host,key,kind,name');catalogHex(value.host,64);catalogHex(value.key,64);label(value.name);
    if(!['folder','workspace','empty','untitled'].includes(String(value.kind)))throw new Error('invalid_window_context_kind');
    return {host:String(value.host),key:String(value.key),kind:value.kind as WindowContextKind,name:String(value.name)};
}
export function validateContextChoices(input: unknown, strictFields=true): CatalogObject {
    const value=catalogObject(input);if(strictFields)exact(value,'id,host,key,kind,name,revision,project_ids,index_ids');
    catalogId(value.id,'ctx_');catalogHex(value.host,64);catalogHex(value.key,64);label(value.name);catalogRevision(value.revision);
    if(!['folder','workspace','empty','untitled','legacy_pending'].includes(String(value.kind)))throw new Error('invalid_catalog_context_kind');
    if(!Array.isArray(value.project_ids)||value.project_ids.length>32||new Set(value.project_ids).size!==value.project_ids.length)throw new Error('invalid_context_projects');
    value.project_ids.forEach(id=>catalogId(id,'prj_'));indexes(value.index_ids);
    if(!value.project_ids.length&&(value.index_ids as unknown[]).length||value.kind==='legacy_pending'&&(value.project_ids.length||(value.index_ids as unknown[]).length))throw new Error('invalid_context_selection');
    return value;
}
export function validateNativeHostContext(input: unknown): NativeHostContextDescriptor {
    const value=catalogObject(input);catalogBound(value,16*1024);
    if(value.schema!==1)throw new Error('invalid_native_host_context_schema');
    catalogHex(value.namespace);catalogHex(value.host,64);catalogHex(value.key,64);catalogRevision(value.catalog_revision);
    if(value.context!==null){
        const context=validateContextChoices(value.context,false);
        if(context.host!==value.host||context.key!==value.key||Number(context.revision)>Number(value.catalog_revision))throw new Error('crossed_native_host_context');
    }
    return input as NativeHostContextDescriptor;
}
function validatePersistedGrant(input: unknown): void {
    const value=catalogObject(input),grant=catalogObject(value.grant),provenance=catalogObject(value.provenance);
    catalogId(grant.id,'grt_');catalogRevision(grant.revision);catalogHex(grant.host,64);
    if(typeof grant.revoked!=='boolean'||!Array.isArray(grant.memberships)||grant.memberships.length>1024)throw new Error('invalid_persisted_grant');
    if(provenance.kind==='imported_legacy_snapshot'){
        catalogHex(provenance.upgrade_id);catalogHex(provenance.plan_digest,64);catalogHex(provenance.legacy_binding,64);catalogHex(provenance.configuration_digest,64);
    }else if(provenance.kind!=='user_reviewed_project_grant')throw new Error('invalid_grant_provenance');
}
/** The UI exposes reviewed metadata operations; native grant preparation has a separate owner. */
export function validateCatalogOperation(input: unknown): CatalogObject {
    const op = catalogObject(input);
    switch (op.operation) {
        case 'create_project': exact(op, 'operation,id,name'); catalogId(op.id, 'prj_'); label(op.name); break;
        case 'rename_project': case 'rename_index':
            exact(op, 'operation,id,expected_definition_revision,name'); catalogId(op.id, op.operation === 'rename_index' ? 'idx_' : 'prj_');
            catalogRevision(op.expected_definition_revision); label(op.name); break;
        case 'delete_project':
            exact(op, 'operation,id,expected_definition_revision,expected_scope_revision,expected_defaults_revision'); catalogId(op.id, 'prj_');
            catalogRevision(op.expected_definition_revision); catalogRevision(op.expected_scope_revision); catalogRevision(op.expected_defaults_revision); break;
        case 'set_membership': {
            exact(op, 'operation,membership,expected_scope_revision,expected_membership_revision'); catalogRevision(op.expected_scope_revision);
            const member = catalogObject(op.membership); exact(member, 'project_id,index_id,revision,scope');
            catalogId(member.project_id, 'prj_'); catalogId(member.index_id, 'idx_');
            const prior = op.expected_membership_revision === null ? 0 : catalogRevision(op.expected_membership_revision);
            if (catalogRevision(member.revision) !== prior + 1) { throw new Error('invalid_membership_revision_or_scope'); }
            validateCatalogScope(member.scope);
            break;
        }
        case 'remove_membership':
            exact(op, 'operation,project_id,index_id,expected_scope_revision,expected_membership_revision'); catalogId(op.project_id, 'prj_'); catalogId(op.index_id, 'idx_');
            catalogRevision(op.expected_scope_revision); catalogRevision(op.expected_membership_revision); break;
        case 'set_project_defaults':
            exact(op, 'operation,project_id,expected_defaults_revision,index_ids'); catalogId(op.project_id, 'prj_'); catalogRevision(op.expected_defaults_revision); indexes(op.index_ids); break;
        case 'set_context': {
            exact(op,'operation,context,expected_revision');const context=validateContextChoices(op.context);
            const prior=op.expected_revision===null?0:catalogRevision(op.expected_revision);
            if(context.revision!==prior+1)throw new Error('invalid_next_context_revision');
            break;
        }
        case 'delete_context': exact(op,'operation,id,expected_revision');catalogId(op.id,'ctx_');catalogRevision(op.expected_revision);break;
        default: throw new Error('unsupported_catalog_ui_operation');
    }
    catalogBound(op); return op;
}
export function validateCatalogApply(input: unknown): CatalogApplyIntent {
    const envelope = catalogObject(input); exact(envelope, 'catalog_apply,request_digest');catalogHex(envelope.request_digest,64);
    const apply = catalogObject(envelope.catalog_apply); exact(apply, 'request,preview_digest'); catalogHex(apply.preview_digest, 64);
    const request = catalogObject(apply.request); exact(request, 'caller,key,expected_catalog_revision,operation'); catalogHex(request.caller); catalogRevision(request.expected_catalog_revision);
    const key = catalogObject(request.key); exact(key, 'namespace,epoch,nonce'); catalogHex(key.namespace); catalogHex(key.nonce);
    if (typeof key.epoch !== 'number' || !Number.isSafeInteger(key.epoch) || key.epoch < 0) { throw new Error('invalid_catalog_epoch'); }
    validateCatalogOperation(request.operation); catalogBound(request);catalogBound(input,65536);
    return input as CatalogApplyIntent;
}
export function catalogItems(response?: CatalogObject, field = 'page'): { kind: string; value: CatalogObject }[] {
    if (!response) { return []; }
    const body = catalogObject(response[field]);
    const items = field === 'detail' ? (body.item ? [body.item] : []) : body.items;
    if (!Array.isArray(items) || items.length > 32) { throw new Error('invalid_catalog_items'); }
    return items.map(item => {
        const row = catalogObject(item), value = catalogObject(row.value), kind = String(row.kind);
        if (kind === 'project' || kind === 'index') {
            catalogId(value.id, kind === 'project' ? 'prj_' : 'idx_'); label(value.name); catalogRevision(value.definition_revision);
            if (kind === 'project') { catalogRevision(value.scope_revision); catalogRevision(value.defaults_revision); indexes(value.default_indexes); }
            else { catalogRevision(value.source_revision); catalogObject(value.backing); }
        } else if (kind === 'membership') {
            catalogId(value.project_id, 'prj_'); catalogId(value.index_id, 'idx_'); catalogRevision(value.revision);
            validateCatalogScope(value.scope);
        } else if (kind === 'source') {
            catalogId(value.id, 'src_'); catalogId(value.index_id, 'idx_');
            if (typeof value.canonical_path !== 'string' || typeof value.owner !== 'boolean' || typeof value.enabled !== 'boolean' || !Array.isArray(value.aliases)) { throw new Error('invalid_catalog_source'); }
        } else if (kind === 'context') { validateContextChoices(value,false); }
        else if (kind === 'grant') { validatePersistedGrant(value); }
        else { throw new Error('invalid_catalog_item_kind'); }
        return { kind, value };
    });
}
/** Complete native responses, including unknown future fields, are retained in copies and exports. */
export function catalogExport(view: CatalogView): string { return JSON.stringify(view, null, 2); }
