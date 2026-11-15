// SPDX-License-Identifier: Apache-2.0
// Studio and Planner drafts shared by the Strategies tabs. They live in webview state, so
// switching tabs or hiding the panel keeps them; nothing is written to the workspace from here.
import { useEffect, useState } from 'preact/hooks';
import { STUDIO_SCHEMA, type StudioDocument, type Plan, type PromptProfile, type RunResult, type Strategy } from '../explorerStrategies';
import { notify, savedState, saveState } from './api';

import type {StudioDraft,PlannerDraft} from '../explorerStrategyDrafts';
export type {StudioDraft,PlannerDraft} from '../explorerStrategyDrafts';
interface Stored { studio: StudioDraft; planner: PlannerDraft }

const EMPTY: Stored = { studio: { params: {}, pathPrefix: '', objective: '', profile: 'portable', promptEdited: false }, planner: { revision: 0 } };
let current: Stored = { ...EMPTY, ...(savedState<{ strategies?: Stored }>()?.strategies ?? {}) };
const listeners = new Set<() => void>();
let selection: string | undefined;
export const currentSelection = () => selection;
export const draftVersions = () => ({ studio: current.studio.revision ?? 0, planner: current.planner.revision });
export const studioDraft = () => current.studio;
export const plannerDraft = () => current.planner;
export function updateStudioDraft(update: (draft: StudioDraft) => StudioDraft): void {
    const next = update(current.studio);
    if (next !== current.studio) { commit({ ...current, studio: next }); }
}
export function acceptStudioRun(run: RunResult, revision: number | undefined, owner: string | undefined, prompt: string): boolean {
    if (owner !== selection || revision !== current.studio.revision) { return false; }
    updateStudioDraft(prior => ({ ...prior, run, ...(prior.promptEdited ? { promptStale: true } : { prompt, promptStale: false }) }));
    return true;
}
export function selectDraftProject(project: string | undefined, epoch = 0): void {
    const next = `${project ?? ''}:${epoch}`;
    if (selection === next) { return; }
    selection = next;
    commit({ studio: { ...current.studio, saved: undefined, run: undefined, prompt: current.studio.promptEdited ? current.studio.prompt : undefined,
        promptStale: current.studio.promptEdited }, planner: { ...current.planner, saved: undefined, revision: current.planner.revision + 1 } });
}
import {reviseStudio,studioDocument,studioHasWork} from '../explorerStrategyDrafts';
export {studioDocument,studioHasWork} from '../explorerStrategyDrafts';

/** Tell the host the current draft revisions so agent drafts can detect a changed target. */
export function reportRevisions(): void {
    notify('suggestions.revisions', { studio: current.studio.revision ?? 0, planner: current.planner.revision });
}

function commit(next: Stored): void {
    const project = selection?.split(':')[0];
    if (project && next.planner.plan && !next.planner.plan.project) {
        next = { ...next, planner: { ...next.planner, plan: { ...next.planner.plan, project } } };
    }
    const changed = next.studio !== current.studio || next.planner.revision !== current.planner.revision;
    if(next.studio!==current.studio)next={...next,studio:reviseStudio(current.studio,next.studio)};
    current = next;
    if (changed) { reportRevisions(); }
    saveState({ ...savedState<object>(), strategies: current });
    for (const listener of listeners) { listener(); }
}

function useStored(): Stored {
    const [, render] = useState(0);
    useEffect(() => { const listener = () => render(value => value + 1); listeners.add(listener); return () => { listeners.delete(listener); }; }, []);
    return current;
}

export function useStudio(): [StudioDraft, (update: (draft: StudioDraft) => StudioDraft) => void] {
    return [useStored().studio, updateStudioDraft];
}
export function usePlanner(): [PlannerDraft, (update: (draft: PlannerDraft) => PlannerDraft) => void] {
    return [useStored().planner, update => commit({ ...current, planner: update(current.planner) })];
}
