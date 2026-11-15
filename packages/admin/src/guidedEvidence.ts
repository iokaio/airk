// SPDX-License-Identifier: Apache-2.0
// Guided UI inputs, not a second native schema or authority catalog.
export type GuidedEvidenceRoute='code/symbols'|'code/references'|'code/outline'|'docs/facts'|'evidence/inspector'|'evidence/checker'|'evidence/changes';
export type GuidedFields={name:string;path:string;prefix:string;kind:string;since:string;exact:boolean;limit:number;text:string};
export const GUIDED_EVIDENCE_ROUTES:readonly GuidedEvidenceRoute[]=['code/symbols','code/references','code/outline','docs/facts','evidence/inspector','evidence/checker','evidence/changes'];
export function isGuidedEvidenceRoute(value:unknown):value is GuidedEvidenceRoute{return typeof value==='string'&&(GUIDED_EVIDENCE_ROUTES as readonly string[]).includes(value);}
export function guidedEvidenceRequest(route:GuidedEvidenceRoute,fields:GuidedFields,advanced:Record<string,unknown>={}):{tool:string;args:Record<string,unknown>} {
 const prefix=fields.prefix,path=fields.path,name=fields.name;
 const bounded={max_bytes:32768,...advanced,limit:fields.limit,...(prefix?{path_prefix:prefix}:{})};
 switch(route){
 case 'code/symbols':return {tool:fields.exact&&(name||advanced.symbol_key!==undefined||advanced.kind!==undefined)?'code_facts':'symbols',args:{...advanced,...bounded,...(name?{name}:{})}};
 case 'code/references':if(!name.trim())throw Error('Enter an exact-case identifier.');return {tool:'references',args:{...advanced,...bounded,name}};
 case 'code/outline':if(!path.trim())throw Error('Choose an indexed relative file path.');return {tool:'outline',args:{max_bytes:32768,...advanced,path,limit:fields.limit}};
 case 'docs/facts':return {tool:'document_facts',args:{...advanced,...bounded,...(fields.kind!=='any'?{kind:fields.kind}:{}),...(name?{name}:{})}};
 case 'evidence/inspector':if(!path.trim())throw Error('Choose an indexed relative file path.');return {tool:'file_info',args:{...advanced,path}};
 case 'evidence/changes':if(!fields.since.trim())throw Error('Enter the exact earlier native publication token.');return {tool:'changes',args:{...advanced,...bounded,since:fields.since}};
 case 'evidence/checker':throw Error('Citation checking uses the existing parser and one bounded file_info read per distinct relative path.');
 }
}

export function guidedIndexedFiles(prefix:string):{tool:'list_files';args:Record<string,unknown>}{return {tool:'list_files',args:{prefix,limit:50,max_bytes:32768}};}
