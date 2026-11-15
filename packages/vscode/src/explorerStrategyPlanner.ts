// SPDX-License-Identifier: Apache-2.0
// One snapshot/prompt algorithm shared by Planner shells.
import {compilePrompt,type Plan,type PlanTask} from './explorerStrategies';
import type {StudioDraft} from './explorerStrategyDrafts';
export const nextId = (plan: Plan) => `T${Math.max(0, ...plan.tasks.map(task => Number(task.id.slice(1)))) + 1}`;
/** A Studio snapshot as a task: its compiled prompt becomes the instructions and its run the evidence. */
export function taskFromStudio(plan: Plan, studio: StudioDraft): PlanTask {
    const run = studio.run?.manifest;
    const prompt = studioPrompt(studio);
    if (studio.promptStale || prompt.length > 8000) { throw new Error('Studio snapshot is stale or exceeds 8,000 characters. Rebuild or explicitly reduce its evidence before adding it to Planner.'); }
    return { id: nextId(plan), title: studio.strategy!.title.slice(0, 160), instructions: prompt || studio.objective, dependsOn: [], state: 'draft',
        ...(run && run.publication !== 'unknown' ? { evidence: { template: run.template, publication: run.publication, digest: run.evidenceDigest, outcome: run.outcome } } : {}) };
}
export const studioPrompt = (studio: StudioDraft) => studio.prompt ?? (studio.strategy ? compilePrompt({ strategy: studio.strategy, objective: studio.objective, profile: studio.profile, run: studio.run }) : '');
export const canAddStudio = (studio: StudioDraft) => !!studio.strategy && !studio.promptStale && studioPrompt(studio).length <= 8000;

