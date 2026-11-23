// SPDX-License-Identifier: Apache-2.0
// Native Core owns every stage/model/configuration DTO and readiness decision.
import type { JSX } from 'preact';
import { useContext, useEffect, useRef, useState } from 'preact/hooks';
import type { DeviceModelStatus, DevicePolicy, DeviceStageConfiguration, DeviceStageStatus, DeviceStatus as Status, DeviceVariantStatus, Enrollment, GpuPrecisionPreference, ManagementRequest, SelectedModel, Stage, Task } from '../../contracts-gen/arbiter';
import { call, isUnknownOutcome, type CallResult } from './api';
import { currentSession, onSessionChange } from './session';
import { isCatalogId, isDigest, OPS, type Operation } from './operations';
import { Field, Outcome, Panel, Text, type CallState } from './render';
import { PageVisible } from './pageVisibility';

const stages: readonly Stage[] = ['focus', 'context'];
const stageTitles: Record<Stage, string> = { focus: 'Focus', context: 'Arbiter' };
type CurrentRequest = Extract<ManagementRequest, { operation: 'status_device' | 'initialize_device' | 'configure_device' | 'migrate_device' | 'provision_device' | 'cancel_device' | 'remove_device' | 'recover_device' }>;
const operations = { status_device: OPS.arbiterStatus, initialize_device: OPS.arbiterInitialize, configure_device: OPS.arbiterConfigure,
  migrate_device: OPS.arbiterMigrate, provision_device: OPS.arbiterProvision, cancel_device: OPS.arbiterCancel, remove_device: OPS.arbiterRemove, recover_device: OPS.arbiterRecover } as const;
export function stageModelTitle(stage: Stage, model: SelectedModel): string | null {
  if (stage === 'focus' && model === 'focus') return 'Focus';
  if (stage === 'context' && model === 'arbiter') return 'Arbiter';
  return null;
}
/** Native model-admission observations are descriptive, never an execution permit. */
export function selectableStageVariant(entry: DeviceVariantStatus, model: DeviceModelStatus): boolean {
  return nativeModelAdmitted(model) && !['unqualified', 'unsupported_system', 'unsupported_task'].includes(entry.availability);
}
/** Requested/Ready is the native owner's admission observation, not inferred qualification. */
function nativeModelAdmitted(entry: DeviceModelStatus): boolean {
  return entry.availability === 'requested' || entry.availability === 'ready';
}
export function selectableStageModel(stage: Stage, entry: DeviceModelStatus): boolean {
  return !!stageModelTitle(stage, entry.model);
}
function admittedStageModel(stage: Stage, entry: DeviceModelStatus): boolean {
  return selectableStageModel(stage, entry) && nativeModelAdmitted(entry);
}
function nativeModelTitle(stage: Stage, model: SelectedModel | null | undefined): string {
  return model ? stageModelTitle(stage, model) ?? 'Unsupported native model choice' : 'none';
}
function activeModelTitle(stage: Stage, status: DeviceStageStatus): string {
  if (!status.active_profile) return 'none';
  const model = status.models.find(model => model.variants.some(variant => variant.manifest === status.active_profile));
  return model ? nativeModelTitle(stage, model.model) : 'Unknown native variant (see Details)';
}
const deviceTitle: Record<DevicePolicy, string> = { auto: 'Auto', cpu: 'CPU' };
const preferenceTitle: Record<GpuPrecisionPreference, string> = { auto: 'Auto', fp32: 'FP32', fp16: 'FP16' };
function precisionChoices(status: DeviceStageStatus): GpuPrecisionPreference[] {
  const variants = status.models.flatMap(model => model.variants).filter(variant => variant.backend !== null);
  const choices: GpuPrecisionPreference[] = ['auto'];
  if (variants.some(variant => variant.precision === 'float32')) choices.push('fp32');
  if (variants.some(variant => variant.precision === 'float16')) choices.push('fp16');
  return choices;
}
function backendTitle(backend: DeviceVariantStatus['backend']): string {
  return backend === null ? 'CPU' : backend === 'cuda' ? 'CUDA' : backend === 'direct_ml' ? 'DirectML' : backend === 'core_ml' ? 'Core ML' : 'Unsupported native backend';
}
function precisionTitle(precision: DeviceVariantStatus['precision']): string {
  return precision === 'float32' ? 'FP32' : precision === 'float16' ? 'FP16' : 'precision not reported';
}
function variantTitle(variant: DeviceVariantStatus): string {
  return variant.backend ? `${backendTitle(variant.backend)} · ${precisionTitle(variant.precision)}` : 'CPU · native profile';
}
function selectionTitle(status: DeviceStageStatus): string {
  const selected = status.selection?.selected;
  if (!selected) return 'No execution variant selected';
  return selected.backend === null ? 'CPU · native CPU profile; precision not reported' : `${backendTitle(selected.backend)} · ${precisionTitle(selected.precision)}`;
}
function taskTitle(task: Task): string {
  if (task === 'airk.focus_rank.v1') return 'Rank retrieved evidence';
  if (task === 'airk.context_action.v1') return 'Gather and assess additional context';
  return task;
}
type Draft = { configuration: DeviceStageConfiguration; index: string; tasks: Task[]; revision: number };
type Review = { generation: number; revision: number; draft: number };
interface ArbiterDependencies {
  session(): unknown;
  visible(): boolean;
  run(operation: Operation, body: unknown): Promise<CallResult>;
  changed(): void;
}
const rec = (value: unknown): Record<string, unknown> => value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};
const backendKnown = (value: unknown): boolean => value === null || ['cuda','direct_ml','core_ml'].includes(String(value));
const precisionKnown = (value: unknown): boolean => value === null || value === 'float32' || value === 'float16';
function selectionDisplayable(value: unknown): boolean {
  if (value == null) return true;
  const selection = rec(value), selected = selection.selected;
  return selection.schema === 'airk.arbiter.device-status/1' && Array.isArray(selection.reasons)
    && selection.reasons.every(reason => typeof rec(reason).reason === 'string')
    && (selection.operation_failure == null || typeof selection.operation_failure === 'string')
    && (selected === null || typeof rec(selected).manifest === 'string'
      && backendKnown(rec(selected).backend) && precisionKnown(rec(selected).precision)
      && (rec(selected).backend === null) === (rec(selected).precision === null));
}
function observationOf(result: CallResult): Status | null {
  if (result.kind !== 'ok' || rec(result.envelope.result).status !== 'arbiter_device') return null;
  const value = rec(rec(result.envelope.result).observation), configuration = rec(value.configuration);
  if (value.initialization != null && typeof rec(value.initialization).reason !== 'string') return null;
  if (value.schema !== 'airk.arbiter.status/3' || configuration.schema !== 'airk.arbiter.configuration/5'
      || !Number.isSafeInteger(configuration.revision) || Number(configuration.revision) < 1
      || !stages.every(stage => Array.isArray(rec(value[stage]).models)
        && (rec(value[stage]).models as unknown[]).every(entry => typeof rec(entry).model === 'string'
          && typeof rec(entry).availability === 'string' && typeof rec(entry).reason === 'string'
          && Array.isArray(rec(entry).tasks) && Array.isArray(rec(entry).variants)
          && (rec(entry).variants as unknown[]).every(variant => typeof rec(variant).manifest === 'string'
            && typeof rec(variant).execution_identity === 'string' && typeof rec(variant).reason === 'string'
            && typeof rec(variant).model_admission === 'string' && Array.isArray(rec(variant).tasks)
            && backendKnown(rec(variant).backend) && precisionKnown(rec(variant).precision)
            && (rec(variant).backend === null) === (rec(variant).precision === null)))
        && typeof rec(value[stage]).reason === 'string' && typeof rec(value[stage]).availability === 'string'
        && ['auto','cpu'].includes(String(rec(value[stage]).device_policy))
        && ['auto','fp32','fp16'].includes(String(rec(value[stage]).gpu_precision))
        && Number.isSafeInteger(rec(value[stage]).queued) && Number(rec(value[stage]).queued) >= 0
        && Number.isSafeInteger(rec(value[stage]).running) && Number(rec(value[stage]).running) >= 0
        && selectionDisplayable(rec(value[stage]).selection)
        && Array.isArray(rec(configuration[stage]).indexes) && (rec(configuration[stage]).indexes as unknown[]).every(entry => typeof rec(entry).index_id === 'string' && Array.isArray(rec(entry).tasks))
        && ['auto','cpu'].includes(String(rec(configuration[stage]).device_policy))
        && ['auto','fp32','fp16'].includes(String(rec(configuration[stage]).gpu_precision))
        && typeof rec(configuration[stage]).enabled === 'boolean')) return null;
  return structuredClone(value) as unknown as Status;
}
/** UI-only draft/review state. Requests retain the generated Rust contract. */
export function createArbiterWorkspace(deps: ArbiterDependencies) {
  let session = deps.session(), generation = 0, disposed = false;
  let observation: Status | null = null, drafts: Record<Stage, Draft> | null = null;
  let outcome: CallState = { phase: 'idle' }, originalRequest: CurrentRequest | null = null;
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
  const adopt = (next: Status, request: CurrentRequest) => {
    observation = next; observationRevision++; lastObservedAt = Date.now(); statusStale = false;
    if (next.initialization) {
      drafts = null; legacyDigest = next.initialization.actual_legacy_digest ?? ''; return;
    }
    legacyDigest = '';
    if (!drafts || request.operation === 'initialize_device') {
      const draft = (stage: Stage): Draft => ({ configuration: structuredClone(next.configuration[stage]), index: '', tasks: [], revision: 0 });
      drafts = { focus: draft('focus'), context: draft('context') };
    } else if (request.operation === 'configure_device') {
      drafts[request.stage] = { ...drafts[request.stage], configuration: structuredClone(next.configuration[request.stage]), revision: drafts[request.stage].revision + 1 };
    }
  };
  const send = async (request: CurrentRequest) => {
    synchronize();
    if (disposed || session === null || !deps.visible() || sending || request.operation !== 'status_device' && (unresolved || statusStale)) return;
    const capturedSession = session, ticket = generation;
    const current = () => !disposed && generation === ticket && deps.session() === capturedSession;
    const operation = operations[request.operation];
    originalRequest = structuredClone(request); if (request.operation === 'status_device') statusStale = true; sending = true; problem = ''; outcome = { phase: 'pending', operation }; deps.changed();
    try {
      const result = await deps.run(operation, { arbiter: { request: structuredClone(request) } });
      if (!current()) return;
      outcome = { phase: 'done', operation, result, at: Date.now() };
      if (request.operation !== 'status_device') unresolved = isUnknownOutcome(result);
      const native = observationOf(result);
      if (native) { adopt(native, request); if (request.operation === 'status_device') unresolved = false; }
      else if (result.kind === 'ok') {
        problem = 'This response has no current device-aware native status. Its complete original outcome is retained below.';
        unresolved = true;
      }
    } catch {
      if (current()) {
        problem = 'No typed management outcome was received. Read native status before another change.';
        if (request.operation !== 'status_device') unresolved = true;
        outcome = { phase: 'idle' };
      }
    } finally { if (current()) { sending = false; deps.changed(); } }
  };
  const edit = (stage: Stage, change: (draft: Draft) => void) => {
    synchronize(); if (!drafts || sending || unresolved || statusStale || disposed || !deps.visible()) return;
    change(drafts[stage]); drafts[stage].revision++; deps.changed();
  };
  const view = () => {
    synchronize();
    const reviews = observation && drafts ? Object.fromEntries(stages.map(stage => [stage, { generation, revision: observation!.configuration.revision, draft: drafts![stage].revision }])) as Record<Stage, Review> : null;
    return { observation: observation && structuredClone(observation), drafts: drafts && structuredClone(drafts), reviews,
      generation, observationRevision, lastObservedAt, statusStale, sending, unresolved, problem, outcome, originalRequest: originalRequest && structuredClone(originalRequest), legacyDigest,
      canSend: !disposed && session !== null && deps.visible() && !sending && !unresolved && !statusStale };
  };
  return {
    view,
    sync() { synchronize(); deps.changed(); },
    readStatus: () => send({ operation: 'status_device' }),
    initialize: (reviewedGeneration: number, reviewedObservation = observationRevision) => {
      synchronize();
      if (reviewedGeneration !== generation || reviewedObservation !== observationRevision || observation && !observation.initialization) return;
      const digest = observation?.initialization ? observation.initialization.actual_legacy_digest ?? null : legacyDigest || null;
      if (digest === null || isDigest(digest)) return send({ operation: 'initialize_device', expected_legacy_configuration: digest });
    },
    setLegacyDigest(value: string) { synchronize(); if (observation?.initialization) return; legacyDigest = value; deps.changed(); },
    enable(stage: Stage, enabled: boolean) { edit(stage, draft => { draft.configuration.enabled = enabled; }); },
    select(stage: Stage, model: SelectedModel | null) {
      if (model !== null && !observation?.[stage].models.some(entry => entry.model === model && selectableStageModel(stage, entry))) return;
      edit(stage, draft => { draft.configuration.selected_model = model; });
    },
    device(stage: Stage, policy: DevicePolicy) {
      if (policy !== 'auto' && policy !== 'cpu') return;
      edit(stage, draft => { draft.configuration.device_policy = policy; });
    },
    precision(stage: Stage, precision: GpuPrecisionPreference) {
      synchronize();
      if (!observation || statusStale || !precisionChoices(observation[stage]).includes(precision)) return;
      edit(stage, draft => { draft.configuration.gpu_precision = precision; });
    },
    indexInput(stage: Stage, index: string) { edit(stage, draft => { draft.index = index; }); },
    taskInput(stage: Stage, task: Task, checked: boolean) {
      if (!observation?.[stage].models.some(entry => !!stageModelTitle(stage, entry.model) && entry.tasks.includes(task))) return;
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
      if (configuration.enabled && (!configuration.indexes.length || !observation![stage].models.some(entry => entry.model === configuration.selected_model && admittedStageModel(stage, entry)))) return;
      return send({ operation: 'configure_device', stage, expected_revision: review!.revision, configuration: structuredClone(configuration) });
    },
    model(stage: Stage, action: 'provision' | 'remove', profile: string, review: Review | undefined) {
      if (!valid(review)) return;
      const model = observation![stage].models.find(model => !!stageModelTitle(stage, model.model) && model.variants.some(entry => entry.manifest === profile));
      const entry = model?.variants.find(entry => entry.manifest === profile);
      if (!entry || !model || action === 'provision' && !selectableStageVariant(entry, model)) return;
      return send({ operation: action === 'provision' ? 'provision_device' : 'remove_device', expected_revision: review!.revision, profile });
    },
    lifecycle(stage: Stage, action: 'cancel' | 'recover', review: Review | undefined) {
      if (valid(review)) return send({ operation: action === 'cancel' ? 'cancel_device' : 'recover_device', expected_revision: review!.revision, stage });
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
  return <Panel title="Optional local models for Answer">
    <p>Focus (Qwen3-Reranker-0.6B) ranks retrieved evidence. Arbiter (Qwen3-1.7B) gathers and assesses context for Answer. Both are off by default and have separate model selection and index consent. They are separate switches: Answer runs with neither, either or both. Ordinary retrieval remains available when both are off.</p>
    <button type="button" disabled={view.sending || !visible} onClick={() => void workspace.readStatus()}>{view.sending ? 'Reading Arbiter status...' : 'Refresh Arbiter status'}</button>
    {view.observation && view.statusStale ? <p role="status">Showing retained Arbiter status{view.lastObservedAt ? ` from ${new Date(view.lastObservedAt).toLocaleTimeString()}` : ''}. {view.sending ? 'Refreshing...' : 'The last refresh did not return current status. Use Refresh Arbiter status to try again.'}</p> : null}
    {!view.observation ? <p>{view.sending ? 'Loading Focus and Context status...' : 'Arbiter status is unavailable. Use Refresh Arbiter status to try again.'}</p> : !view.drafts ? <p>Native configuration requires initialization. The reported disabled configuration is a placeholder and cannot be edited or saved.</p> : stages.map(stage => {
      const status = view.observation![stage], draft = view.drafts![stage], review = view.reviews![stage];
      const title = stageTitles[stage], tasks = [...new Set(status.models.filter(entry => !!stageModelTitle(stage, entry.model)).flatMap(entry => entry.tasks))];
      const choices = precisionChoices(status);
      const requested = draft.configuration.gpu_precision;
      const validEnabled = !draft.configuration.enabled || draft.configuration.indexes.length > 0 && status.models.some(entry => entry.model === draft.configuration.selected_model && admittedStageModel(stage, entry));
      return <section aria-label={title} key={stage}>
        <h3>{title}</h3>
        <p>Status: <Text value={status.availability} />. <Text value={status.reason} /></p>
        <p>Turned {view.observation!.configuration[stage].enabled ? 'on' : 'off'}. Selected model: <Text value={nativeModelTitle(stage, status.selected_model)} />. Active model: <Text value={activeModelTitle(stage, status)} />. Queued: {status.queued}; running: {status.running}.</p>
        <p>Requested device: <Text value={deviceTitle[status.device_policy]} />. Requested GPU precision: <Text value={preferenceTitle[status.gpu_precision]} />. Native selection: <Text value={selectionTitle(status)} />.</p>
        {status.selection?.reasons.length ? <ul aria-label={`${title} native selection reasons`}>{status.selection.reasons.map((row, index) => <li key={index}><Text value={row.reason.replaceAll('_',' ')} /></li>)}</ul> : null}
        {status.selection?.operation_failure ? <p role="status">Native operation failure: <Text value={status.selection.operation_failure} /></p> : null}
        <details><summary>Complete native stage and device selection</summary><pre><Text value={JSON.stringify(status, null, 2)} /></pre></details>
        <label><input type="checkbox" checked={draft.configuration.enabled} disabled={!view.canSend} onChange={event => workspace.enable(stage, event.currentTarget.checked)} /> Enable {title}</label>
        <h4>Choose a model and execution policy</h4>
        <p>Model selection, device policy and GPU precision change this draft only. Auto permits native CPU fallback when eligible; CPU excludes GPU candidates. A saved request never establishes current capacity, runtime availability or qualification.</p>
        <label>Device policy<select aria-label={`${title} device policy`} value={draft.configuration.device_policy} disabled={!view.canSend} onChange={event => workspace.device(stage, event.currentTarget.value as DevicePolicy)}><option value="auto">Auto</option><option value="cpu">CPU</option></select></label>
        <label>GPU precision<select aria-label={`${title} GPU precision`} value={requested} disabled={!view.canSend || view.statusStale} onChange={event => workspace.precision(stage, event.currentTarget.value as GpuPrecisionPreference)}>
          {choices.map(choice => <option key={choice} value={choice}>{preferenceTitle[choice]}</option>)}
          {!choices.includes(requested) && <option value={requested} disabled>{preferenceTitle[requested]} (unavailable)</option>}
        </select></label>
        <p>Precision applies only to GPU candidates and is saved but unused while CPU is selected. CPU fallback retains its native CPU profile; requested FP32 or FP16 is never shown as the CPU's actual precision.</p>
        <label><input type="radio" name={`arbiter-${stage}-model`} checked={!draft.configuration.selected_model} disabled={!view.canSend} onChange={() => workspace.select(stage, null)} /> No model selected</label>
        {!status.models.length ? <p>No native model choices are available for this stage.</p> : null}
        {status.models.map(model => { const name = stageModelTitle(stage, model.model), selectable = selectableStageModel(stage, model);
          return <article key={model.model}>
            <h4><Text value={name ?? 'Unsupported native model choice'} /></h4>
            <p><Text value={model.availability} />: <Text value={model.reason} /></p>
            <label><input type="radio" name={`arbiter-${stage}-model`} checked={draft.configuration.selected_model === model.model} disabled={!view.canSend || !selectable} onChange={() => workspace.select(stage, model.model)} /> Select <Text value={name ?? model.model} /></label>
            {!model.variants.length ? <p>No execution variants are reported. Model choice does not establish availability.</p> : null}
            {model.variants.map(entry => <div role="group" aria-label={`${name ?? model.model}: ${variantTitle(entry)}`} key={entry.manifest}>
              <h5><Text value={variantTitle(entry)} /></h5><p><Text value={entry.availability} />: <Text value={entry.reason} /></p>
              <p>Native model admission: <Text value={entry.model_admission} />.{entry.model_admission === 'local_candidate' ? ' Local candidate admission does not establish independent model qualification.' : ''} Execution still requires native runtime, device and capacity admission.</p>
              <p>Download size: {entry.download_bytes ?? 'not reported'} bytes. Provisional memory reservation: {entry.memory_reservation_bytes ?? 'not reported'} bytes. A reservation is not a measurement of currently available memory.</p>
              {entry.cpu_fit ? <p>Declared CPU fit: {entry.cpu_fit.minimum_physical_cores} physical cores; intra-op threads {entry.cpu_fit.intra_threads}; inter-op threads {entry.cpu_fit.inter_threads}. These are native profile requirements, not a measurement of this host.</p> : <p>No native CPU fit was reported.</p>}
              <div class="actions"><button type="button" disabled={!view.canSend || !name || !selectableStageVariant(entry, model)} onClick={() => void workspace.model(stage, 'provision', entry.manifest, review)}>Download <Text value={`${name ?? model.model} · ${variantTitle(entry)}`} /></button>
                <button type="button" class="danger" disabled={!view.canSend || !name} onClick={() => void workspace.model(stage, 'remove', entry.manifest, review)}>Remove local <Text value={`${name ?? model.model} · ${variantTitle(entry)}`} /> model</button></div>
              <details><summary>Complete native variant status and qualification</summary><pre><Text value={JSON.stringify(entry, null, 2)} /></pre></details>
            </div>)}
            <details><summary>Complete native model status</summary><pre><Text value={JSON.stringify(model, null, 2)} /></pre></details>
          </article>;
        })}
        <h4>Choose which indexes {title} may read</h4>
        {!draft.configuration.indexes.length ? <p>No indexes are enrolled in this stage. Enablement never enrolls indexes automatically.</p> : draft.configuration.indexes.map(entry => <div class="row" key={entry.index_id}><Text value={entry.index_id} /><span><Text value={entry.tasks.map(taskTitle).join(', ')} /></span>
          <button type="button" disabled={!view.canSend} onClick={() => workspace.removeIndex(stage, entry.index_id)}>Remove {title} enrollment</button></div>)}
        <Field label={`${title}: immutable index ID from Index library`} value={draft.index} onInput={value => workspace.indexInput(stage, value)} invalid={draft.index !== '' && !isCatalogId('idx_', draft.index)} />
        {tasks.map(task => <label key={task}><input type="checkbox" checked={draft.tasks.includes(task)} disabled={!view.canSend} onChange={event => workspace.taskInput(stage, task, event.currentTarget.checked)} /><Text value={taskTitle(task)} /></label>)}
        <button type="button" class="secondary" disabled={!view.canSend || !isCatalogId('idx_', draft.index) || !draft.tasks.length || draft.configuration.indexes.some(entry => entry.index_id === draft.index)} onClick={() => workspace.addIndex(stage)}>Add explicit {title} enrollment to draft</button>
        {!validEnabled ? <p role="status">Before enabling this stage, select a model with native admission and explicitly enroll at least one index. An unavailable model can be saved while this stage is off. Turning it off does not change the other stage.</p> : null}
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
