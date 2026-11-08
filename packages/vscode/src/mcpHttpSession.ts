// SPDX-License-Identifier: Apache-2.0
import * as https from 'node:https';
import * as tls from 'node:tls';
import { ClientRequest } from 'node:http';
import { McpSessionError } from './mcpStdioSession';
import { httpUrl } from './serviceHttp';
export interface McpHttpOptions {url:string;ca:Buffer;credential:string;clientName:string;clientVersion:string;maxMessageBytes?:number;responseMode?:'full'|'terse'}
/** A bounded Streamable HTTP connection. Closing it never stops the service. */
export class McpHttpSession {
    private readonly pending=new Map<number,ClientRequest>();
    private readonly agent:https.Agent;
    private nextId=1;
    private sessionId?:string;
    private protocol='2025-11-25';
    private failure?:McpSessionError;
    private readonly listeners:((error:McpSessionError)=>void)[]=[];
    private constructor(private readonly options:McpHttpOptions){
        httpUrl(options.url);if(!/^airk1\.[a-f0-9]{32}\.[a-f0-9]{64}$/.test(options.credential))throw Error('invalid_http_credential');
        // Add the private issuer to normal trust; never change process globals.
        const roots=typeof tls.getCACertificates==='function'?tls.getCACertificates('default'):tls.rootCertificates;
        this.agent=new https.Agent({keepAlive:true,maxSockets:16,ca:[...roots,options.ca],rejectUnauthorized:true});
    }
    static async open(options:McpHttpOptions,timeout=90000):Promise<{session:McpHttpSession;initialize:any}>{
        const session=new McpHttpSession(options);
        try{
            const initialize=await session.request('initialize',{protocolVersion:session.protocol,capabilities:{},clientInfo:{name:options.clientName,version:options.clientVersion}},timeout);
            if(typeof initialize?.protocolVersion!=='string'||initialize.protocolVersion.length>64)throw Error('invalid_http_mcp_initialize');
            if(!['2025-03-26','2025-06-18','2025-11-25'].includes(initialize.protocolVersion))throw Error('unsupported_http_mcp_protocol');
            session.protocol=initialize.protocolVersion;await session.sendNotification('notifications/initialized');return {session,initialize};
        }catch(error){session.close();throw error;}
    }
    get closed():boolean{return !!this.failure;}
    get closeReason():McpSessionError|undefined{return this.failure;}
    onClose(listener:(error:McpSessionError)=>void):void{if(this.failure)listener(this.failure);else this.listeners.push(listener);}
    request(method:string,params:unknown,timeout=120000):Promise<any>{
        if(this.failure)return Promise.reject(this.failure);
        if(this.pending.size>=64)return Promise.reject(new McpSessionError('overloaded','AIRK HTTP request queue is full.'));
        return this.exchange({jsonrpc:'2.0',id:this.nextId++,method,params},timeout);
    }
    notify(method:string,params?:unknown):void{void this.sendNotification(method,params).catch(()=>{});}
    private sendNotification(method:string,params?:unknown):Promise<any>{return this.exchange({jsonrpc:'2.0',method,...(params===undefined?{}:{params})},5000);}
    private exchange(message:Record<string,any>,timeout:number):Promise<any>{
        if(this.failure)return Promise.reject(this.failure);
        const bytes=Buffer.from(JSON.stringify(message)),maximum=this.options.maxMessageBytes??1048576;
        if(bytes.length>maximum)return Promise.reject(new McpSessionError('oversize','AIRK HTTP request exceeds byte bound.'));
        return new Promise((resolve,reject)=>{
            const key=message.id??-this.nextId++,headers:Record<string,string>={'Content-Type':'application/json',Accept:'application/json, text/event-stream',Authorization:'Bearer '+this.options.credential,'MCP-Protocol-Version':this.protocol,'X-AIRK-Response-Mode':this.options.responseMode??'full'};
            if(this.sessionId)headers['Mcp-Session-Id']=this.sessionId;
            let finished=false,total=0,buffer='',timer:NodeJS.Timeout;
            const finish=(error?:Error,value?:unknown)=>{if(finished)return;finished=true;clearTimeout(timer);this.pending.delete(key);error?reject(error):resolve(value);};
            const request=https.request(this.options.url,{method:'POST',headers,agent:this.agent},response=>{
                if((response.statusCode??0)>=300){finish(new McpSessionError('rpc',`AIRK HTTPS returned status ${response.statusCode}.`));response.destroy();return;}
                const observed=response.headers['mcp-session-id'];
                if(observed!==undefined){if(typeof observed!=='string'||!/^[\x21-\x7e]{1,256}$/.test(observed)||this.sessionId!==undefined&&this.sessionId!==observed){response.destroy();finish(new McpSessionError('invalid_json','AIRK HTTP session identity changed.'));return;}this.sessionId=observed;}
                if(message.id===undefined&&response.statusCode===202){finish(undefined,undefined);response.destroy();return;}
                const type=String(response.headers['content-type']??'').split(';')[0].trim();if(!['application/json','text/event-stream'].includes(type)){finish(new McpSessionError('invalid_json','AIRK HTTPS returned unsupported content type.'));response.destroy();return;}
                const accept=(wire:any)=>{
                    if(wire?.id!==message.id)return false;
                    if(wire.jsonrpc!=='2.0'||Object.hasOwn(wire,'result')===Object.hasOwn(wire,'error')){finish(new McpSessionError('invalid_json','AIRK HTTPS returned an invalid RPC envelope.'));return true;}
                    if(wire.error){finish(new McpSessionError('rpc','AIRK MCP returned an RPC error.',wire.error));return true;}
                    if(!Object.hasOwn(wire,'result')){finish(new McpSessionError('invalid_json','AIRK MCP result is missing.'));return true;}
                    finish(undefined,wire.result);return true;
                };
                response.setEncoding('utf8');response.on('data',(chunk:string)=>{
                    total+=Buffer.byteLength(chunk);if(total>maximum){response.destroy();finish(new McpSessionError('oversize','AIRK HTTP response exceeds byte bound.'));return;}buffer+=chunk;
                    if(type==='text/event-stream'){
                        let at:number;while((at=buffer.search(/\r?\n\r?\n/))>=0){const block=buffer.slice(0,at),match=buffer.slice(at).match(/^\r?\n\r?\n/)![0];buffer=buffer.slice(at+match.length);const data=block.split(/\r?\n/).filter(row=>row.startsWith('data:')).map(row=>row.slice(5).trimStart()).join('\n');if(!data)continue;try{if(accept(JSON.parse(data))){response.destroy();return;}}catch{response.destroy();finish(new McpSessionError('invalid_json','AIRK HTTP returned invalid MCP JSON.'));return;}}
                    }
                });
                response.once('error',()=>finish(new McpSessionError('exited','AIRK HTTPS response closed before completion.')));
                response.once('end',()=>{if(finished)return;if(type==='application/json'){try{if(accept(JSON.parse(buffer)))return;}catch{}}finish(new McpSessionError('invalid_json','AIRK HTTPS ended without a matching response.'));});
            });
            request.once('error',error=>finish(new McpSessionError('exited',`AIRK HTTPS request failed (${(error as NodeJS.ErrnoException).code??'connection'}).`)));
            request.once('close',()=>{if(!finished)finish(this.failure??new McpSessionError('exited','AIRK HTTPS request closed before completion.'));});
            timer=setTimeout(()=>{request.destroy();finish(new McpSessionError('timeout','AIRK HTTPS request exceeded its deadline.'));if(message.id!==undefined)this.notify('notifications/cancelled',{requestId:message.id,reason:'client deadline'});},Math.max(1,timeout));
            this.pending.set(key,request);request.end(bytes);
        });
    }
    diagnosticSnapshot(){return {pid:null,exitCode:null,signalCode:null,closed:this.closed,stderr:'',transport:'https' as const};}
    close():void{
        if(this.failure)return;this.failure=new McpSessionError('closed','AIRK HTTP session closed.');for(const request of this.pending.values())request.destroy();this.pending.clear();
        if(this.sessionId){const request=https.request(this.options.url,{method:'DELETE',agent:this.agent,headers:{Authorization:'Bearer '+this.options.credential,'Mcp-Session-Id':this.sessionId,'MCP-Protocol-Version':this.protocol}},response=>response.destroy());request.on('error',()=>{});const timer=setTimeout(()=>request.destroy(),1000);timer.unref();request.end();request.once('close',()=>{clearTimeout(timer);this.agent.destroy();});}else this.agent.destroy();
        for(const listener of this.listeners.splice(0))listener(this.failure);
    }
}
