// SPDX-License-Identifier: Apache-2.0
/** Explicit window selection. Native metadata and saved preferences are never
 * credentials; ConnectScoped revalidates the reviewed grant on admission. */
import { createHash } from 'node:crypto';
import * as path from 'node:path';
import { catalogBound, catalogHex, catalogId, catalogObject, catalogRevision, validateCatalogScope, validateNativeHostContext } from './indexingCatalog';
import { validateCatalogSession, validateCombinedCatalogSession, type CatalogSessionSelector, type CombinedCatalogSessionSelector } from './catalogSession';
export { validateCatalogSession, validateCombinedCatalogSession, type CatalogSessionSelector, type CombinedCatalogSessionSelector } from './catalogSession';
export interface ReviewedScopeChoice {
    grant_id: string; expected_grant_revision: number; index_id: string; project_ids: string[];
}
export interface NativeScopeIndex {
    id: string; name: string;
    backing: { binding: string; root: string; data_root: string; collection: string };
}
export interface ReviewedScopeSelection {
    session: CatalogSessionSelector; index: NativeScopeIndex; host: string;
    semanticIdentity: string; label: string;
}
export interface ReviewedCombinedScopeChoice {
    grant_id:string;expected_grant_revision:number;project_ids:string[];index_ids?:string[];
}
export interface ReviewedCombinedScopeSelection {
    session:CombinedCatalogSessionSelector;indexes:NativeScopeIndex[];host:string;semanticIdentity:string;label:string;
}
export type ReviewedWindowScopeSelection=ReviewedScopeSelection|ReviewedCombinedScopeSelection;
export function combinedScopeSelectionIdentity(input:CombinedCatalogSessionSelector,nativeFingerprints?:ReadonlyArray<readonly [string,string]>):string {
    const session=validateCombinedCatalogSession(input);
    if(nativeFingerprints&&nativeFingerprints.length>32)throw new Error('invalid_combined_scope_fingerprints');
    const fingerprints=nativeFingerprints?.map(([id,fingerprint])=>[catalogId(id,'idx_'),catalogHex(fingerprint,64)]).sort(([a],[b])=>a.localeCompare(b));
    if(fingerprints&&(fingerprints.length>32||new Set(fingerprints.map(([id])=>id)).size!==fingerprints.length))throw new Error('invalid_combined_scope_fingerprints');
    return createHash('sha256').update(JSON.stringify(['combined',session.namespace,session.context_key,session.grant_id,
        session.expected_grant_revision,session.project_ids,session.index_ids??null,fingerprints??null])).digest('hex');
}
function exact(value: Record<string,unknown>, fields: string): void {
    if(Object.keys(value).sort().join(',')!==fields.split(',').sort().join(','))throw new Error('invalid_catalog_session_fields');
}
/** Catalog clock changes alone do not rename captured evidence or pool identity.
 * A native scope fingerprint, when observed, additionally binds runtime scope. */
export function scopeSelectionIdentity(input: CatalogSessionSelector, nativeFingerprint?: string): string {
    const session=validateCatalogSession(input);
    if(nativeFingerprint!==undefined)catalogHex(nativeFingerprint,64);
    return createHash('sha256').update(JSON.stringify([session.namespace,session.context_key,session.grant_id,
        session.expected_grant_revision,session.index_id,session.project_ids,nativeFingerprint??null])).digest('hex');
}
/** Bind a retained runtime observation only after matching its selected IDs.
 * This does not turn a captured observation into current authorization. */
export function observedScopeSelectionIdentity(input:CatalogSessionSelector,observationInput:unknown):string {
    const session=validateCatalogSession(input),observation=catalogObject(observationInput),grant=catalogObject(observation.grant);
    if(observation.schema!==1||observation.qualifier!=='selected_subset'||observation.provenance!=='user_reviewed_project_grant'
        ||grant.id!==session.grant_id||grant.revision!==session.expected_grant_revision
        ||!Array.isArray(observation.members)||observation.members.length!==1)throw new Error('crossed_scope_observation');
    const member=catalogObject(observation.members[0]);
    if(member.index_id!==session.index_id||!Array.isArray(member.memberships))throw new Error('crossed_scope_observation');
    const selected=validateCatalogSession({...session,project_ids:member.memberships.map(raw=>catalogObject(raw).project_id)}).project_ids;
    if(JSON.stringify(selected)!==JSON.stringify(session.project_ids))throw new Error('crossed_scope_observation');
    return scopeSelectionIdentity(session,catalogHex(observation.fingerprint,64));
}
function nativePath(input: unknown): string {
    if(typeof input!=='string'||!path.isAbsolute(input)||Buffer.byteLength(input)>4096||/[\u0000-\u001f\u007f]/.test(input))throw new Error('invalid_native_scope_path');
    return input;
}
function detail(input: unknown, kind: string, namespace: string, revision: number): Record<string,unknown> {
    const response=catalogObject(input);catalogBound(response);const item=catalogObject(response.item);
    if(response.namespace!==namespace||response.revision!==revision||item.kind!==kind)throw new Error('crossed_scope_selection_metadata');
    return catalogObject(item.value);
}
/** Native metadata review for a single explicit combined choice. Shared indexes
 * are described once; their selected memberships remain separately attributed. */
export function reviewCombinedScopeSelection(choiceInput:unknown,contextInput:unknown,grantDetail:unknown,indexDetails:unknown[]):ReviewedCombinedScopeSelection {
    const choice=catalogObject(choiceInput),context=validateNativeHostContext(contextInput);
    const keys=Object.keys(choice).sort().join(',');
    if(keys!=='expected_grant_revision,grant_id,project_ids'&&keys!=='expected_grant_revision,grant_id,index_ids,project_ids')throw new Error('invalid_combined_scope_choice_fields');
    const session=validateCombinedCatalogSession({schema:1,context_key:context.key,namespace:context.namespace,expected_catalog_revision:context.catalog_revision,...choice});
    const persisted=detail(grantDetail,'grant',context.namespace,context.catalog_revision),grant=catalogObject(persisted.grant);
    if(!Array.isArray(grant.memberships)||grant.memberships.length>1024)throw new Error('reviewed_scope_grant_unavailable');
    const memberships=grant.memberships.map(raw=>catalogObject(catalogObject(raw).membership));
    if(session.project_ids.some(project=>!memberships.some(member=>member.project_id===project)))throw new Error('scope_selection_outside_reviewed_grant');
    const union=[...new Set(memberships.filter(member=>session.project_ids.includes(String(member.project_id))).map(member=>catalogId(member.index_id,'idx_')))].sort();
    const selected=session.index_ids??union;
    if(!selected.length||selected.length>32||selected.some(index=>!union.includes(index)))throw new Error('scope_selection_outside_reviewed_grant');
    if(indexDetails.length!==selected.length)throw new Error('incomplete_scope_selection_metadata');
    const details=new Map<string,unknown>();
    for(const raw of indexDetails){const row=detail(raw,'index',context.namespace,context.catalog_revision),id=catalogId(row.id,'idx_');if(details.has(id)||!selected.includes(id))throw new Error('crossed_scope_selection_metadata');details.set(id,raw);}
    const indexes=selected.map(index_id=>{
        const project_ids=session.project_ids.filter(project=>memberships.some(member=>member.project_id===project&&member.index_id===index_id));
        return reviewScopeSelection({grant_id:session.grant_id,expected_grant_revision:session.expected_grant_revision,index_id,project_ids},context,grantDetail,details.get(index_id)).index;
    });
    return {session,indexes,host:context.host,semanticIdentity:combinedScopeSelectionIdentity(session),
        label:`${session.project_ids.length} selected index set${session.project_ids.length===1?'':'s'} · ${indexes.length} index${indexes.length===1?'':'es'}`};
}
/** Called only for the user's explicit Use Grant action. Defaults and context
 * preferences never select a grant, project subset or index here. */
export function reviewScopeSelection(input: ReviewedScopeChoice, contextInput: unknown, grantDetail: unknown, indexDetail: unknown): ReviewedScopeSelection {
    const choice=catalogObject(input);exact(choice,'grant_id,expected_grant_revision,index_id,project_ids');
    const context=validateNativeHostContext(contextInput);
    const session=validateCatalogSession({schema:1,context_key:context.key,namespace:context.namespace,
        expected_catalog_revision:context.catalog_revision,...choice});
    const persisted=detail(grantDetail,'grant',context.namespace,context.catalog_revision);
    const grant=catalogObject(persisted.grant),provenance=catalogObject(persisted.provenance);
    exact(provenance,'kind');
    if(provenance.kind!=='user_reviewed_project_grant'||grant.id!==session.grant_id||grant.host!==context.host
        ||grant.revision!==session.expected_grant_revision||grant.revoked!==false
        ||session.expected_grant_revision>context.catalog_revision
        ||parseInt(session.grant_id.slice(4,6),16)>=128||!Array.isArray(grant.memberships)||grant.memberships.length>1024)throw new Error('reviewed_scope_grant_unavailable');
    const pairs=new Set<string>(),selected=new Set<string>();
    for(const raw of grant.memberships){
        const frozen=catalogObject(raw),member=catalogObject(frozen.membership);
        const project=catalogId(member.project_id,'prj_'),index=catalogId(member.index_id,'idx_');
        catalogRevision(member.revision);validateCatalogScope(member.scope);catalogRevision(frozen.source_revision);
        if(typeof frozen.policy_epoch!=='number'||!Number.isSafeInteger(frozen.policy_epoch)||frozen.policy_epoch<0
            ||(frozen.policy_epoch===0)!==(frozen.policy_digest===null))throw new Error('invalid_scope_grant_policy');
        if(frozen.policy_digest!==null)catalogHex(frozen.policy_digest,64);
        const pair=JSON.stringify([project,index]);if(pairs.has(pair))throw new Error('duplicate_scope_grant_membership');pairs.add(pair);
        if(index===session.index_id&&session.project_ids.includes(project))selected.add(project);
    }
    if(session.project_ids.some(id=>!selected.has(id)))throw new Error('scope_selection_outside_reviewed_grant');
    const row=detail(indexDetail,'index',context.namespace,context.catalog_revision),backing=catalogObject(row.backing);
    if(row.id!==session.index_id||row.tombstone!==false||row.source_import!=='complete'
        ||typeof row.name!=='string'||!row.name.trim()||Buffer.byteLength(row.name)>256)throw new Error('scope_index_unavailable');
    catalogRevision(row.definition_revision);catalogRevision(row.source_revision);
    const policy=row.policy===null?null:catalogObject(row.policy);
    if(policy){catalogRevision(policy.epoch);catalogHex(policy.digest,64);}
    for(const raw of grant.memberships){
        const frozen=catalogObject(raw),member=catalogObject(frozen.membership);
        if(member.index_id===session.index_id&&session.project_ids.includes(String(member.project_id))
            &&(frozen.source_revision!==row.source_revision||frozen.policy_epoch!==(policy?.epoch??0)
                ||frozen.policy_digest!==(policy?.digest??null)))throw new Error('scope_grant_source_policy_changed');
    }
    if(typeof backing.collection!=='string'||!/^[A-Za-z0-9._-]{1,128}$/.test(backing.collection)||['.','..'].includes(backing.collection))throw new Error('invalid_scope_collection');
    const index:NativeScopeIndex={id:session.index_id,name:row.name,backing:{binding:catalogHex(backing.binding,64),
        root:nativePath(backing.root),data_root:nativePath(backing.data_root),collection:backing.collection}};
    return {session,index,host:context.host,semanticIdentity:scopeSelectionIdentity(session),
        label:`${index.name} · ${session.project_ids.length} selected index set${session.project_ids.length===1?'':'s'}`};
}

/** Inert preference only. Restoring it never calls use(), connects or creates a
 * grant. The host must perform fresh native review after explicit user action. */
export interface ScopeSelectionPreference extends ReviewedScopeChoice { schema:1; namespace:string; host:string; context_key:string }
export function scopeSelectionPreference(selection: ReviewedScopeSelection): ScopeSelectionPreference;
export function scopeSelectionPreference(selection: ReviewedCombinedScopeSelection): Omit<CombinedCatalogSessionSelector,'expected_catalog_revision'> & {host:string};
export function scopeSelectionPreference(selection: ReviewedWindowScopeSelection): ScopeSelectionPreference|Omit<CombinedCatalogSessionSelector,'expected_catalog_revision'> & {host:string};
export function scopeSelectionPreference(selection: ReviewedWindowScopeSelection): ScopeSelectionPreference|Omit<CombinedCatalogSessionSelector,'expected_catalog_revision'> & {host:string} {
    if('indexes' in selection){
        const {expected_catalog_revision:_clock,...session}=validateCombinedCatalogSession(selection.session);catalogHex(selection.host,64);
        return {...session,host:selection.host};
    }
    const session=validateCatalogSession(selection.session);catalogHex(selection.host,64);
    return {schema:1,namespace:session.namespace,host:selection.host,context_key:session.context_key,
        grant_id:session.grant_id,expected_grant_revision:session.expected_grant_revision,index_id:session.index_id,project_ids:[...session.project_ids]};
}
export function scopePreferenceKey(namespace:string,host:string,contextKey:string):string {
    catalogHex(namespace);catalogHex(host,64);catalogHex(contextKey,64);
    return 'airk.scope.preference.1.'+createHash('sha256').update(JSON.stringify([namespace,host,contextKey])).digest('hex');
}
export class WindowScopeSelection {
    private value?:ReviewedWindowScopeSelection;
    private epoch=0;
    get generation():number{return this.epoch;}
    get current():ReviewedWindowScopeSelection|undefined{return this.value?structuredClone(this.value):undefined;}
    /** Explicit user action only; caller invalidates active transports/turns
     * before publishing this selection. Captured user evidence is retained. */
    use(selection:ReviewedWindowScopeSelection):void {this.value=structuredClone(selection);this.epoch++;}
    clear():void {this.value=undefined;this.epoch++;}
}
