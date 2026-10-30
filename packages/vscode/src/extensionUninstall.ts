// SPDX-License-Identifier: Apache-2.0
/** Standalone VS Code uninstall hook; never loads the editor API or starts a service. */
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { createHash, randomUUID } from 'node:crypto';
import type { NativeRunner } from './serviceBootstrap';
import { record } from './serviceClient';
import { LaunchSpec } from './model';
import { migrateLocalAgents } from './agentMigration';
import { readJsonConfig } from './agentConfig';
import type { ShellTouch } from './cliInstall';
import { defaultProfiles, removeShellIntegration, ShellRemoval } from './cliRemove';

interface Receipt { schema:1; installDir:string; storage:string; spec?:LaunchSpec }
const limit=1024, receiptLimit=32768;
const same=(a:string,b:string)=>path.relative(process.platform==='win32'?path.toNamespacedPath(a):a,process.platform==='win32'?path.toNamespacedPath(b):b)==='';
function absolute(value:unknown):string {
    if(typeof value!=='string'||!path.isAbsolute(value)||value.length>4096||/[\0\r\n]/.test(value))throw new Error('Invalid uninstall receipt path');
    return value;
}
function validate(value:unknown):Receipt {
    const item=record(value);if(item.schema!==1)throw new Error('Invalid uninstall receipt schema');
    const installDir=absolute(item.installDir),storage=absolute(item.storage);
    let spec:LaunchSpec|undefined;
    if(item.spec){
        const saved=record(item.spec),service=record(saved.service);
        if(!same(absolute(service.installDir),installDir)||typeof service.project!=='string'||!/^[a-f0-9]{64}$/.test(service.project)
            ||typeof saved.collection!=='string'||!/^[A-Za-z0-9_-][A-Za-z0-9._-]{0,127}$/.test(saved.collection))throw new Error('Invalid uninstall project binding');
        spec={command:absolute(saved.command),root:absolute(saved.root),dataDir:absolute(saved.dataDir),collection:saved.collection,name:'AIRK',
            service:{installDir,project:service.project,universal:service.universal===true}};
    }
    return {schema:1,installDir,storage,...(spec?{spec}:{})};
}
function receiptName(value:Receipt):string {
    return createHash('sha256').update(JSON.stringify([value.installDir,value.spec?.root??''])).digest('hex')+'.json';
}
async function directory(root:string):Promise<string> {
    const destination=path.join(absolute(root),'extension-uninstall');
    // Check existing ancestors before creating anything through them.
    const ensure=async(target:string):Promise<void>=>{
        try{const info=await fs.lstat(target);if(!info.isDirectory()||info.isSymbolicLink())throw new Error('Uninstall receipt directory redirected');}
        catch(error){
            if((error as NodeJS.ErrnoException).code!=='ENOENT')throw error;
            const parent=path.dirname(target);if(parent===target)throw error;
            await ensure(parent);
            try{await fs.mkdir(target,{mode:0o700});}catch(error){if((error as NodeJS.ErrnoException).code!=='EEXIST')throw error;}
        }
    };
    await ensure(destination);
    const info=await fs.lstat(destination);
    if(!info.isDirectory()||info.isSymbolicLink()||!same(await fs.realpath(destination),destination))throw new Error('Uninstall receipt directory redirected');
    return destination;
}
export async function rememberUninstallTarget(launcherDirectory:string,installDir:string,storage:string,spec?:LaunchSpec):Promise<void> {
    const item=validate({schema:1,installDir,storage,spec}),root=await directory(launcherDirectory);
    const names=await fs.readdir(root),name=receiptName(item);
    if(names.length>=limit&&!names.includes(name))throw new Error('Uninstall receipt capacity exceeded');
    const target=path.join(root,name),before=await readJsonConfig(target);
    const bytes=JSON.stringify(item);
    if(before===bytes)return;
    const temporary=path.join(root,`${randomUUID()}.tmp`);
    try{
        await fs.writeFile(temporary,bytes,{flag:'wx',mode:0o600});
        const current=await readJsonConfig(target);
        if(current===bytes)return; // Another window completed the same receipt.
        if(current!==before)throw new Error('Uninstall receipt changed concurrently');
        await fs.rename(temporary,target);
    }finally{await fs.unlink(temporary).catch(()=>undefined);}
}
interface ShellReceipt { schema:1; storage:string; profiles:ShellTouch[] }
const shellName=(storage:string)=>`shell-${createHash('sha256').update(storage).digest('hex')}.json`;
function validateShell(value:unknown):ShellReceipt {
    const item=record(value);if(item.schema!==1||!Array.isArray(item.profiles)||item.profiles.length>64)throw new Error('Invalid shell integration receipt');
    return {schema:1,storage:absolute(item.storage),profiles:item.profiles.map(raw=>{const entry=record(raw);return {file:absolute(entry.file),created:entry.created===true};})};
}
/** Record startup files that hold an AIRK block. "created" stays true once observed so
 * removal can delete a file AIRK created; files are never deleted otherwise. */
export async function rememberShellIntegration(launcherDirectory:string,storage:string,touched:readonly ShellTouch[]):Promise<void>{
    const root=await directory(launcherDirectory),target=path.join(root,shellName(absolute(storage)));
    const before=await readJsonConfig(target);
    const merged=new Map<string,ShellTouch>();
    if(before){for(const entry of validateShell(JSON.parse(before)).profiles)merged.set(entry.file,entry);}
    for(const entry of touched){const file=absolute(entry.file);merged.set(file,{file,created:entry.created||merged.get(file)?.created===true});}
    const bytes=JSON.stringify({schema:1,storage,profiles:[...merged.values()].slice(0,64)});
    if(before===bytes)return;
    const temporary=path.join(root,`${randomUUID()}.tmp`);
    try{
        await fs.writeFile(temporary,bytes,{flag:'wx',mode:0o600});
        if(await readJsonConfig(target)!==before)throw new Error('Shell integration receipt changed concurrently');
        await fs.rename(temporary,target);
    }finally{await fs.unlink(temporary).catch(()=>undefined);}
}
/** Recorded startup files for one extension storage root; empty when none were recorded. */
export async function shellProfiles(launcherDirectory:string,storage:string):Promise<ShellTouch[]>{
    const source=await readJsonConfig(path.join(absolute(launcherDirectory),'extension-uninstall',shellName(absolute(storage))));
    return source?validateShell(JSON.parse(source)).profiles:[];
}
async function saveConfig(destination:string,before:string,after:string):Promise<void> {
    const temporary=path.join(path.dirname(destination),`.airk-uninstall-${randomUUID()}.tmp`);
    try{
        await fs.writeFile(temporary,after,{flag:'wx',mode:(await fs.stat(destination)).mode});
        if(await readJsonConfig(destination)!==before)throw new Error('Agent configuration changed during uninstall');
        await fs.rename(temporary,destination);
    }finally{await fs.unlink(temporary).catch(()=>undefined);}
}

/** Delete only the owned, bounded Workbench hierarchy. Never follow directory links. */
export async function removeWorkbenchSessions(storage:string):Promise<void>{
    const base=absolute(storage),root=path.join(base,'workbench');
    if(path.relative(base,root)!=='workbench')throw new Error('Unsafe Workbench cleanup target');
    let directories:string[];
    try{if((await fs.lstat(root)).isSymbolicLink())throw new Error('Workbench storage redirected');directories=await fs.readdir(root);}
    catch(error){if((error as NodeJS.ErrnoException).code==='ENOENT')return;throw error;}
    if(directories.length>1024)throw new Error('Workbench cleanup inventory exceeds its bound');
    let count=0;
    for(const project of directories){
        if(!/^[a-f0-9]{64}$/.test(project))throw new Error('Unknown Workbench project directory');
        const directory=path.join(root,project),info=await fs.lstat(directory);
        if(!info.isDirectory()||info.isSymbolicLink())throw new Error('Workbench project redirected');
        const files=await fs.readdir(directory);count+=files.length;if(count>4096)throw new Error('Workbench cleanup exceeds file budget');
        for(const name of files){
            if(!/^(?:[a-f0-9-]{36}\.json|[a-f0-9-]{36}\.[a-f0-9-]{36}\.tmp|shortcuts\.json|\.writer)$/.test(name))throw new Error('Unknown Workbench session file');
            const target=path.join(directory,name),stat=await fs.lstat(target);if(stat.isDirectory())throw new Error('Unexpected Workbench subdirectory');await fs.unlink(target);
        }
        await fs.rmdir(directory);
    }
    await fs.rmdir(root);
}

type ShellRemover=(storage:string,recorded:ShellTouch[])=>Promise<ShellRemoval>;
const removeOwnedShell:ShellRemover=(storage,recorded)=>removeShellIntegration(storage,[...defaultProfiles(),...recorded]);
export async function uninstallTargets(main:string,run:NativeRunner,log:(message:string)=>void,removeShell:ShellRemover=removeOwnedShell):Promise<void> {
    const targets=new Map<string,{receipts:{file:string;item:Receipt}[]}>(),failures:string[]=[];
    const shells=new Map<string,{files:string[];profiles:ShellTouch[]}>();
    // Native user locations find both fresh and historical default layouts. Receipts
    // also cover custom installation roots and closed workspaces used by this extension.
    for(const channel of ['stable','preview']){
        try{
            const location=await run(main,['runtime','location','--channel',channel]);
            if(location.protocol!==1)throw new Error('Unexpected native location protocol');
            const installDir=absolute(location.install_dir);
            targets.set(installDir,targets.get(installDir)??{receipts:[]});
            const root=path.join(absolute(record(location.user_paths).launcher),'extension-uninstall');
            let names:string[];
            try{names=await fs.readdir(root);}catch(error){if((error as NodeJS.ErrnoException).code==='ENOENT')continue;throw error;}
            if(names.length>limit||!same(await fs.realpath(root),root))throw new Error('Unsafe or oversized uninstall receipt directory');
            for(const name of names){
                if(/^shell-[a-f0-9]{64}\.json$/.test(name)){
                    const file=path.join(root,name),source=await readJsonConfig(file);
                    if(!source||Buffer.byteLength(source)>receiptLimit)throw new Error('Invalid shell integration receipt size');
                    const item=validateShell(JSON.parse(source));
                    if(shellName(item.storage)!==name)throw new Error('Shell integration receipt identity changed');
                    const entry=shells.get(item.storage)??{files:[],profiles:[]};entry.files.push(file);entry.profiles.push(...item.profiles);shells.set(item.storage,entry);
                    continue;
                }
                if(!/^[a-f0-9]{64}\.json$/.test(name))continue;
                const file=path.join(root,name),source=await readJsonConfig(file);
                if(!source||Buffer.byteLength(source)>receiptLimit)throw new Error('Invalid uninstall receipt size');
                const item=validate(JSON.parse(source));
                if(receiptName(item)!==name)throw new Error('Uninstall receipt identity changed');
                const target=targets.get(item.installDir)??{receipts:[]};target.receipts.push({file,item});targets.set(item.installDir,target);
            }
        }catch(error){failures.push(`${channel}: ${String(error)}`);}
    }
    for(const [installDir,target] of targets){
        try{
            let clean=true;
            for(const {item} of target.receipts){
                if(!item.spec)continue;
                for(const message of await migrateLocalAgents(item.spec,item.storage,()=>{},saveConfig,true,true)){
                    log(message);if(message.startsWith('Could not '))clean=false;
                }
            }
            let exists=true;
            try{await fs.lstat(installDir);}catch(error){if((error as NodeJS.ErrnoException).code==='ENOENT')exists=false;else throw error;}
            if(exists){
                log(`Stopping and uninstalling AIRK runtime: ${installDir}`);
                const result=await run(main,['runtime','uninstall','--install-dir',installDir],180000);
                if(result.uninstalled!==true)throw new Error(`Runtime uninstall incomplete: ${JSON.stringify(result)}`);
            }
            // The command no longer has a runtime behind it: retire owned PATH entries,
            // profile blocks and wrappers. Modified or redirected files are preserved and reported.
            const storages=new Set(target.receipts.map(receipt=>receipt.item.storage));
            for(const storage of storages){
                const shell=shells.get(storage);
                const removal=await removeShell(storage,shell?.profiles??[]);
                for(const message of removal.removed)log(message);
                for(const message of removal.preserved){log(message);clean=false;}
                if(removal.preserved.length===0)for(const file of shell?.files??[])await fs.unlink(file).catch(()=>undefined);
            }
            if(!clean)throw new Error('Runtime removed; some agent entries or shell integration require manual review (see messages above)');
            for(const storage of storages)await removeWorkbenchSessions(storage);
            for(const {file} of target.receipts)await fs.unlink(file);
        }catch(error){failures.push(`${installDir}: ${String(error)}`);}
    }
    if(failures.length)throw new Error(`AIRK uninstall needs attention:\n${failures.join('\n')}`);
    log('AIRK runtime and recorded agent connections removed. Project sources and index data were preserved.');
}
