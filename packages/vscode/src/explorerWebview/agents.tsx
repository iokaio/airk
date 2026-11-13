// SPDX-License-Identifier: Apache-2.0
// Management › Agents (U104): connect a coding agent to this window's search
// scope and list the agent entries of this window's folder with Disconnect.
// Connect runs the host hook `connectAgentToScope`; entries come from
// `airkctl agents list --no-user` with the complete native report. Advanced
// connection management stays in AIRK Admin › Agents.
import { useEffect, useState } from 'preact/hooks';
import type { AgentClient } from '../explorerContracts';
import type { ScreenProps } from './workspaces';
import { request } from './api';
import { CompleteNativeResponse } from './components';
import { AdminButton } from './adminLinks';
import { Notice, noticeKey } from './notices';

/** The agents Connect offers, with how each picks up a changed entry. */
export const AGENT_CHOICES: readonly { client: AgentClient; label: string; reconnect: string }[] = [
    { client: 'claude', label: 'Claude Code', reconnect: 'A running Claude Code session reconnects with /mcp, or restart Claude Code.' },
    { client: 'codex', label: 'Codex', reconnect: 'Restart Codex to load the changed entry.' },
    { client: 'vscode', label: 'VS Code / GitHub Copilot', reconnect: 'VS Code restarts the AIRK MCP server when its configuration changes; otherwise restart it from MCP: List Servers.' },
];
export interface AgentEntryView { client: AgentClient; agent: string; file: string; scope: string; serverKey: string; connectionId?: string; status: string | null }
export interface AgentsReadView { folder?: string; entries?: AgentEntryView[]; report?: unknown }
interface ConnectView { status?: string; message?: string; file?: string; connectionId?: string; grantCreated?: boolean; replaced?: { revoked?: boolean } | null }

const label = (client: AgentClient) => AGENT_CHOICES.find(choice => choice.client === client)?.label ?? client;
const readable = (props: ScreenProps) => props.state.trusted && props.state.nativeExecutionEnabled !== false && (props.state.connection === 'ready' || props.state.connection === 'degraded');

export function AgentsScreen(props: ScreenProps) {
    const [client, setClient] = useState<AgentClient>('claude');
    const [busy, setBusy] = useState(false), [error, setError] = useState(''), [view, setView] = useState<AgentsReadView>();
    const [connected, setConnected] = useState<{ client: AgentClient; value: ConnectView }>();
    const scope = props.state.searchScope;
    const disabled = busy || !readable(props);
    const run = async (work: () => Promise<void>) => {
        if (busy) { return; }
        setBusy(true); setError('');
        try { await work(); }
        catch (failure) { setError(failure instanceof Error ? failure.message : String(failure)); }
        finally { setBusy(false); }
    };
    const read = () => run(async () => { setView(await request<AgentsReadView>('agents.read', {})); });
    useEffect(() => { if (readable(props)) { void read(); } }, [readable(props)]);
    const connect = () => run(async () => { setConnected({ client, value: await request<ConnectView>('agents.connect', { client }) }); });
    const disconnect = (connectionId: string) => run(async () => { await request('agents.disconnect', { connectionId }); setView(await request<AgentsReadView>('agents.read', {})); });
    const entries = view?.entries ?? [];
    const result = connected?.value;
    return <section class="manage-agents">
        <article class="card">
            <div class="section-head"><h2>Connect a coding agent</h2>
                <AdminButton page="agents" label="Open AIRK Admin" ghost disabled={!props.state.trusted} choose={false} /></div>
            <p>Connect Claude Code, Codex or VS Code / GitHub Copilot to AIRK for this folder. The agent reads the indexes in this window's search scope.</p>
            {!scope?.available ? <p class="callout" role="status">The window search scope is not available from this AIRK Service.</p>
                : scope.active ? <p role="status">Search scope: <strong>{scope.label}</strong>{scope.automatic ? ' (this folder\'s index, chosen automatically)' : ''}</p>
                : <p class="callout" role="status">No search scope is applied in this window. <button type="button" class="ghost" onClick={() => props.go({ workspace: 'indexing', tab: 'window' })}>Choose a search scope</button></p>}
            <div class="actions">
                <label>Agent <select value={client} disabled={disabled} onChange={event => { setClient(event.currentTarget.value as AgentClient); setConnected(undefined); }}>
                    {AGENT_CHOICES.map(choice => <option key={choice.client} value={choice.client}>{choice.label}</option>)}
                </select></label>
                <button type="button" disabled={disabled || !scope?.active} onClick={() => { void connect(); }}>Connect</button>
            </div>
            {result && connected && (result.status === 'pending'
                ? <p class="callout" role="status">{result.message ?? 'Coming in the next step.'}</p>
                : <div role="status">
                    <p>{label(connected.client)} connected{result.file ? <> in <code class="path">{result.file}</code></> : ''}{result.connectionId ? <> (connection <code>{result.connectionId}</code>)</> : ''}.
                        {result.grantCreated === undefined ? '' : result.grantCreated ? ' A new access grant was created for this scope.' : ' An existing access grant was reused.'}
                        {result.replaced ? result.replaced.revoked ? ' The previous connection was revoked.' : ' The previous connection was kept because another entry names it.' : ''}</p>
                    <p class="muted">{AGENT_CHOICES.find(choice => choice.client === connected.client)?.reconnect}</p>
                    <CompleteNativeResponse value={result as Record<string, unknown>} />
                </div>)}
        </article>
        <article class="card">
            <div class="section-head"><h2>Connected agents{view?.folder ? <> for <code class="path">{view.folder}</code></> : ''}</h2>
                <button type="button" class="ghost" disabled={disabled} onClick={() => { void read(); }}>Refresh</button></div>
            {busy && <p role="status">Working…</p>}
            {error && <Notice condition noticeKey={noticeKey('agents-error', error)} role="alert"><p>{error}</p></Notice>}
            {view && (entries.length
                ? <ul class="agent-entries">{entries.map((entry, at) => <li key={`${entry.file}:${entry.serverKey}:${at}`}>
                    <strong>{label(entry.client)}</strong> · <code class="path">{entry.file}</code> · {entry.serverKey}
                    {entry.connectionId ? <> · connection <code>{entry.connectionId}</code></> : ' · no connection'} · {entry.status ?? 'status not observed'}
                    {entry.connectionId && <> <button type="button" class="ghost" disabled={disabled} onClick={() => { void disconnect(entry.connectionId!); }}>Disconnect</button></>}
                </li>)}</ul>
                : <p class="muted">No AIRK agent entries in this folder.</p>)}
            {view?.report !== undefined && <CompleteNativeResponse value={view.report as Record<string, unknown>} />}
        </article>
    </section>;
}
