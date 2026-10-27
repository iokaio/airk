// SPDX-License-Identifier: Apache-2.0
import { useEffect, useMemo, useRef, useState } from 'preact/hooks';
import {
    CATEGORIES, CatalogEntry, compilePrompt, CORE_PACK, estimateTokens, operator, OPERATORS, OperatorId, PromptProfile, RunResult, Strategy, StrategyStep,
    PLAN_SCHEMA, STUDIO_SCHEMA, StudioDocument, unavailableReason, validateStrategy,
} from '../explorerStrategies';
import { request } from './api';
import { shortToken } from './format';
import { canAddStudio, taskFromStudio } from './planner';
import { acceptStudioRun, currentSelection, studioDraft, studioDocument, StudioDraft, studioHasWork, usePlanner, useStudio } from './strategyStore';
import type { ScreenProps } from './workspaces';
import { dialogFocus, ShareSnapshot } from './components';
import { FileBrowser, type LibraryFile } from './fileBrowser';

const message = (failure: unknown) => failure instanceof Error ? failure.message : String(failure);
const STUDIO = { workspace: 'strategies', tab: 'studio' } as const;

/** A fresh draft for a strategy; the objective is kept because the user wrote it. */
function draftFor(strategy: Strategy, prior: StudioDraft, origin?: string): StudioDraft {
    return { strategy, ...(origin ? { origin } : {}), params: Object.fromEntries(strategy.params.map(param => [param.name, param.default ?? ''])),
        pathPrefix: '', objective: prior.objective, profile: prior.profile, promptEdited: false };
}

export function LibraryScreen(props: ScreenProps) {
    const [studio, setStudio] = useStudio();
    const [category, setCategory] = useState<string>('all');
    const [filter, setFilter] = useState('');
    const [pending, setPending] = useState<CatalogEntry>();
    const [notice, setNotice] = useState<string>();
    const pendingGeneration = useRef<string>();
    const root = useRef<HTMLDivElement>(null);
    useEffect(() => dialogFocus(root.current?.querySelector<HTMLElement>('[role="alertdialog"]') ?? null, () => setPending(undefined)), [pending]);
    const shown = CORE_PACK.filter(entry => (category === 'all' || entry.strategy.category === category)
        && `${entry.strategy.title} ${entry.strategy.summary}`.toLowerCase().includes(filter.trim().toLowerCase()));
    const load = (entry: CatalogEntry, confirmed = false) => {
        const generation = JSON.stringify([currentSelection(), studioDraft().revision]);
        if (!confirmed && studioHasWork(studio)) { pendingGeneration.current = generation; setPending(entry); return; }
        if (confirmed && pendingGeneration.current !== generation) { setPending(undefined); setNotice('Studio changed while the confirmation was open; your work was kept. Choose the template again.'); return; }
        setPending(undefined);
        setStudio(prior => draftFor(entry.strategy, prior, entry.strategy.id));
        props.go(STUDIO);
    };
    return (
        <div class="strategies" ref={root}>
            {notice && <p class="callout" role="status">{notice}</p>}
            <p class="muted">25 templates in the AIRK core pack. A strategy gathers bounded, cited evidence and compiles a prompt; the Explorer never runs an agent.</p>
            <div class="search-form">
                <label>Category
                    <select value={category} onChange={event => setCategory((event.target as HTMLSelectElement).value)}>
                        <option value="all">All categories</option>{CATEGORIES.map(name => <option key={name} value={name}>{name}</option>)}
                    </select>
                </label>
                <label class="grow">Filter<input value={filter} maxLength={120} onInput={event => setFilter((event.target as HTMLInputElement).value)} /></label>
            </div>
            {pending && (
                <div class="callout" role="alertdialog" aria-label="Replace Studio work">
                    <p>Studio has unsaved work{studio.run ? ' and a run' : ''}. Loading <strong>{pending.strategy.title}</strong> replaces it.</p>
                    <div class="actions"><button onClick={() => load(pending, true)}>Replace Studio work</button><button class="ghost" onClick={() => setPending(undefined)}>Keep Studio work</button></div>
                </div>
            )}
            <div class="cards two">
                {shown.map(entry => (
                    <article class="card" key={entry.strategy.id}>
                        <h2>{entry.strategy.title} <span class={entry.runnable ? 'tag ok' : 'tag'}>{entry.runnable ? 'Runnable' : 'Preview'}</span></h2>
                        <p class="muted"><code>{entry.strategy.id}@{entry.strategy.version}</code> · {entry.strategy.category}</p>
                        <p>{entry.strategy.summary}</p>
                        {entry.reason && <p class="muted">{entry.reason}</p>}
                        <ol class="steps">{entry.strategy.steps.map(step => <li key={step.id}>{operator(step.op)?.title ?? step.op} <code>{step.op}</code></li>)}</ol>
                        <div class="actions"><button onClick={() => load(entry)}>{entry.runnable ? 'Open in Studio' : 'Preview in Studio'}</button></div>
                    </article>
                ))}
            </div>
            {!shown.length && <p class="muted">No template matches this filter.</p>}
        </div>
    );
}

/** Default arguments when a step is added by hand; required text arguments start bound to a parameter. */
function newStep(op: OperatorId, steps: readonly StrategyStep[]): StrategyStep {
    const spec = operator(op)!;
    let index = steps.length + 1;
    while (steps.some(step => step.id === `${op.split('.')[1].toLowerCase()}-${index}`)) { index++; }
    const args: Record<string, string | number> = {};
    for (const key of spec.required ?? []) { args[key] = key === 'name' ? '${params.symbol}' : key === 'query' ? '${params.query}' : ''; }
    if ('path_prefix' in spec.args) { args.path_prefix = '${scope.pathPrefix}'; }
    const input = spec.input ? [...steps].reverse().find(step => step.op !== 'airk.status' && operator(step.op)?.stage === 'retrieval')?.id : undefined;
    return { id: `${op.split('.')[1].toLowerCase()}-${index}`, op, args, ...(input ? { input } : {}) };
}

/** Editing a core template forks it; adding, removing or reordering steps clears the run. */
function fork(strategy: Strategy, steps: StrategyStep[]): Strategy {
    const id = strategy.id.startsWith('user/') ? strategy.id : `user/${strategy.id.split('/')[1]}`;
    const used = new Set(steps.flatMap(step => Object.values(step.args)).flatMap(value => typeof value === 'string' ? [...value.matchAll(/\$\{params\.([A-Za-z0-9]+)\}/g)].map(match => match[1]) : []));
    const params = [...strategy.params.filter(param => used.has(param.name))];
    for (const name of used) { if (!params.some(param => param.name === name)) { params.push({ name, label: name === 'symbol' ? 'Symbol or identifier' : name === 'query' ? 'Search words' : name, kind: 'text', required: true }); } }
    return { ...strategy, id, title: strategy.id === id ? strategy.title : `${strategy.title} (fork)`, params, steps };
}

export function StudioScreen(props: ScreenProps) {
    const [draft, setDraft] = useStudio();
    const [, setPlanner] = usePlanner();
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState<string>();
    const [notice, setNotice] = useState<string>();
    const [confirmRebuild, setConfirmRebuild] = useState(false);
    const [pendingImport, setPendingImport] = useState<{ strategy: Strategy; document?: StudioDocument; revision: number | undefined; owner: string | undefined }>();
    const [saving, setSaving] = useState(false);
    const root = useRef<HTMLDivElement>(null);
    useEffect(() => dialogFocus(root.current?.querySelector<HTMLElement>('[role="alertdialog"]') ?? null,
        () => { setPendingImport(undefined); setConfirmRebuild(false); }), [pendingImport, confirmRebuild]);
    const [adding, setAdding] = useState<OperatorId>('airk.search');
    const strategy = draft.strategy;
    const checked = useMemo(() => strategy ? validateStrategy(strategy) : undefined, [strategy]);
    const blocked = strategy ? unavailableReason(strategy) ?? CORE_PACK.find(entry => entry.strategy.id === draft.origin)?.reason : undefined;
    const runnable = !!checked?.ok && !blocked && (props.state.connection === 'ready' || props.state.connection === 'degraded');

    if (!strategy) {
        return (
            <div class="strategies" ref={root}>
                <article class="card">
                    <h2>No strategy loaded</h2>
                    <p>Open a template from the Library, or import a <code>.strategy.airk.json</code> file.</p>
                    <div class="actions">
                        <button onClick={() => props.go({ workspace: 'strategies', tab: 'library' })}>Open the Library</button>
                        <button class="ghost" onClick={() => void importFile()}>Import strategy…</button>
                    </div>
                    {error && <p class="callout" role="alert">{error}</p>}
                </article>
                <FileBrowser kind="strategies" trusted={props.state.trusted} load={importFile} />
            </div>
        );
    }

    async function importFile(file?: LibraryFile) {
        setError(undefined); setConfirmRebuild(false);
        const revision = studioDraft().revision, owner = currentSelection();
        try {
            const imported = file ? await request<Strategy | StudioDocument>('files.load', { kind: 'strategies', ...file }) : await request<Strategy | StudioDocument | null>('strategy.import', {});
            if (!imported) { return; }
            if (studioDraft().revision !== revision || currentSelection() !== owner) { setError('Studio or the project changed while the file picker was open; your work was kept.'); return; }
            const pending = imported.schema === STUDIO_SCHEMA ? { strategy: imported.strategy, document: imported, revision, owner } : { strategy: imported, revision, owner };
            if (studioHasWork(studioDraft())) { setPendingImport(pending); } else { applyImported(pending); }
        } catch (failure) { setError(message(failure)); }
    }
    function applyImported(pending: { strategy: Strategy; document?: StudioDocument; revision: number | undefined; owner: string | undefined }) {
        setPendingImport(undefined);
        if (studioDraft().revision !== pending.revision || currentSelection() !== pending.owner) { setError('Studio changed during review; your work was kept. Import again.'); return; }
        const document = pending.document;
        setDraft(prior => document ? { strategy: document.strategy, params: document.params, objective: document.objective,
            pathPrefix: document.pathPrefix, profile: document.profile, prompt: document.editedPrompt, promptEdited: document.editedPrompt !== undefined,
            promptStale: document.editedPrompt !== undefined, saveName: document.strategy.id.split('/').pop() } : draftFor(pending.strategy, prior));
        setNotice(`Loaded ${pending.strategy.title}. Collect evidence again before relying on indexed citations.`);
    }
    const edit = (steps: StrategyStep[]) => setDraft(prior => ({ ...prior, strategy: fork(prior.strategy!, steps), origin: undefined, run: undefined }));
    const move = (index: number, by: number) => { const steps = [...strategy.steps]; const [step] = steps.splice(index, 1); steps.splice(index + by, 0, step); edit(steps); };
    const setArg = (index: number, key: string, value: string | number) =>
        edit(strategy.steps.map((step, at) => at === index ? { ...step, args: { ...step.args, [key]: value } } : step));
    const compile = (run: RunResult | undefined, profile: PromptProfile) => compilePrompt({ strategy, objective: draft.objective, profile, ...(run ? { run } : {}) });
    const run = async () => {
        const revision = draft.revision, selection = currentSelection();
        setBusy(true); setError(undefined); setNotice(undefined);
        try {
            // Send only the parameters this strategy declares; values left over from removed steps or other drafts are dropped.
            const params = Object.fromEntries(strategy.params.map(param => [param.name, draft.params[param.name] ?? '']));
            const result = await request<RunResult>('strategy.run', { strategy: strategy as unknown as Record<string, unknown>, params,
                scope: draft.pathPrefix.trim() ? { pathPrefix: draft.pathPrefix.trim() } : {} });
            if (!acceptStudioRun(result, revision, selection, compile(result, draft.profile))) { setNotice('The draft or project changed; the old run was discarded.'); return; }
            if (draft.promptEdited) { setNotice('The run finished. Your edited prompt was kept; use Rebuild to include the new evidence.'); }
        } catch (failure) { setError(message(failure)); } finally { setBusy(false); }
    };
    const rebuild = () => { setConfirmRebuild(false); setDraft(prior => ({ ...prior, prompt: compile(prior.run, prior.profile), promptEdited: false, promptStale: false })); };
    const prompt = draft.prompt ?? compile(draft.run, draft.profile);
    const manifest = draft.run?.manifest;
    const saveName = draft.saveName ?? strategy.title.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '').slice(0, 64);
    const savedCurrent = draft.saved?.name === saveName && draft.saved.content === JSON.stringify(studioDocument(draft));
    const save = async () => {
        const owner = currentSelection(), revision = draft.revision;
        const document = studioDocument(draft);
        setSaving(true); setError(undefined); setNotice(undefined);
        try {
            const result = await request<{ saved: boolean; path?: string; hash?: string; reason?: string }>('studio.save', {
                document: document as unknown as Record<string, unknown>, name: saveName, revision: revision ?? 0,
                ...(draft.saved?.name === saveName ? { expectedHash: draft.saved.hash } : {}) });
            if (!result.saved) { setError(result.reason ?? 'The draft was not saved.'); return; }
            if (currentSelection() !== owner || studioDraft().revision !== revision) { setNotice(`Saved ${result.path}; the newer draft has unsaved changes.`); return; }
            const strategy = { ...document.strategy, id: `user/${saveName}` };
            setDraft(prior => ({ ...prior, strategy, saveName, saved: { name: saveName, path: result.path!, hash: result.hash!, content: JSON.stringify({ ...document, strategy }) } }));
            setNotice(`Saved ${result.path}.`);
        } catch (failure) { setError(message(failure)); } finally { setSaving(false); }
    };

    return (
        <div class="strategies studio" ref={root}>
            <details class="library-disclosure"><summary>Browse saved strategies</summary><FileBrowser kind="strategies" trusted={props.state.trusted} load={importFile} /></details>
            {pendingImport && <div class="callout" role="alertdialog" aria-label="Replace Studio with imported file"><p>Replace Studio work with {pendingImport.strategy.title}? The selected file is unchanged.</p>
                <button onClick={() => applyImported(pendingImport)}>Replace Studio draft</button><button class="ghost" onClick={() => setPendingImport(undefined)}>Keep Studio work</button></div>}
            <header class="studio-head">
                <h2>{strategy.title} {draft.origin && <span class={blocked ? 'tag' : 'tag ok'}>{blocked ? 'Preview' : 'Runnable'}</span>}{strategy.id.startsWith('user/') && <span class="tag">fork</span>}</h2>
                <p class="muted"><code>{strategy.id}@{strategy.version}</code> · {strategy.summary}</p>
                <div class="actions">
                    <label>Save name <input aria-label="Studio save name" value={saveName} maxLength={64} onInput={event => setDraft(prior => ({ ...prior, saveName: (event.target as HTMLInputElement).value }))} /></label>
                    <button disabled={saving || !checked?.ok || !props.state.trusted || !props.state.selectedProject} onClick={() => void save()}>{saving ? 'Saving…' : 'Save'}</button>
                    <button class="ghost" onClick={() => void importFile()}>Import…</button>
                    <button class="ghost" disabled={!checked?.ok} onClick={async () => {
                        try { const saved = await request<{ saved: boolean; name?: string }>('strategy.export', { strategy: strategy as unknown as Record<string, unknown> }); if (saved.saved) { setNotice(`Exported ${saved.name}.`); } }
                        catch (failure) { setError(message(failure)); }
                    }}>Export…</button>
                </div>
                <p class="muted" role="status">{savedCurrent ? `Saved: ${draft.saved!.path}` : 'Unsaved changes'} · Saves to this project’s <code>.airk/strategies/</code>. Change the name to save a copy. Export saves only the strategy definition.</p>
            </header>
            {error && <p class="callout" role="alert">{error}</p>}
            {notice && <p class="muted" role="status">{notice}</p>}
            {blocked && <p class="muted">Preview: {blocked} You can still edit the steps and compile a prompt without evidence.</p>}
            <section class="card">
                <h3>Objective and scope</h3>
                <label>Objective<textarea rows={3} maxLength={4000} value={draft.objective} placeholder="What should the coding agent achieve?"
                    onInput={event => setDraft(prior => ({ ...prior, objective: (event.target as HTMLTextAreaElement).value }))} /></label>
                <div class="search-form">
                    {strategy.params.map(param => (
                        <label key={param.name} class="grow">{param.label}{param.required ? ' *' : ''}
                            <input value={draft.params[param.name] ?? ''} maxLength={512} onInput={event => setDraft(prior => ({ ...prior, params: { ...prior.params, [param.name]: (event.target as HTMLInputElement).value } }))} />
                        </label>
                    ))}
                    <label>Path scope<input value={draft.pathPrefix} maxLength={1024} placeholder="e.g. src/auth/" onInput={event => setDraft(prior => ({ ...prior, pathPrefix: (event.target as HTMLInputElement).value }))} /></label>
                </div>
                <p class="muted">Budgets: {strategy.budgets.evidenceBytes.toLocaleString()} bytes of evidence · {strategy.budgets.calls} calls · fan-out {strategy.budgets.fanout} · {strategy.budgets.seconds} s · {Math.min(3, strategy.budgets.concurrency)} concurrent calls. Search steps are always lexical.</p>
            </section>
            <section class="card">
                <h3>Steps</h3>
                <ol class="steps editable">
                    {strategy.steps.map((step, index) => {
                        const spec = operator(step.op);
                        return (
                            <li key={step.id}>
                                <div><strong>{step.id}</strong> · {spec?.title ?? step.op} <code>{step.op}</code>{step.input && <span class="muted"> ← {step.input}</span>}</div>
                                <div class="search-form">
                                    {Object.entries(spec?.args ?? {}).map(([key, arg]) => (
                                        <label key={key}>{key}
                                            {arg.kind === 'enum' ? (
                                                <select value={String(step.args[key] ?? '')} onChange={event => setArg(index, key, (event.target as HTMLSelectElement).value)}>
                                                    <option value="">(default)</option>{arg.values.map(value => <option key={value} value={value}>{value}</option>)}
                                                </select>
                                            ) : arg.kind === 'int' ? (
                                                <input type="number" min={arg.min} max={arg.max} value={String(step.args[key] ?? '')}
                                                    onInput={event => setArg(index, key, Math.min(arg.max, Math.max(arg.min, Number((event.target as HTMLInputElement).value) || arg.min)))} />
                                            ) : (
                                                <input value={String(step.args[key] ?? '')} maxLength={512} onInput={event => setArg(index, key, (event.target as HTMLInputElement).value)} />
                                            )}
                                        </label>
                                    ))}
                                </div>
                                <div class="actions">
                                    <button class="ghost" disabled={index === 0} onClick={() => move(index, -1)} aria-label={`Move ${step.id} up`}>↑</button>
                                    <button class="ghost" disabled={index === strategy.steps.length - 1} onClick={() => move(index, 1)} aria-label={`Move ${step.id} down`}>↓</button>
                                    <button class="ghost" disabled={strategy.steps.length === 1} onClick={() => edit(strategy.steps.filter((_, at) => at !== index))}>Remove</button>
                                </div>
                            </li>
                        );
                    })}
                </ol>
                <div class="search-form">
                    <label>Add step
                        <select value={adding} onChange={event => setAdding((event.target as HTMLSelectElement).value as OperatorId)}>
                            {(Object.keys(OPERATORS) as OperatorId[]).filter(id => !operator(id)!.unavailable).map(id => <option key={id} value={id}>{id} — {operator(id)!.title}</option>)}
                        </select>
                    </label>
                    <button class="ghost" disabled={strategy.steps.length >= 24} onClick={() => edit([...strategy.steps, newStep(adding, strategy.steps)])}>Add</button>
                </div>
                {checked && !checked.ok && <ul class="callout" role="alert">{checked.errors.map(error => <li key={error}>{error}</li>)}</ul>}
                <div class="actions">
                    <button disabled={busy || !runnable} onClick={() => void run()}>{busy ? 'Running…' : 'Run'}</button>
                    {!runnable && !busy && <span class="muted">{blocked ? 'Preview templates do not run.' : checked?.ok ? 'Connect to the AIRK service to run.' : 'Fix the steps to run.'}</span>}
                </div>
            </section>
            {manifest && draft.run && (
                <section class="card">
                    <h3>Run <span class={manifest.outcome === 'verified_snapshot' ? 'tag ok' : 'tag'}>{manifest.outcome === 'verified_snapshot' ? 'verified snapshot' : 'partial'}</span></h3>
                    <p class="muted">Publication <code title={manifest.publication}>{shortToken(manifest.publication)}</code> · {draft.run.records.length} record(s){draft.run.omitted ? ` · ${draft.run.omitted} left out by the budget` : ''} · digest <code>{manifest.evidenceDigest.slice(0, 16)}</code></p>
                    {manifest.reasons.length > 0 && <ul>{manifest.reasons.map(reason => <li key={reason}>{reason}</li>)}</ul>}
                    <div class="table-wrap"><table class="grid"><thead><tr><th>Step</th><th>Operator</th><th>Calls</th><th>Records</th><th>Outcome</th></tr></thead>
                        <tbody>{manifest.steps.map(step => <tr key={step.id}><td>{step.id}</td><td><code>{step.op}</code></td><td>{step.calls}</td><td>{step.records}</td><td>{step.outcome}{step.message ? ` — ${step.message}` : ''}</td></tr>)}</tbody></table></div>
                </section>
            )}
            <section class="card">
                <h3>Prompt</h3>
                {draft.promptStale && <p class="callout">Inputs changed. Your edited prompt is stale; rebuild it before copying or adding it to Planner.</p>}
                <div class="search-form">
                    <label>Profile
                        <select value={draft.profile} onChange={event => { const profile = (event.target as HTMLSelectElement).value as PromptProfile; setDraft(prior => ({ ...prior, profile, ...(prior.promptEdited ? {} : { prompt: compile(prior.run, profile) }) })); }}>
                            <option value="portable">Portable (excerpts included; assumes no AIRK)</option>
                            <option value="mcp_aware">MCP-aware (re-check with AIRK if available)</option>
                        </select>
                    </label>
                    <span class="muted">≈{estimateTokens(prompt).toLocaleString()} tokens (estimate)</span>
                </div>
                <textarea class="prompt" rows={18} value={prompt} aria-label="Prompt"
                    onInput={event => setDraft(prior => ({ ...prior, prompt: (event.target as HTMLTextAreaElement).value, promptEdited: true }))} />
                {confirmRebuild && (
                    <div class="callout" role="alertdialog" aria-label="Discard prompt edits">
                        <p>Rebuilding discards your edits to the prompt.</p>
                        <div class="actions"><button onClick={rebuild}>Discard edits and rebuild</button><button class="ghost" onClick={() => setConfirmRebuild(false)}>Keep edits</button></div>
                    </div>
                )}
                <div class="actions">
                    <button disabled={draft.promptStale} onClick={async () => { try { await request('clipboard.write', { text: prompt }); setNotice('Prompt copied.'); } catch (failure) { setError(message(failure)); } }}>Copy prompt</button>
                    <button class="ghost" onClick={() => draft.promptEdited ? setConfirmRebuild(true) : rebuild()}>Rebuild</button>
                    <button class="ghost" disabled={!canAddStudio({ ...draft, prompt })} title="A complete snapshot must fit 8,000 characters. Reduce evidence explicitly; nothing is truncated." onClick={() => {
                        const snapshot = { ...draft, prompt };
                        setPlanner(prior => {
                            const plan = prior.plan ?? { schema: PLAN_SCHEMA, title: strategy.title, objective: draft.objective, ...(props.state.selectedProject ? { project: props.state.selectedProject } : {}), tasks: [] };
                            if (plan.tasks.length >= 40) { return prior; }
                            return { ...prior, plan: { ...plan, tasks: [...plan.tasks, taskFromStudio(plan, snapshot)] }, revision: prior.revision + 1 };
                        });
                        props.go({ workspace: 'strategies', tab: 'planner' });
                    }}>Add to Planner</button>
                </div>
                <ShareSnapshot kind="studio" revision={draft.revision ?? 0} content={prompt} disabled={draft.promptStale}
                    publication={manifest?.publication === 'unknown' ? undefined : manifest?.publication} />
                <p class="muted">Review the prompt before sharing it: repositories can contain confidential code, secrets, or instructions planted in source comments.</p>
            </section>
        </div>
    );
}
