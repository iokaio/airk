// SPDX-License-Identifier: Apache-2.0
import { useMemo, useState } from 'preact/hooks';
import type { CitationCheckView } from '../explorerContracts';
import { effectiveState, handoffPrompt, Plan, PLAN_SCHEMA, PlanTask, renderPlan, StoredTaskState, TASK_STATES, TaskState, validatePlan } from '../explorerStrategies';
import { request } from './api';
import { PlannerDraft, StudioDraft, usePlanner, useStudio } from './strategyStore';
import type { ScreenProps } from './workspaces';

const message = (failure: unknown) => failure instanceof Error ? failure.message : String(failure);
const LABELS: Record<TaskState, string> = { draft: 'Draft', ready: 'Ready', exported: 'Exported', review: 'Review', needs_work: 'Needs work', completed: 'Completed', blocked: 'Blocked', stale: 'Stale' };
type SaveReply = { saved: true; path: string; hash: string; revision: number } | { saved: false; reason: string; revision: number };

const emptyPlan = (objective: string, project?: string): Plan => ({ schema: PLAN_SCHEMA, title: 'New plan', objective, ...(project ? { project } : {}), tasks: [] });
const nextId = (plan: Plan) => `T${Math.max(0, ...plan.tasks.map(task => Number(task.id.slice(1)))) + 1}`;
const fileName = (title: string) => `${title.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 60) || 'plan'}.md`;

/** A Studio snapshot as a task: its compiled prompt becomes the instructions and its run the evidence. */
export function taskFromStudio(plan: Plan, studio: StudioDraft): PlanTask {
    const run = studio.run?.manifest;
    const prompt = (studio.prompt ?? '').slice(0, 8000);
    return { id: nextId(plan), title: studio.strategy!.title.slice(0, 160), instructions: prompt || studio.objective, dependsOn: [], state: 'draft',
        ...(run && run.publication !== 'unknown' ? { evidence: { template: run.template, publication: run.publication, digest: run.evidenceDigest, outcome: run.outcome } } : {}) };
}

export function PlannerScreen(props: ScreenProps) {
    const [draft, setDraft] = usePlanner();
    const [studio] = useStudio();
    const [error, setError] = useState<string>();
    const [notice, setNotice] = useState<string>();
    const [busy, setBusy] = useState(false);
    const [handoffTask, setHandoffTask] = useState<string>();
    const [confirmComplete, setConfirmComplete] = useState<string>();
    const [confirmDiscard, setConfirmDiscard] = useState(false);
    const [report, setReport] = useState('');
    const [check, setCheck] = useState<CitationCheckView>();
    const plan = draft.plan;
    const publication = props.state.status?.publication;
    const checked = useMemo(() => plan ? validatePlan(plan) : undefined, [plan]);
    const markdown = useMemo(() => plan && checked?.ok ? renderPlan(plan, { workspace: 'workspace', extensionVersion: props.state.extensionVersion, ...(publication ? { currentPublication: publication } : {}) }) : '', [plan, checked, publication]);
    const saved = draft.saved && draft.saved.revision === draft.revision ? draft.saved : undefined;

    const edit = (update: (plan: Plan) => Plan) => setDraft(prior => ({ ...prior, plan: update(prior.plan!), revision: prior.revision + 1 }));
    const editTask = (id: string, update: (task: PlanTask) => PlanTask) => edit(current => ({ ...current, tasks: current.tasks.map(task => task.id === id ? update(task) : task) }));
    const setState = (id: string, state: StoredTaskState) => editTask(id, task => ({ ...task, state }));

    if (!plan) {
        return (
            <div class="strategies">
                <article class="card">
                    <h2>No plan yet</h2>
                    <p>A plan combines Studio snapshots and manual tasks into one Markdown file that a coding agent can follow, one task at a time.</p>
                    <div class="actions">
                        <button onClick={() => setDraft(prior => ({ ...prior, plan: emptyPlan(studio.objective, props.state.selectedProject), revision: prior.revision + 1 }))}>New plan</button>
                        {studio.strategy && <button class="ghost" onClick={() => setDraft(prior => {
                            const base = emptyPlan(studio.objective, props.state.selectedProject);
                            return { ...prior, plan: { ...base, title: studio.strategy!.title, tasks: [taskFromStudio(base, studio)] }, revision: prior.revision + 1 };
                        })}>Start from the Studio strategy</button>}
                    </div>
                </article>
            </div>
        );
    }

    const save = async () => {
        setBusy(true); setError(undefined); setNotice(undefined);
        const revision = draft.revision;
        try {
            const reply = await request<SaveReply>('planner.saveInWorkspace', { plan: plan as unknown as Record<string, unknown>, revision, fileName: fileName(plan.title) });
            if (reply.revision !== revision) { return; }
            if (reply.saved) {
                setDraft(prior => prior.revision === reply.revision ? { ...prior, saved: { path: reply.path, revision: reply.revision, hash: reply.hash } } : prior);
                setNotice(`Saved ${reply.path}. The handoff prompt is ready below.`);
            } else { setNotice(reply.reason); }
        } catch (failure) { setError(message(failure)); } finally { setBusy(false); }
    };
    const copyHandoff = async (id: string) => {
        try {
            await request('clipboard.write', { text: handoffPrompt(saved!.path, plan, id) });
            const task = plan.tasks.find(entry => entry.id === id);
            // Copying hands the task off; it never completes it.
            if (task && (task.state === 'draft' || task.state === 'ready')) {
                setDraft(prior => ({ ...prior, plan: { ...prior.plan!, tasks: prior.plan!.tasks.map(entry => entry.id === id ? { ...entry, state: 'exported' } : entry) } }));
            }
            setNotice(`Handoff prompt for ${id} copied. The saved file is unchanged until you save again.`);
        } catch (failure) { setError(message(failure)); }
    };
    const runCheck = async () => {
        setCheck(undefined); setError(undefined);
        try { setCheck(await request<CitationCheckView>('evidence.check', { text: report })); } catch (failure) { setError(message(failure)); }
    };
    const counts = check ? check.citations.reduce<Record<string, number>>((all, item) => ({ ...all, [item.status]: (all[item.status] ?? 0) + 1 }), {}) : {};

    return (
        <div class="strategies planner">
            <section class="card">
                <label>Title<input value={plan.title} maxLength={160} onInput={event => edit(current => ({ ...current, title: (event.target as HTMLInputElement).value }))} /></label>
                <label>Objective<textarea rows={3} maxLength={4000} value={plan.objective} onInput={event => edit(current => ({ ...current, objective: (event.target as HTMLTextAreaElement).value }))} /></label>
                <div class="actions">
                    <button class="ghost" disabled={plan.tasks.length >= 40} onClick={() => edit(current => ({ ...current, tasks: [...current.tasks, { id: nextId(current), title: 'New task', instructions: '', dependsOn: [], state: 'draft' }] }))}>Add task</button>
                    <button class="ghost" disabled={!studio.strategy || plan.tasks.length >= 40} title={studio.strategy ? `Add ${studio.strategy.title}` : 'Load a strategy in Studio first'}
                        onClick={() => edit(current => ({ ...current, tasks: [...current.tasks, taskFromStudio(current, studio)] }))}>Add Studio snapshot</button>
                    <button class="ghost" onClick={() => setConfirmDiscard(true)}>Discard plan</button>
                </div>
                {confirmDiscard && (
                    <div class="callout" role="alertdialog" aria-label="Discard plan">
                        <p>Discard this plan from the Explorer? A file you already saved is not changed.</p>
                        <div class="actions"><button onClick={() => { setDraft(prior => ({ revision: prior.revision + 1 })); setCheck(undefined); setConfirmDiscard(false); }}>Discard plan</button>
                            <button class="ghost" onClick={() => setConfirmDiscard(false)}>Keep plan</button></div>
                    </div>
                )}
            </section>
            {plan.tasks.map((task, index) => {
                const state = effectiveState(plan, task, publication);
                return (
                    <section class="card task" key={task.id}>
                        <h3>{task.id}. <input class="inline" value={task.title} maxLength={160} aria-label={`${task.id} title`} onInput={event => editTask(task.id, entry => ({ ...entry, title: (event.target as HTMLInputElement).value }))} />
                            <span class={state === 'completed' ? 'tag ok' : 'tag'}>{LABELS[state]}</span></h3>
                        {task.evidence && <p class="muted">Evidence: {task.evidence.template}, {task.evidence.outcome === 'verified_snapshot' ? 'verified snapshot' : 'partial'}{state === 'stale' ? ' — stale: the index was republished since this run' : ''}</p>}
                        <label>Instructions<textarea rows={5} maxLength={8000} value={task.instructions} onInput={event => editTask(task.id, entry => ({ ...entry, instructions: (event.target as HTMLTextAreaElement).value }))} /></label>
                        <fieldset class="deps"><legend>Depends on</legend>
                            {plan.tasks.filter(other => other.id !== task.id).map(other => (
                                <label class="check" key={other.id}><input type="checkbox" checked={task.dependsOn.includes(other.id)}
                                    onChange={event => editTask(task.id, entry => ({ ...entry, dependsOn: (event.target as HTMLInputElement).checked ? [...entry.dependsOn, other.id].sort() : entry.dependsOn.filter(dep => dep !== other.id) }))} /> {other.id}</label>
                            ))}
                            {plan.tasks.length === 1 && <span class="muted">No other tasks.</span>}
                        </fieldset>
                        <div class="search-form">
                            <label>Status
                                <select value={task.state} onChange={event => {
                                    const next = (event.target as HTMLSelectElement).value as StoredTaskState;
                                    if (next === 'completed') { setConfirmComplete(task.id); } else { setState(task.id, next); }
                                }}>
                                    {TASK_STATES.map(value => <option key={value} value={value}>{LABELS[value]}</option>)}
                                </select>
                            </label>
                            {state === 'blocked' && <span class="muted">Blocked until {task.dependsOn.filter(dep => plan.tasks.find(entry => entry.id === dep)?.state !== 'completed').join(', ')} {task.dependsOn.length > 1 ? 'are' : 'is'} completed.</span>}
                        </div>
                        {confirmComplete === task.id && (
                            <div class="callout" role="alertdialog" aria-label={`Complete ${task.id}`}>
                                <p>Mark {task.id} completed? Check the agent's report against current files first (Completion check below).</p>
                                <div class="actions"><button onClick={() => { setState(task.id, 'completed'); setConfirmComplete(undefined); }}>Mark completed</button><button class="ghost" onClick={() => setConfirmComplete(undefined)}>Cancel</button></div>
                            </div>
                        )}
                        <div class="actions">
                            <button class="ghost" disabled={index === 0} onClick={() => edit(current => { const tasks = [...current.tasks]; tasks.splice(index - 1, 0, tasks.splice(index, 1)[0]); return { ...current, tasks }; })} aria-label={`Move ${task.id} up`}>↑</button>
                            <button class="ghost" disabled={index === plan.tasks.length - 1} onClick={() => edit(current => { const tasks = [...current.tasks]; tasks.splice(index + 1, 0, tasks.splice(index, 1)[0]); return { ...current, tasks }; })} aria-label={`Move ${task.id} down`}>↓</button>
                            <button class="ghost" onClick={() => edit(current => ({ ...current, tasks: current.tasks.filter(entry => entry.id !== task.id).map(entry => ({ ...entry, dependsOn: entry.dependsOn.filter(dep => dep !== task.id) })) }))}>Remove</button>
                        </div>
                    </section>
                );
            })}
            {checked && !checked.ok && <ul class="callout" role="alert">{checked.errors.map(item => <li key={item}>{item}</li>)}</ul>}
            <section class="card">
                <h3>Save</h3>
                <p class="muted">Save in workspace opens the save dialog in the <code>airk.explorer.planner.directory</code> folder (default <code>.airk/plans</code>). The handoff prompt is offered only after the saved file is confirmed.</p>
                <div class="actions">
                    <button disabled={busy || !checked?.ok || !plan.tasks.length} onClick={() => void save()}>{busy ? 'Saving…' : 'Save in workspace…'}</button>
                    <button class="ghost" disabled={!checked?.ok} onClick={async () => { try { await request('planner.openDraft', { plan: plan as unknown as Record<string, unknown> }); setNotice('Opened as an untitled draft; it is not saved.'); } catch (failure) { setError(message(failure)); } }}>Open as draft</button>
                </div>
                {saved ? <p class="muted" role="status">Saved as <code>{saved.path}</code> (sha256 {saved.hash.slice(0, 12)}).</p>
                    : draft.saved ? <p class="muted">The plan changed since it was saved as <code>{draft.saved.path}</code>; save again before handing it off.</p> : null}
                {notice && <p class="muted" role="status">{notice}</p>}
                {error && <p class="callout" role="alert">{error}</p>}
                <details><summary>Markdown preview</summary><pre class="log">{markdown}</pre></details>
            </section>
            {saved && (
                <section class="card">
                    <h3>Handoff</h3>
                    <div class="search-form">
                        <label>Task
                            <select value={handoffTask ?? ''} onChange={event => setHandoffTask((event.target as HTMLSelectElement).value || undefined)}>
                                <option value="">Choose a task</option>
                                {plan.tasks.map(task => <option key={task.id} value={task.id} disabled={effectiveState(plan, task, publication) === 'blocked'}>{task.id}. {task.title}</option>)}
                            </select>
                        </label>
                    </div>
                    {handoffTask && <><pre class="log">{handoffPrompt(saved.path, plan, handoffTask)}</pre><button onClick={() => void copyHandoff(handoffTask)}>Copy handoff prompt</button></>}
                </section>
            )}
            <section class="card">
                <h3>Completion check</h3>
                <p class="muted">Paste the agent's report. Its citations are checked against the index and the saved files; then set the task to Review, Needs work or Completed.</p>
                <textarea rows={5} maxLength={65536} value={report} aria-label="Agent report" onInput={event => setReport((event.target as HTMLTextAreaElement).value)} />
                <div class="actions"><button class="ghost" disabled={!report.trim()} onClick={() => void runCheck()}>Check citations</button></div>
                {check && (check.supported
                    ? <p role="status">{check.citations.length} citation(s): {Object.entries(counts).map(([status, count]) => `${count} ${status.replaceAll('_', ' ')}`).join(', ')}{check.limited ? ' (more were not checked)' : ''}.</p>
                    : <p class="callout" role="status">The report has no supported citations (path:line). Prose alone is never a pass.</p>)}
            </section>
        </div>
    );
}
