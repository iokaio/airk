// SPDX-License-Identifier: Apache-2.0
import type { BootstrapObservation, NativeRunner } from './serviceBootstrap';
import { nativeRunner } from './serviceBootstrap';
import { record, ServiceError } from './serviceClient';
import * as path from 'node:path';

/** Explicit local error details never enter metadata-only reports or source evidence. */
export async function recordDiagnosticFailure(observation:BootstrapObservation|undefined,operation:string,error:unknown,run:NativeRunner=nativeRunner):Promise<void>{
    if(!observation)throw new Error('No verified runtime is available to retain this diagnostic');
    const control=observation.inspection?.log_control_dir??observation.inspection?.control_dir;
    if(typeof control!=='string'||!path.isAbsolute(control)||/[\u0000-\u001f\u007f]/.test(control))throw new Error('Diagnostic control directory is unavailable');
    const chain:string[]=[];let cause:unknown=error;const seen=new Set<unknown>();
    while(cause!==undefined&&!seen.has(cause)&&chain.length<32){
        seen.add(cause);chain.push(cause instanceof Error?(cause.stack??String(cause)):String(cause));
        cause=cause instanceof Error?cause.cause:undefined;
    }
    const bytes=Buffer.from(chain.join('\nCaused by: '));
    const input=JSON.stringify({code:error instanceof ServiceError?error.code:'extension_operation_failed',
        detail:{chain:bytes.subarray(0,8188).toString('utf8'),truncated:bytes.length>8188}});
    const result=await run(observation.bundle.main,['service','record-error','--runtime-dir',control,'--operation',operation],5000,input);
    if(record(result).recorded!==true)throw new Error('Diagnostic error was not acknowledged');
}
