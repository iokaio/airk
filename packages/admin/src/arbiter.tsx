// SPDX-License-Identifier: Apache-2.0
// Native Core owns every stage/model/configuration DTO and readiness decision.
import type { JSX } from 'preact';
import { useContext, useEffect, useRef, useState } from 'preact/hooks';
import type { Enrollment, ManagementRequest, ModelChoice, ProfileStatus, Stage, StageConfiguration, Status, Task } from '../../contracts-gen/arbiter';
import { call, isUnknownOutcome, type CallResult } from './api';
import { currentSession, onSessionChange } from './session';
import { isCatalogId, isDigest, OPS, type Operation } from './operations';
import { Field, Outcome, Panel, Text, type CallState } from './render';
import { PageVisible } from './pageVisibility';

const stages: readonly Stage[] = ['focus', 'context'];
const stageTitles: Record<Stage, string> = { focus: 'Arbiter Focus', context: 'Arbiter Context' };
const operations = { status: OPS.arbiterStatus, initialize: OPS.arbiterInitialize, configure: OPS.arbiterConfigure,
  provision: OPS.arbiterProvision, cancel: OPS.arbiterCancel, remove: OPS.arbiterRemove, recover: OPS.arbiterRecover } as const;
export function stageProfileTitle(stage: Stage, model: ModelChoice): string | null {
  if (stage === 'focus' && model === 'arbiter-focus-qwen3-reranker-06b') return 'Arbiter Focus';
  if (stage === 'context' && model === 'arbiter-context-qwen3-17b') return 'Arbiter Context — Compact';
  if (stage === 'context' && model === 'arbiter-context-qwen3-4b') return 'Arbiter Context — Large';
  return null;
}
export function selectableStageProfile(stage: Stage, entry: ProfileStatus): boolean {
  return !!stageProfileTitle(stage, entry.model) && !!entry.profile
    && !['unqualified', 'unsupported_system', 'unsupported_task'].includes(entry.availability);
}
function nativeProfileTitle(stage: Stage, profiles: readonly ProfileStatus[], profile: string | null | undefined): string {
  if (!profile) return 'none';
  const entry = profiles.find(entry => entry.profile === profile);
  return entry && stageProfileTitle(stage, entry.model) || profile;
}
function taskTitle(task: Task): string {
  if (task === 'airk.focus_rank.v1') return 'Rank retrieved evidence';
  if (task === 'airk.context_action.v1') return 'Gather and assess additional context';
  return task;
}
type Draft = { configuration: StageConfiguration; index: string; tasks: Task[]; revision: number };
type Review = { generation: number; revision: number; draft: number };
interface ArbiterDependencies {
  session(): unknown;
  visible(): boolean;
  run(operation: Operation, body: unknown): Promise<CallResult>;
  changed(): void;
}
const rec = (value: unknown): Record<string, unknown> => value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};
function observationOf(result: CallResult): Status | null {
  if (result.kind !== 'ok' || rec(result.envelope.result).status !== 'arbiter') return null;
  const value = rec(rec(result.envelope.result).observation), configuration = rec(value.configuration);
  if (value.initialization != null && typeof rec(value.initialization).reason !== 'string') return null;
  if (value.schema !== 'airk.arbiter.status/2' || configuration.schema !== 'airk.arbiter.configuration/3'
      || !Number.isSafeInteger(configuration.revision) || Number(configuration.revision) < 1
      || !stages.every(stage => Array.isArray(rec(value[stage]).profiles)
        && (rec(value[stage]).profiles as unknown[]).every(entry => typeof rec(entry).model === 'string' && Array.isArray(rec(entry).tasks))
        && typeof rec(value[stage]).reason === 'string' && typeof rec(value[stage]).availability === 'string'
        && Array.isArray(rec(configuration[stage]).indexes) && (rec(configuration[stage]).indexes as unknown[]).every(entry => typeof rec(entry).index_id === 'string' && Array.isArray(rec(entry).tasks))
        && typeof rec(configuration[stage]).enabled === 'boolean')) return null;
  return structuredClone(value) as unknown as Status;
}
/** UI-only draft/review state. Requests retain the generated Rust contract. */
export function createArbiterWorkspace(deps: ArbiterDependencies) {
  let session = deps.session(), generation = 0, disposed = false;
  let observation: Status | null = null, drafts: Record<Stage, Draft> | null = null;
  let outcome: CallState = { phase: 'idle' }, originalRequest: ManagementRequest | null = null;
  let sending = false, unresolved = false, problem = '', legacyDigest = '', observationRevision = 0, lastObservedAt: number | null = null, statusStale = false;
  const synchronize = () => {
    if (session === deps.session()) return;
    session = deps.session(); generation++; observation = null; drafts = null; outcome = { phase: 'idle' };
    originalRequest = null; sending = false; unresolved = false; problem = ''; legacyDigest = ''; lastObservedAt = null; statusStale = false;
  };
  const valid = (review: Review | undefined, stage?: Stage) => {
    synchronize();
    return !!review && !!observation && !!drafts && review.generation === generation
      && review.revision === observation.configuration.revision && (stage === undefined || review.draft === drafts[stage].revision);
  };
  const adopt = (next: Status, request: ManagementRequest) => {
    observation = next; observationRevision++; lastObservedAt = Date.now(); statusStale = false;
    if (next.initialization) {
      drafts = null; legacyDigest = next.initialization.actual_legacy_digest ?? ''; return;
    }
    legacyDigest = '';
    if (!drafts || request.operation === 'initialize') {
      const draft = (stage: Stage): Draft => ({ configuration: structuredClone(next.configuration[stage]), index: '', tasks: [], revision: 0 });
      drafts = { focus: draft('focus'), context: draft('context') };
    } else if (request.operation === 'configure') {
      drafts[request.stage] = { ...drafts[request.stage], configuration: structuredClone(next.configuration[request.stage]), revision: drafts[request.stage].revision + 1 };
    }
  };
  const send = async (request: ManagementRequest) => {
    synchronize();
    if (disposed || session === null || !deps.visible() || sending || request.operation !== 'status' && unresolved) return;
    const capturedSession = session, ticket = generation;
    const current = () => !disposed && generation === ticket && deps.session() === capturedSession;
    const operation = operations[request.operation];
    originalRequest = structuredClone(request); if (request.operation === 'status') statusStale = true; sending = true; problem = ''; outcome = { phase: 'pending', operation }; deps.changed();
    try {
      const result = await deps.run(operation, { arbiter: { request: structuredClone(request) } });
      if (!current()) return;
      outcome = { phase: 'done', operation, result, at: Date.now() };
      if (request.operation !== 'status') unresolved = isUnknownOutcome(result);
      const native = observationOf(result);
      if (native) { adopt(native, request); if (request.operation === 'status') unresolved = false; }
      else if (result.kind === 'ok') {
        problem = 'This response has no current two-stage native status. Its complete original outcome is retained below.';
        unresolved = true;
      }
    } catch {
      if (current()) {
        problem = 'No typed management outcome was received. Read native status before another change.';
        if (request.operation !== 'status') unresolved = true;
        outcome = { phase: 'idle' };
      }
    } finally { if (current()) { sending = false; deps.changed(); } }
  };
  const edit = (stage: Stage, change: (draft: Draft) => void) => {
    synchronize(); if (!drafts || sending || unresolved || disposed || !deps.visible()) return;
    change(drafts[stage]); drafts[stage].revision++; deps.changed();
  };
  const view = () => {
    synchronize();
    const reviews = observation && drafts ? Object.fromEntries(stages.map(stage => [stage, { generation, revision: observation!.configuration.revision, draft: drafts![stage].revision }])) as Record<Stage, Review> : null;
    return { observation: observation && structuredClone(observation), drafts: drafts && structuredClone(drafts), reviews,
      generation, observationRevision, lastObservedAt, statusStale, sending, unresolved, problem, outcome, originalRequest: originalRequest && structuredClone(originalRequest), legacyDigest,
      canSend: !disposed && session !== null && deps.visible() && !sending && !unresolved };
  };
  return {
    view,
    sync() { synchronize(); deps.changed(); },
    readStatus: () => send({ operation: 'status' }),
    initialize: (reviewedGeneration: number, reviewedObservation = observationRevision) => {
      synchronize();
      if (reviewedGeneration !== generation || reviewedObservation !== observationRevision || observation && !observation.initialization) return;
      const digest = observation?.initialization ? observation.initialization.actual_legacy_digest ?? null : legacyDigest || null;
      if (digest === null || isDigest(digest)) return send({ operation: 'initialize', expected_legacy_configuration: digest });
    },
    setLegacyDigest(value: string) { synchronize(); if (observation?.initialization) return; legacyDigest = value; deps.changed(); },
    enable(stage: Stage, enabled: boolean) { edit(stage, draft => { draft.configuration.enabled = enabled; }); },
    select(stage: Stage, profile: string | null) {
      if (profile !== null && !observation?.[stage].profiles.some(entry => entry.profile === profile && selectableStageProfile(stage, entry))) return;
      edit(stage, draft => { draft.configuration.selected_profile = profile; });
    },
    indexInput(stage: Stage, index: string) { edit(stage, draft => { draft.index = index; }); },
    taskInput(stage: Stage, task: Task, checked: boolean) {
      if (!observation?.[stage].profiles.some(entry => !!stageProfileTitle(stage, entry.model) && entry.tasks.includes(task))) return;
      edit(stage, draft => { draft.tasks = checked ? [...new Set([...draft.tasks, task])] : draft.tasks.filter(value => value !== task); });
    },
    addIndex(stage: Stage) {
      edit(stage, draft => {
        if (!isCatalogId('idx_', draft.index) || !draft.tasks.length || draft.configuration.indexes.some(entry => entry.index_id === draft.index)) return;
        const enrollment: Enrollment = { index_id: draft.index, tasks: [...draft.tasks] };
        draft.configuration.indexes.push(enrollment); draft.index = ''; draft.tasks = [];
      });
    },
    removeIndex(stage: Stage, index: string) { edit(stage, draft => { draft.configuration.indexes = draft.configuration.indexes.filter(entry => entry.index_id !== index); }); },
    reset(stage: Stage) { edit(stage, draft => { draft.configuration = structuredClone(observation!.configuration[stage]); draft.index = ''; draft.tasks = []; }); },
    save(stage: Stage, review: Review | undefined) {
      if (!valid(review, stage) || !drafts) return;
      const configuration = drafts[stage].configuration;
      if (configuration.enabled && (!configuration.indexes.length || !observation![stage].profiles.some(entry => entry.profile === configuration.selected_profile && selectableStageProfile(stage, entry)))) return;
      return send({ operation: 'configure', stage, expected_revision: review!.revision, ...structuredClone(configuration) });
    },
    model(stage: Stage, action: 'provision' | 'remove', profile: string, review: Review | undefined) {
      if (!valid(review)) return;
      const entry = observation![stage].profiles.find(entry => entry.profile === profile && !!stageProfileTitle(stage, entry.model));
      if (!entry || action === 'provision' && !selectableStageProfile(stage, entry)) return;
      return send({ operation: action, expected_revision: review!.revision, profile });
    },
    lifecycle(stage: Stage, action: 'cancel' | 'recover', review: Review | undefined) {
      if (valid(review)) return send({ operation: action, expected_revision: review!.revision, stage });
    },
    dispose() { disposed = true; generation++; },
  };
}

export function Arbiter(): JSX.Element {
  const [, changed] = useState(0), visible = useContext(PageVisible), visibility = useRef(visible); visibility.current = visible;
  const owner = useRef<ReturnType<typeof createArbiterWorkspace> | null>(null);
  if (!owner.current) owner.current = createArbiterWorkspace({ session: currentSession, visible: () => visibility.current,
    run: call, changed: () => changed(value => value + 1) });
  const workspace = owner.current;
  useEffect(() => { const stop = onSessionChange(() => workspace.sync()); return () => { stop(); workspace.dispose(); }; }, [workspace]);
  useEffect(() => { if (visible) void workspace.readStatus(); }, [workspace, visible, currentSession()]);
  const view = workspace.view();
  return <Panel title="Optional local Arbiter stages">
    <p>Arbiter Focus ranks retrieved evidence. Arbiter Context gathers and organizes additional context. Both stages are off by default and have separate model selection and index consent. Either stage can run while the other is off. Ordinary retrieval remains available when both are off.</p>
    <button type="button" disabled={view.sending || !visible} onClick={() => void workspace.readStatus()}>{view.sending ? 'Reading Arbiter status...' : 'Refresh Arbiter status'}</button>
    {view.observation && view.statusStale ? <p role="status">Showing retained Arbiter status{view.lastObservedAt ? ` from ${new Date(view.lastObservedAt).toLocaleTimeString()}` : ''}. {view.sending ? 'Refreshing...' : 'The last refresh did not return current status. Use Refresh Arbiter status to try again.'}</p> : null}
    {!view.observation ? <p>{view.sending ? 'Loading Focus and Context status...' : 'Arbiter status is unavailable. Use Refresh Arbiter status to try again.'}</p> : !view.drafts ? <p>Native configuration requires initialization. The reported disabled configuration is a placeholder and cannot be edited or saved.</p> : stages.map(stage => {
      const status = view.observation![stage], draft = view.drafts![stage], review = view.reviews![stage];
      const title = stageTitles[stage], tasks = [...new Set(status.profiles.filter(entry => !!stageProfileTitle(stage, entry.model)).flatMap(entry => entry.tasks))];
      const validEnabled = !draft.configuration.enabled || draft.configuration.indexes.length > 0 && status.profiles.some(entry => entry.profile === draft.configuration.selected_profile && selectableStageProfile(stage, entry));
      return <section aria-label={title} key={stage}>
        <h3>{title}</h3>
        <p>Status: <Text value={status.availability} />. <Text value={status.reason} /></p>
        <p>Turned {view.observation!.configuration[stage].enabled ? 'on' : 'off'}. Selected model: <Text value={nativeProfileTitle(stage, status.profiles, status.selected_profile)} />. Active model: <Text value={nativeProfileTitle(stage, status.profiles, status.active_profile)} />. Queued: {status.queued}; running: {status.running}.</p>
        <label><input type="checkbox" checked={draft.configuration.enabled} disabled={!view.canSend} onChange={event => workspace.enable(stage, event.currentTarget.checked)} /> Enable {title}</label>
        <h4>Choose and download a model</h4>
        <p>Selection changes this draft only. Provisioning is a separate explicit action. Verified cached assets can be used offline; this page offers no file import. Missing assets are never downloaded by an ordinary read or a checkbox.</p>
        <label><input type="radio" name={`arbiter-${stage}-profile`} checked={!draft.configuration.selected_profile} disabled={!view.canSend} onChange={() => workspace.select(stage, null)} /> No model selected</label>
        {!status.profiles.length ? <p>No native model choices are available for this stage.</p> : null}
        {status.profiles.map(entry => { const name = stageProfileTitle(stage, entry.model), selectable = selectableStageProfile(stage, entry);
          return <article key={entry.model}>
            <h4><Text value={name ?? 'Unsupported native model choice'} /></h4>
            <p><Text value={entry.availability} />: <Text value={entry.reason} /></p>
            <p>Download size: {entry.download_bytes ?? 'not reported'} bytes. Provisional memory reservation: {entry.memory_reservation_bytes ?? 'not reported'} bytes. A reservation is not a measurement of currently available memory.</p>
            {entry.cpu_fit ? <p>Declared CPU fit: {entry.cpu_fit.minimum_physical_cores} physical cores; intra-op threads {entry.cpu_fit.intra_threads}; inter-op threads {entry.cpu_fit.inter_threads}. These are native profile requirements, not a measurement of this host.</p> : <p>No native CPU fit was reported.</p>}
            <p>Use the native availability and reason above for this host; model names or sizes do not establish fit or qualification.</p>
            <label><input type="radio" name={`arbiter-${stage}-profile`} checked={!!entry.profile && draft.configuration.selected_profile === entry.profile} disabled={!view.canSend || !selectable} onChange={() => workspace.select(stage, entry.profile ?? null)} /> Select <Text value={name ?? entry.model} /></label>
            <div class="actions"><button type="button" disabled={!view.canSend || !selectable} onClick={() => entry.profile && void workspace.model(stage, 'provision', entry.profile, review)}>Download <Text value={name ?? entry.model} /></button>
              <button type="button" class="danger" disabled={!view.canSend || !name || !entry.profile} onClick={() => entry.profile && void workspace.model(stage, 'remove', entry.profile, review)}>Remove local <Text value={name ?? entry.model} /> model</button></div>
            <details><summary>Complete native profile status and qualification</summary><pre><Text value={JSON.stringify(entry, null, 2)} /></pre></details>
          </article>;
        })}
        <h4>Choose which indexes {title} may read</h4>
        {!draft.configuration.indexes.length ? <p>No indexes are enrolled in this stage. Enablement never enrolls indexes automatically.</p> : draft.configuration.indexes.map(entry => <div class="row" key={entry.index_id}><Text value={entry.index_id} /><span><Text value={entry.tasks.map(taskTitle).join(', ')} /></span>
          <button type="button" disabled={!view.canSend} onClick={() => workspace.removeIndex(stage, entry.index_id)}>Remove {title} enrollment</button></div>)}
        <Field label={`${title}: immutable index ID from Index library`} value={draft.index} onInput={value => workspace.indexInput(stage, value)} invalid={draft.index !== '' && !isCatalogId('idx_', draft.index)} />
        {tasks.map(task => <label key={task}><input type="checkbox" checked={draft.tasks.includes(task)} disabled={!view.canSend} onChange={event => workspace.taskInput(stage, task, event.currentTarget.checked)} /><Text value={taskTitle(task)} /></label>)}
        <button type="button" class="secondary" disabled={!view.canSend || !isCatalogId('idx_', draft.index) || !draft.tasks.length || draft.configuration.indexes.some(entry => entry.index_id === draft.index)} onClick={() => workspace.addIndex(stage)}>Add explicit {title} enrollment to draft</button>
        {!validEnabled ? <p role="status">Before enabling this stage, select its available native model and explicitly enroll at least one index. Turning it off does not change the other stage.</p> : null}
        <p class="muted">Changes apply only to this stage. Review the selected model and enrolled indexes before applying.</p>
        <div class="actions"><button type="button" disabled={!view.canSend || !validEnabled} onClick={() => void workspace.save(stage, review)}>Apply {title} choices</button>
          <button type="button" class="secondary" disabled={!view.canSend} onClick={() => workspace.reset(stage)}>Reset {title} draft to saved native state</button>
          <button type="button" class="secondary" disabled={!view.canSend} onClick={() => void workspace.lifecycle(stage, 'cancel', review)}>Cancel {title} model operation</button>
          <button type="button" class="secondary" disabled={!view.canSend} onClick={() => void workspace.lifecycle(stage, 'recover', review)}>Recover {title} model state</button></div>
      </section>;
    })}
    {!view.observation || view.observation.initialization ? <details open={!!view.observation?.initialization}><summary>Initialize separate disabled stages</summary><p>Initialization creates both stages off with no enrolled indexes. It preserves an existing legacy configuration. If native status reports a legacy configuration digest, use that exact digest; leave this field empty only for a new configuration.</p>
      {view.observation?.initialization ? <div><p>Native initialization reason: <Text value={view.observation.initialization.reason} /></p><p>Actual retained legacy configuration digest: <Text value={view.observation.initialization.actual_legacy_digest ?? 'No single retained legacy digest was reported.'} /></p><p>This action sends the exact native digest when reported. Native checks may still refuse initialization, including ambiguous or corrupt retained state. No stage consent is migrated automatically.</p></div> : <Field label="Original legacy configuration digest from native status (optional)" value={view.legacyDigest} invalid={view.legacyDigest !== '' && !isDigest(view.legacyDigest)} onInput={value => workspace.setLegacyDigest(value)} />}
      <button type="button" disabled={!view.canSend || view.legacyDigest !== '' && !isDigest(view.legacyDigest)} onClick={() => void workspace.initialize(view.generation, view.observationRevision)}>Initialize both stages off</button></details> : null}
    {view.unresolved ? <p class="error" role="alert">The original management outcome is unknown. Read current native stage status and review its revision before another change. No automatic retry occurs.</p> : null}
    {view.problem ? <p role="alert"><Text value={view.problem} /></p> : null}
    {view.originalRequest ? <details><summary>Complete original native management request</summary><pre><Text value={JSON.stringify(view.originalRequest, null, 2)} /></pre></details> : null}
    <details><summary>Details and complete management outcome</summary><Outcome state={view.outcome} /></details>
  </Panel>;
}
