// SPDX-License-Identifier: Apache-2.0
// Callers own destination/scope authority; this helper only publishes complete JSON.
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const {performance}=require('node:perf_hooks');
const LIMIT=16*1024*1024;
const RENAME_DELAYS=[100,200,400,600,700,800];
function budget(options){const maximum=options?.maxBytes??LIMIT;if(!Number.isSafeInteger(maximum)||maximum<1||maximum>LIMIT)throw Error('Invalid editor mailbox byte budget');return maximum;}
async function parentFor(filename){
    if(typeof filename!=='string'||!path.isAbsolute(filename)||!path.basename(filename)||['.','..'].includes(path.basename(filename)))throw Error('Absolute editor mailbox filename required');
    const parent=path.dirname(filename),info=await fs.lstat(parent),canonical=await fs.realpath(parent);
    if(!info.isDirectory()||info.isSymbolicLink()||path.relative(parent,canonical)!=='')throw Error('Editor mailbox parent must be an existing canonical directory');
    return parent;
}
function sameFile(info,identity){return info.isFile()&&!info.isSymbolicLink()&&info.dev===identity.dev&&info.ino===identity.ino;}
async function publishMailbox(filename,value,options){
    const maximum=budget(options),serialized=JSON.stringify(value);
    if(typeof serialized!=='string')throw Error('Editor mailbox requires a JSON value');
    const bytes=Buffer.from(serialized);if(bytes.length>maximum)throw Error('Editor mailbox JSON exceeds its byte budget');
    const parent=await parentFor(filename),stage=path.join(parent,`.${path.basename(filename)}.${crypto.randomUUID()}.stage`);
    let file,identity,created=false,primary;
    try{
        file=await fs.open(stage,'wx',0o600);created=true;identity=await file.stat();
        await file.writeFile(bytes);await file.close();file=undefined;
        const deadline=performance.now()+3000;let previous;
        for(let attempt=0;;attempt++){
            if(attempt&&performance.now()>=deadline)throw previous;
            if(!sameFile(await fs.lstat(stage),identity))throw Error('Editor mailbox stage identity changed');
            try{await fs.rename(stage,filename);created=false;break;}catch(error){
                previous=error;
                // Windows readers/security scans can temporarily deny replacing
                // an open destination. Keep complete bytes; never delete it.
                if(process.platform!=='win32'||!['EPERM','EACCES','EBUSY'].includes(error.code)||attempt>=RENAME_DELAYS.length)throw error;
                try{const target=await fs.lstat(filename);if(!target.isFile()||target.isSymbolicLink())throw error;}catch(targetError){if(targetError.code!=='ENOENT')throw error;}
                const delay=RENAME_DELAYS[attempt];if(performance.now()+delay>=deadline)throw error;
                await new Promise(resolve=>setTimeout(resolve,delay));
            }
        }
    }catch(error){primary=error;throw error;}finally{
        const cleanupErrors=[];
        try{await file?.close();}catch(error){cleanupErrors.push(error);}
        if(created&&identity)try{
            let current;try{current=await fs.lstat(stage);}catch(error){if(error.code!=='ENOENT')throw error;}
            if(current&&sameFile(current,identity))await fs.unlink(stage);
        }catch(error){cleanupErrors.push(error);}
        if(cleanupErrors.length){
            if(primary===undefined)throw cleanupErrors[0];
            if(primary instanceof Error){primary.cause??=cleanupErrors[0];primary.cleanupErrors=cleanupErrors;}
        }
    }
}
async function readMailbox(filename,options){
    const maximum=budget(options);await parentFor(filename);
    let file,primary;
    try{file=await fs.open(filename,'r');}catch(error){if(error.code==='ENOENT')return undefined;throw error;}
    try{
        const stat=await file.stat();if(!stat.isFile()||stat.size>maximum)throw Error('Editor mailbox JSON exceeds its byte budget');
        const bytes=Buffer.alloc(stat.size+1);let used=0;
        while(used<bytes.length){const {bytesRead}=await file.read(bytes,used,bytes.length-used,null);if(!bytesRead)break;used+=bytesRead;}
        if(used>stat.size||used>maximum)throw Error('Editor mailbox grew during observation');
        return JSON.parse(bytes.subarray(0,used).toString('utf8'));
    }catch(error){primary=error;throw error;}finally{
        try{await file.close();}catch(error){
            if(primary===undefined)throw error;
            if(primary instanceof Error){primary.cause??=error;primary.cleanupErrors=[error];}
        }
    }
}
module.exports={publishMailbox,readMailbox};
