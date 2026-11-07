// SPDX-License-Identifier: Apache-2.0
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs/promises'),path=require('node:path'),os=require('node:os');
const {prepareOwnedReceipts,cleanupOwnedReceipts,verifyOwnedReceiptsAbsent,receiptName}=require('../native-editor-receipts.cjs');
async function fixture(){
    const base=await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(),'airk-receipt-test-'))),owned=path.join(base,'fixture'),launcher=path.join(base,'account-launcher'),profile=path.join(base,'owned-code-profile');
    const source=path.join(owned,'source'),installDir=path.join(owned,'native-service'),storage=path.join(profile,'User','globalStorage','iokaio.airk'),receiptRoot=path.join(launcher,'extension-uninstall');
    for(const directory of [source,installDir,storage,receiptRoot])await fs.mkdir(directory,{recursive:true});
    const plan=await prepareOwnedReceipts({launcher,fixture:owned,installDir,sourceRoots:[source]});
    const file=path.join(receiptRoot,receiptName(installDir,source)),value={schema:1,installDir,storage,spec:{root:source,command:path.join(installDir,'airk-launcher'),dataDir:path.join(owned,'data'),collection:'fixture',service:{installDir,project:'a'.repeat(64)}}};
    const stop={qualified:true,stop:{requested_binding:{installDir},capture:{runtime:{installDir}}},observation:{lifecycle:{explicitly_stopped:true}}};
    const options={profileRoots:[profile],editorsClosed:true,stop,evidenceDirectory:path.join(owned,'removed-receipts')};
    return {base,owned,profile,storage,launcher,receiptRoot,source,installDir,plan,file,value,options,write:()=>fs.writeFile(file,JSON.stringify(value)),close:()=>fs.rm(base,{recursive:true,force:true})};
}
test('exact newly created fixture receipt is backed up and removed; sibling and account directory survive',async()=>{
    const f=await fixture();try{await f.write();const sibling=path.join(f.receiptRoot,'unrelated.json');await fs.writeFile(sibling,'unrelated sentinel');
        const receipt=await cleanupOwnedReceipts(f.plan,f.options);assert.equal(receipt.removed.length,1);assert.equal(receipt.unrelated_files_enumerated,false);assert.equal(receipt.account_directory_removed,false);
        await assert.rejects(fs.lstat(f.file),{code:'ENOENT'});assert.equal(await fs.readFile(sibling,'utf8'),'unrelated sentinel');
        assert.equal(await fs.readFile(path.join(f.options.evidenceDirectory,path.basename(f.file)),'utf8'),JSON.stringify(f.value));assert.ok((await fs.stat(f.receiptRoot)).isDirectory());
    }finally{await f.close();}
});
test('baseline collisions refuse even a receipt claiming the fixture; no overwrite or deletion',async()=>{
    const f=await fixture();try{await f.write();await assert.rejects(prepareOwnedReceipts({launcher:f.launcher,fixture:f.owned,installDir:f.installDir,sourceRoots:[f.source]}),/collides/);assert.equal(await fs.readFile(f.file,'utf8'),JSON.stringify(f.value));}finally{await f.close();}
});
test('missing exact targets need no account directory creation or unrelated inventory read',async()=>{
    const f=await fixture();try{await fs.rmdir(f.receiptRoot);await fs.rmdir(f.launcher);const receipt=await verifyOwnedReceiptsAbsent(f.plan);assert.equal(receipt.removed.length,0);await assert.rejects(fs.stat(f.launcher),{code:'ENOENT'});}finally{await f.close();}
});
test('foreign storage, crossed installation/root, and oversized receipts are retained',async()=>{
    for(const kind of ['storage','installation','root','oversize']){const f=await fixture();try{
        if(kind==='storage')f.value.storage=path.join(f.base,'normal-user-profile');
        if(kind==='installation')f.value.installDir=path.join(f.base,'other-install');
        if(kind==='root')f.value.spec.root=path.join(f.base,'other-source');
        if(kind==='oversize')f.value.unknown='x'.repeat(32769);
        await f.write();const before=await fs.readFile(f.file);await assert.rejects(cleanupOwnedReceipts(f.plan,f.options));assert.deepEqual(await fs.readFile(f.file),before);
    }finally{await f.close();}}
});
test('qualified Stop and every owned editor exit are required before any receipt deletion',async()=>{
    const f=await fixture();try{await f.write();for(const options of [{...f.options,editorsClosed:false},{...f.options,stop:{qualified:false}},{...f.options,stop:{...f.options.stop,observation:{lifecycle:{explicitly_stopped:false}}}}])await assert.rejects(cleanupOwnedReceipts(f.plan,options),/Stop/);
        assert.ok((await fs.stat(f.file)).isFile());await assert.rejects(verifyOwnedReceiptsAbsent(f.plan),/Unexpected receipt/);
    }finally{await f.close();}
});
test('hardlinked receipts and redirected exact profile storage are preserved',async()=>{
    for(const kind of ['hardlink','redirect']){const f=await fixture();try{await f.write();
        if(kind==='hardlink')await fs.link(f.file,path.join(f.base,'receipt-alias'));
        else{await fs.rmdir(f.storage);const target=path.join(f.base,'redirected-storage');await fs.mkdir(target);await fs.symlink(target,f.storage,process.platform==='win32'?'junction':'dir');}
        await assert.rejects(cleanupOwnedReceipts(f.plan,f.options));assert.ok((await fs.lstat(f.file)).isFile());
    }finally{await f.close();}}
});
test('receipt changes after evidence capture are detected and preserved',async()=>{
    const f=await fixture(),write=fs.writeFile;try{await f.write();const replacement=JSON.stringify({...f.value,storage:path.join(f.base,'changed-storage')});
        fs.writeFile=async function(file,...args){const result=await write.call(this,file,...args);if(String(file).startsWith(f.options.evidenceDirectory+path.sep))await write(f.file,replacement);return result;};
        await assert.rejects(cleanupOwnedReceipts(f.plan,f.options),/changed before/);assert.equal(await fs.readFile(f.file,'utf8'),replacement);
    }finally{fs.writeFile=write;await f.close();}
});
test('redirected receipt entries are preserved without reading or unlinking their target',async()=>{
    const f=await fixture();try{const target=path.join(f.base,'unrelated-directory');await fs.mkdir(target);await fs.writeFile(path.join(target,'sentinel'),'preserved');
        await fs.symlink(target,f.file,process.platform==='win32'?'junction':'dir');
        await assert.rejects(cleanupOwnedReceipts(f.plan,f.options),/regular file/);assert.ok((await fs.lstat(f.file)).isSymbolicLink());assert.equal(await fs.readFile(path.join(target,'sentinel'),'utf8'),'preserved');
    }finally{await f.close();}
});
