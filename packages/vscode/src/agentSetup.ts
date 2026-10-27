// SPDX-License-Identifier: Apache-2.0
import * as vscode from 'vscode';
import * as path from 'node:path';
import { Client, LaunchSpec, McpVerification, ServerEntry, serializeServers } from './model';
import { installJsonConfig, isJsonClient, jsonConfigPath, readJsonConfig, reviewLegacyJsonConfig, replaceReviewedLegacyJsonConfig } from './agentConfig';
import { agentChoices, assistancePrompt, codexConfigPath, connectionAction, createCodexConfig, renderConnectionGuide, verificationPrompt } from './agentConnections';

export interface AgentSetupHost {
    trusted(): void;
    chooseSpec(): Promise<LaunchSpec | undefined>;
    registrationEnabled(): boolean;
    registrationChanged(): void;
    manageVscodeMcp(): Promise<void>;
    verify(spec: LaunchSpec): Promise<void>;
    verification(spec: LaunchSpec): McpVerification | undefined;
    isCurrent(spec: LaunchSpec): boolean;
    rootBindings(): ReadonlyArray<{ opened: string; canonical: string }>;
    subscribe(panel: vscode.WebviewPanel): void;
    /** Retrieval, plus Explorer Assistance once the workspace has a native service binding. */
    connectionEntries(spec: LaunchSpec): ServerEntry[];
    /** Start Explorer Assistance through the agent entry and return its tool names. */
    verifyAssistance(spec: LaunchSpec): Promise<string[]>;
}

const locations: Record<Client, string> = {
    vscode: 'VS Code: use MCP: List Servers to enable the registered AIRK collection, or merge into .vscode/mcp.json.',
    claude: 'Install project .mcp.json, then restart Claude Code and review the server in /mcp.',
    codex: 'Codex: merge the TOML table into .codex/config.toml for a trusted project or your user config.toml, then check its MCP server list.',
    cursor: 'Cursor: merge into .cursor/mcp.json and enable it in Cursor MCP settings.',
    gemini: 'Install .gemini/settings.json, then restart Gemini CLI and check /mcp. Existing allow/exclude and approval policies remain in force.',
    antigravity: 'Install .agents/mcp_config.json for this project. In Antigravity IDE, open MCP Servers → Manage MCP Servers to reload; in the CLI use /mcp reload and /mcp list.',
    cline: 'Cline: open Configure MCP Servers and merge the entry into the settings file that Cline opens.',
    roo: 'Roo Code: merge into .roo/mcp.json using Edit Project MCP.',
    junie: 'Junie: merge into .junie/mcp/mcp.json, then check /mcp.',
    'copilot-cli': 'Copilot CLI: merge into your MCP configuration, preserving existing server entries and tool policies.',
    opencode: 'OpenCode: merge into opencode.json under mcp. Verify your installed version uses this schema.',
    'opencode-v2': 'OpenCode V2: merge into the installed version\'s configuration under mcp.servers.',
    kilo: 'Kilo Code: merge into kilo.json under mcp, then check Settings → MCP.',
    'amazon-q': 'Amazon Q Developer: use Tools → Add stdio server and supply these command/args details. This is connection guidance, not a complete default.json agent configuration.',
    windsurf: 'Windsurf/Devin: identify the installed agent/version and open its MCP configuration UI. Supply these command/args details; legacy Cascade and Devin Local use different configuration schemas.'
};

export async function setupAgent(host: AgentSetupHost): Promise<void> {
        host.trusted();
        const selected = await vscode.window.showQuickPick(agentChoices(), {
            title: 'Connect a coding agent to AIRK', placeHolder: 'Which agent should use this workspace index?', matchOnDescription: true
        });
        if (!selected) { return; }
        const spec = await host.chooseSpec(); if (!spec) { return; }
        const client = selected.client;
        const entries = host.connectionEntries(spec);
        const panel = vscode.window.createWebviewPanel('airk.connection', `AIRK → ${selected.label}`, vscode.ViewColumn.Active,
            { enableScripts: true, localResourceRoots: [] });
        let status = client === 'vscode' ? `VS Code registration is ${host.registrationEnabled() ? 'on' : 'off'}. Agent connection has not been verified.`
            : 'Configuration has not been installed by this guide.';
        let busy = false, closed = false;
        const render = () => { panel.webview.html = renderConnectionGuide(client, spec, locations[client], status, !!vscode.env.remoteName, busy, entries); };
        const checkCurrent = () => {
            host.trusted();
            if (closed || !host.isCurrent(spec)) {
                throw new Error('Workspace or storage changed. Close this guide and run Connect Coding Agent again.');
            }
        };
        const configDocument = (destination: string) => {
            const samePath = (left: string, right: string) => process.platform === 'win32'
                ? path.resolve(left).toLowerCase() === path.resolve(right).toLowerCase() : path.resolve(left) === path.resolve(right);
            const matches = vscode.workspace.textDocuments.filter(document => samePath(document.uri.fsPath, destination)
                || host.rootBindings().some(binding => samePath(binding.canonical, spec.root)
                    && samePath(document.uri.fsPath, path.join(binding.opened, path.relative(spec.root, destination)))));
            if (matches.some(document => document.isDirty)) { throw new Error('Agent configuration has unsaved edits. Save and review it before installing AIRK.'); }
            return matches[0];
        };
        const saveConfiguration = async (destination: string, before: string, after: string) => {
            const document = configDocument(destination) ?? await vscode.workspace.openTextDocument(vscode.Uri.file(destination));
            const version = document.version;
            if (document.isDirty || document.getText() !== before) { throw new Error('Configuration has unsaved or external changes. Review it before installing AIRK.'); }
            const edit = new vscode.WorkspaceEdit();
            edit.replace(document.uri, new vscode.Range(document.positionAt(0), document.positionAt(before.length)), after);
            if (await readJsonConfig(destination) !== before) { throw new Error('Configuration changed on disk. Retry setup.'); }
            checkCurrent(); configDocument(destination);
            if (document.version !== version || !await vscode.workspace.applyEdit(edit)) { throw new Error('Configuration changed during setup. Review the file and retry.'); }
            checkCurrent();
            if (document.getText() !== after || !await document.save()) { throw new Error('Configuration was not saved. Review the editor’s changes and save manually.'); }
        };
        render();
        const listener = panel.webview.onDidReceiveMessage(async value => {
            const action = connectionAction(value, client);
            if (!action || busy || closed) { return; }
            busy = true;
            try {
                checkCurrent();
                if (action === 'copy' || action === 'copyPrompt' || action === 'copyAssistancePrompt') {
                    await vscode.env.clipboard.writeText(action === 'copy' ? serializeServers(client, entries) : action === 'copyPrompt' ? verificationPrompt(spec) : assistancePrompt());
                    status = action === 'copy' ? 'Configuration copied. Merge it into the selected agent’s configuration; copying does not install it.' : 'Verification prompt copied. Send it in the selected agent to check its actual connection.';
                } else if (action === 'createCodex') {
                    if (vscode.env.remoteName) { throw new Error('Use manual configuration on the host running Codex.'); }
                    const destination = codexConfigPath(spec);
                    configDocument(destination);
                    await createCodexConfig(spec, entries);
                    status = 'Codex project configuration created. Restart Codex and verify its tools below. Agent connection has not been verified.';
                } else if (action === 'installJson' && isJsonClient(client)) {
                    if (vscode.env.remoteName) { throw new Error('Use manual configuration on the host running this agent.'); }
                    const checkInstall = () => { checkCurrent(); configDocument(jsonConfigPath(client, spec)); };
                    await installJsonConfig(client, spec, checkInstall, saveConfiguration, entries);
                    status = `${selected.label} project configuration installed. Reload the agent and complete its trust prompts. Agent connection has not been verified.`;
                } else if (action === 'reviewLegacy' && isJsonClient(client)) {
                    if (vscode.env.remoteName) { throw new Error('Review the configuration on the host running this agent.'); }
                    const checkReview = () => { checkCurrent(); configDocument(jsonConfigPath(client, spec)); };
                    const review = await reviewLegacyJsonConfig(client, spec, checkReview);
                    const proposal = await vscode.workspace.openTextDocument({ content: review.after, language: 'jsonc' });
                    checkReview();
                    await vscode.commands.executeCommand('vscode.diff', vscode.Uri.file(review.destination), proposal.uri, 'AIRK: review selected legacy server replacement');
                    const approved = await vscode.window.showInformationMessage('Review the configuration diff. Replace this legacy AIRK server with the registered 1.4 connection?', 'Replace reviewed entry');
                    if (approved !== 'Replace reviewed entry') { status = 'Legacy entry preserved. Review remains explicit.'; }
                    else {
                        checkReview();
                        if (proposal.getText() !== review.after) { throw new Error('The proposed diff was edited. Review the original configuration again before replacing it.'); }
                        await replaceReviewedLegacyJsonConfig(client, spec, review, checkReview, saveConfiguration);
                        status = 'Reviewed AIRK entry replaced. Reload this agent and check for duplicates in its user configuration. Old processes and indexes were preserved.';
                    }
                } else if (action === 'openJson' && isJsonClient(client)) {
                    const document = await vscode.workspace.openTextDocument(vscode.Uri.file(jsonConfigPath(client, spec)));
                    await vscode.window.showTextDocument(document, { viewColumn: vscode.ViewColumn.Beside, preview: false });
                    status = 'Project configuration opened. Review the AIRK entry and any existing client policies before reloading the agent.';
                } else if (action === 'enableCopilot') {
                    if (typeof vscode.lm?.registerMcpServerDefinitionProvider !== 'function'
                        || !(await vscode.commands.getCommands(true)).includes('workbench.mcp.listServer')) {
                        throw new Error('This editor does not support AIRK’s MCP registration. Update VS Code or use manual MCP configuration.');
                    }
                    checkCurrent();
                    await vscode.workspace.getConfiguration('airk').update('mcp.registrationEnabled', true, vscode.ConfigurationTarget.Global);
                    host.registrationChanged();
                    checkCurrent();
                    await host.manageVscodeMcp();
                    status = 'AIRK registration enabled for open workspace collections. Select this collection in VS Code’s MCP controls and complete any start or trust prompts. Copilot’s connection has not been verified.';
                } else if (action === 'openCodex') {
                    const document = await vscode.workspace.openTextDocument(vscode.Uri.file(codexConfigPath(spec)));
                    await vscode.window.showTextDocument(document, { viewColumn: vscode.ViewColumn.Beside, preview: false });
                    status = 'Existing configuration opened. Merge the AIRK entry without duplicating its table or changing other settings, save, then restart Codex.';
                } else if (action === 'vscode') {
                    await host.manageVscodeMcp();
                    status = `VS Code MCP controls opened. Registration is ${host.registrationEnabled() ? 'on' : 'off'}; enable it in the AIRK sidebar if needed. Other agents use their own configuration.`;
                } else if (action === 'verifyAssistance') {
                    status = 'Checking Explorer Assistance…'; render();
                    const tools = await host.verifyAssistance(spec);
                    status = `Explorer Assistance verified: ${tools.length} tools (${tools.join(', ')}). Agent connection still needs the Assistance prompt below.`;
                } else if (action === 'verify') {
                    const target = spec.service ? 'AIRK MCP connection' : 'AIRK helper';
                    status = `Checking ${target}…`; render();
                    await host.verify(spec);
                    const check = host.verification(spec);
                    status = check?.phase === 'verified' ? `${target} verified: ${check.tools?.length ?? 0} tools. Agent connection still needs the verification prompt below.`
                        : `${target} check did not succeed: ${check?.error ?? 'workspace or storage changed'}`;
                }
            } catch (error) { status = error instanceof Error ? error.message : String(error); }
            finally { busy = false; if (!closed) { render(); } }
        });
        panel.onDidDispose(() => { closed = true; listener.dispose(); });
        host.subscribe(panel);
    }
