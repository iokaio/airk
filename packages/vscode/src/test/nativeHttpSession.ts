// SPDX-License-Identifier: Apache-2.0
/** Actual native qualification adapter. No stdio client, fake grants or global TLS settings. */
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import {strict as assert} from 'node:assert';
import {httpMaterial,type HttpConnectionInfo} from '../serviceHttp';
import {InstalledHttpTransport} from './installedEditorHttp';
export class NativeHttpSession {
    private closed=false;
    private constructor(private readonly transport:InstalledHttpTransport,private readonly info:HttpConnectionInfo){}
    static async open(info:HttpConnectionInfo,clientName:string,timeout=60000):Promise<NativeHttpSession>{
        const material=await httpMaterial(info),deadline=performance.now()+timeout;
        const version=JSON.parse(await fs.readFile(path.resolve(__dirname,'../../package.json'),'utf8')).version;
        const transport=new InstalledHttpTransport({uri:{toString:()=>material.url},headers:{Authorization:'Bearer '+material.credential,'X-AIRK-Response-Mode':'full'}},material.ca);
        const session=new NativeHttpSession(transport,info);
        try{
            await session.request('initialize',{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:clientName,version}},Math.max(1,deadline-performance.now()));
            await transport.notify('notifications/initialized',Math.max(1,deadline-performance.now()));return session;
        }catch(error){await session.close();throw error;}
    }
    async request(method:string,params:object,timeout=120000):Promise<any>{
        const reply=await this.transport.request(method,params,timeout);
        assert.equal(reply.error,undefined,JSON.stringify(reply));return reply.result;
    }
    diagnosticSnapshot():unknown{return {transport:'https',closed:this.closed,connection:this.info,responses:this.transport.observations};}
    async close():Promise<void>{if(this.closed)return;try{await this.transport.close();}finally{this.closed=true;}}
}
