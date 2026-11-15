// SPDX-License-Identifier: Apache-2.0
// Explicit browser inputs; never a claim that the browser observed Git or saved files.
import type {ReviewChange} from '../../ui/src/review';
export function manualReviewChanges(text:string,format:'paths'|'patch',change:ReviewChange['change']='modified'):ReviewChange[]{
 const entries=new Map<string,ReviewChange>();
 const path=(raw:string)=>{if(!raw||raw!==raw.trim()||raw.length>4096||/[\\:\u0000\t]/.test(raw)||raw.startsWith('/')||raw.split('/').some(part=>!part||part==='.'||part==='..'))throw Error('Use exact relative indexed paths without parent segments, drive prefixes, tabs or surrounding whitespace. Quoted Git paths must be entered explicitly in Paths mode.');return raw;};
 const add=(file:string,kind:ReviewChange['change'],originalPath?:string)=>{const value=path(file);if(format==='patch'&&entries.has(value))throw Error('Repeated Git destination sections are unsupported. The whole patch was refused; enter every old and new changed path explicitly in Paths mode.');entries.set(value,{path:value,file:value,change:kind,...(originalPath?{originalPath:path(originalPath)}:{})});};
 if(format==='paths'){for(const line of text.split(/\r?\n/)){if(line.trim())add(line,change);}}
 else{
  const unsupported=(reason:string):never=>{throw Error(`${reason} The whole patch was refused; enter every changed path explicitly in Paths mode.`);};
  let headerBefore:string|undefined,headerAfter:string|undefined,before:string|undefined,after:string|undefined;
  let sawBefore=false,sawAfter=false,sawRenameBefore=false,sawRenameAfter=false,sections=0,oldRemaining=0,newRemaining=0;
  const flush=()=>{
   if(!sections)return;
   if(oldRemaining||newRemaining)unsupported('An incomplete unified hunk is unsupported.');
   if(!(sawBefore&&sawAfter)&&!(sawRenameBefore&&sawRenameAfter))unsupported('A binary, mode-only or incomplete Git patch section is unsupported.');
   if(before&&before!==headerBefore||after&&after!==headerAfter)unsupported('Git section paths disagree with the supplied file headers.');
   if(before&&after)add(after,before===after?'modified':'renamed',before===after?undefined:before);else if(after)add(after,'added');else if(before)add(before,'deleted');else unsupported('A Git section has no usable source path.');
  };
  for(const line of text.split(/\r?\n/)){
   if(line.startsWith('diff ')){
    if(!line.startsWith('diff --git '))unsupported('Only separate diff --git sections are supported.');
    flush();if(line.includes('"'))unsupported('Quoted or escaped Git paths are unsupported.');
    const header=/^diff --git a\/(.+) b\/(.+)$/.exec(line);if(!header)unsupported('A Git section header is malformed.');
    headerBefore=path(header![1]);headerAfter=path(header![2]);before=after=undefined;sawBefore=sawAfter=sawRenameBefore=sawRenameAfter=false;oldRemaining=newRemaining=0;sections++;continue;
   }
   if(!sections){if(line.startsWith('--- ')||line.startsWith('+++ ')||line.startsWith('@@'))unsupported('Standalone unified patches need explicit Git section separators.');continue;}
   if(oldRemaining||newRemaining){
    if(line==='\\ No newline at end of file')continue;
    if(line.startsWith(' ')){oldRemaining--;newRemaining--;}
    else if(line.startsWith('-'))oldRemaining--;
    else if(line.startsWith('+'))newRemaining--;
    else unsupported('An incomplete or malformed unified hunk is unsupported.');
    if(oldRemaining<0||newRemaining<0)unsupported('Unified hunk counts disagree with their content.');
    continue;
   }
   if(line.startsWith('Binary files ')||line==='GIT binary patch')unsupported('Binary patch contents are unsupported.');
   if(line.startsWith('@@')){
    if(!sawBefore||!sawAfter)unsupported('A hunk has no complete file header.');
    const hunk=/^@@ -[0-9]+(?:,([0-9]+))? \+[0-9]+(?:,([0-9]+))? @@/.exec(line);if(!hunk)unsupported('Combined or malformed hunk headers are unsupported.');
    oldRemaining=hunk![1]===undefined?1:Number(hunk![1]);newRemaining=hunk![2]===undefined?1:Number(hunk![2]);
    if(!Number.isSafeInteger(oldRemaining)||!Number.isSafeInteger(newRemaining))unsupported('Unified hunk counts exceed the supported range.');
   }else if(line.startsWith('--- ')){
    if(sawBefore)unsupported('Every changed file needs its own diff --git section.');
    const name=line.slice(4);if(name.startsWith('"'))unsupported('Quoted file headers are unsupported.');before=name==='/dev/null'?undefined:path(name.startsWith('a/')?name.slice(2):name);sawBefore=true;
   }else if(line.startsWith('+++ ')){
    if(!sawBefore||sawAfter)unsupported('Every changed file needs one ordered pair of file headers.');
    const name=line.slice(4);if(name.startsWith('"'))unsupported('Quoted file headers are unsupported.');after=name==='/dev/null'?undefined:path(name.startsWith('b/')?name.slice(2):name);sawAfter=true;
   }else if(line.startsWith('rename from ')){if(sawRenameBefore)unsupported('Duplicate rename metadata is unsupported.');before=path(line.slice(12));sawRenameBefore=true;}
   else if(line.startsWith('rename to ')){if(sawRenameAfter)unsupported('Duplicate rename metadata is unsupported.');after=path(line.slice(10));sawRenameAfter=true;}
  }
  if(!sections)unsupported('No supported Git diff sections were found.');flush();
 }
 if(!entries.size)throw Error('Supply at least one changed relative path.');
 return [...entries.values()].sort((a,b)=>a.path.localeCompare(b.path));
}
