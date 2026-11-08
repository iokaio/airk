// SPDX-License-Identifier: Apache-2.0
// Short, explicitly owned editor profiles; never infer ownership from a basename.
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto'),{isDeepStrictEqual}=require('node:util');
const digest=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const assert=(value,message)=>{if(!value)throw Error(message);};
async function privateBytes(filename){const info=await fs.lstat(filename);assert(info.isFile()&&!info.isSymbolicLink()&&info.nlink===1&&info.size<=8192,'Owned profile receipt changed');return fs.readFile(filename);}
async function createOwnedProfile(run,{platform=process.platform,temporaryRoot='/tmp'}={}){
    run=await fs.realpath(run);
    const short=platform==='darwin',parent=short?await fs.realpath(temporaryRoot):run;
    const profile=short?await fs.realpath(await fs.mkdtemp(path.join(parent,'airk-hot-'))):path.join(run,'profile');
    if(!short)await fs.mkdir(profile,{mode:0o700});
    await fs.chmod(profile,0o700);
    assert(!short||Buffer.byteLength(profile)<=70,'Editor profile exceeds the short socket path budget');
    const stat=await fs.lstat(profile),marker={schema:1,purpose:'airk-explorer-hot-update',run,profile,parent,platform,nonce:crypto.randomBytes(16).toString('hex'),device:String(stat.dev),inode:String(stat.ino)};
    const bytes=JSON.stringify(marker),proof={...marker,marker_sha256:digest(bytes)};
    await fs.writeFile(path.join(profile,'.airk-owned-profile.json'),bytes,{flag:'wx',mode:0o600});
    await fs.writeFile(path.join(run,'profile-owned.json'),JSON.stringify(proof,null,2),{flag:'wx',mode:0o600});
    return proof;
}
async function finishOwnedProfile(proof,ownedEditorClosed){
    const receipt={profile:proof.profile,owned_editor_closed:ownedEditorClosed,removed:false,logs_copied:0,log_bytes:0};
    if(!ownedEditorClosed)return {...receipt,retained_reason:'Owned editor graceful exit was not confirmed'};
    if(proof.platform!=='darwin')return {...receipt,retained_reason:'Artifact-local profile retained'};
    try{
        assert(proof.schema===1&&proof.purpose==='airk-explorer-hot-update'&&path.isAbsolute(proof.profile)&&path.isAbsolute(proof.parent)&&path.isAbsolute(proof.run)&&/^[a-f0-9]{32}$/.test(proof.nonce),'Malformed owned profile proof');
        assert(path.dirname(proof.profile)===proof.parent&&/^airk-hot-[A-Za-z0-9]+$/.test(path.basename(proof.profile))&&Buffer.byteLength(proof.profile)<=70,'Owned short profile path differs');
        assert(await fs.realpath(proof.parent)===proof.parent&&await fs.realpath(proof.run)===proof.run&&await fs.realpath(proof.profile)===proof.profile,'Owned profile was redirected');
        const directory=await fs.lstat(proof.profile);assert(directory.isDirectory()&&!directory.isSymbolicLink()&&String(directory.dev)===proof.device&&String(directory.ino)===proof.inode,'Owned profile directory identity changed');
        if(process.platform!=='win32')assert((directory.mode&0o777)===0o700&&directory.uid===process.getuid(),'Owned profile permissions or owner changed');
        assert(isDeepStrictEqual(JSON.parse(await privateBytes(path.join(proof.run,'profile-owned.json'))),proof),'Artifact ownership receipt changed');
        const filename=path.join(proof.profile,'.airk-owned-profile.json'),stat=await fs.lstat(filename);assert(stat.isFile()&&!stat.isSymbolicLink()&&stat.nlink===1&&stat.size<=4096,'Owned profile marker changed');
        const bytes=await fs.readFile(filename);assert(digest(bytes)===proof.marker_sha256,'Owned profile marker digest changed');
        const {marker_sha256,...expected}=proof;assert(isDeepStrictEqual(JSON.parse(bytes),expected),'Owned profile marker binding changed');
        let directories=0;
        async function copyLogs(source,destination,depth=0){
            assert(depth<=12&&++directories<=256,'Editor log directory budget exceeded');
            let rows;try{rows=await fs.readdir(source,{withFileTypes:true});}catch(error){if(error.code==='ENOENT')return;throw error;}
            await fs.mkdir(destination,{recursive:true,mode:0o700});
            for(const row of rows){const input=path.join(source,row.name),output=path.join(destination,row.name);assert(!row.isSymbolicLink(),'Editor log symlink refused');if(row.isDirectory())await copyLogs(input,output,depth+1);else if(row.isFile()){const info=await fs.lstat(input);assert(++receipt.logs_copied<=256&&(receipt.log_bytes+=info.size)<=33554432,'Editor log byte/file budget exceeded');await fs.copyFile(input,output);}}
        }
        await copyLogs(path.join(proof.profile,'logs'),path.join(proof.run,'profile-logs'));
        // The exact canonical directory and immutable ownership receipts above
        // are checked before this sole recursive removal.
        await fs.rm(proof.profile,{recursive:true,maxRetries:2,retryDelay:100});receipt.removed=true;
        return receipt;
    }catch(error){return {...receipt,retained_reason:String(error)};}
}
module.exports={createOwnedProfile,finishOwnedProfile};
