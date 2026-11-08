// SPDX-License-Identifier: Apache-2.0
import { randomBytes } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { isJsonClient, jsonConfigPath } from './agentConfig';
import { Client, LaunchSpec, ServerEntry, clients, unifiedEntry, serializeServers } from './model';

const labels: Record<Client, string> = {
    vscode: 'VS Code / GitHub Copilot Chat', codex: 'Codex', claude: 'Claude Code', cursor: 'Cursor',
    gemini: 'Gemini CLI', antigravity: 'Google Antigravity', cline: 'Cline', roo: 'Roo Code', junie: 'JetBrains Junie',
    'copilot-cli': 'GitHub Copilot CLI', opencode: 'OpenCode', 'opencode-v2': 'OpenCode V2',
    kilo: 'Kilo Code', 'amazon-q': 'Amazon Q Developer', windsurf: 'Windsurf / Devin'
};

export function agentChoices(): { label: string; description: string; client: Client }[] {
    return clients.map(client => ({ label: labels[client], client,
        description: client === 'vscode' ? 'Uses VS Code MCP controls'
            : client === 'codex' ? 'Create project configuration or review an existing file'
                : isJsonClient(client) ? 'Install project configuration, preserving existing settings' : 'Guided setup in this agentâ€™s own configuration' }));
}

export const verificationPrompt = (spec: LaunchSpec): string =>
    `Discover AIRK tools in the available tool catalog, including deferred tools, before assessing access. Call the AIRK server's status and capabilities tools. Confirm the collection is ${spec.collection}. Then use search for a known phrase in this workspace and cite its source path and indexed revision. If a project is required, call projects and select the matching configured binding. Report actual tool-call results or the exact discovery/connection error. Tools missing from the initial visible list alone do not establish unavailability.`;

export const assistancePrompt = (): string =>
    "Use the AIRK server's status tool, then help with query 'repair' and help with the returned topic_id. Report index availability separately from Explorer attachment. UI drafts require a trusted Explorer window showing the configured project.";

export function codexConfigPath(spec: LaunchSpec): string { return path.join(spec.root, '.codex', 'config.toml'); }

/** Create only: never parse, replace or append to somebody else's TOML. */
export async function createCodexConfig(spec: LaunchSpec, entries: readonly ServerEntry[] = [unifiedEntry(spec)]): Promise<string> {
    const root = await fs.realpath(spec.root), directory = path.join(root, '.codex');
    try { await fs.mkdir(directory); }
    catch (error) { if ((error as NodeJS.ErrnoException).code !== 'EEXIST') { throw error; } }
    const info = await fs.lstat(directory);
    if (!info.isDirectory() || info.isSymbolicLink() || await fs.realpath(directory) !== directory) {
        throw new Error('The project .codex directory must be a regular directory inside this workspace. Use manual setup for redirected configuration.');
    }
    const destination = path.join(directory, 'config.toml');
    try { await fs.writeFile(destination, serializeServers('codex', entries), { encoding: 'utf8', flag: 'wx', mode: 0o600 }); }
    catch (error) {
        if ((error as NodeJS.ErrnoException).code === 'EEXIST') {
            throw new Error('Codex project configuration already exists and was preserved. Open it and merge the displayed AIRK entry, preserving existing settings and tool policies.');
        }
        throw error;
    }
    return destination;
}

const escape = (value: string): string => value.replace(/[&<>"']/g,
    character => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[character]!);
export type ConnectionAction = 'copy' | 'createCodex' | 'openCodex' | 'verify' | 'copyPrompt' | 'vscode' | 'enableCopilot' | 'installJson' | 'openJson' | 'reviewLegacy'
    | 'verifyAssistance' | 'copyAssistancePrompt' | 'updateCodex' | 'trustIssuer';
export function connectionAction(value: unknown, client: Client): ConnectionAction | undefined {
    if (!value || typeof value !== 'object') { return; }
    const action = (value as { action?: unknown }).action;
    const allowed = ['copy', 'verify', 'copyPrompt', 'verifyAssistance', 'copyAssistancePrompt', ...(client === 'codex' ? ['createCodex', 'openCodex', 'updateCodex'] : []),
        ...(client === 'vscode' ? ['vscode', 'enableCopilot', 'trustIssuer'] : []), ...(isJsonClient(client) ? ['installJson', 'openJson', 'reviewLegacy'] : [])];
    return typeof action === 'string' && allowed.includes(action) ? action as ConnectionAction : undefined;
}

/** Persistent instructions; only the client can establish its live connection. */
export function renderConnectionGuide(client: Client, spec: LaunchSpec, instructions: string, status = 'Configuration has not been installed by this guide.', remote = false, busy = false,
    entries: readonly ServerEntry[] = [unifiedEntry(spec)]): string {
    const nonce = randomBytes(24).toString('hex');
    let configuration:string,unsupported=false;try{configuration=serializeServers(client,entries);}catch(error){unsupported=true;configuration=error instanceof Error?error.message:String(error);}
    const button = (action: ConnectionAction, label: string): string => `<button data-action="${action}"${busy||unsupported&&['copy','createCodex','updateCodex','installJson','reviewLegacy'].includes(action) ? ' disabled' : ''}>${label}</button>`;
    const codex = client === 'codex';
    const assistance = entries.some(entry => entry.http||entry.args.includes('--explorer-descriptor'));
    if(spec.http)instructions=unsupported?'This client requires HTTP configuration and trust compatibility verification. Existing configuration is preserved.':client==='vscode'?'Use the registered HTTPS connection in VS Code MCP controls. A project file requires the protected credential input command provided by the AIRK extension; it works without changing the host environment or reloading its UI. Complete trust enrollment in the actual host before connecting.':`Save the HTTPS connection for ${labels[client]}, then start it from a new terminal created by this VS Code window. Keep the existing tool approval settings.`;
    const vscodeTrust=spec.http&&client==='vscode'?(process.platform==='linux'
        ?`<p>Linux VS Code MCP prerequisite: automatic per-user OS trust installation is unavailable. An administrator must install this exact public issuer in the distribution's system trust store and the VS Code user's NSS trust store. Follow the official VS Code network certificate instructions, then reconnect the actual MCP host. Internal AIRK HTTPS checks do not verify that host. Certificate SHA256: <code>${escape(spec.http.endpoint.ca_sha256)}</code>; public CA: <code>${escape(spec.http.ca_certificate_file)}</code>. Keep TLS verification enabled.</p>`
        :`<p>Review and install this native public issuer for the actual VS Code host, then retry its MCP connection. A running host can cache trust; report its actual result instead of assuming that a saved setting refreshed it. ${button('trustIssuer','Review local HTTPS issuer trust')}</p>`):'';
    return `<!doctype html><html lang="en"><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width, initial-scale=1">
    <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'nonce-${nonce}'; script-src 'nonce-${nonce}';">
    <title>Connect ${escape(labels[client])} to AIRK</title><style nonce="${nonce}">
    *{box-sizing:border-box}body{margin:0;font-family:var(--vscode-font-family);color:var(--vscode-foreground);background:var(--vscode-editor-background);padding:24px;max-width:900px;line-height:1.5}
    pre{white-space:pre-wrap;overflow-wrap:anywhere;padding:12px;background:var(--vscode-textCodeBlock-background)}
    button{font:inherit;margin:4px 8px 4px 0;padding:7px 12px;border:0;color:var(--vscode-button-foreground);background:var(--vscode-button-background);cursor:pointer}
    button:focus-visible{outline:2px solid var(--vscode-focusBorder);outline-offset:2px}button:disabled{opacity:.5;cursor:wait}
    .status{border-left:3px solid var(--vscode-focusBorder);padding:12px}h2{margin-top:28px;font-size:1.2em}dd{overflow-wrap:anywhere}
    </style></head><body><h1>Connect ${escape(labels[client])} to AIRK</h1>
    <dl><dt>Workspace</dt><dd>${escape(spec.root)}</dd><dt>Collection</dt><dd>${escape(spec.collection)}</dd><dt>Index storage</dt><dd>${escape(spec.dataDir)}</dd></dl>
    <p class="status" role="status" aria-live="polite">${escape(status)}</p>
    ${spec.http?`<p>Connect directly to <code>${escape(spec.http.endpoint.url)}</code>. The selected native grant controls access. Trust enrollment is required in the process running the agent; certificate verification stays enabled.</p>${client==='codex'||client==='claude'?'<p>Start the agent in a new terminal created by this VS Code window to receive its prepared credential and additive CA references. Existing terminals, other applications and remote hosts are not updated. The agent must still load and verify this connection.</p>':''}${unsupported?'<p>This clientâ€™s HTTP trust or configuration format has not been qualified. No configuration will be written. Use a supported client or complete compatibility verification first.</p>':''}`:''}
    ${vscodeTrust}<h2>Connections</h2><ul>${entries.map(entry => `<li><strong>${escape(entry.label)}</strong> â€” server <code>${escape(entry.key)}</code></li>`).join('')}</ul>
    <p>One AIRK connection provides indexed evidence, help and reviewed Explorer drafts. Search and file tools read the configured index. Draft tools submit suggestions for your review in an attached Explorer window. Index availability and editor attachment are reported separately.</p>
    ${remote ? '<p>This is a remote extension host. These executable and index paths belong to that host. Configure only an agent running there; local-agent connectivity is not established by this guide.</p>' : ''}
    <h2>1. Configure ${escape(labels[client])}</h2><p>${escape(instructions)}</p>
    ${client === 'vscode' ? `${button('enableCopilot', 'Enable AIRK for Copilot')}${button('vscode', 'Open VS Code / Copilot MCP controls')}<p>Enable registers AIRK for open workspace collections and opens VS Codeâ€™s MCP controls. Choose this collection and complete any start or trust prompts. This controls Copilot Chat; Codex and other agents use their own settings.</p>` : ''}
    ${codex ? `<p>Project file: <code>${escape(codexConfigPath(spec))}</code></p><p>Create a missing file, or update an existing AIRK entry to this project's current index. Other servers and Codex settings are preserved. AIRK checks the saved connection afterward; Codex must load it in its own session.</p>
    ${remote ? '' : button('createCodex', 'Create Codex project configuration')}${!remote && spec.service ? button('updateCodex', 'Update existing AIRK connection') : ''}${button('openCodex', 'Open existing Codex project configuration')}` : ''}
    ${isJsonClient(client) ? `<p>Project file: <code>${escape(jsonConfigPath(client, spec))}</code></p><p>Install writes one AIRK connection and consolidates recognized earlier AIRK entries. Existing servers, comments and approval policies are preserved; conflicting entries require review. Check user-level settings for duplicate AIRK entries too.</p>${remote ? '' : button('installJson', `Install for ${escape(labels[client])}`)}${button('openJson', 'Open project configuration')}` : ''}
    ${spec.service ? `<p>Startup updates recognized project configurations to one airk server. Custom entries require review; check user-level configuration for duplicates. Restart the coding agent after migration and approve renamed tools in that client.</p>${isJsonClient(client) && !remote ? button('reviewLegacy', 'Review legacy AIRK entry') : '<p>Use the agentâ€™s configuration editor to review and merge the displayed connection. Custom TOML syntax and user-level configurations require manual review.</p>'}` : ''}
    <details${client === 'vscode' ? '' : ' open'}><summary>${client === 'vscode' ? 'Manual configuration fallback (avoid duplicate registration)' : 'Configuration to review'}</summary><pre>${escape(configuration)}</pre>${button('copy', 'Copy configuration')}</details>
    <h2>2. ${spec.http?'Connect and enable the server':'Reload and enable the server'}</h2><p>${spec.service ? 'If this collection has not been indexed, start native indexing from AIRK Explorer â€º Management â€º Projects and check its readiness before querying your sources.' : 'If this collection has not been indexed, run the AIRK helper check below before restarting your agent.'}</p><p>${codex ? 'In Codex, open the gear menu â†’ MCP servers, then restart the extension. For Codex CLI, start a new session and check /mcp. Project configuration requires a trusted project.' : 'Use the selected agentâ€™s MCP controls to reload and enable AIRK. Complete that clientâ€™s normal trust and tool-approval prompts.'}</p>
    <p>${spec.http?'The service publishes its actual endpoint. A changed endpoint or host environment can require the agentâ€™s MCP connection to reconnect; saving a file alone does not establish that it did.':spec.service ? 'Keep the stable launcher and exact project binding together. Review configuration again after changing the registered source or storage binding.' : 'Keep the executable, root, storage and collection arguments together. Refresh this configuration after a helper upgrade or storage change.'}</p>
    <h2>3. Check AIRK and the agent separately</h2>${button('verify', spec.service ? 'Check AIRK MCP connection' : 'Check AIRK helper')}
    <p>${spec.service ? 'The connection check reads readiness and tests the MCP handshake without starting indexing. Start indexing from AIRK Explorer â€º Management â€º Projects when needed.' : 'The helper check updates this collectionâ€™s index and tests its MCP handshake.'} It cannot confirm that ${escape(labels[client])} loaded the configuration.</p>
    <p>Send this prompt in ${escape(labels[client])} to check its actual tools:</p><pre>${escape(verificationPrompt(spec))}</pre>${button('copyPrompt', 'Copy verification prompt')}
    ${assistance ? `<p>Help and Explorer drafts use this same server. To check attachment and help:</p><pre>${escape(assistancePrompt())}</pre>${button('copyAssistancePrompt', 'Copy help and UI verification prompt')}` : ''}
    <p>If tools are missing, check that agentâ€™s active configuration, server list and logs. A running AIRK server in VS Code does not configure another agent.</p>
    <script nonce="${nonce}">const vscode=acquireVsCodeApi();document.addEventListener('click',event=>{const button=event.target.closest('button');if(!button)return;document.querySelectorAll('button').forEach(item=>item.disabled=true);vscode.postMessage({action:button.dataset.action});});</script></body></html>`;
}
