// SPDX-License-Identifier: Apache-2.0
/** Pure Core service::ScopedConnectRequest wire contract. IDs select a native
 * reviewed grant; they are not credentials or a host activation controller. */
import { catalogBound, catalogHex, catalogId, catalogObject, catalogRevision } from './indexingCatalog';

export interface CatalogSessionSelector {
    schema: 1; context_key: string; namespace: string; expected_catalog_revision: number;
    grant_id: string; expected_grant_revision: number; index_id: string; project_ids: string[];
}
/** Frozen Project selection. Only native admission can turn this into authority. */
export interface CombinedCatalogSessionSelector {
    schema: 1; context_key: string; namespace: string; expected_catalog_revision: number;
    grant_id: string; expected_grant_revision: number; project_ids: string[]; index_ids?: string[];
}
export function validateCombinedCatalogSession(input: unknown): CombinedCatalogSessionSelector {
    const value=catalogObject(input),keys=Object.keys(value).sort().join(',');
    if(keys!=='context_key,expected_catalog_revision,expected_grant_revision,grant_id,namespace,project_ids,schema'
        &&keys!=='context_key,expected_catalog_revision,expected_grant_revision,grant_id,index_ids,namespace,project_ids,schema')throw new Error('invalid_combined_catalog_session_fields');
    if(value.schema!==1)throw new Error('invalid_combined_catalog_session_schema');
    const ids=(raw:unknown,prefix:'prj_'|'idx_'):string[]=>{
        if(!Array.isArray(raw)||!raw.length||raw.length>32||new Set(raw).size!==raw.length)throw new Error('invalid_combined_catalog_session_selection');
        return raw.map(id=>catalogId(id,prefix)).sort();
    };
    const result:CombinedCatalogSessionSelector={schema:1,context_key:catalogHex(value.context_key,64),namespace:catalogHex(value.namespace),
        expected_catalog_revision:catalogRevision(value.expected_catalog_revision),grant_id:catalogId(value.grant_id,'grt_'),
        expected_grant_revision:catalogRevision(value.expected_grant_revision),project_ids:ids(value.project_ids,'prj_'),
        ...(value.index_ids!==undefined?{index_ids:ids(value.index_ids,'idx_')}:{})};
    catalogBound(result,4096);return result;
}
export function validateCatalogSession(input: unknown): CatalogSessionSelector {
    const value=catalogObject(input);
    if(Object.keys(value).sort().join(',')!=='context_key,expected_catalog_revision,expected_grant_revision,grant_id,index_id,namespace,project_ids,schema')throw new Error('invalid_catalog_session_fields');
    if(value.schema!==1)throw new Error('invalid_catalog_session_schema');
    if(!Array.isArray(value.project_ids)||!value.project_ids.length||value.project_ids.length>32
        ||new Set(value.project_ids).size!==value.project_ids.length)throw new Error('invalid_catalog_session_projects');
    const result:CatalogSessionSelector={schema:1,context_key:catalogHex(value.context_key,64),namespace:catalogHex(value.namespace),
        expected_catalog_revision:catalogRevision(value.expected_catalog_revision),grant_id:catalogId(value.grant_id,'grt_'),
        expected_grant_revision:catalogRevision(value.expected_grant_revision),index_id:catalogId(value.index_id,'idx_'),
        project_ids:value.project_ids.map(id=>catalogId(id,'prj_')).sort()};
    catalogBound(result,4096);return result;
}
