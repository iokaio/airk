// SPDX-License-Identifier: Apache-2.0
// Agents: how a coding agent reaches this service's MCP endpoint. The page reads
// the live endpoint (agents.endpoint, M15a) and certificate trust
// (https.trust_status), explains automatic and manual setup with snippets in the
// exact shapes `airkctl agents connect` writes (crates/airkctl/src/agents/config),
// and looks up, verifies or revokes an existing connection. Creating a
// connection (credential enrollment) is never served over HTTP; the page only
// shows the commands. No bearer token is ever read, shown or logged here: the
// snippets name the credential helper command, which runs on the user's machine.
// Every read keeps its complete native result in a collapsed JSON block.
import { Fragment, type ComponentChildren, type JSX } from 'preact';
import { useEffect, useRef, useState } from 'preact/hooks';
import { call } from './api';
import { body, isIncarnation, OPS } from './operations';
import { Confirm, Field, Outcome, Panel, Text, visible, type CallState, useCall } from './render';

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

function copyText(text: string): void {
  void navigator.clipboard?.writeText(text);
}

function CopyButton(props: { text: string; label?: string }): JSX.Element {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      class="small"
      onClick={() => {
        copyText(props.text);
        setCopied(true);
        window.setTimeout(() => setCopied(false), 1500);
      }}
    >
      {copied ? 'Copied' : (props.label ?? 'Copy')}
    </button>
  );
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
// Snippet text. Shapes mirror crates/airkctl/src/agents/config/edit.rs
// (json_fields, env_header, input/input_header, MODE), recognize.rs
// (helper_command: HELPER_MIDDLE/HELPER_END and its quoting) and toml.rs
// (append_server).

const MODE_HEADER = 'X-AIRK-Response-Mode';
const ID_PLACEHOLDER = '<connection-id>';
const CA_PLACEHOLDER = '<path to the AIRK CA certificate file>';

/**
 * recognize.rs `helper_command`'s rule for the airkctl path: absolute, at most
 * 1024 characters, no shell metacharacters or controls, not a UNC path, and
 * named airkctl / airkctl.exe. Only such a helper is written and recognized.
 */
function helperPathOk(airkctl: string): boolean {
  const name = airkctl.split(/[\\/]/).pop() ?? airkctl;
  return (
    isAbsolutePath(airkctl) &&
    !airkctl.startsWith('\\\\') &&
    airkctl.length <= 1024 &&
    !/["'`$%!\u0000-\u001f\u007f]/.test(airkctl) &&
    (name === 'airkctl' || name.toLowerCase() === 'airkctl.exe')
  );
}

/** recognize.rs helper_command: plain when every byte is safe, else double-quoted. */
function helperCommand(airkctl: string, id: string): string {
  const path = helperPathOk(airkctl) ? airkctl : '<full path to airkctl>';
  const plain = /^[A-Za-z0-9_.:\\/+-]+$/.test(path);
  const executable = plain ? path : `"${path}"`;
  return `${executable} agents credential --connection-id ${id} --format header-json`;
}

function isAbsolutePath(path: string): boolean {
  return /^[A-Za-z]:[\\/]/.test(path) || path.startsWith('/') || path.startsWith('\\\\');
}

function envHeader(id: string): string {
  const name = isIncarnation(id) ? id.toUpperCase() : 'CONNECTION_ID';
  return `Bearer \${AIRK_MCP_TOKEN_${name}}`;
}

function claudeJson(url: string, id: string, airkctl: string): string {
  return JSON.stringify(
    {
      mcpServers: {
        airk: {
          type: 'http',
          url,
          headers: { Authorization: envHeader(id), [MODE_HEADER]: 'terse' },
          headersHelper: helperCommand(airkctl, id),
        },
      },
    },
    null,
    2,
  );
}

function vscodeJson(url: string, id: string): string {
  const input = `airkToken_${id}`;
  return JSON.stringify(
    {
      servers: {
        airk: {
          type: 'http',
          url,
          headers: { Authorization: `Bearer \${input:${input}}`, [MODE_HEADER]: 'terse' },
        },
      },
      inputs: [{ id: input, type: 'command', command: 'airk.httpCredential', args: { connection_id: id } }],
    },
    null,
    2,
  );
}

/** TOML basic strings accept JSON string escapes for these values. */
function tomlString(value: string): string {
  return JSON.stringify(value);
}

function codexToml(url: string, id: string, airkctl: string): string {
  return [
    '[mcp_servers.airk]',
    `url = ${tomlString(url)}`,
    `http_headers_helper = ${tomlString(helperCommand(airkctl, id))}`,
    '',
    '[mcp_servers.airk.http_headers]',
    `"${MODE_HEADER}" = "terse"`,
    '',
  ].join('\n');
}

function isWindowsPlatform(platform: string | undefined): boolean {
  if (platform !== undefined) return platform === 'windows';
  return navigator.userAgent.includes('Windows');
}

function setEnvCommand(name: string, value: string, windows: boolean): string {
  const literal = windows ? `'${value.replaceAll("'", "''")}'` : `'${value.replaceAll("'", `'"'"'`)}'`;
  return windows ? `$env:${name} = ${literal}` : `export ${name}=${literal}`;
}

// ---------------------------------------------------------------------------
// (1) Endpoint and trust.

const STATE_TEXT: Record<string, [string, 'ok' | 'warn' | 'bad']> = {
  ready: ['Ready: agents can connect now.', 'ok'],
  recovering: ['Recovering: the service is restoring its state; connections may be refused briefly.', 'warn'],
  stopping: ['Stopping: the service is shutting down; connections will fail until it starts again.', 'bad'],
};

type Endpoint = { url: string; state?: string; ca_sha256?: string; certificate_sha256?: string; generation?: unknown };

function endpointOf(state: CallState): Endpoint | undefined {
  const result = okResult(state);
  if (str(result?.status) !== 'agent_endpoint') return undefined;
  const endpoint = obj(result?.endpoint);
  const url = str(endpoint?.url);
  if (endpoint === undefined || url === undefined) return undefined;
  const out: Endpoint = { url, generation: endpoint.generation };
  const s = str(endpoint.state);
  if (s !== undefined) out.state = s;
  const ca = str(endpoint.ca_sha256);
  if (ca !== undefined) out.ca_sha256 = ca;
  const cert = str(endpoint.certificate_sha256);
  if (cert !== undefined) out.certificate_sha256 = cert;
  return out;
}

type Trust = {
  trusted: boolean;
  present: boolean;
  owned: boolean;
  outcome?: string;
  store?: string;
  platform?: string;
  guidance?: string;
  ca_sha256?: string;
};

function trustOf(state: CallState): Trust | undefined {
  const r = okResult(state);
  if (r === undefined || typeof r.trusted !== 'boolean') return undefined;
  const t: Trust = { trusted: r.trusted, present: r.present === true, owned: r.owned === true };
  for (const key of ['outcome', 'store', 'platform', 'guidance', 'ca_sha256'] as const) {
    const value = str(r[key]);
    if (value !== undefined) t[key] = value;
  }
  return t;
}

function TrustSummary(props: { trust: Trust; ca: string | undefined }): JSX.Element {
  const t = props.trust;
  const ca = t.ca_sha256 ?? props.ca;
  const where = t.store !== undefined ? ` (${t.store})` : '';
  let headline: JSX.Element;
  if (t.trusted) {
    headline = (
      <p>
        <Badge tone="ok">Trusted</Badge> Your account's certificate store{where} trusts AIRK's local HTTPS
        issuer{t.owned ? ', installed by AIRK' : ''}. Clients that use the operating system's store can connect
        without extra steps.
      </p>
    );
  } else if (t.outcome === 'unsupported') {
    headline = (
      <p>
        <Badge tone="warn">No OS trust on this platform</Badge> AIRK cannot add its issuer to a per-user store
        here. Give each client the AIRK CA certificate file instead (see the per-client steps below).
      </p>
    );
  } else {
    headline = (
      <p>
        <Badge tone="bad">Not trusted</Badge> AIRK's local HTTPS issuer is{' '}
        {t.present ? 'in your certificate store but not trusted' : 'not in your certificate store'}
        {where}. Agents will refuse the connection until it is trusted. Run this in a terminal on this machine;
        the operating system may ask you to confirm:
      </p>
    );
  }
  return (
    <div class="au-trust">
      {headline}
      {!t.trusted && t.outcome !== 'unsupported' && ca !== undefined ? (
        <Cmd text={`airkctl service trust install --expected-ca-sha256 ${ca}`} />
      ) : null}
      {t.guidance !== undefined ? (
        <p class="muted">
          <Text value={t.guidance} />
        </p>
      ) : null}
      <p class="muted">
        Some clients use their own certificate list rather than the system store. For those, add the AIRK CA
        file as an additional certificate, as shown in each client&apos;s steps below. Check the store again
        with <code>airkctl service trust inspect</code>.
      </p>
    </div>
  );
}

function EndpointCard(props: { endpoint: CallState; trust: CallState; onReload: () => void }): JSX.Element {
  const e = endpointOf(props.endpoint);
  const t = trustOf(props.trust);
  const stateText = e?.state !== undefined ? STATE_TEXT[e.state] : undefined;
  return (
    <Panel title="Your AIRK MCP endpoint">
      <p class="muted">
        Every agent on this machine connects to this one address. The port can change when the service restarts
        on a busy port, so copy it from here rather than assuming a fixed number.
      </p>
      {e !== undefined ? (
        <>
          <div class="au-url">
            <code>
              <Text value={e.url} />
            </code>
            <CopyButton text={e.url} label="Copy URL" />
          </div>
          <Facts
            rows={[
              [
                'State',
                stateText !== undefined ? (
                  <>
                    <Badge tone={stateText[1]}>{e.state}</Badge> {stateText[0]}
                  </>
                ) : (
                  <Text value={e.state ?? 'unknown'} />
                ),
              ],
              ['Transport', 'Streamable HTTP over HTTPS (IPv4 loopback only)'],
              [
                'Issuer (CA) SHA-256',
                <code>
                  <Text value={e.ca_sha256 ?? 'not reported'} />
                </code>,
              ],
            ]}
          />
        </>
      ) : props.endpoint.phase !== 'done' ? (
        <p class="muted">Reading the endpoint…</p>
      ) : problemOf(props.endpoint) === null ? (
        <p class="error">The service answered without an endpoint; see the complete result below.</p>
      ) : null}
      <Problem state={props.endpoint} />
      <h3>Certificate trust</h3>
      {t !== undefined ? <TrustSummary trust={t} ca={e?.ca_sha256} /> : null}
      <Problem state={props.trust} />
      <div class="row">
        <button type="button" class="secondary" onClick={props.onReload}>
          Refresh
        </button>
      </div>
      <Complete state={props.endpoint} label="Complete endpoint result (JSON)" />
      <Complete state={props.trust} label="Complete trust result (JSON)" />
    </Panel>
  );
}

// ---------------------------------------------------------------------------
// (2) Automatic setup.

function newConnectionId(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return [...bytes].map((byte) => byte.toString(16).padStart(2, '0')).join('');
}

function AutomaticCard(props: { id: string }): JSX.Element {
  const [fresh, setFresh] = useState(newConnectionId);
  const shown = isIncarnation(props.id) ? props.id : ID_PLACEHOLDER;
  return (
    <Panel title="Connect an agent automatically">
      <p>
        Connecting creates a credential for one reviewed search scope, so it is done on this machine, never from
        this page. Choose one of these:
      </p>
      <h3>From VS Code</h3>
      <ol class="au-steps">
        <li>Open the AIRK Explorer and go to Indexing → Search scope.</li>
        <li>Review the indexes the agent may read, then choose Apply selection.</li>
        <li>
          Choose <strong>Connect Coding Agent</strong> and pick Claude Code, Codex or VS Code. AIRK writes only its
          own entry in that agent's configuration file and keeps your other servers.
        </li>
      </ol>
      <h3>From a terminal</h3>
      <p>Enroll a reviewed selection under a new connection id (here is a freshly generated one):</p>
      <Cmd
        text={`airkctl agents connect --client claude --connection-id ${fresh} --selection-file <reviewed-selection.json>`}
      />
      <div class="row">
        <button type="button" class="small" onClick={() => setFresh(newConnectionId())}>
          Generate another id
        </button>
      </div>
      <p class="muted">
        Use <code>--client codex</code> or <code>--client vscode</code> for the other agents, and{' '}
        <code>--user</code> to write the user-level file instead of the project's. The selection file is the
        owner-private reviewed selection; AIRK never derives scope from a folder. Reuse the same id when retrying
        an uncertain attempt.
      </p>
      <p>Add an existing connection to another agent, or start an agent with it directly:</p>
      <Cmd text={`airkctl agents connect --client codex --connection-id ${shown}`} />
      <Cmd text={`airkctl agents launch --client claude --connection-id ${shown}`} />
      <p class="muted">
        <code>launch</code> starts the agent in the foreground with the credential in its environment and the AIRK
        issuer added to its trust, without changing any file. See every entry AIRK manages with{' '}
        <code>airkctl agents list</code>; fix stale ones with <code>airkctl agents repair</code>.
      </p>
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

function ManualCard(props: {
  url: string | undefined;
  ca: string | undefined;
  caFile: string | undefined;
  platform: string | undefined;
  id: string;
  setId: (id: string) => void;
}): JSX.Element {
  const [client, setClient] = useState<Client>('claude');
  const [airkctl, setAirkctl] = useState('airkctl');
  // A ref, so the inventory answer never replaces a path typed meanwhile.
  const airkctlTouched = useRef(false);
  // The installed airkctl: the program location (storage inventory) and the
  // install record's `bin/airkctl(.exe)`, until the user types another path.
  useEffect(() => {
    void (async () => {
      const result = await call(OPS.storageInventory, body.storageInventory());
      if (result.kind !== 'ok' || airkctlTouched.current) return;
      const inventory = (result.envelope.result as { inventory?: { locations?: { kind: string; path: string }[] } })
        ?.inventory;
      const program = inventory?.locations?.find((location) => location.kind === 'program')?.path;
      if (program === undefined) return;
      const windowsPath = program.includes('\\');
      setAirkctl(windowsPath ? `${program}\\bin\\airkctl.exe` : `${program}/bin/airkctl`);
    })();
  }, []);
  const idValid = isIncarnation(props.id);
  const id = idValid ? props.id : ID_PLACEHOLDER;
  const url = props.url ?? 'https://127.0.0.1:<port>/mcp';
  const caFile = props.caFile ?? CA_PLACEHOLDER;
  const windows = isWindowsPlatform(props.platform);
  const helper = helperCommand(airkctl, id);
  const trustCommand =
    props.ca !== undefined
      ? `airkctl service trust install --expected-ca-sha256 ${props.ca}`
      : 'airkctl service trust install --expected-ca-sha256 <issuer-sha256>';

  const caFileNote = (
    <p class="muted">
      {props.caFile !== undefined
        ? 'The CA file path comes from the connection you looked up below.'
        : 'Look up your connection below to fill in the CA certificate file path.'}{' '}
      If the variable already names a bundle, add the AIRK certificate to a combined copy instead of replacing it,
      so your other trusted roots keep working. Never turn off certificate checks.
    </p>
  );

  let panel: JSX.Element;
  switch (client) {
    case 'claude':
      panel = (
        <>
          <p>
            Add this to <code>.mcp.json</code> in your project folder (or merge the <code>airk</code> entry into an
            existing <code>mcpServers</code>). Claude Code runs <code>headersHelper</code> to get the token; the{' '}
            <code>Authorization</code> line is a fallback that reads an environment variable.
          </p>
          <Snippet caption=".mcp.json" text={claudeJson(url, id, airkctl)} />
          <h4>Trust</h4>
          <p>Start Claude Code with the AIRK CA certificate added to Node's trusted roots:</p>
          <Cmd text={setEnvCommand('NODE_EXTRA_CA_CERTS', caFile, windows)} />
          {caFileNote}
        </>
      );
      break;
    case 'codex':
      panel = (
        <>
          <p>
            Add this to <code>~/.codex/config.toml</code> (or the project's <code>.codex/config.toml</code>). Codex
            runs <code>http_headers_helper</code> to get the token.
          </p>
          <Snippet caption="config.toml" text={codexToml(url, id, airkctl)} />
          <p class="muted">
            If your Codex version cannot run a header helper, use{' '}
            <code>bearer_token_env_var = "AIRK_MCP_TOKEN"</code> instead and set that variable from the credential
            command for that process only.
          </p>
          <h4>Trust</h4>
          <p>Start Codex with the AIRK CA certificate:</p>
          <Cmd text={setEnvCommand('CODEX_CA_CERTIFICATE', caFile, windows)} />
          {caFileNote}
        </>
      );
      break;
    case 'vscode':
      panel = (
        <>
          <p>
            Add this to <code>.vscode/mcp.json</code> (or your user <code>mcp.json</code>). The input asks the AIRK
            VS Code extension for the token, so the extension must be installed; no token is written to the file.
          </p>
          <Snippet caption=".vscode/mcp.json" text={vscodeJson(url, id)} />
          <h4>Trust</h4>
          <p>
            VS Code uses the operating system's store. On Windows and macOS, install the AIRK issuer for your
            account (the certificate trust section above shows whether it is already trusted):
          </p>
          <Cmd text={trustCommand} />
          <p class="muted">
            On Linux there is no automatic per-user trust: an administrator must install the reviewed issuer in the
            system store and VS Code's NSS store. Reload the MCP server in VS Code after trust changes.
          </p>
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
              ['Transport', 'Streamable HTTP (HTTPS). Stdio and SSE are not offered.'],
              [
                'Header',
                <>
                  <code>Authorization: Bearer &lt;token&gt;</code> and optionally{' '}
                  <code>{MODE_HEADER}: terse</code>
                </>,
              ],
              [
                'Token',
                <>
                  Printed by the credential command below as <code>{'{"Authorization":"Bearer <token>"}'}</code>
                </>,
              ],
              [
                'Certificate',
                <>
                  Issued by AIRK's local CA (SHA-256 <code>{props.ca ?? 'see above'}</code>). Trust it in the OS
                  store or give the client the CA file.
                </>,
              ],
            ]}
          />
          <p>If the client can run a command to get headers, point it at:</p>
          <Cmd text={helper} />
          <div class="au-warning" role="note">
            <strong>Keep the token out of shared files.</strong> Do not paste the raw token into project files,
            repositories, chat or screenshots. Prefer a header helper, or set the token in the environment of the
            one agent process. Revoke the connection below if a token was exposed.
          </div>
        </>
      );
      break;
  }

  return (
    <Panel title="Configure an agent by hand">
      <p>
        These snippets use your live endpoint and the entry shapes <code>airkctl agents connect</code> writes.
        With a full airkctl path below, <code>airkctl agents list</code> and <code>repair</code> recognize them.
        You still need a connection id: create one with the automatic steps above, or enter an existing one.
      </p>
      <div class="row">
        <Field
          label="Connection id (32 hex, optional)"
          wide
          value={props.id}
          placeholder={ID_PLACEHOLDER}
          invalid={props.id !== '' && !idValid}
          onInput={(value) => props.setId(value.trim().toLowerCase())}
        />
        <Field
          label="airkctl path used by the helper"
          wide
          value={airkctl}
          onInput={(value) => {
            setAirkctl(value);
            airkctlTouched.current = true;
          }}
        />
      </div>
      {!helperPathOk(airkctl) ? (
        <p class="muted">
          Agents often run helpers without your shell's PATH. Use the full path to airkctl; find it with{' '}
          <code>{windows ? 'where.exe airkctl' : 'command -v airkctl'}</code>.
        </p>
      ) : null}
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
        <h4>Get the token by hand</h4>
        <p class="muted">
          Prints <code>{'{"Authorization":"Bearer <token>"}'}</code> for an existing, unrevoked connection. It works
          offline, reads only your account's private credential file and never logs the token.
        </p>
        <Cmd text={helper} />
        <h4>Check it</h4>
        <p class="muted">
          Restart or reload the agent's MCP connection, then confirm it lists AIRK's tools. Verifying below proves
          the endpoint and credential work; it does not prove a particular agent loaded its configuration.
        </p>
      </div>
    </Panel>
  );
}

// ---------------------------------------------------------------------------
// (4) Existing connections.

const OWNERSHIP_TEXT: Record<string, [string, 'ok' | 'warn' | 'bad']> = {
  owned: ['This installation owns the connection and it is active.', 'ok'],
  not_owned: ['This installation does not own a connection with this id.', 'warn'],
  unavailable: ['This installation owns the id, but the connection cannot be used now.', 'bad'],
};

function InfoView(props: { state: CallState }): JSX.Element | null {
  const r = okResult(props.state);
  if (str(r?.status) !== 'agent_connection_info') return null;
  const c = obj(r?.connection);
  const endpoint = obj(c?.endpoint);
  const selection = obj(c?.selection);
  return (
    <Facts
      rows={[
        ['Endpoint', <code>{visible(str(endpoint?.url) ?? 'not reported')}</code>],
        ['Search scope', <Text value={str(selection?.kind)?.replace(/_/g, ' ') ?? 'not reported'} />],
        ['CA certificate file', <code>{visible(str(c?.ca_certificate_file) ?? 'not reported')}</code>],
        [
          'Credential file',
          <>
            <code>{visible(str(c?.credential_file) ?? 'not reported')}</code>{' '}
            <span class="muted">(private to your account; its contents are never shown)</span>
          </>,
        ],
      ]}
    />
  );
}

function OwnershipView(props: { state: CallState }): JSX.Element | null {
  const r = okResult(props.state);
  if (str(r?.status) !== 'agent_connection_ownership') return null;
  const result = obj(r?.result);
  const status = str(result?.status);
  const words = status !== undefined ? OWNERSHIP_TEXT[status] : undefined;
  const code = str(result?.code);
  const guidance = str(result?.guidance);
  const nativeError = str(result?.native_error);
  return (
    <div class="au-result">
      <p>
        {words !== undefined ? <Badge tone={words[1]}>{status}</Badge> : <Text value={status ?? 'unknown'} />}{' '}
        {words?.[0] ?? ''}
        {code !== undefined ? (
          <>
            {' '}
            <code>
              <Text value={code} />
            </code>
          </>
        ) : null}
      </p>
      {guidance !== undefined ? (
        <p class="muted">
          <Text value={guidance} />
        </p>
      ) : null}
      {nativeError !== undefined ? (
        <p class="au-detail">
          <Text value={nativeError} />
        </p>
      ) : null}
    </div>
  );
}

function VerifyView(props: { state: CallState }): JSX.Element | null {
  const r = okResult(props.state);
  if (str(r?.status) !== 'agent_connection_verified') return null;
  const result = obj(r?.result);
  const verified = result?.verified === true;
  const server = obj(result?.server);
  const detail = result?.detail;
  const detailText = detail === undefined ? undefined : typeof detail === 'string' ? detail : JSON.stringify(detail);
  const serverText =
    server !== undefined ? [str(server.name), str(server.version)].filter((part) => part !== undefined).join(' ') : '';
  return (
    <div class="au-result">
      <p>
        {verified ? <Badge tone="ok">Verified</Badge> : <Badge tone="bad">Not verified</Badge>}{' '}
        {verified
          ? 'The service answered an MCP initialize request over HTTPS with this connection’s credential.'
          : 'The MCP probe with this connection’s credential did not succeed.'}
      </p>
      <Facts
        rows={[
          ['URL', <code>{visible(str(result?.url) ?? 'not reported')}</code>],
          ['HTTP status', <Text value={result?.http_status === undefined ? 'no answer' : String(result.http_status)} />],
          ['Protocol', <Text value={str(result?.protocol_version) ?? 'not reported'} />],
          ['Server', <Text value={serverText === '' ? 'not reported' : serverText} />],
        ]}
      />
      {detailText !== undefined ? (
        <p class="au-detail">
          <Text value={detailText} />
        </p>
      ) : null}
    </div>
  );
}

function ConnectionCard(props: {
  id: string;
  setId: (id: string) => void;
  info: CallState;
  runInfo: () => void;
}): JSX.Element {
  const [ownership, runOwnership] = useCall();
  const [verify, runVerify] = useCall();
  const [revoke, runRevoke] = useCall();
  const valid = isIncarnation(props.id);
  const revoked = okResult(revoke);
  return (
    <>
      <Panel title="Look up an existing connection">
        <p class="muted">
          Find connection ids with <code>airkctl agents list</code>, or in an agent's configuration file (the id
          after <code>--connection-id</code> or in <code>airkToken_…</code>).
        </p>
        <div class="row">
          <Field
            label="Connection id (32 hex)"
            wide
            value={props.id}
            placeholder={ID_PLACEHOLDER}
            invalid={props.id !== '' && !valid}
            onInput={(value) => props.setId(value.trim().toLowerCase())}
          />
          <button type="button" disabled={!valid} onClick={props.runInfo}>
            Details
          </button>
          <button
            type="button"
            class="secondary"
            disabled={!valid}
            onClick={() => void runOwnership(OPS.agentsOwnership, body.agentsOwnership(props.id))}
          >
            Ownership
          </button>
          <button
            type="button"
            class="secondary"
            disabled={!valid}
            onClick={() => void runVerify(OPS.agentsVerify, body.agentsVerify(props.id))}
          >
            Verify
          </button>
        </div>
        {props.info.phase !== 'idle' ? (
          <section class="au-sub" aria-label="Details">
            <h3>Details</h3>
            <InfoView state={props.info} />
            <Problem state={props.info} />
            <Complete state={props.info} />
          </section>
        ) : null}
        {ownership.phase !== 'idle' ? (
          <section class="au-sub" aria-label="Ownership">
            <h3>Ownership</h3>
            <OwnershipView state={ownership} />
            <Problem state={ownership} />
            <Complete state={ownership} />
          </section>
        ) : null}
        {verify.phase !== 'idle' ? (
          <section class="au-sub" aria-label="Verification">
            <h3>Verification</h3>
            <VerifyView state={verify} />
            <Problem state={verify} />
            <Complete state={verify} />
          </section>
        ) : null}
      </Panel>
      <Panel title="Revoke a connection">
        <p>
          Revoking ends this connection's credential at once; every agent using it loses access. Agent
          configuration files are not changed; remove their entries with{' '}
          <code>airkctl agents remove --connection-id {valid ? props.id : ID_PLACEHOLDER}</code>.
        </p>
        {valid ? (
          <Confirm
            phrase={`revoke ${props.id}`}
            action="Revoke connection"
            danger
            onConfirm={() => void runRevoke(OPS.agentsRevoke, body.agentsRevoke(props.id))}
          />
        ) : (
          <p class="muted">Enter a connection id above first.</p>
        )}
        {str(revoked?.status) === 'http_revoked' ? (
          <p>
            <Badge tone="ok">Revoked</Badge> Connection <code>{visible(str(revoked?.connection_id) ?? '')}</code> no
            longer has access.
          </p>
        ) : null}
        <Problem state={revoke} />
        <Complete state={revoke} />
      </Panel>
    </>
  );
}

// ---------------------------------------------------------------------------

export function Agents(): JSX.Element {
  const [endpoint, runEndpoint] = useCall();
  const [trust, runTrust] = useCall();
  const [info, runInfo] = useCall();
  const [id, setId] = useState('');
  const reload = (): void => {
    void runEndpoint(OPS.agentsEndpoint, body.agentsEndpoint());
    void runTrust(OPS.trustStatus, body.trustStatus());
  };
  useEffect(reload, []);
  const e = endpointOf(endpoint);
  const t = trustOf(trust);
  const infoResult = okResult(info);
  const connection = str(infoResult?.status) === 'agent_connection_info' ? obj(infoResult?.connection) : undefined;
  // Only use the looked-up CA file for snippets while it still names the entered id.
  const caFile = connection !== undefined && str(connection.connection_id) === id ? str(connection.ca_certificate_file) : undefined;
  return (
    <>
      <EndpointCard endpoint={endpoint} trust={trust} onReload={reload} />
      <AutomaticCard id={id} />
      <ManualCard
        url={e?.url}
        ca={e?.ca_sha256 ?? t?.ca_sha256}
        caFile={caFile}
        platform={t?.platform}
        id={id}
        setId={setId}
      />
      <ConnectionCard
        id={id}
        setId={setId}
        info={info}
        runInfo={() => void runInfo(OPS.agentsConnection, body.agentsConnection(id))}
      />
    </>
  );
}
