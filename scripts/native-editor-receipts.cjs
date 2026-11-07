// SPDX-License-Identifier: Apache-2.0
// Test harness cleanup only: never enumerate or run an account-wide uninstall hook.
const fs=require('node:fs/promises'),path=require('node:path'),crypto=require('node:crypto');
const digest=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const normalized=value=>process.platform==='win32'?path.toNamespacedPath(path.resolve(value)).toLowerCase():path.resolve(value);
const same=(left,right)=>normalized(left)===normalized(right);
function within(root,value){const relative=path.relative(normalized(root),normalized(value));return !!relative&&relative!=='..'&&!relative.startsWith('..'+path.sep)&&!path.isAbsolute(relative);}
function receiptName(installDir,root){return digest(JSON.stringify([installDir,root]))+'.json';}
function absolute(value){if(typeof value!=='string'||value.length>4096||!path.isAbsolute(value)||/[\0\r\n]/.test(value))throw Error('Invalid exact receipt cleanup path');return value;}
async function plannedCanonical(value){
    value=absolute(value);try{return await fs.realpath(value);}catch(error){if(error.code!=='ENOENT')throw error;const parent=path.dirname(value);if(parent===value)throw error;return path.join(await plannedCanonical(parent),path.basename(value));}
}
async function directory(value){const stat=await fs.lstat(value);if(!stat.isDirectory()||stat.isSymbolicLink()||!same(await fs.realpath(value),value))throw Error('Receipt cleanup directory is redirected');}
/** Baseline exact possible filenames before the first product activation. */
async function prepareOwnedReceipts({launcher,fixture,installDir,sourceRoots}){
    fixture=await fs.realpath(absolute(fixture));installDir=absolute(installDir);launcher=absolute(launcher);
    if(!within(fixture,installDir)||!same(await plannedCanonical(installDir),installDir))throw Error('Receipt installation is outside the exact canonical fixture');
    if(!Array.isArray(sourceRoots)||sourceRoots.length>4)throw Error('Receipt source-root budget exceeded');
    const roots=['',...new Set(sourceRoots)];
    for(const root of roots.filter(Boolean))if(!within(fixture,root)||!same(await fs.realpath(root),root))throw Error('Receipt source is outside the exact canonical fixture');
    const canonicalLauncher=await plannedCanonical(launcher);
    // The native location observation supplies this path. Do not create it or
    // follow an existing redirected launcher/receipt directory during cleanup.
    if(!same(canonicalLauncher,launcher))throw Error('Native launcher location is redirected');
    const receiptRoot=path.join(canonicalLauncher,'extension-uninstall');
    try{await directory(receiptRoot);}catch(error){if(error.code!=='ENOENT')throw error;}
    const targets=roots.map(root=>({root,name:receiptName(installDir,root)}));
    for(const target of targets){try{await fs.lstat(path.join(receiptRoot,target.name));throw Error('Existing uninstall receipt collides with the fresh fixture');}catch(error){if(error.code!=='ENOENT')throw error;}}
    return {schema:1,fixture,installDir,launcher:canonicalLauncher,receiptRoot,targets,baseline:'all_exact_targets_absent'};
}
async function readReceipt(filename){
    const before=await fs.lstat(filename);
    if(!before.isFile()||before.isSymbolicLink()||before.nlink!==1||before.size>32768)throw Error('Receipt is not one bounded unlinked regular file');
    const handle=await fs.open(filename,'r');
    try{
        const opened=await handle.stat();if(opened.dev!==before.dev||opened.ino!==before.ino||opened.size!==before.size)throw Error('Receipt identity changed before read');
        const bytes=Buffer.alloc(32769);let used=0;
        while(used<bytes.length){const read=await handle.read(bytes,used,bytes.length-used,null);if(!read.bytesRead)break;used+=read.bytesRead;}
        if(used!==before.size||used>32768)throw Error('Receipt changed or exceeds its read bound');
        return {bytes:bytes.subarray(0,used),stat:before};
    }finally{await handle.close();}
}
async function ownedStorage(storage,profileRoots){
    storage=absolute(storage);
    for(const profile of profileRoots){
        const root=await fs.realpath(absolute(profile));
        if(!same(root,profile))throw Error('Owned editor profile is redirected');
        const expected=path.join(root,'User','globalStorage','iokaio.airk');
        if(!same(expected,storage))continue;
        let current=root;await directory(current);
        for(const part of ['User','globalStorage','iokaio.airk']){current=path.join(current,part);await directory(current);}
        return;
    }
    throw Error('Receipt storage does not belong to an exact owned editor profile');
}
/** Remove only the exact newly created fixture receipts after complete teardown. */
async function cleanupOwnedReceipts(plan,{profileRoots,editorsClosed,stop,evidenceDirectory}){
    if(plan?.schema!==1||plan.baseline!=='all_exact_targets_absent'||!Array.isArray(plan.targets)||plan.targets.length>5)throw Error('Invalid receipt cleanup plan');
    if(!same(await fs.realpath(plan.fixture),plan.fixture)||!within(plan.fixture,plan.installDir)||!same(await plannedCanonical(plan.installDir),plan.installDir)||!same(plan.receiptRoot,path.join(plan.launcher,'extension-uninstall')))throw Error('Receipt cleanup plan escaped its exact fixture');
    if(!editorsClosed||stop?.qualified!==true||!same(stop.stop?.requested_binding?.installDir??'',plan.installDir)
        ||!same(stop.stop?.capture?.runtime?.installDir??'',plan.installDir)||stop.observation?.lifecycle?.explicitly_stopped!==true)throw Error('Owned editors and authenticated fixture service Stop must complete before receipt cleanup');
    if(!Array.isArray(profileRoots)||profileRoots.length>4)throw Error('Owned receipt profile budget exceeded');
    const removed=[],absent=[];
    for(const target of plan.targets){
        if(target.name!==receiptName(plan.installDir,target.root))throw Error('Receipt cleanup filename does not match its exact binding');
        const filename=path.join(plan.receiptRoot,target.name);let observed;
        try{await directory(plan.launcher);await directory(plan.receiptRoot);observed=await readReceipt(filename);}catch(error){if(error.code==='ENOENT'){absent.push(target.name);continue;}throw error;}
        const value=JSON.parse(observed.bytes.toString('utf8'));
        if(!value||value.schema!==1||Object.keys(value).some(key=>!['schema','installDir','storage','spec'].includes(key))||value.installDir!==plan.installDir)throw Error('Receipt belongs to another installation or schema');
        if((value.spec?.root??'')!==target.root||receiptName(value.installDir,value.spec?.root??'')!==target.name)throw Error('Receipt source binding does not match its exact filename');
        if(value.spec){
            if(value.spec.service?.installDir!==plan.installDir||!within(plan.fixture,value.spec.root)||!within(plan.fixture,absolute(value.spec.dataDir))
                ||!within(plan.installDir,absolute(value.spec.command))||!/^[a-f0-9]{64}$/.test(value.spec.service.project)||!/^[A-Za-z0-9_-][A-Za-z0-9._-]{0,127}$/.test(value.spec.collection))throw Error('Receipt project binding is not the exact owned fixture');
        }
        await ownedStorage(value.storage,profileRoots);
        // Retain the exact receipt before deleting it; failure preserves the file.
        await fs.mkdir(evidenceDirectory,{recursive:true});
        await fs.writeFile(path.join(evidenceDirectory,target.name),observed.bytes,{flag:'wx',mode:0o600});
        const confirmed=await readReceipt(filename);
        if(confirmed.stat.dev!==observed.stat.dev||confirmed.stat.ino!==observed.stat.ino||confirmed.stat.mtimeMs!==observed.stat.mtimeMs||confirmed.stat.ctimeMs!==observed.stat.ctimeMs||!confirmed.bytes.equals(observed.bytes))throw Error('Receipt changed before exact owned cleanup');
        await directory(plan.launcher);await directory(plan.receiptRoot);await fs.unlink(filename);
        removed.push({name:target.name,sha256:digest(observed.bytes),bytes:observed.bytes.length,storage:value.storage});
    }
    return {schema:1,installDir:plan.installDir,receiptRoot:plan.receiptRoot,removed,absent,unrelated_files_enumerated:false,account_directory_removed:false};
}
async function verifyOwnedReceiptsAbsent(plan){
    for(const target of plan.targets){try{await fs.lstat(path.join(plan.receiptRoot,target.name));throw Error('Unexpected receipt without a qualified service Stop; receipt and profile retained');}catch(error){if(error.code!=='ENOENT')throw error;}}
    return {schema:1,installDir:plan.installDir,removed:[],absent:plan.targets.map(target=>target.name),unrelated_files_enumerated:false,account_directory_removed:false};
}
module.exports={prepareOwnedReceipts,cleanupOwnedReceipts,verifyOwnedReceiptsAbsent,receiptName};
