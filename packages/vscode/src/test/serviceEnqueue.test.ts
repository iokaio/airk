// SPDX-License-Identifier: Apache-2.0
import {strict as assert} from 'node:assert';
import {test} from 'node:test';
import * as path from 'node:path';
import * as fs from 'node:fs/promises';
import {serviceTestHost} from './serviceTestHost';
import {ServiceClient,ServiceError} from '../serviceClient';
import {INTENT_PREFIX} from '../serviceIntents';

for(const phase of ['prepare_key','prepare_record','native_reply'] as const)for(const changed of ['generation','binding','selection'] as const)test(`index enqueue preserves captured ${changed} ownership across ${phase}`,async context=>{
    const f=await serviceTestHost(context),controller=f.context.subscriptions.find((item:any)=>typeof item.resolveAgentSpec==='function');
    let invalidated=false,reply:Record<string,unknown>|undefined,initializations=0,creates=0;
    const invalidate=()=>{if(invalidated)return;invalidated=true;if(changed==='generation')controller.generation++;else if(changed==='binding')controller.binding={...controller.binding,controlDir:path.join(f.root,'different-control')};else context.mock.method(controller,'isCurrent',()=>false);};
    try{
        if(phase==='prepare_key'){
            const initialize=f.intentStorage.initialize!.bind(f.intentStorage),create=f.intentStorage.create!.bind(f.intentStorage);
            context.mock.method(f.intentStorage,'initialize',async(scope:string)=>{initializations++;await initialize(scope);});
            context.mock.method(f.intentStorage,'create',async(key:string,value:unknown)=>{creates++;await create(key,value);});
            const prepare=ServiceClient.prototype.prepareJob;context.mock.method(ServiceClient.prototype,'prepareJob',async function(this:ServiceClient,...args:Parameters<ServiceClient['prepareJob']>){const intent=await prepare.apply(this,args);invalidate();return intent;});
        }else if(phase==='prepare_record'){
            const create=f.intentStorage.create!.bind(f.intentStorage);context.mock.method(f.intentStorage,'create',async(key:string,value:unknown)=>{await create(key,value);invalidate();});
        }else{
            const submit=ServiceClient.prototype.submitMutation;context.mock.method(ServiceClient.prototype,'submitMutation',async function(this:ServiceClient,...args:Parameters<ServiceClient['submitMutation']>){const response=await submit.apply(this,args);reply=structuredClone(response);invalidate();return response;});
        }
        await assert.rejects(f.commands.get('airk.index')!(),(error:unknown)=>{
            assert.ok(error instanceof ServiceError);assert.equal(error.code,'workspace_registration_changed');
            if(phase==='native_reply'){assert.deepEqual(error.nativeResponse,reply);assert.equal(error.outcomeUnknown,true);}return true;
        });
        assert.equal(invalidated,true);assert.equal(f.writes.filter(value=>value.enqueue).length,phase==='native_reply'?1:0,'owner loss never dispatches or replays stale work');
        if(phase==='prepare_key'){
            assert.equal(initializations,0,'stale key preparation must not initialize private journal storage');assert.equal(creates,0,'stale key preparation must not persist an intent');
            await assert.rejects(fs.lstat(path.join(f.context.globalStorageUri.fsPath,'service-intents')),{code:'ENOENT'});
            assert.equal([...f.values.keys()].some(key=>key.startsWith(INTENT_PREFIX)),false,'stale preparation cannot fall back to Memento writes');
        }else{
            const pending=f.intentStorage.keys().filter((key:string)=>key.startsWith(INTENT_PREFIX));assert.equal(pending.length,phase==='prepare_record'?1:0);
        }
    }finally{await f.close();}
});
