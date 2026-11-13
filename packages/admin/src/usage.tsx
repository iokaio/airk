// SPDX-License-Identifier: Apache-2.0
// Usage (usage.document, N1): one index's recorded usage, read as the same
// document `airkctl usage report --format json` and `airkctl usage policy`
// print (the kernel's UsageView, crates/airk-kernel/src/usage.rs). The page
// explains the recording policy, summarizes calls by client, operation and day,
// shows recording health and labels every saving as an estimate. Nothing is
// computed that the document does not carry; a missing value reads "not
// available". Exports (Markdown/CSV) and settings documents are the same reads in
// other formats. Every read keeps its complete native result in a collapsed JSON
// block.
import { Fragment, type ComponentChildren, type JSX } from 'preact';
import { useEffect, useState } from 'preact/hooks';
import { call } from './api';
import { body, isDigest, OPS, type UsageRead } from './operations';
import { Field, Outcome, Panel, Text, visible, type CallState, useCall } from './render';

// ---------------------------------------------------------------------------
// Readers over untyped native JSON.

type Obj = Record<string, unknown>;

function obj(value: unknown): Obj | undefined {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as Obj) : undefined;
}

function str(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined;
}

function num(value: unknown): number | undefined {
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  if (typeof value === 'string' && /^-?\d+$/.test(value)) return Number(value);
  return undefined;
}

/** Wide totals are decimal strings (usage.rs wide_number!); format without precision loss. */
function count(value: unknown): string {
  if (typeof value === 'number' && Number.isFinite(value)) return value.toLocaleString();
  if (typeof value === 'string' && /^-?\d+$/.test(value)) return BigInt(value).toLocaleString();
  return 'not available';
}

function bytes(value: unknown): string {
  const n = num(value);
  if (n === undefined) return 'not available';
  const sign = n < 0 ? '-' : '';
  let size = Math.abs(n);
  const units = ['bytes', 'KiB', 'MiB', 'GiB', 'TiB'];
  let unit = 0;
  while (size >= 1024 && unit < units.length - 1) {
    size /= 1024;
    unit += 1;
  }
  return unit === 0 ? `${sign}${size.toLocaleString()} bytes` : `${sign}${size.toFixed(1)} ${units[unit]}`;
}

function localTime(ms: unknown): string {
  const n = num(ms);
  return n === undefined ? 'not available' : new Date(n).toLocaleString();
}

function utcDay(ms: unknown): string {
  const n = num(ms);
  return n === undefined ? '?' : new Date(n).toISOString().slice(0, 10);
}

function words(key: string): string {
  return key.replace(/_/g, ' ');
}

function okResult(state: CallState): Obj | undefined {
  return state.phase === 'done' && state.result.kind === 'ok' ? obj(state.result.envelope.result) : undefined;
}

/** The `document` of a UsageDocument result, or undefined. */
function documentOf(state: CallState): unknown {
  const r = okResult(state);
  return str(r?.status) === 'usage_document' ? r?.document : undefined;
}

// ---------------------------------------------------------------------------
// Failures and complete results.

const FAILURES: Record<string, string> = {
  operation_failed: 'The service could not complete the read.',
  usage_read_failed: 'AIRK could not read this index’s usage.',
  not_yet_served: 'This service version does not answer this read yet.',
  admin_malformed_request: 'The service did not accept the request shape (a page and service version mismatch).',
};

function problemOf(state: CallState): { text: string; code?: string; detail?: string } | null {
  if (state.phase !== 'done') return null;
  const r = state.result;
  switch (r.kind) {
    case 'no_session':
      return { text: 'Your admin session has ended. Run airkctl admin open to start a new one.' };
    case 'transport':
      return { text: r.message };
    case 'failed': {
      const e = r.envelope.error;
      const detail = str(obj(e.native)?.message);
      const text = FAILURES[e.code] ?? `The service refused the read (${e.class}).`;
      return detail === undefined ? { text, code: e.code } : { text, code: e.code, detail };
    }
    case 'ok': {
      const result = obj(r.envelope.result);
      if (str(result?.status) !== 'usage_read_failed') return null;
      const message = str(result?.message);
      const text = 'AIRK could not read this index’s usage.';
      return message === undefined
        ? { text, code: 'usage_read_failed' }
        : { text, code: 'usage_read_failed', detail: message };
    }
  }
}

function Problem(props: { state: CallState }): JSX.Element | null {
  const problem = problemOf(props.state);
  if (problem === null) return null;
  return (
    <div class="au-problem" role="alert">
      <p>
        <Text value={problem.text} />
        {problem.code !== undefined ? (
          <>
            {' '}
            <code>
              <Text value={problem.code} />
            </code>
          </>
        ) : null}
      </p>
      {problem.detail !== undefined ? (
        <p class="au-detail">
          <Text value={problem.detail} />
        </p>
      ) : null}
    </div>
  );
}

function Complete(props: { state: CallState; label?: string; open?: boolean }): JSX.Element | null {
  const s = props.state;
  if (s.phase === 'idle') return null;
  if (s.phase === 'pending') return <p class="muted">Loading…</p>;
  return (
    <details class="au-complete" open={props.open ?? false}>
      <summary>{props.label ?? 'Complete result (JSON)'}</summary>
      <Outcome state={s} />
    </details>
  );
}

function Facts(props: { rows: [string, ComponentChildren][] }): JSX.Element {
  return (
    <dl class="summary">
      {props.rows.map(([label, value]) => (
        <Fragment key={label}>
          <dt>{label}</dt>
          <dd>{value}</dd>
        </Fragment>
      ))}
    </dl>
  );
}

function Cmd(props: { text: string }): JSX.Element {
  return (
    <div class="command">
      <code>{visible(props.text)}</code>
      <button type="button" class="small" onClick={() => void navigator.clipboard?.writeText(props.text)}>
        Copy
      </button>
    </div>
  );
}

function Tile(props: { label: string; value: string; note?: string }): JSX.Element {
  return (
    <div class="au-tile">
      <div class="au-tile-value">{props.value}</div>
      <div class="au-tile-label">{props.label}</div>
      {props.note !== undefined ? <div class="au-tile-note">{props.note}</div> : null}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Index choice: every `indexes` page of the catalog, following continuations.

type Choice = { binding: string; label: string };

function choicesOf(page: Obj | undefined): Choice[] {
  const choices: Choice[] = [];
  const items = Array.isArray(page?.items) ? page.items : [];
  for (const entry of items) {
    const item = obj(entry);
    const value = obj(item?.value);
    if (item?.kind !== 'index' || value === undefined) continue;
    const binding = str(obj(value.backing)?.binding);
    if (binding === undefined || !isDigest(binding)) continue;
    const name = str(value.name) ?? binding;
    choices.push({ binding, label: `${name}${value.tombstone === true ? ' (retired)' : ''}` });
  }
  return choices;
}

type IndexList = {
  choices: Choice[];
  continuation: unknown;
  revision: number | null;
  loading: boolean;
  error: string | null;
};

function useIndexes(): [IndexList, (more: boolean) => void] {
  const [list, setList] = useState<IndexList>({
    choices: [],
    continuation: null,
    revision: null,
    loading: false,
    error: null,
  });
  const load = (more: boolean): void => {
    const after = more ? list.continuation : null;
    const revision = more ? list.revision : null;
    setList((current) => ({ ...current, loading: true, error: null }));
    void (async () => {
      const request = body.catalogPage('indexes', {}, after, revision);
      const result = await call(OPS.catalogPage, request);
      if (result.kind !== 'ok') {
        const why =
          result.kind === 'failed'
            ? `The index list could not be read: ${result.envelope.error.code}.`
            : result.kind === 'no_session'
              ? 'Your admin session has ended. Run airkctl admin open to start a new one.'
              : result.message;
        setList((current) => ({ ...current, loading: false, error: why }));
        return;
      }
      const page = obj(obj(result.envelope.result)?.page);
      const rev = num(page?.revision) ?? null;
      setList((current) => ({
        choices: more ? [...current.choices, ...choicesOf(page)] : choicesOf(page),
        continuation: page?.continuation ?? null,
        revision: rev,
        loading: false,
        error: null,
      }));
    })();
  };
  return [list, load];
}

// ---------------------------------------------------------------------------
// Policy.

const MODE_TEXT: Record<string, string> = {
  disabled: 'Off. AIRK records nothing for this index.',
  aggregates_only:
    'On, daily totals only. AIRK counts calls, outcomes, timings and sizes per day; it never stores queries, paths, file contents or tokens.',
  diagnostic_events:
    'On, daily totals plus short-lived diagnostic events (still without queries, paths, contents or tokens).',
};

const MODES = ['disabled', 'aggregates_only', 'diagnostic_events'] as const;

/** UsagePolicy (crates/airk-core/src/usage.rs UsagePolicy), wherever the document carries it. */
function policyIn(value: unknown, depth = 0): Obj | undefined {
  const o = obj(value);
  if (o === undefined || depth > 3) return undefined;
  if (typeof o.mode === 'string' && 'aggregate_retention_days' in o) return o;
  for (const key of ['policy', 'structuredContent', 'result', 'value']) {
    const found = policyIn(o[key], depth + 1);
    if (found !== undefined) return found;
  }
  return undefined;
}

function PolicyView(props: { policy: Obj; binding: string }): JSX.Element {
  const p = props.policy;
  const mode = str(p.mode) ?? 'unknown';
  const [next, setNext] = useState<string>(mode === 'disabled' ? 'aggregates_only' : 'disabled');
  return (
    <>
      <p>
        <strong>Recording:</strong> <Text value={MODE_TEXT[mode] ?? mode} />
      </p>
      <Facts
        rows={[
          ['Daily totals kept for', `${count(p.aggregate_retention_days)} days`],
          ['Diagnostic events kept for', `${count(p.diagnostic_retention_hours)} hours`],
          ['Size limit for this index', bytes(p.max_collection_bytes)],
          ['Size limit for its data folder', bytes(p.max_data_root_bytes)],
        ]}
      />
      <h4>Change recording</h4>
      <p class="muted">
        Changing recording is a configuration request for this index, so it is made with airkctl on this machine.
      </p>
      <div class="row">
        <label class="field">
          New mode
          <select value={next} onChange={(event) => setNext(event.currentTarget.value)}>
            {MODES.map((m) => (
              <option value={m}>{words(m)}</option>
            ))}
          </select>
        </label>
      </div>
      <Cmd text={`airkctl --service-project ${props.binding} usage configure --mode ${next}`} />
    </>
  );
}

// ---------------------------------------------------------------------------
// Report (UsageView).

const STATUS_TEXT: Record<string, string> = {
  disabled: 'Recording is off for this index, so there is nothing new to report.',
  empty: 'Recording is on, but no calls were recorded in this window.',
  partial:
    'Recorded calls are shown below. Totals are a lower bound: calls the recorder could not store are not included.',
};

const CLIENT_NAMES: Record<string, string> = {
  unknown: 'Unknown client',
  native_scheduler: 'AIRK scheduler',
  cli: 'airkctl',
  vscode: 'VS Code',
  claude_code: 'Claude Code',
  codex: 'Codex',
  copilot_cli: 'Copilot CLI',
  open_code: 'OpenCode',
  amazon_q: 'Amazon Q',
  explorer: 'AIRK Explorer',
  workbench: 'AIRK Workbench',
};

const BASELINE_NAMES: Record<string, string> = {
  full_returned_sources: 'Versus reading every returned source in full',
  observed_alternative: 'Versus an observed alternative',
  user_supplied: 'Versus a user-supplied baseline',
};

function entries(value: unknown): [string, unknown][] {
  const o = obj(value);
  return o === undefined ? [] : Object.entries(o);
}

function ClientsTable(props: { clients: unknown; total: number }): JSX.Element {
  const rows = entries(props.clients).sort((a, b) => (num(b[1]) ?? 0) - (num(a[1]) ?? 0));
  if (rows.length === 0) return <p class="muted">No calls by any client in this window.</p>;
  return (
    <table class="au-table">
      <caption>Calls by client</caption>
      <thead>
        <tr>
          <th scope="col">Client</th>
          <th scope="col" class="num">
            Calls
          </th>
          <th scope="col" class="num">
            Share
          </th>
        </tr>
      </thead>
      <tbody>
        {rows.map(([key, value]) => {
          const n = num(value) ?? 0;
          return (
            <tr key={key}>
              <td>{CLIENT_NAMES[key] ?? words(key)}</td>
              <td class="num">{count(value)}</td>
              <td class="num">{props.total > 0 ? `${Math.round((n / props.total) * 100)}%` : '—'}</td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

function OperationsTable(props: { operations: unknown }): JSX.Element {
  const rows = entries(props.operations).sort((a, b) => (num(obj(b[1])?.calls) ?? 0) - (num(obj(a[1])?.calls) ?? 0));
  if (rows.length === 0) return <p class="muted">No operations recorded in this window.</p>;
  return (
    <div class="au-scroll">
      <table class="au-table">
        <caption>Calls by operation</caption>
        <thead>
          <tr>
            <th scope="col">Operation</th>
            <th scope="col" class="num">
              Calls
            </th>
            <th scope="col" class="num">
              Succeeded
            </th>
            <th scope="col" class="num">
              Empty
            </th>
            <th scope="col" class="num">
              Failed
            </th>
            <th scope="col" class="num">
              Not ready
            </th>
            <th scope="col" class="num">
              Cancelled
            </th>
            <th scope="col" class="num">
              Over capacity
            </th>
            <th scope="col" class="num">
              Truncated
            </th>
            <th scope="col" class="num">
              Average time
            </th>
            <th scope="col" class="num">
              Returned
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map(([key, value]) => {
            const o = obj(value) ?? {};
            const calls = num(o.calls) ?? 0;
            const elapsed = num(o.total_elapsed_us);
            const average = calls > 0 && elapsed !== undefined ? elapsed / calls / 1000 : undefined;
            const measured = num(o.measured_responses) ?? 0;
            return (
              <tr key={key}>
                <td>{words(key)}</td>
                <td class="num">{count(o.calls)}</td>
                <td class="num">{count(o.successful)}</td>
                <td class="num">{count(o.empty)}</td>
                <td class={num(o.failed) ? 'num au-bad-text' : 'num'}>{count(o.failed)}</td>
                <td class="num">{count(o.not_ready)}</td>
                <td class="num">{count(o.cancelled)}</td>
                <td class={num(o.capacity_limited) ? 'num au-warn-text' : 'num'}>{count(o.capacity_limited)}</td>
                <td class={num(o.truncated) ? 'num au-warn-text' : 'num'}>{count(o.truncated)}</td>
                <td class="num">{average === undefined ? '—' : `${average.toFixed(average < 10 ? 1 : 0)} ms`}</td>
                <td class="num" title={`${count(o.measured_responses)} measured responses`}>
                  {measured > 0 ? bytes(o.returned_utf8_bytes) : '—'}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function failedIn(report: Obj): number {
  let failed = 0;
  for (const [, value] of entries(report.operations)) failed += num(obj(value)?.failed) ?? 0;
  return failed;
}

function HistoryChart(props: { history: unknown }): JSX.Element {
  const days = (Array.isArray(props.history) ? props.history : [])
    .map((entry) => obj(entry))
    .filter((entry): entry is Obj => entry !== undefined)
    .sort((a, b) => (num(a.start_unix_ms) ?? 0) - (num(b.start_unix_ms) ?? 0));
  if (days.length === 0) return <p class="muted">No recorded days in this window.</p>;
  const max = Math.max(1, ...days.map((day) => num(day.calls) ?? 0));
  return (
    <div class="au-scroll">
      <table class="au-table au-history">
        <caption>Calls per day (UTC days)</caption>
        <thead>
          <tr>
            <th scope="col">Day (UTC)</th>
            <th scope="col" class="num">
              Calls
            </th>
            <th scope="col" class="num">
              Failed
            </th>
            <th scope="col" class="au-bar-col">
              <span class="au-sr">Bar</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {days.map((day) => {
            const calls = num(day.calls) ?? 0;
            const failed = failedIn(day);
            return (
              <tr key={String(day.start_unix_ms)}>
                <td>{utcDay(day.start_unix_ms)}</td>
                <td class="num">{count(day.calls)}</td>
                <td class={failed > 0 ? 'num au-bad-text' : 'num'}>{failed.toLocaleString()}</td>
                <td class="au-bar-col" aria-hidden="true">
                  <span class="au-bar">
                    <span class="au-bar-fill" style={{ width: `${(calls / max) * 100}%` }} />
                    {failed > 0 ? (
                      <span class="au-bar-failed" style={{ width: `${(failed / max) * 100}%` }} />
                    ) : null}
                  </span>
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function Estimates(props: { report: Obj }): JSX.Element {
  const r = props.report;
  const comparisons = entries(r.comparisons);
  const savings = r.estimated_bill_savings;
  return (
    <>
      <p class="muted">
        These are estimates of how much context AIRK's answers saved compared with a baseline. They count bytes
        and convert them to tokens with a fixed bytes-per-token assumption; they are not provider token counts or
        billed amounts.
      </p>
      {comparisons.length === 0 ? (
        <p>No comparisons are available for this window.</p>
      ) : (
        <div class="au-scroll">
          <table class="au-table">
            <caption>Context comparisons</caption>
            <thead>
              <tr>
                <th scope="col">Baseline</th>
                <th scope="col" class="num">
                  Compared calls
                </th>
                <th scope="col" class="num">
                  Baseline size
                </th>
                <th scope="col" class="num">
                  Returned size
                </th>
                <th scope="col" class="num">
                  Estimated tokens saved
                </th>
                <th scope="col" class="num">
                  Range
                </th>
              </tr>
            </thead>
            <tbody>
              {comparisons.map(([key, value]) => {
                const o = obj(value) ?? {};
                const t = obj(o.token_estimate) ?? {};
                return (
                  <tr key={key}>
                    <td>{BASELINE_NAMES[key] ?? words(key)}</td>
                    <td class="num">{count(o.compared_calls)}</td>
                    <td class="num">{bytes(o.baseline_utf8_bytes)}</td>
                    <td class="num">{bytes(o.returned_utf8_bytes)}</td>
                    <td class="num">{count(t.nominal_reduction)}</td>
                    <td class="num">
                      {count(t.assumption_low)} to {count(t.assumption_high)}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      <Facts
        rows={[
          [
            'Bytes per token assumed',
            (() => {
              const h = obj(r.heuristic);
              return h === undefined
                ? 'not available'
                : `${count(h.nominal_bytes_per_token)} (range ${count(h.minimum_bytes_per_token)}–${count(h.maximum_bytes_per_token)})`;
            })(),
          ],
          ['Calls that returned source evidence', count(r.eligible_evidence_calls)],
          ['Calls without a baseline', count(r.missing_baseline_calls)],
          ['Calls with a mismatched baseline', count(r.mismatched_baseline_calls)],
          ['Calls excluded from estimates', count(r.excluded_from_estimation_calls)],
          ['Provider token usage observed', r.provider_token_usage_observed === true ? 'yes' : 'no'],
          [
            'Estimated bill savings',
            typeof savings === 'string' ? (
              <Text value={savings} />
            ) : (
              'Not available. AIRK does not see provider bills.'
            ),
          ],
          ['Estimator', <Text value={str(r.estimator_version) ?? 'not available'} />],
        ]}
      />
      {entries(r.baseline_unavailable).length > 0 ? (
        <p class="muted">
          Baseline unavailable:{' '}
          {entries(r.baseline_unavailable)
            .map(([key, value]) => `${words(key)} ${count(value)}`)
            .join(', ')}
          .
        </p>
      ) : null}
    </>
  );
}

function Health(props: { view: Obj; report: Obj }): JSX.Element {
  const h = obj(props.view.health) ?? {};
  const dropped = h.dropped_observations;
  return (
    <Facts
      rows={[
        ['Days on record', count(h.retained_days)],
        ['Oldest recorded day', localTime(h.earliest_retained_unix_ms)],
        ['Newest recorded day', localTime(h.latest_retained_unix_ms)],
        ['History complete', h.complete_history === true ? 'yes' : h.complete_history === false ? 'no' : 'not available'],
        ['Usage database size', bytes(h.database_bytes)],
        [
          'Lost observations',
          dropped === null || dropped === undefined
            ? 'Unknown. Losses are not counted here, which is not the same as none.'
            : count(dropped),
        ],
        ['Lost in this service process', count(props.view.process_dropped_observations)],
        ['Duplicate observations ignored', count(h.duplicate_observations)],
        ['Duplicate supplemental observations', count(h.supplemental_duplicate_observations)],
        ['Calls outside scope or window', count(props.report.outside_scope_or_window)],
        [
          'Feedback',
          `${count(h.useful_feedback)} useful, ${count(h.partly_useful_feedback)} partly useful, ${count(h.not_useful_feedback)} not useful`,
        ],
      ]}
    />
  );
}

function ReportView(props: { view: Obj; days: number }): JSX.Element {
  const v = props.view;
  const report = obj(v.report);
  const status = str(v.status) ?? 'unknown';
  if (report === undefined) {
    return <p class="error">The document has no report section. See the complete result below.</p>;
  }
  const total = num(report.calls) ?? 0;
  const lifetime = obj(v.lifetime);
  const notice = str(v.notice);
  return (
    <>
      <p>
        <Text value={STATUS_TEXT[status] ?? `Report status: ${status}.`} />
      </p>
      <div class="au-tiles">
        <Tile label="Calls" value={count(report.calls)} note={`last ${props.days} day${props.days === 1 ? '' : 's'}`} />
        <Tile label="Failed" value={failedIn(report).toLocaleString()} />
        <Tile label="Clients" value={entries(report.clients).length.toLocaleString()} />
        <Tile
          label="Since recording began"
          value={lifetime !== undefined ? count(lifetime.calls) : 'n/a'}
          note={lifetime !== undefined ? 'partial totals' : 'not available'}
        />
      </div>
      <p class="muted">
        Window: {localTime(report.start_unix_ms)} to {localTime(report.end_unix_ms_exclusive)} (your local time;
        AIRK counts whole UTC days).
      </p>
      <div class="au-grid">
        <ClientsTable clients={report.clients} total={total} />
        <div>
          <HistoryChart history={v.history} />
        </div>
      </div>
      <OperationsTable operations={report.operations} />
      {entries(report.content_kinds).length > 0 ? (
        <p class="muted">
          Content returned:{' '}
          {entries(report.content_kinds)
            .map(([key, value]) => `${words(key)} ${count(value)}`)
            .join(', ')}
          .
        </p>
      ) : null}
      <h3>Estimated savings</h3>
      <Estimates report={report} />
      <h3>Recording health</h3>
      <Health view={v} report={report} />
      {Array.isArray(v.supplemental) && v.supplemental.length > 0 ? (
        <p class="muted">
          {v.supplemental.length} supplemental session record{v.supplemental.length === 1 ? '' : 's'} are in the
          complete result.
        </p>
      ) : null}
      {notice !== undefined ? (
        <p class="au-notice">
          <Text value={notice} />
        </p>
      ) : null}
    </>
  );
}

// ---------------------------------------------------------------------------

const DAY_CHOICES = [1, 7, 14, 30] as const;

const SETTINGS_VIEWS = [
  'extraction_policy',
  'vector_selection',
  'vector_status',
  'roots',
  'scan_limits',
  'quota_policy',
  'quota_status',
] as const;

export function Usage(): JSX.Element {
  const [indexes, loadIndexes] = useIndexes();
  const [report, runReport] = useCall();
  const [policy, runPolicy] = useCall();
  const [exported, runExport] = useCall();
  const [settings, runSettings] = useCall();
  const [binding, setBinding] = useState('');
  const [days, setDays] = useState(7);
  const [shownDays, setShownDays] = useState(7);
  const [format, setFormat] = useState<'markdown' | 'csv'>('markdown');
  const [view, setView] = useState<string>('roots');
  useEffect(() => loadIndexes(false), []);
  const valid = isDigest(binding);
  const chosen = indexes.choices.find((choice) => choice.binding === binding);
  const read = (what: UsageRead) => body.usageDocument(binding, what);
  // The index the shown results belong to (the selection may change after).
  const [loaded, setLoaded] = useState<{ binding: string; label: string } | null>(null);
  const load = (): void => {
    setShownDays(days);
    setLoaded({ binding, label: chosen?.label ?? `index ${binding.slice(0, 12)}` });
    void runReport(OPS.usageDocument, read({ report: { days, format: 'json' } }));
    void runPolicy(OPS.usageDocument, read('policy'));
  };
  const reportView = obj(documentOf(report));
  const policyValue = policyIn(documentOf(policy)) ?? policyIn(reportView);
  return (
    <>
      <Panel title="Usage">
        <p class="muted">
          How agents and tools used one index: the same report as <code>airkctl usage report</code>. AIRK records
          counts, timings and sizes only, never queries or file contents.
        </p>
        <div class="row">
          <label class="field wide">
            Index
            <select value={binding} onChange={(event) => setBinding(event.currentTarget.value)}>
              <option value="">Choose an index…</option>
              {indexes.choices.map((choice) => (
                <option value={choice.binding}>{`${choice.label} (${choice.binding.slice(0, 12)})`}</option>
              ))}
            </select>
          </label>
          <label class="field narrow">
            Period
            <select
              value={String(days)}
              onChange={(event) => setDays(Number.parseInt(event.currentTarget.value, 10))}
            >
              {DAY_CHOICES.map((d) => (
                <option value={String(d)}>{d === 1 ? 'Today' : `${d} days`}</option>
              ))}
            </select>
          </label>
          <button type="button" disabled={!valid} onClick={load}>
            Show usage
          </button>
        </div>
        {indexes.loading ? <p class="muted">Loading indexes…</p> : null}
        {indexes.error !== null ? (
          <p class="error" role="alert">
            <Text value={indexes.error} />
          </p>
        ) : null}
        {indexes.continuation !== null && indexes.continuation !== undefined ? (
          <p class="muted">
            The list above shows the first {indexes.choices.length} indexes; more exist.{' '}
            <button type="button" class="small" disabled={indexes.loading} onClick={() => loadIndexes(true)}>
              Load more indexes
            </button>
          </p>
        ) : null}
        {!indexes.loading && indexes.error === null && indexes.choices.length === 0 ? (
          <p class="muted">No indexes are listed. Enter a project binding below instead.</p>
        ) : null}
        <details>
          <summary>Use a project binding instead</summary>
          <div class="row">
            <Field
              label="Project binding (64 hex)"
              wide
              value={binding}
              invalid={binding !== '' && !valid}
              onInput={(value) => setBinding(value.trim().toLowerCase())}
            />
            <button type="button" class="secondary" onClick={() => loadIndexes(false)}>
              Reload index list
            </button>
          </div>
        </details>
      </Panel>

      {policy.phase !== 'idle' || report.phase !== 'idle' ? (
        <Panel title={`Recording${loaded !== null ? ` for ${loaded.label}` : ''}`}>
          {policyValue !== undefined && loaded !== null ? (
            <PolicyView key={loaded.binding} policy={policyValue} binding={loaded.binding} />
          ) : null}
          {policyValue === undefined && policy.phase === 'done' && problemOf(policy) === null ? (
            <p class="muted">The policy document has an unfamiliar shape; see the complete result.</p>
          ) : null}
          <Problem state={policy} />
          <Complete state={policy} label="Complete policy result (JSON)" />
        </Panel>
      ) : null}

      {report.phase !== 'idle' ? (
        <Panel title={`Report${loaded !== null ? ` for ${loaded.label}` : ''}`}>
          {loaded !== null && loaded.binding !== binding ? (
            <p class="muted">These results are for {loaded.label}; choose Show usage to read the selected index.</p>
          ) : null}
          {reportView !== undefined ? <ReportView view={reportView} days={shownDays} /> : null}
          <Problem state={report} />
          <Complete state={report} label="Complete report result (JSON)" />
        </Panel>
      ) : null}

      <Panel title="Export a report">
        <p class="muted">The same report as Markdown or CSV, as the CLI prints it.</p>
        <div class="row">
          <label class="field narrow">
            Format
            <select
              value={format}
              onChange={(event) => {
                const value = event.currentTarget.value;
                if (value === 'markdown' || value === 'csv') setFormat(value);
              }}
            >
              <option value="markdown">Markdown</option>
              <option value="csv">CSV</option>
            </select>
          </label>
          <button
            type="button"
            class="secondary"
            disabled={!valid}
            onClick={() => void runExport(OPS.usageDocument, read({ report: { days, format } }))}
          >
            Read {format === 'csv' ? 'CSV' : 'Markdown'}
          </button>
        </div>
        <Problem state={exported} />
        <Complete state={exported} label="Report text and complete result" open />
      </Panel>

      <Panel title="Index settings documents">
        <p class="muted">
          The settings documents <code>airkctl settings</code> reads for the same index.
        </p>
        <div class="row">
          <label class="field">
            Document
            <select value={view} onChange={(event) => setView(event.currentTarget.value)}>
              {SETTINGS_VIEWS.map((name) => (
                <option value={name}>{words(name)}</option>
              ))}
            </select>
          </label>
          <button
            type="button"
            class="secondary"
            disabled={!valid}
            onClick={() => void runSettings(OPS.usageDocument, read({ settings: { view } }))}
          >
            Read
          </button>
        </div>
        <Problem state={settings} />
        <Complete state={settings} open />
      </Panel>
    </>
  );
}
