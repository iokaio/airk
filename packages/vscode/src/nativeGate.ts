// SPDX-License-Identifier: Apache-2.0
//! One process-wide gate for spawning native AIRK executables from the editor.
//! The extension installs the user's setting; standalone hooks keep the default.

/** Thrown before any native process starts while execution is disabled. */
export class NativeExecutionDisabled extends Error {
    readonly code='native_execution_disabled';
    constructor(){super('native_execution_disabled: AIRK native execution is turned off (airk.runtime.nativeExecution). Enable it to run AIRK.');}
}
let allowed:()=>boolean=()=>true;
export function setNativeExecutionGate(check:()=>boolean):void{allowed=check;}
export function nativeExecutionAllowed():boolean{return allowed();}
export function requireNativeExecution():void{if(!allowed())throw new NativeExecutionDisabled();}
