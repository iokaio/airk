// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { renamedDefaultDataRoot, ServiceWorkspace } from '../serviceWorkspace';
import { ProjectRegistrationReview, RuntimeBinding } from '../serviceBootstrap';

test('saved historical defaults follow the renamed native root while custom and existing locations stay selected',async()=>{
    const root=await fs.mkdtemp(path.join(os.tmpdir(),'airk-default-storage-'));
    try{
        const old=path.join(root,'service-1.4'),current=path.join(root,'index-v1');
        await fs.mkdir(current);
        assert.equal(await renamedDefaultDataRoot(old,current),current);
        assert.equal(await renamedDefaultDataRoot(path.join(root,'custom','service-1.4'),current),undefined);
        await fs.mkdir(old);
        assert.equal(await renamedDefaultDataRoot(old,current),undefined,'an existing custom root is preserved');
        assert.equal(await renamedDefaultDataRoot(path.join(root,'service-1.4-preview'),path.join(root,'index-v1-preview')),path.join(root,'index-v1-preview'));
        assert.equal(await renamedDefaultDataRoot(old,path.join(root,'custom')),undefined);
    }finally{await fs.rm(root,{recursive:true,force:true});}
});

test('workspace definitions require explicit review, persist opaque bindings and retain unavailable sources',async()=>{
    const root=await fs.mkdtemp(path.join(os.tmpdir(),'airk-workspace-fixture-')),values=new Map<string,unknown>();
    const source=path.join(root,'source');await fs.mkdir(source);
    const binding:RuntimeBinding={installDir:root,controlDir:path.join(root,'control'),runtimeSha256:'ab'.repeat(32),version:'1.4.0',
        executable:path.join(root,'airk'),launcher:path.join(root,'airk-launcher'),lifecycle:'on-demand',verificationBasis:'trusted_distribution'};
    const review:ProjectRegistrationReview={installDir:root,controlDir:binding.controlDir,runtimeSha256:binding.runtimeSha256,
        root:await fs.realpath(source),data_root:path.join(root,'data'),binding:'cd'.repeat(32),collection:'fixture'};
    let authorized=true,submitted=0;
    const workspace=new ServiceWorkspace({keys:()=>[...values.keys()],get:<T>(key:string)=>values.get(key) as T|undefined,
        update:async(key,value)=>{values.set(key,value);}},
    {reviewProject:async()=>review,registerProject:async()=>{submitted++;return review;}},()=>authorized);
    try{
        assert.deepEqual(await workspace.definitions(binding,[source]),[]);
        await workspace.register(binding,source,review.data_root,'fixture',async()=>false);assert.equal(submitted,0);assert.equal(values.size,0);
        await workspace.register(binding,source,review.data_root,'fixture',async()=>true);assert.equal(submitted,1);
        const specs=await workspace.definitions(binding,[source]);assert.equal(specs.length,1);assert.equal(specs[0].service?.project,review.binding);
        assert.deepEqual(await workspace.definitions({...binding,installDir:path.join(root,'another')},[source]),[]);
        await fs.rename(source,path.join(root,'offline'));
        assert.equal((await workspace.definitions(binding,[source]))[0].service?.project,review.binding);
        await assert.rejects(workspace.register(binding,source,review.data_root,'fixture',async()=>{authorized=false;return true;}),/runtime_client_no_longer_authorized/);
        assert.equal(submitted,1);
    }finally{await fs.rm(root,{recursive:true,force:true});}
});

test('reviewing new storage changes only the active workspace binding and preserves the preceding registration',async()=>{
    const root=await fs.mkdtemp(path.join(os.tmpdir(),'airk-workspace-selection-')),values=new Map<string,unknown>();
    const source=path.join(root,'source');await fs.mkdir(source);
    const binding:RuntimeBinding={installDir:root,controlDir:path.join(root,'control'),executable:path.join(root,'main'),launcher:path.join(root,'airk-launcher'),
        runtimeSha256:'ab'.repeat(32),version:'1.4.0',lifecycle:'on-demand',verificationBasis:'trusted_distribution'};
    let selected='cd'.repeat(32);
    const workspace=new ServiceWorkspace({keys:()=>[...values.keys()],get:<T>(key:string)=>values.get(key) as T|undefined,update:async(key,value)=>{values.set(key,value);}},
        {reviewProject:async(_binding,root,data_root,collection)=>({...binding,binding:selected,root:await fs.realpath(root),data_root,collection}),registerProject:async(_binding,review)=>review},()=>true);
    try{
        await workspace.register(binding,source,path.join(root,'data-a'),'fixture',async()=>true);
        selected='ef'.repeat(32);await workspace.register(binding,source,path.join(root,'data-b'),'fixture',async()=>true);
        const current=await workspace.definitions(binding,[source]);assert.equal(current.length,1);assert.equal(current[0].service?.project,selected);
        assert.equal([...values.keys()].filter(key=>key.startsWith('airk.service.workspace.1.')).length,2);
    }finally{await fs.rm(root,{recursive:true,force:true});}
});

test('agent connections resolve stale editor registrations from complete native metadata without registering or indexing',async()=>{
    const root=await fs.realpath(await fs.mkdtemp(path.join(os.tmpdir(),'airk-workspace-agent-'))),values=new Map<string,unknown>();
    const source=path.join(root,'source');await fs.mkdir(source);
    const binding:RuntimeBinding={installDir:root,controlDir:path.join(root,'control'),executable:path.join(root,'main'),launcher:path.join(root,'airk-launcher'),
        runtimeSha256:'ab'.repeat(32),version:'1.5.11',lifecycle:'on-demand',verificationBasis:'trusted_distribution'};
    const old:ProjectRegistrationReview={...binding,binding:'cd'.repeat(32),root:source,data_root:path.join(root,'service-1.4'),collection:'fixture'};
    let registrations=0;
    const workspace=new ServiceWorkspace({keys:()=>[...values.keys()],get:<T>(key:string)=>values.get(key) as T|undefined,
        update:async(key,value)=>{values.set(key,value);}},
        {reviewProject:async()=>old,registerProject:async()=>{registrations++;return old;}},()=>true);
    const page=(projects:ProjectRegistrationReview[],after:string|null=null,revision=3)=>({revision,cached:true as const,observed_unix_ms:1,after,
        projects:projects.map(registration=>({project:{registration}}))});
    try{
        await workspace.register(binding,source,old.data_root,old.collection,async()=>true);
        const stale=(await workspace.definitions(binding,[source]))[0];
        const current={...old,binding:'ef'.repeat(32),data_root:path.join(root,'index-v1')};
        await assert.rejects(workspace.resolveConnection(binding,stale,async()=>page([])),/no longer registered/);
        await assert.rejects(workspace.resolveConnection(binding,stale,async()=>{throw new Error('offline');}),/offline/);
        await assert.rejects(workspace.resolveConnection(binding,stale,async()=>page([current,{...current,binding:'12'.repeat(32)}])),/Several native indexes/);
        await assert.rejects(workspace.resolveConnection(binding,stale,async()=>page([{...old,data_root:current.data_root}])),/workspace_registration_changed/);
        assert.equal((await workspace.definitions(binding,[source]))[0].service!.project,old.binding,'failed resolution cannot replace the selection');
        const unrelated={...old,root:path.join(root,'other'),binding:'34'.repeat(32)};
        const calls:unknown[]=[];
        const selected=await workspace.resolveConnection(binding,stale,async(after,revision)=>{
            calls.push([after,revision]);return after===null?page([unrelated],unrelated.binding):page([current]);
        });
        assert.deepEqual(calls,[[null,null],[unrelated.binding,3]]);
        assert.equal(selected.service!.project,current.binding);assert.equal(selected.dataDir,current.data_root);
        assert.equal((await workspace.definitions(binding,[source]))[0].service!.project,current.binding);
        assert.equal(registrations,1,'resolution never reserves or indexes a project');
        const exact=await workspace.resolveConnection(binding,selected,async()=>page([old,current]));
        assert.equal(exact.service!.project,current.binding,'existing selected binding wins over another retained index');
        await assert.rejects(workspace.resolveConnection(binding,selected,async(after)=>after===null?page([unrelated],unrelated.binding):page([current],null,4)),/registry_changed/);
        await assert.rejects(workspace.resolveConnection(binding,selected,async()=>page([unrelated],unrelated.binding)),/page_invalid/);
    }finally{
        assert.equal(path.dirname(await fs.realpath(root)),await fs.realpath(os.tmpdir()));
        assert.ok(path.basename(root).startsWith('airk-workspace-agent-'));
        await fs.rm(root,{recursive:true,force:true});
    }
});


test('native removal withdraws only the matching editor launch receipt and preserves another index',async()=>{
    const root=await fs.mkdtemp(path.join(os.tmpdir(),'airk-workspace-removal-')),source=path.join(root,'source');await fs.mkdir(source);
    const values=new Map<string,unknown>();
    const binding:RuntimeBinding={installDir:root,controlDir:path.join(root,'control'),runtimeSha256:'ab'.repeat(32),version:'1.6.2',
        executable:path.join(root,'airk'),launcher:path.join(root,'launcher'),lifecycle:'on-demand',verificationBasis:'trusted_distribution'};
    const a='cd'.repeat(32),b='ef'.repeat(32);
    const workspace=new ServiceWorkspace({keys:()=>[...values.keys()],get:<T>(key:string)=>values.get(key) as T|undefined,
        update:async(key,value)=>{if(value===undefined)values.delete(key);else values.set(key,value);}},
        {reviewProject:async(_runtime,opened,data,collection)=>({installDir:root,controlDir:binding.controlDir,runtimeSha256:binding.runtimeSha256,
            root:await fs.realpath(opened),data_root:data,collection,binding:collection==='a'?a:b}),registerProject:async(_runtime,review)=>review},()=>true);
    try{
        await workspace.register(binding,source,path.join(root,'data'),'a',async()=>true);
        await workspace.register(binding,source,path.join(root,'data'),'b',async()=>true);
        await workspace.withdrawProject(binding,a);
        assert.equal((await workspace.definitions(binding,[source]))[0].service!.project,b);
        assert.equal([...values.keys()].filter(key=>key.startsWith('airk.service.workspace.1.')).length,1);
        await workspace.withdrawProject(binding,b);
        assert.deepEqual(await workspace.definitions(binding,[source]),[]);
        assert.equal(values.size,0);
    }finally{await fs.rm(root,{recursive:true,force:true});}
});
