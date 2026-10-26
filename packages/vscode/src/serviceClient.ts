// SPDX-License-Identifier: Apache-2.0
import { ChildProcessWithoutNullStreams, spawn } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import * as path from 'node:path';

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
export class ServiceError extends Error {
    constructor(readonly code: string, readonly outcomeUnknown = false) { super(`AIRK service: ${code}`); }
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
export interface ProjectConfigurationRequest { incarnation:string;settings:string;configuration:RecordValue }
export interface ConfigurationIntent {project:string;caller:string;key:RequestKey;request:ProjectConfigurationRequest}
export type MutationIntent = { enqueue: JobIntent } | { retry: RetryIntent } | { plan_create: PlanIntent } | {configure_project:ConfigurationIntent};
export function validateConfigurationRequest(input:unknown):ProjectConfigurationRequest {
    const value=record(input),configuration=record(value.configuration),operation=configuration.operation;
    if(Object.keys(value).sort().join(',')!=='configuration,incarnation,settings'||Buffer.byteLength(JSON.stringify(value))>16384)throw new ServiceError('invalid_project_configuration');
    identifier(value.incarnation,16);identifier(value.settings,32);
    const names:Record<string,string[]>={unregister:[],prepare_derived_cleanup:[],delete_derived:['preview'],bundled_model:[],vectors:['enabled'],detail:['level'],vector_selection:['policy'],
        add_root:['alias','path'],remove_root:['alias'],install_model:['directory','manifest_sha256'],usage:['mode'],
        clear_usage:[],reset_scan_limits:[],usage_feedback:['request_id','usefulness'],import_usage:['input'],extraction_policy:['input'],scan_limits:['input'],quota_policy:['input','expected_policy_digest'],
        calibrate:['tokenizer','baseline','returned','generation','content_kind','record']};
    if(typeof operation!=='string'||!Object.hasOwn(names,operation)||Object.keys(configuration).sort().join(',')!==['operation',...names[operation]].sort().join(','))throw new ServiceError('invalid_project_configuration');
    const filePath=(input:unknown)=>{if(typeof input!=='string'||!path.isAbsolute(input)||input.length>4096||/[\u0000-\u001f\u007f]/.test(input))throw new ServiceError('invalid_configuration_path');};
    if(operation==='vectors'&&typeof configuration.enabled!=='boolean')throw new ServiceError('invalid_project_configuration');
    if(operation==='detail'&&!['essential','agent_lean','balanced','detailed'].includes(String(configuration.level)))throw new ServiceError('invalid_project_configuration');
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
        const response = await this.transport.request(method);
        if (response.status === 'failure') {
            const code = typeof response.code === 'string' && /^[a-z0-9_]{1,128}$/.test(response.code) ? response.code : 'invalid_service_failure';
            throw new ServiceError(code, code.endsWith('_outcome_unknown') || code === 'registry_response_unknown_retry_with_key');
        }
        if (expected && response.status !== expected) { throw new ServiceError('unexpected_service_response', true); }
        return response;
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
    async prepareJob(project: string, kind: JobKind): Promise<JobIntent> {
        return { project: identifier(project,32), caller: this.caller, key: await this.key(), kind };
    }
    // Preserve this exact intent before sending. An uncertain result must be checked
    // or retried with the same key; never manufacture a fresh mutation automatically.
    async submitJob(intent: JobIntent): Promise<RecordValue> { return this.submitMutation({ enqueue: intent }); }
    async prepareRetry(id: string): Promise<MutationIntent> {
        return { retry: { id: identifier(id,32), caller: this.caller, key: await this.key() } };
    }
    async prepareConfiguration(project:string,request:ProjectConfigurationRequest):Promise<MutationIntent>{
        validateConfigurationRequest(request);
        return {configure_project:{project:identifier(project,32),caller:this.caller,key:await this.key(),request:structuredClone(request)}};
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
        // The host persists the complete prepared intent before this call and
        // retires it only after receiving a complete, matching native result.
        const response = await this.rpc(intent, 'plan_create' in intent ? 'plan' : 'job');
        if ('enqueue' in intent && record(response.job).project !== intent.enqueue.project) { throw new ServiceError('crossed_job_receipt',true); }
        if ('configure_project' in intent && record(response.job).project !== intent.configure_project.project) { throw new ServiceError('crossed_job_receipt',true); }
        identifier(record('plan_create' in intent ? response.plan : response.job).id,32);
        return response;
    }
    dispose(): void { this.transport.dispose(); }
}
