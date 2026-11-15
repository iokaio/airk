// SPDX-License-Identifier: Apache-2.0
// Presentation custody only; the captured native scope remains the authority.
import type {ScopedReadCapture} from './SearchScreen';
export function guardWorkflowCapture(actual:ScopedReadCapture,isCurrent:()=>boolean):ScopedReadCapture{
 const checkCurrent=()=>{if(!isCurrent())throw Error('The original workflow view was hidden or replaced; no further read was sent.');actual.checkCurrent();};
 return {...actual,checkCurrent,read:async(tool,args)=>{checkCurrent();const reply=await actual.read(tool,args).catch(failure=>{checkCurrent();throw failure;});checkCurrent();return reply;}};
}
