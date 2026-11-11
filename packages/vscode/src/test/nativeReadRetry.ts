// SPDX-License-Identifier: Apache-2.0
/** Native test reads honor only explicit safe-to-repeat replies, with exact arguments. */
import {execFile} from 'node:child_process';
import {NativeProcessError} from '../native';
import {requireNativeExecution} from '../nativeGate';
import {readRetryDelay} from '../readRetry';
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

export type ReadRetryReceipt={code:string;query_submitted:false}|{code:string;action:'repeat_same_request'};
/** Observe both native receipt shapes through the same strict replay validator. */
export function readRetryReceipt(reply:any,expectedScope?:unknown):ReadRetryReceipt{
    assert.notEqual(readRetryDelay(reply,expectedScope),undefined,`Unrecognized native read refusal: ${boundedReply(reply)}`);
    const body=JSON.parse(reply.content[0].text);
    if(body.schema==='airk.combined-read/1'){
        const code=body.read_retry.code;
        return ['direct_query_admission','direct_query_setup'].includes(code)?{code,query_submitted:false}:{code,action:'repeat_same_request'};
    }
    return body.error?{code:body.error.code,query_submitted:false}:{code:body.code,action:'repeat_same_request'};
}

/** Repeat one MCP read only after an explicit typed rejection, within its original budget. */
export async function requestWithReadRetry(session:Pick<McpStdioSession,'request'>,name:string,args:Record<string,unknown>={},timeout=120000,onReply?:(reply:any)=>void,expectedScope?:unknown):Promise<any>{
    const deadline=performance.now()+timeout;
    let previousRefusal:any;
    while(true){
        if(previousRefusal!==undefined&&performance.now()>=deadline)
            throw Object.assign(new Error(`${name} failed: ${boundedReply(previousRefusal)}`),{nativeResponse:previousRefusal});
        const result=await session.request('tools/call',{name,arguments:args},Math.max(1,deadline-performance.now()));
        // Test assertions and diagnostics see every receipt, including refusals;
        // the shared native hint parser alone determines whether retry is safe.
        onReply?.(result);
        const delay=readRetryDelay(result,expectedScope);
        if(delay!==undefined&&performance.now()+delay<deadline){previousRefusal=result;await new Promise(resolve=>setTimeout(resolve,delay));continue;}
        assert.notEqual(result?.isError,true,`${name} failed: ${boundedReply(result)}`);
        if(performance.now()>=deadline)throw Object.assign(new Error('MCP read observation budget exhausted'),{nativeResponse:result});
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

export type QuickStatusOutcome='available'|'unavailable'|'status_timeout'|'retry'|'error'|'other'|'read_error'|'scope_mismatch'|'invalid_timeout'|'late';
export type QuickStatusObservation=
    {kind:'attempt';attempt:number;durationMs:number;elapsedMs:number;outcome:QuickStatusOutcome}|
    {kind:'confirmed';attempts:number;elapsedMs:number;recovered:boolean};
export type QuickStatusObserver=(observation:QuickStatusObservation)=>void;

/** Observe the same scoped read after an explicit timeout/refusal; never accept either as readiness.
 * Optional measurements contain fixed categories and numbers only; observer failures cannot affect reads.
 */
export async function observeQuickStatus(read:(deadline:number)=>Promise<RecordValue>,deadline=performance.now()+60000,observer?:QuickStatusObserver,expectedScope?:unknown):Promise<RecordValue>{
    let project:unknown;
    const started=observer?performance.now():0;let attempts=0;
    const emit=(value:QuickStatusObservation)=>{try{observer?.(value);}catch{/* Measurements never control native reads. */}};
    while(performance.now()<deadline){
        const attemptStarted=observer?performance.now():0;if(observer)attempts++;
        let status:RecordValue;
        try{status=await read(deadline);}catch(error){
            if(observer){const ended=performance.now();emit({kind:'attempt',attempt:attempts,durationMs:ended-attemptStarted,elapsedMs:ended-started,outcome:'read_error'});}throw error;
        }
        if(observer){
            const ended=performance.now();
            let outcome:QuickStatusOutcome=status.isError===true?(readRetryDelay(status,expectedScope)!==undefined?'retry':'error'):
                status.index_state==='available'?'available':status.index_state==='unavailable'?'unavailable':
                status.index_state==='unknown'?'status_timeout':'other';
            if(ended>=deadline)outcome='late';
            else if(status.isError!==true&&project!==undefined&&status.project!==project)outcome='scope_mismatch';
            else if(status.isError!==true&&status.index_state==='unknown'){
                try{assertQuickStatusTimeout(status);}catch{outcome='invalid_timeout';}
            }
            emit({kind:'attempt',attempt:attempts,durationMs:ended-attemptStarted,elapsedMs:ended-started,outcome});
        }
        assert.ok(performance.now()<deadline,'Quick status observation budget exhausted');
        if(status.isError===true){
            // A timed-out call can still hold native materialization admission.
            // Only the existing exact read-retry contract permits another read.
            const delay=readRetryDelay(status,expectedScope);
            assert.ok(delay!==undefined,`status failed: ${boundedReply(status)}`);
            assert.ok(performance.now()+delay<deadline,'Quick status observation budget exhausted');
            await new Promise(resolve=>setTimeout(resolve,delay));continue;
        }
        if(project!==undefined)assert.equal(status.project,project,'Status retries must retain the configured source scope');
        if(status.index_state!=='unknown'){
            if(observer&&(status.index_state==='available'||status.index_state==='unavailable'))emit({kind:'confirmed',attempts,elapsedMs:performance.now()-started,recovered:attempts>1});
            return status;
        }
        assertQuickStatusTimeout(status);project=status.project;
        assert.ok(performance.now()+50<deadline,'Quick status observation budget exhausted');
        await new Promise(resolve=>setTimeout(resolve,50));
    }
    throw new Error('Quick status observation budget exhausted');
}

export async function readNative(command:string,args:readonly string[],cwd:string,until?:number):Promise<unknown>{
    const deadline=Math.min(until??Infinity,performance.now()+120000);
    // Only the original launch argument can bind an unconfirmed setup receipt.
    const selectors=args.flatMap((arg,index)=>arg==='--catalog-session'?[args[index+1]]:[]);
    let expectedScope:unknown;
    if(selectors.length===1){try{expectedScope=JSON.parse(selectors[0]);}catch{/* Refuse scoped replay without a valid selector. */}}
    while(true){
        const started=performance.now();
        if(started>=deadline)throw new Error('Native fixture read exceeded its original observation budget');
        const timeout=Math.max(1,Math.ceil(deadline-started));
        const result=await new Promise<{envelope:unknown;delay?:number}>((resolve,reject)=>{
            execFile(command,[...args],{cwd,timeout,maxBuffer:1024*1024,windowsHide:true,encoding:'utf8'},(error,stdout,stderr)=>{
                let envelope:unknown;try{envelope=JSON.parse(stdout);}catch{ /* Report the actual process failure below. */ }
                const delay=readRetryDelay(envelope,expectedScope);
                if(error&&!(error.code===1&&!error.killed&&!error.signal&&delay!==undefined)){
                    reject(Object.assign(new NativeProcessError(command,error,stderr,Math.round(performance.now()-started),timeout),
                        {nativeStdout:stdout,nativeStderr:stderr,...(envelope!==undefined?{nativeResponse:envelope}:{})}));return;
                }
                if(envelope===undefined){reject(new Error('Native read returned no JSON envelope'));return;}
                if(performance.now()>=deadline){reject(Object.assign(new Error('Native fixture read exceeded its original observation budget'),
                    {nativeResponse:envelope,nativeStdout:stdout,nativeStderr:stderr}));return;}
                resolve({envelope,delay});
            });
        });
        if(result.delay===undefined||performance.now()+result.delay>=deadline)return result.envelope;
        await new Promise(resolve=>setTimeout(resolve,result.delay));
    }
}

/** One-shot test read with runText's exact process policy and complete failure streams. */
export function capturedNativeText(command:string,args:readonly string[],cwd:string,timeout:number):Promise<string>{
    try{requireNativeExecution();}catch(error){return Promise.reject(error);}
    const started=performance.now();
    return new Promise((resolve,reject)=>{
        execFile(command,[...args],{cwd,timeout,maxBuffer:16*1024*1024,windowsHide:true,encoding:'utf8',
            env:{...process.env,AIRK_CLIENT:'vscode',AIRK_ERROR_FORMAT:'json'}},(error,stdout,stderr)=>{
            if(error){reject(Object.assign(new NativeProcessError(command,error,stderr,
                Math.round(performance.now()-started),timeout),{nativeStdout:stdout,nativeStderr:stderr}));}
            else resolve(stdout);
        });
    });
}
