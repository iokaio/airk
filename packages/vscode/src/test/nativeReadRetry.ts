// SPDX-License-Identifier: Apache-2.0
/** Native test reads honor only explicit safe-to-repeat replies, with exact arguments. */
import {execFile} from 'node:child_process';
import {NativeProcessError} from '../native';
import {readRetryDelay} from '../explorerEvidence';

// The Explorer evidence client and these fixtures honor the same native retry hints.
export {readRetryDelay};

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
