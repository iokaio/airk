// SPDX-License-Identifier: Apache-2.0
// Pure draft shapes; shells own their session and storage lifetimes.
import {STUDIO_SCHEMA,type Strategy,type RunResult,type PromptProfile,type Plan,type StudioDocument} from './explorerStrategies';
export interface StudioDraft {
    readonly strategy?: Strategy;
    /** The Library entry this draft came from, for its Runnable/Preview label. */
    readonly origin?: string;
    readonly params: Readonly<Record<string, string>>;
    readonly pathPrefix: string;
    readonly objective: string;
    readonly profile: PromptProfile;
    readonly run?: RunResult;
    readonly prompt?: string;
    readonly promptEdited: boolean;
    readonly promptStale?: boolean;
    /** Increments on every edit; agents name it as expected_target_revision. */
    readonly revision?: number;
    readonly saveName?: string;
    readonly saved?: { readonly name: string; readonly path: string; readonly hash: string; readonly content: string };
}
export interface PlannerDraft {
    readonly plan?: Plan;
    /** Increments on every edit so a late save result for an older revision is ignored. */
    readonly revision: number;
    readonly saved?: { readonly path: string; readonly revision: number; readonly hash: string };
}
export const studioInputs=(draft:StudioDraft)=>JSON.stringify([draft.strategy,draft.params,draft.pathPrefix,draft.objective]);
export function reviseStudio(previous:StudioDraft,next:StudioDraft):StudioDraft {
    if(next===previous)return previous;
    if(studioInputs(next)!==studioInputs(previous))next={...next,run:undefined,prompt:next.promptEdited?next.prompt:undefined,promptStale:next.promptEdited};
    return {...next,revision:(previous.revision??0)+1};
}
export function studioDocument(draft:StudioDraft):StudioDocument {
    return {schema:STUDIO_SCHEMA,strategy:draft.strategy!,objective:draft.objective,
        params:Object.fromEntries(draft.strategy!.params.map(param=>[param.name,draft.params[param.name]??''])),
        pathPrefix:draft.pathPrefix,profile:draft.profile,...(draft.promptEdited?{editedPrompt:draft.prompt??''}:{})};
}
export function studioHasWork(draft:StudioDraft):boolean {
    return !!draft.strategy&&(!!draft.run||draft.promptEdited||!!draft.objective.trim()||Object.values(draft.params).some(value=>value.trim())||!!draft.strategy.id.startsWith('user/'));
}
