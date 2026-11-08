// SPDX-License-Identifier: Apache-2.0
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import {createHash, X509Certificate} from 'node:crypto';
import {isDeepStrictEqual} from 'node:util';
import {EndpointDescriptor, httpPublicTrustFile, validateEndpoint} from './serviceHttp';

export type IssuerRotationAction='status'|'prepare'|'trust-new'|'commit'|'finalize'|'abort';
export interface IssuerRotationRecord extends Record<string,unknown>{schema:1;operation_id:string;phase:'prepared'|'stopping'|'activating'|'restarting'|'activated'|'finalized'|'aborted';revision:number;installation:string;user:string;namespace:string;old_ca_sha256:string;new_ca_sha256:string;old_ca_certificate_file:string;new_ca_certificate_file:string;old_endpoint:EndpointDescriptor;new_endpoint:EndpointDescriptor|null;overlap_seconds:number;overlap_expires_unix_ms:number|null}
export interface IssuerRotationStatus extends Record<string,unknown>{schema:1;current_endpoint:EndpointDescriptor|null;current_descriptor_sha256:string|null;rotation:IssuerRotationRecord|null;receipt_sha256:string|null;finalize_overdue:boolean;guidance:string}
const object=(input:unknown):Record<string,any>=>{if(!input||typeof input!=='object'||Array.isArray(input))throw Error('invalid_http_issuer_rotation');return input as Record<string,any>;};
const hex=(input:unknown,length=64):void=>{if(typeof input!=='string'||!new RegExp(`^[a-f0-9]{${length}}$`).test(input))throw Error('invalid_http_rotation_identity');};
const count=(input:unknown):number=>{if(typeof input!=='number'||!Number.isSafeInteger(input)||input<0)throw Error('invalid_http_rotation_count');return input;};
const samePath=(a:string,b:string)=>process.platform==='win32'?path.toNamespacedPath(path.resolve(a)).toLowerCase()===path.toNamespacedPath(path.resolve(b)).toLowerCase():a===b;
function contained(root:string,file:unknown):void{if(typeof file!=='string'||!path.isAbsolute(file)){throw Error('invalid_http_rotation_path');}const relative=path.relative(root,file);if(!relative||relative==='..'||relative.startsWith('..'+path.sep)||path.isAbsolute(relative))throw Error('escaped_http_rotation_path');}

/** Native receipts bind the durable transaction; never recompute their digest
 * using a different language's JSON serialization or reduce unknown fields. */
export function validateIssuerRotation(input:unknown,controlDir:string,expected?:IssuerRotationStatus):IssuerRotationStatus{
    const value=object(input);if(Buffer.byteLength(JSON.stringify(value))>61440||value.schema!==1||typeof value.guidance!=='string'||value.guidance.length>8192||typeof value.finalize_overdue!=='boolean')throw Error('invalid_http_issuer_rotation');
    if(value.current_endpoint!==null){validateEndpoint(value.current_endpoint);hex(value.current_descriptor_sha256);}else if(value.current_descriptor_sha256!==null)throw Error('crossed_http_rotation_descriptor');
    if(value.rotation===null){if(value.receipt_sha256!==null)throw Error('crossed_http_rotation_receipt');if(expected?.rotation)throw Error('crossed_http_rotation_transaction');return value as IssuerRotationStatus;}
    const record=object(value.rotation);hex(value.receipt_sha256);hex(record.operation_id,32);hex(record.installation);hex(record.namespace,32);hex(record.old_ca_sha256);hex(record.new_ca_sha256);
    if(record.schema!==1||!['prepared','stopping','activating','restarting','activated','finalized','aborted'].includes(record.phase)||count(record.revision)<1||typeof record.user!=='string'||!record.user.length||record.user.length>256||record.old_ca_sha256===record.new_ca_sha256)throw Error('invalid_http_rotation_record');
    const old=validateEndpoint(record.old_endpoint),next=record.new_endpoint===null?null:validateEndpoint(record.new_endpoint);
    for(const endpoint of [old,next,value.current_endpoint].filter(Boolean) as EndpointDescriptor[])if(endpoint.installation!==record.installation||endpoint.namespace!==record.namespace||endpoint.user!==record.user)throw Error('crossed_http_rotation_owner');
    if(old.ca_sha256!==record.old_ca_sha256||next&&next.ca_sha256!==record.new_ca_sha256)throw Error('crossed_http_rotation_issuer');
    if(next&&(next.generation<=old.generation||next.service_instance===old.service_instance||next.executable_sha256!==old.executable_sha256))throw Error('regressed_http_rotation_generation');
    if(record.overlap_expires_unix_ms!==null)count(record.overlap_expires_unix_ms);
    if(count(record.overlap_seconds)<60||record.overlap_seconds>86400)throw Error('invalid_http_rotation_overlap');
    if(['activated','finalized'].includes(record.phase)&&(!next||record.overlap_expires_unix_ms===null))throw Error('incomplete_http_rotation_activation');
    contained(path.join(controlDir,'http-1'),record.old_ca_certificate_file);contained(path.join(controlDir,'http-1'),record.new_ca_certificate_file);
    if(expected?.rotation){const before=expected.rotation;if(record.operation_id!==before.operation_id||record.installation!==before.installation||record.user!==before.user||record.namespace!==before.namespace||record.old_ca_sha256!==before.old_ca_sha256||record.new_ca_sha256!==before.new_ca_sha256||record.old_ca_certificate_file!==before.old_ca_certificate_file||record.new_ca_certificate_file!==before.new_ca_certificate_file||record.revision<before.revision||record.overlap_seconds!==before.overlap_seconds||!isDeepStrictEqual(record.old_endpoint,before.old_endpoint))throw Error('crossed_http_rotation_transaction');}
    return value as IssuerRotationStatus;
}

export interface TrustVariable {type:number;value:string}
export interface TrustCollection {get(variable:string):TrustVariable|undefined;replace(variable:string,value:string):void;persistent:boolean}
export interface ClientTrustUpdate {variable:string;before:string;after:string;before_sha256:string;after_sha256:string;changed:boolean}
export class ClientTrustPublicationError extends Error{constructor(readonly updates:ClientTrustUpdate[],readonly published:string[]){super('http_trust_publication_outcome_unknown');}}
const sha=(bytes:Buffer)=>createHash('sha256').update(bytes).digest('hex');
function certificates(bytes:Buffer):{pem:Buffer;digest:string}[]{
    const text=new TextDecoder('utf-8',{fatal:true}).decode(bytes);if(text.includes('PRIVATE KEY'))throw Error('private_key_in_http_trust_bundle');
    const matches=[...text.matchAll(/-----BEGIN CERTIFICATE-----[\s\S]*?-----END CERTIFICATE-----/g)];
    if(!matches.length||matches.length>256)throw Error('unsupported_http_trust_bundle');
    return matches.map(match=>({pem:Buffer.from(match[0]+'\n'),digest:sha(new X509Certificate(match[0]).raw)}));
}
const stagePrefix='.issuer-bundle-';
const sameIdentity=(a:{dev:number;ino:number},b:{dev:number;ino:number})=>a.dev===b.dev&&a.ino===b.ino;
async function syncBundleDirectory(directory:string):Promise<void>{
    // Windows does not expose directory fsync through Node. File contents are
    // flushed there; Unix also durably orders the stage and directory entries.
    if(process.platform==='win32')return;
    const handle=await fs.open(directory,'r');try{await handle.sync();}finally{await handle.close();}
}
async function cleanBundleStage(stage:string,identity:{dev:number;ino:number},receipt:Buffer):Promise<void>{
    const temporary=path.join(stage,'bundle.pem'),marker=path.join(stage,'owner.json');
    const owner=await fs.lstat(marker);
    if(!receipt.equals(await httpPublicTrustFile(marker,4096)))throw Error('changed_http_trust_stage_receipt');
    const stat=await fs.lstat(temporary).catch(error=>{if((error as NodeJS.ErrnoException).code==='ENOENT')return undefined;throw error;});
    if(stat){if(!stat.isFile()||stat.isSymbolicLink()||!sameIdentity(stat,identity))throw Error('changed_http_trust_stage');await fs.unlink(temporary);}
    if(!sameIdentity(owner,await fs.lstat(marker))||!receipt.equals(await httpPublicTrustFile(marker,4096)))throw Error('changed_http_trust_stage_receipt');
    await fs.unlink(marker);
    await fs.rmdir(stage).catch(error=>{if((error as NodeJS.ErrnoException).code!=='ENOTEMPTY')throw error;});
    await syncBundleDirectory(path.dirname(stage));
}
/** A durable receipt permits recovery only of the exact complete temporary link.
 * Parsed metadata never authorizes replacing or removing a final bundle. */
async function recoverBundleLink(directory:string,final:string,output:Buffer):Promise<void>{
    const target=await fs.lstat(final);if(!target.isFile()||target.isSymbolicLink()||target.nlink!==2||target.size!==output.length)throw Error('unsafe_http_trust_publication');
    const entries:string[]=[],scan=await fs.opendir(directory);
    for await(const entry of scan){if(entries.length>=1024)throw Error('http_trust_stage_bound');entries.push(entry.name);}
    let recovered=false;
    for(const name of entries){
        if(!/^\.issuer-bundle-[A-Za-z0-9]{6}$/.test(name))continue;
        const stage=path.join(directory,name),stat=await fs.lstat(stage);
        if(!stat.isDirectory()||stat.isSymbolicLink()||!samePath(await fs.realpath(stage),stage)||process.platform!=='win32'&&((stat.mode&0o077)!==0||typeof process.getuid==='function'&&stat.uid!==process.getuid()))continue;
        const temporary=path.join(stage,'bundle.pem'),candidate=await fs.lstat(temporary).catch(()=>undefined);
        if(!candidate||!sameIdentity(candidate,target)||!candidate.isFile()||candidate.isSymbolicLink()||candidate.nlink!==2)continue;
        const receipt=await httpPublicTrustFile(path.join(stage,'owner.json'),4096),record=JSON.parse(receipt.toString('utf8'));
        if(record.schema!==1||record.stage!==name||record.sha256!==sha(output)||record.size!==output.length||record.dev!==target.dev||record.ino!==target.ino)throw Error('crossed_http_trust_stage_receipt');
        const handle=await fs.open(temporary,'r');
        try{
            const before=await handle.stat();if(!sameIdentity(before,target)||before.size!==output.length)throw Error('crossed_http_trust_stage_body');
            const bytes=Buffer.alloc(output.length+1);let used=0;
            while(used<bytes.length){const read=await handle.read(bytes,used,bytes.length-used,null);if(!read.bytesRead)break;used+=read.bytesRead;}
            const after=await handle.stat();if(!sameIdentity(before,after)||after.size!==before.size||after.mtimeMs!==before.mtimeMs||!output.equals(bytes.subarray(0,used)))throw Error('crossed_http_trust_stage_body');
        }finally{await handle.close();}
        if(!sameIdentity(await fs.lstat(final),target))throw Error('changed_http_trust_publication');
        await cleanBundleStage(stage,target,receipt);recovered=true;break;
    }
    if(!recovered)throw Error('unowned_http_trust_publication');
}
export async function publishOwnedIssuerBundle(directory:string,output:Buffer,current:()=>void):Promise<string>{
    if(output.length>2097152)throw Error('http_client_trust_bundle_limit');
    if(!samePath(await fs.realpath(directory),directory)||(await fs.lstat(directory)).isSymbolicLink())throw Error('unsafe_http_trust_directory');current();
    const final=path.join(directory,sha(output)+'.pem');
    const existing=async()=>{
        const stat=await fs.lstat(final).catch(error=>{if((error as NodeJS.ErrnoException).code==='ENOENT')return undefined;throw error;});
        if(!stat)return false;
        if(stat.nlink===2)await recoverBundleLink(directory,final,output);
        if(!output.equals(await httpPublicTrustFile(final)))throw Error('crossed_http_trust_bundle');return true;
    };
    if(await existing()){current();return final;}
    const stage=await fs.mkdtemp(path.join(directory,stagePrefix)),temporary=path.join(stage,'bundle.pem'),marker=path.join(stage,'owner.json');
    let identity:{dev:number;ino:number}|undefined,receipt:Buffer|undefined;
    try{
        const handle=await fs.open(temporary,'wx',0o600);
        try{identity=await handle.stat();await handle.writeFile(output);await handle.sync();}finally{await handle.close();}
        if(!output.equals(await httpPublicTrustFile(temporary)))throw Error('changed_http_trust_stage_body');
        receipt=Buffer.from(JSON.stringify({schema:1,stage:path.basename(stage),sha256:sha(output),size:output.length,dev:identity.dev,ino:identity.ino}));
        const owner=await fs.open(marker,'wx',0o600);try{await owner.writeFile(receipt);await owner.sync();}finally{await owner.close();}
        await syncBundleDirectory(stage);await syncBundleDirectory(directory);
        current();try{await fs.link(temporary,final);}catch(error){if((error as NodeJS.ErrnoException).code!=='EEXIST')throw error;await existing();}
    }finally{
        if(identity){
            if(receipt)await cleanBundleStage(stage,identity,receipt);
            else{const stat=await fs.lstat(temporary);if(!sameIdentity(stat,identity)||!stat.isFile()||stat.isSymbolicLink()||stat.nlink!==1)throw Error('changed_http_trust_stage');await fs.unlink(temporary);await fs.rmdir(stage);}
        }else await fs.rmdir(stage);
    }
    if(!await existing())throw Error('missing_http_trust_publication');current();return final;
}
/** Changes only this window's exact extension-owned immutable bundle references.
 * Original enterprise/user files, unrelated certificates and config scopes stay
 * untouched. All preimages are checked again before any synchronous publication. */
export async function updateOwnedIssuerBundles(storage:string,collection:TrustCollection,record:IssuerRotationRecord,mode:'overlap'|'finalize'|'abort',replaceType:number,current:()=>void):Promise<ClientTrustUpdate[]>{
    if(mode==='overlap'&&['finalized','aborted'].includes(record.phase)||mode==='finalize'&&record.phase!=='finalized'||mode==='abort'&&record.phase!=='aborted')throw Error('http_rotation_phase_requires_review');
    current();const directory=path.join(storage,'http-trust'),physical=await fs.realpath(directory).catch(error=>{if((error as NodeJS.ErrnoException).code==='ENOENT')return undefined;throw error;});current();
    if(physical===undefined)return [];
    if(!samePath(physical,directory)||(await fs.lstat(directory)).isSymbolicLink())throw Error('unsafe_http_trust_directory');
    const oldBytes=await httpPublicTrustFile(record.old_ca_certificate_file,8192),newBytes=await httpPublicTrustFile(record.new_ca_certificate_file,8192);current();
    const oldCerts=certificates(oldBytes),newCerts=certificates(newBytes);
    if(oldCerts.length!==1||newCerts.length!==1||oldCerts[0].digest!==record.old_ca_sha256||newCerts[0].digest!==record.new_ca_sha256||!new X509Certificate(oldBytes).ca||!new X509Certificate(newBytes).ca)throw Error('crossed_http_rotation_certificate');
    const updates:ClientTrustUpdate[]=[];
    for(const variable of ['CODEX_CA_CERTIFICATE','NODE_EXTRA_CA_CERTS']){
        const prior=collection.get(variable);if(!prior)continue;
        if(prior.type!==replaceType||!samePath(path.dirname(prior.value),directory)||!/^[a-f0-9]{64}\.pem$/.test(path.basename(prior.value)))throw Error('http_client_trust_requires_review');
        const bytes=await httpPublicTrustFile(prior.value),digest=sha(bytes);current();
        if(path.basename(prior.value)!==digest+'.pem')throw Error('changed_http_owned_trust_bundle');
        const parsed=certificates(bytes),hasOld=parsed.some(cert=>cert.digest===record.old_ca_sha256),hasNew=parsed.some(cert=>cert.digest===record.new_ca_sha256);
        if((mode==='overlap'||mode==='abort')&&!hasOld||mode==='finalize'&&!hasNew)throw Error('crossed_http_rotation_bundle');
        const retiring=mode==='abort'?record.new_ca_sha256:record.old_ca_sha256;
        const output=mode==='overlap'?(hasNew?bytes:Buffer.concat([bytes,Buffer.from('\n'),newBytes])):Buffer.from(bytes.toString('utf8').replace(/-----BEGIN CERTIFICATE-----[\s\S]*?-----END CERTIFICATE-----/g,pem=>sha(new X509Certificate(pem).raw)===retiring?'':pem));
        if(output.length>2097152)throw Error('http_client_trust_bundle_limit');
        const next=await publishOwnedIssuerBundle(directory,output,current);
        updates.push({variable,before:prior.value,after:next,before_sha256:digest,after_sha256:sha(output),changed:!samePath(prior.value,next)});
    }
    for(const update of updates){const prior=collection.get(update.variable);if(prior?.type!==replaceType||prior.value!==update.before||sha(await httpPublicTrustFile(update.before))!==update.before_sha256)throw Error('http_trust_configuration_changed');current();}
    current();collection.persistent=false;
    const published:string[]=[];
    try{for(const update of updates)if(update.changed){collection.replace(update.variable,update.after);published.push(update.variable);}}
    catch{throw new ClientTrustPublicationError(updates,published);}
    return updates;
}
