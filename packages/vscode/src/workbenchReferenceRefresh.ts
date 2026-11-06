// SPDX-License-Identifier: Apache-2.0
/** Host-only preparation from immutable native turns. Webviews supply opaque handles only. */
import {isDeepStrictEqual} from 'node:util';
import {catalogBound} from './indexingCatalog';
import {validateCombinedCatalogSession} from './catalogSession';
import {evidenceCards} from './workbenchRender';
import {object,type Value,type WorkbenchTurn} from './workbenchContracts';
import type {ReferenceRefreshFlow,ReferenceRefreshIntent,RefreshFlowHost} from './referenceRefresh';
export interface PreparedWorkbenchRefresh {flow:ReferenceRefreshFlow;intent:ReferenceRefreshIntent;capabilities:Value}
export interface ReferenceRefreshControl {
    referenceRefreshPrepare(selection:Value,members:Value[],mode:'selected_only'|'full_reconcile_reviewed',query:Value,result:Value,host:RefreshFlowHost,valid:()=>boolean):Promise<PreparedWorkbenchRefresh>;
    referenceRefreshFlow(host:RefreshFlowHost,valid:()=>boolean):ReferenceRefreshFlow;
    referenceRefreshReconfirm(selection:Value,valid:()=>boolean):Promise<Value>;
    referenceRefreshPending(selection:Value):Array<{id:string;latest?:Value;historical?:Value}>;
}
export function sameRefreshAuthority(left:Value,right:Value):boolean {
    const before={...left},after={...right};delete before.expected_catalog_revision;delete after.expected_catalog_revision;
    return isDeepStrictEqual(before,after);
}
/** Receipt visibility may narrow a current frozen selection, never change its grant. */
export function refreshWithinAuthority(saved:Value,current:Value):boolean {
    try{const before=validateCombinedCatalogSession(saved),after=validateCombinedCatalogSession(current),left={...before},right={...after};
        delete (left as Partial<typeof left>).project_ids;delete left.index_ids;delete (right as Partial<typeof right>).project_ids;delete right.index_ids;
        delete (left as Partial<typeof left>).expected_catalog_revision;delete (right as Partial<typeof right>).expected_catalog_revision;
        return isDeepStrictEqual(left,right)&&before.project_ids.every(id=>after.project_ids.includes(id))&&!!before.index_ids&&(!after.index_ids||before.index_ids.every(id=>after.index_ids!.includes(id)));
    }catch{return false;}
}
/** No query reconstruction from text, reduced cards or the current form. */
export function captureWorkbenchRefresh(turn:WorkbenchTurn,envelope:Value,handles:string[],selection:Value):{query:Value;result:Value;cards:ReturnType<typeof evidenceCards>;selection:Value} {
    if(!turn.envelope||envelope.executed!==true||!['complete','partial'].includes(String(envelope.status))||!handles.length||handles.length>64||new Set(handles).size!==handles.length)throw new Error('refresh_live_evidence_required');
    const cards=evidenceCards(envelope),selected=handles.map(id=>{const card=cards.find(value=>value.id===id);if(!card)throw new Error('refresh_evidence_handle_expired');return card;});
    const step=selected[0].step,index=selected[0].indexId,publication=selected[0].publication;
    if(!step||!index||!publication||selected.some(card=>card.step!==step||card.indexId!==index||card.publication!==publication||!card.sourceRootId||!card.sourceRelativePath))throw new Error('refresh_select_one_qualified_step_and_index');
    const reports=Array.isArray(envelope.steps)?envelope.steps.map(object):[],report=reports.find(value=>value.id===step);
    if(!report||report.status!=='complete'||!report.result||!report.arguments)throw new Error('refresh_original_canonical_query_unavailable');
    const args=structuredClone(object(report.arguments)),tool=String(report.tool).startsWith('airk_')?String(report.tool):`airk_${String(report.tool)}`;
    const actual=validateCombinedCatalogSession(selection);
    if(actual.index_ids&&!actual.index_ids.includes(index)||selected.some(card=>!card.projectIds?.length||card.projectIds.some(id=>!actual.project_ids.includes(id))))throw new Error('refresh_evidence_outside_frozen_selection');
    // Query-specific selectors remain exactly as resolved by native execution.
    if(args.project_ids!==undefined&&(!Array.isArray(args.project_ids)||args.project_ids.some(id=>!actual.project_ids.includes(String(id)))))throw new Error('refresh_query_scope_differs');
    if(args.index_ids!==undefined&&(!Array.isArray(args.index_ids)||!args.index_ids.includes(index)))throw new Error('refresh_query_index_differs');
    const query={tool,arguments:args},result=structuredClone(object(report.result));catalogBound(query,16384);catalogBound(result,65536);
    const scoped=validateCombinedCatalogSession({...actual,...(args.project_ids!==undefined?{project_ids:args.project_ids}:{}),...(args.index_ids!==undefined?{index_ids:args.index_ids}:{})});
    if(!scoped.index_ids)throw new Error('refresh_explicit_native_index_selection_required');
    if(!scoped.index_ids.includes(index)||selected.some(card=>card.projectIds!.some(id=>!scoped.project_ids.includes(id))))throw new Error('refresh_evidence_outside_original_query_selection');
    return {query,result,cards:selected,selection:structuredClone(scoped) as unknown as Value};
}
