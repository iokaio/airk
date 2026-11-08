// SPDX-License-Identifier: Apache-2.0
import {test} from 'node:test';
import {strict as assert} from 'node:assert';
import * as https from 'node:https';
import {McpHttpSession} from '../mcpHttpSession';
import {ca,cert,key} from './fixtures/httpTlsFixture';
const credential='airk1.'+'a'.repeat(32)+'.'+'b'.repeat(64);
test('actual HTTPS MCP carries bounded JSON/SSE sessions and cancellation without a child process',async()=>{
    const calls:{method:string;id?:number;auth:boolean;mode:unknown}[]=[];let notificationClosed=false,deleteClosed=false;
    const endless=(response:import('node:http').ServerResponse,closed:()=>void)=>{response.writeHead(202);response.write('ignored notification response');const timer=setInterval(()=>response.write('x'.repeat(8192)),5);response.once('close',()=>{clearInterval(timer);closed();});};
    const server=https.createServer({cert,key},(request,response)=>{
        if(request.method==='DELETE'){endless(response,()=>{deleteClosed=true;});return;}
        let input='';request.on('data',bytes=>input+=bytes);request.on('end',()=>{
            const value=JSON.parse(input);calls.push({method:value.method,id:value.id,auth:request.headers.authorization==='Bearer '+credential,mode:request.headers['x-airk-response-mode']});
            if(value.id===undefined){if(value.method==='notifications/initialized'){endless(response,()=>{notificationClosed=true;});return;}response.writeHead(202);response.end();return;}
            const result=value.method==='initialize'?{protocolVersion:'2025-11-25',capabilities:{},serverInfo:{name:'fictional',version:'0'}}:{tools:[],nativeUnknown:{preserved:true}};
            if(value.method==='redirect'){response.writeHead(307,{location:'https://127.0.0.1:1/mcp'});response.end();return;}
            if(value.method==='oversize'){response.writeHead(200,{'content-type':'application/json'});response.end('x'.repeat(1048577));return;}
            if(value.method==='slow'){setTimeout(()=>{if(!response.destroyed){response.writeHead(200,{'content-type':'application/json'});response.end(JSON.stringify({jsonrpc:'2.0',id:value.id,result}));}},100);return;}
            if(value.method==='sse'){response.writeHead(200,{'content-type':'text/event-stream','mcp-session-id':'fixture-session'});response.end('event: message\ndata: '+JSON.stringify({jsonrpc:'2.0',id:value.id,result})+'\n\n');return;}
            response.writeHead(200,{'content-type':'application/json','mcp-session-id':'fixture-session'});response.end(JSON.stringify({jsonrpc:'2.0',id:value.id,result}));
        });
    });
    await new Promise<void>(resolve=>server.listen(0,'127.0.0.1',resolve));let session:McpHttpSession|undefined;
    try{
        const url='https://127.0.0.1:'+(server.address() as {port:number}).port+'/mcp';
        await assert.rejects(McpHttpSession.open({url,ca:Buffer.alloc(0),credential,clientName:'fictional',clientVersion:'0'}),/CERT|ISSUER|UNABLE_TO_VERIFY_LEAF_SIGNATURE/);assert.equal(calls.length,0,'untrusted TLS cannot send an HTTP authorization header');
        session=(await McpHttpSession.open({url,ca:Buffer.from(ca),credential,clientName:'fictional',clientVersion:'0'})).session;
        assert.equal(session.diagnosticSnapshot().pid,null);assert.deepEqual(await session.request('sse',{}),{tools:[],nativeUnknown:{preserved:true}});
        await assert.rejects(session.request('redirect',{}),/status 307/);await assert.rejects(session.request('oversize',{}),/byte bound/);await assert.rejects(session.request('slow',{},10),/deadline/);
        assert.deepEqual(await session.request('tools/list',{}),{tools:[],nativeUnknown:{preserved:true}});assert.ok(calls.every(row=>row.auth&&row.mode==='full'));for(let attempt=0;attempt<20&&!calls.some(row=>row.method==='notifications/cancelled');attempt++)await new Promise(resolve=>setTimeout(resolve,5));assert.ok(calls.some(row=>row.method==='notifications/cancelled')); assert.ok(!JSON.stringify(session.diagnosticSnapshot()).includes(credential));
        session.close();for(let attempt=0;attempt<40&&(!notificationClosed||!deleteClosed);attempt++)await new Promise(resolve=>setTimeout(resolve,5));assert.equal(notificationClosed,true,'accepted notification cannot leave an unbounded response stream');assert.equal(deleteClosed,true,'session close does not retain a streaming DELETE response');
    }finally{session?.close();server.closeAllConnections();await new Promise<void>(resolve=>server.close(()=>resolve()));}
});
