// SPDX-License-Identifier: Apache-2.0
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { X509Certificate, createHash, randomBytes } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import { validateCatalogSession, validateCombinedCatalogSession, CatalogSessionSelector, CombinedCatalogSessionSelector } from './catalogSession';
import { validateNativeLegacyBinding } from './indexingCatalog';

type InputStamp={dev:number;ino:number;birthtimeMs:number;mode:number;uid:number};
type InputOwner={control:string;directory:string;parent:InputStamp;directoryStamp:InputStamp;file:InputStamp};
const nativeInputs=new Map<string,InputOwner>();
function inputIdentity(value:InputStamp,expected:InputStamp):boolean{return value.dev===expected.dev&&value.ino===expected.ino&&value.birthtimeMs===expected.birthtimeMs&&value.mode===expected.mode&&value.uid===expected.uid;}
async function requireInputDirectory(owner:Omit<InputOwner,'file'>):Promise<void>{
    const parent=await fs.lstat(owner.control),directory=await fs.lstat(owner.directory);
    if(!parent.isDirectory()||parent.isSymbolicLink()||!directory.isDirectory()||directory.isSymbolicLink()
        ||!inputIdentity(parent,owner.parent)||!inputIdentity(directory,owner.directoryStamp)
        ||await fs.realpath(owner.directory)!==path.join(await fs.realpath(owner.control),'http-1'))throw Error('native_input_directory_changed');
}
/** Remove only this process's exact transient publication; preserve a replaced
 * file or parent rather than deleting an unknown object during error recovery. */
export async function removeNativeInput(file:string):Promise<void>{
    const owner=nativeInputs.get(file);if(!owner)throw Error('native_input_ownership_unavailable');
    await requireInputDirectory(owner);
    const current=await fs.lstat(file);
    if(!current.isFile()||current.isSymbolicLink()||current.nlink!==1||!inputIdentity(current,owner.file))throw Error('native_input_file_changed');
    await fs.unlink(file);nativeInputs.delete(file);
}

/** Publish inert setup input in the already native-protected HTTP directory.
 * Assistance uses a different Windows ACL; never apply it to native input.
 * Native readers independently verify directory/file ownership again. */
export async function publishNativeInput(control:string,content:string,check:()=>void=()=>{},retainedName?:string):Promise<string>{
    check();if(!path.isAbsolute(control))throw Error('unsafe_native_input_control');
    const directory=path.join(control,'http-1'),parent=await fs.lstat(control),stat=await fs.lstat(directory);
    if(!parent.isDirectory()||parent.isSymbolicLink()||!stat.isDirectory()||stat.isSymbolicLink()
        ||await fs.realpath(directory)!==path.join(await fs.realpath(control),'http-1'))throw Error('unsafe_native_input_directory');
    if(process.platform!=='win32'&&(stat.uid!==process.getuid?.()||(stat.mode&0o077)!==0))throw Error('unsafe_native_input_directory');
    const owner={control,directory,parent,directoryStamp:stat};
    const bytes=Buffer.from(content);if(bytes.length>1048576)throw Error('native_input_exceeds_budget');
    const name=retainedName??'native-input-'+randomBytes(16).toString('hex')+'.json';
    if(!/^native-input-[a-z0-9-]{1,128}\.json$/.test(name))throw Error('invalid_native_input_name');
    const file=path.join(directory,name);check();
    const temporary=retainedName?path.join(directory,'native-input-stage-'+randomBytes(16).toString('hex')+'.json'):file;
    const handle=await fs.open(temporary,'wx',0o600);
    const created=await handle.stat();nativeInputs.set(temporary,{...owner,file:created});
    try{await requireInputDirectory(owner);check();await handle.writeFile(bytes);await handle.sync();check();}
    catch(error){await handle.close();try{await removeNativeInput(temporary);}catch{/* Preserve uncertain objects. */}throw error;}
    finally{await handle.close();}
    await requireInputDirectory(owner);
    if(retainedName){
        try{try{await fs.link(temporary,file);}catch(error){if((error as NodeJS.ErrnoException).code!=='EEXIST')throw error;}}
        finally{
            // A successful link temporarily has two names for this exact file.
            await requireInputDirectory(owner);const linked=await fs.lstat(temporary);
            if(!linked.isFile()||linked.isSymbolicLink()||!inputIdentity(linked,created))throw Error('native_input_file_changed');
            await fs.unlink(temporary);nativeInputs.delete(temporary);
        }
        const existing=await fs.lstat(file);
        if(!existing.isFile()||existing.isSymbolicLink()||existing.nlink!==1||existing.size>1048576
            ||process.platform!=='win32'&&(existing.uid!==process.getuid?.()||(existing.mode&0o077)!==0))throw Error('unsafe_native_input_file');
        // The retained batch caller validates the immutable accepted body.
    }
    check();return file;
}

export interface ImportedHttpSelection {namespace:string;expected_catalog_revision:number;registration:{binding:string;root:string;data_root:string;collection:string};grant:Record<string,any>;configuration_receipt:string}
export type HttpSelection={kind:'scoped';selection:CatalogSessionSelector}|{kind:'combined';selection:CombinedCatalogSessionSelector}|{kind:'imported_legacy';selection:ImportedHttpSelection}|{kind:'imported_legacy_union';selection:{namespace:string;selections:ImportedHttpSelection[];no_default_project:true}};
export interface EndpointDescriptor {
    schema:1;installation:string;user:string;namespace:string;generation:number;service_instance:string;
    executable_sha256:string;url:string;ca_sha256:string;certificate_sha256:string;
    state:'recovering'|'ready'|'stopping';predecessor_digest:string|null;
}
export interface HttpConnectionInfo {schema:1;connection_id:string;endpoint:EndpointDescriptor;selection:HttpSelection;ca_certificate_file:string;credential_file:string}
export interface AttachmentRetirementProof {
    schema:1;connection_id:string;installation:string;namespace:string;client:string;
    expires_unix_ms:number;explorer_descriptor_sha256:string;authority_sha256:string;
}
function object(value:unknown):Record<string,any>{if(!value||typeof value!=='object'||Array.isArray(value))throw Error('invalid_http_connection');return value as Record<string,any>;}
function keys(value:Record<string,any>,fields:string[]):void{if(Object.keys(value).some(key=>!fields.includes(key))||fields.some(key=>!Object.hasOwn(value,key)))throw Error('invalid_http_connection_fields');}
function hex(value:unknown,length=64):void{if(typeof value!=='string'||!new RegExp(`^[a-f0-9]{${length}}$`).test(value))throw Error('invalid_http_identity');}
export function httpUrl(value:unknown):string{
    if(typeof value!=='string'||!/^https:\/\/127\.0\.0\.1:[1-9][0-9]{0,4}\/mcp$/.test(value))throw Error('invalid_http_endpoint');
    const port=Number(value.slice('https://127.0.0.1:'.length,-4));if(port<1||port>65535)throw Error('invalid_http_port');return value;
}
export function validateHttpSelection(input:unknown):HttpSelection{
    const value=object(input);keys(value,['kind','selection']);
    if(value.kind==='scoped')return {kind:'scoped',selection:validateCatalogSession(value.selection)};
    if(value.kind==='combined')return {kind:'combined',selection:validateCombinedCatalogSession(value.selection)};
    if(value.kind==='imported_legacy'){
        const selection=object(value.selection);keys(selection,['namespace','expected_catalog_revision','registration','grant','configuration_receipt']);hex(selection.configuration_receipt);
        const registration=object(selection.registration);keys(registration,['binding','root','data_root','collection']);
        validateNativeLegacyBinding({status:'catalog_legacy_binding',observation:{schema:1,namespace:selection.namespace,project:registration.binding,catalog_revision:selection.expected_catalog_revision,eligibility:{kind:'imported_snapshot',registration,grant:selection.grant}}},registration.binding,selection.expected_catalog_revision);
        const provenance=object(object(selection.grant).provenance);
        if(provenance.kind==='imported_standalone_adoption'&&!object(provenance.adoption).configuration_claims.some((claim:unknown)=>object(claim).config_sha256===selection.configuration_receipt))throw Error('crossed_http_standalone_configuration');
        return {kind:'imported_legacy',selection:structuredClone(selection) as unknown as ImportedHttpSelection};
    }
    if(value.kind==='imported_legacy_union'){
        const selection=object(value.selection);keys(selection,['namespace','selections','no_default_project']);hex(selection.namespace,32);
        if(selection.no_default_project!==true||!Array.isArray(selection.selections)||selection.selections.length<2||selection.selections.length>32)throw Error('invalid_http_legacy_union');
        const seen=new Set<string>();
        const members=selection.selections.map((member:unknown)=>{const result=validateHttpSelection({kind:'imported_legacy',selection:member}) as Extract<HttpSelection,{kind:'imported_legacy'}>;if(result.selection.namespace!==selection.namespace||seen.has(result.selection.registration.binding))throw Error('crossed_http_legacy_union');seen.add(result.selection.registration.binding);return result.selection;});
        return {kind:'imported_legacy_union',selection:{namespace:selection.namespace,selections:members,no_default_project:true}};
    }
    throw Error('invalid_http_selection');
}
/** Relates a validated native retarget to the original saved declaration.
 * Only standalone adoption may change data storage; its complete immutable
 * provenance retains the original registration and exact configuration bytes. */
export function matchesImportedHttpOrigin(member:ImportedHttpSelection,registration:ImportedHttpSelection['registration'],configurationReceipt?:string):boolean{
    const validated=validateHttpSelection({kind:'imported_legacy',selection:member}) as Extract<HttpSelection,{kind:'imported_legacy'}>;
    const selection=validated.selection,provenance=object(object(selection.grant).provenance);
    const original=provenance.kind==='imported_standalone_adoption'?object(object(provenance.adoption).plan).original_registration:selection.registration;
    return (configurationReceipt===undefined||selection.configuration_receipt===configurationReceipt)&&isDeepStrictEqual(original,registration);
}
export function standaloneHttpServerKey(connection:HttpConnectionInfo,candidate?:string,container?:readonly string[]):string|undefined{
    const info=validateHttpConnection(connection);if(info.selection.kind!=='imported_legacy')return;
    const selected=info.selection.selection,provenance=object(object(selected.grant).provenance);
    if(provenance.kind==='imported_standalone_adoption'){
        const claims=object(provenance.adoption).configuration_claims as Array<Record<string,any>>;
        const matching=claims.filter(claim=>claim.config_sha256===selected.configuration_receipt&&(candidate===undefined||claim.original_server===candidate)&&(container===undefined||isDeepStrictEqual(claim.container_path,container)));
        const names=new Set(matching.map(claim=>String(claim.original_server)));if(names.size===1)return [...names][0];
    }
}
/** Generated service-era aliases are bound to the native imported collection;
 * arbitrary aliases still require a standalone original-configuration claim. */
export function preservedImportedHttpServerKey(connection:HttpConnectionInfo,candidate:string,container?:readonly string[]):string|undefined{
    const info=validateHttpConnection(connection);if(info.selection.kind!=='imported_legacy')return;
    if(object(object(info.selection.selection.grant).provenance).kind==='imported_legacy_snapshot'&&candidate==='airk-'+info.selection.selection.registration.collection)return candidate;
    return standaloneHttpServerKey(info,candidate,container);
}
export function sameHttpAuthority(actual:HttpSelection,expected:HttpSelection):boolean{
    if(actual.kind!==expected.kind)return false;
    if(actual.kind==='imported_legacy_union'&&expected.kind==='imported_legacy_union'){
        return actual.selection.namespace===expected.selection.namespace&&actual.selection.no_default_project===true&&actual.selection.selections.length===expected.selection.selections.length&&actual.selection.selections.every((member,index)=>sameHttpAuthority({kind:'imported_legacy',selection:member},{kind:'imported_legacy',selection:expected.selection.selections[index]}));
    }
    if(actual.kind==='imported_legacy_union'||expected.kind==='imported_legacy_union')return false;
    const left={...actual.selection},right={...expected.selection};
    if(left.expected_catalog_revision<right.expected_catalog_revision)return false;
    delete (left as any).expected_catalog_revision;delete (right as any).expected_catalog_revision;
    return isDeepStrictEqual(left,right);
}
/** Matches native authority_digest: preserve complete reviewed authority while
 * normalizing only saved minimum catalog clocks. Arrays retain their order. */
export function httpAuthorityDigest(input:HttpSelection):string{
    const selection=validateHttpSelection(structuredClone(input));
    if(selection.kind==='imported_legacy_union')for(const member of selection.selection.selections)member.expected_catalog_revision=1;
    else selection.selection.expected_catalog_revision=1;
    const canonical=(value:unknown):string=>{
        if(value===null||typeof value==='boolean'||typeof value==='string')return JSON.stringify(value);
        if(typeof value==='number'){if(!Number.isSafeInteger(value))throw Error('unsupported_http_authority_number');return JSON.stringify(value);}
        if(Array.isArray(value))return '['+value.map(canonical).join(',')+']';
        if(value&&typeof value==='object')return '{'+Object.keys(value).sort((left,right)=>Buffer.compare(Buffer.from(left),Buffer.from(right))).map(key=>JSON.stringify(key)+':'+canonical((value as Record<string,unknown>)[key])).join(',')+'}';
        throw Error('invalid_http_authority_value');
    };
    const hash=createHash('sha256').update(Buffer.from('AIRK-DIGEST-1\0'));
    for(const bytes of [Buffer.from('airk-http-authority/1'),Buffer.from(canonical(selection))]){const length=Buffer.alloc(8);length.writeBigUInt64BE(BigInt(bytes.length));hash.update(length).update(bytes);}
    return hash.digest('hex');
}
export function validateAttachmentRetirement(input:unknown,connection:HttpConnectionInfo,client:string,now=Date.now()):AttachmentRetirementProof{
    const info=validateHttpConnection(connection),value=object(input);
    keys(value,['schema','connection_id','installation','namespace','client','expires_unix_ms','explorer_descriptor_sha256','authority_sha256']);
    for(const field of ['installation','explorer_descriptor_sha256','authority_sha256'])hex(value[field]);
    hex(value.connection_id,32);hex(value.namespace,32);
    const nativeClient=client==='claude'?'claude_code':client;
    if(value.schema!==1||value.connection_id!==info.connection_id||value.installation!==info.endpoint.installation||value.namespace!==info.selection.selection.namespace
        ||!['codex','claude_code','vscode'].includes(nativeClient)||value.client!==nativeClient||!Number.isSafeInteger(value.expires_unix_ms)||value.expires_unix_ms<=now
        ||value.authority_sha256!==httpAuthorityDigest(info.selection))throw Error('crossed_http_attachment_retirement');
    return structuredClone(value) as AttachmentRetirementProof;
}
export function validateEndpoint(input:unknown):EndpointDescriptor{
    const endpoint=object(input);keys(endpoint,['schema','installation','user','namespace','generation','service_instance','executable_sha256','url','ca_sha256','certificate_sha256','state','predecessor_digest']);
    if(endpoint.schema!==1||!Number.isSafeInteger(endpoint.generation)||endpoint.generation<1||typeof endpoint.user!=='string'||!/^[\x21-\x7e]{1,256}$/.test(endpoint.user)||!['recovering','ready','stopping'].includes(endpoint.state))throw Error('invalid_http_descriptor');
    for(const field of ['installation','executable_sha256','ca_sha256','certificate_sha256'])hex(endpoint[field]);
    for(const field of ['namespace','service_instance'])hex(endpoint[field],32);if(endpoint.predecessor_digest!==null)hex(endpoint.predecessor_digest);httpUrl(endpoint.url);
    return endpoint as EndpointDescriptor;
}
export function validateHttpConnection(input:unknown,expected?:HttpSelection,directory?:string):HttpConnectionInfo{
    if(Buffer.byteLength(JSON.stringify(input))>61440)throw Error('http_connection_bound');
    const value=object(input);keys(value,['schema','connection_id','endpoint','selection','ca_certificate_file','credential_file']);
    if(value.schema!==1)throw Error('unsupported_http_connection');hex(value.connection_id,32);
    const endpoint=validateEndpoint(value.endpoint);
    const selection=validateHttpSelection(value.selection);if(selection.selection.namespace!==endpoint.namespace||expected&&!sameHttpAuthority(selection,expected))throw Error('crossed_http_selection');
    for(const field of ['ca_certificate_file','credential_file'])if(typeof value[field]!=='string'||value[field].length>4096||!path.isAbsolute(value[field])||/[\x00-\x1f]/.test(value[field]))throw Error('invalid_http_private_reference');
    const parent=directory??path.dirname(value.ca_certificate_file);
    if(!sameFile(value.ca_certificate_file,path.join(parent,'ca.pem'))||!sameFile(value.credential_file,path.join(parent,`credential-${value.connection_id}.token`)))throw Error('crossed_http_private_reference');
    return structuredClone({...value,selection}) as HttpConnectionInfo;
}
function sameFile(left:string,right:string):boolean{
    const normalize=(value:string)=>process.platform==='win32'?path.toNamespacedPath(path.resolve(value)).toLowerCase():value;
    return normalize(left)===normalize(right);
}
/** Native setup has already checked the private ACL; recheck filesystem identity
 * on every open. Never accept a repository-nominated path or redirected ancestor. */
async function boundedFile(filename:string,maximum:number,ownerPrivate:boolean):Promise<Buffer>{
    if(!sameFile(await fs.realpath(filename),filename))throw Error('redirected_http_private_reference');
    const stat=await fs.lstat(filename);if(!stat.isFile()||stat.isSymbolicLink()||stat.nlink!==1||stat.size>maximum||ownerPrivate&&process.platform!=='win32'&&((stat.mode&0o077)!==0||typeof process.getuid==='function'&&stat.uid!==process.getuid()))throw Error('unsafe_http_private_file');
    const handle=await fs.open(filename,'r');
    try{
        const opened=await handle.stat();if(opened.dev!==stat.dev||opened.ino!==stat.ino||opened.size!==stat.size)throw Error('changed_http_private_file');
        const bytes=Buffer.alloc(maximum+1);let used=0;
        while(used<bytes.length){const result=await handle.read(bytes,used,bytes.length-used,null);if(!result.bytesRead)break;used+=result.bytesRead;}
        const after=await handle.stat();if(used!==stat.size||used>maximum||after.size!==stat.size||after.mtimeMs!==stat.mtimeMs)throw Error('changed_http_private_file');return bytes.subarray(0,used);
    }finally{await handle.close();}
}
/** An existing user-selected public trust bundle is additive, never a token or key. */
export async function httpPublicTrustFile(filename:string,maximum=2097152):Promise<Buffer>{return boundedFile(filename,maximum,false);}
const privateBytes=(filename:string,maximum:number)=>boundedFile(filename,maximum,true);
export async function httpMaterial(input:HttpConnectionInfo):Promise<{url:string;ca:Buffer;credential:string}>{
    const info=validateHttpConnection(input);if(info.endpoint.state!=='ready')throw Error('http_endpoint_not_ready');
    const ca=await privateBytes(info.ca_certificate_file,65536),credential=(await privateBytes(info.credential_file,103)).toString('utf8');
    if(ca.includes('PRIVATE KEY')||(ca.toString('ascii').match(/-----BEGIN CERTIFICATE-----/g)??[]).length!==1)throw Error('invalid_http_public_ca');
    const certificate=new X509Certificate(ca);if(createHash('sha256').update(certificate.raw).digest('hex')!==info.endpoint.ca_sha256||!certificate.ca)throw Error('crossed_http_ca');
    if(!new RegExp(`^airk1\\.${info.connection_id}\\.[a-f0-9]{64}$`).test(credential))throw Error('crossed_http_credential');
    return {url:info.endpoint.url,ca,credential};
}
export function credentialEnvironment(info:HttpConnectionInfo):string{return 'AIRK_MCP_TOKEN_'+info.connection_id.toUpperCase();}

export function vscodeCredentialInputId(info:HttpConnectionInfo):string{return 'airkToken_'+info.connection_id;}
export function vscodeCredentialInput(info:HttpConnectionInfo):{id:string;type:'command';command:string;args:{connection_id:string}}{
    validateHttpConnection(info);return vscodeCredentialInputForId(info.connection_id);
}
export function vscodeCredentialInputForId(id:string):{id:string;type:'command';command:string;args:{connection_id:string}}{
    hex(id,32);return {id:'airkToken_'+id,type:'command',command:'airk.httpCredential',args:{connection_id:id}};
}
