// SPDX-License-Identifier: Apache-2.0
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { randomUUID } from 'node:crypto';
import { object, Value, WorkbenchSession } from './workbenchContracts';

const LIMIT = 4 * 1024 * 1024;
const validId = (id: string) => /^[a-f0-9-]{36}$/.test(id);
export function newSession(project: string): WorkbenchSession {
    return {schema:1,id:randomUUID(),project,title:'New session',createdAt:Date.now(),updatedAt:Date.now(),revision:0,omitted:0,turns:[]};
}
export function trimSession(session: WorkbenchSession): void {
    while (session.turns.length > 200) { session.turns.shift(); session.omitted++; }
    let bytes=Buffer.byteLength(JSON.stringify(session));
    for (const turn of session.turns) {
        if (bytes <= LIMIT-64) { break; }
        const before=Buffer.byteLength(JSON.stringify(turn));
        delete turn.envelope; turn.trimmed=true;
        bytes-=before-Buffer.byteLength(JSON.stringify(turn));
    }
    while (bytes > LIMIT-64 && session.turns.length) {bytes-=Buffer.byteLength(JSON.stringify(session.turns.shift()))+1;session.omitted++;}
}
/** All writes use a project-wide exclusive lock and a revision check. Sessions are inert. */
export class SessionStore {
    constructor(private readonly directory: string) {}
    private async root(project: string): Promise<string> {
        if (!/^[0-9a-f]{64}$/.test(project)) { throw new Error('Invalid Workbench project binding.'); }
        for (const directory of [this.directory, path.join(this.directory, project)]) {
            await fs.mkdir(directory,{recursive:true}); if ((await fs.lstat(directory)).isSymbolicLink()) { throw new Error('Unsafe session directory.'); }
        }
        return path.join(this.directory,project);
    }
    private async scan(project: string): Promise<WorkbenchSession[]> {
        const root=await this.root(project), sessions:WorkbenchSession[]=[];
        const names=await fs.readdir(root); if (names.length>100) {throw new Error('Session directory exceeds its bounded inventory.');}
        for (const file of names.filter(n=>n.endsWith('.json') && validId(n.slice(0,-5)))) {
            try { sessions.push(await this.load(project,file.slice(0,-5))); } catch { /* Invalid/future files never execute or block valid sessions. */ }
        }
        return sessions.sort((a,b)=>b.updatedAt-a.updatedAt);
    }
    async list(project:string):Promise<WorkbenchSession[]>{return (await this.scan(project)).slice(0,10);}
    async load(project: string,id: string): Promise<WorkbenchSession> {
        if (!validId(id)) {throw new Error('Invalid session ID.');}
        const file=path.join(await this.root(project),`${id}.json`), stat=await fs.lstat(file);
        if (stat.isSymbolicLink() || !stat.isFile() || stat.size>LIMIT) {throw new Error('Unsafe or oversized session.');}
        const session=JSON.parse(await fs.readFile(file,'utf8')) as WorkbenchSession;
        if (session.schema!==1 || session.id!==id || session.project!==project || !Array.isArray(session.turns) || session.turns.length>200
            || !Number.isSafeInteger(session.revision) || typeof session.title!=='string' || session.title.length>80
            || session.turns.some(t=>typeof t.input!=='string'||t.input.length>4096||typeof t.id!=='string'|| !['structured','json','markdown'].includes(t.format))) {throw new Error('Unsupported or corrupt session.');}
        return session;
    }
    private async lock<T>(project:string, action:(root:string)=>Promise<T>):Promise<T> {
        const root=await this.root(project), file=path.join(root,'.writer');
        let handle;
        try {handle=await fs.open(file,'wx',0o600);} catch {throw new Error('Another Workbench window is saving this project; retry or fork the session.');}
        try {return await action(root);} finally {await handle.close();await fs.unlink(file);}
    }
    async save(session:WorkbenchSession,persistResults=true):Promise<void> {
        if (!session.turns.length) {return;}
        trimSession(session);
        await this.lock(session.project,async root=>{
            let current:WorkbenchSession|undefined;
            try {current=await this.load(session.project,session.id);} catch(error) {if((error as NodeJS.ErrnoException).code!=='ENOENT'){throw error;}}
            if(current && current.revision!==session.revision){throw new Error('Session changed in another window. Open it again or start a new session.');}
            const saved=structuredClone(session);saved.revision++;
            if(!persistResults){for(const turn of saved.turns){delete turn.envelope;turn.trimmed=true;}}
            const file=path.join(root,`${session.id}.json`),temporary=path.join(root,`${session.id}.${randomUUID()}.tmp`);
            try {await fs.writeFile(temporary,JSON.stringify(saved),{flag:'wx',mode:0o600});await fs.rename(temporary,file);session.revision=saved.revision;}
            finally {await fs.unlink(temporary).catch(()=>undefined);}
            const recent=await this.scan(session.project);
            for(const expired of recent.slice(10)){await fs.unlink(path.join(root,`${expired.id}.json`));}
        });
    }
    async remove(project:string,id?:string):Promise<void>{
        await this.lock(project,async root=>{for(const name of await fs.readdir(root)){if((name.endsWith('.json')&&validId(name.slice(0,-5))&&(!id||name===`${id}.json`))||(!id&&name==='shortcuts.json')){await fs.unlink(path.join(root,name));}}});
    }
    /** Project directories with saved state; bounded like each project's inventory. */
    async projects():Promise<string[]>{
        let entries;
        try{entries=await fs.readdir(this.directory,{withFileTypes:true});}catch(error){if((error as NodeJS.ErrnoException).code==='ENOENT'){return [];}throw error;}
        if(entries.length>1024){throw new Error('Workbench storage exceeds its bounded inventory.');}
        return entries.filter(entry=>entry.isDirectory()&&!entry.isSymbolicLink()&&/^[0-9a-f]{64}$/.test(entry.name)).map(entry=>entry.name);
    }
    async clearResults(project:string):Promise<void>{
        for(const session of await this.scan(project)){for(const turn of session.turns){delete turn.envelope;turn.trimmed=true;}await this.save(session,false);}
    }
    private async shortcuts(project:string):Promise<{input:string;scope:string;plan:Value}[]>{
        const file=path.join(await this.root(project),'shortcuts.json');
        try{const stat=await fs.lstat(file);if(!stat.isFile()||stat.isSymbolicLink()||stat.size>131072){throw new Error('Unsafe shortcut library.');}
            const data=object(JSON.parse(await fs.readFile(file,'utf8')));if(data.schema!==1||!Array.isArray(data.entries)||data.entries.length>32){throw new Error('Unsupported shortcut library.');}
            return data.entries.filter((raw):raw is {input:string;scope:string;plan:Value}=>{const entry=object(raw);return typeof entry.input==='string'&&entry.input.length<=4096&&typeof entry.scope==='string'&&entry.scope.length<=4096&&Object.keys(object(entry.plan)).length>0;});
        }catch(error){if((error as NodeJS.ErrnoException).code==='ENOENT'){return [];}throw error;}
    }
    async findShortcut(project:string,input:string,scope:string):Promise<Value|undefined>{
        // Case and whitespace inside quoted paths/identifiers can change the query.
        const normalize=(value:string)=>value.trim();
        return (await this.shortcuts(project)).find(entry=>normalize(entry.input)===normalize(input)&&entry.scope===scope)?.plan;
    }
    async remember(project:string,input:string,plan:Value):Promise<void>{
        if(!Array.isArray(plan.steps)||!plan.steps.length){throw new Error('Prepare an executable plan before remembering it.');}
        const template=structuredClone(plan);delete template.digest;delete template.publication;delete template.collection;
        for(const raw of Array.isArray(template.steps)?template.steps:[]){const args=object(object(raw).args);if(args.symbol){throw new Error('A direct symbol ID cannot be remembered; bind symbol IDs from a fresh definition query.');}for(const key of ['known_hash','revision','cursor']){delete args[key];}}
        await this.lock(project,async root=>{
            const scope=String(template.path_prefix??''),entries=(await this.shortcuts(project)).filter(entry=>entry.input!==input||entry.scope!==scope);
            entries.unshift({input,scope,plan:template});while(entries.length>32||Buffer.byteLength(JSON.stringify({schema:1,entries}))>131072){entries.pop();}
            const temporary=path.join(root,`${randomUUID()}.${randomUUID()}.tmp`);
            try{await fs.writeFile(temporary,JSON.stringify({schema:1,entries}),{flag:'wx',mode:0o600});await fs.rename(temporary,path.join(root,'shortcuts.json'));}finally{await fs.unlink(temporary).catch(()=>undefined);}
        });
    }
}
