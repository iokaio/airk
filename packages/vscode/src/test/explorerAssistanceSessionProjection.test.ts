// SPDX-License-Identifier: Apache-2.0
import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { AssistanceServer } from '../explorerAssistance/server';
import { HelpIndex } from '../explorerAssistance/help';
import type { AssistanceTools } from '../explorerAssistance/tools';

test('Assistance MCP preserves every present error detail from raw one-shot ToolResult serialization',async()=>{
    for(const detail of ['',null,false,0,undefined,'Unicode 雪 with "quotes"']){
        const raw={ok:false,code:'INVALID_SCHEMA',detail};
        let calls=0;
        const tools={call:async()=>{calls++;return raw;}} as unknown as AssistanceTools;
        const server=new AssistanceServer(new HelpIndex({catalogVersion:1,extensionVersion:'1.8.0',topics:[]}),tools,'1.8.0');
        await server.handle(JSON.stringify({jsonrpc:'2.0',id:1,method:'initialize',params:{clientInfo:{name:'Cli',version:'1.8.0'}}}));
        const response=await server.handle(JSON.stringify({jsonrpc:'2.0',id:2,method:'tools/call',params:{name:'airk_explorer_template_get',arguments:{}}})) as any;
        const oneShot=JSON.parse(JSON.stringify(raw));
        const structured=response.result.structuredContent;
        assert.equal(response.result.isError,true);
        assert.equal(Object.hasOwn(structured.error,'detail'),Object.hasOwn(oneShot,'detail'));
        assert.deepEqual(structured.error.detail,oneShot.detail);
        assert.equal(JSON.parse(response.result.content[0].text).error.detail,oneShot.detail);
        assert.equal(calls,1);
        // Native legacy projection uses only string detail, otherwise its fixed
        // fallback. Preserve empty string separately from missing/null/false/0.
        const project=(value:Record<string,unknown>)=>({code:value.code,message:typeof value.detail==='string'?value.detail:'legacy fallback'});
        assert.deepEqual(project(structured.error),project(oneShot));
    }
});
