// SPDX-License-Identifier: Apache-2.0
// Explorer Assistance tool catalog. Help tools answer from the bundled catalog;
// ui_status reports whether a trusted Explorer window is attached. No tool reads
// source, calls AIRK Retrieval, runs commands or changes settings.
import { ASSISTANCE_HELP_URI, ASSISTANCE_SERVER_NAME, ASSISTANCE_TOOLS } from '../explorerContracts';
import { EXPLORER_ERRORS, type ExplorerErrorCode } from '../explorerErrors';
import { FEATURES } from '../explorerFeatures';
import { ROUTE_KEYS } from '../explorerRoutes';
import { DETAILS, type Detail, HelpIndex, location, SEARCH_BYTES, SEARCH_LIMIT, steps, summary } from './help';

/** What the extension host reports about the Explorer window this agent would reach. */
export interface Attachment {
    readonly attached: boolean;
    /** Why no window is reachable; absent when attached. */
    readonly code?: ExplorerErrorCode;
    /** Opaque, path-free binding identity for detecting stale drafts. */
    readonly bindingFingerprint?: string;
}
export type AttachmentProbe = () => Promise<Attachment>;

export type ToolResult = { readonly ok: true; readonly value: Record<string, unknown> } | { readonly ok: false; readonly code: ExplorerErrorCode; readonly detail?: string };

const TOPIC_ID = '^[a-z][a-z0-9.-]{2,63}$';
const READ_ONLY = { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false } as const;
const CLI_NOTE = 'These are AIRK Explorer steps in the VS Code extension. The native airk CLI has separate commands; ask AIRK Retrieval airk_help for those.';

export function toolDefinitions(index: HelpIndex): object[] {
    const definitions: Record<typeof ASSISTANCE_TOOLS[number], object> = {
        airk_explorer_help_search: {
            description: 'Search how-to topics for the AIRK Explorer in this VS Code extension version. Deterministic lexical ranking over a bundled, offline catalog. Returns topic ids for airk_explorer_help_get; check applicability before directing a user to a screen.',
            inputSchema: { type: 'object', additionalProperties: false, required: ['query'], properties: {
                query: { type: 'string', minLength: 1, maxLength: 256 },
                area: { type: 'string', enum: index.areas },
                limit: { type: 'integer', minimum: 1, maximum: SEARCH_LIMIT.max, default: SEARCH_LIMIT.default },
                max_bytes: { type: 'integer', minimum: SEARCH_BYTES.min, maximum: SEARCH_BYTES.max, default: SEARCH_BYTES.default },
            } },
        },
        airk_explorer_help_get: {
            description: 'Read one AIRK Explorer help topic by id: where it is in the UI, ordered steps, applicability for this installed version and related airk_help topics on AIRK Retrieval.',
            inputSchema: { type: 'object', additionalProperties: false, required: ['topic_id'], properties: {
                topic_id: { type: 'string', pattern: TOPIC_ID },
                detail: { type: 'string', enum: DETAILS, default: 'full' },
            } },
        },
        airk_explorer_ui_status: {
            description: 'Report whether a trusted AIRK Explorer window is attached, the screens this version ships and their availability, and the schema versions. Returns no paths or source content.',
            inputSchema: { type: 'object', additionalProperties: false, properties: {} },
        },
    };
    return ASSISTANCE_TOOLS.map(name => ({ name, ...definitions[name], annotations: READ_ONLY }));
}

type Field = { kind: 'string'; max: number; pattern?: RegExp; values?: readonly string[] } | { kind: 'integer'; min: number; max: number };

/** Strict argument check: unknown fields, wrong types and out-of-range values are refused, never coerced. */
function parse(args: unknown, fields: Record<string, Field>, required: readonly string[]): Record<string, string | number> | string {
    const value = args ?? {};
    if (typeof value !== 'object' || Array.isArray(value)) { return 'arguments must be an object'; }
    const result: Record<string, string | number> = {};
    for (const [key, entry] of Object.entries(value)) {
        const field = Object.hasOwn(fields, key) ? fields[key] : undefined;
        if (!field) { return `unknown field ${key.slice(0, 64)}`; }
        if (field.kind === 'string') {
            if (typeof entry !== 'string' || !entry.trim() || entry.length > field.max || (field.pattern && !field.pattern.test(entry))
                || (field.values && !field.values.includes(entry))) { return `invalid ${key}`; }
        } else if (!Number.isInteger(entry) || (entry as number) < field.min || (entry as number) > field.max) { return `invalid ${key}`; }
        result[key] = entry as string | number;
    }
    const missing = required.find(key => !(key in result));
    return missing ? `missing ${missing}` : result;
}

const bytes = (value: unknown) => Buffer.byteLength(JSON.stringify(value), 'utf8');

export class AssistanceTools {
    constructor(private readonly index: HelpIndex, private readonly attachment: AttachmentProbe) {}

    private identity() {
        return { server: ASSISTANCE_SERVER_NAME, extension_version: this.index.catalog.extensionVersion, catalog_version: this.index.catalog.catalogVersion };
    }

    async call(name: string, args: unknown): Promise<ToolResult> {
        switch (name) {
        case 'airk_explorer_help_search': return this.search(args);
        case 'airk_explorer_help_get': return this.get(args);
        case 'airk_explorer_ui_status': return this.status(args);
        default: return { ok: false, code: 'CAPABILITY_DISABLED', detail: 'Unknown Explorer Assistance tool.' };
        }
    }

    private search(args: unknown): ToolResult {
        const input = parse(args, {
            query: { kind: 'string', max: 256 }, area: { kind: 'string', max: 32, values: this.index.areas },
            limit: { kind: 'integer', min: 1, max: SEARCH_LIMIT.max }, max_bytes: { kind: 'integer', min: SEARCH_BYTES.min, max: SEARCH_BYTES.max },
        }, ['query']);
        if (typeof input === 'string') { return { ok: false, code: 'INVALID_SCHEMA', detail: input }; }
        const query = input.query as string;
        if (!/[a-z0-9]/i.test(query)) { return { ok: false, code: 'INVALID_SCHEMA', detail: 'query needs at least one letter or digit' }; }
        const limit = (input.limit as number | undefined) ?? SEARCH_LIMIT.default;
        const budget = (input.max_bytes as number | undefined) ?? SEARCH_BYTES.default;
        const matches = this.index.search(query, input.area as string | undefined);
        const value = { ...this.identity(), query, ...(input.area ? { area: input.area } : {}), results: [] as object[], total_matches: matches.length, truncated: matches.length > limit };
        for (const { topic } of matches.slice(0, limit)) {
            const entry = { topic_id: topic.id, title: topic.title, area: topic.area, applicability: topic.applicability,
                ...(topic.route ? { route: topic.route, location: location(topic.route) } : {}), snippet: summary(topic.body).slice(0, 240) };
            if (bytes({ ...value, results: [...value.results, entry], truncated: true }) > budget) { value.truncated = true; break; }
            value.results.push(entry);
        }
        return { ok: true, value };
    }

    private get(args: unknown): ToolResult {
        const input = parse(args, { topic_id: { kind: 'string', max: 64, pattern: new RegExp(TOPIC_ID) }, detail: { kind: 'string', max: 16, values: DETAILS } }, ['topic_id']);
        if (typeof input === 'string') { return { ok: false, code: 'INVALID_SCHEMA', detail: input }; }
        const topic = this.index.topic(input.topic_id as string);
        if (!topic) { return { ok: false, code: 'HELP_TOPIC_UNKNOWN' }; }
        const detail = (input.detail as Detail | undefined) ?? 'full';
        const shipped = topic.applicability === 'implemented';
        return { ok: true, value: {
            ...this.identity(), topic_id: topic.id, topic_revision: this.index.revision(topic.id), source_id: topic.source, title: topic.title, area: topic.area,
            applicability: topic.applicability, ...(topic.route ? { route: topic.route, location: location(topic.route) } : {}),
            summary: summary(topic.body), ...(detail !== 'summary' ? { steps: steps(topic.body) } : {}), ...(detail === 'full' ? { body: topic.body } : {}),
            aliases: topic.aliases, related_airk_help: topic.relatedAirkHelp,
            notes: [CLI_NOTE, ...(shipped ? [] : [`This screen is ${topic.applicability.replace('_', ' ')} in extension ${this.index.catalog.extensionVersion}; do not tell the user it exists yet.`])],
        } };
    }

    private async status(args: unknown): Promise<ToolResult> {
        const input = parse(args, {}, []);
        if (typeof input === 'string') { return { ok: false, code: 'INVALID_SCHEMA', detail: input }; }
        const attachment = await this.attachment();
        return { ok: true, value: {
            ...this.identity(), schema_versions: { ui_status: 1, help: 1, drafts: [] },
            ui: attachment.attached
                ? { attached: true, binding_fingerprint: attachment.bindingFingerprint ?? null }
                : { attached: false, code: attachment.code ?? 'UI_NOT_ATTACHED', message: EXPLORER_ERRORS[attachment.code ?? 'UI_NOT_ATTACHED'].message },
            routes: ROUTE_KEYS.map(route => ({ route, location: location(route), availability: FEATURES[route] })),
            tools: [...ASSISTANCE_TOOLS], help_uri_prefix: ASSISTANCE_HELP_URI,
            notes: ['Help works without an attached window. Draft tools are not available in this version.'],
        } };
    }
}
