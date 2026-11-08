// SPDX-License-Identifier: Apache-2.0
/** Actual qualification transport. Complete RPC/native bodies stay in evidence. */
import {strict as assert} from 'node:assert';
import * as https from 'node:https';
import * as tls from 'node:tls';
import type {ClientRequest} from 'node:http';
import type {Socket} from 'node:net';
import {httpUrl} from '../serviceHttp';

export class InstalledHttpFailure extends Error {
    constructor(readonly kind:'http'|'network'|'protocol'|'timeout'|'closed',readonly detail:{code?:string;status?:number;response_bytes:number;body?:unknown}){
        super(`Installed HTTPS ${kind} refusal (${detail.code??detail.status??'unavailable'})`);
    }
}
export function installedHttpIdentity(definition:any):{url:string;credentialId:string;headers:Record<string,string>} {
    assert.ok(definition&&typeof definition.uri?.toString==='function');
    for(const field of ['command','args','env','cwd'])assert.equal(definition[field],undefined,'HTTPS definition cannot start a client process');
    const url=httpUrl(definition.uri.toString()),headers:Record<string,string>={},seen=new Set<string>();
    assert.ok(definition.headers&&typeof definition.headers==='object');
    for(const [name,value] of Object.entries(definition.headers)){
        const key=name.toLowerCase();assert.ok(!seen.has(key),'Duplicate definition header');seen.add(key);
        assert.ok(/^[a-z0-9-]{1,64}$/.test(key)&&typeof value==='string'&&value.length<=4096&&!/[\r\n]/.test(value));
        assert.ok(!['host','content-type','content-length','transfer-encoding','connection','accept','mcp-session-id','mcp-protocol-version'].includes(key),'Definition cannot replace transport framing');
        headers[name]=value as string;
    }
    const authorization=Object.entries(headers).find(([name])=>name.toLowerCase()==='authorization')?.[1];
    const match=authorization?.match(/^Bearer airk1\.([a-f0-9]{32})\.[a-f0-9]{64}$/);assert.ok(match,'Actual native bearer definition');
    const mode=Object.entries(headers).find(([name])=>name.toLowerCase()==='x-airk-response-mode')?.[1];assert.ok(mode==='full'||mode==='terse');
    return {url,credentialId:match[1],headers};
}
export function installedHttpReply(bytes:Buffer,type:string,id:number|undefined,complete=true):{messages:any[];reply?:any} {
    // Decode only complete SSE frames: a chunk may end inside UTF-8.
    if(type==='text/event-stream'&&!complete){const lf=bytes.lastIndexOf(Buffer.from('\n\n')),crlf=bytes.lastIndexOf(Buffer.from('\r\n\r\n'));const end=Math.max(lf<0?-1:lf+2,crlf<0?-1:crlf+4);if(end<0)return {messages:[]};bytes=bytes.subarray(0,end);}
    let text=new TextDecoder('utf-8',{fatal:true}).decode(bytes),messages:any[]=[];
    if(type==='application/json'){if(!complete)return {messages};messages=[JSON.parse(text)];}
    else {
        assert.equal(type,'text/event-stream','Actual negotiated JSON/SSE content type');
        text=text.replace(/\r\n/g,'\n');const blocks=text.split('\n\n');const tail=blocks.pop()!;
        if(complete)assert.equal(tail.trim(),'','Truncated SSE event');
        for(const block of blocks){const data=block.split('\n').filter(line=>line.startsWith('data:')).map(line=>line.slice(5).replace(/^ /,'')).join('\n');if(data)messages.push(JSON.parse(data));}
    }
    assert.ok(messages.length<=512,'Bounded SSE event count');let reply:any;
    for(const message of messages){
        assert.ok(message&&message.jsonrpc==='2.0'&&!Array.isArray(message),'Actual RPC object');
        if(message.id!==undefined){assert.equal(message.id,id,'Crossed RPC response');assert.equal(reply,undefined,'Duplicate RPC response');assert.notEqual(Object.hasOwn(message,'result'),Object.hasOwn(message,'error'));reply=message;}
        else assert.equal(typeof message.method,'string','RPC notification');
    }
    if(complete&&id!==undefined)assert.ok(reply,'Missing matching RPC response');
    return {messages,...(reply?{reply}:{})};
}
export class InstalledHttpTransport {
    private readonly agent:https.Agent;
    private readonly pending=new Set<ClientRequest>();
    private readonly sockets=new Set<Socket>();
    private sequence=0;private session?:string;private protocol='2025-11-25';private closed=false;private received=0;
    readonly observations:unknown[]=[];
    private readonly identity:ReturnType<typeof installedHttpIdentity>;
    constructor(definition:any,ca:Buffer){
        this.identity=installedHttpIdentity(definition);
        const roots=typeof tls.getCACertificates==='function'?tls.getCACertificates('default'):tls.rootCertificates;
        this.agent=new https.Agent({keepAlive:true,maxSockets:8,ca:[...roots,ca],rejectUnauthorized:true});
    }
    request(method:string,params:unknown,timeout:number):Promise<any>{return this.exchange('POST',{jsonrpc:'2.0',id:++this.sequence,method,params},timeout);}
    notify(method:string,timeout:number):Promise<any>{return this.exchange('POST',{jsonrpc:'2.0',method},timeout);}
    private exchange(verb:'POST'|'DELETE',message:any,timeout:number):Promise<any>{
        if(this.closed)return Promise.reject(new InstalledHttpFailure('closed',{response_bytes:0}));
        assert.ok(this.pending.size<8&&timeout>0,'Bounded pending HTTP requests/deadline');
        const bytes=message===undefined?Buffer.alloc(0):Buffer.from(JSON.stringify(message));assert.ok(bytes.length<=65536,'Native request byte bound');
        return new Promise((resolve,reject)=>{
            let settled=false,total=0;const chunks:Buffer[]=[];let status:number|undefined;
            const finish=(error?:unknown,value?:unknown)=>{if(settled)return;settled=true;clearTimeout(timer);error?reject(error):resolve(value);};
            const headers={...this.identity.headers,'Content-Type':'application/json',Accept:'application/json, text/event-stream','MCP-Protocol-Version':this.protocol,...(this.session?{'Mcp-Session-Id':this.session}:{})};
            const request=https.request(this.identity.url,{method:verb,headers,agent:this.agent},response=>{
                status=response.statusCode;const type=String(response.headers['content-type']??'').split(';')[0].trim();
                const session=response.headers['mcp-session-id'];
                if(session!==undefined){if(typeof session!=='string'||!/^[\x21-\x7e]{1,256}$/.test(session)||this.session!==undefined&&session!==this.session){response.destroy();finish(new InstalledHttpFailure('protocol',{response_bytes:0,body:'Changed HTTP session identity'}));return;}this.session=session;}
                const accept=(complete:boolean)=>{
                    if(settled)return;
                    const data=Buffer.concat(chunks);
                    try{
                        if((status??500)>=300){if(!complete)return;let body:unknown;try{body=JSON.parse(data.toString('utf8'));}catch{body=data.toString('utf8');}finish(new InstalledHttpFailure('http',{status,response_bytes:total,body}));return;}
                        if(verb==='DELETE'||message.id===undefined){if(complete){assert.ok([200,202,204].includes(status!));assert.equal(total,0,'Notification/Delete response has no RPC body');finish(undefined,{status});}return;}
                        const decoded=installedHttpReply(data,type,message.id,complete);
                        if(decoded.reply){this.observations.push({method:message.method,status,content_type:type,messages:decoded.messages});
                            if(message.method==='initialize'&&decoded.reply.result){assert.ok(['2025-03-26','2025-06-18','2025-11-25'].includes(decoded.reply.result.protocolVersion));this.protocol=decoded.reply.result.protocolVersion;}
                            finish(undefined,decoded.reply);if(!complete)response.destroy();}
                    }catch(error){finish(new InstalledHttpFailure('protocol',{status,response_bytes:total,body:{error:String(error),retained:data.toString('utf8')}}));response.destroy();}
                };
                response.on('data',(chunk:Buffer)=>{total+=chunk.length;this.received+=chunk.length;if(total>1048576||this.received>67108864){finish(new InstalledHttpFailure('protocol',{status,response_bytes:total,body:'HTTP response byte bound'}));response.destroy();return;}chunks.push(chunk);if(type==='text/event-stream'&&(Buffer.concat(chunks).includes(Buffer.from('\n\n'))||Buffer.concat(chunks).includes(Buffer.from('\r\n\r\n'))))accept(false);});
                response.once('end',()=>accept(true));
                response.once('error',(error:NodeJS.ErrnoException)=>finish(new InstalledHttpFailure('network',{code:error.code??'ERR_STREAM_PREMATURE_CLOSE',status,response_bytes:total,body:Buffer.concat(chunks).toString('utf8')})));
            });
            this.pending.add(request);request.once('socket',socket=>{if(!this.sockets.has(socket)){this.sockets.add(socket);socket.once('close',()=>this.sockets.delete(socket));}});
            request.once('close',()=>{this.pending.delete(request);if(!settled)finish(new InstalledHttpFailure('network',{code:'ERR_STREAM_PREMATURE_CLOSE',status,response_bytes:total}));});
            request.once('error',(error:NodeJS.ErrnoException)=>finish(new InstalledHttpFailure('network',{code:error.code,status,response_bytes:total})));
            const timer=setTimeout(()=>{finish(new InstalledHttpFailure('timeout',{status,response_bytes:total}));request.destroy();},timeout);
            request.end(bytes);
        });
    }
    async close():Promise<unknown>{
        if(this.closed)return {transport:'https',client_closed:true};
        let deletion:unknown={state:'not_stateful'};
        if(this.session){try{deletion={state:'acknowledged',response:await this.exchange('DELETE',undefined,2000)};}catch(error){deletion={state:'unconfirmed',error:String(error),detail:error instanceof InstalledHttpFailure?error.detail:undefined};}}
        this.closed=true;for(const request of this.pending)request.destroy();
        const closing=[...this.sockets].map(socket=>new Promise<void>(resolve=>socket.once('close',()=>resolve())));
        this.agent.destroy();let timer:NodeJS.Timeout|undefined;
        try{await Promise.race([Promise.all(closing),new Promise<never>((_,reject)=>{timer=setTimeout(()=>reject(Error('Owned HTTPS sockets did not close')),3000);})]);}finally{if(timer)clearTimeout(timer);}
        return {transport:'https',client_closed:true,server_session_delete:deletion,received_bytes:this.received,service_stop_requested:false};
    }
}
