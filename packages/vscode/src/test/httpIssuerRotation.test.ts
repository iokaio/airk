// SPDX-License-Identifier: Apache-2.0
import {test} from 'node:test';
import {strict as assert} from 'node:assert';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import {rootCertificates} from 'node:tls';
import {createHash,X509Certificate} from 'node:crypto';
import {ClientTrustPublicationError,IssuerRotationRecord,IssuerRotationStatus,publishOwnedIssuerBundle,updateOwnedIssuerBundles,validateIssuerRotation} from '../httpIssuerRotation';
import {RuntimeHost} from '../serviceBootstrap';
import {ServiceError} from '../serviceClient';
import {ca} from './fixtures/httpTlsFixture';
const sha=(input:Buffer|string)=>createHash('sha256').update(input).digest('hex');
const certSha=(pem:string)=>sha(new X509Certificate(pem).raw);
const nextCa=rootCertificates[0]+'\n',unrelatedCa=rootCertificates[1]+'\n';
async function fixture(){
    const root=await fs.mkdtemp(path.join(os.tmpdir(),'airk-issuer-')),control=path.join(root,'control'),directory=path.join(root,'storage','http-trust'),native=path.join(control,'http-1');await fs.mkdir(directory,{recursive:true});await fs.mkdir(native,{recursive:true});
    const old=path.join(native,'old-ca.pem'),next=path.join(native,'new-ca.pem');await fs.writeFile(old,ca);await fs.writeFile(next,nextCa);
    const endpoint={schema:1 as const,installation:'11'.repeat(32),user:'fictional-user',namespace:'22'.repeat(16),generation:1,service_instance:'33'.repeat(16),executable_sha256:'44'.repeat(32),url:'https://127.0.0.1:57421/mcp',ca_sha256:certSha(ca),certificate_sha256:'55'.repeat(32),state:'ready' as const,predecessor_digest:null};
    const record:IssuerRotationRecord={schema:1,operation_id:'66'.repeat(16),phase:'prepared',revision:1,installation:endpoint.installation,user:endpoint.user,namespace:endpoint.namespace,old_ca_sha256:certSha(ca),new_ca_sha256:certSha(nextCa),old_ca_certificate_file:old,new_ca_certificate_file:next,old_endpoint:endpoint,new_endpoint:null,overlap_seconds:86400,overlap_expires_unix_ms:null,future_native:{retained:true}};
    const status:IssuerRotationStatus={schema:1,current_endpoint:endpoint,current_descriptor_sha256:'77'.repeat(32),rotation:record,receipt_sha256:'88'.repeat(32),finalize_overdue:false,guidance:'fictional native receipt',future_envelope:{retained:true}};
    const bytes=Buffer.from('# existing enterprise roots stay unchanged\n'+unrelatedCa+'# reviewed AIRK issuer\n'+ca),file=path.join(directory,sha(bytes)+'.pem');await fs.writeFile(file,bytes);
    const values=new Map([['CODEX_CA_CERTIFICATE',{type:1,value:file}],['NODE_EXTRA_CA_CERTS',{type:1,value:file}]]),writes:string[]=[];
    const collection={persistent:true,get:(key:string)=>values.get(key),replace:(key:string,value:string)=>{writes.push(key);values.set(key,{type:1,value});}};
    return {root,control,directory,record,status,file,bytes,values,writes,collection,storage:path.dirname(directory),async close(){await fs.rm(root,{recursive:true,force:true});}};
}

test('exclusive publication keeps the original handle live until the awaited creator observation finishes',async context=>{
    const f=await fixture(),promises=require('node:fs/promises'),open=promises.open;
    let original:any,closed=false,observed:Promise<void>|undefined;
    context.mock.method(promises,'open',async(filename:string,...args:unknown[])=>{
        const handle=await open(filename,...args);
        if(path.basename(filename)==='bundle.pem'&&args[0]==='wx'){
            original=handle;const close=handle.close.bind(handle);
            context.mock.method(handle,'close',async()=>{closed=true;return close();});
        }
        return handle;
    });
    try{
        await publishOwnedIssuerBundle(f.directory,Buffer.from(nextCa),()=>{},identity=>{
            observed=(async()=>{
                assert.equal(closed,false,'the actual original handle must still prevent creator inode reuse');
                const before=await original.stat({bigint:true});assert.equal(identity.dev,before.dev.toString());assert.equal(identity.ino,before.ino.toString());
                await new Promise<void>(resolve=>setImmediate(resolve));
                assert.equal(closed,false,'the publisher must await asynchronous caller ownership validation');
                const after=await original.stat({bigint:true});assert.equal(after.ino.toString(),identity.ino);
            })();
            // Observe a rejection even against the old non-awaited API, then
            // explicitly assert it after publication for one deterministic test.
            void observed.catch(()=>{});return observed;
        });
        assert.ok(observed);await observed;assert.equal(closed,true,'the original handle closes after the observer settles');
    }finally{await f.close();}
});

test('rotation receipts retain complete native data and reject crossed owners, generations and transactions',async()=>{
    const f=await fixture();try{
        assert.equal(validateIssuerRotation(f.status,f.control),f.status);assert.deepEqual(f.status.rotation!.future_native,{retained:true});
        const activated={...f.status,rotation:{...f.record,phase:'activated' as const,revision:2,new_endpoint:{...f.record.old_endpoint,generation:2,service_instance:'99'.repeat(16),ca_sha256:f.record.new_ca_sha256},overlap_expires_unix_ms:1234}};
        assert.equal(validateIssuerRotation(activated,f.control,f.status),activated);
        for(const mutate of [(v:any)=>v.rotation.namespace='99'.repeat(16),(v:any)=>v.rotation.user='other-user',(v:any)=>v.rotation.old_ca_sha256='99'.repeat(32),(v:any)=>v.rotation.new_endpoint.generation=1,(v:any)=>v.rotation.operation_id='99'.repeat(16),(v:any)=>v.rotation.overlap_seconds=59,(v:any)=>v.rotation.old_ca_certificate_file=path.join(f.root,'outside.pem'),(v:any)=>v.rotation.overlap_expires_unix_ms=null,(v:any)=>v.receipt_sha256='bad']){
            const changed=structuredClone(activated);mutate(changed);assert.throws(()=>validateIssuerRotation(changed,f.control,f.status));
        }
        const empty={...f.status,rotation:null,receipt_sha256:null};assert.equal(validateIssuerRotation(empty,f.control),empty);assert.throws(()=>validateIssuerRotation({...empty,receipt_sha256:'aa'.repeat(32)},f.control));
        assert.throws(()=>validateIssuerRotation(empty,f.control,f.status),/crossed_http_rotation_transaction/,'a retained native transaction cannot disappear from a reviewed response');
    }finally{await f.close();}
});

test('owned bundle overlap and finalization retain unrelated roots, source preimages and ephemeral credentials',async()=>{
    const f=await fixture();try{
        const overlap=await updateOwnedIssuerBundles(f.storage,f.collection,f.record,'overlap',1,()=>{});assert.equal(overlap.length,2);assert.equal(f.collection.persistent,false);
        for(const update of overlap){const bytes=await fs.readFile(update.after);assert.ok(bytes.includes(Buffer.from(ca)));assert.ok(bytes.includes(Buffer.from(nextCa)));assert.ok(bytes.includes(Buffer.from(unrelatedCa)));assert.equal(sha(bytes),update.after_sha256);}
        assert.deepEqual(await fs.readFile(f.file),f.bytes,'immutable predecessor file is not edited');
        const final={...f.record,phase:'finalized' as const};const removed=await updateOwnedIssuerBundles(f.storage,f.collection,final,'finalize',1,()=>{});
        for(const update of removed){const bytes=await fs.readFile(update.after);assert.ok(!bytes.includes(Buffer.from(ca)));assert.ok(bytes.includes(Buffer.from(nextCa)));assert.ok(bytes.includes(Buffer.from(unrelatedCa)));assert.ok(bytes.includes(Buffer.from('# existing enterprise roots stay unchanged')));}
        const again=await updateOwnedIssuerBundles(f.storage,f.collection,final,'finalize',1,()=>{});assert.ok(again.every(update=>!update.changed));
        assert.deepEqual(await fs.readFile(f.file),f.bytes);assert.equal(f.values.size,2,'no config/server/token entries added or widened');
    }finally{await f.close();}
});

test('bundle publication refuses a changed preimage or unowned reference without publishing another variable',async()=>{
    const f=await fixture();try{
        let reads=0;f.collection.get=(key:string)=>{if(++reads===3)f.values.set('CODEX_CA_CERTIFICATE',{type:1,value:path.join(f.root,'changed-by-user.pem')});return f.values.get(key);};
        await assert.rejects(updateOwnedIssuerBundles(f.storage,f.collection,f.record,'overlap',1,()=>{}),/configuration_changed/);assert.deepEqual(f.writes,[]);
        f.collection.get=key=>f.values.get(key);await assert.rejects(updateOwnedIssuerBundles(f.storage,f.collection,f.record,'overlap',1,()=>{}),/requires_review/);assert.deepEqual(f.writes,[]);
    }finally{await f.close();}
});

test('rotation certificate mismatch, private keys, hard links and stale caller refuse trust publication',async()=>{
    for(const scenario of ['mismatch','private','hardlink','stale','early_finalize'] as const){
        const f=await fixture();try{
            if(scenario==='mismatch')f.record.new_ca_sha256='aa'.repeat(32);
            if(scenario==='private')await fs.writeFile(f.record.new_ca_certificate_file,nextCa+'\n-----BEGIN PRIVATE KEY-----\nfictional\n-----END PRIVATE KEY-----');
            if(scenario==='hardlink')await fs.link(f.file,path.join(f.root,'second-link.pem'));
            const current=()=>{if(scenario==='stale')throw Error('stale_generation');};
            await assert.rejects(updateOwnedIssuerBundles(f.storage,f.collection,f.record,scenario==='early_finalize'?'finalize':'overlap',1,current));assert.deepEqual(f.writes,[],scenario);
        }finally{await f.close();}
    }
});

test('partial environment publication remains explicit and resumable without reverting foreign state',async()=>{
    const f=await fixture();try{
        const replace=f.collection.replace;f.collection.replace=(variable,value)=>{if(variable==='NODE_EXTRA_CA_CERTS')throw Error('closed terminal environment');replace(variable,value);};
        await assert.rejects(updateOwnedIssuerBundles(f.storage,f.collection,f.record,'overlap',1,()=>{}),error=>{assert.ok(error instanceof ClientTrustPublicationError);assert.equal(error.updates.length,2);assert.deepEqual(error.published,['CODEX_CA_CERTIFICATE']);return true;});
        assert.notEqual(f.values.get('CODEX_CA_CERTIFICATE')!.value,f.file);assert.equal(f.values.get('NODE_EXTRA_CA_CERTS')!.value,f.file);
        f.collection.replace=replace;const resumed=await updateOwnedIssuerBundles(f.storage,f.collection,f.record,'overlap',1,()=>{});assert.equal(resumed[0].changed,false);assert.equal(resumed[1].changed,true);
    }finally{await f.close();}
});

test('interrupted staged bundle writes never create a partial final filename and remain retryable',async t=>{
    const f=await fixture();try{
        const filesystem:typeof fs=require('node:fs/promises'),open=filesystem.open;
        let interrupted=false;
        const mock=t.mock.method(filesystem,'open',async(...args:Parameters<typeof fs.open>)=>{
            const handle=await open(...args);
            if(!interrupted&&path.basename(String(args[0]))==='bundle.pem'&&args[1]==='wx'){
                interrupted=true;const write=handle.writeFile.bind(handle);
                t.mock.method(handle,'writeFile',async()=>{await write(Buffer.from('partial'));throw Object.assign(Error('fictional disk full'),{code:'ENOSPC'});});
            }
            return handle;
        });
        await assert.rejects(updateOwnedIssuerBundles(f.storage,f.collection,f.record,'overlap',1,()=>{}),/fictional disk full/);
        assert.deepEqual(f.writes,[]);assert.deepEqual(await fs.readdir(f.directory),[path.basename(f.file)]);
        mock.mock.restore();await updateOwnedIssuerBundles(f.storage,f.collection,f.record,'overlap',1,()=>{});
        assert.equal(f.writes.length,2);
    }finally{await f.close();}
});

test('post-link interruption recovers only the receipt-bound complete temporary hardlink',async t=>{
    const f=await fixture();try{
        const filesystem:typeof fs=require('node:fs/promises'),unlink=filesystem.unlink;
        let interrupted=false;
        const mock=t.mock.method(filesystem,'unlink',async(file:Parameters<typeof fs.unlink>[0])=>{
            if(!interrupted&&path.basename(String(file))==='bundle.pem'){interrupted=true;throw Error('fictional shutdown after publication');}
            return unlink(file);
        });
        await assert.rejects(updateOwnedIssuerBundles(f.storage,f.collection,f.record,'overlap',1,()=>{}),/fictional shutdown/);
        assert.deepEqual(f.writes,[]);
        const stages=(await fs.readdir(f.directory)).filter(name=>name.startsWith('.issuer-bundle-'));assert.equal(stages.length,1);
        const complete=path.join(f.directory,stages[0],'bundle.pem');assert.equal((await fs.lstat(complete)).nlink,2);
        mock.mock.restore();await updateOwnedIssuerBundles(f.storage,f.collection,f.record,'overlap',1,()=>{});
        assert.ok(!(await fs.readdir(f.directory)).some(name=>name.startsWith('.issuer-bundle-')));
        for(const entry of f.values.values()){assert.equal((await fs.lstat(entry.value)).nlink,1);assert.ok((await fs.readFile(entry.value)).includes(Buffer.from(nextCa)));}
    }finally{await f.close();}
});

test('foreign collided final files and unreceipted hardlinks are preserved and refuse publication',async()=>{
    for(const linked of [false,true]){
        const f=await fixture();try{
            const output=Buffer.concat([f.bytes,Buffer.from('\n'),Buffer.from(nextCa)]),final=path.join(f.directory,sha(output)+'.pem');
            const bytes=linked?output:Buffer.from('foreign partial final');await fs.writeFile(final,bytes);
            const foreign=path.join(f.root,'foreign-link.pem');if(linked)await fs.link(final,foreign);
            await assert.rejects(updateOwnedIssuerBundles(f.storage,f.collection,f.record,'overlap',1,()=>{}));
            assert.deepEqual(await fs.readFile(final),bytes);if(linked)assert.deepEqual(await fs.readFile(foreign),bytes);
            assert.deepEqual(f.writes,[]);assert.ok(!(await fs.readdir(f.directory)).some(name=>name.startsWith('.issuer-bundle-')));
        }finally{await f.close();}
    }
});

test('changed recovery receipts or linked bodies refuse cleanup and preserve all evidence',async t=>{
    for(const scenario of ['receipt','body'] as const){
        const f=await fixture();try{
            const filesystem:typeof fs=require('node:fs/promises'),unlink=filesystem.unlink;
            const mock=t.mock.method(filesystem,'unlink',async(file:Parameters<typeof fs.unlink>[0])=>{
                if(path.basename(String(file))==='bundle.pem')throw Error('fictional retained publication');return unlink(file);
            });
            await assert.rejects(updateOwnedIssuerBundles(f.storage,f.collection,f.record,'overlap',1,()=>{}),/fictional retained publication/);mock.mock.restore();
            const stage=path.join(f.directory,(await fs.readdir(f.directory)).find(name=>name.startsWith('.issuer-bundle-'))!);
            const temporary=path.join(stage,'bundle.pem'),marker=path.join(stage,'owner.json');
            if(scenario==='receipt'){const receipt=JSON.parse(await fs.readFile(marker,'utf8'));receipt.sha256='00'.repeat(32);await fs.writeFile(marker,JSON.stringify(receipt));}
            else{const body=await fs.readFile(temporary);body[0]^=1;await fs.writeFile(temporary,body);}
            const beforeBody=await fs.readFile(temporary),beforeReceipt=await fs.readFile(marker);
            await assert.rejects(updateOwnedIssuerBundles(f.storage,f.collection,f.record,'overlap',1,()=>{}));
            assert.deepEqual(await fs.readFile(temporary),beforeBody);assert.deepEqual(await fs.readFile(marker),beforeReceipt);assert.equal((await fs.lstat(temporary)).nlink,2);assert.deepEqual(f.writes,[]);
        }finally{await f.close();}
    }
});

test('aborting a prepared transaction retires only its staged issuer and leaves old trust intact',async()=>{
    const f=await fixture();try{
        await updateOwnedIssuerBundles(f.storage,f.collection,f.record,'overlap',1,()=>{});
        const aborted={...f.record,phase:'aborted' as const};const updates=await updateOwnedIssuerBundles(f.storage,f.collection,aborted,'abort',1,()=>{});
        for(const update of updates){const bytes=await fs.readFile(update.after);assert.ok(bytes.includes(Buffer.from(ca)));assert.ok(bytes.includes(Buffer.from(unrelatedCa)));assert.ok(!bytes.includes(Buffer.from(nextCa)));}
        await assert.rejects(updateOwnedIssuerBundles(f.storage,f.collection,aborted,'overlap',1,()=>{}),/phase_requires_review/);
        const again=await updateOwnedIssuerBundles(f.storage,f.collection,aborted,'abort',1,()=>{});assert.ok(again.every(value=>!value.changed));
    }finally{await f.close();}
});

test('native rotation commands pin exact descriptor/receipt and never issue a mutation without reviewed identity',async()=>{
    const f=await fixture();try{
        const calls:string[][]=[];let reply=f.status;
        const host=new RuntimeHost(f.root,'1.8.14',{channel:'stable'},async(_command,args)=>{calls.push(args);return reply;});(host as any).verified=async()=>({main:path.join(f.root,'sealed-main')});
        const binding={installDir:f.root,controlDir:f.control,executable:path.join(f.root,'sealed-main'),launcher:null,runtimeSha256:'aa'.repeat(32),version:'1.8.14',lifecycle:'on-demand' as const,verificationBasis:'trusted_distribution'};
        const status=await host.httpIssuerRotation(binding,'status');assert.equal(status,f.status);
        await assert.rejects(host.httpIssuerRotation(binding,'commit'),/review_required/);assert.equal(calls.length,1);
        await host.httpIssuerRotation(binding,'prepare',f.status,f.record.operation_id);assert.ok(calls.at(-1)!.includes('--expected-descriptor-sha256'));assert.ok(calls.at(-1)!.includes(f.status.current_descriptor_sha256!));
        await host.httpIssuerRotation(binding,'trust-new',f.status);assert.deepEqual(calls.at(-1)!.slice(-4),['--operation-id',f.record.operation_id,'--expected-receipt-sha256',f.status.receipt_sha256]);
        reply={...f.status,rotation:{...f.record,operation_id:'bb'.repeat(16)}};await assert.rejects(host.httpIssuerRotation(binding,'commit',f.status),error=>{assert.ok(error instanceof ServiceError);assert.match(error.code,/crossed_http_rotation/);assert.equal(error.nativeResponse,reply);assert.equal(error.outcomeUnknown,true);return true;});
        const cleared:IssuerRotationStatus={...f.status,rotation:null,receipt_sha256:null};reply=cleared;
        for(const action of ['trust-new','commit','finalize','abort'] as const){
            const before:number=calls.length;
            await assert.rejects(host.httpIssuerRotation(binding,action,f.status),error=>{assert.ok(error instanceof ServiceError);assert.equal(error.code,'crossed_http_rotation_transaction');assert.equal(error.nativeResponse,cleared,'retain the complete actual native response');assert.equal(error.outcomeUnknown,true,'an executed mutation with a missing transaction has an unknown outcome');return true;});
            assert.equal(calls.length,before+1,'each explicit mutation is dispatched once without retry');
        }
    }finally{await f.close();}
});
