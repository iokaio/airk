// SPDX-License-Identifier: Apache-2.0
// Entry point of out/explorerAssistance.js, started by the VS Code MCP provider or by
// `airk explorer-assistance --descriptor` for external agents. stdout carries MCP only.
import catalog from 'airk-explorer-help';
import { HelpIndex } from './help';
import { AssistanceServer, serve } from './server';
import { AssistanceTools } from './tools';

const index = new HelpIndex(catalog);
const tools = new AssistanceTools(index, async () => ({ attached: false, code: 'UI_NOT_ATTACHED' }));
const server = new AssistanceServer(index, tools, catalog.extensionVersion);
void serve(server, process.stdin, text => { process.stdout.write(text); }).then(() => { process.exitCode = 0; });
