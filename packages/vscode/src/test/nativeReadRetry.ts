// SPDX-License-Identifier: Apache-2.0
/** Native test reads honor only explicit safe-to-repeat replies, with exact arguments. */
import {execFile} from 'node:child_process';
import {NativeProcessError} from '../native';

export function readRetryDelay(envelope:unknown):number|undefined {
    if(!envelope||typeof envelope!=='object')return;
    const value=envelope as Record<string,unknown>,content=value.content;
    if(value.isError!==true||!Array.isArray(content)||content.length!==1||content[0]?.type!=='text'||typeof content[0]?.text!=='string')return;
    let hint:Record<string,any>;
    try{hint=JSON.parse(content[0].text);}catch{return;}
    if(!hint||typeof hint!=='object')return;
    let delay:unknown;
    if(hint.status==='retry'){
        if(hint.retryable!==true||hint.action!=='repeat_same_request'||!['publication_busy','selection_busy'].includes(hint.code))return;
        delay=hint.retry_after_ms;
    }else{
        const error=hint.error;
        if(!error||!['direct_query_admission','direct_query_setup'].includes(error.code)||error.reason!=='busy'
            ||error.query_submitted!==false||error.retryable!==true||error.fresh_brokerage_required!==(error.code==='direct_query_setup'))return;
        delay=error.retry_after_ms;
    }
    return Number.isInteger(delay)&&Number(delay)>=1&&Number(delay)<=1000?Number(delay):undefined;
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
