// SPDX-License-Identifier: Apache-2.0
import { createHash } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import * as fs from 'node:fs/promises';
import { HttpConnectionInfo, HttpSelection, validateHttpConnection, validateEndpoint, matchesImportedHttpOrigin, validateAttachmentRetirement,validatePredecessorHttpLookup,type PredecessorHttpLookup,type PredecessorHttpScope } from './serviceHttp';
import * as path from 'node:path';
import { runText } from './native';
import { NativeControlTransport, ServiceClient, ServiceError, record, RecordValue, identifier, count, ProjectConfigurationRequest, validateConfigurationRequest } from './serviceClient';
import type { LifecycleIntent } from './serviceIntents';
import { validateRetirementReply } from './runtimeLegacyRetirement';
import {validateNativeLegacyBinding} from './indexingCatalog';
import {IssuerRotationAction,IssuerRotationStatus,validateIssuerRotation} from './httpIssuerRotation';
import { NativeRebuildObservation, validateRebuildObservation, runtimeCleanBreakRequired, RuntimeCleanBreakPolicy, validateCleanBreakPolicy } from './runtimeRebuild';

/** Version direction only, never a protocol/storage compatibility claim.
 * Native authenticated formats and migration capabilities decide whether a
 * selected older payload can actually be replaced. Skipped releases are allowed. */
export function automaticUpgrade(installed: string, bundled: string): boolean {
    if(!/^\d+\.\d+\.\d+$/.test(installed)||!/^\d+\.\d+\.\d+$/.test(bundled))return false;
    const from = installed.split('.').map(Number), to = bundled.split('.').map(Number);
    if (from.length !== 3 || to.length !== 3 || [...from, ...to].some(part => !Number.isSafeInteger(part) || part < 0)) { return false; }
    return from[0]<to[0]||from[0]===to[0]&&(from[1]<to[1]||from[1]===to[1]&&from[2]<=to[2]);
}
export type RuntimeChannel = 'stable' | 'preview';
export interface RuntimeOptions {
    channel: RuntimeChannel; installDir?: string;
    managed?: { source: string; approvedSha256: string; lifecycle: 'external' | 'user-on-demand'; stateDir?: string };
}
export type NativeRunner = (command: string, args: string[], timeout?: number,input?:string) => Promise<RecordValue>;
export const nativeRunner: NativeRunner = async (command,args,timeout=60000,input) => {
    const text=await runText(command,args,path.dirname(command),timeout,undefined,undefined,input);
    const adoption=args[0]==='runtime'&&args[1]==='adopt-legacy-indexes',maximum=adoption?16*1024*1024:2*1024*1024;
    if(Buffer.byteLength(text)>maximum){throw new ServiceError('bootstrap_response_oversize',adoption);}
    return record(JSON.parse(text));
};
export interface VerifiedBundle { main: string; helper?: string; source: string; digest: string; version: string; platform: string }
export interface RuntimeBinding {
    installDir: string; controlDir: string; executable: string; launcher: string | null;
    /** Version-free launcher that currently forwards to this installation, when published. */
    universalLauncher?: string | null;
    runtimeSha256: string; version: string; lifecycle: 'on-demand' | 'external' | 'user-on-demand';
    verificationBasis: string;
    payloadVerified?:boolean;
    /** Complete native observation, kept separate from runtime health/readiness. */
    rebuild?:NativeRebuildObservation;
    cleanBreak?:RuntimeCleanBreakPolicy;
}
export interface LegacyOriginLocator {path:string;format:'codex_toml'|'mcp_servers_json'|'vscode_servers_json'|'open_code_json';server:string;container_path?:string[]}
export function legacyOriginLocatorIdentity(locator:LegacyOriginLocator):string{
    // Literal locator identity stays separate from native effective-origin
    // identity; omitted and explicit default containers may share one proof.
    return JSON.stringify([locator.path,locator.format,locator.server,locator.container_path??null]);
}
function validateLegacyOriginLocator(input:unknown):LegacyOriginLocator{
    const value=record(input);safePath(value.path);
    if(!['codex_toml','mcp_servers_json','vscode_servers_json','open_code_json'].includes(String(value.format))||typeof value.server!=='string'||!value.server||Buffer.byteLength(value.server)>256||/[\x00-\x1f\x7f-\x9f]/.test(value.server)||value.container_path!==undefined&&(!Array.isArray(value.container_path)||value.container_path.length>16||value.container_path.some(part=>typeof part!=='string'||!part||Buffer.byteLength(part)>4096||/[\x00-\x1f\x7f-\x9f]/.test(part))))throw Error('invalid_native_original_locator');
    return input as LegacyOriginLocator;
}
/** Complete native inventory. Scope is derived by native OS installation
 * identity; caller-provided editor paths are candidates, never ownership proof. */
function validateLegacyCachePreparation(input:unknown,binding:RuntimeBinding,context?:string):RecordValue{
    const value=record(input);
    if(value.schema!==1||value.active!==binding.runtimeSha256||!['native-defaults','context-only'].includes(String(value.scope)))throw new ServiceError('legacy_cache_inspection_unconfirmed',false,value);
    identifier(value.inspection_sha256,32);safePath(value.context_global_storage);
    if(context&&!sameNativeLocation(String(value.context_global_storage),context))throw new ServiceError('legacy_cache_inspection_unconfirmed',false,value);
    const roots=value.global_storage_roots;
    if(!Array.isArray(roots)||!roots.length||roots.length>128||!roots.every(root=>typeof root==='string')||new Set(roots).size!==roots.length)throw new ServiceError('legacy_cache_inspection_unconfirmed',false,value);
    roots.forEach(root=>safePath(root));
    if(!roots.some(root=>sameNativeLocation(root,String(value.context_global_storage)) )||!Array.isArray(value.caches)||value.caches.length>128)throw new ServiceError('legacy_cache_inspection_unconfirmed',false,value);
    if(!Array.isArray(value.origin_locators)||value.origin_locators.length>128||!Array.isArray(value.origin_proofs)||value.origin_proofs.length>128)throw new ServiceError('legacy_cache_inspection_unconfirmed',false,value);
    const locators=value.origin_locators.map(validateLegacyOriginLocator),keys=locators.map(legacyOriginLocatorIdentity);
    const effective=(locator:LegacyOriginLocator)=>locator.container_path??[locator.format==='codex_toml'?'mcp_servers':locator.format==='vscode_servers_json'?'servers':locator.format==='open_code_json'?'mcp':'mcpServers'];
    const matches=(input:unknown,locator:LegacyOriginLocator)=>{const proof=record(input);return proof.config_path===locator.path&&proof.server===locator.server&&isDeepStrictEqual(proof.container_path,effective(locator));};
    if(locators.some(locator=>(value.origin_proofs as unknown[]).filter(proof=>matches(proof,locator)).length!==1)||(value.origin_proofs as unknown[]).some(proof=>!locators.some(locator=>matches(proof,locator))))throw new ServiceError('legacy_cache_inspection_unconfirmed',false,value);
    if(locators.length){
        if(!Array.isArray(value.origin_classifications)||value.origin_classifications.length!==locators.length||!Array.isArray(value.adoption_locators)||!Array.isArray(value.managed_locators))throw new ServiceError('legacy_origin_classification_unconfirmed',false,value);
        const classified=new Map<string,{kind:string;origin:string;count:number}>(),origins=new Map<string,string>(),originProofs=new Map<string,unknown>();
        for(const input of value.origin_classifications){
            const item=record(input),locator=validateLegacyOriginLocator(item.locator),key=legacyOriginLocatorIdentity(locator);identifier(item.origin_sha256,32);const proof=(value.origin_proofs as unknown[]).find(value=>matches(value,locator));
            const previous=classified.get(key),origin=String(item.origin_sha256),kind=String(item.kind);
            if(!keys.includes(key)||!['standalone','managed','unsupported'].includes(kind)||item.authority!=='not_observed'||previous&&(previous.origin!==origin||previous.kind!==kind)||origins.has(origin)&&(origins.get(origin)!==kind||!isDeepStrictEqual(originProofs.get(origin),proof)))throw new ServiceError('legacy_origin_classification_unconfirmed',false,value);
            classified.set(key,{kind,origin,count:(previous?.count??0)+1});origins.set(origin,kind);originProofs.set(origin,proof);
        }
        for(const key of new Set(keys))if(classified.get(key)?.count!==keys.filter(value=>value===key).length)throw new ServiceError('legacy_origin_classification_unconfirmed',false,value);
        for(const [field,kind]of [['adoption_locators','standalone'],['managed_locators','managed']]){
            const selected=(value[field] as unknown[]).map(validateLegacyOriginLocator).map(legacyOriginLocatorIdentity),seen=new Set<string>(),expected=[...origins].filter(([,entry])=>entry===kind).map(([origin])=>origin);
            for(const key of selected){const item=classified.get(key);if(!item||item.kind!==kind||seen.has(item.origin))throw new ServiceError('legacy_origin_classification_unconfirmed',false,value);seen.add(item.origin);}
            if(seen.size!==expected.length||expected.some(origin=>!seen.has(origin)))throw new ServiceError('legacy_origin_classification_unconfirmed',false,value);
        }
    }
    const images=new Set<string>();
    for(const entry of value.caches){
        const cache=record(entry);safePath(cache.global_storage);safePath(cache.cache_directory);safePath(cache.executable);identifier(cache.executable_sha256,32);
        if(cache.schema!==1||cache.provenance!=='owned_legacy_helper_cache'||typeof cache.user!=='string'||!cache.user||typeof cache.artifact_approved!=='boolean'||!roots.some(root=>sameNativeLocation(root,String(cache.global_storage)))||typeof cache.digest_prefix!=='string'||!/^[a-f0-9]{16}$/.test(cache.digest_prefix)||!String(cache.executable_sha256).startsWith(cache.digest_prefix)||images.has(String(cache.executable)))throw new ServiceError('legacy_cache_inspection_unconfirmed',false,value);
        for(const field of ['global_storage_identity','helpers_identity','cache_directory_identity','executable_identity','cache_version','platform'])if(typeof cache[field]!=='string'||!cache[field]||Buffer.byteLength(String(cache[field]))>4096||/[\x00-\x1f\x7f]/.test(String(cache[field])))throw new ServiceError('legacy_cache_inspection_unconfirmed',false,value);
        if(!sameNativeLocation(path.dirname(String(cache.cache_directory)),path.join(String(cache.global_storage),'iokaio.airk','helpers'))||!sameNativeLocation(path.dirname(String(cache.executable)),String(cache.cache_directory))||!['airk','airk.exe'].includes(path.basename(String(cache.executable)).toLowerCase()))throw new ServiceError('legacy_cache_inspection_unconfirmed',false,value);
        images.add(String(cache.executable));
    }
    return value;
}
export interface BootstrapObservation {
    bundle: VerifiedBundle; installDir: string; location: RecordValue;
    installed: boolean; inspection?: RecordValue; diagnosis?: RecordValue; error?: string;
}
export interface RuntimeTransitionReview {
    readonly action:'upgrade'|'rollback'|'resume'; readonly installDir:string; readonly controlDir:string;
    readonly observedActive:string; readonly expectedActive:string; readonly target:string; readonly version:string;
    readonly rebuildIndexes?:boolean;
    readonly cleanBreak?:boolean;
    readonly cleanBreakPolicy?:RuntimeCleanBreakPolicy;
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
function sameNativeLocation(left:string,right:string):boolean {
    return path.relative(process.platform==='win32'?path.toNamespacedPath(left):left,
        process.platform==='win32'?path.toNamespacedPath(right):right)==='';
}
export interface NativeSourceScope {
    readonly root:string; readonly collection:string|null;
    readonly namespace:string|null; readonly revision:number|null;
    readonly observed_unix_ms:number|null;
    readonly outcome:'registered'|'missing'|'removed'|'ambiguous';
    readonly registration:Readonly<{binding:string;root:string;data_root:string;collection:string}>|null;
    readonly fencing_generation:number|null;
}
const exactScopeOwners=new WeakMap<NativeSourceScope,{binding:string;root:string;collection:string|null}>();
/** Recovery progress is an observation, not part of a connection's identity. */
export const runtimeBindingIdentity=(binding:RuntimeBinding)=>{const {rebuild:_rebuild,...identity}=binding;return JSON.stringify(identity);};
const exactBindingIdentity=runtimeBindingIdentity;
export function exactScopeMatches(value:NativeSourceScope,binding:RuntimeBinding,root:string,collection:string|null):boolean {
    const owner=exactScopeOwners.get(value);
    return !!owner&&owner.binding===exactBindingIdentity(binding)&&owner.root===root&&owner.collection===collection;
}
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
    const prefix=host.platform==='darwin'?'AIRK.app/Contents/MacOS/':'';
    const main=prefix+(host.platform==='win32'?'airk.exe':'airk'),helper=prefix+(host.platform==='win32'?'airk-embed.exe':'airk-embed');
    const payload=record(wrapper.payload);
    const [major,minor,patch]=version.split(/[.-]/).slice(0,3).map(Number),singleImage=major>1||major===1&&(minor>8||minor===8&&patch>=14);
    if(singleImage){
        if(Object.keys(payload).length!==1||payload[main]!==files[main]||!files[main]||names.some(name=>/(?:^|\/)(?:airk-embed|airk-launcher)(?:\.exe)?$/.test(name)))throw new ServiceError('single_image_runtime_required');
        const metadata=(await boundedJson(path.join(source,'airk-runtime.json'),16384)).value;
        if(Object.keys(metadata).sort().join(',')!=='airk_sha256,arch,os,schema,version'||metadata.schema!==2||metadata.version!==version||metadata.os!==os||metadata.arch!==arch||metadata.airk_sha256!==files[main])throw new ServiceError('single_image_runtime_metadata_mismatch');
        return {main:path.join(source,main),source,digest,version,platform};
    }
    if(payload[main]!==files[main]||payload[helper]!==files[helper]||!files[main]||!files[helper]){throw new ServiceError('runtime_pair_missing');}
    return {main:path.join(source,main),helper:path.join(source,helper),source,digest,version,platform};
}
/** Early UX only: native runtime ownership and ancestor checks remain authoritative. */
export function validateRuntimeInstallDirectory(installDir:string,
    metadata:{isDirectory():boolean;isSymbolicLink():boolean;uid:number;mode:number},
    platform:string=process.platform,effectiveUid:number|undefined=process.geteuid?.()):void {
    if(!metadata.isDirectory()||metadata.isSymbolicLink())throw new ServiceError('unsafe_runtime_install_location');
    if(platform==='win32')return;
    if(effectiveUid===undefined||metadata.uid!==effectiveUid){
        const error=new ServiceError('runtime_install_directory_wrong_owner');
        error.message=`AIRK installation directory ${JSON.stringify(installDir)} is not owned by the current effective user. Choose a directory you own or a new, nonexistent directory. Changing permissions alone cannot fix ownership.`;
        throw error;
    }
    if((metadata.mode&0o077)!==0){
        const quoted="'"+installDir.replace(/'/g,"'\"'\"'")+"'";
        const error=new ServiceError('runtime_install_directory_not_private');
        error.message=`AIRK installation directory ${JSON.stringify(installDir)} has mode ${(metadata.mode&0o777).toString(8)} with group or other access. Make this user-owned directory private with chmod 700 ${quoted}, or choose a new, nonexistent directory. AIRK will not change its permissions automatically.`;
        throw error;
    }
}
export class RuntimeHost {
    private bundle?:Promise<VerifiedBundle>;
    private preparing?:Promise<RuntimeBinding>;
    private discovering?:{execution:Promise<BootstrapObservation>;full?:Promise<BootstrapObservation>;connections:number;fullSettled:boolean};
    private tested?:Promise<void>;
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
    private discovery():NonNullable<RuntimeHost['discovering']>{
        if(this.discovering)return this.discovering;
        const attempt:NonNullable<RuntimeHost['discovering']>={execution:this.discoverOnce(),connections:0,fullSettled:false};
        this.discovering=attempt;return attempt;
    }
    private retireDiscovery(attempt:NonNullable<RuntimeHost['discovering']>):void{
        if(attempt.connections===0&&(!attempt.full||attempt.fullSettled)&&this.discovering===attempt)this.discovering=undefined;
    }
    private fullDiscovery(attempt:NonNullable<RuntimeHost['discovering']>):Promise<BootstrapObservation>{
        if(attempt.full)return attempt.full;
        attempt.full=attempt.execution.then(async value=>{
            const observation=structuredClone(value);
            if(observation.installed){
                try{observation.diagnosis=await this.invoke(observation.bundle.main,['runtime','diagnose','--install-dir',observation.installDir]);}
                catch{this.active();observation.error??='runtime_diagnostics_unavailable';}
            }
            return observation;
        }).finally(()=>{attempt.fullSettled=true;this.retireDiscovery(attempt);});
        // Full is owned only after an explicit caller or mandatory fallback.
        // A connection waiter never cancels an accepted shared full discovery.
        void attempt.full.catch(()=>{});
        return attempt.full;
    }
    discover():Promise<BootstrapObservation>{return this.fullDiscovery(this.discovery()).then(value=>structuredClone(value));}
    /** Current verified execution metadata + exact existing healthy service only.
     * Never starts/elects a process or settles lifecycle work. Missing native
     * recovery observations, managed pins and pending work keep full discovery. */
    async discoverForConnection():Promise<BootstrapObservation>{
        const attempt=this.discovery();attempt.connections++;
        try{
        const observed=await attempt.execution;this.active();
        const inspection=observed.inspection;
        if(!inspection||observed.error||inspection.payload_verified!==true||inspection.metadata_only===true||this.options.managed)
            {const complete=await this.fullDiscovery(attempt);this.active();return structuredClone(complete);}
        const lifecycle=record(inspection.lifecycle??{});
        if(lifecycle.explicitly_stopped===true)throw new ServiceError('service_explicitly_stopped');
        const binding=this.binding(inspection,observed.installDir);
        if(binding.version!==this.version)throw new ServiceError('explicit_runtime_upgrade_or_matching_extension_required');
        const recovery=record(inspection.connection_observation??{}),repair=record(inspection.payload_repair??{});
        if(recovery.schema!==1||recovery.activation_pending!==false||recovery.launcher_repair_pending!==false
            ||!['none','completed'].includes(String(repair.phase))||lifecycle.explicitly_stopped!==false||lifecycle.pending!==null
            ||typeof inspection.registry_namespace!=='string'||!/^[a-f0-9]{32}$/.test(inspection.registry_namespace))
            {const complete=await this.fullDiscovery(attempt);this.active();return structuredClone(complete);}
        const client=this.client(binding);
        try{
            const response=await client.rpc('health','health');this.active();const health=record(response.observation);
            if(health.version!==this.version||health.runtime_sha256!==binding.runtimeSha256||record(health.policy).accepting!==true)
                throw new ServiceError('runtime_selection_changed_review_again');
            identifier(health.instance,16);
            return structuredClone(observed);
        }finally{client.dispose();}
        }finally{attempt.connections--;this.retireDiscovery(attempt);}
    }
    private async discoverOnce():Promise<BootstrapObservation>{
        const bundle=await this.verified();
        this.tested??=this.invoke(bundle.main,['runtime','self-test']).then(self=>{
            if(self.protocol!==1||self.version!==this.version||self.self_test!=='passed'||self.metadata_console!==1)throw new ServiceError('service_runtime_self_test_failed');
        }).catch(error=>{this.tested=undefined;throw error;});
        await this.tested;
        const location=await this.invoke(bundle.main,['runtime','location','--channel',this.options.channel]);
        if(location.protocol!==1){throw new ServiceError('runtime_locator_protocol_mismatch');}
        const installDir=this.options.installDir??safePath(location.install_dir);
        const observation:BootstrapObservation={bundle,installDir,location,installed:false};
        try {const metadata=await fs.lstat(installDir);this.active();validateRuntimeInstallDirectory(installDir,metadata);}
        catch(error){this.active();if((error as NodeJS.ErrnoException).code==='ENOENT')return observation;throw error;}
        observation.installed=true;
        try {observation.inspection=await this.invoke(bundle.main,['runtime','inspect','--install-dir',installDir]);}
        catch {observation.error='runtime_selection_unavailable';}
        if(!observation.inspection){
            try{const snapshot=await this.invoke(bundle.main,['runtime','snapshot','--install-dir',installDir]);
                if(snapshot.schema===1&&snapshot.metadata_only===true&&snapshot.payload_verified===false){observation.inspection=snapshot;observation.error='selected_runtime_payload_unverified';}}
            catch{/* Preserve unavailable state without nominating a different installation. */}
        }
        return observation;
    }
    /** A verified installation can reconnect without running install/ensure again.
     * Missing or mismatched health falls back to the native lifecycle authority. */
    async reconnect(observed:BootstrapObservation):Promise<RuntimeBinding>{
        this.active();
        const inspection=observed.inspection;
        if(!inspection||inspection.payload_verified===false||inspection.metadata_only===true||observed.error
            ||observed.diagnosis?.pending_activation||record(inspection.lifecycle??{}).pending||this.options.managed)return this.ensure();
        if(record(inspection.lifecycle??{}).explicitly_stopped===true)throw new ServiceError('service_explicitly_stopped');
        const binding=this.binding(inspection,observed.installDir);
        if(binding.version!==this.version)throw new ServiceError('explicit_runtime_upgrade_or_matching_extension_required');
        const client=this.client(binding);
        try{
            const reply=await client.rpc('health','health');this.active();
            const health=record(reply.observation);
            if(health.version===this.version&&health.runtime_sha256===binding.runtimeSha256&&record(health.policy).accepting===true)return binding;
        }catch(error){this.active();}finally{client.dispose();}
        return this.ensure();
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
                if(selection?.active&&runtimeCleanBreakRequired(runtimeVersion(record(inspected?.approval).version),this.version,inspected?.clean_break))throw new ServiceError('clean_break_required_before_connection');
                if(!explicitStart&&record(inspected?.lifecycle??{}).explicitly_stopped===true)throw new ServiceError('service_explicitly_stopped');
                await this.invoke(bundle.main,['runtime',explicitStart?'start':'ensure','--install-dir',installDir],180000);
                if(managed)await this.invoke(bundle.main,['runtime','prepare-client','--install-dir',installDir],180000);
                const inspection=await this.invoke(bundle.main,['runtime','inspect','--install-dir',installDir]);
                return this.binding(inspection,installDir);
            }catch(error){
                if(error instanceof ServiceError&&['runtime_client_no_longer_authorized','managed_runtime_policy_mismatch','managed_runtime_state_mismatch','service_explicitly_stopped','selected_runtime_payload_unverified','explicit_runtime_upgrade_or_matching_extension_required','clean_break_required_before_connection'].includes(error.code))throw error;
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
            launcher:inspection.launcher===null?null:safePath(inspection.launcher),
            universalLauncher:typeof inspection.universal_launcher==='string'?safePath(inspection.universal_launcher):null,runtimeSha256,version,
            lifecycle:managed?.lifecycle==='external'?'external':managed?'user-on-demand':'on-demand',
            verificationBasis:typeof approval.verification_basis==='string'?approval.verification_basis:'unknown',
            payloadVerified:inspection.payload_verified!==false&&inspection.metadata_only!==true,
            ...(inspection.rebuild!==undefined&&inspection.rebuild!==null?{rebuild:validateRebuildObservation(inspection.rebuild)}:{}),
            ...(inspection.clean_break!==undefined&&inspection.clean_break!==null?{cleanBreak:validateCleanBreakPolicy(inspection.clean_break)}:{})};
    }
    async retirePrevious(binding:RuntimeBinding):Promise<RecordValue|undefined>{
        this.active();
        if(this.options.managed)return;
        const bundle=await this.verified();
        const result=await this.invoke(bundle.main,['runtime','retire-previous','--install-dir',binding.installDir,
            '--expected-active',binding.runtimeSha256],180000);
        if(result.active!==binding.runtimeSha256)throw new ServiceError('crossed_runtime_retirement_receipt',true,result);
        if(bundle.helper===undefined){
            const processes=result.processes&&typeof result.processes==='object'&&!Array.isArray(result.processes)?result.processes as RecordValue:{},runtimes=result.retired_runtimes;
            if(result.complete!==true||result.selected_service_restarted!==false||!Array.isArray(runtimes)||runtimes.length>4||runtimes.some(value=>typeof value!=='string'||!/^[a-f0-9]{64}$/.test(value)||value===binding.runtimeSha256)||processes.scope!=='verified_superseded_runtime_images'||processes.remaining!==0||!Array.isArray(processes.observations)||processes.observations.length>20)throw new ServiceError('previous_runtime_cleanup_unconfirmed',true,result);
        }
        return result;
    }
    /** Read-only native proof of this installation's exact legacy command lineage. */
    async inspectLegacyLaunchers(binding:RuntimeBinding,purpose?:'predecessor-cleanup'):Promise<RecordValue>{
        this.active();if(binding.payloadVerified===false)throw new ServiceError('selected_runtime_payload_unverified');
        const bundle=await this.verified(),result=await this.invoke(bundle.main,['runtime','inspect-legacy-launchers','--install-dir',binding.installDir,'--expected-active',binding.runtimeSha256,...(purpose?['--purpose',purpose]:[])],60000);
        if(result.schema!==1||result.active!==binding.runtimeSha256)throw new ServiceError('legacy_launcher_inspection_unconfirmed',false,result);
        for(const field of ['launcher','universal_launcher']){const item=result[field];if(item===null)continue;const value=record(item);safePath(value.path);identifier(value.sha256,32);}
        if(result.legacy_runtime_images!==undefined){if(!Array.isArray(result.legacy_runtime_images)||result.legacy_runtime_images.length>20)throw new ServiceError('legacy_launcher_inspection_unconfirmed',false,result);for(const item of result.legacy_runtime_images){const value=record(item);safePath(value.path);identifier(value.sha256,32);identifier(value.runtime_sha256,32);if(value.runtime_sha256===binding.runtimeSha256||path.relative(process.platform==='win32'?path.toNamespacedPath(String(value.path)).toLowerCase():String(value.path),process.platform==='win32'?path.toNamespacedPath(binding.executable).toLowerCase():binding.executable)==='')throw new ServiceError('legacy_launcher_inspection_unconfirmed',false,result);}}
        return result;
    }
    /** Final upgrade gate: native ownership/identity checks stop only verified old
     * launcher images and prove exact launcher absence. Keep the full audit. */
    async retireLegacyLaunchers(binding:RuntimeBinding):Promise<RecordValue>{
        this.active();if(binding.payloadVerified===false)throw new ServiceError('selected_runtime_payload_unverified');
        const bundle=await this.verified(),result=await this.invoke(bundle.main,['runtime','retire-legacy-launchers','--install-dir',binding.installDir,'--expected-active',binding.runtimeSha256],180000);
        const processes=result.processes&&typeof result.processes==='object'&&!Array.isArray(result.processes)?result.processes as RecordValue:{};
        if(result.schema!==1||result.active!==binding.runtimeSha256||result.legacy_launchers_retired!==true||result.launcher!==null||result.universal_launcher!==null||processes.scope!=='verified_launcher_images'||processes.remaining!==0||!Array.isArray(processes.observations))throw new ServiceError('legacy_launcher_cleanup_unconfirmed',true,result);
        return result;
    }
    async inspectLegacyCaches(binding:RuntimeBinding,contextGlobalStorage:string,originLocatorsFile?:string):Promise<RecordValue>{
        this.active();safePath(contextGlobalStorage);if(originLocatorsFile)safePath(originLocatorsFile);
        const bundle=await this.verified(),result=await this.invoke(bundle.main,['runtime','inspect-legacy-caches','--install-dir',binding.installDir,'--expected-active',binding.runtimeSha256,'--global-storage',contextGlobalStorage,...(originLocatorsFile?['--origin-locators-file',originLocatorsFile]:[])],180000);
        try{return validateLegacyCachePreparation(result,binding,contextGlobalStorage);}catch{throw new ServiceError('legacy_cache_inspection_unconfirmed',false,result);}
    }
    /** Captures a native-owned immutable inventory before any original config
     * changes. Resumption returns its accepted preimage, never transformed bytes. */
    async prepareLegacyCaches(binding:RuntimeBinding,contextGlobalStorage:string,originLocatorsFile?:string):Promise<RecordValue>{
        this.active();if(binding.payloadVerified===false)throw new ServiceError('selected_runtime_payload_unverified');
        safePath(contextGlobalStorage);if(originLocatorsFile)safePath(originLocatorsFile);
        const bundle=await this.verified(),result=await this.invoke(bundle.main,['runtime','prepare-legacy-caches','--install-dir',binding.installDir,'--expected-active',binding.runtimeSha256,'--global-storage',contextGlobalStorage,...(originLocatorsFile?['--origin-locators-file',originLocatorsFile]:[])],180000);
        try{return validateLegacyCachePreparation(result,binding,contextGlobalStorage);}catch{throw new ServiceError('legacy_cache_inspection_unconfirmed',true,result);}
    }
    async reprepareLegacyCaches(binding:RuntimeBinding,previous:RecordValue,originLocatorsFile?:string):Promise<RecordValue>{
        this.active();const prepared=validateLegacyCachePreparation(previous,binding);if(originLocatorsFile)safePath(originLocatorsFile);
        const bundle=await this.verified(),result=await this.invoke(bundle.main,['runtime','reprepare-legacy-caches','--install-dir',binding.installDir,'--expected-active',binding.runtimeSha256,'--global-storage',String(prepared.context_global_storage),'--expected-inspection-sha256',String(prepared.inspection_sha256),...(originLocatorsFile?['--origin-locators-file',originLocatorsFile]:[])],180000);
        try{
            const next=validateLegacyCachePreparation(result,binding,String(prepared.context_global_storage));
            if(next.scope!==prepared.scope||(prepared.global_storage_roots as string[]).some(root=>!(next.global_storage_roots as string[]).includes(root))||(prepared.caches as unknown[]).some(cache=>!(next.caches as unknown[]).some(value=>isDeepStrictEqual(value,cache)))||(prepared.origin_proofs as unknown[]).some(proof=>!(next.origin_proofs as unknown[]).some(value=>isDeepStrictEqual(value,proof)))||(prepared.origin_locators as unknown[]).some(locator=>!(next.origin_locators as unknown[]).some(value=>isDeepStrictEqual(value,locator))))throw Error('changed_accepted_cache_preimage');
            return next;
        }catch{throw new ServiceError('legacy_cache_reprepare_unconfirmed',true,result);}
    }
    /** One native batch owns grouping, archive publication and original claims.
     * Frontend paths never decide whether two declarations share an index. */
    async adoptLegacyIndexes(binding:RuntimeBinding,preparation:RecordValue,batchOperation:string,locatorsFile:string,acceptedLocators?:readonly LegacyOriginLocator[]):Promise<RecordValue>{
        this.active();const prepared=validateLegacyCachePreparation(preparation,binding);identifier(batchOperation,16);safePath(locatorsFile);
        const available=(prepared.adoption_locators as unknown[]).map(validateLegacyOriginLocator),locators=acceptedLocators?[...acceptedLocators].map(validateLegacyOriginLocator):available;
        if(locators.some(locator=>!available.some(value=>legacyOriginLocatorIdentity(value)===legacyOriginLocatorIdentity(locator)))||new Set(locators.map(legacyOriginLocatorIdentity)).size!==locators.length)throw new ServiceError('legacy_batch_input_unconfirmed');
        if(!locators.length||locators.length>32)throw new ServiceError('legacy_adoption_claim_budget_exceeded');
        const bundle=await this.verified(),result=await this.invoke(bundle.main,['runtime','adopt-legacy-indexes','--install-dir',binding.installDir,'--expected-active',binding.runtimeSha256,'--operation-id',batchOperation,'--locators-file',locatorsFile,'--expected-cache-inspection-sha256',String(prepared.inspection_sha256)],1850000);
        try{
            if(result.schema!==1||result.active!==binding.runtimeSha256||result.status!=='ready'||result.operation_id!==batchOperation||result.batch_operation_id!==batchOperation||result.cache_inspection_sha256!==prepared.inspection_sha256||typeof result.retained_completed_operation!=='boolean'||!Array.isArray(result.groups)||!result.groups.length||result.groups.length>32||!Array.isArray(result.claims)||result.claims.length!==locators.length)throw Error('invalid_adoption_batch');
            const groups=new Map<string,RecordValue>();
            for(const input of result.groups){
                const group=record(input);identifier(group.operation_id,16);
                if(groups.has(String(group.operation_id))||group.schema!==1||group.active!==binding.runtimeSha256||group.status!=='ready'||group.batch_operation_id!==batchOperation||group.source_availability!=='not_observed_saved_only')throw Error('invalid_adoption_group');
                const original=record(group.original_registration),registration=record(group.registration),grant=record(group.grant),provenance=record(grant.provenance),adoption=record(provenance.adoption),plan=record(adoption.plan),setup=record(plan.setup),key=record(setup.key),publication=record(adoption.publication),receipt=record(group.adoption_receipt),completed=record(receipt.setup);
                if(provenance.kind!=='imported_standalone_adoption'||!isDeepStrictEqual(plan.original_registration,original)||!isDeepStrictEqual(setup.registration,registration)||plan.config_sha256!==group.original_config_sha256||key.nonce!==group.operation_id||receipt.schema!==1||receipt.plan_digest!==publication.plan_digest||completed.schema!==1||completed.stage!=='ready'||!isDeepStrictEqual(completed.request,setup)||completed.operation_id!==adoption.operation_id||completed.legacy_adoption!==receipt.plan_digest||record(completed.preparation).legacy_adoption!==receipt.plan_digest||!isDeepStrictEqual(record(completed.completion).legacy_adoption,publication))throw Error('crossed_adoption_publication');
                validateNativeLegacyBinding({status:'catalog_legacy_binding',observation:{schema:1,namespace:key.namespace,catalog_revision:completed.catalog_revision,project:registration.binding,eligibility:{kind:'imported_snapshot',registration,grant}}},String(registration.binding),Number(completed.catalog_revision));
                const members=record(grant.grant).memberships;
                if(!Array.isArray(members)||members.length!==1||record(record(members[0]).membership).index_id!==completed.index_id||record(members[0]).source_revision!==completed.source_revision)throw Error('crossed_adoption_membership');
                record(group.conversion);groups.set(String(group.operation_id),group);
            }
            const expected=new Set(locators.map(legacyOriginLocatorIdentity)),seen=new Set<string>(),groupClaims=new Map<string,Set<string>>();
            for(const input of result.claims){
                const mapped=record(input),locator=validateLegacyOriginLocator(mapped.locator),key=legacyOriginLocatorIdentity(locator),group=groups.get(String(mapped.group_operation_id)),claim=record(mapped.claim);identifier(mapped.origin_sha256,32);identifier(mapped.config_sha256,32);
                if(!expected.has(key)||seen.has(key)||!group)throw Error('crossed_adoption_claim_locator');seen.add(key);
                const adoption=record(record(record(group.grant).provenance).adoption),claims=adoption.configuration_claims as RecordValue[];
                if(mapped.origin_sha256!==claim.origin_sha256||mapped.config_sha256!==claim.config_sha256||claim.original_server!==locator.server||!isDeepStrictEqual(mapped.original_registration,group.original_registration)||!isDeepStrictEqual(mapped.registration,group.registration)||!claims.some(candidate=>isDeepStrictEqual(candidate,claim)))throw Error('crossed_adoption_claim');
                const container=locator.container_path??[locator.format==='codex_toml'?'mcp_servers':locator.format==='vscode_servers_json'?'servers':locator.format==='open_code_json'?'mcp':'mcpServers'];
                if(!isDeepStrictEqual(claim.container_path,container)||!(prepared.origin_proofs as unknown[]).some(value=>{const origin=record(value);return sameNativeLocation(String(origin.config_path),locator.path)&&origin.server===locator.server&&origin.config_sha256===claim.config_sha256&&origin.executable_sha256===claim.executable_sha256&&origin.selected_entry_sha256===claim.selected_entry_sha256&&isDeepStrictEqual(origin.container_path,container);}))throw Error('crossed_adoption_origin');
                const origins=groupClaims.get(String(group.operation_id))??new Set<string>();if(origins.has(String(claim.origin_sha256)))throw Error('duplicate_adoption_claim');origins.add(String(claim.origin_sha256));groupClaims.set(String(group.operation_id),origins);
            }
            for(const [id,group]of groups){const claims=record(record(record(group.grant).provenance).adoption).configuration_claims as RecordValue[],origins=groupClaims.get(id);if(!origins||origins.size!==claims.length||claims.some(claim=>!origins.has(String(claim.origin_sha256))))throw Error('incomplete_adoption_group_claims');}
            const maintenance=record(result.maintenance);identifier(maintenance.execution_sha256,32);count(maintenance.observed_unix_ms);
            if(maintenance.operation!==batchOperation||typeof maintenance.originally_running!=='boolean'||typeof maintenance.originally_explicitly_stopped!=='boolean'||maintenance.restarted!==(maintenance.originally_running&&!maintenance.originally_explicitly_stopped))throw Error('unconfirmed_adoption_restoration');
            if(maintenance.restarted)identifier(maintenance.service_instance,16);else if(maintenance.service_instance!==null)throw Error('unconfirmed_adoption_restoration');
            const withdrawal=record(result.withdrawal),held=record(withdrawal.held),restored=record(withdrawal.restored);
            if(held.schema!==1||held.operation_id!==batchOperation||held.inspection_sha256!==prepared.inspection_sha256||held.scope!=='verified_legacy_cache_images'||held.remaining!==0||!Array.isArray(held.image_roster)||!Array.isArray(held.image_exclusions)||!Array.isArray(held.observations)||restored.schema!==1||restored.operation_id!==batchOperation||restored.inspection_sha256!==prepared.inspection_sha256||restored.original_image_paths_restored!==true||restored.retirement_pending!==true)throw Error('unconfirmed_adoption_cache_restore');
            return result;
        }catch{throw new ServiceError('legacy_index_adoption_unconfirmed',true,result);}
    }
    /** Final absence proof consumes exactly the native accepted inventory. It
     * cannot discover additional roots after original declarations were saved. */
    async retireLegacyCaches(binding:RuntimeBinding,preparation:RecordValue):Promise<RecordValue>{
        this.active();const prepared=validateLegacyCachePreparation(preparation,binding),bundle=await this.verified();
        const result=await this.invoke(bundle.main,['runtime','retire-legacy-caches','--install-dir',binding.installDir,'--expected-active',binding.runtimeSha256,'--expected-inspection-sha256',String(prepared.inspection_sha256)],180000);
        const processes=result.processes&&typeof result.processes==='object'&&!Array.isArray(result.processes)?result.processes as RecordValue:{};
        if(result.schema!==1||result.active!==binding.runtimeSha256||result.inspection_sha256!==prepared.inspection_sha256||result.scope!==prepared.scope||!isDeepStrictEqual(result.global_storage_roots,prepared.global_storage_roots)||result.caches_retired!==true||result.remaining!==0||processes.scope!=='verified_legacy_cache_images'||processes.remaining!==0||!Array.isArray(processes.observations)||!Array.isArray(processes.image_roster)||!Array.isArray(processes.image_exclusions)||!Array.isArray(result.retired_caches)||!isDeepStrictEqual(result.retired_caches,prepared.caches))throw new ServiceError('legacy_cache_cleanup_unconfirmed',true,result);
        return result;
    }
    /** Native saved-scope observation only; not permission to start or broker evidence. */
    async sourceScope(binding:RuntimeBinding,root:string,collection:string|null):Promise<NativeSourceScope> {
        this.active();safePath(root);
        if(binding.version!==this.version||binding.payloadVerified===false)throw new ServiceError('selected_runtime_payload_unverified');
        if(collection!==null&&(!/^[A-Za-z0-9._-]{1,128}$/.test(collection)||['.','..'].includes(collection)))throw new ServiceError('invalid_project_registration');
        const identity=exactBindingIdentity(binding);
        const response=await this.invoke(binding.executable,['runtime','source-scope','--install-dir',binding.installDir,
            '--expected-control',binding.controlDir,'--expected-runtime',binding.runtimeSha256,'--source-root',root,
            ...(collection===null?[]:['--index-collection',collection])],15000);
        this.active();
        if(identity!==exactBindingIdentity(binding)||response.install_dir!==binding.installDir||response.control_dir!==binding.controlDir||response.runtime_sha256!==binding.runtimeSha256)throw new ServiceError('crossed_source_scope_observation');
        const raw=record(response.scope);
        if(Object.keys(raw).sort().join(',')!=='collection,fencing_generation,namespace,observed_unix_ms,outcome,registration,revision,root,schema'||raw.schema!==1||raw.collection!==collection||!['registered','missing','removed','ambiguous'].includes(String(raw.outcome)))throw new ServiceError('invalid_source_scope_observation');
        const canonical=safePath(raw.root);
        if(!sameNativeLocation(canonical,root))throw new ServiceError('crossed_source_scope_observation');
        const namespace=raw.namespace===null?null:identifier(raw.namespace,16);
        const revision=raw.revision===null?null:(count(raw.revision),raw.revision as number);
        const observed=raw.observed_unix_ms===null?null:(count(raw.observed_unix_ms),raw.observed_unix_ms as number);
        if((namespace===null)!==(revision===null)||namespace===null&&raw.outcome!=='missing')throw new ServiceError('invalid_source_scope_observation');
        const registration=raw.registration===null?null:projectRegistration(raw.registration);
        const generation=raw.fencing_generation===null?null:(count(raw.fencing_generation),raw.fencing_generation as number);
        if(raw.outcome==='registered'){
            if(!registration||namespace===null||generation===null||!sameNativeLocation(registration.root,canonical)||collection!==null&&registration.collection!==collection)throw new ServiceError('crossed_source_scope_observation');
        }else if(registration!==null||generation!==null)throw new ServiceError('invalid_source_scope_observation');
        if(registration)Object.freeze(registration);
        const value:NativeSourceScope=Object.freeze({root:canonical,collection,namespace,revision,observed_unix_ms:observed,outcome:raw.outcome as NativeSourceScope['outcome'],registration,fencing_generation:generation});
        exactScopeOwners.set(value,{binding:identity,root,collection});return value;
    }
    /** Metadata-only predecessor cleanup adapter. General UI/evidence clients
     * keep their strict running-version gate. No job/grant mutation is exposed. */
    async migrationClient(binding:RuntimeBinding):Promise<Pick<ServiceClient,'projects'|'dispose'>>{
        this.active();
        if(binding.payloadVerified===false||!automaticUpgrade(binding.version,this.version))throw new ServiceError('legacy_cleanup_protocol_unverified');
        const bundle=await this.verified(),inspection=await this.invoke(bundle.main,['runtime','inspect','--install-dir',binding.installDir],60000);
        const selected=this.binding(inspection,binding.installDir);
        if(inspection.payload_verified!==true||inspection.metadata_only===true||inspection.launcher!==null||selected.runtimeSha256!==binding.runtimeSha256||selected.version!==binding.version||selected.executable!==binding.executable||selected.controlDir!==binding.controlDir)throw new ServiceError('legacy_cleanup_selection_changed',false,inspection);
        const directory=path.dirname(binding.executable),contents=path.dirname(directory),app=path.dirname(contents),runtimeRoot=path.basename(directory)==='MacOS'&&path.basename(contents)==='Contents'&&path.basename(app)==='AIRK.app'?path.dirname(app):directory;
        const pair=(await boundedJson(path.join(runtimeRoot,'airk-runtime.json'),65536)).value;
        if(pair.schema!==2||pair.version!==binding.version)throw new ServiceError('legacy_cleanup_protocol_unverified');identifier(pair.airk_sha256,32);
        const capabilities=await this.invoke(binding.executable,['runtime','self-test'],60000);
        if(capabilities.version!==binding.version||capabilities.protocol!==1||capabilities.metadata_console!==1||capabilities.self_test!=='passed')throw new ServiceError('legacy_cleanup_protocol_unverified',false,capabilities);
        // Metadata calls bind the caller's compiled version/image to the owner.
        // Use the verified predecessor for its protocol; the new installer
        // above still verifies the selected payload before any old code runs.
        const raw=await this.invoke(binding.executable,['service','rpc','--runtime-dir',binding.controlDir,'--stdin'],60000,JSON.stringify('health'));
        const health=record(raw.observation),clock=record(health.catalog_clock);
        if(raw.status!=='health'||health.version!==binding.version||health.protocol!==1||health.runtime_sha256!==binding.runtimeSha256||health.executable_sha256!==pair.airk_sha256||record(health.policy).accepting!==true)throw new ServiceError('legacy_cleanup_health_unconfirmed',false,raw);
        identifier(health.instance,16);identifier(clock.namespace,16);
        const endpoint=validateEndpoint(await this.invoke(binding.executable,['runtime','http-endpoint','--install-dir',binding.installDir],60000));
        if(endpoint.service_instance!==health.instance||endpoint.namespace!==clock.namespace||endpoint.executable_sha256!==pair.airk_sha256||endpoint.state!=='ready')throw new ServiceError('legacy_cleanup_endpoint_unconfirmed',false,record(endpoint));
        this.active();let disposed=false;
        const client=new ServiceClient({dispose:()=>{disposed=true;},request:async method=>{
            this.active();if(disposed)throw new ServiceError('legacy_cleanup_reader_disposed');
            const observed=await this.invoke(binding.executable,['service','rpc','--runtime-dir',binding.controlDir,'--stdin'],60000,JSON.stringify('health'));
            const live=record(observed.observation);
            if(observed.status!=='health'||live.instance!==health.instance||live.version!==binding.version||live.protocol!==1||live.runtime_sha256!==binding.runtimeSha256||live.executable_sha256!==pair.airk_sha256)throw new ServiceError('legacy_cleanup_health_unconfirmed',false,observed);
            // The facade exposes Console metadata pages only. Existing canonical
            // ServiceClient parsing retains their complete native response.
            return this.invoke(binding.executable,['service','rpc','--runtime-dir',binding.controlDir,'--stdin'],60000,JSON.stringify(method));
        }});
        return {projects:(after,revision,limit,inactive)=>client.projects(after,revision,limit,inactive),dispose:()=>client.dispose()};
    }
    client(binding:RuntimeBinding,timeout=20000):ServiceClient {
        this.active();
        if(binding.version!==this.version)throw new ServiceError('service_runtime_version_mismatch');
        if(binding.payloadVerified===false)throw new ServiceError('selected_runtime_payload_unverified');
        if(runtimeCleanBreakRequired(binding.version,this.version,binding.cleanBreak))throw new ServiceError('clean_break_required_before_connection');
        return new ServiceClient(new NativeControlTransport(binding.executable,['service','console','--runtime-dir',binding.controlDir,
            '--runtime-sha256',binding.runtimeSha256],timeout));
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
    private async legacyAdmin(action:string,input?:RecordValue,index?:string,admit?:()=>void,after?:string):Promise<RecordValue|null>{
        this.active();admit?.();const bundle=await this.verified();this.active();admit?.();
        const location=await this.invoke(bundle.main,['runtime','location','--channel',this.options.channel]);
        const install=this.options.installDir??safePath(location.install_dir);
        const args=['runtime',`legacy-retirement-${action}`,'--install-dir',install];
        if(index){if(!/^idx_[0-9a-f]{32}$/.test(index))throw new ServiceError('invalid_legacy_retirement_index');args.push('--index-id',index);}
        if(after){if(!/^idx_[0-9a-f]{32}$/.test(after))throw new ServiceError('invalid_legacy_retirement_continuation');args.push('--after',after);}
        // Use the paired verified bundled candidate, never an older selected predecessor.
        // Native admission must independently match the saved installer identity.
        this.active();admit?.(); // No await between the session's final Stop/trust check and dispatch.
        const result=await this.run(bundle.main,args,180000,input?JSON.stringify(input):undefined);this.active();
        if(action==='receipt'){
            if(result.schema!==1||!Object.hasOwn(result,'observation'))throw new ServiceError('invalid_legacy_retirement_observation');
            return result.observation===null?null:validateRetirementReply(result.observation);
        }
        return action==='candidates'?record(result):validateRetirementReply(result);
    }
    async legacyRetirementCandidates(after?:string):Promise<RecordValue>{return record(await this.legacyAdmin('candidates',undefined,undefined,undefined,after));}
    async legacyRetirementReview(index:string):Promise<RecordValue>{return record(await this.legacyAdmin('review',undefined,index));}
    async legacyRetirementAuthorize(review:RecordValue,admit:()=>void):Promise<RecordValue>{
        return record(await this.legacyAdmin('authorize',{preview:review.preview,review_digest:review.review_digest},undefined,admit));
    }
    async legacyRetirementReceipt(request:RecordValue):Promise<RecordValue|null>{return this.legacyAdmin('receipt',request);}
    async legacyRetirementResume(request:RecordValue,admit:()=>void):Promise<RecordValue>{return record(await this.legacyAdmin('resume',request,undefined,admit));}
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
    async registerProject(binding:RuntimeBinding,review:ProjectRegistrationReview,setupWatch?:boolean,onlyIfMissing=false):Promise<ProjectRegistrationReview>{
        this.active();
        if(!this.projectReviews.has(review)||review.installDir!==binding.installDir||review.controlDir!==binding.controlDir
            ||review.runtimeSha256!==binding.runtimeSha256)throw new ServiceError('project_registration_not_reviewed');
        const registration={binding:review.binding,root:review.root,data_root:review.data_root,collection:review.collection};
        const client=this.client(binding);
        try {
            const response=await client.rpc({reserve_project:{project:registration,...(setupWatch===undefined?{}:{setup_watch:setupWatch}),...(onlyIfMissing?{only_if_missing:true}:{})}},'registered');this.active();
            if(JSON.stringify(projectRegistration(record(response.project).registration))!==JSON.stringify(registration))throw new ServiceError('crossed_project_registration');
        }finally{client.dispose();}
        if(setupWatch!==undefined){this.projectReviews.delete(review);return review;}
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
    /** Setup/enrollment only. Evidence requests subsequently use the shared HTTPS
     * listener and never execute this native command. Native validates the input
     * file's owner-private ACL and the live selected grant before returning refs. */
    async httpConnection(binding:RuntimeBinding,selection:HttpSelection,selectionFile:string,connectionId:string,client:string,explorerDescriptor?:string):Promise<HttpConnectionInfo>{
        if(runtimeCleanBreakRequired(binding.version,this.version,binding.cleanBreak))throw new ServiceError('clean_break_required_before_connection');
        this.active();if(!/^[a-f0-9]{32}$/.test(connectionId)||!/^[A-Za-z0-9._-]{1,64}$/.test(client))throw new ServiceError('invalid_http_enrollment_identity');
        safePath(selectionFile);const bundle=await this.verified();
        const response=await this.invoke(bundle.main,['runtime','http-connection','--install-dir',binding.installDir,'--selection-file',selectionFile,'--connection-id',connectionId,'--client',client,...(explorerDescriptor?['--explorer-descriptor',safePath(explorerDescriptor)]:[])],60000);
        this.active();const info=validateHttpConnection(response,selection,path.join(binding.controlDir,'http-1'));if(info.connection_id!==connectionId)throw new ServiceError('crossed_http_connection_identity');return info;
    }
    async httpLegacyConnection(binding:RuntimeBinding,registration:{binding:string;root:string;data_root:string;collection:string},registrationFile:string,receipt:string,connectionId:string,client:string,explorerDescriptor?:string,originSha256?:string,reattachConnectionId?:string):Promise<HttpConnectionInfo>{
        if(runtimeCleanBreakRequired(binding.version,this.version,binding.cleanBreak))throw new ServiceError('clean_break_does_not_migrate_legacy_grants');
        this.active();if(originSha256)identifier(originSha256,32);if(reattachConnectionId)identifier(reattachConnectionId,16);if(!/^[a-f0-9]{32}$/.test(connectionId)||!/^[a-f0-9]{64}$/.test(receipt)||!/^[A-Za-z0-9._-]{1,64}$/.test(client))throw new ServiceError('invalid_http_enrollment_identity');
        safePath(registrationFile);const bundle=await this.verified();
        const response=await this.invoke(bundle.main,['runtime','http-legacy-connection','--install-dir',binding.installDir,'--registration-file',registrationFile,'--configuration-receipt',receipt,...(originSha256?['--origin-sha256',originSha256]:[]),'--connection-id',connectionId,'--client',client,...(explorerDescriptor?['--explorer-descriptor',safePath(explorerDescriptor)]:[]),...(reattachConnectionId?['--reattach-connection-id',reattachConnectionId]:[])],60000);this.active();
        if(response.status==='review_required')throw new ServiceError('legacy_selection_review_required',false,response);
        let info:HttpConnectionInfo;try{info=validateHttpConnection(response,undefined,path.join(binding.controlDir,'http-1'));}catch{throw new ServiceError('http_legacy_connection_unconfirmed',true,response);}
        if(info.connection_id!==connectionId||info.selection.kind!=='imported_legacy'||!matchesImportedHttpOrigin(info.selection.selection,registration,receipt))throw new ServiceError('crossed_http_legacy_connection',true,response);
        if(originSha256){const provenance=record(info.selection.selection.grant.provenance),adoption=record(provenance.adoption);if(provenance.kind!=='imported_standalone_adoption'||!Array.isArray(adoption.configuration_claims)||!adoption.configuration_claims.some(value=>{const claim=record(value);return claim.origin_sha256===originSha256&&claim.config_sha256===receipt;}))throw new ServiceError('crossed_http_legacy_origin',true,response);}
        return info;
    }
    async httpLegacyUnionConnection(binding:RuntimeBinding,registrations:{registration:{binding:string;root:string;data_root:string;collection:string};configuration_receipt:string}[],file:string,connectionId:string,client:string,explorerDescriptor?:string):Promise<HttpConnectionInfo>{
        if(runtimeCleanBreakRequired(binding.version,this.version,binding.cleanBreak))throw new ServiceError('clean_break_does_not_migrate_legacy_grants');
        this.active();if(registrations.length<2||registrations.length>32||!/^[a-f0-9]{32}$/.test(connectionId)||!/^[A-Za-z0-9._-]{1,64}$/.test(client))throw new ServiceError('invalid_http_enrollment_identity');
        safePath(file);const bundle=await this.verified();
        const response=await this.invoke(bundle.main,['runtime','http-legacy-union-connection','--install-dir',binding.installDir,'--registrations-file',file,'--connection-id',connectionId,'--client',client,...(explorerDescriptor?['--explorer-descriptor',safePath(explorerDescriptor)]:[])],60000);this.active();
        if(response.status==='review_required')throw new ServiceError('legacy_selection_review_required',false,response);
        let info:HttpConnectionInfo;try{info=validateHttpConnection(response,undefined,path.join(binding.controlDir,'http-1'));}catch{throw new ServiceError('http_legacy_connection_unconfirmed',true,response);}
        if(info.connection_id!==connectionId||info.selection.kind!=='imported_legacy_union'||info.selection.selection.selections.length!==registrations.length||info.selection.selection.selections.some((member,index)=>!matchesImportedHttpOrigin(member,registrations[index].registration,registrations[index].configuration_receipt)))throw new ServiceError('crossed_http_legacy_union',true,response);return info;
    }
    /** Typed ownership lookup never issues/renews credentials. Absence is valid
     * only when the native private ledger proves this installation never owned it. */
    async lookupPredecessorHttpOwnership(binding:RuntimeBinding,connectionId:string,scope:PredecessorHttpScope):Promise<PredecessorHttpLookup>{
        this.active();identifier(connectionId,16);identifier(scope.predecessor,32);if(scope.operationId!==undefined)identifier(scope.operationId,16);
        const bundle=await this.verified();
        const result=await this.invoke(bundle.main,['runtime','lookup-http-connection','--install-dir',binding.installDir,'--expected-active',binding.runtimeSha256,'--connection-id',connectionId,'--purpose','predecessor-cleanup','--predecessor',scope.predecessor],60000);
        try{return validatePredecessorHttpLookup(result,binding.runtimeSha256,connectionId,scope);}catch{throw new ServiceError('predecessor_http_ownership_unconfirmed',false,result);}
    }
    async lookupHttpConnection(binding:RuntimeBinding,connectionId:string,reattachment?:{connection:HttpConnectionInfo;client:string}):Promise<RecordValue>{
        this.active();identifier(connectionId,16);const bundle=await this.verified();
        const result=await this.invoke(bundle.main,['runtime','lookup-http-connection','--install-dir',binding.installDir,'--expected-active',binding.runtimeSha256,'--connection-id',connectionId],60000);
        if(result.schema!==1||result.active!==binding.runtimeSha256||result.connection_id!==connectionId)throw new ServiceError('http_connection_lookup_unconfirmed',false,result);
        if(result.status==='not_owned'){if(result.connection!==undefined||result.attachment_retirement!==undefined)throw new ServiceError('http_connection_lookup_unconfirmed',false,result);return result;}
        if(result.status==='unavailable'){
            if(typeof result.code!=='string'||!/^owned_connection_(?:revoked|expired|unavailable|attachment_retired)$/.test(result.code))throw new ServiceError('http_connection_lookup_unconfirmed',false,result);
            if(result.code==='owned_connection_attachment_retired'&&reattachment){
                if(reattachment.connection.connection_id!==connectionId)throw new ServiceError('http_connection_lookup_unconfirmed',false,result);
                try{validateAttachmentRetirement(result.attachment_retirement,reattachment.connection,reattachment.client);}catch{throw new ServiceError('http_attachment_retirement_unconfirmed',false,result);}
                return result;
            }
            throw new ServiceError(result.code,false,result);
        }
        if(result.attachment_retirement!==undefined)throw new ServiceError('http_connection_lookup_unconfirmed',false,result);
        if(result.status!=='owned')throw new ServiceError('http_connection_lookup_unconfirmed',false,result);
        const connection=validateHttpConnection(result.connection,undefined,path.join(binding.controlDir,'http-1'));
        if(connection.connection_id!==connectionId)throw new ServiceError('http_connection_lookup_unconfirmed',false,result);
        return {...result,connection};
    }
    async httpConnectionInfo(binding:RuntimeBinding,connectionId:string):Promise<HttpConnectionInfo>{
        this.active();if(!/^[a-f0-9]{32}$/.test(connectionId))throw new ServiceError('invalid_http_connection_identity');
        const bundle=await this.verified(),response=await this.invoke(bundle.main,['runtime','http-connection-info','--install-dir',binding.installDir,'--connection-id',connectionId],60000);this.active();
        const info=validateHttpConnection(response,undefined,path.join(binding.controlDir,'http-1'));if(info.connection_id!==connectionId)throw new ServiceError('crossed_http_connection_identity');return info;
    }
    async httpTrust(binding:RuntimeBinding,action:'inspect'|'install'|'remove',expectedCa?:string):Promise<RecordValue>{
        this.active();if(action!=='inspect'&&!expectedCa||expectedCa!==undefined&&!/^[a-f0-9]{64}$/.test(expectedCa))throw new ServiceError('http_trust_review_required');
        const bundle=await this.verified(),response=await this.invoke(bundle.main,['runtime','http-trust','--install-dir',binding.installDir,'--action',action,...(expectedCa?['--expected-ca-sha256',expectedCa]:[])],60000);this.active();
        if(Buffer.byteLength(JSON.stringify(response))>16384||response.schema!==1||typeof response.ca_sha256!=='string'||!/^[a-f0-9]{64}$/.test(response.ca_sha256)||expectedCa!==undefined&&response.ca_sha256!==expectedCa||typeof response.present!=='boolean'||typeof response.trusted!=='boolean'||typeof response.owned!=='boolean'||typeof response.outcome!=='string'||typeof response.guidance!=='string')throw new ServiceError('invalid_native_http_trust_receipt');
        return response;
    }
    async httpIssuerRotation(binding:RuntimeBinding,action:IssuerRotationAction,expected?:IssuerRotationStatus,operationId?:string):Promise<IssuerRotationStatus>{
        this.active();if(!['status','prepare','trust-new','commit','finalize','abort'].includes(action))throw new ServiceError('invalid_http_rotation_action');
        const args=['runtime','http-issuer-rotation','--install-dir',binding.installDir,'--action',action];
        if(action==='prepare'){
            if(!expected?.current_endpoint||!expected.current_descriptor_sha256||!operationId||!/^[a-f0-9]{32}$/.test(operationId))throw new ServiceError('http_rotation_review_required');
            args.push('--operation-id',operationId,'--expected-ca-sha256',expected.current_endpoint.ca_sha256,'--expected-descriptor-sha256',expected.current_descriptor_sha256);
        }else if(action!=='status'){
            if(!expected?.rotation||!expected.receipt_sha256)throw new ServiceError('http_rotation_review_required');
            args.push('--operation-id',expected.rotation.operation_id,'--expected-receipt-sha256',expected.receipt_sha256);
        }
        const bundle=await this.verified(),raw=await this.invoke(bundle.main,args,180000);this.active();
        let result:IssuerRotationStatus;
        try{result=validateIssuerRotation(raw,binding.controlDir,action==='prepare'?undefined:expected);}
        catch(error){throw new ServiceError(error instanceof Error?error.message:'invalid_native_http_rotation_receipt',action!=='status',raw);}
        if(action==='prepare'&&(!result.rotation||result.rotation.operation_id!==operationId||result.rotation.old_ca_sha256!==expected!.current_endpoint!.ca_sha256))throw new ServiceError('crossed_http_rotation_transaction',true,result);
        return result;
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
    async reviewTransition(binding:RuntimeBinding,action:RuntimeTransitionReview['action'],options?:{rebuildIndexes?:boolean}):Promise<RuntimeTransitionReview>{
        if(options?.rebuildIndexes!==undefined&&typeof options.rebuildIndexes!=='boolean'||options?.rebuildIndexes&&action!=='upgrade')throw new ServiceError('invalid_runtime_rebuild_choice');
        return this.inspectTransition(binding,action,false,options?.rebuildIndexes===true);
    }
    private async inspectTransition(binding:RuntimeBinding,action:RuntimeTransitionReview['action'],superseding:boolean,rebuildIndexes=false):Promise<RuntimeTransitionReview>{
        this.active();
        if(binding.lifecycle!=='on-demand'||this.options.managed)throw new ServiceError('runtime_payload_managed_externally');
        if(!['upgrade','rollback','resume'].includes(action))throw new ServiceError('invalid_runtime_transition');
        const observed=await this.discover();
        if(path.relative(binding.installDir,observed.installDir)!==''||!observed.inspection)throw new ServiceError('runtime_installation_changed');
        const current=this.binding(observed.inspection,observed.installDir);
        if(current.payloadVerified===false)throw new ServiceError('repair_selected_payload_before_activation');
        if(current.runtimeSha256!==binding.runtimeSha256||path.relative(current.controlDir,binding.controlDir)!=='')throw new ServiceError('runtime_selection_changed_review_again');
        const selection=record(observed.inspection.selection),pending=observed.diagnosis?.pending_activation;
        const cleanBreak=runtimeCleanBreakRequired(current.version,this.version,current.cleanBreak??observed.diagnosis?.clean_break);
        if(cleanBreak&&action==='rollback')throw new ServiceError('clean_break_required_before_runtime_rollback');
        let expectedActive=current.runtimeSha256,target:string,version=this.version;
        if(cleanBreak){
            // Native reset owns any predecessor activation intent. No old
            // candidate or legacy checkpoint receives execution authority here.
            target=observed.bundle.digest;
        }else if(action==='resume'){
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
        if(action!=='upgrade'&&!cleanBreak){
            const candidate=await this.invoke(observed.bundle.main,['runtime','inspect-candidate','--install-dir',binding.installDir,'--runtime-sha256',target]);
            const approval=record(candidate.approval);
            version=runtimeVersion(approval.version);
            // An older pending candidate may be inspected to abort its pre-probe
            // activation, but must never receive authority to run under this extension.
            const compatible=superseding?automaticUpgrade(version,this.version)
                :version.split('.').slice(0,2).join('.')===this.version.split('.').slice(0,2).join('.');
            if(candidate.metadata_only!==true||candidate.selection_changed!==false||candidate.executed!==false||candidate.runtime_sha256!==target||approval.manifest_sha256!==target
                ||!compatible
                ||!['trusted_distribution','organization_digest'].includes(String(approval.verification_basis)))throw new ServiceError('candidate_not_compatible_with_this_extension');
        }
        const review:RuntimeTransitionReview=Object.freeze({action,installDir:binding.installDir,controlDir:binding.controlDir,
            observedActive:current.runtimeSha256,expectedActive,target,version,...(cleanBreak?{cleanBreak:true,...(current.cleanBreak?{cleanBreakPolicy:structuredClone(current.cleanBreak)}:{})}:rebuildIndexes?{rebuildIndexes:true}:{})});
        if(!superseding)this.transitionReviews.add(review);return review;
    }
    async transition(binding:RuntimeBinding,review:RuntimeTransitionReview,progress?:(message:string,stage:'staging'|'activating')=>void):Promise<RecordValue>{
        this.active();
        if(!this.transitionReviews.has(review)||review.installDir!==binding.installDir||review.controlDir!==binding.controlDir
            ||review.observedActive!==binding.runtimeSha256)throw new ServiceError('runtime_transition_not_reviewed');
        if(binding.lifecycle!=='on-demand'||this.options.managed)throw new ServiceError('runtime_payload_managed_externally');
        const bundle=await this.verified();
        if(review.action==='upgrade'||review.cleanBreak){
            if(bundle.digest!==review.target)throw new ServiceError('reviewed_distribution_changed');
            progress?.('Staging verified runtime files…','staging');
            await this.invoke(bundle.main,['runtime','stage','--source',bundle.source,'--install-dir',binding.installDir,
                '--approved-sha256',bundle.digest,'--verification-basis','trusted-distribution','--channel',this.options.channel],180000);
        }
        progress?.(review.cleanBreak?'Preparing the clean installation and fresh indexes from saved sources…':review.rebuildIndexes?'Checking rebuild prerequisites and activating the selected runtime…':'Checking upgrade prerequisites and activating the selected runtime…','activating');
        const selected=await this.invoke(bundle.main,['runtime','activate','--install-dir',binding.installDir,
            '--runtime-sha256',review.target,'--expected-active',review.expectedActive,...(review.rebuildIndexes?['--rebuild-indexes']:[])],180000);
        if(selected.schema!==1||selected.active!==review.target)throw new ServiceError('crossed_runtime_activation_receipt',true);
        this.transitionReviews.delete(review);return selected;
    }
    async supersedePending(binding:RuntimeBinding,diagnosis:RecordValue|undefined):Promise<boolean>{
        if(runtimeCleanBreakRequired(binding.version,this.version,binding.cleanBreak??diagnosis?.clean_break))return false;
        if(!diagnosis?.pending_activation)return false;
        const saved=record(diagnosis?.pending_activation);
        if(!saved.candidate)return false;
        const bundle=await this.verified();
        if(saved.candidate===bundle.digest||!['drain','launcher'].includes(String(saved.phase)))return false;
        const review=await this.inspectTransition(binding,'resume',true);
        if(!automaticUpgrade(review.version,this.version))return false;
        const result=await this.invoke(bundle.main,['runtime','abort-activation','--install-dir',binding.installDir,
            '--expected-candidate',review.target,'--expected-active',review.expectedActive],180000);
        if(result.activation_aborted!==true||result.payloads_preserved!==true)throw new ServiceError('crossed_runtime_abort_receipt',true);
        this.transitionReviews.delete(review);return true;
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
