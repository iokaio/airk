# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import json,hashlib
b=Path(__file__).resolve().parent;w=b/'p10-worktree';out=b/'p12-workbench-proposal';out.mkdir(exist_ok=True);bases={}
def read(p):
 raw=(w/p).read_bytes();bases[p]=hashlib.sha256(raw).hexdigest();return raw.decode().replace('\r\n','\n')
def write(p,s):
 q=out/p;q.parent.mkdir(parents=True,exist_ok=True);q.write_text(s,encoding='utf-8',newline='\n')
def rep(s,a,z):
 assert s.count(a)==1,(a[:100],s.count(a));return s.replace(a,z)
pkg='packages/vscode/src/'
write(pkg+'referenceRefresh.ts',(b/'p12-referenceRefresh.proposed.ts').read_text(encoding='utf-8-sig'))
write(pkg+'test/referenceRefresh.test.ts',(b/'p12-referenceRefresh.test.proposed.ts').read_text(encoding='utf-8-sig'))
s=read(pkg+'serviceClient.ts')
s=rep(s,"import { randomUUID } from 'node:crypto';", "import { randomUUID } from 'node:crypto';\nimport {ReferenceRefreshIntent,refreshConfiguration,refreshDigest,validateRefreshIntent,validateRefreshJob,validateRefreshReceipt,validateRefreshRefusal} from './referenceRefresh';")
s=rep(s,"calibrate:['tokenizer','baseline','returned','generation','content_kind','record']};", "calibrate:['tokenizer','baseline','returned','generation','content_kind','record'],refresh_references:['selection']};")
s=rep(s,"    if(operation==='vectors'&&typeof configuration.enabled!=='boolean')", "    if(operation==='refresh_references')refreshConfiguration(input);\n    if(operation==='vectors'&&typeof configuration.enabled!=='boolean')")
s=rep(s,"'configuration_result','request_receipt'", "'configuration_result','reference_refresh_receipt','request_receipt'")
s=rep(s,'    async prepareConfiguration(project:string,request:ProjectConfigurationRequest):Promise<MutationIntent>{','''    async prepareReferenceRefresh(project:string,draft:ProjectConfigurationRequest,originalQuery:RecordValue,originalResult:RecordValue,persistEvidence=false):Promise<ReferenceRefreshIntent>{
        const key=await this.key(),selection=structuredClone(record(draft.configuration.selection));
        selection.caller=this.caller;selection.key=key;selection.original_query_digest=refreshDigest(originalQuery);selection.original_result_digest=refreshDigest(originalResult);selection.original_evidence_selection_digest=refreshDigest(selection.members);
        const request={...structuredClone(draft),configuration:{operation:'refresh_references',selection}},intent={reference_refresh:{configure_project:{project:identifier(project,32),caller:this.caller,key,request},original_query:structuredClone(originalQuery),original_result:structuredClone(originalResult),evidence_retention:persistEvidence?'local_approved':'memory_only'}};
        delete (request as ProjectConfigurationRequest).prepare;
        return validateRefreshIntent(intent);
    }
    async submitReferenceRefresh(input:ReferenceRefreshIntent):Promise<RecordValue>{
        const intent=validateRefreshIntent(input),response=await this.rpc({configure_project:intent.reference_refresh.configure_project});
        try{if(response.status==='reference_refresh_review_required')validateRefreshRefusal(response,intent);else {if(response.status!=='job')throw new Error('invalid_refresh_ack_status');validateRefreshJob(response.job,intent);}return response;}catch{throw new ServiceError('crossed_reference_refresh_acknowledgment',true,response);}
    }
    async referenceRefreshReceipt(input:ReferenceRefreshIntent):Promise<RecordValue>{
        const intent=validateRefreshIntent(input),request=intent.reference_refresh.configure_project;
        const response=await this.rpc({reference_refresh_receipt:{caller:request.caller,key:request.key}},'reference_refresh_receipt');
        try{validateRefreshReceipt(response,intent);return response;}catch{throw new ServiceError('unqualified_reference_refresh_receipt',true,response);}
    }
    async prepareConfiguration(project:string,request:ProjectConfigurationRequest):Promise<MutationIntent>{
        if(request.configuration.operation==='refresh_references')throw new ServiceError('refresh_requires_original_evidence_review');''')
s=rep(s,'    async submitMutation(intent: MutationIntent): Promise<RecordValue> {\n        validateMutationIntent(intent);','''    async submitMutation(intent: MutationIntent): Promise<RecordValue> {
        validateMutationIntent(intent);
        if('configure_project'in intent&&intent.configure_project.request.configuration.operation==='refresh_references')throw new ServiceError('refresh_requires_exact_receipt_tracking');''')
write(pkg+'serviceClient.ts',s)
s=read(pkg+'serviceIntents.ts')
s=rep(s,"import { SplitIntent,", "import {ReferenceRefreshIntent,REFRESH_JOURNAL_BYTES,refreshIntentKey,validateRefreshIntent,validateRefreshJob,validateRefreshReceipt,validateRefreshRefusal} from './referenceRefresh';\nimport { SplitIntent,")
s=rep(s,'|PolicyResumeIntent|SplitIntent;','|PolicyResumeIntent|SplitIntent|ReferenceRefreshIntent;')
s=rep(s,'export interface SavedIntent { schema:', 'export interface SavedIntent { nativeRefreshReceipt?:Record<string,unknown>;nativeRefreshLookup?:Record<string,unknown>;nativeRefreshValidationError?:string; schema:')
s=rep(s,"    if('catalog_split_apply' in object)", "    if('reference_refresh'in object)return validateRefreshIntent(input);\n    if('catalog_split_apply' in object)")
s=rep(s,"    if(!('lifecycle' in object))return validateMutationIntent(input);", "    if(!('lifecycle' in object)){const intent=validateMutationIntent(input);if('configure_project'in intent&&intent.configure_project.request.configuration.operation==='refresh_references')throw new ServiceError('refresh_requires_original_evidence_review');return intent;}")
s=rep(s,"    if('catalog_split_apply' in intent||'catalog_split_recover' in intent){const key=splitIntentKey(intent);", "    if('reference_refresh'in intent){const key=refreshIntentKey(intent);return {namespace:key.namespace,id:key.nonce};}\n    if('catalog_split_apply' in intent||'catalog_split_recover' in intent){const key=splitIntentKey(intent);")
s=rep(s,"function chargedRecordBytes(value:unknown):number {const saved=record(value),bytes=Buffer.byteLength(JSON.stringify(saved));return saved.reservedObservationBytes===SPLIT_JOURNAL_BYTES?Math.max(bytes,SPLIT_JOURNAL_BYTES):bytes;}", """function reservedBytes(intent:unknown):number|undefined {const value=record(intent);return 'reference_refresh'in value?REFRESH_JOURNAL_BYTES:splitIntent(value)?SPLIT_JOURNAL_BYTES:undefined;}
function maximumRecordBytes(intent:unknown):number{return reservedBytes(intent)??MAX_BYTES;}
function chargedRecordBytes(value:unknown):number {const saved=record(value),bytes=Buffer.byteLength(JSON.stringify(saved));return [SPLIT_JOURNAL_BYTES,REFRESH_JOURNAL_BYTES].includes(Number(saved.reservedObservationBytes))?Math.max(bytes,Number(saved.reservedObservationBytes)):bytes;}""")
for value in ['value.intent','copied','saved.value.intent']:
 s=s.replace(f'(splitIntent({value})?SPLIT_JOURNAL_BYTES:MAX_BYTES)',f'maximumRecordBytes({value})')
s=rep(s,"if(splitIntent(intent)?value.reservedObservationBytes!==SPLIT_JOURNAL_BYTES:value.reservedObservationBytes!==undefined)","if(value.reservedObservationBytes!==reservedBytes(intent))")
s=rep(s,"...(splitIntent(copied)?{reservedObservationBytes:SPLIT_JOURNAL_BYTES,localDispatchState:'submitted' as const}:{})", "...(reservedBytes(copied)!==undefined?{reservedObservationBytes:reservedBytes(copied),localDispatchState:'submitted' as const}:{})")
s=rep(s,'        const copied=validate(JSON.parse(JSON.stringify(intent)));','''        const copied=validate(JSON.parse(JSON.stringify(intent)));
        // Native intent/keys/digests are always durable. Query/snippet text is
        // retained only under an explicit existing persistence approval.
        if('reference_refresh'in copied&&copied.reference_refresh.evidence_retention==='memory_only'){
            copied.reference_refresh.original_query=null;copied.reference_refresh.original_result=null;
        }''')

s=rep(s,'    /** Preserve the full authenticated native outcome without changing the immutable request. */','''    /** Full bounded current lookup and last accepted historical proof stay separate. */
    observeRefresh(id:string,response:Record<string,unknown>):Promise<void>{
        const copied=structuredClone(response);
        return this.serialize(async()=>{
            const saved=this.list().find(value=>value.id===id);if(!saved||!('reference_refresh'in saved.value.intent))throw new ServiceError('local_refresh_intent_not_found',true);
            const intent=saved.value.intent;let known=saved.value.nativeRefreshReceipt,error:string|undefined;
            try{
                if(copied.status==='reference_refresh_review_required')validateRefreshRefusal(copied,intent);
                else if(copied.status==='job')validateRefreshJob(copied.job,intent);
                else validateRefreshReceipt(copied,intent);
                if(copied.status==='reference_refresh_review_required'){if(known?.job!=null)throw new Error('accepted_refresh_cannot_become_unaccepted_refusal');known=copied;}
                else if(copied.job!==null&&(copied.status==='job'||copied.configuration!==null)){
                    const prior=known?.status==='reference_refresh_receipt'?known:undefined;
                    if(prior?.result!=null&&copied.result!=null&&!isDeepStrictEqual(prior.result,copied.result))throw new Error('historical_refresh_result_changed');
                    if(prior?.configuration!=null&&copied.configuration!=null&&!isDeepStrictEqual(prior.configuration,copied.configuration))throw new Error('historical_refresh_work_changed');
                    const previous=prior&&record(prior.job),next=record(copied.job);
                    if(previous&&['succeeded','failed','canceled'].includes(String(previous.state))&&!isDeepStrictEqual(previous,next))throw new Error('historical_refresh_terminal_job_changed');
                    if(!prior||copied.status==='reference_refresh_receipt'&&(prior.result==null||copied.result!=null))known=copied;
                }
            }catch(failure){error=failure instanceof Error?failure.message:'invalid_refresh_receipt';}
            const value:SavedIntent={...saved.value,nativeRefreshLookup:copied,...(known?{nativeRefreshReceipt:structuredClone(known),localDispatchState:'known_receipt' as const}:{}),...(error?{nativeRefreshValidationError:error}:{})};
            if(!error)delete value.nativeRefreshValidationError;
            if(Buffer.byteLength(JSON.stringify(value))>REFRESH_JOURNAL_BYTES)throw new ServiceError('local_intent_record_oversize',true);
            const total=this.storage.keys().filter(key=>key.startsWith(PREFIX)&&key!==this.key(id)).reduce((sum,key)=>{const other=this.storage.get(key);return sum+(other===undefined?0:chargedRecordBytes(other));},chargedRecordBytes(value));
            if(total>MAX_TOTAL_BYTES)throw new ServiceError('local_intent_byte_capacity_exceeded',true);
            await this.storage.update(this.key(id),value);
            if(error)throw new ServiceError(error,true,copied);
        });
    }
    /** Preserve the full authenticated native outcome without changing the immutable request. */''')
# Dedicated native receipt reads are the only refresh reconciliation; generic
# journal submit/reconcile must never replay or prematurely forget its intent.
s=s.replace("&&!('catalog_split_recover' in saved.value.intent));", "&&!('catalog_split_recover' in saved.value.intent)&&!('reference_refresh'in saved.value.intent));")
s=rep(s,"||'catalog_split_recover' in intent)continue;", "||'catalog_split_recover' in intent||'reference_refresh'in intent)continue;")
s=rep(s,"        // Catalog applies are never replayed from a saved journal entry. Only receipt reads may settle them.", "        if('reference_refresh'in intent)throw new ServiceError('refresh_requires_exact_native_receipt_reconciliation');\n        // Catalog applies are never replayed from a saved journal entry. Only receipt reads may settle them.")
write(pkg+'serviceIntents.ts',s)
s=read('scripts/package-extension.py');s=rep(s,'"indexingCatalog.js", "indexingSplit.js",','"indexingCatalog.js", "indexingSplit.js", "referenceRefresh.js",');write('scripts/package-extension.py',s)
(b/'p12-workbench-base.json').write_text(json.dumps(bases,indent=2)+'\n')
print(json.dumps({'files':len(list(out.rglob('*.ts'))),'status':'INERT no TS compile/native execution; actual UI action wiring remains separate'}))
