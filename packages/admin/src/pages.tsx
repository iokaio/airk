// SPDX-License-Identifier: Apache-2.0
// Admin pages (split plan §7.3). Each page calls only admin-surface operations and
// shows complete native results as text.
import type { JSX } from 'preact';
import { useEffect, useState } from 'preact/hooks';
import { IndexSelect, useIndexes } from './activity';
import { reviewedCall } from './api';
import { body, isDigest, isIncarnation, MAX_INDEX_LIST_LIMIT, MAX_PAGE_LIMIT, OPS } from './operations';
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
} from './render';
import { ProgressPanel } from './progress';
import { ConcurrencyNote, useQueryConcurrency } from './concurrencyNote';
import { concurrencyNote, typedScopeCount } from './scopeConcurrency';

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

export function Service(props: { onLifecycle: (action: 'stop' | 'restart') => void }): JSX.Element {
  const [resources, runResources] = useCall();
  const [trust, runTrust] = useCall();
  const [rotation, runRotation] = useCall();
  const [lifecycle, runLifecycle] = useCall();
  const lifecycleCall = async (action: 'stop' | 'restart'): Promise<void> => {
    const result =
      action === 'stop'
        ? await runLifecycle(OPS.serviceStop, body.serviceStop())
        : await runLifecycle(OPS.serviceRestart, body.serviceRestart());
    if (result.kind === 'ok' && result.envelope.request?.state !== 'refused') props.onLifecycle(action);
  };
  return (
    <>
      <Panel title="Resources">
        <button type="button" onClick={() => void runResources(OPS.resources, body.resources())}>
          Load
        </button>
        <Outcome state={resources} />
      </Panel>
      <Panel title="HTTPS issuer">
        <div class="row">
          <button type="button" onClick={() => void runTrust(OPS.trustStatus, body.trustStatus())}>
            Trust status
          </button>
          <button type="button" onClick={() => void runRotation(OPS.rotationStatus, body.rotationStatus())}>
            Rotation status
          </button>
        </div>
        <Outcome state={trust} />
        <Outcome state={rotation} />
        <p>Changing OS trust is never done over HTTP. Run on this machine:</p>
        <Command text="airkctl service trust inspect" />
        <Command text="airkctl service trust install --expected-ca-sha256 <issuer-sha256>" />
        <Command text="airkctl service trust remove --expected-ca-sha256 <issuer-sha256>" />
        <Command text="airkctl service issuer-rotation status" />
      </Panel>
      <LoginStartup />
      <Panel title="Restart or stop">
        <p>
          A restart or stop ends this admin session. After a stop, start the service again with{' '}
          <code>airk</code> or <code>airkctl service start</code>, then run <code>airkctl admin open</code>.
        </p>
        <div class="actions">
          <Confirm phrase="restart service" action="Restart" danger onConfirm={() => void lifecycleCall('restart')} />
          <Confirm phrase="stop service" action="Stop" danger onConfirm={() => void lifecycleCall('stop')} />
        </div>
        <Outcome state={lifecycle} />
      </Panel>
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

export function Search(): JSX.Element {
  const [catalog, runCatalog] = useCall();
  const [scope, runScope] = useCall();
  const [search, runSearch] = useCall();
  const [namespace, setNamespace] = useState('');
  const [revision, setRevision] = useState('');
  const [indexIds, setIndexIds] = useState('');
  const [projectIds, setProjectIds] = useState('');
  const [tool, setTool] = useState('airk_search');
  const [query, setQuery] = useState('');
  const [limit, setLimit] = useState(10);
  const [extra, setExtra] = useState('');
  const [mode, setMode] = useState<'terse' | 'full'>('full');
  const revisionNumber = /^\d+$/.test(revision) ? Number.parseInt(revision, 10) : null;
  const scopeValid =
    isIncarnation(namespace) && revisionNumber !== null && list(indexIds).length + list(projectIds).length > 0;
  // (U108) Informational only: the selection can be used whatever the note says.
  const concurrency = useQueryConcurrency();
  const typed = typedScopeCount(list(indexIds), list(projectIds));
  const scopeNote = concurrencyNote({ indexes: typed.count, atLeast: !typed.exact, concurrency, subject: 'scope' });
  let extraArgs: Record<string, unknown> | null = {};
  if (extra.trim() !== '') {
    try {
      const parsed: unknown = JSON.parse(extra);
      extraArgs =
        typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)
          ? (parsed as Record<string, unknown>)
          : null;
    } catch {
      extraArgs = null;
    }
  }
  const args = (): Record<string, unknown> => ({ ...(extraArgs ?? {}), ...(query !== '' ? { query } : {}), limit });
  return (
    <>
      <Panel title="1. Choose indexes">
        <p>
          This selection belongs to this browser session only. It does not change any editor window's scope
          or any agent connection's grant.
        </p>
        <button
          type="button"
          class="secondary"
          onClick={() => void runCatalog(OPS.catalogPage, body.catalogIndexes(null))}
        >
          List catalog indexes
        </button>
        <Outcome state={catalog} />
        <div class="row">
          <Field
            label="Namespace"
            value={namespace}
            invalid={namespace !== '' && !isIncarnation(namespace)}
            onInput={setNamespace}
          />
          <Field
            label="Expected catalog revision"
            value={revision}
            invalid={revision !== '' && revisionNumber === null}
            onInput={setRevision}
          />
        </div>
        <div class="row">
          <Field label="Index ids" wide value={indexIds} placeholder="idx_…" onInput={setIndexIds} />
          <Field label="Project ids" wide value={projectIds} placeholder="prj_…" onInput={setProjectIds} />
        </div>
        <button
          type="button"
          disabled={!scopeValid}
          onClick={() =>
            void runScope(
              OPS.searchSelectScope,
              body.searchSelectScope(namespace, revisionNumber ?? 0, list(indexIds), list(projectIds)),
            )
          }
        >
          Use this selection
        </button>
        <ConcurrencyNote text={scopeNote} />
        <Outcome state={scope} />
      </Panel>
      <Panel title="2. Search">
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (extraArgs === null || tool === '') return;
            void runSearch(OPS.searchCall, body.searchCall(tool, args(), mode));
          }}
        >
          <div class="row">
            <Field label="Tool" value={tool} onInput={setTool} />
            <Field label="Query" wide value={query} onInput={setQuery} />
            <NumberField label="Limit" value={limit} min={1} max={MAX_PAGE_LIMIT} onInput={setLimit} />
            <label class="field narrow">
              Response
              <select value={mode} onChange={(event) => setMode(event.currentTarget.value === 'terse' ? 'terse' : 'full')}>
                <option value="full">full</option>
                <option value="terse">terse</option>
              </select>
            </label>
          </div>
          <label class="field wide">
            Other arguments (JSON object, optional)
            <textarea
              rows={3}
              spellcheck={false}
              class={extraArgs === null ? 'invalid' : ''}
              value={extra}
              onInput={(event) => setExtra(event.currentTarget.value)}
            />
          </label>
          <button type="submit" disabled={extraArgs === null || tool === ''}>
            Search
          </button>
        </form>
        <p class="muted">Results are indexed evidence and are shown as plain text.</p>
        <Outcome state={search} />
      </Panel>
    </>
  );
}
