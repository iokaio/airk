// SPDX-License-Identifier: Apache-2.0
/** Pure host/webview contract. Evidence content is always inert text. */
import type { PublicationRelation } from './publications';
export type Value = Record<string, unknown>;
/** Core workbench::MAX_STEP_REPORTS: twelve calls with six bound report slots. */
export const WORKBENCH_MAX_STEP_REPORTS=72;
export const WORKBENCH_HELP_ROUTE='help?focus=engine-workbench';
export interface WorkbenchTurn { id: string; at: number; input: string; format: 'structured' | 'json' | 'markdown'; request?: Value; envelope?: Value; error?: string;
    /** Full original native error reply, never a fabricated successful result. */
    nativeFailure?:Value;trimmed?: boolean }
export interface WorkbenchFormDraft { activeTool:string; values:Record<string,Record<string,string>>; revision:number; scope:string; recipe:string }
export interface WorkbenchSession { schema: 1; id: string; project: string; title: string; createdAt: number; updatedAt: number; revision: number; omitted: number; turns: WorkbenchTurn[]; formDraft?:WorkbenchFormDraft }
const FORM_TOOLS=['search','symbols','references','read_file','outline','list_files','file_info','imports','code_facts','document_facts','changes','context','relationships','status','diagnostics','capabilities','query_scope','help'];
const FORM_RECIPES=['','definition','documents','impact','tests','component','readiness'];
const validFormText=(value:unknown,max=32768):value is string=>typeof value==='string'&&value.length<=max&&!/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(value);
export const defaultWorkbenchFormDraft=():WorkbenchFormDraft=>({activeTool:'search',values:{},revision:0,scope:'',recipe:''});
export function validWorkbenchToolFormValues(raw:unknown):raw is Record<string,string>{
    if(!raw||typeof raw!=='object'||Array.isArray(raw)||![Object.prototype,null].includes(Object.getPrototypeOf(raw)))return false;
    const entries=Object.entries(raw);
    return entries.length<=32&&entries.every(([key,value])=>/^[a-z][a-z0-9_]{0,63}$/.test(key)&&!['constructor','prototype'].includes(key)&&validFormText(value));
}
/** Validate storage/message shape only. Incomplete form values stay inert until Run validates them. */
export function normalizeWorkbenchFormDraft(raw:unknown):WorkbenchFormDraft|undefined{
    if(raw===undefined)return defaultWorkbenchFormDraft();
    if(!raw||typeof raw!=='object'||Array.isArray(raw))return;
    const draft=raw as Record<string,unknown>,keys=Object.keys(draft);
    if(keys.some(key=>!['activeTool','values','revision','scope','recipe'].includes(key))
        ||typeof draft.activeTool!=='string'||!FORM_TOOLS.includes(draft.activeTool)
        ||!Number.isSafeInteger(draft.revision)||Number(draft.revision)<0||Number(draft.revision)>2147483647
        ||!draft.values||typeof draft.values!=='object'||Array.isArray(draft.values)||![Object.prototype,null].includes(Object.getPrototypeOf(draft.values))
        ||Object.keys(draft.values).length>FORM_TOOLS.length)return;
    const scope=draft.scope??'',recipe=draft.recipe??'';
    if(!validFormText(scope,1024)||typeof recipe!=='string'||!FORM_RECIPES.includes(recipe))return;
    const values:Record<string,Record<string,string>>={};
    for(const [tool,fields]of Object.entries(draft.values)){
        if(!FORM_TOOLS.includes(tool)||!validWorkbenchToolFormValues(fields))return;
        values[tool]={...fields};
    }
    const normalized={activeTool:draft.activeTool,values,revision:Number(draft.revision),scope,recipe};
    if(new TextEncoder().encode(JSON.stringify(normalized)).length>131072)return;
    return normalized;
}
export interface EvidenceCard { id: string; path: string; start: number; end: number; hash: string; text: string; location?: unknown; step: string; publication?: string;
    indexId?:string;projectIds?:string[];sourceRootId?:string;sourceRelativePath?:string;resultPointer?:string }
export interface Continuation { id:string; tool:string; callsRemaining:number;requiredRecordBytes?:number }
export interface WorkbenchSessionSummary { id: string; title: string; updatedAt: number; turns: number }
/** Verified by the host for presentation only; native envelopes remain unchanged. */
export type CapabilityFieldDisplay={state:'verified';values:Record<string,Value>}|{state:'unavailable';error:string};
export interface WorkbenchState {
    projects: { id: string; name: string }[]; project?: string; session?: WorkbenchSession;
    recent: WorkbenchSessionSummary[];
    cards: Record<string, EvidenceCard[]>; busy: boolean; notice?: string; save: string;
    sourceOpen?:{id:string;observation:Value};
    continuations?:Record<string,Continuation[]>;
    metadataViews?:Record<string,Value>;metadataIssues?:Record<string,string>;
    capabilityDetails?:Record<string,Record<string,CapabilityFieldDisplay>>;
    status?: Value; publicationRelations?:Record<string,PublicationRelation>; persistence: boolean; operations?: Value;
    sessionEpoch?:number;
}
/** Host-to-webview display updates. Full result ownership stays in the host. */
export type WorkbenchUpdate =
    | {type:'state';state:WorkbenchState}
    | {type:'status';sessionId:string|undefined;project:string|undefined;status:Value|undefined;publicationRelations?:Record<string,PublicationRelation>}
    | {type:'saved';sessionId:string;project:string;revision:number;recent:WorkbenchState['recent'];save:string}
    | {type:'formDraft';sessionId:string;project:string;epoch:number;formDraft:WorkbenchFormDraft;save:string};
/** Shared actual webview consumer; delta fields cannot replace result bodies. */
export function applyWorkbenchUpdate(previous:WorkbenchState,message:WorkbenchUpdate):WorkbenchState {
    if(message.type==='state'){
        const next=message.state;
        if(typeof previous.sessionEpoch==='number'&&typeof next.sessionEpoch==='number'&&next.sessionEpoch<previous.sessionEpoch)return previous;
        if(previous.session?.id===next.session?.id&&previous.session?.project===next.session?.project
            &&previous.sessionEpoch===next.sessionEpoch&&previous.session&&next.session){
            const before=normalizeWorkbenchFormDraft(previous.session.formDraft),incoming=normalizeWorkbenchFormDraft(next.session.formDraft);
            if(before&&incoming&&before.revision>incoming.revision)return {...next,session:{...next.session,formDraft:before}};
        }
        return next;
    }
    if(message.sessionId!==previous.session?.id||message.project!==previous.session?.project)return previous;
    if(message.type==='status')return {...previous,status:message.status,publicationRelations:message.publicationRelations};
    if(!previous.session)return previous;
    if(message.type==='formDraft'){
        const current=normalizeWorkbenchFormDraft(previous.session.formDraft),incoming=normalizeWorkbenchFormDraft(message.formDraft);
        if(message.epoch!==previous.sessionEpoch||!incoming||current&&incoming.revision<current.revision)return previous;
        return {...previous,session:{...previous.session,formDraft:incoming},save:message.save};
    }
    return {...previous,session:{...previous.session,revision:message.revision},recent:message.recent,save:message.save};
}
export interface WorkbenchToolField {
    name:string;label:string;type:'string'|'integer'|'boolean'|'json';required:boolean;
    values?:string[];description?:string;defaultValue?:string|number|boolean;minimum?:number;maximum?:number;minLength?:number;maxLength?:number;
}
const fieldLabels:Record<string,string>={project_ids:'Project IDs (JSON array)',index_ids:'Index subset (JSON array)',member_availability:'Member availability',path_prefix:'Path prefix',exclude_path_prefix:'Exclude path prefix',evidence_kind:'Evidence type',name_match:'Name matching',reply_detail:'Reply detail',verification:'Saved-source verification',verification_max_sources:'Maximum sources to check',verification_max_bytes:'Raw-source byte budget',verification_deadline_ms:'Verification deadline (ms)',compact:'Compact results',include_profile_families:'Include effective profile families',include_count:'Show count precision',include_files:'Summarize files',max_bytes:'Response byte budget'};
/** Resolve only bundled local schema alternatives; no remote schema loading. */
function fieldSchema(raw:unknown,root:Value,depth=0):Value {
    const schema=object(raw);if(depth>4)return {};
    if(typeof schema.$ref==='string'&&schema.$ref.startsWith('#/$defs/')){
        const name=schema.$ref.slice('#/$defs/'.length);return {...fieldSchema(object(root.$defs)[name],root,depth+1),...schema};
    }
    const variants=Array.isArray(schema.anyOf)?schema.anyOf:Array.isArray(schema.oneOf)?schema.oneOf:[];
    const selected=variants.map(value=>fieldSchema(value,root,depth+1)).find(value=>value.type!=='null'&&(value.type!==undefined||Array.isArray(value.enum)));
    return selected?{...schema,...selected}:schema;
}
/** Shared by the actual form and tests so optional booleans stay typed and absent by default. */
export function toolFormFields(raw:unknown):WorkbenchToolField[]{
    const schema=object(raw),required=Array.isArray(schema.required)?schema.required:[];
    return Object.entries(object(schema.properties)).map(([name,value])=>{
        const spec=fieldSchema(value,schema),types=Array.isArray(spec.type)?spec.type:[spec.type];
        const type:WorkbenchToolField['type']=types.includes('boolean')?'boolean':types.includes('integer')?'integer':types.includes('array')||types.includes('object')?'json':'string';
        return {name,...(['string','number','boolean'].includes(typeof spec.default)?{defaultValue:spec.default as string|number|boolean}:{}),label:fieldLabels[name]??name.replaceAll('_',' '),type,required:required.includes(name),
            ...(Array.isArray(spec.enum)?{values:spec.enum.filter(item=>['string','number','boolean'].includes(typeof item)).map(String)}:{}),
            ...(typeof spec.description==='string'?{description:spec.description}:{}),
            ...(typeof spec.minLength==='number'?{minLength:spec.minLength}:{}),...(typeof spec.maxLength==='number'?{maxLength:Math.min(32768,spec.maxLength)}:type==='json'?{maxLength:32768}:{}),
            ...(typeof spec.minimum==='number'?{minimum:spec.minimum}:{}),...(typeof spec.maximum==='number'?{maximum:spec.maximum}:{})};
    });
}
export function toolFormArguments(schema:unknown,values:Record<string,string>):Value {
    const fields=toolFormFields(schema),known=new Set(fields.map(field=>field.name)),entries:Array<[string,unknown]>=[];
    for(const [name,value]of Object.entries(values))if(value!==''&&!known.has(name))throw new Error('Choose a field from the current tool form.');
    for(const field of fields){
        const value=values[field.name]??'';
        if(value===''){if(field.required)throw new Error(`${field.label} is required.`);continue;}
        if(value.length>(field.maxLength??4096)||(field.minLength!==undefined&&value.length<field.minLength))throw new Error(`${field.label} is outside the allowed input length.`);
        if(field.values&&!field.values.includes(value))throw new Error(`Choose a supported ${field.label}.`);
        if(field.type==='boolean'){
            if(value!=='true'&&value!=='false')throw new Error(`Choose Yes, No or Default for ${field.label}.`);
            entries.push([field.name,value==='true']);
        }else if(field.type==='integer'){
            const number=Number(value);
            if(!Number.isSafeInteger(number)||(field.minimum!==undefined&&number<field.minimum)||(field.maximum!==undefined&&number>field.maximum))throw new Error(`Enter an allowed whole number for ${field.label}.`);
            entries.push([field.name,number]);
        }else if(field.type==='json'){
            let parsed:unknown;try{parsed=JSON.parse(value);}catch{throw new Error(`Enter valid JSON for ${field.label}.`);}
            if(field.name==='selectors'&&(!Array.isArray(parsed)||parsed.length>64))throw new Error('Enter at most 64 selectors as a JSON array.');
            entries.push([field.name,parsed]);
        }else entries.push([field.name,value]);
    }
    return Object.fromEntries(entries);
}
export type WorkbenchMessage =
    | { type: 'ready' | 'stop' | 'new' | 'clear' | 'manage' | 'help' | 'refresh' | 'retrySave' | 'fork' | 'forms' }
    | { type: 'project' | 'recent' | 'delete'; id: string }
    | { type: 'rename'; title: string }
    | { type: 'run'; input: string; prepare: boolean; recipe?: string; plan?: Value; pathPrefix?: string }
    | { type: 'rerun' | 'saveStrategy' | 'remember'; turn: string }
    | { type: 'open' | 'context'; turn: string; card: string }
    | { type:'continue'; turn:string; step:string }
    | { type:'followTarget';turn:string;step:string;target:string }
    | { type:'followDefinition';turn:string;step:string;name:string;path:string }
    | { type:'formDraft';sessionId:string;project:string;epoch:number;revision:number;tool:string;values?:Record<string,string>;scope?:string;recipe?:string }
    | { type:'resetForm';sessionId:string;project:string;epoch:number;revision:number;tool:string }
    | {type:'exportSourceOpen';id:string;format:'copy'|'json'}
    | { type: 'copy' | 'export'; turn?: string; format: 'json' | 'markdown'; cards?: string[] };
export const object = (v: unknown): Value => v && typeof v === 'object' && !Array.isArray(v) ? v as Value : {};
export function parseWorkbenchMessage(raw: unknown): WorkbenchMessage | undefined {
    const v = object(raw), type = v.type;
    const str = (k: string, max = 128) => typeof v[k] === 'string' && (v[k] as string).length <= max && !/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(v[k] as string);
    const exact = (keys: string[]) => Object.keys(v).every(k => keys.includes(k));
    if (typeof type !== 'string' || JSON.stringify(raw).length > (type==='formDraft'?132096:40000)) { return; }
    const formFence=()=>str('sessionId')&&str('project',64)&&/^[0-9a-f]{64}$/.test(String(v.project))
        &&Number.isSafeInteger(v.epoch)&&Number(v.epoch)>=0&&Number.isSafeInteger(v.revision)&&Number(v.revision)>0&&Number(v.revision)<=2147483647
        &&typeof v.tool==='string'&&FORM_TOOLS.includes(v.tool);
    if(type==='formDraft'&&exact(['type','sessionId','project','epoch','revision','tool','values','scope','recipe'])&&formFence()
        &&(v.values===undefined||validWorkbenchToolFormValues(v.values))&&(v.scope===undefined||validFormText(v.scope,1024))
        &&(v.recipe===undefined||typeof v.recipe==='string'&&FORM_RECIPES.includes(v.recipe))){return v as WorkbenchMessage;}
    if(type==='resetForm'&&exact(['type','sessionId','project','epoch','revision','tool'])&&formFence()){return v as WorkbenchMessage;}
    if(type==='exportSourceOpen'&&exact(['type','id','format'])&&str('id',32)&&/^[0-9a-f]{32}$/.test(String(v.id))&&['copy','json'].includes(String(v.format)))return v as WorkbenchMessage;
    if (['ready','stop','new','clear','manage','help','refresh','retrySave','fork','forms'].includes(type) && exact(['type'])) { return v as WorkbenchMessage; }
    if (['project','recent','delete'].includes(type) && exact(['type','id']) && str('id')) { return v as WorkbenchMessage; }
    if (type === 'rename' && exact(['type','title']) && str('title',80)) { return v as WorkbenchMessage; }
    if (type === 'run' && exact(['type','input','prepare','recipe','plan','pathPrefix']) && str('input',4096) && typeof v.prepare === 'boolean' && (v.pathPrefix===undefined || str('pathPrefix',1024))
        && (v.recipe === undefined || str('recipe')) && (v.plan === undefined || Object.keys(object(v.plan)).length > 0)) { return v as WorkbenchMessage; }
    if (['rerun','saveStrategy','remember'].includes(type) && exact(['type','turn']) && str('turn')) { return v as WorkbenchMessage; }
    if (['open','context'].includes(type) && exact(['type','turn','card']) && str('turn') && str('card')) { return v as WorkbenchMessage; }
    if(type==='continue'&&exact(['type','turn','step'])&&str('turn')&&str('step')){return v as WorkbenchMessage;}
    if(type==='followTarget'&&exact(['type','turn','step','target'])&&str('turn')&&str('step')&&str('target',4096)){return v as WorkbenchMessage;}
    if(type==='followDefinition'&&exact(['type','turn','step','name','path'])&&str('turn')&&str('step')&&str('name',1024)&&str('path',4096)){return v as WorkbenchMessage;}
    if (['copy','export'].includes(type) && exact(['type','turn','format','cards']) && ['json','markdown'].includes(String(v.format)) && (v.turn === undefined || str('turn'))
        && (v.cards === undefined || Array.isArray(v.cards) && v.cards.length <= 1000 && v.cards.every(id => typeof id === 'string' && id.length <= 128))) { return v as WorkbenchMessage; }
}
