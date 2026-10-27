// SPDX-License-Identifier: Apache-2.0
import { useEffect, useRef, useState } from 'preact/hooks';
import type { SuggestionCard, SuggestionsView } from '../explorerContracts';
import { appendTasks, type DraftBody, type DraftEvidence, MANAGEMENT_INTENTS } from '../explorerDrafts';
import { parseRoute, type Route } from '../explorerRoutes';
import { request } from './api';
import { ago, shortToken } from './format';
import { dialogFocus } from './components';
import { currentSelection, draftVersions, plannerDraft, studioDraft, studioHasWork, usePlanner, useStudio } from './strategyStore';

interface Detail { id: string; title: string; status: string; client: string; warnings: string[]; targetRevision?: number; evidence?: DraftEvidence; body: DraftBody }
type Choice = { kind: 'strategy' | 'plan'; detail: Detail; reasons: string[]; generation: string };
const STATUS: Record<SuggestionCard['status'], string> = { pending_review: 'Pending review', applied: 'Applied to draft', rejected: 'Rejected', deferred: 'Deferred', expired: 'Expired', conflicted: 'Conflicted' };
const message = (failure: unknown) => failure instanceof Error ? failure.message : String(failure);

/** The authoritative admission must succeed before a local draft is touched. */
export async function applyReviewedSuggestion(expected: string, generation: () => string, authorize: () => Promise<void>, update: () => void, conflict: () => Promise<void>): Promise<void> {
    if (generation() !== expected) { throw new Error('The target draft changed; review the suggestion again.'); }
    await authorize();
    if (generation() !== expected) { await conflict(); throw new Error('The target draft changed while applying; it was kept. Review a new proposal.'); }
    update();
}

export interface InboxProps {
    view?: SuggestionsView;
    close(): void;
    /** Fill the Search form from a draft without running it. */
    applySearch(search: Extract<DraftBody, { kind: 'search' }>['search']): void;
    goStudio(): void;
    goPlanner(): void;
    /** Open a screen for review or management guidance; nothing on it runs by itself. */
    go(route: Route): void;
    applyNavigation(body: Extract<DraftBody, { kind: 'navigation' }>): void;
}

export function Inbox(props: InboxProps) {
    const [studio, setStudio] = useStudio();
    const [planner, setPlanner] = usePlanner();
    const [details, setDetails] = useState<Record<string, Detail>>({});
    const [choice, setChoice] = useState<Choice>();
    const [error, setError] = useState<string>();
    const [confirmDiscard, setConfirmDiscard] = useState(false);
    const root = useRef<HTMLElement>(null);
    useEffect(() => dialogFocus(root.current?.querySelector<HTMLElement>('[role="alertdialog"]') ?? null), [choice, confirmDiscard]);
    const view = props.view;

    const review = async (id: string) => {
        setError(undefined);
        try { const detail = await request<Detail>('suggestions.get', { id }); setDetails(prior => ({ ...prior, [id]: detail })); return detail; }
        catch (failure) { setError(message(failure)); return undefined; }
    };
    const resolve = async (id: string, status: 'applied' | 'rejected' | 'deferred') => {
        try { await request('suggestions.resolve', { id, status }); } catch (failure) { setError(message(failure)); }
    };
    const generation = () => JSON.stringify([currentSelection(), draftVersions()]);
    const finish = async (detail: Detail, update: () => void, go: () => void, expected = generation()) => {
        try {
            const versions = draftVersions();
            await applyReviewedSuggestion(expected, generation, async () => {
                await request('suggestions.resolve', { id: detail.id, status: 'applied',
                    ...(detail.body.kind === 'strategy' ? { targetRevision: versions.studio } : detail.body.kind === 'plan' ? { targetRevision: versions.planner } : {}) });
            }, update, async () => { await request('suggestions.resolve', { id: detail.id, status: 'conflicted' }); });
            if (detail.body.kind === 'strategy' || detail.body.kind === 'plan') {
                const versions = draftVersions();
                await request('suggestions.applied', { id: detail.id, revision: detail.body.kind === 'strategy' ? versions.studio : versions.planner });
            }
            setChoice(undefined); go();
        } catch (failure) { setError(message(failure)); }
    };

    /** Host acceptance precedes every local mutation; stale review dialogs keep the target intact. */
    const apply = async (id: string) => {
        const inspectedGeneration = generation();
        const detail = await review(id);
        if (!detail) { return; }
        if (generation() !== inspectedGeneration) { setError('The target draft changed while loading this suggestion; review it again.'); return; }
        const body = detail.body;
        if (body.kind === 'search') { await finish(detail, () => props.applySearch(body.search), () => undefined); return; }
        if (body.kind === 'navigation') { await finish(detail, () => props.applyNavigation(body), () => undefined); return; }
        if (body.kind === 'review' || body.kind === 'management') {
            const route = parseRoute(body.kind === 'review' ? `#review/${body.scope}` : `#${MANAGEMENT_INTENTS[body.intent].route}`);
            await finish(detail, () => undefined, () => { if (route) { props.go(route); } }); return;
        }
        const currentStudio = studioDraft(), currentPlanner = plannerDraft();
        const current = body.kind === 'strategy' ? currentStudio.revision ?? 0 : currentPlanner.revision;
        const reasons = [...(detail.targetRevision !== undefined && detail.targetRevision !== current ? ['The target draft changed since this suggestion was made.'] : []),
            ...(body.kind === 'strategy' && studioHasWork(currentStudio) ? ['Studio has unsaved work.'] : []), ...(body.kind === 'plan' && currentPlanner.plan ? ['The Planner already has a plan.'] : [])];
        if (reasons.length) { setChoice({ kind: body.kind, detail, reasons, generation: generation() }); return; }
        await finish(detail, () => {
            if (body.kind === 'strategy') { setStudio(prior => ({ ...prior, strategy: body.strategy, origin: undefined, saved: undefined, saveName: undefined, params: { ...body.params }, pathPrefix: body.pathPrefix ?? '', objective: body.objective, run: undefined, prompt: undefined, promptEdited: false })); }
            else { setPlanner(prior => ({ ...prior, plan: body.plan, saved: undefined, revision: prior.revision + 1 })); }
        }, body.kind === 'strategy' ? props.goStudio : props.goPlanner);
    };

    const differences = (detail: Detail): [string, string, string][] => {
        const body = detail.body;
        if (body.kind === 'strategy') {
            const mine = studio.strategy;
            return [['Strategy', mine ? `${mine.title} (${mine.steps.length} steps)` : 'none', `${body.strategy.title} (${body.strategy.steps.length} steps)`],
                ['Objective', studio.objective || '(empty)', body.objective || '(empty)'],
                ['Parameters', JSON.stringify(studio.params), JSON.stringify(body.params)], ['Path scope', studio.pathPrefix || '(all)', body.pathPrefix ?? '(all)']];
        }
        if (body.kind === 'plan') { return [['Plan', planner.plan ? `${planner.plan.title} (${planner.plan.tasks.length} tasks)` : 'none', `${body.plan.title} (${body.plan.tasks.length} tasks)`]]; }
        return [];
    };

    return (
        <aside ref={root} class="inbox card" aria-label="Agent suggestions">
            <header class="inbox-head">
                <h2>Agent suggestions</h2>
                <span class="spacer" />
                <button class="ghost" disabled={!view?.open} onClick={() => setConfirmDiscard(true)}>Discard all</button>
                <button class="ghost" onClick={props.close} aria-label="Close agent suggestions">Close</button>
            </header>
            <p class="muted">Drafts proposed by coding agents through Explorer Assistance. They are kept in memory for this window only. Applying copies a draft into the Explorer; nothing runs or saves until you do it.</p>
            {confirmDiscard && (
                <div class="callout" role="alertdialog" aria-label="Discard all suggestions">
                    <p>Reject every open suggestion for this window? Agents see them as rejected.</p>
                    <div class="actions"><button onClick={async () => { setConfirmDiscard(false); try { await request('suggestions.discardAll', {}); } catch (failure) { setError(message(failure)); } }}>Discard all</button>
                        <button class="ghost" onClick={() => setConfirmDiscard(false)}>Keep them</button></div>
                </div>
            )}
            {error && <p class="callout" role="alert">{error}</p>}
            {choice && (
                <div class="callout" role="alertdialog" aria-label="Apply suggestion">
                    {choice.reasons.map(reason => <p key={reason}>{reason}</p>)}
                    <div class="table-wrap"><table class="grid"><thead><tr><th>Field</th><th>Current draft</th><th>Suggestion</th></tr></thead>
                        <tbody>{differences(choice.detail).map(([field, mine, theirs]) => <tr key={field} class={mine !== theirs ? 'selected' : ''}><td>{field}</td><td>{mine}</td><td>{theirs}</td></tr>)}</tbody></table></div>
                    <div class="actions">
                        {choice.detail.body.kind === 'strategy' ? (() => { const body = choice.detail.body; return <>
                            <button onClick={() => { void finish(choice.detail, () => setStudio(prior => ({ ...prior, strategy: body.strategy, origin: undefined, saved: undefined, saveName: undefined, params: { ...body.params }, pathPrefix: body.pathPrefix ?? '', objective: body.objective, run: undefined, prompt: undefined, promptEdited: false })), props.goStudio, choice.generation); }}>Replace Studio draft</button>
                            <button class="ghost" onClick={() => { void finish(choice.detail, () => setStudio(prior => ({ ...prior, strategy: body.strategy, origin: undefined, saved: undefined, saveName: undefined, params: Object.fromEntries(body.strategy.params.map(param => [param.name, prior.params[param.name] || body.params[param.name] || ''])), run: undefined, prompt: undefined, promptEdited: false })), props.goStudio, choice.generation); }}>Merge: take the steps, keep my objective and values</button>
                        </>; })() : choice.detail.body.kind === 'plan' ? (() => { const body = choice.detail.body; return <>
                            <button disabled={!!planner.plan && !appendTasks(planner.plan, body.plan.tasks)} title={planner.plan && !appendTasks(planner.plan, body.plan.tasks) ? 'A plan holds at most 40 tasks' : undefined}
                                onClick={() => { void finish(choice.detail, () => setPlanner(prior => ({ ...prior, plan: prior.plan ? appendTasks(prior.plan, body.plan.tasks) ?? prior.plan : body.plan, revision: prior.revision + 1 })), props.goPlanner, choice.generation); }}>Append the suggested tasks</button>
                            <button class="ghost" onClick={() => { void finish(choice.detail, () => setPlanner(prior => ({ ...prior, plan: body.plan, revision: prior.revision + 1 })), props.goPlanner, choice.generation); }}>Replace the plan</button>
                        </>; })() : null}
                        <button class="ghost" onClick={() => setChoice(undefined)}>Cancel</button>
                    </div>
                </div>
            )}
            {!view || !view.cards.length ? <p class="muted">No agent suggestions.</p> : (
                <ul class="inbox-cards">
                    {view.cards.map(card => {
                        const open = card.status === 'pending_review' || card.status === 'deferred';
                        const detail = details[card.id];
                        return (
                            <li key={card.id} class={`card suggestion ${open ? '' : 'resolved'}`}>
                                <div class="hit-head">
                                    <strong>{card.title}</strong>
                                    <span class="tag">{STATUS[card.status]}</span>
                                    {card.duplicates > 1 && <span class="tag" title="Drafts with the same destination and title">×{card.duplicates}</span>}
                                </div>
                                <p class="muted">From <strong>{card.client}</strong> (self-reported) · {ago(card.receivedAt)} · to {card.destination} · {card.summary}</p>
                                {card.warnings.map(warning => <p key={warning} class="callout">{warning}</p>)}
                                {detail && <Review detail={detail} />}
                                {open && (
                                    <div class="actions">
                                        {!detail && <button class="ghost" onClick={() => void review(card.id)}>Review</button>}
                                        <button onClick={() => void apply(card.id)}>{card.kind === 'review' || card.kind === 'management' || card.kind === 'navigation' ? 'Open screen' : 'Apply to draft'}</button>
                                        {card.status !== 'deferred' && <button class="ghost" onClick={() => void resolve(card.id, 'deferred')}>Defer</button>}
                                        <button class="ghost" onClick={() => void resolve(card.id, 'rejected')}>Reject</button>
                                    </div>
                                )}
                            </li>
                        );
                    })}
                </ul>
            )}
            {view && view.hidden > 0 && <p class="muted">{view.hidden} more suggestion(s) are not shown. Review or discard the ones above first.</p>}
        </aside>
    );
}

/** The draft body as plain text; agent text is never rendered as HTML or treated as instructions. */
function Review(props: { detail: Detail }) {
    const { body, evidence } = props.detail;
    return (
        <div class="review">
            {body.kind === 'search' && <p>Search <code>{body.search.query}</code> · {body.search.mode} · scope {body.search.pathPrefix ?? '(all)'} · limit {body.search.limit}{body.intent ? <><br />Intent: {body.intent}</> : null}</p>}
            {body.kind === 'strategy' && <>
                <p>{body.strategy.title} <code>{body.strategy.id}</code>{body.objective ? <> · {body.objective}</> : null}</p>
                <ol class="steps">{body.strategy.steps.map(step => <li key={step.id}><code>{step.op}</code> {step.id}{step.input ? ` ← ${step.input}` : ''}</li>)}</ol>
            </>}
            {body.kind === 'review' && <p>Open Review › {body.scope}{body.names.length ? <> and look at <code>{body.names.join(', ')}</code></> : null}.{body.note ? <><br />Agent note: {body.note}</> : null}</p>}
            {body.kind === 'navigation' && <><p>Open <code>{body.route}</code> and fill these fields for review. Submit the form yourself to run its query.</p>
                <pre>{JSON.stringify(body.filters, null, 2)}</pre>{body.note && <p>{body.note}</p>}</>}
            {body.kind === 'management' && <>
                <p>{MANAGEMENT_INTENTS[body.intent].guidance}</p>
                {body.reason && <p class="muted">Agent's reason: {body.reason}</p>}
                <p class="muted">Opening the screen runs nothing; every action there asks for your confirmation.</p>
            </>}
            {body.kind === 'plan' && <ol class="steps">{body.plan.tasks.map(task => <li key={task.id}>{task.id}. {task.title}{task.dependsOn.length ? ` (after ${task.dependsOn.join(', ')})` : ''}</li>)}</ol>}
            {evidence && <p class="muted">Cites {evidence.citations.length} location(s) at publication <code title={evidence.publication}>{shortToken(evidence.publication)}</code>. Citations are checked when you run or inspect them, not trusted.</p>}
        </div>
    );
}
