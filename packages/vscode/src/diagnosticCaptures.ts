// SPDX-License-Identifier: Apache-2.0
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { randomUUID } from 'node:crypto';

/** Finite derived captures, separate from retained log streams and user-saved exports. */
export class DiagnosticCaptures {
    private folder?:Promise<string>;
    private readonly files=new Set<string>();
    constructor(private readonly storage:string){}
    private async directory():Promise<string>{
        await fs.mkdir(this.storage,{recursive:true});const storage=await fs.realpath(this.storage),pool=path.join(storage,'diagnostic-captures');
        await fs.mkdir(pool,{recursive:true,mode:0o700});
        const info=await fs.lstat(pool);
        if(!info.isDirectory()||info.isSymbolicLink()||await fs.realpath(pool)!==pool)throw new Error('Diagnostic capture pool redirected');
        for(const entry of (await fs.readdir(pool)).slice(0,256)){
            const match=/^(\d+)-[a-f0-9-]{36}$/.exec(entry);if(!match)continue;
            let live=true;try{process.kill(Number(match[1]),0);}catch(error){if((error as NodeJS.ErrnoException).code==='ESRCH')live=false;}
            if(!live)await this.removeDirectory(path.join(pool,entry));
        }
        const folder=path.join(pool,`${process.pid}-${randomUUID()}`);await fs.mkdir(folder,{mode:0o700});return folder;
    }
    async allocate():Promise<string>{
        if(this.files.size>=4)throw new Error('Close an earlier diagnostic log before opening another capture.');
        this.folder??=this.directory();const file=path.join(await this.folder,`${randomUUID()}.jsonl`);this.files.add(file);return file;
    }
    async release(file:string):Promise<void>{
        if(!this.files.delete(file))return;
        try{await fs.unlink(file);}catch(error){if((error as NodeJS.ErrnoException).code!=='ENOENT')throw error;}
    }
    private async removeDirectory(folder:string):Promise<void>{
        const directory=await fs.lstat(folder);
        if(!directory.isDirectory()||directory.isSymbolicLink())return;
        const names=await fs.readdir(folder);if(names.length>12)return;
        for(const name of names){
            if(!/^(?:[a-f0-9-]{36}\.jsonl|\.tmp[A-Za-z0-9]{6,})$/.test(name))return;
            const info=await fs.lstat(path.join(folder,name));if(!info.isFile()||info.isSymbolicLink()||info.nlink!==1)return;
        }
        for(const name of names)await fs.unlink(path.join(folder,name));await fs.rmdir(folder);
    }
    dispose():void{
        void this.folder?.then(async folder=>{for(const file of [...this.files])await this.release(file);await this.removeDirectory(folder);}).catch(()=>{});
    }
}
