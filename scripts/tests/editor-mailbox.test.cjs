// SPDX-License-Identifier: Apache-2.0
const {test}=require('node:test'),assert=require('node:assert/strict'),fs=require('node:fs/promises'),os=require('node:os'),path=require('node:path'),crypto=require('node:crypto');
const {publishMailbox,readMailbox}=require('../editor-mailbox.cjs');
async function fixture(){const root=await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(),'airk-editor-mailbox-')));return {root,file:path.join(root,'reply.json')};}
async function cleanup(root){
    const temporary=await fs.realpath(os.tmpdir()),actual=await fs.realpath(root),info=await fs.lstat(root),relative=path.relative(temporary,actual);
    assert.equal(path.relative(root,actual),'');assert.equal(info.isSymbolicLink(),false);assert.equal(path.dirname(actual),temporary);
    assert.ok(relative&&!relative.startsWith('..')&&!path.isAbsolute(relative)&&/^airk-editor-mailbox-[A-Za-z0-9_-]+$/.test(path.basename(actual)));
    await fs.rm(actual,{recursive:true,force:true});
}

test('atomic repeated replacement exposes complete JSON without changing original values',async()=>{
    const {root,file}=await fixture();
    try{
        const value={sequence:0,native:{complete:true,items:Array.from({length:300},(_,i)=>({id:i,text:'fictional '.repeat(40)}))}},original=structuredClone(value);
        await publishMailbox(file,value);let stopped=false,reads=0;
        // Actual editor mailboxes poll at 100 ms. Keep a concurrent, faster
        // bounded poll without continuously starving Windows rename admission.
        const reader=(async()=>{while(!stopped){const observed=await readMailbox(file);assert.ok(observed&&Number.isInteger(observed.sequence));assert.deepEqual(observed.native,value.native);reads++;await new Promise(resolve=>setTimeout(resolve,5));}})();
        try{for(let sequence=1;sequence<=24;sequence++)await publishMailbox(file,{...value,sequence});}finally{stopped=true;await reader;}
        assert.ok(reads>0);assert.deepEqual(value,original);assert.equal((await readMailbox(file)).sequence,24);
        assert.deepEqual(await fs.readdir(root),['reply.json']);
    }finally{await cleanup(root);}
});

test('missing alone is undefined; malformed JSON and oversized messages remain failures',async()=>{
    const {root,file}=await fixture();
    try{
        assert.equal(await readMailbox(file),undefined);
        await fs.writeFile(file,'{"incomplete":');await assert.rejects(readMailbox(file),SyntaxError);
        await publishMailbox(file,{preserved:true});const original=await fs.readFile(file);
        await assert.rejects(publishMailbox(file,{large:'x'.repeat(100)},{maxBytes:32}),/byte budget/);
        assert.deepEqual(await fs.readFile(file),original);assert.deepEqual(await fs.readdir(root),['reply.json']);
        await assert.rejects(readMailbox(file,{maxBytes:2}),/byte budget/);
        await assert.rejects(readMailbox('relative.json'),/Absolute/);
    }finally{await cleanup(root);}
});

test('exclusive stage collision and rename failure preserve foreign files',async()=>{
    const {root,file}=await fixture(),uuid='12345678-1234-4234-8234-123456789abc',stage=path.join(root,`.reply.json.${uuid}.stage`),random=crypto.randomUUID;
    try{
        await publishMailbox(file,{original:true});await fs.writeFile(stage,'foreign collision',{flag:'wx'});
        crypto.randomUUID=()=>uuid;
        await assert.rejects(publishMailbox(file,{replacement:true}),error=>error.code==='EEXIST');
        assert.equal(await fs.readFile(stage,'utf8'),'foreign collision');assert.deepEqual(await readMailbox(file),{original:true});
        crypto.randomUUID=random;
        const directory=path.join(root,'destination');await fs.mkdir(directory);await fs.writeFile(path.join(directory,'preserve'),'foreign marker');
        await assert.rejects(publishMailbox(directory,{mustNotReplaceDirectory:true}));
        assert.equal(await fs.readFile(path.join(directory,'preserve'),'utf8'),'foreign marker');
        assert.deepEqual((await fs.readdir(root)).sort(),[path.basename(stage),'destination','reply.json'].sort());
    }finally{crypto.randomUUID=random;await cleanup(root);}
});

test('Windows transient rename refusals retain complete staged bytes; unrelated errors are never retried',{skip:process.platform!=='win32'},async()=>{
    const {root,file}=await fixture(),rename=fs.rename;
    try{
        await publishMailbox(file,{original:true});let calls=0;
        fs.rename=async(...args)=>{if(++calls<=2)throw Object.assign(new Error('temporary reader sharing refusal'),{code:'EPERM'});return rename(...args);};
        await publishMailbox(file,{complete:true});assert.equal(calls,3);assert.deepEqual(await readMailbox(file),{complete:true});
        const permanent=Object.assign(new Error('permanent rename refusal'),{code:'EINVAL'});calls=0;
        fs.rename=async()=>{calls++;throw permanent;};
        await assert.rejects(publishMailbox(file,{mustNotPublish:true}),error=>error===permanent);
        assert.equal(calls,1);assert.deepEqual(await readMailbox(file),{complete:true});assert.deepEqual(await fs.readdir(root),['reply.json']);
        const busy=Object.assign(new Error('persistent Windows sharing refusal'),{code:'EPERM'});calls=0;
        fs.rename=async()=>{calls++;throw busy;};
        await assert.rejects(publishMailbox(file,{mustNotPublish:true}),error=>error===busy);
        assert.equal(calls,7,'One initial attempt and exactly six bounded additional attempts');
        assert.deepEqual(await readMailbox(file),{complete:true});assert.deepEqual(await fs.readdir(root),['reply.json']);
    }finally{fs.rename=rename;await cleanup(root);}
});

test('stage cleanup failure is disclosed without replacing the original publication error',async()=>{
    const {root,file}=await fixture(),rename=fs.rename,unlink=fs.unlink;
    const primary=Object.assign(new Error('original publication refusal'),{code:'EINVAL'}),cleanupFailure=Object.assign(new Error('owned stage cleanup refused'),{code:'EACCES'});
    try{
        await publishMailbox(file,{original:true});
        fs.rename=async()=>{throw primary;};fs.unlink=async()=>{throw cleanupFailure;};
        await assert.rejects(publishMailbox(file,{unpublished:true}),error=>error===primary);
        assert.equal(primary.cause,cleanupFailure);assert.deepEqual(primary.cleanupErrors,[cleanupFailure]);
        assert.deepEqual(await readMailbox(file),{original:true});
        const retained=(await fs.readdir(root)).filter(name=>name.endsWith('.stage'));
        assert.equal(retained.length,1,'An unremoved owned stage remains explicit failed-cleanup evidence');
        assert.deepEqual(JSON.parse(await fs.readFile(path.join(root,retained[0]),'utf8')),{unpublished:true});
    }finally{fs.rename=rename;fs.unlink=unlink;await cleanup(root);}
});

test('read close failure preserves malformed JSON as primary and still rejects a successful read',async()=>{
    const {root,file}=await fixture(),open=fs.open,closeFailure=Object.assign(new Error('read handle close refused'),{code:'EACCES'});
    try{
        await fs.writeFile(file,'{"incomplete":');
        fs.open=async(...args)=>{const handle=await open(...args),close=handle.close.bind(handle);handle.close=async()=>{await close();throw closeFailure;};return handle;};
        await assert.rejects(readMailbox(file),error=>error instanceof SyntaxError&&error.cause===closeFailure&&error.cleanupErrors.length===1&&error.cleanupErrors[0]===closeFailure);
        await fs.writeFile(file,'{"complete":true}');
        await assert.rejects(readMailbox(file),error=>error===closeFailure);
    }finally{fs.open=open;await cleanup(root);}
});
