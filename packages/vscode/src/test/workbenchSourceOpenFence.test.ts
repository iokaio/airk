// SPDX-License-Identifier: Apache-2.0
// Controlled filesystem-boundary test; no actual editor or native parity claim.
import { strict as assert } from 'node:assert';
import Module from 'node:module';
import { test } from 'node:test';

test('answer source Open rechecks host authority after asynchronous source path resolution before editor effects',async()=>{
    let release!:()=>void,entered!:()=>void;
    const held=new Promise<void>(resolve=>{release=resolve;}),observed=new Promise<void>(resolve=>{entered=resolve;});
    let authorized=true,opens=0,checks=0;
    const vscode={ViewColumn:{Beside:2,Active:1},Uri:{file:(fsPath:string)=>({fsPath})},
        workspace:{textDocuments:[],workspaceFolders:[]},window:{showTextDocument:async()=>{opens++;}},commands:{executeCommand:async()=>{opens++;}}};
    const loader=Module as unknown as {_load:(name:string,...args:any[])=>any},original=loader._load;
    loader._load=function(name:string,...args:any[]){
        if(name==='vscode')return vscode;
        if(name==='./explorerSource')return {...original.call(this,name,...args),sourceFile:async(root:string,relative:string)=>{assert.equal(root,'/native/root');assert.equal(relative,'saved.rs');entered();await held;return '/native/root/saved.rs';}};
        return original.call(this,name,...args);
    };
    let open:typeof import('../explorerWorkspaceIO').openSource;
    try{delete require.cache[require.resolve('../explorerWorkspaceIO')];open=require('../explorerWorkspaceIO').openSource;}finally{loader._load=original;}
    const pending=open({root:'/native/root',relative:'saved.rs',contentHash:'31'.repeat(32),startLine:9},1,()=>{checks++;if(!authorized)throw Error('original answer window changed');});
    await observed;authorized=false;release();
    await assert.rejects(pending,/original answer window changed/);
    assert.equal(checks,2);assert.equal(opens,0);
});
