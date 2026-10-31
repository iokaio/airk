// SPDX-License-Identifier: Apache-2.0
// Fixed system tools only; no shell, request-derived executable or credential.
import * as childProcess from 'node:child_process';
import * as fs from 'node:fs';
import * as path from 'node:path';
export type AclRunner = (file:string,args:readonly string[])=>Promise<string>;
const run:AclRunner=(file,args)=>new Promise((resolve,reject)=>{
    childProcess.execFile(file,[...args],{encoding:'utf8',windowsHide:true,timeout:10000,maxBuffer:128*1024},
        (error,stdout)=>error?reject(error):resolve(stdout));
});
interface Stamp {physical:string;changed:bigint}
const restricted=new Map<string,Promise<Stamp>>();
async function stamp(directory:string):Promise<Stamp>{
    const value=await fs.promises.lstat(directory,{bigint:true});
    if(!value.isDirectory()||value.isSymbolicLink())throw new Error('The Assistance directory was replaced or redirected.');
    if(value.ino===0n||value.birthtimeNs<=0n)throw new Error('Physical Assistance directory identity is unavailable.');
    return {physical:`${value.dev}:${value.ino}:${value.birthtimeNs}`,changed:value.ctimeNs};
}
function authorized(valid:()=>boolean):void{if(!valid())throw new Error('Assistance setup is no longer authorized.');}
export interface PrivateDirectoryGuard {assertCurrent():void}
/** Async ACL completion precedes every credential/descriptor write. The cache is
 * physical-object/replacement aware, process-local and never a native ACL proof. */
export async function restrictWindowsDirectory(directory:string,execute:AclRunner=run,valid:()=>boolean=()=>true):Promise<PrivateDirectoryGuard>{
    authorized(valid);
    if(process.platform!=='win32')return {assertCurrent:()=>authorized(valid)};
    const before=await stamp(directory);authorized(valid);
    let pending=restricted.get(before.physical);
    if(pending){
        try{const cached=await pending;authorized(valid);if(cached.changed!==before.changed){restricted.delete(before.physical);pending=undefined;}}
        catch{authorized(valid);if(restricted.get(before.physical)===pending)restricted.delete(before.physical);pending=undefined;}
    }
    if(!pending){
        pending=(async()=>{
            const systemRoot=process.env.SystemRoot||'C:\\Windows';
            if(!path.isAbsolute(systemRoot)||/[\u0000-\u001f]/.test(systemRoot))throw new Error('Windows system directory is unavailable.');
            const system32=path.join(systemRoot,'System32');
            const user=/"(S-1-5-[0-9-]+)"/.exec(await execute(path.join(system32,'whoami.exe'),['/user','/fo','csv','/nh']))?.[1];
            authorized(valid);if(!user)throw new Error('Could not resolve the current Windows account for Assistance.');
            await execute(path.join(system32,'icacls.exe'),[directory,'/inheritance:r','/grant:r',`*${user}:(OI)(CI)F`,'*S-1-5-18:(OI)(CI)F','*S-1-5-32-544:(OI)(CI)F']);
            authorized(valid);const after=await stamp(directory);authorized(valid);
            if(after.physical!==before.physical)throw new Error('The Assistance directory changed during setup.');
            return after;
        })();
        restricted.set(before.physical,pending);
        if(restricted.size>128)restricted.delete(restricted.keys().next().value!);
        pending.catch(()=>{if(restricted.get(before.physical)===pending)restricted.delete(before.physical);});
    }
    const accepted=await pending;authorized(valid);
    const guard:PrivateDirectoryGuard={assertCurrent:()=>{
        authorized(valid);const value=fs.lstatSync(directory,{bigint:true});
        if(!value.isDirectory()||value.isSymbolicLink()||`${value.dev}:${value.ino}:${value.birthtimeNs}`!==accepted.physical||value.ctimeNs!==accepted.changed)
            throw new Error('The Assistance directory changed before publication.');
    }};
    guard.assertCurrent();return guard;
}
