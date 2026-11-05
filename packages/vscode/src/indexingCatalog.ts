// SPDX-License-Identifier: Apache-2.0
/** Administrative metadata only. These IDs and preferences confer no evidence access. */
export type CatalogObject = Record<string, unknown>;
export type CatalogKind = 'projects' | 'indexes' | 'memberships' | 'contexts' | 'grants' | 'sources';
export interface CatalogKey { namespace: string; epoch: number; nonce: string }
export interface CatalogRequest { caller: string; key: CatalogKey; expected_catalog_revision: number; operation: CatalogObject }
/** request_digest is local journal metadata; only catalog_apply is sent on the native transport. */
export interface CatalogApplyIntent { catalog_apply: { request: CatalogRequest; preview_digest: string }; request_digest: string }
export interface CatalogReview { request: CatalogRequest; response: CatalogObject; previewDigest: string; requestDigest: string; contextKey?: string }
export interface IndexSetupReview { response: CatalogObject; draft: CatalogObject; requestDigest: string }
/** Full read-only evidence; authorizes only an explicit local tracking decision. */
export interface CatalogRetirementReview { id:string; response:CatalogObject }
export interface IndexSetupApplyIntent { catalog_index_setup_apply: { draft: CatalogObject }; request_digest: string }
/** namespace is local journal metadata; only the exact resolve body crosses the wire. */
export interface SourceResolveIntent { catalog_source_change_resolve:{begin_operation:string}; namespace:string }
/** project is local receipt-routing metadata; only catalog_policy_resume crosses the wire. */
export interface PolicyResumeIntent { catalog_policy_resume:{index_id:string;operation_id:string;caller:string;key:CatalogKey}; project:string }
export interface CatalogScopeAtom { source_root_id: string; kind: 'exact' | 'subtree'; path: string }
export type WindowContextKind = 'folder' | 'workspace' | 'empty' | 'untitled';
/** Supplied by the trusted host adapter, never inferred from a webview selection. */
export interface WindowContextDescriptor { host: string; key: string; kind: WindowContextKind; name: string }
/** Complete native administrative result, including future fields; never an access credential. */
export interface NativeHostContextDescriptor extends CatalogObject {
    schema: 1; namespace: string; host: string; key: string; catalog_revision: number; context: CatalogObject | null;
}
export interface CatalogView {
    recommendations?: CatalogObject;
    generation: number; busy: boolean; error?: string; namespace?: string; revision?: number;
    page?: CatalogObject; detail?: CatalogObject; sources?: CatalogObject; memberships?: CatalogObject;
    review?: CatalogReview; outcome?: CatalogObject; pending?: readonly string[];
    submittedReview?: CatalogReview;
    windowDescriptor?: WindowContextDescriptor; context?: CatalogObject;
    hostContext?: CatalogObject; canReadWindowContext?: boolean;
    canReviewGrant?: boolean; pendingDetails?: readonly CatalogObject[];
    canReviewSetup?: boolean; setupReview?: IndexSetupReview; submittedSetupReview?: IndexSetupReview;
    grants?: CatalogObject; grantDetail?: CatalogObject; canUseGrant?: boolean;
    canReviewSource?:boolean; canReadPendingSource?:boolean; sourceObservation?:CatalogObject;
    retirementReview?:CatalogRetirementReview; canReviewRetirement?:boolean;
    policyStatus?:CatalogObject; canReadPolicy?:boolean; canResumePolicy?:boolean;
}
export const CATALOG_PAGE_BYTES = 48 * 1024;
export const CATALOG_RECEIPT_BYTES = 16 * 1024;
export const CATALOG_UI_OPERATIONS = ['create_project', 'rename_project', 'delete_project', 'rename_index', 'set_membership', 'remove_membership', 'set_project_defaults', 'set_context', 'delete_context', 'create_grant', 'revoke_grant', 'begin_index_source_change', 'retire_index', 'change_index_policy'] as const;
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
export function sameCatalogValue(left: unknown, right: unknown): boolean {
    if(left===right)return true;
    if(Array.isArray(left)||Array.isArray(right))return Array.isArray(left)&&Array.isArray(right)&&left.length===right.length&&left.every((value,index)=>sameCatalogValue(value,right[index]));
    if(!left||!right||typeof left!=='object'||typeof right!=='object')return false;
    const a=left as CatalogObject,b=right as CatalogObject,keys=Object.keys(a).sort();
    return sameCatalogValue(keys,Object.keys(b).sort())&&keys.every(key=>sameCatalogValue(a[key],b[key]));
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
export function validateNativeLegacyBinding(input:unknown,project:string,expectedRevision:number|null=null):CatalogObject{
    const response=catalogObject(input);catalogBound(response,65536);if(response.status!=='catalog_legacy_binding')throw new Error('invalid_native_legacy_binding_status');
    const value=catalogObject(response.observation);catalogBound(value);catalogHex(value.namespace);catalogHex(value.project,64);
    if(value.schema!==1||value.project!==project||value.catalog_revision!==null&&catalogRevision(value.catalog_revision)<1||expectedRevision!==null&&value.catalog_revision!==expectedRevision)throw new Error('crossed_native_legacy_binding');
    const eligibility=catalogObject(value.eligibility);
    if(eligibility.kind==='unavailable')return response;
    if(!['authority_zero','imported_snapshot'].includes(String(eligibility.kind)))throw new Error('invalid_native_legacy_binding_eligibility');
    const registration=catalogObject(eligibility.registration);catalogHex(registration.binding,64);setupPath(registration.root);setupPath(registration.data_root);
    if(registration.binding!==project||typeof registration.collection!=='string'||!/^[a-zA-Z0-9._-]{1,128}$/.test(registration.collection)||['.','..'].includes(registration.collection))throw new Error('crossed_native_legacy_registration');
    if(eligibility.kind==='imported_snapshot'){
        if(value.catalog_revision===null)throw new Error('invalid_native_legacy_revision');validatePersistedGrant(eligibility.grant);
        const persisted=catalogObject(eligibility.grant),grant=catalogObject(persisted.grant),provenance=catalogObject(persisted.provenance);
        if(grant.revoked!==false||provenance.kind!=='imported_legacy_snapshot'||provenance.legacy_binding!==project)throw new Error('crossed_native_legacy_grant');
    }
    return response;
}
export function catalogWindowAssociation(view: CatalogView): CatalogObject | undefined {
    if(view.hostContext)return validateNativeHostContext(view.hostContext.descriptor).context??undefined;
    return catalogItems(view.context,'detail')[0]?.value;
}
function validatePersistedGrant(input: unknown): void {
    const value=catalogObject(input),grant=catalogObject(value.grant),provenance=catalogObject(value.provenance);
    catalogId(grant.id,'grt_');catalogRevision(grant.revision);catalogHex(grant.host,64);
    if(typeof grant.revoked!=='boolean'||!Array.isArray(grant.memberships)||grant.memberships.length>1024)throw new Error('invalid_persisted_grant');
    if(provenance.kind==='imported_legacy_snapshot'){
        catalogHex(provenance.upgrade_id);catalogHex(provenance.plan_digest,64);catalogHex(provenance.legacy_binding,64);catalogHex(provenance.configuration_digest,64);
    }else if(provenance.kind!=='user_reviewed_project_grant')throw new Error('invalid_grant_provenance');
}
function validateReviewedGrant(input: unknown): void {
    const grant=catalogObject(input);exact(grant,'id,host,revision,memberships,revoked');catalogId(grant.id,'grt_');catalogHex(grant.host,64);
    if(grant.revision!==1||grant.revoked!==false||!Array.isArray(grant.memberships)||grant.memberships.length>1024||parseInt(String(grant.id).slice(4,6),16)>=128)throw new Error('invalid_native_reviewed_grant');
    const pairs=new Set<string>(),projects=new Set<string>(),members=new Set<string>();
    for(const input of grant.memberships){
        const frozen=catalogObject(input);exact(frozen,'membership,source_revision,policy_epoch,policy_digest');
        const member=catalogObject(frozen.membership);exact(member,'project_id,index_id,revision,scope');
        catalogId(member.project_id,'prj_');catalogId(member.index_id,'idx_');catalogRevision(member.revision);validateCatalogScope(member.scope);catalogRevision(frozen.source_revision);
        if(typeof frozen.policy_epoch!=='number'||!Number.isSafeInteger(frozen.policy_epoch)||frozen.policy_epoch<0||(frozen.policy_epoch===0)!==(frozen.policy_digest===null))throw new Error('invalid_native_grant_policy');
        if(frozen.policy_digest!==null)catalogHex(frozen.policy_digest,64);
        const pair=JSON.stringify([member.project_id,member.index_id]);if(pairs.has(pair))throw new Error('duplicate_native_grant_membership');pairs.add(pair);projects.add(String(member.project_id));members.add(String(member.index_id));
    }
    if(projects.size>32||members.size>32)throw new Error('invalid_native_grant_selection');
}
export function validateGrantSelection(input: unknown): {project_ids: string[];index_ids: string[] | null} {
    const selection=catalogObject(input);exact(selection,'project_ids,index_ids');
    if(!Array.isArray(selection.project_ids)||!selection.project_ids.length||selection.project_ids.length>32||new Set(selection.project_ids).size!==selection.project_ids.length)throw new Error('invalid_grant_projects');
    selection.project_ids.forEach(id=>catalogId(id,'prj_'));
    if(selection.index_ids!==null){indexes(selection.index_ids);if(!(selection.index_ids as unknown[]).length)throw new Error('invalid_grant_indexes');}
    return {project_ids:[...selection.project_ids],index_ids:selection.index_ids===null?null:[...selection.index_ids as string[]]};
}
export function catalogReviewPreview(review: CatalogReview): CatalogObject {
    return catalogObject(['catalog_grant_draft','catalog_source_mutation_draft'].includes(String(review.response.status))?catalogObject(review.response.draft).preview:review.response.preview);
}
/** Return the original nested enum; never normalize or replace the complete native reply. */
export function catalogFenceObservation(response: CatalogObject): CatalogObject | undefined {
    const observation=response.observation==null?undefined:catalogObject(response.observation);
    const fence=response.fence??(observation?.kind==='authority_fence'?observation.fence:undefined);
    return fence==null?undefined:catalogObject(fence);
}
export function validateNativeGrantDraft(input: unknown, selection: CatalogObject): CatalogReview {
    const response=catalogObject(input);if(response.status!=='catalog_grant_draft')throw new Error('invalid_native_grant_draft_status');
    const draft=catalogObject(response.draft),preview=catalogObject(draft.preview);catalogBound(draft);
    const prepared=validateCatalogApply({catalog_apply:{request:draft.request,preview_digest:draft.preview_digest},request_digest:preview.request_digest});
    const request=prepared.catalog_apply.request;
    if(request.operation.operation!=='create_grant'||!sameCatalogValue(request.key,selection.key)||request.expected_catalog_revision!==selection.expected_catalog_revision
        ||preview.catalog_revision!==request.expected_catalog_revision||preview.next_revision!==request.expected_catalog_revision+1||!sameCatalogValue(preview.operation,request.operation)
        ||typeof preview.authority_fence_required!=='boolean'||typeof preview.file_data_changes!=='boolean')throw new Error('crossed_native_grant_draft');
    const chosen=validateGrantSelection({project_ids:selection.project_ids,index_ids:selection.index_ids});
    const grants=catalogObject(request.operation.grant).memberships as CatalogObject[];
    const projects=new Set(grants.map(value=>String(catalogObject(value.membership).project_id))),indexes=new Set(grants.map(value=>String(catalogObject(value.membership).index_id)));
    if(projects.size!==chosen.project_ids.length||chosen.project_ids.some(id=>!projects.has(id))||chosen.index_ids!==null&&(indexes.size!==chosen.index_ids.length||chosen.index_ids.some(id=>!indexes.has(id))))throw new Error('crossed_native_grant_selection');
    return {request,response,previewDigest:prepared.catalog_apply.preview_digest,requestDigest:prepared.request_digest};
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
        case 'create_grant': exact(op,'operation,grant');validateReviewedGrant(op.grant);break;
        case 'revoke_grant':exact(op,'operation,id,expected_revision');catalogId(op.id,'grt_');catalogRevision(op.expected_revision);break;
        case 'begin_index_source_change':exact(op,'operation,id,expected_source_revision,plan');catalogId(op.id,'idx_');catalogRevision(op.expected_source_revision);validateSourceChangePlan(op.plan);break;
        case 'change_index_policy':{
            exact(op,'operation,id,expected_source_revision,expected_policy_epoch,expected_policy_digest,change,configuration,engine_identity');catalogId(op.id,'idx_');catalogRevision(op.expected_source_revision);
            if(typeof op.expected_policy_epoch!=='number'||!Number.isSafeInteger(op.expected_policy_epoch)||op.expected_policy_epoch<0||(op.expected_policy_epoch===0)!==(op.expected_policy_digest===null))throw new Error('invalid_source_policy_revision');
            if(op.expected_policy_digest!==null)catalogHex(op.expected_policy_digest,64);const change=validatePolicyChange(op.change);catalogHex(op.engine_identity,64);
            const policy=validateNativePolicyConfiguration(op.configuration);
            if(policy.index_id!==op.id||policy.epoch!==op.expected_policy_epoch+1)throw new Error('crossed_native_policy_configuration');
            const denies=policy.denies as CatalogObject[];
            if(change.operation==='trim'){
                const selector=catalogObject(change.selector),added=denies.find(rule=>rule.rule_id===change.rule_id);
                if(!added||added.reason!=='trim'||added.created_epoch!==policy.epoch||added.inherited_from!==undefined||!sameCatalogValue(selector,{source_root_id:added.source_root_id,kind:added.kind,path:added.path}))throw new Error('crossed_native_policy_change');
            }else if(denies.some(rule=>rule.rule_id===change.rule_id))throw new Error('crossed_native_policy_change');break;
        }
        case 'retire_index':{
            exact(op,'operation,id,expected_definition_revision,expected_source_revision,reason');catalogId(op.id,'idx_');catalogRevision(op.expected_definition_revision);catalogRevision(op.expected_source_revision);
            const reason=catalogObject(op.reason);exact(reason,'reason');if(reason.reason!=='explicit')throw new Error('native_explicit_retirement_required');break;
        }
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
/** Native read output only: missing-source retirement cannot be submitted by the UI. */
export function validateNativeRetirementRequest(input:unknown):CatalogRequest{
    const request=catalogObject(input),key=catalogObject(request.key),operation=catalogObject(request.operation),reason=catalogObject(operation.reason);
    catalogHex(request.caller);catalogHex(key.namespace);catalogHex(key.nonce);catalogRevision(request.expected_catalog_revision);
    if(!Number.isSafeInteger(key.epoch)||Number(key.epoch)<0)throw new Error('invalid_catalog_epoch');
    catalogId(operation.id,'idx_');catalogRevision(operation.expected_definition_revision);catalogRevision(operation.expected_source_revision);
    if(operation.operation!=='retire_index')throw new Error('native_retirement_required');
    if(reason.reason==='confirmed_missing_source'){
        if(!Number.isSafeInteger(reason.registration_epoch)||Number(reason.registration_epoch)<0||!Number.isSafeInteger(reason.configuration_epoch)||Number(reason.configuration_epoch)<0)throw new Error('invalid_native_retirement_epochs');
    }else if(reason.reason!=='explicit')throw new Error('invalid_native_retirement_reason');
    catalogBound(request);return input as CatalogRequest;
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

function setupPath(input: unknown): string {
    if(typeof input!=='string'||!input||new TextEncoder().encode(input).length>4096||/[\u0000-\u001f\u007f-\u009f]/.test(input))throw new Error('invalid_setup_path');
    return input;
}
function setupKey(input: unknown): CatalogKey {
    const key=catalogObject(input);exact(key,'namespace,epoch,nonce');catalogHex(key.namespace);catalogHex(key.nonce);
    if(typeof key.epoch!=='number'||!Number.isSafeInteger(key.epoch)||key.epoch<0)throw new Error('invalid_setup_key');
    return input as CatalogKey;
}
/** User choices only. The native producer owns canonical paths, IDs and storage decisions. */
export function validateIndexSetupSelection(input: unknown): CatalogObject {
    const value=catalogObject(input);exact(value,'name,primary_root,data_root,collection,additional_roots,setup_watch');label(value.name);setupPath(value.primary_root);setupPath(value.data_root);
    if(typeof value.collection!=='string'||!value.collection||new TextEncoder().encode(value.collection).length>255||typeof value.setup_watch!=='boolean'||!Array.isArray(value.additional_roots)||value.additional_roots.length>16)throw new Error('invalid_setup_selection');
    const aliases=new Set<string>();
    for(const input of value.additional_roots){const root=catalogObject(input);exact(root,'alias,path');setupPath(root.path);if(typeof root.alias!=='string'||!/^[a-zA-Z0-9_-]{1,64}$/.test(root.alias)||aliases.has(root.alias))throw new Error('invalid_setup_alias');aliases.add(root.alias);}
    catalogBound(value,CATALOG_RECEIPT_BYTES);return value;
}
export function validateIndexSetupRequest(input: unknown): CatalogObject {
    const value=catalogObject(input);catalogBound(value,CATALOG_RECEIPT_BYTES);catalogHex(value.caller);setupKey(value.key);catalogRevision(value.expected_catalog_revision);label(value.name);
    const registration=catalogObject(value.registration);catalogHex(registration.binding,64);setupPath(registration.root);setupPath(registration.data_root);
    if(typeof registration.collection!=='string'||!/^[a-zA-Z0-9._-]{1,128}$/.test(registration.collection)||['.','..'].includes(registration.collection)||typeof value.setup_watch!=='boolean'||!Array.isArray(value.sources)||!value.sources.length||value.sources.length>17)throw new Error('invalid_native_setup_request');
    const paths=new Set<string>(),aliases=new Set<string>();let owners=0;
    for(const input of value.sources){const root=catalogObject(input),path=setupPath(root.canonical_path);
        if(paths.has(path)||typeof root.owner!=='boolean'||root.owner!==(path===registration.root)||!Array.isArray(root.aliases)||root.aliases.length!==(root.owner?0:1))throw new Error('invalid_native_setup_source');paths.add(path);if(root.owner)owners++;
        for(const alias of root.aliases){if(typeof alias!=='string'||!/^[a-zA-Z0-9_-]{1,64}$/.test(alias)||aliases.has(alias))throw new Error('invalid_native_setup_alias');aliases.add(alias);}
    }
    if(owners!==1)throw new Error('invalid_native_setup_owner');return value;
}
export function validateIndexSetupDraft(input: unknown): CatalogObject {
    const draft=catalogObject(input);catalogBound(draft,CATALOG_RECEIPT_BYTES);validateIndexSetupRequest(draft.request);catalogHex(draft.review_digest,64);
    if(draft.schema!==1||!['create_missing_leaf','claim_existing_empty_storage','reuse_owned_storage','resume_reserved_storage_claim'].includes(String(draft.storage_action)))throw new Error('invalid_native_setup_draft');return draft;
}
export function validateIndexSetupApply(input: unknown): IndexSetupApplyIntent {
    const value=catalogObject(input);exact(value,'catalog_index_setup_apply,request_digest');catalogHex(value.request_digest,64);const apply=catalogObject(value.catalog_index_setup_apply);exact(apply,'draft');validateIndexSetupDraft(apply.draft);catalogBound(value,65536);return input as IndexSetupApplyIntent;
}
export function validateNativeIndexSetupDraft(input: unknown,selection: CatalogObject): IndexSetupReview {
    const response=catalogObject(input);catalogBound(response,65536);if(response.status!=='catalog_index_setup_draft')throw new Error('invalid_native_setup_draft_status');
    const draft=validateIndexSetupDraft(response.draft),request=catalogObject(draft.request);
    if(!sameCatalogValue(request.key,selection.key)||request.expected_catalog_revision!==selection.expected_catalog_revision||request.name!==selection.name||request.setup_watch!==selection.setup_watch||catalogObject(request.registration).collection!==selection.collection)throw new Error('crossed_native_setup_draft');
    // Paths are deliberately not compared to user spelling: native canonicalization is authoritative.
    return {response,draft,requestDigest:catalogHex(response.request_digest,64)};
}
/** Validate full native receipts; cryptographic derivations remain native, never synthesized in JS. */
export function validateIndexSetupReceipt(input: unknown,draftInput: unknown,priorInput?:unknown): CatalogObject {
    const draft=validateIndexSetupDraft(draftInput),request=catalogObject(draft.request),receipt=catalogObject(input);catalogBound(receipt,CATALOG_RECEIPT_BYTES);
    if(receipt.schema!==1||!sameCatalogValue(receipt.request,request))throw new Error('crossed_setup_receipt');
    catalogHex(receipt.operation_id);catalogHex(receipt.request_digest,64);catalogId(receipt.index_id,'idx_');catalogRevision(receipt.catalog_revision);catalogRevision(receipt.source_revision);
    if(!Array.isArray(receipt.source_ids)||receipt.source_ids.length!==(request.sources as unknown[]).length||new Set(receipt.source_ids).size!==receipt.source_ids.length)throw new Error('invalid_setup_source_ids');receipt.source_ids.forEach(id=>catalogId(id,'src_'));
    if(!['reserved','prepared','ready'].includes(String(receipt.stage)))throw new Error('invalid_setup_stage');
    const initial=Number(request.expected_catalog_revision)+1,prepared=receipt.preparation==null?undefined:catalogObject(receipt.preparation),completion=receipt.completion==null?undefined:catalogObject(receipt.completion);
    if(receipt.stage==='reserved'&&(prepared||completion)||receipt.stage==='prepared'&&(!prepared||completion)||receipt.stage==='ready'&&(!prepared||!completion))throw new Error('incomplete_setup_receipt');
    if(prepared){catalogHex(prepared.digest,64);catalogHex(prepared.storage_identity,64);catalogHex(prepared.source_configuration_digest,64);const review=catalogObject(prepared.review);if(catalogRevision(review.expected_catalog_revision)<initial||review.expected_source_revision!==1)throw new Error('invalid_setup_preparation_revision');}
    if(completion){if(completion.prepared_digest!==prepared?.digest||completion.expected_source_revision!==1||catalogRevision(completion.expected_catalog_revision)<Number(catalogObject(prepared?.review).expected_catalog_revision))throw new Error('crossed_setup_completion');}
    if(receipt.catalog_revision!==(completion?Number(completion.expected_catalog_revision)+1:initial)||receipt.source_revision!==(receipt.stage==='ready'?2:1))throw new Error('invalid_setup_receipt_revision');
    if(priorInput){const prior=catalogObject(priorInput),stages=['reserved','prepared','ready'];if(!sameCatalogValue(prior.request,receipt.request)||prior.operation_id!==receipt.operation_id||prior.request_digest!==receipt.request_digest||prior.index_id!==receipt.index_id||!sameCatalogValue(prior.source_ids,receipt.source_ids)||stages.indexOf(String(prior.stage))>stages.indexOf(String(receipt.stage))||prior.preparation!=null&&!sameCatalogValue(prior.preparation,receipt.preparation)||prior.completion!=null&&!sameCatalogValue(prior.completion,receipt.completion))throw new Error('crossed_setup_receipt_continuity');}
    return receipt;
}
export function indexSetupObservation(response: CatalogObject): CatalogObject | undefined {
    const observed=response.observation==null?undefined:catalogObject(response.observation);
    const receipt=response.receipt??(observed?.kind==='index_setup'?observed.receipt:undefined);return receipt==null?undefined:catalogObject(receipt);
}

function sourceSnapshot(input:unknown):CatalogObject{
    const value=catalogObject(input);exact(value,'primary_root,aliases');setupPath(value.primary_root);const aliases=catalogObject(value.aliases),paths=new Set([value.primary_root]);
    if(Object.keys(aliases).length>16)throw new Error('source_alias_capacity_exceeded');
    for(const [alias,path] of Object.entries(aliases)){if(!/^[a-zA-Z0-9_-]{1,64}$/.test(alias)||paths.has(setupPath(path)))throw new Error('invalid_source_alias');paths.add(path);}
    return value;
}
export function validateSourceChangePlan(input:unknown):CatalogObject{
    const plan=catalogObject(input);exact(plan,'registration,engine_identity,job,configuration,before,after,sources');catalogBound(plan,CATALOG_RECEIPT_BYTES);
    const registration=catalogObject(plan.registration);catalogHex(registration.binding,64);setupPath(registration.root);setupPath(registration.data_root);
    if(typeof registration.collection!=='string'||!/^[a-zA-Z0-9._-]{1,128}$/.test(registration.collection)||['.','..'].includes(registration.collection))throw new Error('invalid_source_registration');
    catalogHex(plan.engine_identity,64);catalogHex(plan.job,64);const configuration=catalogObject(plan.configuration);exact(configuration,configuration.prepare===undefined?'incarnation,settings,configuration':'prepare,incarnation,settings,configuration');
    // Native concrete collection identity/settings are independent of registry keys and backing IDs.
    catalogHex(configuration.incarnation);catalogHex(configuration.settings,64);
    if(configuration.prepare!==undefined&&configuration.prepare!==false)throw new Error('native_concrete_source_configuration_required');
    const before=sourceSnapshot(plan.before),after=sourceSnapshot(plan.after),change=catalogObject(configuration.configuration);
    if(before.primary_root!==registration.root||after.primary_root!==registration.root)throw new Error('source_change_cannot_relink_primary');
    const expected=new Map(Object.entries(catalogObject(before.aliases)));
    if(change.operation==='add_root'){exact(change,'operation,alias,path');setupPath(change.path);if(typeof change.alias!=='string'||!/^[a-zA-Z0-9_-]{1,64}$/.test(change.alias)||expected.has(change.alias))throw new Error('invalid_source_add');expected.set(change.alias,change.path);}
    else if(change.operation==='remove_root'){exact(change,'operation,alias');if(typeof change.alias!=='string'||!expected.delete(change.alias))throw new Error('invalid_source_remove');}
    else throw new Error('invalid_source_configuration_operation');
    if(!sameCatalogValue(Object.fromEntries(expected),after.aliases)||sameCatalogValue(before,after)||!Array.isArray(plan.sources)||plan.sources.length!==expected.size+1||plan.sources.length>17)throw new Error('crossed_native_source_plan');
    const ids=new Set<string>(),paths=new Set<string>(),described=new Map<string,unknown>();let owners=0;
    for(const input of plan.sources){const source=catalogObject(input);exact(source,'id,canonical_path,aliases,owner');const id=catalogId(source.id,'src_'),path=setupPath(source.canonical_path);
        if(ids.has(id)||paths.has(path)||typeof source.owner!=='boolean'||source.owner!==(path===registration.root)||!Array.isArray(source.aliases)||source.aliases.length!==(source.owner?0:1))throw new Error('invalid_native_source_descriptor');ids.add(id);paths.add(path);
        if(source.owner)owners++;else{const alias=source.aliases[0];if(typeof alias!=='string'||described.has(alias)||expected.get(alias)!==path)throw new Error('crossed_native_source_alias');described.set(alias,path);}
    }
    if(owners!==1||!sameCatalogValue(Object.fromEntries(described),after.aliases))throw new Error('incomplete_native_source_plan');return plan;
}
export function validateSourceMutationSelection(input:unknown):CatalogObject{
    const change=catalogObject(input);
    if(change.operation==='add_root'){exact(change,'operation,alias,path');setupPath(change.path);}
    else if(change.operation==='remove_root')exact(change,'operation,alias');
    else if(change.operation==='retire_index'){exact(change,'operation');return change;}
    else if(change.operation==='trim_path'){
        exact(change,'operation,source_root_id,kind,path');validateCatalogScope([{source_root_id:change.source_root_id,kind:change.kind,path:change.path}]);catalogBound(change,CATALOG_RECEIPT_BYTES);return change;
    }
    else if(change.operation==='unblock_path'){
        exact(change,'operation,rule_id');validatePolicyChange({operation:'unblock',rule_id:change.rule_id});return change;
    }
    else throw new Error('invalid_source_mutation_selection');
    if(typeof change.alias!=='string'||!/^[a-zA-Z0-9_-]{1,64}$/.test(change.alias))throw new Error('invalid_source_alias');catalogBound(change,CATALOG_RECEIPT_BYTES);return change;
}
export function validateNativeSourceDraft(input:unknown,selection:CatalogObject):CatalogReview{
    const response=catalogObject(input);catalogBound(response,65536);if(response.status!=='catalog_source_mutation_draft')throw new Error('invalid_native_source_draft_status');
    const draft=catalogObject(response.draft),preview=catalogObject(draft.preview);catalogBound(draft);
    const intent=validateCatalogApply({catalog_apply:{request:draft.request,preview_digest:draft.preview_digest},request_digest:preview.request_digest}),request=intent.catalog_apply.request,operation=request.operation,chosen=validateSourceMutationSelection(selection.change);
    if(!sameCatalogValue(request.key,selection.key)||request.expected_catalog_revision!==selection.expected_catalog_revision||operation.id!==selection.index_id||!sameCatalogValue(preview.operation,operation)||preview.catalog_revision!==request.expected_catalog_revision||preview.next_revision!==request.expected_catalog_revision+1||preview.authority_fence_required!==true||preview.file_data_changes!==(operation.operation==='change_index_policy'))throw new Error('crossed_native_source_draft');
    if(chosen.operation==='trim_path'||chosen.operation==='unblock_path'){
        if(operation.operation!=='change_index_policy')throw new Error('crossed_native_source_selection');
        const impact=validatePolicyImpact(preview.source_policy_impact),target=catalogObject(impact.target),policy=catalogObject(catalogObject(catalogObject(operation.configuration).configuration).policy);
        const root=(policy.roots as CatalogObject[]).find(root=>root.id===target.source_root_id);
        if(!root||root.canonical_path!==impact.canonical_root)throw new Error('crossed_native_policy_impact_root');
        const change=validatePolicyChange(operation.change);
        if(chosen.operation==='trim_path'){
            if(change.operation!=='trim'||!sameCatalogValue(change.selector,{source_root_id:chosen.source_root_id,kind:chosen.kind,path:chosen.path})||!sameCatalogValue(change.selector,target))throw new Error('crossed_native_policy_selection');
        }else if(change.operation!=='unblock'||change.rule_id!==chosen.rule_id)throw new Error('crossed_native_policy_selection');
    }
    else if(chosen.operation==='retire_index'){if(operation.operation!=='retire_index')throw new Error('crossed_native_source_selection');}
    else {if(operation.operation!=='begin_index_source_change')throw new Error('crossed_native_source_selection');const change=catalogObject(catalogObject(catalogObject(operation.plan).configuration).configuration);if(change.operation!==chosen.operation||change.alias!==chosen.alias)throw new Error('crossed_native_source_selection');}
    return {request,response,previewDigest:intent.catalog_apply.preview_digest,requestDigest:intent.request_digest};
}
export function validateSourceResolveIntent(input:unknown):SourceResolveIntent{
    const value=catalogObject(input);exact(value,'catalog_source_change_resolve,namespace');catalogHex(value.namespace);const request=catalogObject(value.catalog_source_change_resolve);exact(request,'begin_operation');catalogHex(request.begin_operation);return input as SourceResolveIntent;
}
export function validatePolicyResumeIntent(input:unknown):PolicyResumeIntent{
    const value=catalogObject(input);exact(value,'catalog_policy_resume,project');catalogHex(value.project,64);
    const request=catalogObject(value.catalog_policy_resume);exact(request,'index_id,operation_id,caller,key');catalogId(request.index_id,'idx_');catalogHex(request.operation_id);catalogHex(request.caller);
    const key=catalogObject(request.key);exact(key,'namespace,epoch,nonce');catalogHex(key.namespace);catalogHex(key.nonce);
    if(typeof key.epoch!=='number'||!Number.isSafeInteger(key.epoch)||key.epoch<0)throw new Error('invalid_policy_resume_epoch');
    return input as PolicyResumeIntent;
}
export function sourceChangeObservation(response:CatalogObject):CatalogObject|undefined{
    if(!['catalog_source_change_observation','catalog_source_change_resolved','source_change_resolution_outcome_unknown','catalog_pending_source_change'].includes(String(response.status)))return undefined;
    return response.observation==null?undefined:catalogObject(response.observation);
}

/** Native source-policy wire checks. These values never establish source authority. */

const POLICY_BYTES = 1024 * 1024;
function policyExact(value: CatalogObject, fields: string): void {
    if (Object.keys(value).sort().join(',') !== fields.split(',').sort().join(',')) throw new Error('invalid_source_policy_fields');
}
function denyId(value: unknown): string {
    if (typeof value !== 'string' || !/^deny_[0-9a-f]{32}$/.test(value)) throw new Error('invalid_source_policy_rule');
    return value;
}
function rootPath(value: unknown): string {
    if (typeof value !== 'string' || !value || new TextEncoder().encode(value).length > 32768 || value.includes('\0') || !/^(?:\/|[A-Za-z]:[\\/]|\\\\)/.test(value)) throw new Error('invalid_source_policy_root');
    return value;
}
/** Matches the existing Core format; unknown transport envelope fields stay with their caller. */
export function validateSourcePolicy(input: unknown): CatalogObject {
    const policy = catalogObject(input); catalogBound(policy, POLICY_BYTES);
    policyExact(policy, 'schema,index_id,owner_binding,namespace,collection,epoch,roots,positive_includes,denies');
    if (policy.schema !== 'airk.source-policy/1' || policy.namespace !== 'collections-1.4') throw new Error('unsupported_source_policy');
    catalogId(policy.index_id, 'idx_'); catalogHex(policy.owner_binding, 64); catalogRevision(policy.epoch);
    if (typeof policy.collection !== 'string' || !/^[a-zA-Z0-9._-]{1,128}$/.test(policy.collection) || ['.', '..'].includes(policy.collection)) throw new Error('invalid_source_policy_collection');
    if (!Array.isArray(policy.roots) || !policy.roots.length || policy.roots.length > 17) throw new Error('invalid_source_policy_roots');
    const roots = new Set<string>(), paths = new Set<string>(), aliases = new Set<string>();
    for (const value of policy.roots) {
        const root = catalogObject(value); policyExact(root, 'id,canonical_path,aliases,owner');
        const id = catalogId(root.id, 'src_'), path = rootPath(root.canonical_path);
        if (roots.has(id) || paths.has(path) || typeof root.owner !== 'boolean' || !Array.isArray(root.aliases) || root.aliases.length > 16) throw new Error('invalid_source_policy_roots');
        roots.add(id); paths.add(path);
        for (const alias of root.aliases) {
            if (typeof alias !== 'string' || !alias || new TextEncoder().encode(alias).length > 128 || /[\u0000-\u001f\u007f-\u009f]/.test(alias) || aliases.has(alias)) throw new Error('invalid_source_policy_alias');
            aliases.add(alias);
        }
    }
    if (policy.positive_includes !== null) {
        const includes = validateCatalogScope(policy.positive_includes);
        if (includes.some(atom => !roots.has(atom.source_root_id))) throw new Error('unknown_source_policy_root');
    }
    if (!Array.isArray(policy.denies) || policy.denies.length > 128) throw new Error('invalid_source_policy_denies');
    const ids = new Set<string>(), predicates = new Set<string>();
    for (const value of policy.denies) {
        const deny = catalogObject(value); policyExact(deny, `source_root_id,kind,path,rule_id,reason,created_epoch${deny.inherited_from === undefined ? '' : ',inherited_from'}`);
        const atom = validateCatalogScope([{ source_root_id: deny.source_root_id, kind: deny.kind, path: deny.path }])[0];
        const id = denyId(deny.rule_id), predicate = JSON.stringify([atom.source_root_id, atom.kind, atom.path]);
        if (!roots.has(atom.source_root_id) || ids.has(id) || predicates.has(predicate) || !['trim', 'split_transfer'].includes(String(deny.reason)) || catalogRevision(deny.created_epoch) > Number(policy.epoch)) throw new Error('invalid_source_policy_deny');
        ids.add(id); predicates.add(predicate);
        if (deny.inherited_from !== undefined) {
            const inherited = catalogObject(deny.inherited_from); policyExact(inherited, 'index_id,rule_id,epoch');
            if (catalogId(inherited.index_id, 'idx_') === policy.index_id) throw new Error('self_inherited_source_policy');
            denyId(inherited.rule_id); catalogRevision(inherited.epoch);
        }
    }
    return policy;
}

export function validatePolicyChange(input: unknown): CatalogObject {
    const change = catalogObject(input);
    if (change.operation === 'trim') {
        policyExact(change, 'operation,rule_id,selector'); denyId(change.rule_id); validateCatalogScope([change.selector]);
    } else if (change.operation === 'unblock') {
        policyExact(change, 'operation,rule_id'); denyId(change.rule_id);
    } else throw new Error('invalid_source_policy_change');
    return change;
}

/** Internal native job carried by an immutable catalog draft, never a generic UI configuration intent. */
export function validateNativePolicyConfiguration(input: unknown): CatalogObject {
    const request = catalogObject(input); policyExact(request, `incarnation,settings,configuration${request.prepare === undefined ? '' : ',prepare'}`);
    catalogBound(request, 16384); catalogHex(request.incarnation); catalogHex(request.settings, 64);
    if (request.prepare !== undefined && request.prepare !== false) throw new Error('native_concrete_policy_configuration_required');
    const configuration = catalogObject(request.configuration); policyExact(configuration, 'operation,policy,operation_id');
    if (configuration.operation !== 'apply_source_policy') throw new Error('native_source_policy_configuration_required');
    catalogHex(configuration.operation_id); return validateSourcePolicy(configuration.policy);
}

/** Bounded native administrative observations; null counts remain unknown. */
export function validatePolicyImpact(input: unknown): CatalogObject {
    const impact = catalogObject(input); catalogBound(impact, 49152);
    policyExact(impact, 'target,canonical_root,affected_artifacts,affected_vectors,affected_facts,affected_relationships,counts_basis,affected_source_root_ids,affected_project_ids,other_covering_index_ids,overlap_basis');
    const target=validateCatalogScope([impact.target])[0];rootPath(impact.canonical_root);
    for (const name of ['affected_artifacts', 'affected_vectors', 'affected_facts', 'affected_relationships']) {
        const value = impact[name];
        if (value !== null && (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0)) throw new Error('invalid_source_policy_impact_count');
    }
    for (const name of ['counts_basis', 'overlap_basis']) {
        if (typeof impact[name] !== 'string' || !impact[name] || new TextEncoder().encode(String(impact[name])).length > 4096) throw new Error('invalid_source_policy_impact_basis');
    }
    for (const [name, prefix, maximum] of [['affected_source_root_ids', 'src_', 17], ['affected_project_ids', 'prj_', 32], ['other_covering_index_ids', 'idx_', 32]] as const) {
        const values = impact[name];
        if (!Array.isArray(values) || values.length > maximum || new Set(values).size !== values.length) throw new Error('invalid_source_policy_impact_selection');
        values.forEach(value => catalogId(value, prefix));
    }
    if(!(impact.affected_source_root_ids as string[]).includes(target.source_root_id))throw new Error('crossed_source_policy_impact_root');
    return impact;
}

/** Null means no retained policy observation, never proof of completed cleanup. */
export function validatePolicyStatus(input: unknown, index: string, status:'catalog_policy_status'|'catalog_policy_resumed'='catalog_policy_status'): CatalogObject {
    const response = catalogObject(input); catalogBound(response, 65536); catalogId(index, 'idx_');
    if (response.status !== status || response.index_id !== index) throw new Error('crossed_source_policy_status');
    if(status==='catalog_policy_resumed'){catalogHex(response.accepted_job_id,64);if(response.status_basis!=='current_policy')throw new Error('invalid_source_policy_status_basis');}
    if (response.snapshot === null) {
        if (response.job !== null) throw new Error('crossed_source_policy_job');
        return response;
    }
    const snapshot = catalogObject(response.snapshot); catalogBound(snapshot, 49152);
    if (snapshot.schema !== 1) throw new Error('unsupported_source_policy_snapshot');
    catalogHex(snapshot.operation_id); catalogHex(snapshot.job, 64);
    const policy = validateSourcePolicy(snapshot.policy);
    if (policy.index_id !== index) throw new Error('crossed_source_policy_index');
    const job = catalogObject(response.job); catalogHex(job.id, 64); catalogHex(job.project, 64);
    if (job.id !== snapshot.job || job.project !== policy.owner_binding || job.kind !== 'configure' || !['pending', 'running', 'succeeded', 'failed', 'canceled'].includes(String(job.state))) throw new Error('crossed_source_policy_job');
    for (const name of ['fencing_generation', 'attempts']) {
        if (typeof job[name] !== 'number' || !Number.isSafeInteger(job[name]) || Number(job[name]) < 0) throw new Error('invalid_source_policy_job');
    }
    if (snapshot.last_error !== null && (typeof snapshot.last_error !== 'string' || new TextEncoder().encode(snapshot.last_error).length > 1024)) throw new Error('invalid_source_policy_error');
    if(snapshot.application===null){if(snapshot.application_job!==null)throw new Error('crossed_source_policy_application_job');}
    else {
        catalogHex(snapshot.application_job,64);
        const application = catalogObject(snapshot.application); catalogHex(application.policy_digest, 64); catalogHex(application.operation_id);
        if (typeof application.observed_unix_ms !== 'number' || !Number.isSafeInteger(application.observed_unix_ms) || application.observed_unix_ms < 1) throw new Error('invalid_source_policy_observation_time');
        if (application.operation_id !== snapshot.operation_id || typeof application.no_index !== 'boolean' || typeof application.physical_cleanup_pending !== 'boolean' || (application.removal_publication !== null) === application.no_index) throw new Error('invalid_source_policy_application');
        for (const name of ['removed_artifacts', 'reclaimed_files']) {
            if (typeof application[name] !== 'number' || !Number.isSafeInteger(application[name]) || Number(application[name]) < 0) throw new Error('invalid_source_policy_accounting');
        }
        if (application.reclaimed_bytes !== null && (typeof application.reclaimed_bytes !== 'number' || !Number.isSafeInteger(application.reclaimed_bytes) || application.reclaimed_bytes < 0)) throw new Error('invalid_source_policy_accounting');
        if (application.cleanup_error !== undefined && (typeof application.cleanup_error !== 'string' || new TextEncoder().encode(application.cleanup_error).length > 1024 || application.physical_cleanup_pending !== true)) throw new Error('invalid_source_policy_cleanup_error');
        if (!Array.isArray(application.retired_components) || application.retired_components.length > 2 || new Set(application.retired_components).size !== application.retired_components.length || application.retired_components.some(value => !['analysis', 'embedding_cache'].includes(String(value)))) throw new Error('invalid_source_policy_retired_components');
        if (application.no_index && (application.removed_artifacts !== 0 || application.retired_components.length !== 0)) throw new Error('invalid_empty_source_policy_application');
        if (application.removal_publication !== null) {
            const token=application.removal_publication;
            const fields=typeof token==='string'&&token.length<=126?/^airk-r1:([0-9a-f]{32}):(0|[1-9][0-9]{0,19}):([0-9a-f]{64})$/.exec(token):null;
            if(!fields||fields[0]!==token||BigInt(fields[2])>18446744073709551615n)throw new Error('invalid_source_policy_publication');
        }
    }
    return response;
}

/** Labels only recorded native states. Missing observations stay unknown. */
export function canReviewPolicyUnblock(response:CatalogObject|undefined):boolean {
    if(!response?.snapshot)return false;
    const state=catalogObject(response.job).state;
    if(state==='pending'||state==='running')return false;
    return catalogObject(response.snapshot).application!==null||state==='failed'||state==='canceled';
}

export function policyStatusLabels(response: CatalogObject): string[] {
    if (response.snapshot === null) return ['No permanent-policy observation returned. Removal and cleanup are unknown.'];
    const snapshot = catalogObject(response.snapshot), policy = catalogObject(snapshot.policy);
    const exclusions=Array.isArray(policy.denies)&&policy.denies.length>0||policy.positive_includes!==null;
    const labels = [exclusions ? 'Permanent exclusions active.' : 'No permanent exclusions in this policy.'];
    if(response.resume_receipt&&catalogObject(response.resume_receipt).operation_id!==snapshot.operation_id)labels.push('The accepted resume belongs to an earlier policy operation. Its exact receipt is retained separately; the displayed policy and cleanup state describe the current operation.');
    if(response.accepted_job_id!==undefined&&response.accepted_job_id!==snapshot.job)labels.push('The accepted resume job differs from the currently observed cleanup job; its exact native receipt is retained separately.');
    if (snapshot.application === null) {
        const state=catalogObject(response.job).state;
        labels.push(state==='failed'||state==='canceled'?'Removal has no recorded publication; the native operation failed or was canceled.':'Removal from the current index pending.');
        if(exclusions&&(state==='failed'||state==='canceled'))labels.push('Recorded exclusions remain active. An existing trim rule can be selected for a separate native unblock review.');
    }
    else {
        const application = catalogObject(snapshot.application);
        const stamp = new Date(Number(application.observed_unix_ms));
        labels.push(`Removal and cleanup observation: ${Number.isNaN(stamp.getTime()) ? `${String(application.observed_unix_ms)} milliseconds since the Unix epoch` : stamp.toISOString()}.`);
        if(snapshot.application_job!==snapshot.job)labels.push('The removal receipt is from an earlier cleanup job.');
        labels.push(application.no_index ? 'No retained index required removal.' : 'Removed from the current index at the recorded publication.');
        labels.push(application.physical_cleanup_pending ? 'Physical cleanup pending.' : 'Physical cleanup complete for this native operation.');
        if (Array.isArray(application.retired_components) && application.retired_components.includes('analysis')) labels.push('Imported analysis was retired for the entire index; it has not been regenerated.');
        if (Array.isArray(application.retired_components) && application.retired_components.includes('embedding_cache')) labels.push('The embedding cache was retired for the entire index.');
        if (application.cleanup_error !== undefined) labels.push(`Physical cleanup needs attention: ${String(application.cleanup_error)}`);
    }
    if (snapshot.last_error !== null) labels.push(exclusions?'Native operation needs attention; exclusions remain active.':'Native operation needs attention; no permanent exclusions are active.');
    return labels;
}
