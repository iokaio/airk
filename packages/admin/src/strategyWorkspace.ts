// SPDX-License-Identifier: Apache-2.0
// Transient presentation drafts, bound to one exact management-session object.
import {reviseStudio,type StudioDraft,type PlannerDraft,type RunResult,PLAN_SCHEMA,type Plan} from '../../ui/src/strategies';
import {currentSession,onSessionChange} from './session';
import type {BrowserStrategyRun} from './strategyRunner';

const emptyStudio=():StudioDraft=>({params:{},pathPrefix:'',objective:'',profile:'portable',promptEdited:false,revision:0});
export function createStrategyWorkspace(checkOwner:()=>void=()=>{}) {
  let studio=emptyStudio(),planner:PlannerDraft={revision:0},proof:BrowserStrategyRun|undefined,scope:string|undefined,closed=false;
  const listeners=new Set<()=>void>();
  const publish=()=>{for(const listener of listeners)listener();};
  const check=()=>{if(closed)throw Error('This strategy draft belongs to an ended session.');checkOwner();};
  return {
    check,
    view:()=>({studio,planner,proof}),
    subscribe(listener:()=>void){listeners.add(listener);return()=>{listeners.delete(listener);};},
    editStudio(update:(value:StudioDraft)=>StudioDraft){check();const next=reviseStudio(studio,update(studio));if(next!==studio){studio=next;proof=undefined;publish();}},
    setScope(next:string|undefined){check();if(next===scope)return;scope=next;studio=reviseStudio(studio,{...studio,run:undefined,saved:undefined,prompt:studio.promptEdited?studio.prompt:undefined,promptStale:studio.promptEdited});proof=undefined;publish();},
    acceptRun(result:BrowserStrategyRun,revision:number|undefined){check();if(studio.revision!==revision)return false;studio=reviseStudio(studio,{...studio,run:result.run,...(studio.promptEdited?{promptStale:true}:{prompt:undefined,promptStale:false})});proof=result;publish();return true;},
    editPlan(update:(value:Plan)=>Plan){check();const original=planner.plan??{schema:PLAN_SCHEMA,title:'New plan',objective:studio.objective,tasks:[]};planner={plan:update(original),revision:planner.revision+1};publish();},
    replacePlan(plan:Plan,revision:number){check();if(revision!==planner.revision)return false;planner={plan,revision:planner.revision+1};publish();return true;},
    close(){closed=true;studio=emptyStudio();planner={revision:0};proof=undefined;publish();listeners.clear();},
  };
}
export type StrategyWorkspace=ReturnType<typeof createStrategyWorkspace>;
const sessions=new WeakMap<object,StrategyWorkspace>();
let prior:object|null=currentSession();
onSessionChange(session=>{if(prior&&prior!==session)sessions.get(prior)?.close();prior=session;});
export function strategyWorkspaceFor(session:object):StrategyWorkspace {
  let value=sessions.get(session);if(!value){value=createStrategyWorkspace(()=>{if(currentSession()!==session)throw Error('The management session changed.');});sessions.set(session,value);}return value;
}
export type {RunResult};
