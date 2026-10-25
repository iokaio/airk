// SPDX-License-Identifier: Apache-2.0
import { createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { runText } from './native';
import { NativeControlTransport, ServiceClient, ServiceError, record, RecordValue, identifier, count, ProjectConfigurationRequest, validateConfigurationRequest } from './serviceClient';
import type { LifecycleIntent } from './serviceIntents';
export type RuntimeChannel = 'stable' | 'preview';
export interface RuntimeOptions {
    channel: RuntimeChannel; installDir?: string;
    managed?: { source: string; approvedSha256: string; lifecycle: 'external' | 'user-on-demand'; stateDir?: string };
}
export type NativeRunner = (command: string, args: string[], timeout?: number,input?:string) => Promise<RecordValue>;
export const nativeRunner: NativeRunner = async (command,args,timeout=60000,input) => {
    const text=await runText(command,args,path.dirname(command),timeout,undefined,undefined,input);
    if(Buffer.byteLength(text)>2*1024*1024){throw new ServiceError('bootstrap_response_oversize');}
    return record(JSON.parse(text));
};
export interface VerifiedBundle { main: string; helper: string; source: string; digest: string; version: string; platform: string }
export interface RuntimeBinding {
    installDir: string; controlDir: string; executable: string; launcher: string | null;
    runtimeSha256: string; version: string; lifecycle: 'on-demand' | 'external' | 'user-on-demand';
    verificationBasis: string;
    payloadVerified?:boolean;
}
export interface BootstrapObservation {
    bundle: VerifiedBundle; installDir: string; location: RecordValue;
    installed: boolean; inspection?: RecordValue; diagnosis?: RecordValue; error?: string;
}
export interface RuntimeTransitionReview {
    readonly action:'upgrade'|'rollback'|'resume'; readonly installDir:string; readonly controlDir:string;
    readonly observedActive:string; readonly expectedActive:string; readonly target:string; readonly version:string;
}
export interface ProjectRegistrationReview {
    readonly installDir:string; readonly controlDir:string; readonly runtimeSha256:string;
    readonly binding:string; readonly root:string; readonly data_root:string; readonly collection:string;
}
export interface LegacyStartupTarget {readonly root:string;readonly data_dir:string;readonly collection:string;readonly executable:string}
export interface LegacyStartupReview {
    readonly installDir:string;readonly controlDir:string;readonly runtimeSha256:string;readonly target:LegacyStartupTarget;
    readonly value:RecordValue;readonly approval?:string;readonly phase?:string;
}
const legacyPhases=['planned','disabled','definition_removed','receipt_removed','complete'];
function legacyTarget(input:unknown):LegacyStartupTarget {
    const value=record(input),collection=value.collection;
    if(Object.keys(value).sort().join(',')!=='collection,data_dir,executable,root'||typeof collection!=='string'||!collection.length||collection.length>128||collection==='.'||collection==='..'||!/^[A-Za-z0-9._-]+$/.test(collection))throw new ServiceError('invalid_legacy_target');
    return {root:safePath(value.root),data_dir:safePath(value.data_dir),collection,executable:safePath(value.executable)};
}
function legacyArguments(target:LegacyStartupTarget):string[]{return ['--root',target.root,'--data-dir',target.data_dir,'--collection',target.collection,'--executable',target.executable];}
function projectRegistration(value:unknown):{binding:string;root:string;data_root:string;collection:string}{
    const registration=record(value),collection=registration.collection;
    if(typeof collection!=='string'||collection.length>128||!collection.length||collection==='.'||collection==='..'||!/^[A-Za-z0-9._-]+$/.test(collection))throw new ServiceError('invalid_project_registration');
    return {binding:identifier(registration.binding,32),root:safePath(registration.root),data_root:safePath(registration.data_root),collection};
}
function safePath(value: unknown): string {
    if(typeof value!=='string'||value.length>4096||!path.isAbsolute(value)||/[\u0000-\u001f\u007f]/.test(value)){throw new ServiceError('invalid_runtime_path');}
    return value;
}
function runtimeVersion(value:unknown):string {
    if(typeof value!=='string'||value.length>64||!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(value))throw new ServiceError('invalid_runtime_version');
    return value;
}
function relativeFile(value: string): string[] {
    const parts=value.split('/');
    if(!value||parts.some(part=>!part||part==='.'||part==='..'||!/^[A-Za-z0-9._+-]+$/.test(part))){throw new ServiceError('unsafe_runtime_payload_path');}
    return parts;
}
async function boundedJson(file: string,limit: number):Promise<{bytes:Buffer;value:RecordValue}> {
    const metadata=await fs.lstat(file);
    if(!metadata.isFile()||metadata.isSymbolicLink()||metadata.nlink!==1||metadata.size>limit){throw new ServiceError('unsafe_runtime_manifest');}
    const handle=await fs.open(file,'r');
    try {
        const opened=await handle.stat();
        if(!opened.isFile()||opened.nlink!==1||opened.size>limit){throw new ServiceError('unsafe_runtime_manifest');}
        const bytes=Buffer.alloc(limit+1);let length=0;
        while(length<bytes.length){const result=await handle.read(bytes,length,bytes.length-length,length);if(!result.bytesRead)break;length+=result.bytesRead;}
        const final=await handle.stat();
        if(length>limit||length!==opened.size||final.size!==length||final.nlink!==1){throw new ServiceError('runtime_manifest_changed');}
        const actual=bytes.subarray(0,length);
        return {bytes:actual,value:record(JSON.parse(actual.toString('utf8')))};
    }finally {await handle.close();}
}
async function fileDigest(file: string):Promise<string> {
    const before=await fs.lstat(file);
    if(!before.isFile()||before.isSymbolicLink()||before.nlink!==1||before.size>2*1024*1024*1024){throw new ServiceError('unsafe_runtime_payload_file');}
    const handle=await fs.open(file,'r');
    try {
        const initial=await handle.stat();
        if(!initial.isFile()||initial.nlink!==1||initial.size!==before.size||initial.dev!==before.dev||initial.ino!==before.ino){throw new ServiceError('runtime_payload_changed');}
        const digest=createHash('sha256'),block=Buffer.alloc(65536);let total=0;
        for(;;){const {bytesRead}=await handle.read(block,0,block.length,null);if(!bytesRead)break;total+=bytesRead;
            if(total>initial.size){throw new ServiceError('runtime_payload_changed');}digest.update(block.subarray(0,bytesRead));}
        const final=await handle.stat();
        if(total!==initial.size||final.size!==total||final.nlink!==1||initial.ino!==final.ino||initial.dev!==final.dev){throw new ServiceError('runtime_payload_changed');}
        return digest.digest('hex');
    }finally {await handle.close();}
}
/** The installed platform VSIX is the distribution trust source; workspace settings cannot approve it. */
export async function verifyBundledRuntime(extensionPath:string,version:string,
    host={platform:process.platform as string,arch:process.arch as string}):Promise<VerifiedBundle> {
    const platform=`${host.platform}-${host.arch}`;
    if(!['win32-x64','linux-x64','darwin-arm64','darwin-x64'].includes(platform)){throw new ServiceError('unsupported_runtime_host');}
    const native=path.join(extensionPath,'native');
    const wrapper=(await boundedJson(path.join(native,'manifest.json'),65536)).value;
    if(wrapper.schema!==2||wrapper.version!==version||wrapper.vsix_target!==platform||wrapper.runtime_dir!=='runtime'){
        throw new ServiceError('matching_service_vsix_required');
    }
    const source=path.join(native,'runtime'),root=await fs.lstat(source);
    if(!root.isDirectory()||root.isSymbolicLink()){throw new ServiceError('unsafe_runtime_payload_directory');}
    const inventory=await boundedJson(path.join(source,'airk-install.json'),512*1024);
    const digest=identifier(wrapper.install_manifest_sha256,32);
    if(createHash('sha256').update(inventory.bytes).digest('hex')!==digest){throw new ServiceError('runtime_inventory_digest_mismatch');}
    const os={win32:'windows',linux:'linux',darwin:'macos'}[host.platform];
    const arch={x64:'x86_64',arm64:'aarch64'}[host.arch];
    if(inventory.value.schema!==1||inventory.value.version!==version||inventory.value.os!==os||inventory.value.arch!==arch){throw new ServiceError('runtime_inventory_platform_mismatch');}
    const files=record(inventory.value.files),names=Object.keys(files);
    if(!names.length||names.length>2048){throw new ServiceError('runtime_inventory_file_bound');}
    const allowed=new Set(['airk-install.json']),directories=new Set<string>();
    for(const name of names){const parts=relativeFile(name);identifier(files[name],32);allowed.add(name);
        for(let end=1;end<parts.length;end++)directories.add(parts.slice(0,end).join('/'));
    }
    let nodes=0,totalBytes=0;
    async function inspect(directory:string,relative:string,depth:number):Promise<void> {
        if(depth>16){throw new ServiceError('runtime_payload_depth_bound');}
        const entries=await fs.opendir(directory);
        for await(const entry of entries){if(++nodes>4100){throw new ServiceError('runtime_payload_node_bound');}
            const name=relative?`${relative}/${entry.name}`:entry.name;relativeFile(name);
            const absolute=path.join(directory,entry.name),metadata=await fs.lstat(absolute);
            if(metadata.isSymbolicLink()){throw new ServiceError('redirected_runtime_payload');}
            if(metadata.isDirectory()){
                if(!directories.has(name)){throw new ServiceError('unknown_runtime_payload_directory');}
                await inspect(absolute,name,depth+1);
            }else {
                if(!allowed.has(name)||!metadata.isFile()||metadata.nlink!==1){throw new ServiceError('unknown_runtime_payload_file');}
                totalBytes+=metadata.size;
                if(totalBytes>4*1024*1024*1024){throw new ServiceError('runtime_payload_byte_bound');}
            }
        }
    }
    await inspect(source,'',0);
    for(const name of names){if(await fileDigest(path.join(source,...relativeFile(name)))!==files[name]){throw new ServiceError('runtime_payload_digest_mismatch');}}
    const main=host.platform==='win32'?'airk.exe':'airk',helper=host.platform==='win32'?'airk-embed.exe':'airk-embed';
    const payload=record(wrapper.payload);
    if(payload[main]!==files[main]||payload[helper]!==files[helper]||!files[main]||!files[helper]){throw new ServiceError('runtime_pair_missing');}
    return {main:path.join(source,main),helper:path.join(source,helper),source,digest,version,platform};
}
export class RuntimeHost {
    private bundle?:Promise<VerifiedBundle>;
    private preparing?:Promise<RuntimeBinding>;
    private readonly transitionReviews=new WeakSet<RuntimeTransitionReview>();
    private readonly projectReviews=new WeakSet<ProjectRegistrationReview>();
    private readonly legacyReviews=new WeakSet<LegacyStartupReview>();
    constructor(private readonly extensionPath:string,private readonly version:string,readonly options:RuntimeOptions,
        private readonly run:NativeRunner=nativeRunner, private readonly canContinue:()=>boolean=()=>true) {
        this.options=structuredClone(options);
        if(this.options.managed)Object.freeze(this.options.managed);Object.freeze(this.options);
        if(!['stable','preview'].includes(options.channel)){throw new ServiceError('invalid_runtime_channel');}
        if(options.installDir)safePath(options.installDir);
        if(options.managed){safePath(options.managed.source);identifier(options.managed.approvedSha256,32);
            if(!['external','user-on-demand'].includes(options.managed.lifecycle)){throw new ServiceError('invalid_managed_lifecycle');}
            if(options.managed.stateDir)safePath(options.managed.stateDir);}
    }
    private active():void {if(!this.canContinue())throw new ServiceError('runtime_client_no_longer_authorized');}
    private async invoke(command:string,args:string[],timeout?:number,input?:string):Promise<RecordValue>{
        this.active();const result=await this.run(command,args,timeout,input);this.active();return result;
    }
    private verified():Promise<VerifiedBundle>{
        this.bundle??=verifyBundledRuntime(this.extensionPath,this.version).catch(error=>{this.bundle=undefined;throw error;});return this.bundle;
    }
    async discover():Promise<BootstrapObservation>{
        const bundle=await this.verified();
        const self=await this.invoke(bundle.main,['runtime','self-test']);
        if(self.protocol!==1||self.version!==this.version||self.self_test!=='passed'||self.metadata_console!==1){throw new ServiceError('service_runtime_self_test_failed');}
        const location=await this.invoke(bundle.main,['runtime','location','--channel',this.options.channel]);
        if(location.protocol!==1){throw new ServiceError('runtime_locator_protocol_mismatch');}
        const installDir=this.options.installDir??safePath(location.install_dir);
        const observation:BootstrapObservation={bundle,installDir,location,installed:false};
        try {const metadata=await fs.lstat(installDir);if(!metadata.isDirectory()||metadata.isSymbolicLink()){throw new ServiceError('unsafe_runtime_install_location');}}
        catch(error){if((error as NodeJS.ErrnoException).code==='ENOENT')return observation;throw error;}
        observation.installed=true;
        try {observation.inspection=await this.invoke(bundle.main,['runtime','inspect','--install-dir',installDir]);}
        catch {observation.error='runtime_selection_unavailable';}
        if(!observation.inspection){
            try{const snapshot=await this.invoke(bundle.main,['runtime','snapshot','--install-dir',installDir]);
                if(snapshot.schema===1&&snapshot.metadata_only===true&&snapshot.payload_verified===false){observation.inspection=snapshot;observation.error='selected_runtime_payload_unverified';}}
            catch{/* Preserve unavailable state without nominating a different installation. */}
        }
        try {observation.diagnosis=await this.invoke(bundle.main,['runtime','diagnose','--install-dir',installDir]);}
        catch {observation.error??='runtime_diagnostics_unavailable';}
        return observation;
    }
    ensure(explicitStart=false):Promise<RuntimeBinding>{
        this.preparing??=this.prepare(explicitStart).finally(()=>{this.preparing=undefined;});return this.preparing;
    }
    private async prepare(explicitStart:boolean):Promise<RuntimeBinding>{
        let lastError:unknown;
        for(let attempt=0;attempt<3;attempt++){
            try {
                const observed=await this.discover(),{bundle,installDir}=observed,managed=this.options.managed;
                const inspected=observed.inspection,selection=inspected?record(inspected.selection):undefined;
                if(inspected?.payload_verified===false)throw new ServiceError('selected_runtime_payload_unverified');
                if(!explicitStart&&inspected?.lifecycle&&record(inspected.lifecycle).explicitly_stopped===true)throw new ServiceError('service_explicitly_stopped');
                if(managed){
                    if(inspected){
                        if(!inspected.managed_policy)throw new ServiceError('managed_runtime_policy_mismatch');
                        const policy=record(inspected.managed_policy),policyApproval=record(policy.approval);
                        const source=await fs.realpath(managed.source),savedSource=await fs.realpath(safePath(policy.source));
                        if(path.relative(source,savedSource)!==''||policyApproval.manifest_sha256!==managed.approvedSha256
                            ||policy.lifecycle!==(managed.lifecycle==='external'?'external':'user_on_demand')){
                            throw new ServiceError('managed_runtime_policy_mismatch');
                        }
                        if(managed.stateDir){
                            const state=await fs.realpath(managed.stateDir),control=await fs.realpath(safePath(inspected.control_dir));
                            if(path.relative(path.join(state,'control'),control)!=='')throw new ServiceError('managed_runtime_state_mismatch');
                        }
                    }else {const args=['runtime','managed','--source',managed.source,'--install-dir',installDir,
                        '--approved-sha256',managed.approvedSha256,'--verification-basis','organization-digest','--lifecycle',managed.lifecycle,'--channel',this.options.channel];
                        if(managed.stateDir)args.push('--state-dir',managed.stateDir);await this.invoke(bundle.main,args,180000);}
                }else if(!selection?.active){
                    await this.invoke(bundle.main,['runtime','stage','--source',bundle.source,'--install-dir',installDir,
                        '--approved-sha256',bundle.digest,'--verification-basis','trusted-distribution','--channel',this.options.channel],180000);
                    await this.invoke(bundle.main,['runtime','activate','--install-dir',installDir,'--runtime-sha256',bundle.digest],180000);
                }
                if(selection?.active&&runtimeVersion(record(inspected?.approval).version)!==this.version)throw new ServiceError('explicit_runtime_upgrade_or_matching_extension_required');
                if(!explicitStart&&record(inspected?.lifecycle??{}).explicitly_stopped===true)throw new ServiceError('service_explicitly_stopped');
                await this.invoke(bundle.main,['runtime',explicitStart?'start':'ensure','--install-dir',installDir],180000);
                if(managed)await this.invoke(bundle.main,['runtime','prepare-client','--install-dir',installDir],180000);
                const inspection=await this.invoke(bundle.main,['runtime','inspect','--install-dir',installDir]);
                return this.binding(inspection,installDir);
            }catch(error){
                if(error instanceof ServiceError&&['runtime_client_no_longer_authorized','managed_runtime_policy_mismatch','managed_runtime_state_mismatch','service_explicitly_stopped','selected_runtime_payload_unverified','explicit_runtime_upgrade_or_matching_extension_required'].includes(error.code))throw error;
                lastError=error;if(attempt<2)await new Promise(resolve=>setTimeout(resolve,1000*(attempt+1)));
            }
        }
        throw lastError;
    }
    binding(inspection:RecordValue,installDir:string):RuntimeBinding {
        const selection=record(inspection.selection),approval=record(inspection.approval),managed=inspection.managed_policy?record(inspection.managed_policy):undefined;
        const version=runtimeVersion(approval.version);
        const runtimeSha256=identifier(selection.active,32);
        if(approval.manifest_sha256!==runtimeSha256){throw new ServiceError('runtime_selection_approval_mismatch');}
        if(!['trusted_distribution','organization_digest'].includes(String(approval.verification_basis))){throw new ServiceError('runtime_verification_policy_unavailable');}
        return {installDir:safePath(installDir),controlDir:safePath(inspection.control_dir),executable:safePath(inspection.executable),
            launcher:inspection.launcher===null?null:safePath(inspection.launcher),runtimeSha256,version,
            lifecycle:managed?.lifecycle==='external'?'external':managed?'user-on-demand':'on-demand',
            verificationBasis:typeof approval.verification_basis==='string'?approval.verification_basis:'unknown',
            payloadVerified:inspection.payload_verified!==false&&inspection.metadata_only!==true};
    }
    client(binding:RuntimeBinding):ServiceClient {
        this.active();
        if(binding.version!==this.version)throw new ServiceError('service_runtime_version_mismatch');
        if(binding.payloadVerified===false)throw new ServiceError('selected_runtime_payload_unverified');
        return new ServiceClient(new NativeControlTransport(binding.executable,['service','console','--runtime-dir',binding.controlDir,
            '--runtime-sha256',binding.runtimeSha256]));
    }
    async snapshot(binding:RuntimeBinding,after:string|null=null,revision:number|null=null):Promise<RecordValue>{
        this.active();const bundle=await this.verified();
        const args=['runtime','snapshot','--install-dir',binding.installDir];
        if(after!==null){identifier(after,32);args.push('--after',after);}
        if(revision!==null){if(!Number.isSafeInteger(revision)||revision<0)throw new ServiceError('invalid_registry_revision');args.push('--revision',String(revision));}
        const value=await this.invoke(bundle.main,args);
        if(value.schema!==1||value.metadata_only!==true||value.payload_verified!==false)throw new ServiceError('invalid_bootstrap_snapshot');
        return value;
    }
    async repair(binding:RuntimeBinding,preview?:string):Promise<RecordValue>{
        this.active();
        if(binding.lifecycle!=='on-demand')throw new ServiceError('runtime_payload_managed_externally');
        const bundle=await this.verified();
        if(bundle.digest!==binding.runtimeSha256)throw new ServiceError('matching_approved_distribution_required_for_repair');
        const args=['runtime','repair-payload','--install-dir',binding.installDir,'--source',bundle.source,'--approved-sha256',bundle.digest];
        if(preview)args.push('--preview',identifier(preview,32));
        const value=await this.invoke(bundle.main,args,180000);
        if(value.schema!==1||value.runtime!==binding.runtimeSha256)throw new ServiceError('invalid_payload_repair_receipt',!!preview);
        if(preview){if(value.repaired!==true||value.service_started!==false)throw new ServiceError('invalid_payload_repair_receipt',true);}
        else {identifier(value.preview,32);safePath(value.preserved_directory);
            if(!Number.isSafeInteger(value.preserved_bytes)||Number(value.preserved_bytes)<0||Number(value.preserved_bytes)>4*1024**3
                ||!Number.isSafeInteger(value.preserved_files)||Number(value.preserved_files)<0||Number(value.preserved_files)>2050
                ||value.starts_service!==false||value.requires_stopped_owners!==true)throw new ServiceError('invalid_payload_repair_preview');}
        return value;
    }
    async reviewProject(binding:RuntimeBinding,root:string,dataRoot:string,collection:string):Promise<ProjectRegistrationReview>{
        this.active();safePath(root);safePath(dataRoot);
        if(binding.version!==this.version||binding.payloadVerified===false)throw new ServiceError('matching_verified_runtime_required');
        if(!/^[A-Za-z0-9._-]{1,128}$/.test(collection)||collection==='.'||collection==='..')throw new ServiceError('invalid_project_collection');
        const bundle=await this.verified();
        const registration=projectRegistration(await this.invoke(bundle.main,['--root',root,'--data-dir',dataRoot,'--collection',collection,'service','prepare-registration']));
        if(registration.collection!==collection)throw new ServiceError('crossed_project_registration');
        const review:ProjectRegistrationReview=Object.freeze({installDir:binding.installDir,controlDir:binding.controlDir,runtimeSha256:binding.runtimeSha256,...registration});
        this.projectReviews.add(review);return review;
    }
    async registerProject(binding:RuntimeBinding,review:ProjectRegistrationReview):Promise<ProjectRegistrationReview>{
        this.active();
        if(!this.projectReviews.has(review)||review.installDir!==binding.installDir||review.controlDir!==binding.controlDir
            ||review.runtimeSha256!==binding.runtimeSha256)throw new ServiceError('project_registration_not_reviewed');
        const registration={binding:review.binding,root:review.root,data_root:review.data_root,collection:review.collection};
        const client=this.client(binding);
        try {
            const response=await client.rpc({reserve_project:{project:registration}},'registered');this.active();
            if(JSON.stringify(projectRegistration(record(response.project).registration))!==JSON.stringify(registration))throw new ServiceError('crossed_project_registration');
        }finally{client.dispose();}
        const bundle=await this.verified();
        const completed=projectRegistration(await this.invoke(bundle.main,['service','prepare-storage','--runtime-dir',binding.controlDir,'--project',review.binding]));
        if(JSON.stringify(completed)!==JSON.stringify(registration))throw new ServiceError('crossed_project_storage');
        this.projectReviews.delete(review);return review;
    }
    async prepareConfiguration(binding:RuntimeBinding,project:string,draft:RecordValue):Promise<ProjectConfigurationRequest>{
        this.active();identifier(project,32);
        if(binding.version!==this.version)throw new ServiceError('service_runtime_version_mismatch');
        const input=JSON.stringify(draft);
        if(Buffer.byteLength(input)>16384)throw new ServiceError('configuration_draft_exceeds_budget');
        const bundle=await this.verified();
        return validateConfigurationRequest(await this.invoke(bundle.main,['service','prepare-configuration','--runtime-dir',binding.controlDir,'--project',project],60000,input));
    }
    private legacyReview(binding:RuntimeBinding,value:RecordValue,phase?:string):LegacyStartupReview {
        if(Buffer.byteLength(JSON.stringify(value))>16384)throw new ServiceError('legacy_review_exceeds_budget');
        const preview=record(value.preview),spec=record(preview.spec),target=legacyTarget(spec.target),workers=record(value.workers);
        if(preview.version!==1||typeof preview.removal_available!=='boolean'||typeof spec.id!=='string'||!/^airk-[a-f0-9]{32}$/.test(spec.id))throw new ServiceError('invalid_legacy_review');
        identifier(preview.namespace,16);count(preview.epoch);safePath(spec.definition);safePath(spec.receipt);
        if(!Array.isArray(workers.matching_executable_pids)||workers.matching_executable_pids.length>64||workers.matching_executable_pids.some(pid=>!Number.isSafeInteger(pid)||Number(pid)<1||Number(pid)>0xffffffff)||typeof workers.complete!=='boolean')throw new ServiceError('invalid_legacy_workers');
        count(workers.scanned);count(workers.unobserved);if(Number(workers.scanned)>16384||Number(workers.unobserved)>Number(workers.scanned))throw new ServiceError('invalid_legacy_workers');
        const approval=value.approval===null?undefined:identifier(value.approval,32);
        if(preview.removal_available!==!!approval||phase!==undefined&&!legacyPhases.includes(phase))throw new ServiceError('invalid_legacy_approval');
        const observation=record(preview.observation);
        if(approval&&(observation.owned!==true||observation.loaded!==false&&!(observation.loaded===true&&observation.definition_matches===true)))throw new ServiceError('unproven_legacy_ownership');
        const review:LegacyStartupReview=Object.freeze({installDir:binding.installDir,controlDir:binding.controlDir,runtimeSha256:binding.runtimeSha256,target:Object.freeze(target),value,approval,phase});
        this.legacyReviews.add(review);return review;
    }
    async reviewLegacyStartup(binding:RuntimeBinding,input:LegacyStartupTarget):Promise<LegacyStartupReview>{
        this.active();if(binding.version!==this.version)throw new ServiceError('service_runtime_version_mismatch');
        const target=legacyTarget(input),bundle=await this.verified();
        const review=this.legacyReview(binding,await this.invoke(bundle.main,['service','legacy','review','--runtime-dir',binding.controlDir,...legacyArguments(target)]));
        if(review.target.collection!==target.collection)throw new ServiceError('crossed_legacy_review');
        for(const key of ['root','data_dir','executable'] as const){const [requested,returned]=await Promise.all([fs.realpath(target[key]),fs.realpath(review.target[key])]);this.active();if(path.relative(requested,returned)!=='')throw new ServiceError('crossed_legacy_review');}
        return review;
    }
    async legacyStartupReceipts(binding:RuntimeBinding):Promise<RecordValue>{
        this.active();if(binding.version!==this.version)throw new ServiceError('service_runtime_version_mismatch');
        const bundle=await this.verified(),value=await this.invoke(bundle.main,['service','legacy','list','--runtime-dir',binding.controlDir]);
        identifier(value.namespace,16);count(value.epoch);
        if(!Array.isArray(value.items)||value.items.length>16)throw new ServiceError('invalid_legacy_receipt_list');
        for(const item of value.items){const row=record(item);identifier(row.approval,32);if(typeof row.service_id!=='string'||!/^airk-[a-f0-9]{32}$/.test(row.service_id)||!legacyPhases.includes(String(row.phase)))throw new ServiceError('invalid_legacy_receipt_list');}
        return value;
    }
    async loadLegacyStartup(binding:RuntimeBinding,approval:string):Promise<LegacyStartupReview>{
        this.active();identifier(approval,32);if(binding.version!==this.version)throw new ServiceError('service_runtime_version_mismatch');
        const bundle=await this.verified(),intent=await this.invoke(bundle.main,['service','legacy','show','--runtime-dir',binding.controlDir,'--approval',approval]);
        if(intent.version!==1||typeof intent.phase!=='string')throw new ServiceError('invalid_legacy_receipt');
        const review=this.legacyReview(binding,record(intent.review),intent.phase);
        if(review.approval!==approval)throw new ServiceError('crossed_legacy_receipt');return review;
    }
    async removeLegacyStartup(binding:RuntimeBinding,review:LegacyStartupReview):Promise<RecordValue>{
        this.active();if(!this.legacyReviews.has(review)||!review.approval||review.installDir!==binding.installDir||review.controlDir!==binding.controlDir||review.runtimeSha256!==binding.runtimeSha256||binding.version!==this.version)throw new ServiceError('legacy_startup_not_reviewed');
        const bundle=await this.verified(),result=await this.invoke(bundle.main,['service','legacy','remove-startup','--runtime-dir',binding.controlDir,...legacyArguments(legacyTarget(review.target)),'--approval',identifier(review.approval,32)],120000);
        if(result.approval!==review.approval||result.startup_removed!==true||result.sources_preserved!==true||result.project_settings_preserved!==true||result.indexes_retained!==true)throw new ServiceError('crossed_legacy_removal',true);
        this.legacyReviews.delete(review);return result;
    }
    async reviewTransition(binding:RuntimeBinding,action:RuntimeTransitionReview['action']):Promise<RuntimeTransitionReview>{
        this.active();
        if(binding.lifecycle!=='on-demand'||this.options.managed)throw new ServiceError('runtime_payload_managed_externally');
        if(!['upgrade','rollback','resume'].includes(action))throw new ServiceError('invalid_runtime_transition');
        const observed=await this.discover();
        if(path.relative(binding.installDir,observed.installDir)!==''||!observed.inspection)throw new ServiceError('runtime_installation_changed');
        const current=this.binding(observed.inspection,observed.installDir);
        if(current.payloadVerified===false)throw new ServiceError('repair_selected_payload_before_activation');
        if(current.runtimeSha256!==binding.runtimeSha256||path.relative(current.controlDir,binding.controlDir)!=='')throw new ServiceError('runtime_selection_changed_review_again');
        const selection=record(observed.inspection.selection),pending=observed.diagnosis?.pending_activation;
        let expectedActive=current.runtimeSha256,target:string,version=this.version;
        if(action==='resume'){
            const saved=record(pending);expectedActive=identifier(record(saved.prior).active,32);target=identifier(saved.candidate,32);
            const [savedInstall,installed,savedControl,control]=await Promise.all([
                fs.realpath(safePath(saved.install)),fs.realpath(binding.installDir),fs.realpath(safePath(saved.control)),fs.realpath(binding.controlDir),
            ]);
            this.active();
            if(path.relative(savedInstall,installed)!==''||path.relative(savedControl,control)!=='')throw new ServiceError('crossed_runtime_activation');
        }else{
            if(pending)throw new ServiceError('resume_pending_runtime_activation');
            if(action==='rollback'&&!selection.previous)throw new ServiceError('runtime_previous_selection_unavailable');
            target=action==='upgrade'?observed.bundle.digest:identifier(selection.previous,32);
            if(target===current.runtimeSha256)throw new ServiceError('runtime_already_selected');
        }
        if(action!=='upgrade'){
            const candidate=await this.invoke(observed.bundle.main,['runtime','inspect-candidate','--install-dir',binding.installDir,'--runtime-sha256',target]);
            const approval=record(candidate.approval);
            version=runtimeVersion(approval.version);
            if(candidate.metadata_only!==true||candidate.selection_changed!==false||candidate.executed!==false||candidate.runtime_sha256!==target||approval.manifest_sha256!==target
                ||version.split('.').slice(0,2).join('.')!==this.version.split('.').slice(0,2).join('.')
                ||!['trusted_distribution','organization_digest'].includes(String(approval.verification_basis)))throw new ServiceError('candidate_not_compatible_with_this_extension');
        }
        const review:RuntimeTransitionReview=Object.freeze({action,installDir:binding.installDir,controlDir:binding.controlDir,
            observedActive:current.runtimeSha256,expectedActive,target,version});
        this.transitionReviews.add(review);return review;
    }
    async transition(binding:RuntimeBinding,review:RuntimeTransitionReview):Promise<RecordValue>{
        this.active();
        if(!this.transitionReviews.has(review)||review.installDir!==binding.installDir||review.controlDir!==binding.controlDir
            ||review.observedActive!==binding.runtimeSha256)throw new ServiceError('runtime_transition_not_reviewed');
        if(binding.lifecycle!=='on-demand'||this.options.managed)throw new ServiceError('runtime_payload_managed_externally');
        const bundle=await this.verified();
        if(review.action==='upgrade'){
            if(bundle.digest!==review.target)throw new ServiceError('reviewed_distribution_changed');
            await this.invoke(bundle.main,['runtime','stage','--source',bundle.source,'--install-dir',binding.installDir,
                '--approved-sha256',bundle.digest,'--verification-basis','trusted-distribution','--channel',this.options.channel],180000);
        }
        const selected=await this.invoke(bundle.main,['runtime','activate','--install-dir',binding.installDir,
            '--runtime-sha256',review.target,'--expected-active',review.expectedActive],180000);
        if(selected.schema!==1||selected.active!==review.target)throw new ServiceError('crossed_runtime_activation_receipt',true);
        this.transitionReviews.delete(review);return selected;
    }
    async lifecycle(binding:RuntimeBinding,intent:LifecycleIntent,force=false):Promise<RecordValue>{
        this.active();
        if(binding.lifecycle==='external')throw new ServiceError('lifecycle_managed_externally');
        if(intent.lifecycle.runtime!==binding.runtimeSha256)throw new ServiceError('lifecycle_runtime_selection_changed');
        const bundle=await this.verified();
        const result=await this.invoke(bundle.main,['runtime','lifecycle','--install-dir',binding.installDir],180000,
            JSON.stringify({...intent.lifecycle,force}));
        if(result.status!=='lifecycle')throw new ServiceError('invalid_lifecycle_receipt',true);
        const receipt=record(result.record),request=record(receipt.request),key=record(request.key);
        if(request.caller!==intent.lifecycle.caller||request.runtime!==intent.lifecycle.runtime
            ||request.action!==intent.lifecycle.action||request.expected_instance!==intent.lifecycle.expected_instance
            ||key.namespace!==intent.lifecycle.key.namespace||key.epoch!==intent.lifecycle.key.epoch||key.nonce!==intent.lifecycle.key.nonce)throw new ServiceError('crossed_lifecycle_receipt',true);
        if(!['complete','abandoned'].includes(String(receipt.phase)))throw new ServiceError('lifecycle_request_incomplete',true);
        return result;
    }
    async abandonLifecycle(binding:RuntimeBinding,intent:LifecycleIntent,id:string):Promise<RecordValue>{
        this.active();identifier(id,32);
        const bundle=await this.verified();
        const result=await this.invoke(bundle.main,['runtime','abandon-lifecycle','--install-dir',binding.installDir,'--request-id',id]);
        const receipt=record(result.record),request=record(receipt.request),key=record(request.key);
        if(result.status!=='lifecycle'||receipt.id!==id||!['complete','abandoned'].includes(String(receipt.phase))
            ||request.runtime!==intent.lifecycle.runtime||request.caller!==intent.lifecycle.caller
            ||request.action!==intent.lifecycle.action||request.expected_instance!==intent.lifecycle.expected_instance
            ||key.namespace!==intent.lifecycle.key.namespace||key.epoch!==intent.lifecycle.key.epoch||key.nonce!==intent.lifecycle.key.nonce)throw new ServiceError('crossed_lifecycle_receipt',true);
        return result;
    }
}
