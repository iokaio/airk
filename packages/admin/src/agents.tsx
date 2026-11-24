// SPDX-License-Identifier: Apache-2.0
// Agents: how a coding agent reaches this service's MCP endpoint (U110). The
// page explains connecting an agent and choosing its scope (per call, or the
// workspace default a VS Code window's applied search scope stores), reads the
// live endpoint (agents.endpoint, M15a), and shows manual snippets in the exact
// shape `airkctl agents connect` writes: the endpoint URL and the response-mode
// header only. The endpoint is plain HTTP on 127.0.0.1 (local trust plan C) and
// needs no token, so no certificate, credential, connection id or header helper
// appears here. Every read keeps its complete native result in a collapsed JSON
// block.
import { Fragment, type ComponentChildren, type JSX } from 'preact';
import { useEffect, useRef, useState } from 'preact/hooks';
import { body, OPS } from './operations';
import type { AgentManualEntry } from '../../contracts-gen/i3';
import { Outcome, Panel, Text, visible, type CallState, useCall } from './render';

// ---------------------------------------------------------------------------
// Small readers over untyped native JSON.

type Obj = Record<string, unknown>;

function obj(value: unknown): Obj | undefined {
  return typeof value === 'object' && value !== null && !Array.isArray(value) ? (value as Obj) : undefined;
}

function str(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined;
}

function okResult(state: CallState): Obj | undefined {
  return state.phase === 'done' && state.result.kind === 'ok' ? obj(state.result.envelope.result) : undefined;
}

/** Readable sentences for failure codes these reads can answer. */
const FAILURES: Record<string, string> = {
  not_yet_served: 'This service version does not answer this read yet.',
  operation_failed: 'The service could not complete the read.',
  admin_malformed_request: 'The service did not accept the request shape (a page and service version mismatch).',
  admin_operation_unknown: 'This service version does not know this operation.',
};

/** The problem with a finished call, in words, or null when it succeeded. */
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
      const native = obj(e.native);
      const detail = str(native?.message);
      const text = FAILURES[e.code] ?? `The service refused the request (${e.class}).`;
      return detail === undefined ? { text, code: e.code } : { text, code: e.code, detail };
    }
    case 'ok': {
      const result = obj(r.envelope.result);
      const status = str(result?.status);
      if (status === 'agent_refused') {
        const message = str(result?.message);
        return message === undefined
          ? { text: 'The service refused the read.', code: status }
          : { text: 'The service refused the read.', code: status, detail: message };
      }
      return null;
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

/** The complete native result (or failure), collapsed. */
function Complete(props: { state: CallState; label?: string }): JSX.Element | null {
  const s = props.state;
  if (s.phase === 'idle') return null;
  if (s.phase === 'pending') return <p class="muted">Loading…</p>;
  return (
    <details class="au-complete">
      <summary>{props.label ?? 'Complete result (JSON)'}</summary>
      <Outcome state={s} />
    </details>
  );
}

function CopyButton(props: { text: string; label?: string }): JSX.Element {
  const [message, setMessage] = useState(''), [pending, setPending] = useState(false);
  const current = useRef(props.text); current.current = props.text;
  useEffect(() => { setMessage(''); setPending(false); }, [props.text]);
  const copy = async (): Promise<void> => {
    const text = props.text;
    setMessage(''); setPending(true);
    try {
      if (!navigator.clipboard?.writeText) throw new Error('Clipboard access is unavailable. Select and copy the entry below.');
      await navigator.clipboard.writeText(text);
      if (current.current === text) setMessage('Copied');
    } catch {
      if (current.current === text) setMessage('Copy failed. Select and copy the entry below.');
    } finally { if (current.current === text) setPending(false); }
  };
  return <span class="au-copy"><button type="button" class="small" disabled={pending} onClick={() => void copy()}>
    {pending ? 'Copying...' : message === 'Copied' ? 'Copied' : (props.label ?? 'Copy')}
  </button>{message && message !== 'Copied' ? <span role="alert"> {message}</span> : null}</span>;
}

/** A one-line command with Copy. */
function Cmd(props: { text: string }): JSX.Element {
  return (
    <div class="command">
      <code>{visible(props.text)}</code>
      <CopyButton text={props.text} />
    </div>
  );
}

/** A multi-line snippet with a caption and Copy. */
function Snippet(props: { caption: ComponentChildren; text: string }): JSX.Element {
  return (
    <figure class="au-snippet">
      <figcaption>
        <span>{props.caption}</span>
        <CopyButton text={props.text} />
      </figcaption>
      <pre>{visible(props.text)}</pre>
    </figure>
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

function Badge(props: { tone: 'ok' | 'warn' | 'bad' | 'neutral'; children: ComponentChildren }): JSX.Element {
  return <span class={`au-badge au-${props.tone}`}>{props.children}</span>;
}

// ---------------------------------------------------------------------------
// Native manual entries preserve Rust's escaping and exact bytes.
const MODE_HEADER = 'X-AIRK-Response-Mode';

export function manualEntriesOf(state: CallState): AgentManualEntry[] {
  const manual = okResult(state)?.manual;
  return Array.isArray(manual) ? manual.filter((entry): entry is AgentManualEntry => {
    const value = obj(entry);
    return !!value && ['claude', 'codex', 'vscode'].includes(String(value.client))
      && ['project', 'user'].includes(String(value.scope)) && ['json', 'toml'].includes(String(value.format))
      && typeof value.file === 'string' && typeof value.text === 'string';
  }) : [];
}

// ---------------------------------------------------------------------------
// (1) Connect an agent.

function ConnectCard(): JSX.Element {
  return (
    <Panel title="Connect an agent">
      <p>
        AIRK writes only its own entry in the agent's configuration file: the endpoint URL and the{' '}
        <code>{MODE_HEADER}</code> header. No token or credential is involved, and your other servers stay as they
        are.
      </p>
      <h3>From VS Code</h3>
      <p>
        Open the AIRK Explorer, go to Management → Agents, pick Claude Code, Codex or VS Code / GitHub Copilot, and
        choose <strong>Connect</strong>.
      </p>
      <h3>From a terminal</h3>
      <Cmd text="airkctl agents connect --client claude --project <project folder>" />
      <p class="muted">
        Use <code>--client codex</code> or <code>--client vscode</code> for the other agents, and{' '}
        <code>--user</code> to write the user-level file instead of the project's. See every entry AIRK manages with{' '}
        <code>airkctl agents list</code>.
      </p>
      <h3>Which indexes an agent reads</h3>
      <p>
        Agents choose the scope on each call: read tools take <code>index_ids</code>, <code>index_set_ids</code>{' '}
        (ids or names) and <code>workspace</code> (an absolute folder path). <code>list_scopes</code> lists the
        indexes, index sets and stored workspace defaults. A call that names no indexes uses the workspace default of
        the most specific stored folder containing <code>workspace</code>, and is refused with{' '}
        <code>scope_required</code> without one. If the agent supports MCP roots, its workspace root is used
        automatically.
      </p>
      <p>
        Applying a search scope in a VS Code window stores it as the workspace default of that window's folders. From
        a terminal:
      </p>
      <Cmd text="airkctl scope default --workspace <folder> --index <index id> --set <index set id>" />
      <p class="muted">
        Without <code>--index</code> or <code>--set</code> the folder's default is cleared. List every stored
        default with <code>airkctl scope defaults</code>.
      </p>
    </Panel>
  );
}

// ---------------------------------------------------------------------------
// (2) Endpoint.

const STATE_TEXT: Record<string, [string, 'ok' | 'warn' | 'bad']> = {
  ready: ['Ready: agents can connect now.', 'ok'],
  recovering: ['Recovering: the service is restoring its state; connections may be refused briefly.', 'warn'],
  stopping: ['Stopping: the service is shutting down; connections will fail until it starts again.', 'bad'],
};

type Endpoint = { url: string; state?: string; generation?: unknown };

function endpointOf(state: CallState): Endpoint | undefined {
  const result = okResult(state);
  if (str(result?.status) !== 'agent_endpoint') return undefined;
  const endpoint = obj(result?.endpoint);
  const url = str(endpoint?.url);
  if (endpoint === undefined || url === undefined) return undefined;
  const out: Endpoint = { url, generation: endpoint.generation };
  const s = str(endpoint.state);
  if (s !== undefined) out.state = s;
  return out;
}

function EndpointCard(props: { endpoint: CallState; onReload: () => void }): JSX.Element {
  const e = endpointOf(props.endpoint);
  const stateText = e?.state !== undefined ? STATE_TEXT[e.state] : undefined;
  return (
    <Panel title="Your AIRK MCP endpoint">
      {e !== undefined ? (
        <>
          <p>
            {stateText !== undefined ? (
              <>
                <Badge tone={stateText[1]}>{e.state}</Badge> {stateText[0]}
              </>
            ) : (
              <Text value={e.state ?? 'State unknown.'} />
            )}
          </p>
          <div class="au-url">
            <code>
              <Text value={e.url} />
            </code>
            <CopyButton text={e.url} label="Copy URL" />
          </div>
          <p class="muted">
            Every agent on this machine connects to this one address, without a token. The port can change when the
            service restarts on a busy port, so copy it from here rather than assuming a fixed number.
          </p>
        </>
      ) : props.endpoint.phase !== 'done' ? (
        <p class="muted">Reading the endpoint…</p>
      ) : problemOf(props.endpoint) === null ? (
        <p class="error">The service answered without an endpoint; see the complete result below.</p>
      ) : null}
      <Problem state={props.endpoint} />
      <div class="row">
        <button type="button" class="secondary" onClick={props.onReload}>
          Refresh
        </button>
      </div>
      <details class="au-complete">
        <summary>Troubleshooting</summary>
        <Facts
          rows={[
            ['Transport', 'Streamable HTTP over plain HTTP (IPv4 loopback only, no certificate)'],
          ]}
        />
        <Complete state={props.endpoint} label="Complete endpoint result (JSON)" />
      </details>
    </Panel>
  );
}

// ---------------------------------------------------------------------------
// (3) Manual configuration.

type Client = 'claude' | 'codex' | 'vscode' | 'other';

const CLIENTS: { id: Client; label: string }[] = [
  { id: 'claude', label: 'Claude Code' },
  { id: 'codex', label: 'Codex' },
  { id: 'vscode', label: 'VS Code / Copilot' },
  { id: 'other', label: 'Other MCP clients' },
];

export function ManualCard(props: { entries: readonly AgentManualEntry[]; url: string | undefined }): JSX.Element {
  const [client, setClient] = useState<Client>('claude');
  const url = props.url ?? 'http://127.0.0.1:<port>/mcp';
  const manual = props.entries.find(entry => entry.client === client);
  const snippet = manual ? <Snippet caption={manual.file} text={manual.text} /> : <p role="status">The service has not reported a manual entry for this agent. Refresh the endpoint, or run <code>airkctl agents manual --client {client} --project &lt;folder&gt;</code>.</p>;
  let panel: JSX.Element;
  switch (client) {
    case 'claude':
      panel = (
        <>
          <p>
            Add this to <code>.mcp.json</code> in your project folder (or merge the <code>airk</code> entry into an
            existing <code>mcpServers</code>).
          </p>
          {snippet}
        </>
      );
      break;
    case 'codex':
      panel = (
        <>
          <p>
            Add this to <code>~/.codex/config.toml</code> (or the project's <code>.codex/config.toml</code>).
          </p>
          {snippet}
        </>
      );
      break;
    case 'vscode':
      panel = (
        <>
          <p>
            Add this to <code>.vscode/mcp.json</code> (or your user <code>mcp.json</code>).
          </p>
          {snippet}
        </>
      );
      break;
    case 'other':
      panel = (
        <>
          <p>Give the client these settings:</p>
          <Facts
            rows={[
              [
                'URL',
                <span class="au-inline">
                  <code>
                    <Text value={url} />
                  </code>
                  <CopyButton text={url} />
                </span>,
              ],
              ['Transport', 'Streamable HTTP (plain HTTP on 127.0.0.1). Stdio and SSE are not offered.'],
              [
                'Header',
                <>
                  Optionally <code>{MODE_HEADER}: terse</code>. No authorization header is needed.
                </>,
              ],
              ['Certificate', 'None: the endpoint is plain HTTP on the loopback address, so there is nothing to trust.'],
            ]}
          />
        </>
      );
      break;
  }

  return (
    <Panel title="Set up manually">
      <p>
        The service supplies these complete entries for the current endpoint. Merge the AIRK entry into any existing file, preserving other settings, so{' '}
        <code>airkctl agents list</code> recognizes them by their URL.
      </p>
      <p class="muted">If the service changes port, copy a fresh entry here or run <code>airkctl agents endpoint</code>.</p>
      <div class="au-tabs" role="tablist" aria-label="Agent">
        {CLIENTS.map((c) => (
          <button
            type="button"
            role="tab"
            id={`au-tab-${c.id}`}
            aria-selected={client === c.id}
            aria-controls="au-tabpanel"
            class={client === c.id ? 'au-tab current' : 'au-tab'}
            onClick={() => setClient(c.id)}
          >
            {c.label}
          </button>
        ))}
      </div>
      <div class="au-tabpanel" role="tabpanel" id="au-tabpanel" aria-labelledby={`au-tab-${client}`}>
        {panel}
        <h4>Check it</h4>
        <p class="muted">
          Restart or reload the agent's MCP connection, then confirm it lists AIRK's tools (for example by calling{' '}
          <code>status</code> or <code>list_scopes</code>).
        </p>
      </div>
    </Panel>
  );
}

// ---------------------------------------------------------------------------

export function Agents(): JSX.Element {
  const [endpoint, runEndpoint] = useCall();
  const reload = (): void => {
    void runEndpoint(OPS.agentsEndpoint, body.agentsEndpoint());
  };
  useEffect(reload, []);
  const e = endpointOf(endpoint);
  return (
    <>
      <ConnectCard />
      <EndpointCard endpoint={endpoint} onReload={reload} />
      <ManualCard entries={manualEntriesOf(endpoint)} url={e?.url} />
    </>
  );
}
