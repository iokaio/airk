// SPDX-License-Identifier: Apache-2.0
// Management › Agents (U104, U105): connect a coding agent with an open
// connection whose default scope is this window's search scope, list the
// agent entries of this window's folder with each open connection's default
// scope, "Follow this window's search scope" and the "Let the agent choose
// indexes" switch (off by default), and Disconnect. Entries come from
// `airkctl agents list --no-user` and connections from `airkctl agents info`,
// with the complete native reports. Native text is rendered as escaped Preact
// text. Advanced connection management stays in AIRK Admin › Agents.
import { useEffect, useState } from 'preact/hooks';
import type { AgentClient } from '../explorerContracts';
import type { ScreenProps } from './workspaces';
import { request } from './api';
import { CompleteNativeResponse } from './components';
import { AdminButton } from './adminLinks';
import { Notice, noticeKey } from './notices';

/** The agents Connect offers, how each loads a new entry, and how each reloads its tool list. */
export const AGENT_CHOICES: readonly { client: AgentClient; label: string; reconnect: string; reloadTools: string }[] = [
    { client: 'claude', label: 'Claude Code', reconnect: 'If Claude Code is already running, run /mcp (or restart Claude Code) to load the new entry.',
        reloadTools: 'A running Claude Code session may need /mcp to reload its tool list.' },
    { client: 'codex', label: 'Codex', reconnect: 'If Codex is already running, restart it to load the new entry.',
        reloadTools: 'A running Codex session may need a restart to reload its tool list.' },
    { client: 'vscode', label: 'VS Code / GitHub Copilot', reconnect: 'VS Code starts the AIRK MCP server from the new entry; otherwise restart it from MCP: List Servers.',
        reloadTools: 'If the tool list does not change, restart the AIRK server from MCP: List Servers.' },
];
/** What the choice switch means, shown beside it. */
export const CHOICE_OFF_TEXT = 'Off (default): the agent searches only its default scope, which is this window\'s search scope while it follows the window. It cannot list or pick indexes.';
export const CHOICE_ON_TEXT = 'On: the agent can list every index and index set on this computer, with their descriptions and folders, and search any of them. A call that names nothing still uses the default scope.';
export const NO_RECONNECT_TEXT = 'Changing the default scope takes effect on the agent\'s next call; no reconnect is needed.';

export interface AgentEntryView { client: AgentClient; agent: string; file: string; scope: string; serverKey: string; connectionId?: string; status: string | null }
export interface ScopeItemView { kind: 'index' | 'set'; id: string; name?: string; description?: string }
interface FailureView { code: string; message: string }
interface OpenView { defaults_revision: number; default_index_ids: string[]; default_index_set_ids: string[]; agent_choice: 'off' | 'on'; scope: ScopeItemView[] }
export interface AgentConnectionView { connectionId: string; open?: OpenView; remembered: boolean; follows: boolean; info?: unknown; infoError?: FailureView; followError?: FailureView }
export interface AgentsReadView { folder?: string; entries?: AgentEntryView[]; report?: unknown; connections?: AgentConnectionView[]; windowScope?: { items: ScopeItemView[] } | null }
interface ConnectView { status?: string; client?: AgentClient; connectionId?: string; file?: string; changed?: boolean; defaultsError?: FailureView; scope?: ScopeItemView[] | null; follows?: boolean; open?: { agent_choice?: string } }
interface Changed { connectionId: string; text: string }

const label = (client: AgentClient) => AGENT_CHOICES.find(choice => choice.client === client)?.label ?? client;
const readable = (props: ScreenProps) => props.state.trusted && props.state.nativeExecutionEnabled !== false && (props.state.connection === 'ready' || props.state.connection === 'degraded');

/** Names of a default scope; each item's description is its tooltip. */
export function ScopeNames(props: { items: readonly ScopeItemView[] }) {
    if (!props.items.length) { return <span class="muted">none</span>; }
    return <span class="agent-scope">{props.items.map((item, at) => <span key={`${item.kind}:${item.id}`}>{at ? ', ' : ''}
        <span title={item.description ?? item.id}>{item.name ?? item.id}</span>{item.kind === 'set' ? ' (index set)' : ''}</span>)}</span>;
}

export function AgentsScreen(props: ScreenProps) {
    const [client, setClient] = useState<AgentClient>('claude');
    const [busy, setBusy] = useState(false), [error, setError] = useState(''), [view, setView] = useState<AgentsReadView>();
    const [connected, setConnected] = useState<{ client: AgentClient; value: ConnectView }>();
    const [changed, setChanged] = useState<Changed>();
    const scope = props.state.searchScope;
    const disabled = busy || !readable(props);
    const run = async (work: () => Promise<void>) => {
        if (busy) { return; }
        setBusy(true); setError('');
        try { await work(); }
        catch (failure) { setError(failure instanceof Error ? failure.message : String(failure)); }
        finally { setBusy(false); }
    };
    const reread = async () => { setView(await request<AgentsReadView>('agents.read', {})); };
    const read = () => run(reread);
    useEffect(() => { if (readable(props)) { void read(); } }, [readable(props)]);
    const connect = () => run(async () => { setChanged(undefined); setConnected({ client, value: await request<ConnectView>('agents.connect', { client }) }); await reread(); });
    const disconnect = (connectionId: string) => run(async () => { await request('agents.disconnect', { connectionId }); setChanged(undefined); await reread(); });
    const choose = (entry: AgentEntryView, on: boolean) => run(async () => {
        await request('agents.choice', { connectionId: entry.connectionId!, choice: on ? 'on' : 'off' });
        setChanged({ connectionId: entry.connectionId!, text: `Agent choice turned ${on ? 'on' : 'off'}. ${AGENT_CHOICES.find(choice => choice.client === entry.client)?.reloadTools ?? ''}` });
        await reread();
    });
    const follow = (connectionId: string, value: boolean) => run(async () => {
        const result = await request<{ error?: FailureView }>('agents.follow', { connectionId, follow: value });
        setChanged({ connectionId, text: result?.error ? `Following is on, but this window's search scope was not passed on: ${result.error.message}`
            : value ? `This connection follows this window's search scope. ${NO_RECONNECT_TEXT}` : 'This connection keeps its current default scope.' });
        await reread();
    });
    const entries = view?.entries ?? [];
    const connections = view?.connections ?? [];
    const result = connected?.value;
    return <section class="manage-agents">
        <article class="card">
            <div class="section-head"><h2>Connect a coding agent</h2>
                <AdminButton page="agents" label="Open AIRK Admin" ghost disabled={!props.state.trusted} choose={false} /></div>
            <p>Connect Claude Code, Codex or VS Code / GitHub Copilot to AIRK for this folder. The agent's default scope is this window's search scope and follows it when you apply another one. The agent cannot list or pick other indexes unless you turn on "Let the agent choose indexes" for it below.</p>
            {!scope?.available ? <p class="callout" role="status">The window search scope is not available from this AIRK Service.</p>
                : scope.active ? <p role="status">Search scope: <strong>{scope.label}</strong>{scope.automatic ? ' (this folder\'s index, chosen automatically)' : ''}
                    {view?.windowScope?.items.length ? <> · <ScopeNames items={view.windowScope.items} /></> : ''}</p>
                : <p class="callout" role="status">No search scope is applied in this window. <button type="button" class="ghost" onClick={() => props.go({ workspace: 'indexing', tab: 'window' })}>Choose a search scope</button></p>}
            <div class="actions">
                <label>Agent <select value={client} disabled={disabled} onChange={event => { setClient(event.currentTarget.value as AgentClient); setConnected(undefined); }}>
                    {AGENT_CHOICES.map(choice => <option key={choice.client} value={choice.client}>{choice.label}</option>)}
                </select></label>
                <button type="button" disabled={disabled || !scope?.active} onClick={() => { void connect(); }}>Connect</button>
            </div>
            {result && connected && <div role="status">
                {result.status === 'already_connected'
                    ? <p>{label(connected.client)} is already connected in this folder{result.connectionId ? <> (connection <code>{result.connectionId}</code>)</> : ''}. {result.defaultsError ? '' : `Its default scope is now this window's search scope. ${NO_RECONNECT_TEXT}`}</p>
                    : <><p>{label(connected.client)} connected{result.file ? <> in <code class="path">{result.file}</code></> : ''}{result.connectionId ? <> (connection <code>{result.connectionId}</code>)</> : ''}.
                        {result.scope && !result.defaultsError ? <> Default scope: <ScopeNames items={result.scope} />.</> : ''} It follows this window's search scope. Agent choice is {result.open?.agent_choice === 'on' ? 'on' : 'off'}.</p>
                        {result.changed !== false && <p class="muted">{AGENT_CHOICES.find(choice => choice.client === connected.client)?.reconnect}</p>}</>}
                {result.defaultsError && <p class="callout" role="alert">The default scope was not set ({result.defaultsError.code}): {result.defaultsError.message} Until it is set, the agent's searches are refused; apply a search scope again to retry.</p>}
                <CompleteNativeResponse value={result as Record<string, unknown>} />
            </div>}
        </article>
        <article class="card">
            <div class="section-head"><h2>Connected agents{view?.folder ? <> for <code class="path">{view.folder}</code></> : ''}</h2>
                <button type="button" class="ghost" disabled={disabled} onClick={() => { void read(); }}>Refresh</button></div>
            {busy && <p role="status">Working…</p>}
            {error && <Notice condition noticeKey={noticeKey('agents-error', error)} role="alert"><p>{error}</p></Notice>}
            {view && (entries.length
                ? <ul class="agent-entries">{entries.map((entry, at) => {
                    const connection = entry.connectionId ? connections.find(item => item.connectionId === entry.connectionId) : undefined;
                    // A connection named by several entries shows its settings once.
                    const first = !!connection && entries.findIndex(other => other.connectionId === entry.connectionId) === at;
                    const open = connection?.open;
                    return <li key={`${entry.file}:${entry.serverKey}:${at}`}>
                        <strong>{label(entry.client)}</strong> · <code class="path">{entry.file}</code> · {entry.serverKey}
                        {entry.connectionId ? <> · connection <code>{entry.connectionId}</code></> : ' · no connection'} · {entry.status ?? 'status not observed'}
                        {entry.connectionId && <> <button type="button" class="ghost" disabled={disabled} onClick={() => { void disconnect(entry.connectionId!); }}>Disconnect</button></>}
                        {first && open && <div class="agent-connection">
                            <p>Default scope: <ScopeNames items={open.scope} /></p>
                            <label><input type="checkbox" checked={connection!.follows} disabled={disabled} onChange={event => { void follow(entry.connectionId!, event.currentTarget.checked); }} /> Follow this window's search scope</label>
                            <label class="agent-choice"><input type="checkbox" role="switch" aria-checked={open.agent_choice === 'on'} checked={open.agent_choice === 'on'} disabled={disabled}
                                onChange={event => { void choose(entry, event.currentTarget.checked); }} /> Let the agent choose indexes</label>
                            <p class="muted">{open.agent_choice === 'on' ? CHOICE_ON_TEXT : CHOICE_OFF_TEXT}</p>
                            {connection!.followError && <p class="callout" role="alert">This window's search scope was not passed on ({connection!.followError.code}): {connection!.followError.message}</p>}
                            {changed && changed.connectionId === entry.connectionId &&<p role="status">{changed.text}</p>}
                        </div>}
                        {first && !open && !connection!.infoError && <p class="muted">This connection reads the scope it was connected with; manage it in AIRK Admin › Agents.</p>}
                        {first && connection!.infoError && <p class="callout" role="status">The connection could not be read ({connection!.infoError.code}): {connection!.infoError.message}</p>}
                    </li>;
                })}</ul>
                : <p class="muted">No AIRK agent entries in this folder.</p>)}
            {view?.report !== undefined && <CompleteNativeResponse value={view.report as Record<string, unknown>} />}
            {connections.some(item => item.info !== undefined) && <CompleteNativeResponse value={{ connections: connections.map(item => ({ connectionId: item.connectionId, info: item.info ?? null })) }} />}
        </article>
    </section>;
}
