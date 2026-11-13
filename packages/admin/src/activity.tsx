// SPDX-License-Identifier: Apache-2.0
// Activity page: the service event timeline, saved (journaled) requests, job
// details with retry/cancel, and pause/resume of an index.
//
// Data shapes (read defensively; each complete native result stays available
// under "Complete result (JSON)"):
// - events.page → `{status: "events", page: EventPage}`; events page forward
//   from `cursor.after` up to `through` (airk-core service_events.rs,
//   airk-store service/registry/events.rs). The newest events are read by
//   naming a cursor window just below `snapshot_through`; "Load older" names
//   the window below that one. Limit 1..=32.
// - jobs.get / jobs.retry / jobs.cancel → `{status: "job", job: JobRecord}`
//   (airk-core service_records/mod.rs).
// - requests.list → `{status: "client_requests", pending, capacity}`.
// - indexes.pause → `{status: "registered", project: RegisteredProject}`.
// - catalog.page(indexes) names indexes (`backing.binding` is the project
//   binding); indexes.list gives each registered project's `paused` flag.
import { createIndexRequests, createJobRequests, type IndexInfo } from './activityRequests';
export type { IndexInfo } from './activityRequests';
import type { ComponentChildren, JSX } from 'preact';
import { useEffect, useRef, useState } from 'preact/hooks';
import type { CallResult } from './api';
import { body, isDigest, OPS } from './operations';
import { Confirm, Field, Outcome, Panel, Text, useCall, type CallState } from './render';

type Rec = Record<string, unknown>;
const rec = (value: unknown): Rec | null =>
  typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as Rec) : null;
const str = (value: unknown): string | null => (typeof value === 'string' ? value : null);
const num = (value: unknown): number | null => (typeof value === 'number' && Number.isFinite(value) ? value : null);
const arr = (value: unknown): unknown[] => (Array.isArray(value) ? value : []);
const short = (id: string): string => id.slice(0, 12);
const okValue = (result: CallResult): Rec | null => (result.kind === 'ok' ? rec(result.envelope.result) : null);

/** EventRegistry::events accepts 1..=32. */
const EVENT_WINDOW = 32;

// ---------------------------------------------------------------------------
// Plain words for native codes.

const JOB_KIND: Record<string, string> = {
  inspect: 'Inspection',
  plan_inspect: 'Plan inspection',
  repair: 'Repair',
  index: 'Indexing',
  reconcile: 'Reconciliation',
  vectors: 'Vector build',
  model_setup: 'Model setup',
  configure: 'Configuration',
  verify: 'Verification',
  rebuild: 'Rebuild',
  recover: 'Recovery',
  maintain: 'Maintenance',
};
export const jobKind = (kind: unknown): string => JOB_KIND[str(kind) ?? ''] ?? str(kind) ?? 'Job';

export const JOB_STATE: Record<string, { text: string; tone: string }> = {
  pending: { text: 'Waiting', tone: 'warn' },
  running: { text: 'Running', tone: 'info' },
  succeeded: { text: 'Succeeded', tone: 'ok' },
  failed: { text: 'Failed', tone: 'danger' },
  canceled: { text: 'Canceled', tone: 'muted' },
};

/** EventKind (service_events.rs) as a phrase about the event's job or index. */
function eventPhrase(event: Rec): { text: string; tone: string } {
  const job = jobKind(event.job_kind).toLowerCase();
  switch (str(event.kind)) {
    case 'registered':
      return { text: 'Index registered with the service', tone: 'info' };
    case 'paused':
      return { text: 'Indexing paused', tone: 'warn' };
    case 'resumed':
      return { text: 'Indexing resumed', tone: 'ok' };
    case 'job_pending':
      return { text: `${capital(job)} queued`, tone: 'muted' };
    case 'job_running':
      return { text: `${capital(job)} started`, tone: 'info' };
    case 'job_succeeded':
      return { text: `${capital(job)} finished`, tone: 'ok' };
    case 'job_failed':
      return { text: `${capital(job)} failed`, tone: 'danger' };
    case 'job_canceled':
      return { text: `${capital(job)} canceled`, tone: 'muted' };
    case 'cancellation_requested':
      return { text: `Cancellation requested for ${job}`, tone: 'warn' };
    case 'job_retry_requested':
      return { text: `Retry requested for ${job}`, tone: 'info' };
    case 'watch_enabled':
      return { text: 'Watching for source changes turned on', tone: 'info' };
    case 'watch_disabled':
      return { text: 'Watching for source changes turned off', tone: 'warn' };
    default:
      return { text: str(event.kind)?.replace(/_/g, ' ') ?? 'Unknown event', tone: 'muted' };
  }
}
const capital = (text: string): string => text.charAt(0).toUpperCase() + text.slice(1);

/** Refusal codes of these operations (EventError, JobError, registry). */
const FAILURES: Record<string, string> = {
  event_cursor_expired: 'Those older events were removed from the retained log; load the latest events again.',
  event_scope_changed: 'The service restarted or the filter changed; load the latest events again.',
  invalid_event_cursor: 'The event position is no longer valid; load the latest events again.',
  invalid_event_page_limit: 'The page size must be between 1 and 32.',
  UnknownJob: 'The service has no job with that id. It may have been removed after it finished.',
  NotRetryable: 'This job cannot be retried in its current state.',
  KeyExpired: 'The retry request expired; try again.',
  KeyConflict: 'Another request already used this request key.',
  Capacity: 'The service job queue is full; try again later.',
  StaleWorker: 'The job belongs to an earlier worker generation.',
  registry_busy: 'The service registry is busy; try again in a moment.',
  invalid_registry_request: 'The service refused the request (for example, an unknown index).',
  data_root_scope_conflict: 'The index data folder belongs to a different scope.',
};

/** RequestProgress states (contracts-gen RequestState). */
const REQUEST_STATE: Record<string, string> = {
  submitted: 'submitted; the service has not confirmed it yet.',
  accepted: 'accepted by the service journal.',
  completed: 'completed.',
  refused: 'refused by the service.',
  unknown: 'outcome unknown. Check Saved requests before trying again.',
};

type Problem = { code: string; sentence: string };

function problemOf(result: CallResult): Problem | null {
  switch (result.kind) {
    case 'ok':
      return null;
    case 'no_session':
      return { code: 'no_session', sentence: 'No session. Open AIRK Admin again with airkctl admin open.' };
    case 'transport':
      return { code: 'transport', sentence: result.message };
    case 'failed': {
      // Non-snake native codes (JobError) arrive as `native_refusal`.
      const code = str(rec(result.envelope.error.native)?.code) ?? result.envelope.error.code;
      return { code, sentence: FAILURES[code] ?? 'The service refused the request.' };
    }
  }
}

export function ProblemLine(props: { state: CallState }): JSX.Element | null {
  if (props.state.phase !== 'done') return null;
  const problem = problemOf(props.state.result);
  if (problem === null) return null;
  return (
    <p class="error" role="alert">
      <code>
        <Text value={problem.code} />
      </code>{' '}
      <Text value={problem.sentence} />
    </p>
  );
}

/** The complete native result, collapsed (open on failure). */
export function Complete(props: { state: CallState; label?: string }): JSX.Element | null {
  if (props.state.phase !== 'done') return null;
  return (
    <details class="complete" open={props.state.result.kind !== 'ok'}>
      <summary>{props.label ?? 'Complete result (JSON)'}</summary>
      <Outcome state={props.state} />
    </details>
  );
}

export function Badge(props: { tone: string; children: ComponentChildren }): JSX.Element {
  return <span class={`badge badge-${props.tone}`}>{props.children}</span>;
}

function localTime(ms: unknown): string {
  const n = num(ms);
  return n === null ? '—' : new Date(n).toLocaleString();
}

// ---------------------------------------------------------------------------
// Index names: binding → name, from the catalog and the registered projects.



export function useIndexes(): {
  indexes: IndexInfo[];
  nameOf: (binding: string) => string;
  reload: () => Promise<void>;
  catalog: CallState;
  projects: CallState;
  partial: boolean;
  registryKnown: boolean;
} {
  const [catalog, runCatalog] = useCall();
  const [projects, runProjects] = useCall();
  const [indexes, setIndexes] = useState<IndexInfo[]>([]);
  const [partial, setPartial] = useState(false);
  const [registryKnown, setRegistryKnown] = useState(false);
  const indexRequests = createIndexRequests({ runCatalog, runProjects, setRegistryKnown, setPartial, setIndexes });
  const reload = indexRequests.reload;
  const nameOf = (binding: string): string => {
    const found = indexes.find((i) => i.binding === binding);
    return found?.name ?? `index ${short(binding)}`;
  };
  return { indexes, nameOf, reload, catalog, projects, partial, registryKnown };
}

export function IndexSelect(props: {
  label: string;
  indexes: IndexInfo[];
  value: string;
  allLabel?: string;
  onChange: (binding: string) => void;
}): JSX.Element {
  return (
    <label class="field">
      {props.label}
      <select value={props.value} onChange={(event) => props.onChange(event.currentTarget.value)}>
        {props.allLabel !== undefined ? <option value="">{props.allLabel}</option> : <option value="">Choose an index…</option>}
        {props.indexes.map((index) => (
          <option key={index.binding} value={index.binding}>
            {index.name ?? `Unnamed index ${short(index.binding)}`}
            {index.retired ? ' (retired)' : ''}
            {index.paused === true ? ' (paused)' : ''}
          </option>
        ))}
      </select>
    </label>
  );
}

// ---------------------------------------------------------------------------
// The page.

type Timeline = {
  namespace: string;
  project: string | null;
  floor: number; // oldest retained sequence - 1
  windowAfter: number; // the lowest `after` loaded so far
  newest: number;
  prefixLost: boolean;
  events: Rec[]; // newest first
};

export function Activity(): JSX.Element {
  const names = useIndexes();
  // Only registered, live indexes can filter events or be paused.
  const usable = names.indexes.filter((index) => index.registered && !index.retired);
  const [events, runEvents] = useCall();
  const [requests, runRequests] = useCall();
  const [job, runJob] = useCall();
  const [jobAction, runJobAction] = useCall();
  const [pause, runPause] = useCall();
  const [eventIndex, setEventIndex] = useState('');
  const [timeline, setTimeline] = useState<Timeline | null>(null);
  const [jobId, setJobId] = useState('');
  const [typedJob, setTypedJob] = useState('');
  const [pauseIndex, setPauseIndex] = useState('');
  const [jobRecord, setJobRecord] = useState<Rec | null>(null);

  /** Reads `(after, through]` for the filter; null on failure. */
  const readWindow = async (namespace: string, project: string | null, after: number, through: number) => {
    const cursor = { namespace, project, after, through };
    const page = rec(okValue(await runEvents(OPS.eventsPage, body.eventsPage(project, cursor, EVENT_WINDOW)))?.page);
    return page === null ? null : arr(page.events).map(rec).filter((e): e is Rec => e !== null);
  };

  // The newest timeline request wins; older answers are dropped.
  const timelineTicket = useRef(0);
  const loadLatest = async (project: string | null): Promise<void> => {
    const ticket = ++timelineTicket.current;
    setTimeline(null);
    // A one-event probe from the start gives the namespace and retained range.
    const probe = rec(okValue(await runEvents(OPS.eventsPage, body.eventsPage(project, null, 1)))?.page);
    if (probe === null) return;
    const namespace = str(probe.namespace) ?? '';
    const oldest = num(probe.oldest_sequence);
    const newest = num(probe.snapshot_through) ?? 0;
    const floor = oldest === null ? newest : Math.max(0, oldest - 1);
    const after = Math.max(floor, newest - EVENT_WINDOW);
    const got = newest > after ? await readWindow(namespace, project, after, newest) : [];
    if (got === null || ticket !== timelineTicket.current) return;
    setTimeline({
      namespace,
      project,
      floor,
      windowAfter: after,
      newest,
      prefixLost: probe.retained_prefix_lost === true,
      events: [...got].reverse(),
    });
  };

  const loadOlder = async (): Promise<void> => {
    if (timeline === null || timeline.windowAfter <= timeline.floor) return;
    const through = timeline.windowAfter;
    const after = Math.max(timeline.floor, through - EVENT_WINDOW);
    const ticket = timelineTicket.current;
    const got = await readWindow(timeline.namespace, timeline.project, after, through);
    if (got === null || ticket !== timelineTicket.current) return;
    // Append only to the timeline this page continues.
    setTimeline((current) =>
      current !== null && current.windowAfter === through && current.namespace === timeline.namespace
        ? { ...current, windowAfter: after, events: [...current.events, ...[...got].reverse()] }
        : current,
    );
  };

  const jobRequests = createJobRequests({ runJob, runJobAction, setJobId, setJobRecord });
  const openJob = jobRequests.openJob;

  const jobChange = (kind: 'retry' | 'cancel'): Promise<void> => jobRequests.jobChange(jobId, kind);

  useEffect(() => {
    void names.reload();
    void loadLatest(null);
    void runRequests(OPS.requestsList, body.requestsList());
  }, []);

  const pauseInfo = names.indexes.find((i) => i.binding === pauseIndex) ?? null;
  const pauseName = pauseInfo?.name ?? short(pauseIndex);
  const pausedNow = (() => {
    const project = rec(resultOk(pause)?.project);
    if (project !== null && str(rec(project.registration)?.binding) === pauseIndex) return project.paused === true;
    return pauseInfo?.paused ?? null;
  })();

  return (
    <>
      <Panel title="Recent activity">
        <div class="row">
          <IndexSelect
            label="Index"
            indexes={usable}
            value={eventIndex}
            allLabel="All indexes"
            onChange={(binding) => {
              setEventIndex(binding);
              void loadLatest(binding === '' ? null : binding);
            }}
          />
          <button
            type="button"
            disabled={events.phase === 'pending'}
            onClick={() => void loadLatest(eventIndex === '' ? null : eventIndex)}
          >
            Refresh
          </button>
        </div>
        <ProblemLine state={events} />
        {timeline !== null ? (
          <EventTable timeline={timeline} nameOf={names.nameOf} onJob={(id) => void openJob(id)} />
        ) : events.phase === 'pending' ? (
          <p class="muted">Loading events…</p>
        ) : null}
        {timeline !== null ? (
          <div class="row">
            <button
              type="button"
              class="secondary"
              disabled={events.phase === 'pending' || timeline.windowAfter <= timeline.floor}
              onClick={() => void loadOlder()}
            >
              Load older
            </button>
            <span class="muted small">
              {timeline.events.length} shown · sequences {timeline.windowAfter + 1}–{timeline.newest}
              {timeline.windowAfter <= timeline.floor
                ? timeline.prefixLost
                  ? ' · the oldest events were rotated out of the retained log'
                  : ' · start of the retained log'
                : ''}
            </span>
          </div>
        ) : null}
        <Complete state={events} label="Last events page: complete result (JSON)" />
      </Panel>

      {jobId !== '' ? (
        <Panel title="Job">
          <JobCard
            id={jobId}
            record={jobRecord}
            state={job}
            action={jobAction}
            timeline={timeline}
            nameOf={names.nameOf}
            onReload={() => void openJob(jobId)}
            onChange={(kind) => void jobChange(kind)}
          />
        </Panel>
      ) : null}

      <Panel title="Saved requests">
        <p class="muted small">
          Requests the service has journaled and not yet finished. If a change's outcome was unknown, check here before
          trying it again.
        </p>
        <div class="row">
          <button
            type="button"
            class="secondary"
            disabled={requests.phase === 'pending'}
            onClick={() => void runRequests(OPS.requestsList, body.requestsList())}
          >
            Refresh
          </button>
        </div>
        <ProblemLine state={requests} />
        <RequestTable state={requests} nameOf={names.nameOf} />
        <Complete state={requests} />
      </Panel>

      <Panel title="Pause or resume an index">
        <p class="muted small">
          Pausing stops new indexing work for the index; searches keep using its published data.
        </p>
        <div class="row">
          <IndexSelect label="Index" indexes={usable} value={pauseIndex} onChange={setPauseIndex} />
          <button
            type="button"
            class="secondary"
            disabled={names.projects.phase === 'pending' || names.catalog.phase === 'pending'}
            onClick={() => void names.reload()}
          >
            Reload indexes
          </button>
        </div>
        {names.partial ? (
          <p class="muted small">Only the first indexes are listed; the catalog has more than this page shows.</p>
        ) : null}
        <ProblemLine state={names.catalog} />
        <ProblemLine state={names.projects} />
        {pauseIndex !== '' ? (
          <>
            <p>
              <strong>
                <Text value={pauseName} />
              </strong>{' '}
              is{' '}
              {pausedNow === null ? (
                <Badge tone="muted">
                  {names.registryKnown ? 'not registered with the service' : 'in an unknown state (the index list could not be read)'}
                </Badge>
              ) : pausedNow ? (
                <Badge tone="warn">paused</Badge>
              ) : (
                <Badge tone="ok">active</Badge>
              )}
            </p>
            <div class="actions">
              {pausedNow !== true ? (
                <Confirm
                  phrase={`pause ${pauseName}`}
                  action="Pause indexing"
                  disabled={!isDigest(pauseIndex) || pause.phase === 'pending'}
                  onConfirm={() => void runPause(OPS.indexesPause, body.indexesPause(pauseIndex, true))}
                />
              ) : null}
              {pausedNow !== false ? (
                <Confirm
                  phrase={`resume ${pauseName}`}
                  action="Resume indexing"
                  disabled={!isDigest(pauseIndex) || pause.phase === 'pending'}
                  onConfirm={() => void runPause(OPS.indexesPause, body.indexesPause(pauseIndex, false))}
                />
              ) : null}
            </div>
          </>
        ) : null}
        <ProblemLine state={pause} />
        <Complete state={pause} label="Last pause or resume: complete result (JSON)" />
      </Panel>

      <Panel title="Open a job by id">
        <form
          class="row"
          onSubmit={(event) => {
            event.preventDefault();
            if (isDigest(typedJob)) void openJob(typedJob);
          }}
        >
          <Field
            label="Job id (64 hex characters)"
            wide
            value={typedJob}
            invalid={typedJob !== '' && !isDigest(typedJob)}
            onInput={(value) => setTypedJob(value.trim())}
          />
          <button type="submit" disabled={!isDigest(typedJob)}>
            Open job
          </button>
        </form>
      </Panel>
    </>
  );
}

const resultOk = (state: CallState): Rec | null =>
  state.phase === 'done' && state.result.kind === 'ok' ? rec(state.result.envelope.result) : null;

function JobLink(props: { id: string; onJob: (id: string) => void }): JSX.Element {
  return (
    <button type="button" class="link" title={props.id} onClick={() => props.onJob(props.id)}>
      <code>{short(props.id)}</code>
    </button>
  );
}

function EventTable(props: { timeline: Timeline; nameOf: (binding: string) => string; onJob: (id: string) => void }) {
  const t = props.timeline;
  if (t.events.length === 0) {
    return (
      <p class="muted">
        {t.windowAfter <= t.floor ? 'No events are recorded for this selection.' : 'No events in the latest range; load older events.'}
      </p>
    );
  }
  return (
    <div class="table-wrap">
      <table class="data">
        <thead>
          <tr>
            <th scope="col">Time</th>
            <th scope="col">What happened</th>
            <th scope="col">Index</th>
            <th scope="col">Job</th>
          </tr>
        </thead>
        <tbody>
          {t.events.map((event) => {
            const phrase = eventPhrase(event);
            const jobId = str(event.job);
            const attempts = num(event.attempts);
            const code = str(event.code);
            const project = str(event.project) ?? '';
            return (
              <tr key={num(event.sequence) ?? 0}>
                <td class="nowrap" title={`sequence ${num(event.sequence) ?? '?'}`}>
                  {localTime(event.observed_unix_ms)}
                </td>
                <td>
                  <Badge tone={phrase.tone}>{phrase.text}</Badge>
                  {code !== null ? (
                    <>
                      {' '}
                      <code>
                        <Text value={code} />
                      </code>
                    </>
                  ) : null}
                  {attempts !== null && attempts > 1 ? <span class="muted small"> · attempt {attempts}</span> : null}
                </td>
                <td title={project}>
                  <Text value={props.nameOf(project)} />
                </td>
                <td>{jobId !== null ? <JobLink id={jobId} onJob={props.onJob} /> : <span class="muted">—</span>}</td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function RequestTable(props: { state: CallState; nameOf: (binding: string) => string }): JSX.Element | null {
  const value = resultOk(props.state);
  if (value === null) return props.state.phase === 'pending' ? <p class="muted">Loading saved requests…</p> : null;
  const pending = arr(value.pending).map(rec).filter((r): r is Rec => r !== null);
  const capacity = num(value.capacity);
  return (
    <>
      <p class="muted small">
        {pending.length} saved{capacity !== null ? ` of ${capacity} the journal can hold` : ''}.
      </p>
      {pending.length === 0 ? (
        <p class="muted">No requests are waiting.</p>
      ) : (
        <div class="table-wrap">
          <table class="data">
            <thead>
              <tr>
                <th scope="col">Request</th>
                <th scope="col">Work</th>
                <th scope="col">Index</th>
                <th scope="col">State</th>
              </tr>
            </thead>
            <tbody>
              {pending.map((request) => {
                const project = str(request.project) ?? '';
                return (
                  <tr key={str(request.nonce) ?? ''}>
                    <td>
                      <code title={str(request.nonce) ?? ''}>{short(str(request.nonce) ?? '')}</code>
                    </td>
                    <td>{jobKind(request.kind)}</td>
                    <td title={project}>
                      <Text value={props.nameOf(project)} />
                    </td>
                    <td>
                      <Badge tone="warn">saved, not finished</Badge>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}

function JobCard(props: {
  id: string;
  record: Rec | null;
  state: CallState;
  action: CallState;
  timeline: Timeline | null;
  nameOf: (binding: string) => string;
  onReload: () => void;
  onChange: (kind: 'retry' | 'cancel') => void;
}): JSX.Element {
  const r = props.record;
  const state = str(r?.state) ?? '';
  const badge = JOB_STATE[state] ?? { text: state || 'unknown', tone: 'muted' };
  const outcome = rec(r?.outcome);
  const project = str(r?.project) ?? '';
  // JobRecord has no timestamps; the loaded events for this job supply them.
  const seen = (props.timeline?.events ?? []).filter((e) => e.job === props.id).reverse();
  const active = state === 'pending' || state === 'running';
  const retryable = state === 'failed' || state === 'canceled';
  const request = props.action.phase === 'done' && props.action.result.kind === 'ok' ? props.action.result.envelope.request : null;
  return (
    <div class="job-card">
      <div class="row">
        <code title={props.id}>{props.id}</code>
        <button type="button" class="small" disabled={props.state.phase === 'pending'} onClick={props.onReload}>
          Reload
        </button>
      </div>
      {props.state.phase === 'pending' ? <p class="muted">Loading the job…</p> : null}
      <ProblemLine state={props.state} />
      {r !== null ? (
        <dl class="summary">
          <dt>Work</dt>
          <dd>{jobKind(r.kind)}</dd>
          <dt>State</dt>
          <dd>
            <Badge tone={badge.tone}>{badge.text}</Badge>
          </dd>
          <dt>Index</dt>
          <dd title={project}>
            <Text value={props.nameOf(project)} />
          </dd>
          <dt>Attempts</dt>
          <dd>{num(r.attempts) ?? '—'}</dd>
          <dt>Outcome</dt>
          <dd>
            {outcome === null ? (
              <span class="muted">not finished</span>
            ) : outcome.state === 'failed' ? (
              <>
                Failed with <code><Text value={str(outcome.code) ?? 'unknown'} /></code>
              </>
            ) : outcome.state === 'succeeded' ? (
              outcome.revision !== null && outcome.revision !== undefined ? (
                <>
                  Succeeded; published revision <code><Text value={JSON.stringify(outcome.revision)} /></code>
                </>
              ) : (
                'Succeeded'
              )
            ) : (
              'Canceled'
            )}
          </dd>
          <dt>Worker generation</dt>
          <dd>{num(r.fencing_generation) ?? '—'}</dd>
          <dt>Timeline</dt>
          <dd>
            {seen.length === 0 ? (
              <span class="muted">No events for this job are loaded; load older events to find them.</span>
            ) : (
              <ul class="summary-lines">
                {seen.map((event) => (
                  <li key={num(event.sequence) ?? 0}>
                    {localTime(event.observed_unix_ms)} — {eventPhrase(event).text}
                    {str(event.code) !== null ? ` (${str(event.code)})` : ''}
                  </li>
                ))}
              </ul>
            )}
          </dd>
        </dl>
      ) : null}
      {r !== null ? (
        <div class="actions">
          {active ? (
            <Confirm
              phrase={`cancel ${short(props.id)}`}
              action="Cancel job"
              danger
              disabled={props.action.phase === 'pending'}
              onConfirm={() => props.onChange('cancel')}
            />
          ) : retryable ? (
            <Confirm
              phrase={`retry ${short(props.id)}`}
              action="Retry job"
              disabled={props.action.phase === 'pending'}
              onConfirm={() => props.onChange('retry')}
            />
          ) : (
            <p class="muted small">A finished job cannot be retried or canceled.</p>
          )}
        </div>
      ) : null}
      {request ? (
        <p class={`request request-${request.state}`}>
          Request <code>{request.nonce}</code>: <Text value={REQUEST_STATE[request.state] ?? request.state} />
        </p>
      ) : null}
      <ProblemLine state={props.action} />
      <Complete state={props.state} label="Job: complete result (JSON)" />
      <Complete state={props.action} label="Last retry or cancel: complete result (JSON)" />
    </div>
  );
}
