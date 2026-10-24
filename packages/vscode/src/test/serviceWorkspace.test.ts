// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import * as fs from 'node:fs/promises';
import * as os from 'node:os';
import * as path from 'node:path';
import { ServiceWorkspace } from '../serviceWorkspace';
import { ProjectRegistrationReview, RuntimeBinding } from '../serviceBootstrap';

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
        {reviewProject:async(_binding,root,data_root,collection)=>({...binding,binding:selected,root,data_root,collection}),registerProject:async(_binding,review)=>review},()=>true);
    try{
        await workspace.register(binding,source,path.join(root,'data-a'),'fixture',async()=>true);
        selected='ef'.repeat(32);await workspace.register(binding,source,path.join(root,'data-b'),'fixture',async()=>true);
        const current=await workspace.definitions(binding,[source]);assert.equal(current.length,1);assert.equal(current[0].service?.project,selected);
        assert.equal([...values.keys()].filter(key=>key.startsWith('airk.service.workspace.1.')).length,2);
    }finally{await fs.rm(root,{recursive:true,force:true});}
});
