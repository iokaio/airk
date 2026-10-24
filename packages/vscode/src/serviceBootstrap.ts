// SPDX-License-Identifier: Apache-2.0
import { createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { runText } from './native';
import { NativeControlTransport, ServiceClient, ServiceError, record, RecordValue, identifier } from './serviceClient';
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
}
export interface BootstrapObservation {
    bundle: VerifiedBundle; installDir: string; location: RecordValue;
    installed: boolean; inspection?: RecordValue; diagnosis?: RecordValue; error?: string;
}
function safePath(value: unknown): string {
    if(typeof value!=='string'||value.length>4096||!path.isAbsolute(value)||/[\u0000-\u001f\u007f]/.test(value)){throw new ServiceError('invalid_runtime_path');}
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
                }else if(!selection?.active||record(inspected?.approval).version!==this.version){
                    await this.invoke(bundle.main,['runtime','stage','--source',bundle.source,'--install-dir',installDir,
                        '--approved-sha256',bundle.digest,'--verification-basis','trusted-distribution','--channel',this.options.channel],180000);
                    await this.invoke(bundle.main,['runtime','activate','--install-dir',installDir,'--runtime-sha256',bundle.digest],180000);
                }
                await this.invoke(bundle.main,['runtime',explicitStart?'start':'ensure','--install-dir',installDir],180000);
                const inspection=await this.invoke(bundle.main,['runtime','inspect','--install-dir',installDir]);
                return this.binding(inspection,installDir);
            }catch(error){
                if(error instanceof ServiceError&&['runtime_client_no_longer_authorized','managed_runtime_policy_mismatch','managed_runtime_state_mismatch','service_explicitly_stopped'].includes(error.code))throw error;
                lastError=error;if(attempt<2)await new Promise(resolve=>setTimeout(resolve,1000*(attempt+1)));
            }
        }
        throw lastError;
    }
    binding(inspection:RecordValue,installDir:string):RuntimeBinding {
        const selection=record(inspection.selection),approval=record(inspection.approval),managed=inspection.managed_policy?record(inspection.managed_policy):undefined;
        if(approval.version!==this.version){throw new ServiceError('service_runtime_version_mismatch');}
        const runtimeSha256=identifier(selection.active,32);
        if(approval.manifest_sha256!==runtimeSha256){throw new ServiceError('runtime_selection_approval_mismatch');}
        if(!['trusted_distribution','organization_digest'].includes(String(approval.verification_basis))){throw new ServiceError('runtime_verification_policy_unavailable');}
        return {installDir:safePath(installDir),controlDir:safePath(inspection.control_dir),executable:safePath(inspection.executable),
            launcher:inspection.launcher===null?null:safePath(inspection.launcher),runtimeSha256,version:this.version,
            lifecycle:managed?.lifecycle==='external'?'external':managed?'user-on-demand':'on-demand',
            verificationBasis:typeof approval.verification_basis==='string'?approval.verification_basis:'unknown'};
    }
    client(binding:RuntimeBinding):ServiceClient {
        this.active();
        return new ServiceClient(new NativeControlTransport(binding.executable,['service','console','--runtime-dir',binding.controlDir,
            '--runtime-sha256',binding.runtimeSha256]));
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
