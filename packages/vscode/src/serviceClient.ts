// SPDX-License-Identifier: Apache-2.0
import { ChildProcessWithoutNullStreams, spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import {ReferenceRefreshIntent,refreshConfiguration,refreshDigest,validateRefreshIntent,validateRefreshJob,validateRefreshReceipt,validateRefreshRefusal} from './referenceRefresh';
import * as path from 'node:path';
import { requireNativeExecution } from './nativeGate';
import {ServiceError,validateNativeReply,retainUnvalidatedNativeReply} from './nativeOutcome';
export {ServiceError} from './nativeOutcome';
import { CatalogApplyIntent, CatalogObject, CatalogRequest, CatalogReview, IndexSetupApplyIntent, IndexSetupReview, catalogBound, catalogFenceObservation, catalogHex, catalogObject, catalogRevision, sameCatalogValue, CATALOG_RECEIPT_BYTES, validateCatalogApply, validateGrantSelection, validateNativeGrantDraft, validateNativeHostContext, validateIndexSetupSelection, validateIndexSetupApply, validateNativeIndexSetupDraft, validateIndexSetupReceipt, indexSetupObservation, validateNativeLegacyBinding } from './indexingCatalog';
import { SourceResolveIntent, PolicyResumeIntent, validatePolicyResumeIntent, validateNativeSourceDraft, validateSourceMutationSelection, validateSourceResolveIntent, sourceChangeObservation, validateNativeRetirementRequest } from './indexingCatalog';
import { validatePolicyStatus, validateNativePolicyConfiguration } from './indexingCatalog';
import { SplitApplyIntent, SplitRecoverIntent, SplitReview, SPLIT_RESPONSE_BYTES, validateSplitApply, validateSplitContinuity, validateSplitDraft, validateSplitRecover, validateSplitRecoveryReceipt, validateSplitSelection, validateSplitSnapshot, validateSplitSourceState } from './indexingSplit';

export type RecordValue = Record<string, unknown>;
export function record(value: unknown): RecordValue {
    if (!value || typeof value !== 'object' || Array.isArray(value)) { throw new Error('Invalid AIRK service object.'); }
    return value as RecordValue;
}
export function identifier(value: unknown, bytes: 16 | 32): string {
    if (typeof value !== 'string' || !(bytes === 16 ? /^[0-9a-f]{32}$/ : /^[0-9a-f]{64}$/).test(value)) {
        throw new Error('Invalid AIRK service identity.');
    }
    return value;
}
export function count(value: unknown): number {
    if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) { throw new Error('Invalid AIRK service count.'); }
    return value;
}
export interface ControlTransport { request(method: unknown): Promise<RecordValue>; dispose(): void }
interface Pending {
    id: string; bytes: Buffer; resolve(value: RecordValue): void; reject(error: Error): void;
    timer?: NodeJS.Timeout;
}
/** One persistent native metadata adapter per client. It never carries evidence or credentials. */
export class NativeControlTransport implements ControlTransport {
    private readonly child: ChildProcessWithoutNullStreams;
    private readonly queue: Pending[] = [];
    private active?: Pending;
    private stopped = false;
    private prefix = Buffer.alloc(4);
    private prefixBytes = 0;
    private body?: Buffer;
    private bodyBytes = 0;
    constructor(command: string, args: string[], private readonly timeout = 20000,
        launch: (command: string, args: string[]) => ChildProcessWithoutNullStreams = (file, argv) =>
            spawn(file, argv, { cwd: path.isAbsolute(file) ? path.dirname(file) : undefined, windowsHide: true, stdio: 'pipe', env: { ...process.env, AIRK_CLIENT: 'vscode', AIRK_ERROR_FORMAT: 'json' } })) {
        requireNativeExecution();
        this.child = launch(command, args);
        this.child.stdout.on('data', (bytes: Buffer) => { try { this.read(bytes); } catch { this.fail('invalid_console_response'); } });
        // Diagnostics arrive as bounded structured results. Do not forward raw stderr/environment/argv.
        this.child.stderr.resume();
        this.child.on('error', () => this.fail('console_transport_unavailable'));
        this.child.on('exit', () => this.fail('console_transport_closed'));
        this.child.stdin.on('error', () => this.fail('console_transport_closed'));
    }
    request(method: unknown): Promise<RecordValue> {
        if (this.stopped) { return Promise.reject(new ServiceError('console_transport_closed')); }
        if (this.queue.length + (this.active ? 1 : 0) >= 8) { return Promise.reject(new ServiceError('console_queue_full')); }
        const id = randomUUID().replaceAll('-', '');
        const body = Buffer.from(JSON.stringify({ id, method }), 'utf8');
        if (!body.length || body.length > 65536) { return Promise.reject(new ServiceError('console_request_oversize')); }
        const bytes = Buffer.alloc(4 + body.length); bytes.writeUInt32BE(body.length); body.copy(bytes, 4);
        return new Promise((resolve, reject) => { this.queue.push({ id, bytes, resolve, reject }); this.pump(); });
    }
    private pump(): void {
        if (this.stopped || this.active || !this.queue.length) { return; }
        const pending = this.queue.shift()!; this.active = pending;
        pending.timer = setTimeout(() => this.fail('console_deadline_exceeded'), this.timeout);
        this.child.stdin.write(pending.bytes, error => { if (error) { this.fail('console_transport_closed'); } });
    }
    private read(bytes: Buffer): void {
        let offset = 0;
        while (offset < bytes.length && !this.stopped) {
            if (!this.body) {
                const copied = Math.min(4 - this.prefixBytes, bytes.length - offset);
                bytes.copy(this.prefix, this.prefixBytes, offset, offset + copied);
                this.prefixBytes += copied; offset += copied;
                if (this.prefixBytes < 4) { continue; }
                const length = this.prefix.readUInt32BE();
                if (!length || length > 65536) { throw new Error('Invalid frame length'); }
                this.body = Buffer.allocUnsafe(length); this.bodyBytes = 0;
            }
            const copied = Math.min(this.body.length - this.bodyBytes, bytes.length - offset);
            bytes.copy(this.body, this.bodyBytes, offset, offset + copied);
            this.bodyBytes += copied; offset += copied;
            if (this.bodyBytes === this.body.length) {
                const value = record(JSON.parse(this.body.toString('utf8')));
                const active = this.active;
                if (!active || value.id !== active.id) { throw new Error('Crossed console request'); }
                const response = record(value.response);
                if (typeof response.status !== 'string' || response.status.length > 64) { throw new Error('Invalid console result'); }
                clearTimeout(active.timer); this.active = undefined; this.body = undefined; this.prefixBytes = 0;
                active.resolve(response); this.pump();
            }
        }
    }
    private fail(code: string): void {
        if (this.stopped) { return; } this.stopped = true;
        if (this.active) { clearTimeout(this.active.timer); this.active.reject(new ServiceError(code, true)); this.active = undefined; }
        for (const pending of this.queue.splice(0)) { pending.reject(new ServiceError(code)); }
        this.body = undefined;
        this.child.stdin.end();
        if (this.child.exitCode === null && this.child.signalCode === null) { this.child.kill(); }
    }
    dispose(): void { this.fail('console_client_closed'); }
}
export interface RequestKey { namespace: string; epoch: number; nonce: string }
export interface JobIntent { project: string; caller: string; key: RequestKey; kind: JobKind }
export type JobKind = 'index' | 'reconcile' | 'vectors' | 'model_setup' | 'verify' | 'inspect' | 'recover' | 'maintain';
export interface RetryIntent { id: string; caller: string; key: RequestKey }
export interface PlanIntent { caller: string; key: RequestKey; request: { selection: 'all' | { selected: string[] }; operation: RepairOperation } }
export interface ProjectConfigurationRequest { incarnation:string;settings:string;configuration:RecordValue;prepare?:boolean }
export interface ConfigurationIntent {project:string;caller:string;key:RequestKey;request:ProjectConfigurationRequest}
export type MutationIntent = { enqueue: JobIntent } | { retry: RetryIntent } | { plan_create: PlanIntent } | {configure_project:ConfigurationIntent};
export function validateConfigurationRequest(input:unknown):ProjectConfigurationRequest {
    const value=record(input),configuration=record(value.configuration),operation=configuration.operation;
    if(!['configuration,incarnation,settings','configuration,incarnation,prepare,settings'].includes(Object.keys(value).sort().join(','))||Buffer.byteLength(JSON.stringify(value))>16384)throw new ServiceError('invalid_project_configuration');
    if(value.prepare!==undefined&&(typeof value.prepare!=='boolean'||value.prepare&&!['vectors','detail'].includes(String(operation))))throw new ServiceError('invalid_project_configuration');
    identifier(value.incarnation,16);identifier(value.settings,32);
    const names:Record<string,string[]>={unregister:[],prepare_derived_cleanup:[],delete_derived:['preview'],bundled_model:[],vectors:['enabled'],detail:['level'],retained_profile:['review'],vector_selection:['policy'],
        add_root:['alias','path'],remove_root:['alias'],install_model:['directory','manifest_sha256'],usage:['mode'],
        clear_usage:[],reset_scan_limits:[],usage_feedback:['request_id','usefulness'],import_usage:['input'],extraction_policy:['input'],scan_limits:['input'],quota_policy:['input','expected_policy_digest'],
        calibrate:['tokenizer','baseline','returned','generation','content_kind','record'],refresh_references:['selection']};
    if(typeof operation!=='string'||!Object.hasOwn(names,operation)||Object.keys(configuration).sort().join(',')!==['operation',...names[operation]].sort().join(','))throw new ServiceError('invalid_project_configuration');
    const filePath=(input:unknown)=>{if(typeof input!=='string'||!path.isAbsolute(input)||input.length>4096||/[\u0000-\u001f\u007f]/.test(input))throw new ServiceError('invalid_configuration_path');};
    if(operation==='refresh_references')refreshConfiguration(input);
    if(operation==='vectors'&&typeof configuration.enabled!=='boolean')throw new ServiceError('invalid_project_configuration');
    if(operation==='detail'&&!['essential','agent_lean','balanced','detailed'].includes(String(configuration.level)))throw new ServiceError('invalid_project_configuration');
    if(operation==='retained_profile'){
        const review=record(configuration.review);
        if(Object.keys(review).sort().join(',')!=='desired_policy,expected_applied_policy,expected_publication,reviewed_configured_policy,schema,target'||review.schema!==1||!['lean','rich'].includes(String(review.target))||typeof review.expected_publication!=='string'||review.expected_publication.length>126||!/^airk-r1:[a-f0-9]{32}:(?:0|[1-9][0-9]*):[a-f0-9]{64}$/.test(review.expected_publication))throw new ServiceError('invalid_retained_migration_review');
        for(const field of ['desired_policy','expected_applied_policy','reviewed_configured_policy'])identifier(review[field],32);
    }
    if(operation==='delete_derived')identifier(configuration.preview,32);
    if(operation==='usage'&&!['disabled','aggregates_only','diagnostic_events'].includes(String(configuration.mode)))throw new ServiceError('invalid_project_configuration');
    if(operation==='usage_feedback'){
        identifier(configuration.request_id,16);
        if(!['useful','partly_useful','not_useful'].includes(String(configuration.usefulness)))throw new ServiceError('invalid_project_configuration');
    }
    if(['import_usage','extraction_policy','scan_limits','quota_policy'].includes(String(operation))){
        const input=record(configuration.input);
        if(Object.keys(input).sort().join(',')!=='path,sha256')throw new ServiceError('invalid_project_configuration');
        filePath(input.path);identifier(input.sha256,32);
        if(operation==='quota_policy')identifier(configuration.expected_policy_digest,32);
    }
    if(operation==='add_root'||operation==='remove_root'){
        if(typeof configuration.alias!=='string'||!/^[A-Za-z0-9_-]{1,64}$/.test(configuration.alias))throw new ServiceError('invalid_configuration_alias');
        if(operation==='add_root')filePath(configuration.path);
    }
    if(operation==='install_model'){filePath(configuration.directory);identifier(configuration.manifest_sha256,32);}
    if(operation==='calibrate'){
        for(const name of ['tokenizer','baseline','returned']){const file=record(configuration[name]);
            if(Object.keys(file).sort().join(',')!=='path,sha256')throw new ServiceError('invalid_configuration_file');filePath(file.path);identifier(file.sha256,32);}
        if(count(configuration.generation)===0||typeof configuration.record!=='boolean'||!['code','structured_text','markdown','pdf','docx','xlsx','pptx','svg','plain_text','mixed','unknown'].includes(String(configuration.content_kind)))throw new ServiceError('invalid_calibration_configuration');
    }
    if(operation==='vector_selection'){
        const policy=record(configuration.policy);
        if(policy.version!==1||Object.keys(policy).some(key=>!['version','exclusions','inclusions','agent_defaults'].includes(key))
            ||!Array.isArray(policy.exclusions)||policy.agent_defaults!==undefined&&typeof policy.agent_defaults!=='boolean')throw new ServiceError('invalid_vector_selection_configuration');
        for(const rules of [policy.exclusions,policy.inclusions??[]]){
            if(!Array.isArray(rules)||rules.length>128)throw new ServiceError('invalid_vector_selection_configuration');
            for(const value of rules){const rule=record(value);
                if(typeof rule.id!=='string'||!rule.id||rule.id.length>128||Object.keys(rule).some(key=>!['id','path_prefix','basename','suffix','adapter','language'].includes(key)))throw new ServiceError('invalid_vector_selection_configuration');
                for(const [key,value] of Object.entries(rule))if(key!=='id'&&value!==null&&(typeof value!=='string'||value.length>4096||/[\u0000-\u001f\u007f]/.test(value)))throw new ServiceError('invalid_vector_selection_configuration');
            }
        }
    }
    return input as ProjectConfigurationRequest;
}
export type RepairOperation = 'auto' | 'verify' | 'recover' | 'reconcile' | 'vectors' | 'rebuild';
export function validateMutationIntent(input:unknown): MutationIntent {
    const intent=record(input),variants=Object.keys(intent);
    if(variants.length!==1||!['enqueue','retry','plan_create','configure_project'].includes(variants[0]))throw new ServiceError('invalid_mutation_intent');
    const kind=variants[0],value=record(intent[kind]),key=record(value.key);
    identifier(value.caller,16);identifier(key.namespace,16);identifier(key.nonce,16);count(key.epoch);
    if(Object.keys(key).sort().join(',')!=='epoch,namespace,nonce')throw new ServiceError('invalid_mutation_key');
    const keys=kind==='enqueue'?'caller,key,kind,project':kind==='retry'?'caller,id,key':kind==='configure_project'?'caller,key,project,request':'caller,key,request';
    if(Object.keys(value).sort().join(',')!==keys)throw new ServiceError('invalid_mutation_fields');
    if(kind==='enqueue'){
        identifier(value.project,32);
        if(!['index','reconcile','vectors','model_setup','verify','inspect','recover','maintain'].includes(String(value.kind)))throw new ServiceError('invalid_job_intent');
    }else if(kind==='retry'){identifier(value.id,32);}
    else if(kind==='configure_project'){identifier(value.project,32);validateConfigurationRequest(value.request);}
    else {
        const request=record(value.request);
        if(Object.keys(request).sort().join(',')!=='operation,selection'||!['auto','verify','recover','reconcile','vectors','rebuild'].includes(String(request.operation)))throw new ServiceError('invalid_repair_intent');
        if(request.selection!=='all'){
            const selection=record(request.selection),members=selection.selected;
            if(Object.keys(selection).join(',')!=='selected'||!Array.isArray(members)||!members.length||members.length>256||new Set(members).size!==members.length)throw new ServiceError('invalid_repair_selection');
            members.forEach(member=>identifier(member,32));
        }
    }
    return input as MutationIntent;
}
export interface ConsolePage { revision: number; cached: true; observed_unix_ms: number; projects: RecordValue[]; after: string | null }
export function parseConsolePage(input:unknown,limit=32):ConsolePage{
    const page=record(input);count(page.revision);count(page.observed_unix_ms);
    if(page.cached!==true||!Array.isArray(page.projects)||page.projects.length>limit)throw new ServiceError('invalid_console_page');
    const projects=page.projects.map(value=>{const row=record(value);identifier(record(record(row.project).registration).binding,32);return row;});
    if(page.after!==null)identifier(page.after,32);
    return {revision:page.revision as number,observed_unix_ms:page.observed_unix_ms as number,cached:true,projects,after:page.after as string|null};
}
export class ServiceClient {
    private readonly caller = randomUUID().replaceAll('-', '');
    constructor(readonly transport: ControlTransport) {}
    async rpc(method: unknown, expected?: string): Promise<RecordValue> {
        const name=typeof method==='string'?method:method&&typeof method==='object'?Object.keys(method)[0]:undefined;
        // The native management boundary owns bounded registry-busy recovery for
        // every frontend. Never multiply that budget or replay uncertain work here.
        const response = await this.transport.request(method);
        if (response.status === 'failure') {
            const code = typeof response.code === 'string' && /^[a-z0-9_]{1,128}$/.test(response.code) ? response.code : 'invalid_service_failure';
            // Older catalog handlers can collapse a post-prepare failure into this generic code.
            // Its absence of a receipt does not establish that an accepted apply had no effect.
            const error = new ServiceError(code, code.endsWith('_outcome_unknown') || code === 'registry_response_unknown_retry_with_key'||['catalog_apply','catalog_index_setup_apply','catalog_source_change_resolve','catalog_policy_resume','catalog_split_apply','catalog_split_recover'].includes(String(name))&&code==='invalid_registry_request',response);
            if(code==='invalid_service_failure')error.cause=new Error(`Invalid native failure code: ${String(JSON.stringify(response.code)).slice(0,2048)}`);
            throw error;
        }
        if(response.status==='source_change_busy')this.rejectSourceChangeBusy(method,response);
        if (expected && response.status !== expected) { throw new ServiceError('unexpected_service_response', true,response); }
        return response;
    }
    private async mutationReply(method:unknown,expected?:string):Promise<RecordValue>{
        try{return await this.rpc(method,expected);}catch(error){
            if(error instanceof ServiceError&&error.nativeResponse&&['unexpected_service_response','invalid_service_failure','crossed_source_change_busy'].includes(error.code))throw retainUnvalidatedNativeReply(error,error.nativeResponse);
            throw error;
        }
    }
    /** Native source review/admission returns this refusal before inserting work.
     * Keep the full response, but qualify no-effect only for its exact index. */
    private rejectSourceChangeBusy(method:unknown,response:RecordValue):never {
        let pending=0,running=0;
        try{
            catalogBound(response,65536);
            const request=record(method),names=Object.keys(request);
            if(names.length!==1)throw new Error('ambiguous_source_method');
            const name=names[0],body=record(request[name]);let index:unknown;
            if(name==='catalog_source_mutation_draft'){
                const selection=record(body.selection);validateSourceMutationSelection(selection.change);index=selection.index_id;
            }else if(name==='catalog_preview'||name==='catalog_apply'){
                const operation=record(record(body.request).operation);
                if(!['begin_index_source_change','change_index_policy','retire_index'].includes(String(operation.operation)))throw new Error('unrelated_catalog_operation');
                index=operation.id;
            }else throw new Error('unrelated_source_method');
            const busy=record(response.busy);pending=count(busy.pending_jobs);running=count(busy.running_jobs);
            if(typeof index!=='string'||!/^idx_[0-9a-f]{32}$/.test(index)||busy.index_id!==index||response.code!=='source_change_jobs_pending'||pending>4294967295||running>4294967295||pending+running===0)throw new Error('invalid_source_busy_identity_or_counts');
        }catch(cause){const error=new ServiceError('crossed_source_change_busy',true,response);error.cause=cause;throw error;}
        const error=new ServiceError('source_change_jobs_pending',false,response);
        error.message=`Finish the current indexing work before changing sources (${pending} queued, ${running} running). Review jobs in Management.`;
        throw error;
    }
    async projects(after: string | null = null, revision: number | null = null, limit = 16, inactive = false): Promise<ConsolePage> {
        if (after !== null) { identifier(after,32); } if (revision !== null) { count(revision); }
        if (!Number.isInteger(limit) || limit < 1 || limit > 32) { throw new ServiceError('invalid_project_page_limit'); }
        return parseConsolePage((await this.rpc({ [inactive?'inactive_console':'console']: { after, revision, limit } }, 'console')).page,limit);
    }
    async workers(projects: string[]): Promise<RecordValue> {
        if (projects.length > 32 || new Set(projects).size !== projects.length) { throw new ServiceError('invalid_worker_observation_scope'); }
        projects.forEach(project => identifier(project,32));
        const result = await this.rpc({ workers: { projects } },'workers');
        count(result.age_ms);
        if (!Array.isArray(result.observations) || result.observations.length !== projects.length) { throw new ServiceError('invalid_worker_observations'); }
        result.observations.forEach((value,index) => {
            const observation = record(value);
            if (observation.project !== projects[index] || typeof observation.restarting !== 'boolean' || typeof observation.automatic_start_blocked !== 'boolean') { throw new ServiceError('crossed_worker_observation'); }
            if (observation.identity !== null) {
                const identity = record(observation.identity);
                identifier(identity.service,16); identifier(identity.worker,16);
                if (identity.project !== observation.project || count(identity.fencing_generation) === 0) { throw new ServiceError('crossed_worker_identity'); }
            }
        });
        return result;
    }
    async key(): Promise<RequestKey> {
        const response = await this.rpc('request_epoch','request_epoch');
        return { namespace: identifier(response.namespace,16), epoch: count(response.epoch), nonce: randomUUID().replaceAll('-','') };
    }
    async prepareCatalog(operation: RecordValue, revision: number): Promise<CatalogRequest> {
        return { caller: this.caller, key: await this.key(), expected_catalog_revision: revision, operation: structuredClone(operation) };
    }
    async catalogHostContext(key: string, expectedCatalogRevision: number | null = null): Promise<RecordValue> {
        catalogHex(key,64);if(expectedCatalogRevision!==null)catalogRevision(expectedCatalogRevision);
        const response=await this.rpc({catalog_host_context:{key,expected_catalog_revision:expectedCatalogRevision}},'catalog_host_context');
        const descriptor=validateNativeHostContext(response.descriptor);
        if(descriptor.key!==key||expectedCatalogRevision!==null&&descriptor.catalog_revision!==expectedCatalogRevision)throw new ServiceError('crossed_native_host_context');
        return response;
    }
    async catalogLegacyBinding(project:string,expectedCatalogRevision:number|null=null):Promise<RecordValue>{
        catalogHex(project,64);if(expectedCatalogRevision!==null)catalogRevision(expectedCatalogRevision);
        return validateNativeLegacyBinding(await this.rpc({catalog_legacy_binding:{project,expected_catalog_revision:expectedCatalogRevision}},'catalog_legacy_binding'),project,expectedCatalogRevision);
    }
    async catalogGrantDraft(contextKey:string,selection:CatalogObject):Promise<CatalogReview>{
        selection=structuredClone(selection);
        catalogHex(contextKey,64);catalogRevision(selection.expected_catalog_revision);validateGrantSelection({project_ids:selection.project_ids,index_ids:selection.index_ids});
        const key=record(selection.key);identifier(key.namespace,16);identifier(key.nonce,16);count(key.epoch);
        if(Object.keys(selection).sort().join(',')!=='expected_catalog_revision,index_ids,key,project_ids'||Object.keys(key).sort().join(',')!=='epoch,namespace,nonce')throw new ServiceError('invalid_grant_draft_selection');
        const response=await this.rpc({catalog_grant_draft:{context_key:contextKey,selection}},'catalog_grant_draft');
        return {...validateNativeGrantDraft(response,selection),contextKey};
    }
    async submitCatalog(intent: CatalogApplyIntent): Promise<RecordValue> {
        validateCatalogApply(intent);
        const response = await this.mutationReply({catalog_apply:intent.catalog_apply});
        if(response.status==='catalog_outcome_unknown'){
            validateNativeReply(response,()=>this.validateCatalogUnknown(response,intent.catalog_apply.request,intent.request_digest,intent.catalog_apply.preview_digest));
            throw new ServiceError(String(response.code),true,response);
        }
        if(response.status!=='catalog_applied')throw retainUnvalidatedNativeReply(new ServiceError('unexpected_service_response',true),response);
        validateNativeReply(response,()=>this.validateCatalogReceipt(response,intent.catalog_apply.request,intent.request_digest,intent.catalog_apply.preview_digest));
        return response;
    }
    async catalogSourceDraft(selection:CatalogObject):Promise<CatalogReview>{
        selection=structuredClone(selection);const key=record(selection.key);identifier(key.namespace,16);identifier(key.nonce,16);count(key.epoch);catalogRevision(selection.expected_catalog_revision);
        if(Object.keys(selection).sort().join(',')!=='change,expected_catalog_revision,index_id,key'||Object.keys(key).sort().join(',')!=='epoch,namespace,nonce'||typeof selection.index_id!=='string'||!/^idx_[0-9a-f]{32}$/.test(selection.index_id))throw new ServiceError('invalid_source_draft_selection');validateSourceMutationSelection(selection.change);
        return validateNativeSourceDraft(await this.rpc({catalog_source_mutation_draft:{selection}},'catalog_source_mutation_draft'),selection);
    }
    async catalogPendingSource(index:string,namespace:string):Promise<RecordValue>{
        if(!/^idx_[0-9a-f]{32}$/.test(index))throw new ServiceError('invalid_source_index');catalogHex(namespace);
        const response=await this.rpc({catalog_pending_source_change:{index_id:index}},'catalog_pending_source_change');return validateNativeReply(response,()=>{catalogBound(response,65536);
        if(response.index_id!==index)throw new ServiceError('crossed_pending_source_index',true,response);
        const observed=sourceChangeObservation(response);
        if(observed){const begin=record(observed.begin),id=identifier(record(begin.metadata).operation_id,16);this.validateSourceObservation(response,id,namespace);if(record(begin.request).operation&&record(record(begin.request).operation).id!==index)throw new ServiceError('crossed_pending_source_begin',true,response);}
        else if(response.observation!==null)throw new ServiceError('invalid_pending_source_observation',true,response);
        return response;});
    }
    async catalogPolicyStatus(index:string):Promise<RecordValue>{
        if(!/^idx_[0-9a-f]{32}$/.test(index))throw new ServiceError('invalid_source_policy_index');
        const response=await this.rpc({catalog_policy_status:{index_id:index}},'catalog_policy_status');
        return validateNativeReply(response,()=>validatePolicyStatus(response,index));
    }
    async catalogSplitDraft(selection:CatalogObject):Promise<SplitReview>{
        catalogBound(selection,16384);
        const response=await this.rpc({catalog_split_draft:{selection}},'catalog_split_draft');
        catalogBound(response,SPLIT_RESPONSE_BYTES);const draft=validateSplitDraft(response.draft,selection);
        return {response,draft,selection:structuredClone(selection)};
    }
    async catalogSplitSourceState(index:string,revision:number):Promise<RecordValue>{
        if(!/^idx_[0-9a-f]{32}$/.test(index))throw new ServiceError('invalid_split_index');catalogRevision(revision);
        return validateSplitSourceState(await this.rpc({catalog_split_source_state:{index_id:index,expected_catalog_revision:revision}},'catalog_split_source_state'),index,revision);
    }
    async submitSplit(input:SplitApplyIntent):Promise<RecordValue>{
        const intent=validateSplitApply(structuredClone(input));
        const response=await this.mutationReply(intent,'catalog_split_accepted');
        try{catalogBound(response,SPLIT_RESPONSE_BYTES);validateSplitSnapshot(response.snapshot,intent.catalog_split_apply.draft);return response;}
        catch(error){const failure=new ServiceError('invalid_native_split_acceptance',true,response);failure.cause=error;throw retainUnvalidatedNativeReply(failure,response);}
    }
    async catalogSplitStatus(operation:string):Promise<RecordValue>{
        catalogHex(operation);const response=await this.rpc({catalog_split_status:{operation_id:operation}},'catalog_split_status');catalogBound(response,SPLIT_RESPONSE_BYTES);
        if(response.operation_id!==operation)throw new ServiceError('crossed_split_status',true,response);
        if(response.snapshot!==null&&catalogObject(validateSplitSnapshot(response.snapshot).receipt).operation_id!==operation)throw new ServiceError('crossed_split_status',true,response);return response;
    }
    async catalogSplitReceipt(input:SplitApplyIntent):Promise<RecordValue>{
        const intent=validateSplitApply(input),request=catalogObject(intent.catalog_split_apply.draft.request);
        const response=await this.rpc({catalog_split_receipt:{caller:request.caller,key:request.key}},'catalog_split_receipt');return validateNativeReply(response,()=>{catalogBound(response,SPLIT_RESPONSE_BYTES);
        if(response.caller!==request.caller||!sameCatalogValue(response.key,request.key)||typeof response.expired!=='boolean')throw new ServiceError('crossed_split_receipt',true,response);
        if(response.snapshot!==null){validateSplitSnapshot(response.snapshot,intent.catalog_split_apply.draft);if(response.expired)throw new ServiceError('crossed_split_receipt',true,response);}return response;});
    }
    async prepareSplitRecovery(reviewed:CatalogObject,action:'resume'|'cancel'):Promise<SplitRecoverIntent>{
        validateSplitSnapshot(reviewed);const receipt=catalogObject(reviewed.receipt);
        return validateSplitRecover({reviewed:structuredClone(reviewed),catalog_split_recover:{request:{caller:this.caller,key:await this.key(),operation_id:receipt.operation_id,request_digest:receipt.request_digest,expected_receipt_digest:reviewed.receipt_digest,...(reviewed.finalization!=null?{expected_finalization_revision:catalogObject(reviewed.finalization).revision}:{}),action}}});
    }
    async submitSplitRecovery(input:SplitRecoverIntent):Promise<RecordValue>{
        const intent=validateSplitRecover(structuredClone(input));
        let response:RecordValue|undefined;
        try{response=await this.mutationReply({catalog_split_recover:intent.catalog_split_recover},'catalog_split_recovered');validateNativeReply(response,()=>this.validateSplitRecoveryResponse(response!,intent));return response;}
        catch(error){if(response)throw retainUnvalidatedNativeReply(error,response,'invalid_native_split_recovery');throw error;}
    }
    async catalogSplitRecoveryReceipt(input:SplitRecoverIntent):Promise<RecordValue>{
        const intent=validateSplitRecover(input),request=intent.catalog_split_recover.request;
        const response=await this.rpc({catalog_split_recovery_receipt:{operation_id:request.operation_id,caller:request.caller,key:request.key}},'catalog_split_recovery_receipt');
        validateNativeReply(response,()=>this.validateSplitRecoveryResponse(response,intent));return response;
    }
    validateSplitRecoveryResponse(response:RecordValue,intent:SplitRecoverIntent):void{
        catalogBound(response,SPLIT_RESPONSE_BYTES);const request=intent.catalog_split_recover.request;
        if(response.status==='catalog_split_recovery_receipt'&&(response.operation_id!==request.operation_id||response.caller!==request.caller||!sameCatalogValue(response.key,request.key)||typeof response.expired!=='boolean'))throw new ServiceError('crossed_split_recovery_receipt',true,response);
        if(response.receipt!==null){validateSplitRecoveryReceipt(response.receipt,intent);if(response.expired===true)throw new ServiceError('crossed_split_recovery_receipt',true,response);}
        else if(response.status==='catalog_split_recovered')throw new ServiceError('incomplete_split_recovery',true,response);
        if(response.snapshot!==null){const snapshot=validateSplitSnapshot(response.snapshot);if(!sameCatalogValue(snapshot.request,intent.reviewed.request)||!sameCatalogValue(snapshot.baseline,intent.reviewed.baseline)||!sameCatalogValue(snapshot.plan,intent.reviewed.plan))throw new ServiceError('crossed_split_recovery_status',true,response);validateSplitContinuity(intent.reviewed,snapshot);if(response.receipt!=null){const outcome=catalogObject(catalogObject(response.receipt).outcome);if(outcome.resulting_receipt){const accepted={...intent.reviewed,receipt:outcome.resulting_receipt,receipt_digest:outcome.receipt_digest};validateSplitContinuity(accepted,snapshot);}if(outcome.outcome==='finalization_accepted'){const final=catalogObject(snapshot.finalization);if(Number(final.revision)<Number(outcome.finalization_revision)||final.terminal_receipt_digest!==outcome.terminal_receipt_digest||Number(final.revision)===Number(outcome.finalization_revision)&&final.attention!=null)throw new ServiceError('crossed_split_finalization_status',true,response);if(snapshot.receipt_digest===outcome.terminal_receipt_digest&&!sameCatalogValue(snapshot.receipt,outcome.terminal_receipt))throw new ServiceError('crossed_split_terminal_receipt',true,response);}}}
    }
    async catalogPolicyResumeReceipt(input:PolicyResumeIntent):Promise<RecordValue>{
        const intent=validatePolicyResumeIntent(structuredClone(input)),request=intent.catalog_policy_resume;
        const response=await this.rpc({catalog_policy_resume_receipt:request},'catalog_policy_resume_receipt');
        validateNativeReply(response,()=>this.validatePolicyResumeReceipt(response,intent));return response;
    }
    validatePolicyResumeReceipt(response:RecordValue,input:PolicyResumeIntent):boolean{
        const intent=validatePolicyResumeIntent(structuredClone(input)),request=intent.catalog_policy_resume;
        catalogBound(response,65536);
        if(response.status!=='catalog_policy_resume_receipt')throw new ServiceError('crossed_policy_resume_receipt',true,response);
        if(response.index_id!==request.index_id||response.operation_id!==request.operation_id||response.caller!==request.caller||!sameCatalogValue(response.key,request.key)||typeof response.expired!=='boolean')throw new ServiceError('crossed_policy_resume_receipt',true,response);
        if(response.receipt===null)return false;
        const receipt=record(response.receipt),job=record(receipt.job),work=record(receipt.configuration);
        catalogHex(receipt.source_job,64);catalogHex(job.id,64);catalogHex(work.engine,64);
        const policy=validateNativePolicyConfiguration(work.request),configuration=record(record(work.request).configuration);
        if(configuration.operation_id!==request.operation_id||policy.index_id!==request.index_id||policy.owner_binding!==intent.project||job.project!==intent.project||job.kind!=='configure'||!['pending','running','succeeded','failed','canceled'].includes(String(job.state))||response.expired)throw new ServiceError('crossed_policy_resume_receipt',true,response);
        if(receipt.result!==null){const result=record(receipt.result);if(result.operation_id!==request.operation_id)throw new ServiceError('crossed_policy_resume_result',true,response);catalogHex(result.policy_digest,64);}
        return true;
    }
    async preparePolicyResume(index:string,operation:string,project:string):Promise<PolicyResumeIntent>{
        return validatePolicyResumeIntent({catalog_policy_resume:{index_id:index,operation_id:operation,caller:this.caller,key:await this.key()},project});
    }
    async submitPolicyResume(input:PolicyResumeIntent):Promise<RecordValue>{
        const intent=validatePolicyResumeIntent(structuredClone(input)),request=intent.catalog_policy_resume;
        const response=await this.mutationReply({catalog_policy_resume:request},'catalog_policy_resumed');
        validateNativeReply(response,()=>this.validatePolicyResumeObservation(response,intent));return response;
    }
    validatePolicyResumeObservation(response:RecordValue,input:PolicyResumeIntent):void{
        const intent=validatePolicyResumeIntent(structuredClone(input)),request=intent.catalog_policy_resume;
        validatePolicyStatus(response,request.index_id,'catalog_policy_resumed');
        const snapshot=record(response.snapshot);
        if(snapshot.operation_id!==request.operation_id||record(snapshot.policy).owner_binding!==intent.project)throw new ServiceError('crossed_policy_resume',true,response);
    }
    async catalogRetirementEvidence(index:string,namespace:string,backing:string,minimumRevision:number,registration?:CatalogObject):Promise<RecordValue>{
        if(!/^idx_[0-9a-f]{32}$/.test(index))throw new ServiceError('invalid_retirement_index');catalogHex(namespace);catalogHex(backing,64);catalogRevision(minimumRevision);
        const response=await this.rpc({catalog_index_retirement_evidence:{index_id:index}},'catalog_index_retirement_evidence');
        this.validateRetirementEvidence(response,index,namespace,backing,minimumRevision,registration);return response;
    }
    validateRetirementEvidence(response:RecordValue,index:string,namespace:string,backing:string,minimumRevision:number,registration?:CatalogObject):boolean{
        if(!/^idx_[0-9a-f]{32}$/.test(index))throw new ServiceError('invalid_retirement_index');catalogHex(namespace);catalogHex(backing,64);catalogRevision(minimumRevision);catalogBound(response,65536);
        if(response.status!=='catalog_index_retirement_evidence'||response.index_id!==index)throw new ServiceError('crossed_retirement_evidence',true,response);
        if(response.evidence===null)return false;
        catalogBound(response.evidence,32768);
        const evidence=record(response.evidence),clock=record(evidence.catalog_clock),definition=record(evidence.index),full=record(evidence.retirement),request=validateNativeRetirementRequest(full.request),key=request.key,operation=request.operation,metadata=record(full.metadata);
        catalogRevision(definition.definition_revision);catalogRevision(definition.source_revision);
        if(registration&&!sameCatalogValue(definition.backing,registration))throw new ServiceError('crossed_retirement_backing',true,response);
        if(operation.operation!=='retire_index'||operation.id!==index||key.namespace!==namespace||definition.id!==index||definition.tombstone!==true||record(definition.backing).binding!==backing
            ||definition.definition_revision!==Number(operation.expected_definition_revision)+1||definition.source_revision!==Number(operation.expected_source_revision)+1
            ||clock.namespace!==namespace||catalogRevision(clock.revision)<minimumRevision||catalogRevision(metadata.catalog_revision)<=minimumRevision||Number(clock.revision)<Number(metadata.catalog_revision))throw new ServiceError('crossed_retirement_lineage',true,response);
        catalogBound(full,CATALOG_RECEIPT_BYTES);catalogHex(full.preview_digest,64);catalogHex(metadata.request_digest,64);
        this.validateCatalogReceipt({receipt:metadata,fenced_receipt:full},request,String(metadata.request_digest),String(full.preview_digest));
        const obligation=record(record(full.proof).worker_obligation);
        if(obligation.index_id!==index||obligation.backing!==backing)throw new ServiceError('crossed_retirement_workers',true,response);
        return true;
    }
    validateSourceObservation(response:RecordValue,beginOperation:string,namespace:string,prior?:RecordValue):boolean{
        catalogHex(beginOperation);catalogHex(namespace);catalogBound(response,65536);
        if(!['catalog_source_change_observation','catalog_source_change_resolved','source_change_resolution_outcome_unknown','catalog_pending_source_change'].includes(String(response.status)))throw new ServiceError('invalid_source_observation_status',true,response);
        if(response.status==='source_change_resolution_outcome_unknown'&&(response.begin_operation!==beginOperation||response.code!=='source_change_resolution_outcome_unknown'))throw new ServiceError('crossed_source_resolution_unknown',true,response);
        const observed=sourceChangeObservation(response),before=prior?sourceChangeObservation(prior):undefined;
        if(!observed){if(response.status!=='source_change_resolution_outcome_unknown')throw new ServiceError('source_observation_unavailable',true,response);return false;}
        catalogBound(observed);const begin=record(observed.begin),metadata=record(begin.metadata);
        const intent=validateCatalogApply({catalog_apply:{request:begin.request,preview_digest:begin.preview_digest},request_digest:metadata.request_digest}),request=intent.catalog_apply.request;
        if(request.key.namespace!==namespace||request.operation.operation!=='begin_index_source_change'||metadata.operation_id!==beginOperation)throw new ServiceError('crossed_native_source_begin',true,response);
        this.validateCatalogReceipt({receipt:metadata,fenced_receipt:begin},request,intent.request_digest,intent.catalog_apply.preview_digest);
        if(before&&(!sameCatalogValue(before.begin,begin)||before.completion!=null&&!sameCatalogValue(before.completion,observed.completion)))throw new ServiceError('crossed_source_observation_continuity',true,response);
        const plan=record(request.operation.plan),registration=record(plan.registration);
        if(observed.job!==null){const job=record(observed.job);if(job.id!==plan.job||job.project!==registration.binding||job.kind!=='configure'||!['pending','running','succeeded','failed','canceled'].includes(String(job.state)))throw new ServiceError('crossed_source_job',true,response);count(job.fencing_generation);count(job.attempts);}
        if(observed.completion!==null){
            const completion=record(observed.completion),key=record(completion.key);catalogBound(completion,CATALOG_RECEIPT_BYTES);identifier(completion.operation_id,16);catalogHex(completion.request_digest,64);identifier(key.nonce,16);count(key.epoch);
            if(completion.schema!==1||completion.caller!==request.caller||key.namespace!==namespace||completion.authority_fence_required!==false||catalogRevision(completion.catalog_revision)<=Number(metadata.catalog_revision))throw new ServiceError('crossed_source_completion',true,response);
        }
        return observed.completion!==null;
    }
    async submitSourceResolve(intent:SourceResolveIntent,prior:RecordValue):Promise<RecordValue>{
        intent=validateSourceResolveIntent(structuredClone(intent));const response=await this.mutationReply({catalog_source_change_resolve:intent.catalog_source_change_resolve});
        validateNativeReply(response,()=>this.validateSourceObservation(response,intent.catalog_source_change_resolve.begin_operation,intent.namespace,prior));
        if(response.status==='source_change_resolution_outcome_unknown')throw new ServiceError('source_change_resolution_outcome_unknown',true,response);
        if(response.status!=='catalog_source_change_resolved')throw retainUnvalidatedNativeReply(new ServiceError('unexpected_source_resolution_response',true),response);
        return response;
    }
    async catalogIndexSetupDraft(selection:CatalogObject):Promise<IndexSetupReview>{
        selection=structuredClone(selection);const {key,expected_catalog_revision,...choices}=selection;
        validateIndexSetupSelection(choices);const requestKey=record(key);identifier(requestKey.namespace,16);identifier(requestKey.nonce,16);count(requestKey.epoch);catalogRevision(expected_catalog_revision);
        const response=await this.rpc({catalog_index_setup_draft:{selection}},'catalog_index_setup_draft');
        return validateNativeIndexSetupDraft(response,selection);
    }
    async submitIndexSetup(intent:IndexSetupApplyIntent):Promise<RecordValue>{
        intent=validateIndexSetupApply(structuredClone(intent));const draft=intent.catalog_index_setup_apply.draft;
        const response=await this.mutationReply({catalog_index_setup_apply:{draft}});
        validateNativeReply(response,()=>this.validateIndexSetupObservation(response,draft,intent.request_digest));
        if(response.status==='catalog_outcome_unknown')throw new ServiceError(String(response.code),true,response);
        if(response.status!=='catalog_index_setup_applied')throw retainUnvalidatedNativeReply(new ServiceError('unexpected_setup_response',true),response);
        return response;
    }
    validateIndexSetupObservation(response:RecordValue,draft:CatalogObject,requestDigest:string,prior?:RecordValue):boolean{
        catalogHex(requestDigest,64);
        catalogBound(response,65536);const request=record(draft.request),receipt=indexSetupObservation(response),previous=prior?indexSetupObservation(prior):undefined;
        if(response.status==='catalog_outcome_unknown'){
            if(typeof response.code!=='string'||!/^[a-z0-9_]{1,128}$/.test(response.code)||response.caller!==request.caller||!sameCatalogValue(response.key,request.key)||response.request_digest!==requestDigest)throw new ServiceError('crossed_setup_outcome_unknown',true,response);
            catalogHex(response.request_digest,64);
            if(response.observation!=null&&record(response.observation).kind!=='index_setup')throw new ServiceError('crossed_setup_outcome_observation',true,response);
        }else if(!['catalog_index_setup_applied','catalog_index_setup_receipt'].includes(String(response.status)))throw new ServiceError('unexpected_setup_response',true,response);
        if(!receipt){if(response.status==='catalog_index_setup_applied'||response.status==='catalog_index_setup_receipt'&&response.receipt!==null)throw new ServiceError('incomplete_setup_receipt',true,response);return false;}
        const validated=validateIndexSetupReceipt(receipt,draft,previous);
        if(validated.request_digest!==requestDigest)throw new ServiceError('crossed_setup_receipt_digest',true,response);
        if(response.status==='catalog_outcome_unknown'&&response.request_digest!==validated.request_digest)throw new ServiceError('crossed_setup_outcome_digest',true,response);
        return validated.stage==='ready'&&response.status!=='catalog_outcome_unknown';
    }
    validateCatalogReceipt(response: RecordValue, request: CatalogRequest, requestDigest: string,previewDigest:string): void {
        catalogBound(response,65536);const receipt=this.validateCatalogMetadata(response.receipt,request,requestDigest);
        const observation=catalogFenceObservation(response);
        const full=response.fenced_receipt??(observation?.state==='applied'?observation.value:undefined);
        if(observation&&observation.state!=='applied')throw new ServiceError('incomplete_catalog_fence',true);
        if(receipt.authority_fence_required===true||full!==undefined){
            if(!full)throw new ServiceError('complete_catalog_fence_required',true);
            this.validateFencedReceipt(full,receipt,request,previewDigest);
        }
    }
    private validateCatalogMetadata(input:unknown,request:CatalogRequest,requestDigest:string):RecordValue{
        catalogBound(input,CATALOG_RECEIPT_BYTES);const receipt=catalogObject(input),key=catalogObject(receipt.key);
        catalogHex(receipt.operation_id); catalogHex(receipt.request_digest, 64);catalogHex(requestDigest,64);
        if (receipt.caller !== request.caller || key.namespace !== request.key.namespace || key.epoch !== request.key.epoch || key.nonce !== request.key.nonce
            || receipt.schema !== 1 || !Number.isSafeInteger(receipt.catalog_revision) || receipt.catalog_revision !== request.expected_catalog_revision+1 || receipt.request_digest!==requestDigest||typeof receipt.authority_fence_required!=='boolean') {
            throw new ServiceError('crossed_catalog_receipt', true);
        }
        if(['delete_project','set_membership','remove_membership','revoke_grant','begin_index_source_change','retire_index','change_index_policy'].includes(String(request.operation.operation))&&receipt.authority_fence_required!==true)throw new ServiceError('complete_catalog_fence_required',true);
        return receipt;
    }
    private validateFencedReceipt(input:unknown,receipt:RecordValue,request:CatalogRequest,previewDigest:string):void{
        const fenced=record(input);catalogBound(fenced,CATALOG_RECEIPT_BYTES);
        if(receipt.authority_fence_required!==true||!sameCatalogValue(fenced.request,request)||!sameCatalogValue(fenced.metadata,receipt)||fenced.preview_digest!==previewDigest)throw new ServiceError('crossed_catalog_fenced_receipt',true);
        catalogHex(fenced.prepared_digest,64);this.validateFenceProof(fenced.proof,request);
    }
    validateCatalogUnknown(response:RecordValue,request:CatalogRequest,requestDigest:string,previewDigest:string):void{
        catalogBound(response,65536);const key=record(response.key);
        if(typeof response.code!=='string'||!/^[a-z0-9_]{1,128}$/.test(response.code)||response.caller!==request.caller||!sameCatalogValue(key,request.key)||response.request_digest!==requestDigest)throw new ServiceError('crossed_catalog_outcome_unknown',true);
        if(response.observation==null)return;
        const observation=record(response.observation);if(observation.kind!=='authority_fence')throw new ServiceError('crossed_catalog_outcome_observation',true);
        const fence=catalogFenceObservation(response);if(!fence)throw new ServiceError('invalid_catalog_outcome_observation',true);
        if(fence.state==='pending')this.validatePendingFence(fence.value,request,previewDigest);
        else if(fence.state==='applied'){
            const full=record(fence.value),metadata=this.validateCatalogMetadata(full.metadata,request,requestDigest);this.validateFencedReceipt(full,metadata,request,previewDigest);
        }else throw new ServiceError('invalid_catalog_outcome_observation',true);
    }
    validateCatalogObservation(response:RecordValue,request:CatalogRequest,requestDigest:string,previewDigest:string):boolean{
        if(response.receipt!==null){this.validateCatalogReceipt(response,request,requestDigest,previewDigest);return true;}
        catalogBound(response,65536);
        const observation=catalogFenceObservation(response);if(!observation)return false;
        if(observation.state!=='pending')throw new ServiceError('crossed_pending_catalog_fence',true);
        this.validatePendingFence(observation.value,request,previewDigest);return false;
    }
    validateCatalogUnprepared(response:RecordValue,request:CatalogRequest,requestDigest:string):void{
        catalogBound(response,65536);catalogBound(response.request);catalogHex(response.request_digest,64);const clock=record(response.catalog_clock);
        if(response.status!=='catalog_receipt_unprepared'||!sameCatalogValue(response.request,request)||response.request_digest!==requestDigest||clock.namespace!==request.key.namespace||catalogRevision(clock.revision)<request.expected_catalog_revision)throw new ServiceError('crossed_catalog_unprepared',true,response);
    }
    private validatePendingFence(input:unknown,request:CatalogRequest,previewDigest:string):void{
        const intent=record(input);catalogBound(intent,CATALOG_RECEIPT_BYTES);
        if(intent.schema!==1||!sameCatalogValue(intent.request,request)||intent.preview_digest!==previewDigest||!['prepared','fence_unknown','fenced'].includes(String(intent.stage)))throw new ServiceError('crossed_pending_catalog_fence',true);
        identifier(intent.operation_id,16);catalogHex(intent.prepared_digest,64);
        const plan=record(intent.plan);identifier(plan.service,16);if(count(plan.gate_generation)<1||!Array.isArray(plan.targets)||plan.targets.length>128)throw new ServiceError('invalid_catalog_fence_plan',true);
        const targets=plan.targets;
        targets.forEach((target,index)=>{this.validateFenceTarget(target,String(plan.service));if(targets.slice(0,index).some((prior:unknown)=>sameCatalogValue(prior,target)))throw new ServiceError('invalid_catalog_fence_plan',true);});
        this.validateFenceConfirmations(intent.confirmations,String(plan.service),targets);
        const workers=this.validateWorkerObligation(plan.worker_obligation,String(plan.service),request),workerConfirmed=this.validateWorkerConfirmations(intent.worker_confirmations??[],workers,String(plan.service));
        const complete=(intent.confirmations as unknown[]).length===targets.length&&workerConfirmed===workers.length;
        if(intent.stage==='prepared'&&((intent.confirmations as unknown[]).length!==0||workerConfirmed!==0)||intent.stage==='fenced'&&!complete||intent.stage==='fence_unknown'&&complete)throw new ServiceError('incomplete_catalog_fence',true);
    }
    validateCatalogFenceContinuity(previous:RecordValue,next:RecordValue):void{
        const oldFence=catalogFenceObservation(previous),nextFence=catalogFenceObservation(next);
        const old=previous.fenced_receipt??oldFence?.value,current=next.fenced_receipt??nextFence?.value;
        if(!old||!current)return;
        const before=record(old),after=record(current);
        if(!sameCatalogValue(before.request,after.request)||before.preview_digest!==after.preview_digest||before.prepared_digest!==after.prepared_digest)throw new ServiceError('crossed_catalog_fence_identity',true);
        const oldOperation=before.operation_id??record(before.metadata??{}).operation_id,newOperation=after.operation_id??record(after.metadata??{}).operation_id;
        if(oldOperation!==newOperation)throw new ServiceError('crossed_catalog_fence_identity',true);
        const wasApplied=previous.fenced_receipt!=null||oldFence?.state==='applied',applied=next.fenced_receipt!=null||nextFence?.state==='applied';
        if(wasApplied&&!applied)throw new ServiceError('catalog_fence_observation_regressed',true);
        if(wasApplied){if(!sameCatalogValue(before.metadata,after.metadata)||!sameCatalogValue(before.proof,after.proof))throw new ServiceError('crossed_catalog_fenced_receipt',true);}
        else if(applied){
            const plan=record(before.plan),proof=record(after.proof),confirmations=proof.confirmations;
            if(!Array.isArray(confirmations)||proof.service!==plan.service||proof.gate_generation!==plan.gate_generation||!sameCatalogValue(confirmations.map(value=>record(value).target),plan.targets)||!sameCatalogValue(proof.worker_obligation??null,plan.worker_obligation??null))throw new ServiceError('crossed_catalog_fence_plan',true);
        }else if(!sameCatalogValue(before.plan,after.plan))throw new ServiceError('crossed_catalog_fence_plan',true);
    }
    private validateFenceTarget(input:unknown,service:string):void{
        const target=record(input),worker=record(target.worker),scope=record(target.scope);
        if(identifier(worker.service,16)!==service||count(worker.fencing_generation)<1||scope.schema!==1||count(scope.lease_generation)<1)throw new ServiceError('invalid_catalog_fence_target',true);
        identifier(worker.worker,16);identifier(worker.project,32);catalogHex(scope.fingerprint,64);catalogRevision(scope.grant_revision);
        if(typeof scope.index_id!=='string'||!/^idx_[0-9a-f]{32}$/.test(scope.index_id)||typeof scope.grant_id!=='string'||!/^grt_[0-9a-f]{32}$/.test(scope.grant_id))throw new ServiceError('invalid_catalog_fence_target',true);
    }
    private validateFenceConfirmations(input:unknown,service:string,targets?:unknown[]):void{
        if(!Array.isArray(input)||input.length>128||targets&&input.length>targets.length)throw new ServiceError('invalid_catalog_fence_confirmations',true);
        const seen:unknown[]=[];
        for(const item of input){const confirmation=record(item);if(!['native_scope_ack','native_worker_reap'].includes(String(confirmation.method)))throw new ServiceError('invalid_catalog_fence_method',true);
            this.validateFenceTarget(confirmation.target,service);
            if(seen.some(target=>sameCatalogValue(target,confirmation.target))||targets&&!targets.some(target=>sameCatalogValue(target,confirmation.target)))throw new ServiceError('crossed_catalog_fence_target',true);
            seen.push(confirmation.target);
        }
    }
    private validateWorkerIdentity(input:unknown,service:string,backing?:string):RecordValue{
        const worker=record(input);if(identifier(worker.service,16)!==service||count(worker.fencing_generation)<1||backing!==undefined&&worker.project!==backing)throw new ServiceError('invalid_catalog_worker_identity',true);
        identifier(worker.worker,16);identifier(worker.project,32);return worker;
    }
    private validateWorkerObligation(input:unknown,service:string,request:CatalogRequest):RecordValue[]{
        if(input==null){if(['retire_index','change_index_policy'].includes(String(request.operation.operation)))throw new ServiceError('native_retirement_worker_obligation_required',true);return [];}
        if(!['retire_index','change_index_policy'].includes(String(request.operation.operation)))throw new ServiceError('unexpected_catalog_worker_obligation',true);
        const obligation=record(input),backing=identifier(obligation.backing,32);
        if(obligation.index_id!==request.operation.id||!Array.isArray(obligation.workers)||obligation.workers.length>128)throw new ServiceError('crossed_catalog_worker_obligation',true);
        const workers=obligation.workers.map(input=>this.validateWorkerIdentity(input,service,backing));
        for(let i=0;i<workers.length;i++)if(i&&JSON.stringify(workers[i-1])>=JSON.stringify(workers[i]))throw new ServiceError('invalid_catalog_worker_census',true);
        const providers=obligation.providers??[];
        if(!Array.isArray(providers)||providers.length!==workers.length)throw new ServiceError('incomplete_catalog_provider_census',true);
        providers.forEach((input,index)=>{const provider=record(input),history=record(provider.history);
            if(!sameCatalogValue(provider.worker,workers[index]))throw new ServiceError('crossed_catalog_provider_identity',true);
            if(history.kind==='helpers'||history.kind==='in_process'){
                if(history.kind==='in_process'&&history.completed_authorizations!=null){
                    const completed=record(history.completed_authorizations);
                    if(Buffer.byteLength(JSON.stringify(completed),'utf8')>2048||completed.schema!==1||identifier(completed.service_instance,16)!==service||identifier(completed.namespace,16)!==request.key.namespace||count(completed.completed_owners)<1)throw new ServiceError('invalid_catalog_completion_ledger',true);
                    identifier(completed.factory_generation,16);catalogHex(completed.approved_execution_sha256,64);catalogHex(completed.head_sha256,64);count(completed.admitted_high_water);
                }
                if(!Array.isArray(history.generations)||(!history.generations.length&&(history.kind==='helpers'||history.completed_authorizations==null))||history.generations.length>32)throw new ServiceError('invalid_catalog_provider_generations',true);
                history.generations.forEach((generation,index,all)=>{identifier(generation,16);if(index&&String(all[index-1])>=String(generation))throw new ServiceError('invalid_catalog_provider_generations',true);});
            }else if(history.kind!=='native_never_authorized')throw new ServiceError('invalid_catalog_provider_history',true);
        });return workers;
    }
    private validateWorkerConfirmations(input:unknown,workers:RecordValue[],service:string):number{
        if(!Array.isArray(input)||input.length>workers.length)throw new ServiceError('invalid_catalog_worker_confirmations',true);
        const confirmed:RecordValue[]=[];
        for(const item of input){const confirmation=record(item),worker=this.validateWorkerIdentity(confirmation.worker,service);
            if(confirmation.method!=='native_worker_reap'||!workers.some(expected=>sameCatalogValue(expected,worker))||confirmed.some(prior=>sameCatalogValue(prior,worker))||confirmed.length&&JSON.stringify(confirmed[confirmed.length-1])>=JSON.stringify(worker))throw new ServiceError('crossed_catalog_worker_confirmation',true);
            confirmed.push(worker);
        }return confirmed.length;
    }
    private validateFenceProof(input:unknown,request:CatalogRequest):void{
        const proof=record(input);catalogHex(proof.plan_digest,64);const service=identifier(proof.service,16);
        if(count(proof.gate_generation)<1)throw new ServiceError('invalid_catalog_fence_generation',true);
        this.validateFenceConfirmations(proof.confirmations,service);
        const workers=this.validateWorkerObligation(proof.worker_obligation,service,request);
        if(this.validateWorkerConfirmations(proof.worker_confirmations??[],workers,service)!==workers.length)throw new ServiceError('incomplete_catalog_worker_fence',true);
    }
    async prepareJob(project: string, kind: JobKind): Promise<JobIntent> {
        return { project: identifier(project,32), caller: this.caller, key: await this.key(), kind };
    }
    // Preserve this exact intent before sending. An uncertain result must be checked
    // or retried with the same key; never manufacture a fresh mutation automatically.
    async submitJob(intent: JobIntent): Promise<RecordValue> { return this.submitMutation({ enqueue: intent }); }
    async prepareRetry(id: string): Promise<MutationIntent> {
        return { retry: { id: identifier(id,32), caller: this.caller, key: await this.key() } };
    }
    async prepareReferenceRefresh(project:string,draft:ProjectConfigurationRequest,originalQuery:RecordValue,originalResult:RecordValue,persistEvidence=false):Promise<ReferenceRefreshIntent>{
        const key=await this.key(),selection=structuredClone(record(draft.configuration.selection));
        selection.caller=this.caller;selection.key=key;selection.original_query_digest=refreshDigest(originalQuery);selection.original_result_digest=refreshDigest(originalResult);selection.original_evidence_selection_digest=refreshDigest(selection.members);
        const request={...structuredClone(draft),configuration:{operation:'refresh_references',selection}},intent={reference_refresh:{configure_project:{project:identifier(project,32),caller:this.caller,key,request},original_query:structuredClone(originalQuery),original_result:structuredClone(originalResult),evidence_retention:persistEvidence?'local_approved':'memory_only'}};
        delete (request as ProjectConfigurationRequest).prepare;
        return validateRefreshIntent(intent);
    }
    async submitReferenceRefresh(input:ReferenceRefreshIntent):Promise<RecordValue>{
        const intent=validateRefreshIntent(input),response=await this.mutationReply({configure_project:intent.reference_refresh.configure_project});
        try{if(response.status==='reference_refresh_review_required')validateRefreshRefusal(response,intent);else {if(response.status!=='job')throw new Error('invalid_refresh_ack_status');validateRefreshJob(response.job,intent);}return response;}catch(cause){const failure=new ServiceError('crossed_reference_refresh_acknowledgment',true,response);failure.cause=cause;throw retainUnvalidatedNativeReply(failure,response);}
    }
    async referenceRefreshReceipt(input:ReferenceRefreshIntent):Promise<RecordValue>{
        const intent=validateRefreshIntent(input),request=intent.reference_refresh.configure_project;
        const response=await this.rpc({reference_refresh_receipt:{caller:request.caller,key:request.key}},'reference_refresh_receipt');
        try{validateRefreshReceipt(response,intent);return response;}catch(cause){const failure=new ServiceError('unqualified_reference_refresh_receipt',true,response);failure.cause=cause;throw retainUnvalidatedNativeReply(failure,response);}
    }
    async prepareConfiguration(project:string,request:ProjectConfigurationRequest):Promise<MutationIntent>{
        if(request.configuration.operation==='refresh_references')throw new ServiceError('refresh_requires_original_evidence_review');
        validateConfigurationRequest(request);
        return {configure_project:{project:identifier(project,32),caller:this.caller,key:await this.key(),request:structuredClone(request)}};
    }
    async preparePreference(project:string,configuration:RecordValue):Promise<MutationIntent>{
        const key=await this.key(),request={incarnation:key.namespace,settings:identifier(project,32),configuration,prepare:true};
        validateConfigurationRequest(request);
        return {configure_project:{project,caller:this.caller,key,request}};
    }
    async preparePlan(selection: 'all' | string[], operation: RepairOperation): Promise<MutationIntent> {
        if (selection !== 'all') {
            if (!selection.length || selection.length > 256 || new Set(selection).size !== selection.length) { throw new ServiceError('invalid_repair_selection'); }
            selection.forEach(value => identifier(value,32));
        }
        return { plan_create: { caller: this.caller, key: await this.key(), request: {
            selection: selection === 'all' ? 'all' : { selected: [...selection] }, operation } } };
    }
    async submitMutation(intent: MutationIntent): Promise<RecordValue> {
        validateMutationIntent(intent);
        if('configure_project'in intent&&intent.configure_project.request.configuration.operation==='refresh_references')throw new ServiceError('refresh_requires_exact_receipt_tracking');
        // The host persists the complete prepared intent before this call and
        // retires it only after receiving a complete, matching native result.
        const response = await this.mutationReply(intent, 'plan_create' in intent ? 'plan' : 'job');
        return validateNativeReply(response,()=>{
            if ('enqueue' in intent && record(response.job).project !== intent.enqueue.project) { throw new ServiceError('crossed_job_receipt',true); }
            if ('configure_project' in intent && record(response.job).project !== intent.configure_project.project) { throw new ServiceError('crossed_job_receipt',true); }
            identifier(record('plan_create' in intent ? response.plan : response.job).id,32);
            return response;
        });
    }
    dispose(): void { this.transport.dispose(); }
}

/** Canonical synchronous validation only: no client constructor, transport,
 * native execution, RPC or mutable client identity is created by this facade. */
const validatorNames=[
    'validateCatalogObservation','validateCatalogFenceContinuity','validateIndexSetupObservation','validateSourceObservation',
    'validatePolicyResumeReceipt','validatePolicyResumeObservation','validateSplitRecoveryResponse','validateCatalogReceipt',
    'validateCatalogMetadata','validateCatalogUnknown','validateFencedReceipt','validateFenceProof','validatePendingFence','validateFenceTarget',
    'validateFenceConfirmations','validateWorkerIdentity','validateWorkerObligation','validateWorkerConfirmations',
] as const;
type PublicObservationValidation=Pick<ServiceClient,'validateCatalogObservation'|'validateCatalogUnknown'|'validateCatalogFenceContinuity'|'validateIndexSetupObservation'|'validateSourceObservation'|'validatePolicyResumeReceipt'|'validatePolicyResumeObservation'|'validateSplitRecoveryResponse'>;
const validatorPrototype=ServiceClient.prototype as unknown as Record<string,unknown>;
const observationValidation=Object.freeze(Object.assign(Object.create(null),Object.fromEntries(validatorNames.map(name=>[name,validatorPrototype[name]])))) as PublicObservationValidation;
export const intentObservationValidators=Object.freeze({
    catalog:(...args:Parameters<ServiceClient['validateCatalogObservation']>)=>{
        if(args[0].status==='catalog_outcome_unknown'){observationValidation.validateCatalogUnknown(...args);return false;}
        return observationValidation.validateCatalogObservation(...args);
    },
    fenceContinuity:(...args:Parameters<ServiceClient['validateCatalogFenceContinuity']>)=>observationValidation.validateCatalogFenceContinuity(...args),
    setup:(...args:Parameters<ServiceClient['validateIndexSetupObservation']>)=>observationValidation.validateIndexSetupObservation(...args),
    source:(...args:Parameters<ServiceClient['validateSourceObservation']>)=>observationValidation.validateSourceObservation(...args),
    policyReceipt:(...args:Parameters<ServiceClient['validatePolicyResumeReceipt']>)=>observationValidation.validatePolicyResumeReceipt(...args),
    policyResumed:(...args:Parameters<ServiceClient['validatePolicyResumeObservation']>)=>observationValidation.validatePolicyResumeObservation(...args),
    splitRecovery:(...args:Parameters<ServiceClient['validateSplitRecoveryResponse']>)=>observationValidation.validateSplitRecoveryResponse(...args),
});
