// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import Module from 'node:module';

test('service discovery ignores workspace executable, approval and lifecycle overrides',()=>{
    const loader=Module as unknown as {_load:(...args:any[])=>any},original=loader._load;
    const global:Record<string,unknown>={'service.channel':'preview','service.installDir':'/trusted/install',
        'service.managedSource':'/trusted/provider','service.managedSha256':'ab'.repeat(32),'service.managedLifecycle':'external'};
    const stub={workspace:{getConfiguration:()=>({inspect:(key:string)=>({globalValue:global[key],workspaceValue:'/untrusted/workspace/instruction',workspaceFolderValue:'user-on-demand'})})}};
    loader._load=function(request:string,...args:any[]){return request==='vscode'?stub:original.call(this,request,...args);};
    try{
        delete require.cache[require.resolve('../serviceConsole')];
        const {serviceRuntimeOptions}=require('../serviceConsole');
        const options=serviceRuntimeOptions();
        assert.equal(options.installDir,'/trusted/install');assert.equal(options.channel,'preview');
        assert.deepEqual(options.managed,{source:'/trusted/provider',approvedSha256:'ab'.repeat(32),lifecycle:'external'});
        for(const key of Object.keys(global))delete global[key];
        assert.deepEqual(serviceRuntimeOptions(),{channel:'stable'});
    }finally{loader._load=original;delete require.cache[require.resolve('../serviceConsole')];}
});
