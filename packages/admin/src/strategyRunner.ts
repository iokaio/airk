// SPDX-License-Identifier: Apache-2.0
// Browser adapter only: scope, metadata decoding and all read authority stay
// with the existing Search capture and canonical I3 operation.
import {runStrategy,validateStrategy,type RunRequest,type RunResult} from '../../ui/src/strategies';
import {EvidenceFailure,publicationOf,publicationValues,type EvidenceReadPort,type ToolResult} from '../../ui/src/evidenceCore';
import {FactHandles} from '../../vscode/src/explorerFacts';
import {RefRegistry} from '../../vscode/src/explorerSearch';
import type {ScopedReadCapture,ScopedReadObservation} from './SearchScreen';
import {readResultBody} from '../../vscode/src/sourceVerification';
import {comparePublication} from '../../vscode/src/publications';
import helpManifest from '../../ui/generated/help-manifest.json';

/** The legacy extensionVersion field records this compiled UI product version
 * in a browser run; it does not claim that a VS Code extension is installed. */
export const browserProductVersion=helpManifest.productVersion;

export interface BrowserStrategyRun {readonly run:RunResult;readonly reads:readonly ScopedReadObservation[];readonly pendingReads:number}
export async function browserSha256(value:string):Promise<string> {
  if(!globalThis.crypto?.subtle)throw new Error('This browser cannot hash the evidence ledger. Use a supported secure browser context.');
  const bytes=await globalThis.crypto.subtle.digest('SHA-256',new TextEncoder().encode(value));
  return [...new Uint8Array(bytes)].map(byte=>byte.toString(16).padStart(2,'0')).join('');
}
export function boundedWorkflowRead<T>(work:Promise<T>,timeout:number):Promise<T> {
  return new Promise((resolve,reject)=>{
    const timer=setTimeout(()=>reject(new EvidenceFailure({code:'REQUEST_TIMEOUT',message:'The workflow stopped waiting at its original deadline. Native work may still finish; no read was replayed.'})),timeout);
    work.then(value=>{clearTimeout(timer);resolve(value);},error=>{clearTimeout(timer);reject(error);});
  });
}
const object=(value:unknown):Record<string,unknown>=>value!==null&&typeof value==='object'&&!Array.isArray(value)?value as Record<string,unknown>:{};
/** Selection and metadata were verified by the captured Search owner. This
 * projection never guesses a member, drops its native outer, or grants scope. */
export function capturedSingleIndexResult(capture:ScopedReadCapture,reply:ScopedReadObservation):ToolResult {
  capture.checkCurrent();
  if(capture.indexIds.length!==1)throw new Error('Choose one originating index for this workflow.');
  const root=reply.logical;
  if(root.schema!=='airk.combined-read/1')return {payload:root,rawPayload:reply.raw,...(publicationOf(root)?{publication:publicationOf(root)}:{})};
  const members=Array.isArray(root.members)?root.members.map(object):[],vector=object(root.publication_vector),heads=Array.isArray(vector.members)?vector.members.map(object):[];
  const index=capture.indexIds[0];
  if(members.length!==1||heads.length!==1||members[0].index_id!==index||heads[0].index_id!==index
    ||typeof vector.authority!=='string'||!/^[0-9a-f]{64}$/.test(vector.authority))throw new Error('The returned member roster differs from the captured single index.');
  const member=members[0],payload=readResultBody(member.result),native=object(member.result);
  if(member.is_error===true||native.isError===true||!['complete','exhausted'].includes(String(member.outcome))||payload.error) {
    throw new EvidenceFailure({code:'TOOL_ERROR',message:'The native originating member refused the read. Its full original response is retained.'},reply.raw,payload);
  }
  const expected=object(heads[0].publication),token=publicationOf(payload);
  if(heads[0].publication===null&&token===undefined)return {payload,rawPayload:reply.raw};
  if(!token||typeof expected.collection!=='string'||!publicationValues(payload).some(value=>value!==undefined)
    ||publicationValues(payload).filter(value=>value!==undefined).some(value=>comparePublication(
      {authority:vector.authority as string,collection:expected.collection as string,publication:value},
      {authority:vector.authority as string,collection:expected.collection as string,publication:expected})!=='equal'))throw new Error('The native member publication does not match its exact returned vector head.');
  return {payload,rawPayload:reply.raw,publication:token};
}
export async function runBrowserStrategy(request:RunRequest,capture:ScopedReadCapture,options:{sha256?:(text:string)=>Promise<string>;now?:()=>number;monotonic?:()=>number;checkCurrent?:()=>void}={}):Promise<BrowserStrategyRun> {
  const checked=validateStrategy(request.strategy);
  if(!checked.ok)throw new Error(checked.errors.join('\n'));
  if(capture.indexIds.length!==1||!capture.backingBinding)throw new Error('Strategies need one explicit originating index with its native backing metadata. Apply a single index before running.');
  const current=()=>{capture.checkCurrent();options.checkCurrent?.();};current();
  const reads:ScopedReadObservation[]=[];let pendingReads=0;
  const port:EvidenceReadPort={call:async(tool,args,callOptions)=>{
    current();pendingReads++;
    const original=capture.read(tool.replace(/^airk_/,''),structuredClone(args));
    original.then(()=>{pendingReads--;},()=>{pendingReads--;});
    const reply=await boundedWorkflowRead(original,callOptions?.timeout??30000);current();
    reads.push(structuredClone(reply));
    return capturedSingleIndexResult(capture,reply);
  }};
  const run=await runStrategy(request,{session:port,facts:new FactHandles(new RefRegistry()),project:capture.backingBinding,
    extensionVersion:browserProductVersion,sha256:options.sha256??browserSha256,checkCurrent:current,
    now:options.now??Date.now,monotonic:options.monotonic??(()=>performance.now())});
  current();return {run,reads,pendingReads};
}
