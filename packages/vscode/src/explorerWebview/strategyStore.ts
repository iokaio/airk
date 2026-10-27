// SPDX-License-Identifier: Apache-2.0
// Studio and Planner drafts shared by the Strategies tabs. They live in webview state, so
// switching tabs or hiding the panel keeps them; nothing is written to the workspace from here.
import { useEffect, useState } from 'preact/hooks';
import type { Plan, PromptProfile, RunResult, Strategy } from '../explorerStrategies';
import { savedState, saveState } from './api';

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
}
export interface PlannerDraft {
    readonly plan?: Plan;
    /** Increments on every edit so a late save result for an older revision is ignored. */
    readonly revision: number;
    readonly saved?: { readonly path: string; readonly revision: number; readonly hash: string };
}
interface Stored { studio: StudioDraft; planner: PlannerDraft }

const EMPTY: Stored = { studio: { params: {}, pathPrefix: '', objective: '', profile: 'portable', promptEdited: false }, planner: { revision: 0 } };
let current: Stored = { ...EMPTY, ...(savedState<{ strategies?: Stored }>()?.strategies ?? {}) };
const listeners = new Set<() => void>();

function commit(next: Stored): void {
    current = next;
    saveState({ ...savedState<object>(), strategies: current });
    for (const listener of listeners) { listener(); }
}

function useStored(): Stored {
    const [, render] = useState(0);
    useEffect(() => { const listener = () => render(value => value + 1); listeners.add(listener); return () => { listeners.delete(listener); }; }, []);
    return current;
}

export function useStudio(): [StudioDraft, (update: (draft: StudioDraft) => StudioDraft) => void] {
    return [useStored().studio, update => commit({ ...current, studio: update(current.studio) })];
}
export function usePlanner(): [PlannerDraft, (update: (draft: PlannerDraft) => PlannerDraft) => void] {
    return [useStored().planner, update => commit({ ...current, planner: update(current.planner) })];
}

/** True when loading something else would discard the user's Studio work. */
export function studioHasWork(draft: StudioDraft): boolean {
    return !!draft.strategy && (!!draft.run || draft.promptEdited || !!draft.objective.trim() || Object.values(draft.params).some(value => value.trim())
        || !!draft.strategy.id.startsWith('user/'));
}
