// SPDX-License-Identifier: Apache-2.0
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { createHash, randomBytes } from 'node:crypto';

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

/** Only the U105 open selection remains: the imported legacy kinds had no native producer after reviewed grants were removed (change-log entry 68 parts 3 and 5). */
export type HttpSelection={kind:'open';selection:{namespace:string}};
export interface EndpointDescriptor {
    schema:1;installation:string;user:string;namespace:string;generation:number;service_instance:string;
    executable_sha256:string;url:string;
    state:'recovering'|'ready'|'stopping';predecessor_digest:string|null;
}
export interface HttpConnectionInfo {schema:1;connection_id:string;endpoint:EndpointDescriptor;selection:HttpSelection;
    open?:{defaults_revision:number;default_index_ids:string[];default_index_set_ids:string[];agent_choice:'off'|'on'}}
export interface AttachmentRetirementProof {
    schema:1;connection_id:string;installation:string;namespace:string;client:string;
    expires_unix_ms:number;explorer_descriptor_sha256:string;authority_sha256:string;
}
function object(value:unknown):Record<string,any>{if(!value||typeof value!=='object'||Array.isArray(value))throw Error('invalid_http_connection');return value as Record<string,any>;}
function keys(value:Record<string,any>,fields:string[]):void{if(Object.keys(value).some(key=>!fields.includes(key))||fields.some(key=>!Object.hasOwn(value,key)))throw Error('invalid_http_connection_fields');}
function hex(value:unknown,length=64):void{if(typeof value!=='string'||!new RegExp(`^[a-f0-9]{${length}}$`).test(value))throw Error('invalid_http_identity');}
function loopbackUrl(value:unknown,scheme:'http'|'https'):string{
    const pattern=scheme==='http'?/^http:\/\/127\.0\.0\.1:[1-9][0-9]{0,4}\/mcp$/:/^https:\/\/127\.0\.0\.1:[1-9][0-9]{0,4}\/mcp$/;
    if(typeof value!=='string'||!pattern.test(value))throw Error('invalid_http_endpoint');
    const port=Number(value.slice(`${scheme}://127.0.0.1:`.length,-4));if(port<1||port>65535)throw Error('invalid_http_port');return value;
}
/** The current loopback endpoint: exactly `http://127.0.0.1:<port>/mcp`. */
export function httpUrl(value:unknown):string{return loopbackUrl(value,'http');}
/** An `https://127.0.0.1:<port>/mcp` URL an older HTTPS build wrote. Use it only
 * to recognise AIRK's legacy agent entries so they are reported and rewritten;
 * it is never a connectable endpoint. */
export function legacyHttpsUrl(value:unknown):string{return loopbackUrl(value,'https');}
/** Recognition only: the current URL or a legacy HTTPS one. */
export function recognizedHttpUrl(value:unknown):string{try{return httpUrl(value);}catch{return legacyHttpsUrl(value);}}
export function validateHttpSelection(input:unknown):HttpSelection{
    const value=object(input);keys(value,['kind','selection']);
    if(value.kind==='open'){const selection=object(value.selection);keys(selection,['namespace']);hex(selection.namespace,32);return {kind:'open',selection:{namespace:selection.namespace}};}
    throw Error('invalid_http_selection');
}
export function sameHttpAuthority(actual:HttpSelection,expected:HttpSelection):boolean{
    return actual.kind===expected.kind&&actual.selection.namespace===expected.selection.namespace;
}
/** Matches native authority_digest over the complete selection. Arrays
 * retain their order. */
export function httpAuthorityDigest(input:HttpSelection):string{
    const selection=validateHttpSelection(structuredClone(input));
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
    const endpoint=object(input);keys(endpoint,['schema','installation','user','namespace','generation','service_instance','executable_sha256','url','state','predecessor_digest']);
    if(endpoint.schema!==1||!Number.isSafeInteger(endpoint.generation)||endpoint.generation<1||typeof endpoint.user!=='string'||!/^[\x21-\x7e]{1,256}$/.test(endpoint.user)||!['recovering','ready','stopping'].includes(endpoint.state))throw Error('invalid_http_descriptor');
    for(const field of ['installation','executable_sha256'])hex(endpoint[field]);
    for(const field of ['namespace','service_instance'])hex(endpoint[field],32);if(endpoint.predecessor_digest!==null)hex(endpoint.predecessor_digest);httpUrl(endpoint.url);
    return endpoint as EndpointDescriptor;
}
export function validateHttpConnection(input:unknown,expected?:HttpSelection):HttpConnectionInfo{
    if(Buffer.byteLength(JSON.stringify(input))>61440)throw Error('http_connection_bound');
    const value=object(input);
    // (U105) `open` is present exactly for an open connection: its stored default scope and agent choice.
    keys(value,['schema','connection_id','endpoint','selection',...(Object.hasOwn(value,'open')?['open']:[])]);
    if(value.schema!==1)throw Error('unsupported_http_connection');hex(value.connection_id,32);
    const endpoint=validateEndpoint(value.endpoint);
    const selection=validateHttpSelection(value.selection);if(selection.selection.namespace!==endpoint.namespace||expected&&!sameHttpAuthority(selection,expected))throw Error('crossed_http_selection');
    if((selection.kind==='open')!==Object.hasOwn(value,'open'))throw Error('invalid_http_connection_fields');
    if(Object.hasOwn(value,'open')){const open=object(value.open);keys(open,['defaults_revision','default_index_ids','default_index_set_ids','agent_choice']);
        if(!Number.isSafeInteger(open.defaults_revision)||open.defaults_revision<1||open.agent_choice!=='off'&&open.agent_choice!=='on')throw Error('invalid_http_open_state');
        for(const [field,prefix] of [['default_index_ids','idx_'],['default_index_set_ids','prj_']] as const){const items=open[field];
            if(!Array.isArray(items)||items.length>32||new Set(items).size!==items.length||!items.every(item=>typeof item==='string'&&new RegExp(`^${prefix}[0-9a-f]{32}$`).test(item)))throw Error('invalid_http_open_state');}}
    return structuredClone({...value,selection}) as HttpConnectionInfo;
}

export function credentialEnvironment(info:HttpConnectionInfo):string{return 'AIRK_MCP_TOKEN_'+info.connection_id.toUpperCase();}

export function vscodeCredentialInputId(info:HttpConnectionInfo):string{return 'airkToken_'+info.connection_id;}
export function vscodeCredentialInput(info:HttpConnectionInfo):{id:string;type:'command';command:string;args:{connection_id:string}}{
    validateHttpConnection(info);return vscodeCredentialInputForId(info.connection_id);
}
export function vscodeCredentialInputForId(id:string):{id:string;type:'command';command:string;args:{connection_id:string}}{
    hex(id,32);return {id:'airkToken_'+id,type:'command',command:'airk.httpCredential',args:{connection_id:id}};
}
