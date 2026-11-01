// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';

test('Management Service lists exactly 4, 8, 16 default, 32 and 64 channels and preserves other policy',async()=>{
    const loader=require('node:module') as {_load:(...args:any[])=>any},original=loader._load;
    let cancel=false;
    const stub={window:{showQuickPick:async(items:any[])=>{
        assert.equal(items.length,5);
        assert.equal(items.find(item=>item.value===16).description,'Default · Current');
        assert.deepEqual(items.map(item=>item.value),[4,8,16,32,64]);
        const choice=items.find(item=>item.value===32);
        assert.equal(choice.label,'32 channels');
        return cancel?undefined:choice;
    }}};
    loader._load=function(request:string,...args:any[]){return request==='vscode'?stub:original.call(this,request,...args);};
    try{
        const {ServiceConsole}=require('../serviceConsole');
        const settings={preset:'responsive',query_concurrency:16,memory_bytes:2**31};
        const selected=await ServiceConsole.prototype.resources.call({},settings,true);
        assert.deepEqual(selected,{...settings,query_concurrency:32});
        assert.equal(settings.query_concurrency,16,'selection must not mutate the current observation');
        cancel=true;
        assert.equal(await ServiceConsole.prototype.resources.call({},settings,true),undefined);
    }finally{loader._load=original;}
});
