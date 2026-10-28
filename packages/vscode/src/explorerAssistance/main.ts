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
const project = option('--project-binding');
if (project && !/^[a-f0-9]{64}$/.test(project)) { throw new Error('Invalid project binding'); }
const tools = new AssistanceTools(index, windowChannel(option('--assistance-dir'), option('--project-root') ?? workingDirectory(), project));
const server = new AssistanceServer(index, tools, catalog.extensionVersion);
async function oneShot(): Promise<void> {
    let input = '';
    process.stdin.setEncoding('utf8');
    for await (const chunk of process.stdin) {
        input += chunk;
        if (Buffer.byteLength(input) > 65536) { throw new Error('Request exceeds bound'); }
    }
    const request = JSON.parse(input);
    if (!request || typeof request.tool !== 'string' || Object.keys(request).some(key => !['tool', 'arguments', 'client'].includes(key))) {
        throw new Error('Invalid request');
    }
    const result = await tools.call(request.tool, request.arguments ?? {}, { name: String(request.client ?? 'AIRK agent').slice(0,64), version: catalog.extensionVersion });
    process.stdout.write(JSON.stringify(result));
}
if (process.argv.includes('--one-shot')) {
    void oneShot().catch(() => { process.stdout.write(JSON.stringify({ ok: false, code: 'INVALID_SCHEMA' })); process.exitCode = 1; });
} else void serve(server, process.stdin, text => new Promise<void>((resolve, reject) => {
    process.stdout.write(text, error => error ? reject(error) : resolve());
})).then(() => { process.stdin.destroy(); process.exitCode = 0; });
