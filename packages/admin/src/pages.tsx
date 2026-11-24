// SPDX-License-Identifier: Apache-2.0
// Admin pages (split plan §7.3). Each page calls only admin-surface operations and
// shows complete native results as text.
import type { JSX } from 'preact';
import { useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';
import { IndexSelect, useIndexes } from './activity';
import { isUnknownOutcome, reviewedCall } from './api';
import { body, isDigest, MAX_INDEX_LIST_LIMIT, MAX_PAGE_LIMIT, OPS } from './operations';
import {
  bindingsIn,
  Command,
  Confirm,
  Field,
  list,
  NumberField,
  Outcome,
  Panel,
  resultOf,
  useCall,
  type CallState,
} from './render';
import { ProgressPanel } from './progress';
import { generatedAction } from './generatedActions';
import { currentSession } from './session';
import { useContext } from 'preact/hooks';
import { PageVisible } from './pageVisibility';
import type { CallResult } from './api';
import type { GpuOptIn } from '../../contracts-gen/i3';

/** U103: live progress of one registered index, chosen from the catalog names. */
function OverviewProgress(): JSX.Element {
  const names = useIndexes();
  const usable = names.indexes.filter((index) => index.registered && !index.retired);
  const [binding, setBinding] = useState('');
  useEffect(() => {
    void names.reload();
  }, []);
  const chosen = usable.some((index) => index.binding === binding) ? binding : (usable[0]?.binding ?? '');
  const loading = names.catalog.phase === 'pending' || names.projects.phase === 'pending';
  return (
    <Panel title="Indexing progress">
      <div class="row">
        <IndexSelect label="Index" indexes={usable} value={chosen} onChange={setBinding} />
        <button type="button" class="secondary" disabled={loading} onClick={() => void names.reload()}>
          Reload index list
        </button>
      </div>
      {names.partial ? (
        <p class="muted">Not every index could be listed; the list may be incomplete.</p>
      ) : null}
      {chosen === '' ? (
        <p class="muted">{loading ? 'Loading indexes…' : 'No registered index to show.'}</p>
      ) : (
        <ProgressPanel binding={chosen} />
      )}
    </Panel>
  );
}

export function Overview(): JSX.Element {
  const [health, runHealth] = useCall();
  const [indexes, runIndexes] = useCall();
  const [workers, runWorkers] = useCall();
  const [after, setAfter] = useState('');
  const [limit, setLimit] = useState(MAX_INDEX_LIST_LIMIT);
  const [projects, setProjects] = useState('');
  useEffect(() => {
    void runHealth(OPS.health, body.health());
    void runIndexes(OPS.indexesList, body.indexesList(null, MAX_INDEX_LIST_LIMIT));
  }, []);
  const projectList = list(projects);
  const projectsValid = projectList.every(isDigest);
  return (
    <>
      <Panel title="Service health">
        <button type="button" onClick={() => void runHealth(OPS.health, body.health())}>
          Refresh
        </button>
        <Outcome state={health} />
      </Panel>
      <OverviewProgress />
      <Panel title="Indexes">
        <div class="row">
          <Field
            label="After binding (optional)"
            value={after}
            invalid={after !== '' && !isDigest(after)}
            onInput={setAfter}
          />
          <NumberField label="Limit" value={limit} min={1} max={MAX_INDEX_LIST_LIMIT} onInput={setLimit} />
          <button
            type="button"
            disabled={after !== '' && !isDigest(after)}
            onClick={() => {
              // A continuation must name the revision of the page it continues.
              const page = (resultOf(indexes) as { page?: { revision?: number } } | undefined)?.page;
              void runIndexes(
                OPS.indexesList,
                body.indexesList(after === '' ? null : after, limit, after === '' ? null : page?.revision ?? null),
              );
            }}
          >
            Load
          </button>
        </div>
        <Outcome state={indexes} />
      </Panel>
      <Panel title="Workers">
        <div class="row">
          <Field
            label="Project bindings"
            wide
            value={projects}
            invalid={!projectsValid}
            onInput={setProjects}
          />
          <button
            type="button"
            class="secondary"
            disabled={resultOf(indexes) === undefined}
            onClick={() => setProjects(bindingsIn(resultOf(indexes)).join(' '))}
          >
            Use listed indexes
          </button>
          <button
            type="button"
            disabled={!projectsValid}
            onClick={() => void runWorkers(OPS.workers, body.workers(projectList))}
          >
            Load
          </button>
        </div>
        <Outcome state={workers} />
      </Panel>
    </>
  );
}

/** Field `name` of an object result, or undefined. */
function field(value: unknown, name: string): unknown {
  return value !== null && typeof value === 'object' ? (value as Record<string, unknown>)[name] : undefined;
}

/** One purgeable location (U100): plan, read every file, type the path, purge. */
function PurgeLocation(props: { kind: 'cache' | 'logs'; title: string }): JSX.Element {
  const [plan, runPlan] = useCall();
  const [purge, runPurge] = useCall();
  const [typed, setTyped] = useState('');
  const result = plan.phase === 'done' && plan.result.kind === 'ok' ? plan.result.envelope.result : undefined;
  const digest = field(result, 'plan_digest');
  const path = field(field(result, 'plan'), 'path');
  const ready = typeof digest === 'string' && isDigest(digest) && typeof path === 'string';
  return (
    <Panel title={props.title}>
      <p>
        {props.kind === 'cache'
          ? 'Deletes every file in the service cache; the service re-creates what it needs.'
          : 'Deletes closed log files only; the log the service is writing stays.'}{' '}
        Nothing is deleted until you review the plan and type the location&apos;s path.
      </p>
      <button type="button" onClick={() => void runPlan(OPS.storagePurgePlan, body.storagePurgePlan(props.kind))}>
        Review purge plan
      </button>
      <Outcome state={plan} />
      {ready ? (
        <div class="row">
          <Field label={`Type ${String(path)} to confirm`} wide value={typed} onInput={setTyped} />
          <button
            type="button"
            class="danger"
            disabled={typed !== path}
            onClick={() =>
              void runPurge(
                OPS.storagePurge,
                reviewedCall(body.storagePurge(props.kind, String(digest)), null, typed),
              )
            }
          >
            Purge exactly this plan
          </button>
        </div>
      ) : null}
      <Outcome state={purge} />
    </Panel>
  );
}

export function Storage(): JSX.Element {
  const [inventory, runInventory] = useCall();
  return (
    <>
      <Panel title="Storage inventory">
        <p>The service&apos;s locations with their sizes. Indexes are removed in the Index library, never here.</p>
        <button
          type="button"
          onClick={() => void runInventory(OPS.storageInventory, body.storageInventory())}
        >
          Load
        </button>
        <Outcome state={inventory} />
      </Panel>
      <PurgeLocation kind="cache" title="Clean up the cache" />
      <PurgeLocation kind="logs" title="Clean up closed logs" />
    </>
  );
}

export function Recovery(): JSX.Element {
  const [plan, runPlan] = useCall();
  const [items, runItems] = useCall();
  const [id, setId] = useState('');
  const [after, setAfter] = useState('');
  const [limit, setLimit] = useState(50);
  const valid = isDigest(id);
  const afterValid = after === '' || isDigest(after);
  return (
    <Panel title="Repair plan">
      <div class="row">
        <Field label="Plan id" wide value={id} invalid={id !== '' && !valid} onInput={setId} />
        <button type="button" disabled={!valid} onClick={() => void runPlan(OPS.recoveryPlan, body.recoveryPlan(id))}>
          Load plan
        </button>
      </div>
      <Outcome state={plan} />
      <div class="row">
        <Field label="Items after binding (optional)" value={after} invalid={!afterValid} onInput={setAfter} />
        <NumberField label="Limit" value={limit} min={1} max={MAX_PAGE_LIMIT} onInput={setLimit} />
        <button
          type="button"
          disabled={!valid || !afterValid}
          onClick={() =>
            void runItems(OPS.recoveryPlanItems, body.recoveryPlanItems(id, after === '' ? null : after, limit))
          }
        >
          Load items
        </button>
      </div>
      <Outcome state={items} />
    </Panel>
  );
}

/** U101: start the service when you sign in to Windows. */
function LoginStartup(): JSX.Element {
  const [login, runLogin] = useCall();
  return (
    <Panel title="Start at sign-in">
      <p>
        Starts AIRK Service when you sign in to Windows. Removing it while the service was started at sign-in is
        refused (<code>login_startup_in_use</code>); stop the service and use <code>airkctl service login uninstall</code>.
        Other platforms report that this is not managed here.
      </p>
      <div class="row">
        <button type="button" onClick={() => void runLogin(OPS.loginStatus, body.loginStartup('status'))}>
          Status
        </button>
        <button type="button" onClick={() => void runLogin(OPS.loginInstall, body.loginStartup('install'))}>
          Turn on
        </button>
        <button type="button" onClick={() => void runLogin(OPS.loginUninstall, body.loginStartup('uninstall'))}>
          Turn off
        </button>
      </div>
      <Outcome state={login} />
    </Panel>
  );
}

/** Only a current persisted resource configuration can authorize editing. */
export function gpuAllowedOf(value: unknown): boolean | null {
  if (field(value, 'status') !== 'resources' || field(value, 'configuration_error') !== false) return null;
  const configured = field(field(field(value, 'observation'), 'configuration'), 'allow_gpu');
  return typeof configured === 'boolean' ? configured : null;
}

/** The reset record's wire value, checked against the type generated from the Rust owner. */
const GPU_DEFAULT_RESET: GpuOptIn = 'default_reset';

/**
 * True while the service reports that GPU use stored as enabled under the earlier default was
 * reset to off and the user has not answered. It reads the same current configuration as
 * `gpuAllowedOf`; any other result shows no notice.
 */
export function gpuResetPendingOf(value: unknown): boolean {
  return gpuAllowedOf(value) === false
    && field(field(field(value, 'observation'), 'configuration'), 'gpu_opt_in') === GPU_DEFAULT_RESET;
}

/** Presentation-only native field projection; the complete ResourceSnapshot stays in Details. */
export function resourceSummaryOf(value: unknown): readonly [string, string][] {
  if (field(value, 'status') !== 'resources') return [];
  const snapshot = field(value, 'observation'), configuration = field(snapshot, 'configuration');
  const count = (value: unknown): string => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? String(value) : 'Unavailable';
  const quantity = (name: string): string => count(field(snapshot, name));
  const bytes = (value: unknown): string => {
    if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) return 'Unavailable';
    const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB']; let size = value, unit = 0;
    while (size >= 1024 && unit < units.length - 1) { size /= 1024; unit++; }
    return `${unit ? size.toFixed(1) : size} ${units[unit]}`;
  };
  const memory = (name: string): string => bytes(field(snapshot, name));
  const readiness = field(snapshot, 'provider_drain_error') === true ? 'Provider recovery needs attention'
    : field(snapshot, 'pressure') === true ? 'Memory pressure: background work may wait'
    : field(snapshot, 'telemetry_complete') === true && field(snapshot, 'pressure') === false && field(snapshot, 'provider_drain_error') === false ? 'Current resource observations available'
    : 'Resource observations incomplete or unavailable';
  return [
    ['Concurrent evidence requests', count(field(configuration, 'query_concurrency'))],
    ['CPU and workers', `${quantity('logical_cpus')} logical CPUs; ${quantity('resident_workers')} of ${quantity('resident_limit')} resident worker slots`],
    ['Service memory in use', memory('observed_rss_bytes')],
    ['System memory available', `${memory('available_memory_bytes')} available; ${memory('detected_memory_bytes')} detected total`],
    ['Configured memory reservation', `${memory('memory_reserved_bytes')} reserved of ${memory('memory_limit_bytes')}; ${memory('query_headroom_bytes')} query headroom`],
    ['Work', `${quantity('running_jobs')} running jobs; ${quantity('waiting_projects')} waiting indexes`],
    ['Resources', readiness],
  ];
}

function ResourceSummary(props: { state: CallState }): JSX.Element {
  if (props.state.phase === 'pending') return <p role="status">Reading current CPU, memory and request capacity...</p>;
  const rows = resourceSummaryOf(resultOf(props.state));
  if (!rows.length) return <p role="status">Resource status is unavailable. Refresh resources to try again; Details retains the complete outcome.</p>;
  return <><dl class="summary">{rows.map(([label, value]) => <><dt>{label}</dt><dd>{value}</dd></>)}</dl>
    <p class="muted">Reservations are service policy limits. They do not establish a model's measured memory use or qualification.</p></>;
}

export function Service(props: { onLifecycle: (action: 'stop' | 'restart') => void }): JSX.Element {
  const visible = useContext(PageVisible);
  const [resources, runResources] = useCall();
  const [gpuOutcome, runGpu] = useCall();
  const [outcomeSession, setOutcomeSession] = useState<ReturnType<typeof currentSession>>(null);
  const [observedSession, setObservedSession] = useState<ReturnType<typeof currentSession>>(null);
  const [gpuReview, setGpuReview] = useState<{ observation: CallState; desired: boolean } | null>(null);
  const readNumber = useRef(0);
  const gpuBusy = useRef(false);
  const refreshResources = async () => {
    const origin = currentSession();
    const ticket = ++readNumber.current;
    setObservedSession(null);
    setGpuReview(null);
    await runResources(OPS.resources, body.resources());
    if (ticket === readNumber.current && origin === currentSession()) setObservedSession(origin);
  };
  const [lifecycle, runLifecycle] = useCall();
  const [, changed] = useState(0);
  const session = currentSession();
  useEffect(() => { if (visible && session !== null) void refreshResources(); }, [visible, session]);
  const allowed = session && session === observedSession ? gpuAllowedOf(resultOf(resources)) : null;
  const resetPending = session !== null && session === observedSession && gpuResetPendingOf(resultOf(resources));
  // A review belongs to the observation it was opened on. The toggle can only invert the value in
  // effect. Keeping GPU use off is a request only while the reset notice is pending: the service
  // takes it as the user's answer and clears the notice without a GPU transition.
  const reviewedChoice = gpuReview?.observation === resources && allowed !== null
    && (gpuReview.desired === !allowed || (resetPending && !gpuReview.desired)) ? gpuReview.desired : null;
  const applyGpu = async () => {
    if (reviewedChoice === null || !visible || currentSession() !== observedSession || gpuBusy.current) return;
    gpuBusy.current = true;
    setOutcomeSession(observedSession);
    setGpuReview(null);
    try {
      await runGpu(OPS.allowGpu, body.allowGpu({ allow_gpu: reviewedChoice }));
    } finally {
      gpuBusy.current = false;
      if (session === currentSession()) await refreshResources();
    }
  };
  const lifetime = useRef<{ session: unknown; owner: ReturnType<typeof generatedAction> | null } | null>(null);
  if (lifetime.current?.session !== session) {
    lifetime.current?.owner?.dispose();
    lifetime.current = { session, owner: session === null ? null : generatedAction(session, 'service.lifecycle', () => changed(value => value + 1)) };
  }
  const owner = lifetime.current?.owner ?? null;
  useLayoutEffect(() => () => owner?.dispose(), [owner]);
  const retained = owner?.snapshot();
  const lifecycleState = retained?.intent && (retained.pending || retained.result !== null)
    ? retained.pending ? { phase: 'pending' as const, operation: retained.intent.operation }
      : { phase: 'done' as const, operation: retained.intent.operation, result: retained.result!, at: retained.at }
    : lifecycle;
  const blocked = retained?.pending === true || retained?.unknown === true || retained?.capacity === true;
  const lifecycleCall = async (action: 'stop' | 'restart'): Promise<void> => {
    if (session === null || session !== currentSession() || !owner || lifetime.current?.owner !== owner) return;
    const intent = owner.begin(action === 'stop' ? OPS.serviceStop : OPS.serviceRestart, action === 'stop' ? body.serviceStop() : body.serviceRestart());
    if (!intent) return;
    let result: CallResult;
    try { result = await runLifecycle(intent.operation, intent.method); }
    catch { result = { kind: 'transport', status: null, message: 'The original lifecycle request did not return a usable outcome. Inspect the service status before another lifecycle action.' }; }
    owner.settle(intent, result, Date.now());
    if (session === currentSession() && result.kind === 'ok' && !isUnknownOutcome(result) && result.envelope.request?.state !== 'refused') props.onLifecycle(action);
  };
  return (
    <>
      <p>Check current resources, manage optional Arbiter models, or choose a service task below.</p>
      <Panel title="Resources">
        <button type="button" onClick={() => void refreshResources()}>
          {resources.phase === 'pending' ? 'Reading resources...' : 'Refresh resources'}
        </button>
        {resources.phase === 'done' ? <p class="muted">Last observed {new Date(resources.at).toLocaleTimeString()}. Refresh after changing models or service settings.</p> : null}
        <ResourceSummary state={resources} />
        <p>Service-wide GPU use: <strong>{allowed === null ? 'Unavailable' : allowed ? 'Allowed' : 'Disabled'}</strong></p>
        {resetPending ? <p role="status">GPU use was reset to off. An earlier AIRK version allowed GPU use by default, and
          this service stored that setting. GPU use is now opt-in, so it was turned off once. Choose Allow GPU use to
          turn it on, or Keep GPU off to leave it off. Either answer clears this notice.</p> : null}
        {allowed !== null && (reviewedChoice !== null ? <div>
          <p>{reviewedChoice ? 'Allow this service to use an eligible GPU?'
            : allowed ? 'Disable GPU use for this service?'
            : 'Keep GPU use off for this service? This records your answer and clears the notice.'}</p>
          <button type="button" disabled={gpuBusy.current || !visible} onClick={() => void applyGpu()}>Confirm change</button>
          <button type="button" onClick={() => setGpuReview(null)}>Cancel</button>
        </div> : <>
          <button type="button" disabled={gpuBusy.current || !visible}
            onClick={() => setGpuReview({ observation: resources, desired: !allowed })}>
            {allowed ? 'Disable GPU use' : 'Allow GPU use'}
          </button>
          {resetPending ? <button type="button" disabled={gpuBusy.current || !visible}
            onClick={() => setGpuReview({ observation: resources, desired: false })}>
            Keep GPU off
          </button> : null}
        </>)}
        {outcomeSession === session && gpuOutcome.phase !== 'idle' && <details open><summary>GPU setting result</summary><Outcome state={gpuOutcome} /></details>}
        <details><summary>Details: complete resource result</summary><Outcome state={resources} /></details>
      </Panel>
      <details><summary>Start AIRK when I sign in</summary>
      <LoginStartup />
      </details>
      <details><summary>Restart or stop AIRK</summary>
      <Panel title="Stop or prepare to restart">
        <p>
          Both actions stop the service and end this management session. Stop for restart records the restart
          request; this page does not start a replacement service. After it stops, run{' '}
          <code>airkctl service start</code> locally. Then run <code>airkctl admin open</code> for a new browser
          session, or reopen management in the editor after verifying the service connection.
        </p>
        <div class="actions">
          <Confirm phrase="restart service" action="Stop for restart" danger disabled={blocked} onConfirm={() => void lifecycleCall('restart')} />
          <Confirm phrase="stop service" action="Stop" danger disabled={blocked} onConfirm={() => void lifecycleCall('stop')} />
        </div>
        {retained?.unknown ? <p class="error" role="alert">The original lifecycle outcome is unresolved. Another Stop or Restart is blocked. Inspect service status and Activity; the received outcome is retained below.</p>
          : retained?.capacity ? <p role="status">This session holds the limit of unresolved generated actions. Inspect saved requests before another lifecycle action.</p> : null}
        <Outcome state={lifecycleState} />
      </Panel>
      </details>
    </>
  );
}

export function Logs(): JSX.Element {
  const [logs, runLogs] = useCall();
  const [limit, setLimit] = useState(200);
  return (
    <Panel title="Recent service logs">
      <div class="row">
        <NumberField label="Limit" value={limit} min={1} max={MAX_PAGE_LIMIT} onInput={setLimit} />
        <button type="button" onClick={() => void runLogs(OPS.logsRecent, body.logsRecent(limit))}>
          Load
        </button>
      </div>
      <Outcome state={logs} />
      <p>Complete and raw logs are local reads:</p>
      <Command text="airkctl service logs --all" />
    </Panel>
  );
}

export { Search } from './SearchScreen';
