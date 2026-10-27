// SPDX-License-Identifier: Apache-2.0
// Entry point of out/explorerAssistance.js, started by the VS Code MCP provider or by
// `airk explorer-assistance --descriptor` for external agents. stdout carries MCP only.
// Arguments: [--assistance-dir <dir>], the directory holding per-window descriptors.
import catalog from 'airk-explorer-help';
import { realpathSync } from 'node:fs';
import { windowChannel } from './attach';
import { HelpIndex } from './help';
import { AssistanceServer, serve } from './server';
import { AssistanceTools } from './tools';

function option(name: string): string | undefined {
    const at = process.argv.indexOf(name, 2);
    return at > 0 ? process.argv[at + 1] : undefined;
}
function workingDirectory(): string {
    try { return realpathSync.native(process.cwd()); } catch { return process.cwd(); }
}

const index = new HelpIndex(catalog);
const tools = new AssistanceTools(index, windowChannel(option('--assistance-dir'), workingDirectory()));
const server = new AssistanceServer(index, tools, catalog.extensionVersion);
void serve(server, process.stdin, text => { process.stdout.write(text); }).then(() => { process.exitCode = 0; });
