// SPDX-License-Identifier: Apache-2.0
// Fictional TLS transport regressions; actual native qualification is separate.
import {test} from 'node:test';
import {strict as assert} from 'node:assert';
import * as https from 'node:https';
import type {Duplex} from 'node:stream';
import {ca,cert,key} from './fixtures/httpTlsFixture';
import {InstalledHttpTransport,InstalledHttpFailure,installedHttpIdentity,installedHttpReply} from './installedEditorHttp';
const authorization='Bearer airk1.'+'a'.repeat(32)+'.'+'b'.repeat(64);
function definition(port:number,extra={}){return {uri:{toString:()=>`https://127.0.0.1:${port}/mcp`},headers:{Authorization:authorization,'X-AIRK-Response-Mode':'full'},...extra};}
test('actual HTTP definition binds credential and cannot replace framing or introduce a process',()=>{
    assert.equal(installedHttpIdentity(definition(12345)).credentialId,'a'.repeat(32));
    for(const extra of [{command:'airk'},{args:[]},{env:{}},{headers:{Authorization:authorization,'X-AIRK-Response-Mode':'full',Host:'foreign'}},{headers:{Authorization:authorization,'X-AIRK-Response-Mode':'full',authorization}},{headers:{Authorization:authorization+'\r\nHost: foreign','X-AIRK-Response-Mode':'full'}}])assert.throws(()=>installedHttpIdentity(definition(12345,extra)));
    assert.throws(()=>installedHttpIdentity({...definition(12345),uri:{toString:()=> 'http://127.0.0.1:12345/mcp'}}));
});
test('complete JSON/SSE retains notifications and future fields, rejects crossed IDs and truncation',()=>{
    const reply={jsonrpc:'2.0',id:7,result:{full:{future:[1,'�']}}},notice={jsonrpc:'2.0',method:'notifications/progress',params:{unknown:true}};
    assert.deepEqual(installedHttpReply(Buffer.from(JSON.stringify(reply)),'application/json',7),{messages:[reply],reply});
    const bytes=Buffer.from('data: '+JSON.stringify(notice)+'\r\n\r\ndata: '+JSON.stringify(reply)+'\r\n\r\n');
    assert.deepEqual(installedHttpReply(bytes,'text/event-stream',7),{messages:[notice,reply],reply});
    const partial=Buffer.concat([bytes,Buffer.from('data: {"unicode":"'),Buffer.from([0xc3])]);
    assert.deepEqual(installedHttpReply(partial,'text/event-stream',7,false),{messages:[notice,reply],reply});
    assert.throws(()=>installedHttpReply(bytes,'text/event-stream',8));
    assert.throws(()=>installedHttpReply(bytes.subarray(0,bytes.length-2),'text/event-stream',7));
    assert.throws(()=>installedHttpReply(Buffer.from(JSON.stringify({...reply,error:{code:1}})),'application/json',7));
});
test('actual CA-validated HTTPS keeps raw responses and session, closes own sockets, never replays reset',async()=>{
    const sockets=new Set<Duplex>(),seen:any[]=[];
    const server=https.createServer({cert,key},async(request,response)=>{
        const chunks:Buffer[]=[];for await(const chunk of request)chunks.push(Buffer.from(chunk));
        const body=Buffer.concat(chunks).length?JSON.parse(Buffer.concat(chunks).toString('utf8')):undefined;
        seen.push({method:request.method,headers:request.headers,body,socket:request.socket});
        if(request.method==='DELETE'){response.writeHead(204);response.end();return;}
        if(body.method==='reset'){request.socket.destroy();return;}
        if(body.method==='refuse'){response.writeHead(403,{'Content-Type':'application/json'});response.end(JSON.stringify({schema:'airk.local-api-error/1',error:{code:'native_grant_refused'},future:{retained:true}}));return;}
        if(body.method==='notifications/initialized'){response.writeHead(202);response.end();return;}
        response.writeHead(200,{'Content-Type':'application/json','Mcp-Session-Id':'actual-fixture-session'});
        response.end(JSON.stringify({jsonrpc:'2.0',id:body.id,result:body.method==='initialize'?{protocolVersion:'2025-11-25',future:{retained:true}}:{complete:{unknown:[1,2,3]}}}));
    });
    server.on('connection',socket=>{sockets.add(socket);socket.once('close',()=>sockets.delete(socket));});
    await new Promise<void>(resolve=>server.listen(0,'127.0.0.1',resolve));
    const address=server.address();assert.ok(address&&typeof address==='object');
    const client=new InstalledHttpTransport(definition(address.port),Buffer.from(ca));
    const untrusted=new InstalledHttpTransport(definition(address.port),Buffer.from(''));
    try{
        await assert.rejects(untrusted.request('initialize',{},3000),error=>error instanceof InstalledHttpFailure&&error.kind==='network'&&error.detail.response_bytes===0);
        await untrusted.close();
        assert.equal(seen.length,0,'Untrusted issuer never reaches HTTP dispatch');
        const initialized=await client.request('initialize',{},3000);assert.deepEqual(initialized.result.future,{retained:true});
        await client.notify('notifications/initialized',3000);
        assert.deepEqual((await client.request('tools/list',{},3000)).result,{complete:{unknown:[1,2,3]}});
        assert.equal(seen[0].headers.authorization,authorization);assert.equal(seen[0].headers['x-airk-response-mode'],'full');
        assert.equal(seen[1].headers['mcp-session-id'],'actual-fixture-session');
        assert.equal(seen[0].socket,seen[1].socket,'Actual native session connection reused');
        assert.equal(seen[1].socket,seen[2].socket);
        await assert.rejects(client.request('refuse',{},3000),error=>error instanceof InstalledHttpFailure&&error.kind==='http'&&(error.detail.body as any).future.retained===true);
        const before=seen.length;
        await assert.rejects(client.request('reset',{},3000),error=>error instanceof InstalledHttpFailure&&error.kind==='network'&&error.detail.response_bytes===0);
        assert.equal(seen.length,before+1,'One request, no retry');
        const receipt=await client.close() as any;assert.equal(receipt.client_closed,true);assert.equal(receipt.service_stop_requested,false);assert.equal(receipt.server_session_delete.state,'acknowledged');
        assert.equal(seen.at(-1).method,'DELETE');assert.equal(server.listening,true);
    }finally{await untrusted.close();await client.close();for(const socket of sockets)socket.destroy();await new Promise<void>(resolve=>server.close(()=>resolve()));}
});
