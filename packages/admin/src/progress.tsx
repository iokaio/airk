// SPDX-License-Identifier: Apache-2.0
// Live indexing progress for one index (U103): `indexes.progress`, the
// ControlMethod Progress answer (service.rs ControlResponse::Progress,
// service_progress.rs ProgressObservation). It refreshes about every two
// seconds while a job runs or waits to run, once otherwise, and never while the
// tab is hidden. The readable summary never replaces the complete native result,
// which stays available below it.
import type { JSX } from 'preact';
import { useEffect, useRef, useState } from 'preact/hooks';
import { Badge, Complete, JOB_STATE, jobKind, ProblemLine } from './activity';
import { call } from './api';
import { body, isDigest, OPS } from './operations';
import { Text, type CallState } from './render';

/** The refresh interval while work is running or queued. */
const POLL_MS = 2000;

type Rec = Record<string, unknown>;
const rec = (value: unknown): Rec | null =>
  typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as Rec) : null;
const str = (value: unknown): string | null => (typeof value === 'string' ? value : null);
const num = (value: unknown): number | null => (typeof value === 'number' && Number.isFinite(value) ? value : null);
const arr = (value: unknown): unknown[] => (Array.isArray(value) ? value : []);

/** WorkPhase (service_progress.rs), in plain words. */
const PHASE: Record<string, string> = {
  starting: 'Starting',
  discovering: 'Finding files',
  extracting: 'Reading files',
  preparing: 'Preparing changes',
  publishing: 'Publishing',
  embedding: 'Building vectors',
  verifying: 'Verifying',
  recovering: 'Recovering',
  retiring: 'Retiring',
  finished: 'Finished',
};
const phaseText = (phase: unknown): string => PHASE[str(phase) ?? ''] ?? str(phase) ?? 'unknown';

/** A millisecond duration as `1 h 2 min`, `3 min 4 s` or `5.6 s`. */
function duration(value: unknown): string {
  const ms = num(value);
  if (ms === null) return '—';
  if (ms < 1000) return `${ms} ms`;
  const seconds = ms / 1000;
  if (seconds < 60) return `${seconds.toFixed(1)} s`;
  const whole = Math.floor(seconds);
  const hours = Math.floor(whole / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  const rest = whole % 60;
  return hours > 0 ? `${hours} h ${minutes} min` : `${minutes} min ${rest} s`;
}

const count = (value: unknown): string => {
  const n = num(value);
  return n === null ? '—' : n.toLocaleString();
};

function localTime(ms: unknown): string {
  const n = num(ms);
  return n === null ? '—' : new Date(n).toLocaleString();
}

/** The progress row outlives its job; only a running or pending job is current work. */
function observedJobActive(result: Rec): boolean {
  const state = str(rec(rec(result.observation)?.job)?.state);
  return state === 'running' || state === 'pending';
}

/** Work is running or waiting to run, so the answer will change soon. */
function isActive(result: Rec): boolean {
  if (observedJobActive(result)) return true;
  const pending = rec(result.pending_job);
  if (str(pending?.state) === 'running') return true;
  if (result.project_paused === true) return false;
  return pending !== null || arr(result.queued_jobs).length > 0;
}

/** JobRecord: kind, state, attempts and id. */
function JobLine(props: { job: Rec }): JSX.Element {
  const j = props.job;
  const state = str(j.state) ?? '';
  const badge = JOB_STATE[state] ?? { text: state || 'unknown', tone: 'muted' };
  const outcome = rec(j.outcome);
  return (
    <>
      {jobKind(j.kind)} <Badge tone={badge.tone}>{badge.text}</Badge> · attempts {count(j.attempts)} · worker
      generation {count(j.fencing_generation)}
      {outcome !== null && str(outcome.code) !== null ? (
        <>
          {' '}
          · <code><Text value={str(outcome.code) ?? ''} /></code>
        </>
      ) : null}{' '}
      · <code title="job id"><Text value={str(j.id) ?? '—'} /></code>
    </>
  );
}

function Summary(props: { result: Rec }): JSX.Element {
  const r = props.result;
  const observation = rec(r.observation);
  const progress = rec(observation?.progress);
  const job = rec(observation?.job);
  const pending = rec(r.pending_job);
  const queued = arr(r.queued_jobs).map(rec).filter((j): j is Rec => j !== null);
  const committed = num(progress?.committed_changes);
  const captured = num(progress?.captured_files);
  let status: { text: string; tone: string };
  if (progress !== null && observedJobActive(r)) {
    status = progress.stalled === true
      ? { text: 'Stalled', tone: 'danger' }
      : progress.paused === true
        ? { text: 'Paused', tone: 'warn' }
        : { text: 'Indexing', tone: 'info' };
  } else if (r.project_paused === true) status = { text: 'Paused', tone: 'warn' };
  else if (pending !== null || queued.length > 0) status = { text: 'Waiting to run', tone: 'warn' };
  else status = { text: 'Idle', tone: 'muted' };
  const totals = Object.entries(rec(progress?.phase_totals_ms) ?? {});
  return (
    <dl class="summary">
      <dt>State</dt>
      <dd>
        <Badge tone={status.tone}>{status.text}</Badge>
        {progress !== null ? (
          <>
            {' '}
            {phaseText(progress.phase)}{' '}
            <span class="muted">
              (<Text value={str(progress.phase) ?? ''} />)
            </span>
          </>
        ) : null}
      </dd>
      {progress !== null ? (
        <>
          <dt>Committed</dt>
          <dd>
            {count(committed)} of {count(captured)} captured files
            {committed !== null && captured !== null && captured > 0 ? (
              <>
                {' '}
                <progress
                  max={captured}
                  value={Math.min(committed, captured)}
                  aria-label={`${committed} of ${captured} captured files committed`}
                />
              </>
            ) : null}
          </dd>
          <dt>Indexed files</dt>
          <dd>
            {count(progress.indexed_files)}
            {num(progress.total_files) !== null ? ` of ${count(progress.total_files)}` : ''}
          </dd>
          <dt>Read and prepared</dt>
          <dd>
            {count(progress.extracted_files)} files read · {count(progress.prepared_changes)} changes prepared
          </dd>
          <dt>Publications</dt>
          <dd>
            {count(progress.publications)} · {count(progress.verified_objects)} objects verified
            {num(progress.completed_semantic_units) !== null
              ? ` · ${count(progress.completed_semantic_units)} semantic units`
              : ''}
          </dd>
          <dt>Elapsed</dt>
          <dd>
            {duration(progress.elapsed_ms)} in total · {duration(progress.phase_elapsed_ms)} in this phase
          </dd>
          <dt>Last advance</dt>
          <dd>
            {num(progress.last_advance_age_ms) === null
              ? 'no advance recorded yet'
              : `${duration(progress.last_advance_age_ms)} ago`}
            {' · '}idle while unpaused {duration(progress.unpaused_idle_ms)}
          </dd>
          <dt>Flags</dt>
          <dd>
            {progress.stalled === true ? 'stalled' : 'not stalled'} · {progress.paused === true ? 'paused' : 'not paused'}
          </dd>
          {totals.length > 0 ? (
            <>
              <dt>Time per phase</dt>
              <dd>
                <ul class="summary-lines">
                  {totals.map(([phase, ms]) => (
                    <li key={phase}>
                      {phaseText(phase)}: {duration(ms)}
                    </li>
                  ))}
                </ul>
              </dd>
            </>
          ) : null}
        </>
      ) : null}
      <dt>Current job</dt>
      <dd>{job !== null ? <JobLine job={job} /> : <span class="muted">none running</span>}</dd>
      <dt>Next job</dt>
      <dd>{pending !== null ? <JobLine job={pending} /> : <span class="muted">none</span>}</dd>
      <dt>Queued jobs</dt>
      <dd>
        {queued.length === 0 ? (
          <span class="muted">none</span>
        ) : (
          <ul class="summary-lines">
            {queued.map((j, i) => (
              <li key={str(j.id) ?? i}>
                <JobLine job={j} />
              </li>
            ))}
          </ul>
        )}
        {r.queue_truncated === true ? (
          <p class="muted">The queue has more jobs than this answer lists; see the complete result.</p>
        ) : null}
      </dd>
      <dt>Index paused</dt>
      <dd>
        {r.project_paused === true ? 'yes' : r.project_paused === false ? 'no' : 'unknown'}
        {r.pending_paused === true ? ' · the next job is paused' : ''}
      </dd>
      <dt>Observed</dt>
      <dd>
        {localTime(r.observed_unix_ms ?? observation?.observed_unix_ms)}
        {observation !== null ? ` · worker generation ${count(observation.current_fencing_generation)}` : ''}
      </dd>
    </dl>
  );
}

type Live = 'loading' | 'polling' | 'idle' | 'hidden' | 'failed';

const LIVE_TEXT: Record<Live, string> = {
  loading: 'Reading progress…',
  polling: 'Refreshing every 2 seconds while a job runs or waits.',
  idle: 'No job is running or waiting; not refreshing. Use Refresh to read again.',
  hidden: 'Refreshing stopped while this tab is hidden.',
  failed: 'The last read failed; not refreshing. Use Refresh to try again.',
};

/** Live progress of one registered index, by its 64-hex project binding. */
export function ProgressPanel(props: { binding: string }): JSX.Element {
  const [shown, setShown] = useState<{ binding: string; state: CallState } | null>(null);
  const [live, setLive] = useState<Live>('loading');
  // The newest read wins; a binding change or unmount drops older answers.
  const ticket = useRef(0);
  const timer = useRef<number | null>(null);
  // Set when refreshing stopped only because the tab was hidden.
  const stoppedHidden = useRef(false);
  const stopTimer = (): void => {
    if (timer.current !== null) {
      window.clearTimeout(timer.current);
      timer.current = null;
    }
  };
  const refresh = async (binding: string): Promise<void> => {
    stopTimer();
    stoppedHidden.current = false;
    const mine = ++ticket.current;
    setLive('loading');
    const result = await call(OPS.indexesProgress, body.indexesProgress(binding));
    if (mine !== ticket.current) return;
    setShown({ binding, state: { phase: 'done', operation: OPS.indexesProgress, result, at: Date.now() } });
    const value = result.kind === 'ok' ? rec(result.envelope.result) : null;
    if (value === null) {
      setLive('failed');
      return;
    }
    if (!isActive(value)) {
      setLive('idle');
      return;
    }
    if (document.visibilityState === 'hidden') {
      stoppedHidden.current = true;
      setLive('hidden');
      return;
    }
    setLive('polling');
    timer.current = window.setTimeout(() => void refresh(binding), POLL_MS);
  };

  useEffect(() => {
    if (!isDigest(props.binding)) return undefined;
    const binding = props.binding;
    stoppedHidden.current = false;
    void refresh(binding);
    const onVisibility = (): void => {
      if (document.visibilityState === 'hidden') {
        if (timer.current !== null) {
          stopTimer();
          stoppedHidden.current = true;
          setLive('hidden');
        }
      } else if (stoppedHidden.current) {
        stoppedHidden.current = false;
        void refresh(binding);
      }
    };
    document.addEventListener('visibilitychange', onVisibility);
    return () => {
      ticket.current++;
      stopTimer();
      document.removeEventListener('visibilitychange', onVisibility);
    };
  }, [props.binding]);

  if (!isDigest(props.binding)) {
    return <p class="muted">This index has no project binding, so its progress cannot be read.</p>;
  }
  const current = shown !== null && shown.binding === props.binding ? shown.state : null;
  const value =
    current !== null && current.phase === 'done' && current.result.kind === 'ok'
      ? rec(current.result.envelope.result)
      : null;
  const mismatch = value !== null && str(value.project) !== null && value.project !== props.binding;
  return (
    <div class="progress-panel">
      <div class="row">
        <button
          type="button"
          class="secondary small"
          disabled={live === 'loading'}
          onClick={() => void refresh(props.binding)}
        >
          Refresh
        </button>
        <span class="muted" role="status" aria-live="polite">
          {LIVE_TEXT[live]}
        </span>
      </div>
      {current === null ? <p class="muted">Reading progress…</p> : null}
      {current !== null ? <ProblemLine state={current} /> : null}
      {mismatch ? (
        <p class="error" role="alert">
          The service answered for a different index; the summary is not shown.
        </p>
      ) : null}
      {value !== null && !mismatch ? <Summary result={value} /> : null}
      {current !== null ? <Complete state={current} label="Progress: complete result (JSON)" /> : null}
    </div>
  );
}
