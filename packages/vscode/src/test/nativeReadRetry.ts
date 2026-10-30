// SPDX-License-Identifier: Apache-2.0
/** Native test reads honor only explicit safe-to-repeat replies, with exact arguments. */
import {execFile} from 'node:child_process';
import {NativeProcessError} from '../native';
import {readRetryDelay} from '../explorerEvidence';
import {strict as assert} from 'node:assert';
import {RecordValue} from '../serviceClient';
import {McpStdioSession} from '../mcpStdioSession';

// The Explorer evidence client and these fixtures honor the same native retry hints.
export {readRetryDelay};

/** A bounded reply excerpt keeps one hosted failure diagnosable without unbounded logs. */
export function boundedReply(value:unknown,limit=2048):string{
    let text:string;
    try{text=JSON.stringify(value)??String(value);}catch{text=String(value);}
    return text.length>limit?`${text.slice(0,limit)}... (${text.length} bytes)`:text;
}

/** Repeat one MCP read only after an explicit typed rejection, within its original budget. */
export async function requestWithReadRetry(session:McpStdioSession,name:string,args:Record<string,unknown>={},timeout=120000):Promise<any>{
    const deadline=performance.now()+timeout;
    while(true){
        const result=await session.request('tools/call',{name,arguments:args},Math.max(1,deadline-performance.now()));
        const delay=readRetryDelay(result);
        if(delay!==undefined&&performance.now()+delay<deadline){await new Promise(resolve=>setTimeout(resolve,delay));continue;}
        assert.notEqual(result?.isError,true,`${name} failed: ${boundedReply(result)}`);
        return result;
    }
}

/** Unknown quick readiness is valid only for the explicit deadline contract, never invented evidence. */
export function assertQuickStatusTimeout(status:RecordValue):void{
    const error=status.index_error as RecordValue|undefined;
    assert.equal(status.index_state,'unknown');
    assert.ok(error?.code==='status_timeout'&&error.retryable===true,'Unknown readiness requires an explicit status_timeout retry hint');
    assert.equal(status.health,'unknown');
    assert.equal(typeof status.project,'string');
    assert.match(String(status.project),/^[a-f0-9]{64}$/);
    for(const field of ['publication','files','chunks'])assert.equal(status[field],null,'An unconfirmed observation must not invent evidence');
}

/** Observe the same scoped read after an explicit timeout/refusal; never accept either as readiness. */
export async function observeQuickStatus(read:(deadline:number)=>Promise<RecordValue>,deadline=performance.now()+60000):Promise<RecordValue>{
    let project:unknown;
    while(performance.now()<deadline){
        const status=await read(deadline);
        assert.ok(performance.now()<deadline,'Quick status observation budget exhausted');
        if(status.isError===true){
            // A timed-out call can still hold native materialization admission.
            // Only the existing exact read-retry contract permits another read.
            const delay=readRetryDelay(status);
            assert.ok(delay!==undefined,`status failed: ${boundedReply(status)}`);
            assert.ok(performance.now()+delay<deadline,'Quick status observation budget exhausted');
            await new Promise(resolve=>setTimeout(resolve,delay));continue;
        }
        if(project!==undefined)assert.equal(status.project,project,'Status retries must retain the configured source scope');
        if(status.index_state!=='unknown')return status;
        assertQuickStatusTimeout(status);project=status.project;
        assert.ok(performance.now()+50<deadline,'Quick status observation budget exhausted');
        await new Promise(resolve=>setTimeout(resolve,50));
    }
    throw new Error('Quick status observation budget exhausted');
}

export async function readNative(command:string,args:readonly string[],cwd:string,until?:number):Promise<unknown>{
    const deadline=Math.min(until??Infinity,performance.now()+120000);
    while(true){
        const started=performance.now();
        if(started>=deadline)throw new Error('Native fixture read exceeded its original observation budget');
        const timeout=Math.max(1,Math.ceil(deadline-started));
        const result=await new Promise<{envelope:unknown;delay?:number}>((resolve,reject)=>{
            execFile(command,[...args],{cwd,timeout,maxBuffer:1024*1024,windowsHide:true,encoding:'utf8'},(error,stdout,stderr)=>{
                let envelope:unknown;try{envelope=JSON.parse(stdout);}catch{ /* Report the actual process failure below. */ }
                const delay=readRetryDelay(envelope);
                if(error&&!(error.code===1&&!error.killed&&!error.signal&&delay!==undefined)){
                    reject(new NativeProcessError(command,error,stderr,Math.round(performance.now()-started),timeout));return;
                }
                if(envelope===undefined){reject(new Error('Native read returned no JSON envelope'));return;}
                resolve({envelope,delay});
            });
        });
        if(result.delay===undefined||performance.now()+result.delay>=deadline)return result.envelope;
        await new Promise(resolve=>setTimeout(resolve,result.delay));
    }
}
