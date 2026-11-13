// SPDX-License-Identifier: Apache-2.0
// Management › Usage and Management › Activity (U103): read in the Explorer
// through the window's airkctl session. Usage is `airkctl usage report` for the
// selected index; Activity is this window's session status and job events and
// the service's recent log records (`airkctl service logs`). Each shows its
// complete native outcome; usage settings and full logs stay in AIRK Admin.
import { useEffect, useState } from 'preact/hooks';
import type { ScreenProps } from './workspaces';
import { request } from './api';
import { CompleteNativeResponse, Facts } from './components';
import { AdminButton } from './adminLinks';
import { Notice, noticeKey } from './notices';

/** One `airkctl` command's complete outcome as the host returns it (liveStatus.ts `CommandOutcome`). */
export interface CommandOutcomeView { ok?: boolean; argv?: string[]; class?: string; exitCode?: number; result?: unknown; stdout?: string; stderr?: string; error?: { code?: string; message?: string } }
export const USAGE_DAYS = [7, 30, 90] as const;
const ROW_LIMIT = 48;

const obj = (value: unknown): Record<string, unknown> => value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : {};
/** Scalar fields of a native document by dotted path (bounded); nested lists stay in the complete response. */
export function scalarRows(value: unknown, prefix = '', depth = 0, rows: [string, string][] = []): [string, string][] {
    for (const [key, item] of Object.entries(obj(value))) {
        if (rows.length >= ROW_LIMIT) { break; }
        const name = prefix ? `${prefix}.${key}` : key;
        if (typeof item === 'number' || typeof item === 'string' || typeof item === 'boolean') { rows.push([name, String(item).slice(0, 512)]); }
        else if (item && typeof item === 'object' && !Array.isArray(item) && depth < 2) { scalarRows(item, name, depth + 1, rows); }
    }
    return rows;
}
function outcomeLine(outcome: CommandOutcomeView): string {
    return outcome.ok ? 'Read complete.' : `${outcome.class ?? 'failed'}${outcome.error?.code ? ` (${outcome.error.code})` : ''}${outcome.error?.message ? `: ${outcome.error.message}` : ''}`;
}
const readable = (props: ScreenProps) => props.state.trusted && props.state.nativeExecutionEnabled !== false && (props.state.connection === 'ready' || props.state.connection === 'degraded');

export function UsageScreen(props: ScreenProps) {
    const [days, setDays] = useState<number>(7);
    const [busy, setBusy] = useState(false), [error, setError] = useState(''), [outcome, setOutcome] = useState<CommandOutcomeView>();
    const project = props.state.projects.find(entry => entry.project === props.state.selectedProject);
    const disabled = busy || !readable(props) || !project;
    const read = async (span: number) => {
        setBusy(true); setError('');
        try { setOutcome(await request<CommandOutcomeView>('work.usage', { days: span })); }
        catch (failure) { setOutcome(undefined); setError(failure instanceof Error ? failure.message : String(failure)); }
        finally { setBusy(false); }
    };
    const owner = JSON.stringify([props.state.selectedProject, readable(props)]);
    useEffect(() => { if (project && readable(props)) { void read(days); } }, [owner]);
    return <section class="manage-usage">
        <article class="card">
            <div class="section-head"><h2>Usage{project ? ` · ${project.name}` : ''}</h2>
                <AdminButton page="usage" label="Usage settings in AIRK Admin" ghost disabled={!props.state.trusted} choose={false} /></div>
            <p>Local usage recorded for the selected index (<code>airkctl usage report</code>). Estimates are local and never leave this computer.</p>
            {!project && <p>Select an index of this window in Overview to read its usage.</p>}
            <div class="actions">
                <label>Period <select value={String(days)} disabled={disabled} onChange={event => { const next = Number(event.currentTarget.value); setDays(next); void read(next); }}>
                    {USAGE_DAYS.map(value => <option key={value} value={String(value)}>Last {value} days</option>)}
                </select></label>
                <button type="button" disabled={disabled} onClick={() => { void read(days); }}>Refresh usage</button>
            </div>
            {busy && <p role="status">Reading usage…</p>}
            {error && <Notice condition noticeKey={noticeKey('usage-error', error)} role="alert"><p>{error}</p></Notice>}
            {outcome && <>
                <p role="status">{outcomeLine(outcome)}</p>
                {outcome.ok && scalarRows(outcome.result).length > 0 && <Facts rows={scalarRows(outcome.result)} />}
                <CompleteNativeResponse value={outcome as Record<string, unknown>} />
            </>}
        </article>
    </section>;
}

interface ActivityView { events?: Record<string, unknown>[]; eventsOmitted?: boolean; logs?: CommandOutcomeView }
const when = (value: unknown) => typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 && value <= 8640000000000000 ? new Date(value).toLocaleString() : 'time not observed';
const short = (value: unknown) => typeof value === 'string' ? value.slice(0, 12) : '';

export function ActivityScreen(props: ScreenProps) {
    const [busy, setBusy] = useState(false), [error, setError] = useState(''), [view, setView] = useState<ActivityView>();
    const disabled = busy || !readable(props);
    const read = async () => {
        setBusy(true); setError('');
        try { setView(await request<ActivityView>('work.activity', {})); }
        catch (failure) { setError(failure instanceof Error ? failure.message : String(failure)); }
        finally { setBusy(false); }
    };
    useEffect(() => { if (readable(props)) { void read(); } }, [readable(props)]);
    const events = view?.events ?? [];
    const records = Array.isArray(obj(view?.logs?.result).records) ? (obj(view?.logs?.result).records as unknown[]).map(obj) : [];
    return <section class="manage-activity">
        <article class="card">
            <div class="section-head"><h2>Recent activity</h2>
                <AdminButton page="activity" label="Full activity in AIRK Admin" ghost disabled={!props.state.trusted} choose={false} /></div>
            <p>Status and job events this window received from AIRK, and the service's recent log records. Index actions are on the Overview.</p>
            <button type="button" disabled={disabled} onClick={() => { void read(); }}>Refresh activity</button>
            {busy && <p role="status">Reading activity…</p>}
            {error && <Notice condition noticeKey={noticeKey('activity-error', error)} role="alert"><p>{error}</p></Notice>}
            {view && <>
                <h3>Events in this window</h3>
                {events.length ? <ul class="activity-events">{events.map((event, at) => <li key={at}>
                    {String(event.kind)}{event.job_kind ? ` · ${String(event.job_kind)}` : ''}{event.state ? ` · ${String(event.state)}` : ''}{event.code ? ` · ${String(event.code)}` : ''}{event.project ? ` · index ${short(event.project)}` : ''} · {when(event.observed_unix_ms)}
                </li>)}</ul> : <p class="muted">No status or job events received since this window connected.</p>}
                {view.eventsOmitted && <p class="muted">Earlier events were omitted (this window keeps the latest 64).</p>}
                <details><summary>Complete events</summary><pre>{JSON.stringify(events, null, 2)}</pre></details>
                <h3>Service log records</h3>
                {view.logs && <p role="status">{outcomeLine(view.logs)}</p>}
                {records.length ? <ul class="activity-logs">{records.slice(-32).reverse().map((record, at) => <li key={at}>
                    {String(record.kind ?? record.level ?? 'record')} · {when(record.observed_unix_ms)}{record.project ? ` · index ${short(record.project)}` : ''}{record.job ? ` · job ${short(record.job)}` : ''}{record.code ? ` · ${String(record.code)}` : ''}
                </li>)}</ul> : view.logs?.ok ? <p class="muted">No recent log records were returned.</p> : null}
                {view.logs && <CompleteNativeResponse value={view.logs as Record<string, unknown>} />}
            </>}
        </article>
    </section>;
}
