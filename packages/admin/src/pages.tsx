// SPDX-License-Identifier: Apache-2.0
// Admin pages (split plan §7.3). Each page calls only admin-surface operations and
// shows complete native results as text.
import type { JSX } from 'preact';
import { useEffect, useState } from 'preact/hooks';
import { body, isDigest, isIncarnation, MAX_PAGE_LIMIT, OPS } from './operations';
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

const short = (id: string): string => id.slice(0, 12);

export function Overview(): JSX.Element {
  const [health, runHealth] = useCall();
  const [indexes, runIndexes] = useCall();
  const [workers, runWorkers] = useCall();
  const [after, setAfter] = useState('');
  const [limit, setLimit] = useState(50);
  const [projects, setProjects] = useState('');
  useEffect(() => {
    void runHealth(OPS.health, body.health());
    void runIndexes(OPS.indexesList, body.indexesList(null, 50));
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
      <Panel title="Indexes">
        <div class="row">
          <Field
            label="After binding (optional)"
            value={after}
            invalid={after !== '' && !isDigest(after)}
            onInput={setAfter}
          />
          <NumberField label="Limit" value={limit} min={1} max={MAX_PAGE_LIMIT} onInput={setLimit} />
          <button
            type="button"
            disabled={after !== '' && !isDigest(after)}
            onClick={() => void runIndexes(OPS.indexesList, body.indexesList(after === '' ? null : after, limit))}
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

export function Activity(): JSX.Element {
  const [events, runEvents] = useCall();
  const [requests, runRequests] = useCall();
  const [job, runJob] = useCall();
  const [jobAction, runJobAction] = useCall();
  const [pause, runPause] = useCall();
  const [eventProject, setEventProject] = useState('');
  const [eventLimit, setEventLimit] = useState(50);
  const [jobId, setJobId] = useState('');
  const [pauseProject, setPauseProject] = useState('');
  const eventProjectValid = eventProject === '' || isDigest(eventProject);
  const loadEvents = (cursor: unknown): void =>
    void runEvents(OPS.eventsPage, body.eventsPage(eventProject === '' ? null : eventProject, cursor, eventLimit));
  const page = resultOf(events);
  const next =
    typeof page === 'object' && page !== null ? (page as Record<string, unknown>).next : undefined;
  const jobValid = isDigest(jobId);
  const pauseValid = isDigest(pauseProject);
  return (
    <>
      <Panel title="Events">
        <div class="row">
          <Field
            label="Project binding (optional)"
            value={eventProject}
            invalid={!eventProjectValid}
            onInput={setEventProject}
          />
          <NumberField label="Limit" value={eventLimit} min={1} max={MAX_PAGE_LIMIT} onInput={setEventLimit} />
          <button type="button" disabled={!eventProjectValid} onClick={() => loadEvents(null)}>
            Load latest
          </button>
          <button
            type="button"
            class="secondary"
            disabled={!eventProjectValid || next === undefined || next === null}
            onClick={() => loadEvents(next)}
          >
            Next page
          </button>
        </div>
        <Outcome state={events} />
      </Panel>
      <Panel title="Saved requests">
        <button type="button" onClick={() => void runRequests(OPS.requestsList, body.requestsList())}>
          Load
        </button>
        <Outcome state={requests} />
      </Panel>
      <Panel title="Job">
        <div class="row">
          <Field label="Job id" wide value={jobId} invalid={jobId !== '' && !jobValid} onInput={setJobId} />
          <button type="button" disabled={!jobValid} onClick={() => void runJob(OPS.jobsGet, body.jobsGet(jobId))}>
            Load
          </button>
        </div>
        <Outcome state={job} />
        {jobValid ? (
          <div class="actions">
            <Confirm
              phrase={`retry ${short(jobId)}`}
              action="Retry job"
              onConfirm={() => void runJobAction(OPS.jobsRetry, body.jobsRetry(jobId))}
            />
            <Confirm
              phrase={`cancel ${short(jobId)}`}
              action="Cancel job"
              danger
              onConfirm={() => void runJobAction(OPS.jobsCancel, body.jobsCancel(jobId))}
            />
          </div>
        ) : null}
        <Outcome state={jobAction} />
      </Panel>
      <Panel title="Pause or resume an index">
        <Field
          label="Project binding"
          wide
          value={pauseProject}
          invalid={pauseProject !== '' && !pauseValid}
          onInput={setPauseProject}
        />
        {pauseValid ? (
          <div class="actions">
            <Confirm
              phrase={`pause ${short(pauseProject)}`}
              action="Pause"
              onConfirm={() => void runPause(OPS.indexesPause, body.indexesPause(pauseProject, true))}
            />
            <Confirm
              phrase={`resume ${short(pauseProject)}`}
              action="Resume"
              onConfirm={() => void runPause(OPS.indexesPause, body.indexesPause(pauseProject, false))}
            />
          </div>
        ) : null}
        <Outcome state={pause} />
      </Panel>
    </>
  );
}

export function Agents(): JSX.Element {
  const [info, runInfo] = useCall();
  const [ownership, runOwnership] = useCall();
  const [revoke, runRevoke] = useCall();
  const [id, setId] = useState('');
  const valid = isIncarnation(id);
  const shown = valid ? id : '<connection-id>';
  return (
    <>
      <Panel title="Connection">
        <div class="row">
          <Field label="Connection id" wide value={id} invalid={id !== '' && !valid} onInput={setId} />
          <button
            type="button"
            disabled={!valid}
            onClick={() => void runInfo(OPS.agentsConnection, body.agentsConnection(id))}
          >
            Connection info
          </button>
          <button
            type="button"
            disabled={!valid}
            onClick={() => void runOwnership(OPS.agentsOwnership, body.agentsOwnership(id))}
          >
            Ownership
          </button>
        </div>
        <Outcome state={info} />
        <Outcome state={ownership} />
      </Panel>
      <Panel title="Revoke">
        <p>
          Revoking ends this connection's credential. Agent configuration files are not changed; use the
          commands below for that.
        </p>
        {valid ? (
          <Confirm
            phrase={`revoke ${id}`}
            action="Revoke connection"
            danger
            onConfirm={() => void runRevoke(OPS.agentsRevoke, body.agentsRevoke(id))}
          />
        ) : (
          <p class="muted">Enter a connection id first.</p>
        )}
        <Outcome state={revoke} />
      </Panel>
      <Panel title="Change agent configuration">
        <p>
          Connecting creates credentials and writes agent configuration, so it is never done over HTTP. Run
          these on this machine:
        </p>
        <Command text="airkctl agents list" />
        <Command text="airkctl agents connect --help" />
        <Command text={`airkctl agents repair --connection-id ${shown}`} />
        <Command text={`airkctl agents remove --connection-id ${shown}`} />
        <Command text={`airkctl agents verify --connection-id ${shown}`} />
      </Panel>
    </>
  );
}

export function Storage(): JSX.Element {
  const [inventory, runInventory] = useCall();
  return (
    <Panel title="Storage inventory">
      <button
        type="button"
        onClick={() => void runInventory(OPS.storageInventory, body.storageInventory())}
      >
        Load
      </button>
      <Outcome state={inventory} />
    </Panel>
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

export function Help(): JSX.Element {
  const [catalog, runCatalog] = useCall();
  const [toolHelp, runToolHelp] = useCall();
  const [topic, runTopic] = useCall();
  const [tool, setTool] = useState('airk_search');
  const [topicName, setTopicName] = useState('');
  useEffect(() => {
    void runCatalog(OPS.toolsCatalog, body.toolsList());
  }, []);
  return (
    <>
      <Panel title="Tools">
        <Outcome state={catalog} />
        <div class="row">
          <Field label="Tool" value={tool} onInput={setTool} />
          <button
            type="button"
            disabled={tool === ''}
            onClick={() => void runToolHelp(OPS.toolsCatalog, body.toolsHelp(tool))}
          >
            Tool help
          </button>
        </div>
        <Outcome state={toolHelp} />
      </Panel>
      <Panel title="Help topics">
        <div class="row">
          <Field label="Topic (empty for the index)" value={topicName} onInput={setTopicName} />
          <button type="button" onClick={() => void runTopic(OPS.searchCall, body.helpTopic(topicName))}>
            Show
          </button>
        </div>
        <Outcome state={topic} />
      </Panel>
    </>
  );
}
