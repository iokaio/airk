// SPDX-License-Identifier: Apache-2.0
import { randomBytes } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';
import { isJsonClient, jsonConfigPath } from './agentConfig';
import { Client, LaunchSpec, clients, configFor } from './model';

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
                : isJsonClient(client) ? 'Install project configuration, preserving existing settings' : 'Guided setup in this agent’s own configuration' }));
}

export const verificationPrompt = (spec: LaunchSpec): string =>
    `Use AIRK's airk_status and airk_capabilities tools. Confirm the collection is ${spec.collection}. Then use airk_search for a known phrase in this workspace and cite its source path. If AIRK tools are unavailable, say so explicitly.`;

export function codexConfigPath(spec: LaunchSpec): string { return path.join(spec.root, '.codex', 'config.toml'); }

/** Create only: never parse, replace or append to somebody else's TOML. */
export async function createCodexConfig(spec: LaunchSpec): Promise<string> {
    const root = await fs.realpath(spec.root), directory = path.join(root, '.codex');
    try { await fs.mkdir(directory); }
    catch (error) { if ((error as NodeJS.ErrnoException).code !== 'EEXIST') { throw error; } }
    const info = await fs.lstat(directory);
    if (!info.isDirectory() || info.isSymbolicLink() || await fs.realpath(directory) !== directory) {
        throw new Error('The project .codex directory must be a regular directory inside this workspace. Use manual setup for redirected configuration.');
    }
    const destination = path.join(directory, 'config.toml');
    try { await fs.writeFile(destination, configFor('codex', spec), { encoding: 'utf8', flag: 'wx', mode: 0o600 }); }
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
export type ConnectionAction = 'copy' | 'createCodex' | 'openCodex' | 'verify' | 'copyPrompt' | 'vscode' | 'enableCopilot' | 'installJson' | 'openJson' | 'reviewLegacy';
export function connectionAction(value: unknown, client: Client): ConnectionAction | undefined {
    if (!value || typeof value !== 'object') { return; }
    const action = (value as { action?: unknown }).action;
    const allowed = ['copy', 'verify', 'copyPrompt', ...(client === 'codex' ? ['createCodex', 'openCodex'] : []),
        ...(client === 'vscode' ? ['vscode', 'enableCopilot'] : []), ...(isJsonClient(client) ? ['installJson', 'openJson', 'reviewLegacy'] : [])];
    return typeof action === 'string' && allowed.includes(action) ? action as ConnectionAction : undefined;
}

/** Persistent instructions; only the client can establish its live connection. */
export function renderConnectionGuide(client: Client, spec: LaunchSpec, instructions: string, status = 'Configuration has not been installed by this guide.', remote = false, busy = false): string {
    const nonce = randomBytes(24).toString('hex');
    const button = (action: ConnectionAction, label: string): string => `<button data-action="${action}"${busy ? ' disabled' : ''}>${label}</button>`;
    const codex = client === 'codex';
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
    ${remote ? '<p>This is a remote extension host. These executable and index paths belong to that host. Configure only an agent running there; local-agent connectivity is not established by this guide.</p>' : ''}
    <h2>1. Configure ${escape(labels[client])}</h2><p>${escape(instructions)}</p>
    ${client === 'vscode' ? `${button('enableCopilot', 'Enable AIRK for Copilot')}${button('vscode', 'Open VS Code / Copilot MCP controls')}<p>Enable registers AIRK for open workspace collections and opens VS Code’s MCP controls. Choose this collection and complete any start or trust prompts. This controls Copilot Chat; Codex and other agents use their own settings.</p>` : ''}
    ${codex ? `<p>Project file: <code>${escape(codexConfigPath(spec))}</code></p><p>Create this file if it is missing, or open it and merge the entry below if it already exists. Existing files are never overwritten. Review any same-named server in your user configuration too.</p>
    ${remote ? '' : button('createCodex', 'Create Codex project configuration')}${button('openCodex', 'Open existing Codex project configuration')}` : ''}
    ${isJsonClient(client) ? `<p>Project file: <code>${escape(jsonConfigPath(client, spec))}</code></p><p>Install adds only this AIRK server. Existing servers, comments and approval policies are preserved; conflicting entries require review. Check user-level settings for duplicate AIRK entries too.</p>${remote ? '' : button('installJson', `Install for ${escape(labels[client])}`)}${button('openJson', 'Open project configuration')}` : ''}
    ${spec.service ? `<p>Review old AIRK entries before enabling 1.4. Replace only the selected server after reviewing its command and arguments; preserve other servers and approval settings. Check project and user configuration for duplicate entries. Old jobs, indexes and processes are separate from this configuration review.</p>${isJsonClient(client) && !remote ? button('reviewLegacy', 'Review legacy AIRK entry') : '<p>Use the agent’s configuration editor to review and merge the displayed connection. Existing TOML and other agent formats require manual review.</p>'}` : ''}
    <details${client === 'vscode' ? '' : ' open'}><summary>${client === 'vscode' ? 'Manual configuration fallback (avoid duplicate registration)' : 'Configuration to review'}</summary><pre>${escape(configFor(client, spec))}</pre>${button('copy', 'Copy configuration')}</details>
    <h2>2. Reload and enable the server</h2><p>${spec.service ? 'If this collection has not been indexed, start native indexing in the Service Console and inspect its readiness before querying your sources.' : 'If this collection has not been indexed, run the AIRK helper check below before restarting your agent.'}</p><p>${codex ? 'In Codex, open the gear menu → MCP servers, then restart the extension. For Codex CLI, start a new session and check /mcp. Project configuration requires a trusted project.' : 'Use the selected agent’s MCP controls to reload and enable AIRK. Complete that client’s normal trust and tool-approval prompts.'}</p>
    <p>${spec.service ? 'Keep the stable launcher and exact project binding together. Review configuration again after changing the registered source or storage binding.' : 'Keep the executable, root, storage and collection arguments together. Refresh this configuration after a helper upgrade or storage change.'}</p>
    <h2>3. Check AIRK and the agent separately</h2>${button('verify', spec.service ? 'Check AIRK MCP connection' : 'Check AIRK helper')}
    <p>${spec.service ? 'The connection check reads readiness and tests the MCP handshake without starting indexing. Start indexing in the Service Console when needed.' : 'The helper check updates this collection’s index and tests its MCP handshake.'} It cannot confirm that ${escape(labels[client])} loaded the configuration.</p>
    <p>Send this prompt in ${escape(labels[client])} to check its actual tools:</p><pre>${escape(verificationPrompt(spec))}</pre>${button('copyPrompt', 'Copy verification prompt')}
    <p>If tools are missing, check that agent’s active configuration, server list and logs. A running AIRK server in VS Code does not configure another agent.</p>
    <script nonce="${nonce}">const vscode=acquireVsCodeApi();document.addEventListener('click',event=>{const button=event.target.closest('button');if(!button)return;document.querySelectorAll('button').forEach(item=>item.disabled=true);vscode.postMessage({action:button.dataset.action});});</script></body></html>`;
}
