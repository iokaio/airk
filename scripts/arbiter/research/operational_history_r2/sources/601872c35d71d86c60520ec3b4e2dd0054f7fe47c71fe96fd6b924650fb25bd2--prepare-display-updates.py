# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import hashlib,json
p=Path('artifacts/optimizations/f17-draft/display-updates');combined=Path('artifacts/optimizations/f17-draft/combined-save-client-source')
names=['packages/vscode/src/workbenchView.ts','packages/vscode/src/workbenchContracts.ts','packages/vscode/src/workbenchRender.ts','packages/vscode/src/workbenchWebview/main.tsx','packages/vscode/src/test/workbenchClientLifetime.test.ts']
record={}
for name in names:
 src=combined/name if (combined/name).exists() else Path(name)
 record[name]={'sha256':hashlib.sha256(src.read_bytes()).hexdigest(),'layer':src.as_posix()}
 for layer in ['base','source']:
  f=p/layer/name;f.parent.mkdir(parents=True,exist_ok=True);f.write_bytes(src.read_bytes())
(p/'BASE.json').write_text(json.dumps({'base':record,'combined_input_sha256':hashlib.sha256(Path('artifacts/optimizations/f17-draft/combined-save-client.patch').read_bytes()).hexdigest()},indent=2)+'\n',encoding='utf-8')
f=p/'source/packages/vscode/src/workbenchContracts.ts';s=f.read_text(encoding='utf-8');pos=s.index('export type WorkbenchMessage =')
s=s[:pos]+'''/** Host-to-webview display updates. Full result ownership stays in the host. */
export type WorkbenchUpdate =
    | {type:'state';state:WorkbenchState}
    | {type:'status';sessionId:string|undefined;project:string|undefined;status:Value|undefined}
    | {type:'saved';sessionId:string;project:string;revision:number;recent:WorkbenchState['recent'];save:string};
/** Shared actual webview consumer; delta fields cannot replace result bodies. */
export function applyWorkbenchUpdate(previous:WorkbenchState,message:WorkbenchUpdate):WorkbenchState {
    if(message.type==='state')return message.state;
    if(message.sessionId!==previous.session?.id||message.project!==previous.session?.project)return previous;
    if(message.type==='status')return {...previous,status:message.status};
    if(!previous.session)return previous;
    return {...previous,session:{...previous.session,revision:message.revision},recent:message.recent,save:message.save};
}
'''+s[pos:];f.write_text(s,encoding='utf-8',newline='\n')
f=p/'source/packages/vscode/src/workbenchRender.ts';s=f.read_text(encoding='utf-8');pos=s.index('export function canonical(')
s=s[:pos]+'''/** A display-only cache owned by one view, bounded to its current transcript.
 * Completed native envelopes are replaced, never mutated by WorkbenchView.
 * No export, continuation admission, citation validation or history read uses it. */
export class CurrentTurnProjections {
    private readonly retained=new Map<WorkbenchTurn,{envelope:Value|undefined;cards:EvidenceCard[];continuations:Continuation[]}>();
    project(turns:WorkbenchTurn[]):{cards:Record<string,EvidenceCard[]>;continuations:Record<string,Continuation[]>} {
        const current=new Set(turns);
        for(const turn of this.retained.keys())if(!current.has(turn))this.retained.delete(turn);
        const cards:Record<string,EvidenceCard[]>={},pages:Record<string,Continuation[]>={};
        for(const turn of turns){
            let value=this.retained.get(turn);
            if(!value||value.envelope!==turn.envelope){
                value={envelope:turn.envelope,cards:evidenceCards(turn.envelope??{}),continuations:continuations(turn.envelope??{})};
                this.retained.set(turn,value);
            }
            cards[turn.id]=value.cards;pages[turn.id]=value.continuations;
        }
        return {cards,continuations:pages};
    }
    clear():void{this.retained.clear();}
}
'''+s[pos:];f.write_text(s,encoding='utf-8',newline='\n')
f=p/'source/packages/vscode/src/workbenchView.ts';s=f.read_text(encoding='utf-8').replace('continuations, evidenceCards, renderExport','CurrentTurnProjections, continuations, evidenceCards, renderExport')
s=s.replace('    private view?:vscode.WebviewView;', '    private view?:vscode.WebviewView;\n    private readonly display=new CurrentTurnProjections();')
s=s.replace("            cards:Object.fromEntries((this.session?.turns??[]).map(t=>[t.id,evidenceCards(t.envelope??{})])),continuations:Object.fromEntries((this.session?.turns??[]).map(t=>[t.id,continuations(t.envelope??{})])),busy:","            ...this.display.project(this.session?.turns??[]),busy:")
needle="    private send():void{if(!this.disposed){void this.view?.webview.postMessage({type:'state',state:this.state()});}}"
s=s.replace(needle,needle+'''
    private sendStatus():void{if(!this.disposed){void this.view?.webview.postMessage({type:'status',sessionId:this.session?.id,project:this.session?.project,status:this.status});}}
    private sendSaved(session:WorkbenchSession):void{if(!this.disposed&&this.session===session){void this.view?.webview.postMessage({type:'saved',sessionId:session.id,project:session.project,revision:session.revision,recent:this.recent,save:this.saving});}}
''')
s=s.replace('this.status=result.payload;this.send();','this.status=result.payload;this.sendStatus();')
s=s.replace("this.saving=results?'Auto-saved':'Inputs saved; results excluded';this.send();", "this.saving=results?'Auto-saved':'Inputs saved; results excluded';this.sendSaved(session);")
# Clear the display-only references when the view/provider ends; full host history remains authoritative.
s=s.replace('this.view=undefined;this.visibilityChanged.fire();this.stop();this.releaseClient();', 'this.view=undefined;this.display.clear();this.visibilityChanged.fire();this.stop();this.releaseClient();')
# state() previously executed even without a view because optional-call args are skipped. Preserve that behavior.
s=s.replace('this.disposed=true;', 'this.disposed=true;this.display.clear();')
f.write_text(s,encoding='utf-8',newline='\n')
f=p/'source/packages/vscode/src/workbenchWebview/main.tsx';s=f.read_text(encoding='utf-8').replace('useEffect, useRef, useState','useEffect, useMemo, useRef, useState').replace('object, WorkbenchMessage, WorkbenchState, WorkbenchTurn','applyWorkbenchUpdate, object, WorkbenchMessage, WorkbenchState, WorkbenchTurn')
s=s.replace("    const rows=cards.filter(c=>`${c.path} ${c.text}`.toLowerCase().includes(filter.toLowerCase()));\n    if(group){rows.sort((a,b)=>a.path.localeCompare(b.path)||a.start-b.start);}", "    const rows=useMemo(()=>{const matching=cards.filter(c=>`${c.path} ${c.text}`.toLowerCase().includes(filter.toLowerCase()));if(group)matching.sort((a,b)=>a.path.localeCompare(b.path)||a.start-b.start);return matching;},[cards,filter,group]);\n    const planText=useMemo(()=>canonical(plan),[turn.envelope]);\n    const formatted=useMemo(()=>format==='json'?canonical(envelope):format==='markdown'?markdown(turn):'', [format,turn]);")
s=s.replace('canonical(plan)', 'planText')
# Restore the real memo producer after replacing its consumers.
s=s.replace('const planText=useMemo(()=>planText,[turn.envelope]);','const planText=useMemo(()=>canonical(plan),[turn.envelope]);')
s=s.replace("{format==='json'?canonical(envelope):markdown(turn)}",'{formatted}')
s=s.replace("if(event.data?.type==='state'){setState(event.data.state);}", "if(['state','status','saved'].includes(event.data?.type)){setState(previous=>applyWorkbenchUpdate(previous,event.data));}")
f.write_text(s,encoding='utf-8',newline='\n')
