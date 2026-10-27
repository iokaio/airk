// SPDX-License-Identifier: Apache-2.0
// Minimal MCP stdio server: newline-delimited JSON-RPC 2.0, tools and help resources.
import { ASSISTANCE_HELP_URI, ASSISTANCE_SERVER_NAME } from '../explorerContracts';
import { EXPLORER_ERRORS } from '../explorerErrors';
import type { HelpIndex } from './help';
import { type AssistanceTools, toolDefinitions } from './tools';

/** Newest first; an unknown requested version is answered with the newest. */
export const PROTOCOL_VERSIONS = ['2025-11-25', '2025-06-18', '2025-03-26', '2024-11-05'] as const;
export const MAX_LINE_BYTES = 1_048_576;
const INSTRUCTIONS = 'AIRK Explorer Assistance: how-to help for the AIRK Explorer in VS Code, from a catalog bundled with this extension version, '
    + 'and inert drafts (search, strategy, plan) for the user to review in the Explorer Suggestions Inbox. '
    + 'It does not search code or documents; use AIRK Retrieval (airk_* tools) for evidence. It never runs commands, changes settings or writes files.';

type Id = string | number;
interface Message { jsonrpc?: unknown; id?: unknown; method?: unknown; params?: unknown }
const reply = (id: Id, result: unknown) => ({ jsonrpc: '2.0', id, result });
const failure = (id: Id | null, code: number, message: string) => ({ jsonrpc: '2.0', id, error: { code, message } });

export class AssistanceServer {
    private initialized = false;
    /** Self-reported by the agent in initialize; shown in the Inbox, never trusted. */
    private client = { name: 'unknown client', version: '' };

    constructor(private readonly index: HelpIndex, private readonly tools: AssistanceTools, private readonly version: string) {}

    /** Handle one decoded line; returns the response, or undefined for notifications. */
    async handle(line: string): Promise<object | undefined> {
        let message: Message;
        try { message = JSON.parse(line); } catch { return failure(null, -32700, 'Parse error'); }
        if (!message || typeof message !== 'object' || Array.isArray(message) || message.jsonrpc !== '2.0' || typeof message.method !== 'string') {
            return failure(null, -32600, 'Invalid request');
        }
        const id = message.id;
        if (id === undefined) { return undefined; }
        if (typeof id !== 'string' && typeof id !== 'number') { return failure(null, -32600, 'Invalid request id'); }
        const params = (message.params ?? {}) as Record<string, unknown>;
        if (typeof params !== 'object' || Array.isArray(params)) { return failure(id, -32602, 'Invalid params'); }
        if (message.method === 'ping') { return reply(id, {}); }
        if (message.method === 'initialize') {
            this.initialized = true;
            const info = params.clientInfo as Record<string, unknown> | undefined;
            if (info && typeof info.name === 'string') { this.client = { name: info.name.slice(0, 64), version: typeof info.version === 'string' ? info.version.slice(0, 32) : '' }; }
            const requested = params.protocolVersion;
            const protocolVersion = PROTOCOL_VERSIONS.find(version => version === requested) ?? PROTOCOL_VERSIONS[0];
            return reply(id, { protocolVersion, capabilities: { tools: { listChanged: false }, resources: { listChanged: false } },
                serverInfo: { name: ASSISTANCE_SERVER_NAME, title: 'AIRK Explorer Assistance', version: this.version }, instructions: INSTRUCTIONS });
        }
        if (!this.initialized) { return failure(id, -32002, 'Server not initialized'); }
        switch (message.method) {
        case 'tools/list': return reply(id, { tools: toolDefinitions(this.index) });
        case 'tools/call': {
            if (typeof params.name !== 'string') { return failure(id, -32602, 'Tool name required'); }
            const result = await this.tools.call(params.name, params.arguments, this.client);
            const structured = result.ok ? result.value : { error: { code: result.code, message: EXPLORER_ERRORS[result.code].message, ...(result.detail ? { detail: result.detail } : {}) } };
            return reply(id, { content: [{ type: 'text', text: JSON.stringify(structured) }], structuredContent: structured, isError: !result.ok });
        }
        case 'resources/list': return reply(id, { resources: this.index.catalog.topics.map(topic => ({
            uri: ASSISTANCE_HELP_URI + topic.id, name: topic.id, title: topic.title, mimeType: 'text/markdown',
            description: `${topic.applicability === 'implemented' ? '' : `[${topic.applicability}] `}${topic.title}` })) });
        case 'resources/templates/list': return reply(id, { resourceTemplates: [] });
        case 'resources/read': {
            const uri = typeof params.uri === 'string' ? params.uri : '';
            const topic = uri.startsWith(ASSISTANCE_HELP_URI) ? this.index.topic(uri.slice(ASSISTANCE_HELP_URI.length)) : undefined;
            if (!topic) { return failure(id, -32002, 'Resource not found'); }
            return reply(id, { contents: [{ uri, mimeType: 'text/markdown', text: topic.body }] });
        }
        default: return failure(id, -32601, 'Method not found');
        }
    }
}

/** Serve newline-delimited messages in order. Oversized lines are refused without buffering them. */
export function serve(server: AssistanceServer, input: NodeJS.ReadableStream, write: (text: string) => void): Promise<void> {
    return new Promise(resolve => {
        let pending = '', discarding = false, queue = Promise.resolve();
        // Responses keep request order, including refusals of oversized input.
        const dispatch = (respond: () => Promise<object | undefined>) => {
            queue = queue.then(async () => {
                const response = await respond().catch(() => failure(null, -32603, 'Internal error'));
                if (response) { write(JSON.stringify(response) + '\n'); }
            });
        };
        const tooLarge = () => dispatch(async () => failure(null, -32600, 'Message too large'));
        input.setEncoding('utf8');
        input.on('data', (chunk: string) => {
            pending += chunk;
            let newline: number;
            while ((newline = pending.indexOf('\n')) >= 0) {
                const line = pending.slice(0, newline).replace(/\r$/, '');
                pending = pending.slice(newline + 1);
                if (discarding) { discarding = false; continue; }
                if (Buffer.byteLength(line, 'utf8') > MAX_LINE_BYTES) { tooLarge(); } else if (line.trim()) { dispatch(() => server.handle(line)); }
            }
            if (Buffer.byteLength(pending, 'utf8') > MAX_LINE_BYTES) {
                pending = '';
                if (!discarding) { discarding = true; tooLarge(); }
            }
        });
        input.on('end', () => { void queue.then(resolve); });
    });
}
