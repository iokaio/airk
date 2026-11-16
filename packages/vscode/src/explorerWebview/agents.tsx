// SPDX-License-Identifier: Apache-2.0
// Management › Agents (U104, U110): connect a coding agent to this window's
// folder. The task comes first (pick an agent, Connect, see its status), then a
// one-line endpoint status with the URL and Copy, and a short note on scope:
// agents choose indexes per call, or use the workspace default this window's
// applied search scope sets for its folders. No credential or connection is
// involved. Entries come from `airkctl agents list --no-user`, the endpoint from
// `airkctl agents endpoint` and the defaults from `airkctl scope defaults`; the
// complete native reports are kept in the collapsed Details. Native text is
// rendered as escaped Preact text.
import { useEffect, useRef, useState } from 'preact/hooks';
import type { AgentClient } from '../explorerContracts';
import type { AgentManualEntry, AgentConfigRefusal } from '../generated/i2';
import type { ScreenProps } from './workspaces';
import { request } from './api';
import { CompleteNativeResponse } from './components';
import { AdminButton } from './adminLinks';
import { Notice, noticeKey } from './notices';

/** The agents Connect offers and how each loads a new entry. */
export const AGENT_CHOICES: readonly { client: AgentClient; label: string; reconnect: string }[] = [
    { client: 'claude', label: 'Claude Code', reconnect: 'If Claude Code is already running, run /mcp (or restart Claude Code) to load the new entry.' },
    { client: 'codex', label: 'Codex', reconnect: 'If Codex is already running, restart it to load the new entry.' },
    { client: 'vscode', label: 'VS Code / GitHub Copilot', reconnect: 'VS Code starts the AIRK MCP server from the new entry; otherwise restart it from MCP: List Servers.' },
];
/** How agents pick what they read (U110), shown under the endpoint line. */
export const SCOPE_NOTE = 'Agents choose indexes or index sets on each call. A call that names none uses the workspace default of its folder, which is this window\'s applied search scope.';
const STATUS_TEXT: Record<string, string> = {
    ok: 'Connected', stdio_retired: 'Retired stdio entry', stdio_missing_command: 'Stdio entry with a missing command',
    https_unknown_connection: 'Old entry (reconnect to update it)', https_endpoint_mismatch: 'Wrong endpoint (reconnect to update it)',
    https_helper_missing: 'Old entry (reconnect to update it)', https_trust_missing: 'Certificate trust missing', unreadable: 'File could not be read',
};

export interface AgentEntryView { client: AgentClient; agent: string; file: string; scope: string; serverKey: string; url?: string; status: string | null }
export interface ScopeItemView { kind: 'index' | 'set'; id: string; name?: string; description?: string }
interface FailureView { code: string; message: string }
interface StoredDefault { folder: string; index_ids: string[]; index_set_ids: string[]; applied_unix_ms: number; items: ScopeItemView[] }
export interface AgentsReadView {
    folder?: string; folders?: string[]; entries?: AgentEntryView[]; report?: unknown; manual?: AgentManualEntry[]; manualErrors?: FailureView[];
    endpoint?: { url: string; state?: string } | null; endpointReport?: unknown; endpointError?: FailureView;
    workspaceDefaults?: { folder: string; stored: StoredDefault | null }[]; defaultsReport?: unknown; defaultsError?: FailureView;
    windowScope?: { items: ScopeItemView[] } | null; followError?: FailureView;
}
interface ConnectView { refusal?: AgentConfigRefusal; manual?: AgentManualEntry; status?: string; client?: AgentClient; file?: string; url?: string; changed?: boolean; scope?: ScopeItemView[] | null; defaultError?: FailureView }

const label = (client: AgentClient) => AGENT_CHOICES.find(choice => choice.client === client)?.label ?? client;
const readable = (props: ScreenProps) => props.state.trusted && props.state.nativeExecutionEnabled !== false && (props.state.connection === 'ready' || props.state.connection === 'degraded');
const statusText = (status: string | null) => status === null ? 'Status not observed' : STATUS_TEXT[status] ?? status;

/** Names of a scope; each item's description is its tooltip. */
export function ScopeNames(props: { items: readonly ScopeItemView[] }) {
    if (!props.items.length) { return <span class="muted">none</span>; }
    return <span class="agent-scope">{props.items.map((item, at) => <span key={`${item.kind}:${item.id}`}>{at ? ', ' : ''}
        <span title={item.description ?? item.id}>{item.name ?? item.id}</span>{item.kind === 'set' ? ' (index set)' : ''}</span>)}</span>;
}

export function AgentsScreen(props: ScreenProps) {
    const [client, setClient] = useState<AgentClient>('claude');
    const [busy, setBusy] = useState(false), [error, setError] = useState(''), [view, setView] = useState<AgentsReadView>();
    const [connected, setConnected] = useState<{ client: AgentClient; value: ConnectView }>();
    const [copied, setCopied] = useState('');
    const [manualOpen, setManualOpen] = useState(false);
    const ownerKey = JSON.stringify([props.state.workspaceFolders, props.state.trusted, props.state.nativeExecutionEnabled]);
    const owner = useRef(ownerKey); owner.current = ownerKey;
    const scope = props.state.searchScope;
    const disabled = busy || !readable(props);
    const run = async (work: () => Promise<void>) => {
        if (busy) { return; }
        const ticket = owner.current; setBusy(true); setError('');
        try { await work(); }
        catch (failure) { if (owner.current === ticket) setError(failure instanceof Error ? failure.message : String(failure)); }
        finally { if (owner.current === ticket) setBusy(false); }
    };
    const reread = async () => { const ticket = owner.current; const next = await request<AgentsReadView>('agents.read', {}); if (owner.current === ticket) setView(next); };
    const read = () => run(reread);
    useEffect(() => {
        setView(undefined); setConnected(undefined); setManualOpen(false); setError('');
        const ticket = owner.current;
        if (!readable(props)) { setBusy(false); return; }
        setBusy(true); void reread().catch(failure => { if (owner.current === ticket) setError(failure instanceof Error ? failure.message : String(failure)); })
            .finally(() => { if (owner.current === ticket) setBusy(false); });
    }, [readable(props), ownerKey]);
    const connect = () => run(async () => { const ticket = owner.current; const value = await request<ConnectView>('agents.connect', { client }); if (owner.current !== ticket) return; setConnected({ client, value }); if (value.refusal) setManualOpen(true); await reread(); });
    const remove = (target: AgentClient) => run(async () => { await request('agents.disconnect', { client: target }); setConnected(undefined); await reread(); });
    const copy = (url: string) => { setCopied(''); void request('clipboard.write', { text: url }).then(() => setCopied('Copied'), failure => setCopied(failure instanceof Error ? failure.message : String(failure))); };
    const entries = view?.entries ?? [];
    const mine = entries.filter(entry => entry.client === client && entry.scope === 'project');
    const result = connected?.value;
    const endpoint = view?.endpoint;
    const defaults = view?.workspaceDefaults ?? [];
    const manual = result?.manual?.client === client ? result.manual : view?.manual?.find(entry => entry.client === client);
    const refusal = result?.refusal;
    const refusalText = refusal?.reason === 'automatic_policy' ? 'AIRK cannot safely edit this file and retain a recovery copy. Other accounts may be able to change the folder, or a file safety check failed. Merge the entry below into the named file, preserving your other settings.'
        : refusal?.reason === 'conflict_retained' ? 'The file changed while AIRK was preparing the edit. Review the current file and merge this entry manually.'
        : refusal?.reason === 'outcome_unknown' ? 'The edit outcome is unknown. Inspect the named file before changing it again. The complete original outcome is in Details.' : '';

    return <section class="manage-agents">
        <article class="card">
            <div class="section-head"><h2>Connect a coding agent</h2></div>
            <div class="actions">
                <label>Agent <select value={client} disabled={disabled} onChange={event => { setClient(event.currentTarget.value as AgentClient); setConnected(undefined); }}>
                    {AGENT_CHOICES.map(choice => <option key={choice.client} value={choice.client}>{choice.label}</option>)}
                </select></label>
                <button type="button" disabled={disabled} onClick={() => { void connect(); }}>Connect</button>
            </div>
            {view && <p role="status">{label(client)}{view.folder ? <> in <code class="path">{view.folder}</code></> : ''}: {mine.length
                ? mine.map((entry, at) => <span key={`${entry.file}:${entry.serverKey}`}>{at ? '; ' : ''}<strong>{statusText(entry.status)}</strong> (<code class="path">{entry.file}</code>)</span>)
                : 'not connected'}
                {mine.length > 0 && <> <button type="button" class="ghost" disabled={disabled} onClick={() => { void remove(client); }}>Remove</button></>}</p>}
            {result && connected && result.status !== 'refused' && <div role="status">
                <p>{label(connected.client)} connected{result.file ? <> in <code class="path">{result.file}</code></> : ''}.{result.changed === false ? ' Its entry was already up to date.' : ''}</p>
                {result.changed !== false && <p class="muted">{AGENT_CHOICES.find(choice => choice.client === connected.client)?.reconnect}</p>}
                {result.defaultError && <p class="callout" role="alert">This window's search scope was not stored as the workspace default ({result.defaultError.code}): {result.defaultError.message}</p>}
            </div>}
            {busy && <p role="status">Working…</p>}
            {error && <Notice condition noticeKey={noticeKey('agents-error', error)} role="alert"><p>{error}</p></Notice>}
        </article>
        <details class="card agent-manual" open={manualOpen} onToggle={event => setManualOpen(event.currentTarget.open)}>
            <summary>Set up manually</summary>
            {refusal && <p role="alert">{refusalText}</p>}
            {manual ? <><p>Merge this entry into <code class="path">{manual.file}</code>. Keep any other servers and settings in that file.</p>
                <button type="button" class="ghost" onClick={() => copy(manual.text)}>Copy complete entry</button>
                {copied && <span role="status"> {copied}</span>}<pre>{manual.text}</pre></>
                : <p role="status">{busy ? 'Reading the manual entry...' : 'No native manual entry is available. Refresh the Agents view or run airkctl agents manual --client <agent> --project <folder>.'}</p>}
            {view?.manualErrors?.map((failure, at) => <p key={at} role="status">{failure.message}</p>)}
            <p class="muted">If the service changes port, copy a fresh entry here or run <code>airkctl agents endpoint</code>.</p>
        </details>
        <article class="card">
            <p class="agent-endpoint" role="status">{endpoint
                ? <>{endpoint.state && endpoint.state !== 'ready' ? `Endpoint ${endpoint.state}` : 'Ready: agents can connect'} · <code>{endpoint.url}</code>{' '}
                    <button type="button" class="ghost" onClick={() => copy(endpoint.url)}>Copy</button>{copied && <span class="muted" role="status"> {copied}</span>}</>
                : view?.endpointError ? <>The MCP endpoint could not be read ({view.endpointError.code}): {view.endpointError.message}</>
                : view ? 'The MCP endpoint was not reported.' : 'Reading the MCP endpoint…'}</p>
            <p class="muted">{SCOPE_NOTE}</p>
            {defaults.map(item => <p key={item.folder}>Workspace default for <code class="path">{item.folder}</code>: {item.stored
                ? <ScopeNames items={item.stored.items} /> : <span class="muted">none (agents must name indexes on each call)</span>}</p>)}
            {view?.defaultsError && <p class="callout" role="status">The workspace defaults could not be read ({view.defaultsError.code}): {view.defaultsError.message}</p>}
            {view?.followError && <p class="callout" role="alert">This window's search scope was not stored as the workspace default ({view.followError.code}): {view.followError.message}</p>}
            {scope?.available && !scope.active && <p class="muted">No search scope is applied in this window. <button type="button" class="ghost" onClick={() => props.go({ workspace: 'indexing', tab: 'window' })}>Choose a search scope</button></p>}
        </article>
        <details class="card">
            <summary>Details</summary>
            <div class="section-head"><h3>Agent entries{view?.folder ? <> in <code class="path">{view.folder}</code></> : ''}</h3>
                <span class="actions"><button type="button" class="ghost" disabled={disabled} onClick={() => { void read(); }}>Refresh</button>
                    <AdminButton page="agents" label="Open AIRK Admin" ghost disabled={!props.state.trusted} choose={false} /></span></div>
            {view && (entries.length
                ? <ul class="agent-entries">{entries.map((entry, at) => <li key={`${entry.file}:${entry.serverKey}:${at}`}>
                    <strong>{label(entry.client)}</strong> · <code class="path">{entry.file}</code> · {entry.serverKey} · {statusText(entry.status)}</li>)}</ul>
                : <p class="muted">No AIRK agent entries in this folder.</p>)}
            <p class="muted">AIRK writes only its own entry (the endpoint URL and the response-mode header) and keeps your other servers. The endpoint is local HTTPS with AIRK's own certificate; AIRK Admin › Agents shows certificate trust and manual setup.</p>
            {view?.report !== undefined && <CompleteNativeResponse value={view.report as Record<string, unknown>} />}
            {view?.endpointReport !== undefined && <CompleteNativeResponse value={view.endpointReport as Record<string, unknown>} />}
            {view?.defaultsReport !== undefined && <CompleteNativeResponse value={view.defaultsReport as Record<string, unknown>} />}
            {refusal && <pre>{refusal.detail}</pre>}
            {result && <CompleteNativeResponse value={result as Record<string, unknown>} />}
        </details>
    </section>;
}
