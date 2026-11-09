// SPDX-License-Identifier: Apache-2.0
/** Shared error metadata; this module has no native process or editor imports. */
export class ServiceError extends Error {
    constructor(readonly code:string,readonly outcomeUnknown=false,readonly nativeResponse?:Record<string,unknown>){super(`AIRK service: ${code}`);}
}
export function isLocalTrackingFailure(error:unknown):error is ServiceError&{localTrackingUnconfirmed:true}{
    return error instanceof ServiceError&&(error as {localTrackingUnconfirmed?:unknown}).localTrackingUnconfirmed===true;
}
export function isNativeReplyValidationFailure(error:unknown):error is ServiceError&{nativeReplyUnvalidated:true}{
    return error instanceof ServiceError&&(error as {nativeReplyUnvalidated?:unknown}).nativeReplyUnvalidated===true;
}
function originalCode(error:unknown,fallback:string):string{
    const value=error&&typeof error==='object'?(error as {code?:unknown}).code:undefined;
    if(typeof value==='string'&&/^[A-Za-z0-9_]{1,128}$/.test(value))return value;
    // Pure contract validators use Error(machine_code), whereas transport
    // errors expose .code. Preserve both without displaying arbitrary messages.
    const message=error instanceof Error?error.message:undefined;
    return typeof message==='string'&&/^[a-z][a-z0-9_]{0,127}$/.test(message)?message:fallback;
}
/** Retain rejected frames for diagnosis without calling them accepted outcomes
 * or failures of local persistence. */
export function retainUnvalidatedNativeReply(error:unknown,response:Record<string,unknown>,fallback='native_response_validation_failed'):ServiceError&{nativeReplyUnvalidated:true}{
    if(isNativeReplyValidationFailure(error))return error;
    const retained=Object.assign(new ServiceError(originalCode(error,fallback),true,response),{nativeReplyUnvalidated:true as const});retained.cause=error;return retained;
}
export function validateNativeReply<T>(response:Record<string,unknown>,validate:()=>T):T{
    try{return validate();}catch(error){throw retainUnvalidatedNativeReply(error,response);}
}

/** The native reply remains historical evidence even when its local tracking
 * cannot be saved or retired. Uncertainty concerns local tracking; the complete
 * reply is retained, and this failure never authorizes another native write. */
export function retainNativeOutcome(error:unknown,response:Record<string,unknown>,fallback='local_native_tracking_unconfirmed'):ServiceError{
    if(isLocalTrackingFailure(error)||isNativeReplyValidationFailure(error)){
        if(error.nativeResponse===response)return error;
        try{if(JSON.stringify(error.nativeResponse)===JSON.stringify(response))return error;}catch{/* Retain a distinct outer reply separately through the cause. */}
    }
    const code=originalCode(error,fallback);
    const retained=Object.assign(new ServiceError(code,true,response),{localTrackingUnconfirmed:true as const});
    retained.cause=error;return retained;
}

export async function retainNativeOutcomeDuring<T>(response:Record<string,unknown>,work:()=>Promise<T>):Promise<T>{
    try{return await work();}catch(error){throw retainNativeOutcome(error,response);}
}
