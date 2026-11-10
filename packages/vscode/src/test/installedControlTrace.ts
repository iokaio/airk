// SPDX-License-Identifier: Apache-2.0
/** Opt-in private-driver observation. Never changes native requests or replies. */
import childProcess = require('node:child_process');
import * as fs from 'node:fs';
import * as path from 'node:path';
import {performance} from 'node:perf_hooks';

type SpawnHost={spawn:typeof childProcess.spawn};
type TraceRow={method:string;status:string;code?:string;elapsed_ms:number};
export interface InstalledControlTraceOptions {installDir:string;runtimeSha256:string;outputDirectory:string;role:string}
/** Injection replaces only the shared builtin owner in external contract tests. */
export interface InstalledControlTraceDependencies {spawnHost?:SpawnHost;now?:()=>number}
// Normalize Windows path representation after realpath, retaining native case.
const identity=(value:string)=>process.platform==='win32'?path.toNamespacedPath(value):value;
const inside=(root:string,file:string)=>{const relative=path.relative(identity(root),identity(file));return !!relative&&!path.isAbsolute(relative)&&relative!=='..'&&!relative.startsWith('..'+path.sep);};
const name=(value:unknown,maximum=64):value is string=>typeof value==='string'&&value.length<=maximum&&/^[a-z][a-z0-9_]*$/.test(value);

/** A bounded observer for the existing 4-byte-BE JSON console framing. */
function reader(observe:(value:any)=>void,invalid:()=>void):(chunk:unknown)=>void {
    let prefix=Buffer.alloc(4),prefixBytes=0,body:Buffer|undefined,bodyBytes=0,disabled=false;
    return chunk=>{
        try{
            if(disabled||!Buffer.isBuffer(chunk))return;
            let offset=0;
            while(offset<chunk.length){
                if(!body){const count=Math.min(4-prefixBytes,chunk.length-offset);chunk.copy(prefix,prefixBytes,offset,offset+count);prefixBytes+=count;offset+=count;
                    if(prefixBytes<4)continue;const length=prefix.readUInt32BE();if(!length||length>65536){disabled=true;invalid();return;}body=Buffer.alloc(length);bodyBytes=0;}
                const count=Math.min(body.length-bodyBytes,chunk.length-offset);chunk.copy(body,bodyBytes,offset,offset+count);bodyBytes+=count;offset+=count;
                if(bodyBytes===body.length){const bytes=body;body=undefined;prefixBytes=0;bodyBytes=0;observe(JSON.parse(bytes.toString('utf8')));}
            }
        }catch{disabled=true;body=undefined;try{invalid();}catch{/* Observation never owns native I/O. */}}
    };
}

export function installInstalledControlTrace(options:InstalledControlTraceOptions,dependencies:InstalledControlTraceDependencies={}):()=>void {
    const host=dependencies.spawnHost??childProcess,now=dependencies.now??(()=>performance.now()),original=host.spawn;
    const records:TraceRow[]=[],active=new Set<()=>void>();
    let file:string,output:string,install:string,descriptor:number|undefined,firstFailure:TraceRow|undefined,restored=false;
    let seen=0,dropped=0,pendingCount=0,droppedRequests=0,closedChildren=0,ignoredChildren=0,observerErrors=0;
    try{
        if(!['a','b'].includes(options.role)||!/^[a-f0-9]{64}$/.test(options.runtimeSha256)||!path.isAbsolute(options.outputDirectory)||!path.isAbsolute(options.installDir))return ()=>{};
        if(!fs.lstatSync(options.outputDirectory).isDirectory()||fs.lstatSync(options.outputDirectory).isSymbolicLink())return ()=>{};
        output=fs.realpathSync.native(options.outputDirectory);install=path.resolve(options.installDir);
        if(!inside(output,install))return ()=>{};
        file=path.join(output,`${options.role}-control-trace-${process.pid}.json`);
    }catch{return ()=>{};}
    const flush=()=>{
        try{
            descriptor??=fs.openSync(file,'wx',0o600);
            const bytes=JSON.stringify({schema:'airk-installed-control-trace/1',records,summary:{seen,dropped,pending:pendingCount,dropped_requests:droppedRequests,closed_children:closedChildren,ignored_children:ignoredChildren,observer_errors:observerErrors}});
            fs.ftruncateSync(descriptor,0);fs.writeSync(descriptor,bytes,0,'utf8');fs.fsyncSync(descriptor);
        }catch{observerErrors++;}
    };
    const retain=(entry:TraceRow,failure:boolean)=>{
        seen++;if(failure&&!firstFailure)firstFailure=entry;records.push(entry);
        if(records.length>512){const discard=records.findIndex((row,index)=>index>0&&row!==firstFailure);records.splice(discard,1);dropped++;}
        if(failure)flush();
    };
    const matches=(command:unknown,args:unknown)=>{
        try{
            if(typeof command!=='string'||!path.isAbsolute(command)||!Array.isArray(args)||args.length!==6||args[0]!=='service'||args[1]!=='console'||args[2]!=='--runtime-dir'||typeof args[3]!=='string'||args[4]!=='--runtime-sha256'||args[5]!==options.runtimeSha256)return false;
            const root=fs.realpathSync.native(install),executable=fs.realpathSync.native(command),control=fs.realpathSync.native(args[3]);
            return inside(output,root)&&inside(root,executable)&&inside(root,control)
                &&path.relative(identity(control),identity(fs.realpathSync.native(path.join(root,'control'))))==='';
        }catch{return false;}
    };
    const wrapped=function(this:unknown,...args:any[]):any {
        const child:any=Reflect.apply(original,this,args);
        try{
            if(restored||!matches(args[0],args[1])||!child?.stdin?.write||!child?.stdout?.on||!child?.once)return child;
            if(active.size>=8){ignoredChildren++;return child;}
            const pending=new Map<string,{method:string;started:number}>(),write=child.stdin.write;
            const requests=reader(value=>{
                if(typeof value?.id!=='string'||!/^[a-f0-9]{32}$/.test(value.id)||pending.has(value.id))return;
                if(pendingCount>=8){droppedRequests++;return;}
                const method=typeof value.method==='string'?value.method:value.method&&typeof value.method==='object'&&!Array.isArray(value.method)&&Object.keys(value.method).length===1?Object.keys(value.method)[0]:undefined;
                if(!name(method))return;pending.set(value.id,{method,started:now()});pendingCount++;
            },()=>{observerErrors++;});
            const responses=reader(value=>{
                const request=pending.get(value?.id);if(!request)return;pending.delete(value.id);pendingCount--;
                const response=value.response,status=name(response?.status)?response.status:'unrecognized_response',code=name(response?.code,128)?response.code:undefined;
                retain({method:request.method,status,...(code?{code}:{}),elapsed_ms:Math.max(0,Math.round(now()-request.started))},status==='failure'||code!==undefined);
            },()=>{observerErrors++;});
            const observedWrite=function(this:unknown,...writeArgs:any[]):any {try{requests(writeArgs[0]);}catch{observerErrors++;}return Reflect.apply(write,this,writeArgs);};
            const cleanup=()=>{try{if(child.stdin.write===observedWrite)child.stdin.write=write;child.stdout.removeListener('data',responses);child.removeListener('close',closed);}catch{observerErrors++;}
                pendingCount-=pending.size;pending.clear();active.delete(cleanup);};
            const closed=()=>{try{for(const request of pending.values())retain({method:request.method,status:'transport_closed',elapsed_ms:Math.max(0,Math.round(now()-request.started))},true);closedChildren++;cleanup();flush();}catch{observerErrors++;}};
            child.stdin.write=observedWrite;child.stdout.on('data',responses);child.once('close',closed);active.add(cleanup);
        }catch{observerErrors++;}
        return child;
    } as typeof childProcess.spawn;
    try{host.spawn=wrapped;}catch{return ()=>{};}
    return ()=>{
        if(restored)return;restored=true;
        try{if(host.spawn===wrapped)host.spawn=original;}catch{observerErrors++;}
        if(records.length||active.size||observerErrors)flush();
        for(const cleanup of [...active])try{cleanup();}catch{observerErrors++;}
        if(descriptor!==undefined)try{fs.closeSync(descriptor);}catch{observerErrors++;}
    };
}
