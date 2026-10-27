// SPDX-License-Identifier: Apache-2.0
// Explorer Assistance tool catalog. Help tools answer from the bundled catalog; ui_status and the
// draft tools reach one trusted Explorer window over its private endpoint. No tool reads source,
// calls AIRK Retrieval, runs commands, changes settings or writes files: drafts wait for the user.
import { ASSISTANCE_HELP_URI, ASSISTANCE_SERVER_NAME, ASSISTANCE_TOOLS } from '../explorerContracts';
import { DESTINATIONS, DRAFT_KINDS as KINDS, DRAFT_LIMIT_BYTES, DRAFT_SCHEMA_VERSION, DraftKind, MANAGEMENT_INTENTS, REVIEW_SCOPES, validateDraft } from '../explorerDrafts';
import { EXPLORER_ERRORS, type ExplorerErrorCode } from '../explorerErrors';
import { FEATURES } from '../explorerFeatures';
import { ROUTE_KEYS } from '../explorerRoutes';
import { CORE_PACK, OPERATORS } from '../explorerStrategies';
import type { WindowChannel } from './attach';
import { DETAILS, type Detail, HelpIndex, location, SEARCH_BYTES, SEARCH_LIMIT, steps, summary } from './help';

export interface Client { readonly name: string; readonly version: string }
export type ToolResult = { readonly ok: true; readonly value: Record<string, unknown> } | { readonly ok: false; readonly code: ExplorerErrorCode; readonly detail?: string };

const TOPIC_ID = '^[a-z][a-z0-9.-]{2,63}$';
const READ_ONLY = { readOnlyHint: true, destructiveHint: false, idempotentHint: true, openWorldHint: false } as const;
/** Drafts change only the Inbox: never files, settings, the index or another program. */
const DRAFT = { readOnlyHint: false, destructiveHint: false, idempotentHint: true, openWorldHint: false } as const;
const CLI_NOTE = 'These are AIRK Explorer steps in the VS Code extension. The native airk CLI has separate commands; ask AIRK Retrieval airk_help for those.';
const OPERATOR_LIST = Object.entries(OPERATORS).filter(([, spec]) => !('unavailable' in spec)).map(([id]) => id).join(', ');

const ENVELOPE = {
    schema_version: { type: 'string', const: DRAFT_SCHEMA_VERSION },
    idempotency_key: { type: 'string', pattern: '^[A-Za-z0-9._:-]{1,128}$', description: 'Reuse it to retry; the same key with a different body is refused.' },
    title: { type: 'string', minLength: 1, maxLength: 160 },
    expected_target_revision: { type: 'integer', minimum: 0, description: 'Target draft revision from airk_explorer_ui_status target_revisions.' },
    workspace_binding: { type: 'string', pattern: '^[0-9a-f]{16}$', description: 'binding_fingerprint from airk_explorer_ui_status.' },
    evidence: { type: 'object', additionalProperties: false, required: ['publication'], properties: {
        publication: { type: 'string', maxLength: 256, description: 'Exact AIRK publication token the evidence came from.' },
        citations: { type: 'array', maxItems: 50, items: { type: 'object', additionalProperties: false, required: ['path', 'start_line'],
            properties: { path: { type: 'string' }, start_line: { type: 'integer', minimum: 1 }, end_line: { type: 'integer', minimum: 1 }, content_hash: { type: 'string', pattern: '^[0-9a-f]{64}$' } } } },
    } },
} as const;
const DRAFT_NOTE = ` The draft goes to the Suggestions Inbox of the attached Explorer window and waits for the user; nothing is applied, saved or run. Bodies are limited to ${DRAFT_LIMIT_BYTES} bytes; unknown fields are refused.`;

export function toolDefinitions(index: HelpIndex): object[] {
    const definitions: Record<typeof ASSISTANCE_TOOLS[number], object> = {
        airk_explorer_help_search: {
            description: 'Search offline Explorer and AIRK engine guidance for this version: Developer, Tester, Architect, Writer workflows, indexing choices, document types, OCR, tools and UX. Returns topic ids for airk_explorer_help_get; check applicability before directing a user to a screen.',
            inputSchema: { type: 'object', additionalProperties: false, required: ['query'], properties: {
                query: { type: 'string', minLength: 1, maxLength: 256 },
                area: { type: 'string', enum: index.areas },
                limit: { type: 'integer', minimum: 1, maximum: SEARCH_LIMIT.max, default: SEARCH_LIMIT.default },
                max_bytes: { type: 'integer', minimum: SEARCH_BYTES.min, maximum: SEARCH_BYTES.max, default: SEARCH_BYTES.default },
            } }, annotations: READ_ONLY,
        },
        airk_explorer_help_get: {
            description: 'Read one AIRK Explorer help topic by id: where it is in the UI, ordered steps, applicability for this installed version and related airk_help topics on AIRK Retrieval.',
            inputSchema: { type: 'object', additionalProperties: false, required: ['topic_id'], properties: {
                topic_id: { type: 'string', pattern: TOPIC_ID },
                detail: { type: 'string', enum: DETAILS, default: 'full' },
            } }, annotations: READ_ONLY,
        },
        airk_explorer_ui_status: {
            description: 'Report whether a trusted AIRK Explorer window is attached for this working directory, its binding fingerprint and draft revisions, the screens this version ships and the schema versions. Returns no paths or source content.',
            inputSchema: { type: 'object', additionalProperties: false, properties: {} }, annotations: READ_ONLY,
        },
        airk_explorer_suggest_search: {
            description: `Propose a Search › Query form (lexical, vector or hybrid; literal path prefix; limit; byte budget). The user decides whether to run it.${DRAFT_NOTE}`,
            inputSchema: { type: 'object', additionalProperties: false, required: ['schema_version', 'idempotency_key', 'title', 'draft'], properties: { ...ENVELOPE,
                draft: { type: 'object', additionalProperties: false, required: ['query'], properties: {
                    query: { type: 'string', minLength: 1, maxLength: 4096 }, mode: { type: 'string', enum: ['policy', 'lexical', 'vector', 'hybrid'], default: 'lexical' },
                    path_prefix: { type: 'string', maxLength: 1024 }, limit: { type: 'integer', minimum: 1, maximum: 100 }, max_bytes: { type: 'integer', minimum: 1024, maximum: 65536 },
                    full_chunks: { type: 'boolean' } } },
                intent: { type: 'string', maxLength: 1000 } } }, annotations: DRAFT,
        },
        airk_explorer_suggest_strategy: {
            description: `Propose a Strategies › Studio draft from a core template (template_id such as airk.core/bug) and/or custom steps. Steps use only these operators: ${OPERATOR_LIST}; arguments bind only \${params.NAME} and \${scope.pathPrefix}, and fan-out steps name an earlier step as input.${DRAFT_NOTE}`,
            inputSchema: { type: 'object', additionalProperties: false, required: ['schema_version', 'idempotency_key', 'title'], properties: { ...ENVELOPE,
                template_id: { type: 'string', pattern: '^airk\\.core/[a-z][a-z0-9-]*$' }, objective: { type: 'string', maxLength: 4000 },
                parameters: { type: 'object', additionalProperties: { type: 'string', maxLength: 512 }, maxProperties: 8 }, path_prefix: { type: 'string', maxLength: 1024 },
                steps: { type: 'array', maxItems: 24, items: { type: 'object', additionalProperties: false, required: ['id', 'op', 'args'], properties: {
                    id: { type: 'string', pattern: '^[a-z][a-z0-9-]{0,31}$' }, op: { type: 'string', enum: Object.keys(OPERATORS) }, args: { type: 'object' }, input: { type: 'string' } } } },
                evidence_budget_bytes: { type: 'integer', minimum: 1024, maximum: 262144 } } }, annotations: DRAFT,
        },
        airk_explorer_suggest_plan: {
            description: `Propose a Strategies › Planner draft: tasks T1, T2… with dependencies and acceptance notes, and an optional relative .md path hint. The user saves the file; there is no save field.${DRAFT_NOTE}`,
            inputSchema: { type: 'object', additionalProperties: false, required: ['schema_version', 'idempotency_key', 'title', 'tasks'], properties: { ...ENVELOPE,
                objective: { type: 'string', maxLength: 4000 }, path_hint: { type: 'string', maxLength: 1024 },
                tasks: { type: 'array', minItems: 1, maxItems: 40, items: { type: 'object', additionalProperties: false, required: ['task_id', 'title'], properties: {
                    task_id: { type: 'string', pattern: '^T[1-9][0-9]{0,2}$' }, title: { type: 'string', maxLength: 160 }, instructions: { type: 'string', maxLength: 6000 },
                    depends_on: { type: 'array', items: { type: 'string' }, maxItems: 20 }, acceptance: { type: 'string', maxLength: 1000 } } } } } }, annotations: DRAFT,
        },
        airk_explorer_suggest_review: {
            description: `Propose opening Review › Changes, Impact or Tests, optionally naming declarations to look at and a short note for the user. Review only reads Git and the index; it never runs tests.${DRAFT_NOTE}`,
            inputSchema: { type: 'object', additionalProperties: false, required: ['schema_version', 'idempotency_key', 'title'], properties: { ...ENVELOPE,
                scope: { type: 'string', enum: REVIEW_SCOPES, default: 'changes' }, names: { type: 'array', maxItems: 20, items: { type: 'string', pattern: '^[A-Za-z_$][\\w$]{0,127}$' } },
                note: { type: 'string', maxLength: 2000 } } }, annotations: DRAFT,
        },
        airk_explorer_suggest_management: {
            description: `Propose management guidance: open one Management screen with fixed guidance for a known intent. It never starts, approves or runs indexing, repair, verification, rebuild, removal or service actions; the user decides on that screen.${DRAFT_NOTE}`,
            inputSchema: { type: 'object', additionalProperties: false, required: ['schema_version', 'idempotency_key', 'title', 'intent'], properties: { ...ENVELOPE,
                intent: { type: 'string', enum: Object.keys(MANAGEMENT_INTENTS) }, mode: { type: 'string', const: 'show_guidance' }, reason: { type: 'string', maxLength: 1000 } } }, annotations: DRAFT,
        },
        airk_explorer_proposal_status: {
            description: 'Read proposal status, machine-readable reason, admitted/current/applied draft revisions and expiry. Applied means copied into an Explorer draft; executed is always false. Returns no body.',
            inputSchema: { type: 'object', additionalProperties: false, required: ['proposal_id'], properties: { proposal_id: { type: 'string', pattern: '^ex-prop-[0-9a-f]{16}$' } } },
            annotations: READ_ONLY,
        },
        airk_explorer_templates_list: {
            description: 'Discover installed core strategy templates, their ids, versions, purposes and runnable/preview availability. Offline, bounded and paginated; never runs a template.',
            inputSchema: { type: 'object', additionalProperties: false, properties: { offset: { type: 'integer', minimum: 0, maximum: 25, default: 0 },
                limit: { type: 'integer', minimum: 1, maximum: 25, default: 10 }, max_bytes: { type: 'integer', minimum: 1024, maximum: 32768, default: 8192 } } }, annotations: READ_ONLY,
        },
        airk_explorer_template_get: {
            description: 'Read one complete installed template: exact parameters/defaults, steps, budgets, instructions and operator argument contracts. Includes runnable/preview availability. No execution.',
            inputSchema: { type: 'object', additionalProperties: false, required: ['template_id'], properties: { template_id: { type: 'string', pattern: '^airk\\.core/[a-z][a-z0-9-]*$' } } }, annotations: READ_ONLY,
        },
        airk_explorer_validate_draft: {
            description: 'Dry-run the same strict draft validation as submission, without Inbox admission, rate consumption or idempotency reservation. If a trusted window is attached, also check binding, target revision and existing idempotency conflicts. Offline validation never proves live applicability.',
            inputSchema: { type: 'object', additionalProperties: false, required: ['kind', 'draft'], properties: { kind: { type: 'string', enum: KINDS }, draft: { type: 'object', description: 'Exact arguments you would pass to the corresponding suggest tool, including its envelope.' } } }, annotations: READ_ONLY,
        },
        airk_explorer_suggest_navigation: {
            description: `Propose opening an implemented Explorer route and prefilling its supported fields. Code/docs allow name and path_prefix; docs also kind; Checker allows text; indexed Changes allows since. Other routes accept no filters. Applying never submits a query or executes management.${DRAFT_NOTE}`,
            inputSchema: { type: 'object', additionalProperties: false, required: ['schema_version', 'idempotency_key', 'title', 'route'], properties: { ...ENVELOPE,
                route: { type: 'string', enum: ROUTE_KEYS.filter(route => FEATURES[route] === 'implemented') }, note: { type: 'string', maxLength: 1000 },
                filters: { type: 'object', additionalProperties: false, properties: { name: { type: 'string', maxLength: 256 }, path_prefix: { type: 'string', maxLength: 1024 },
                    kind: { type: 'string', maxLength: 32 }, text: { type: 'string', maxLength: 8000 }, since: { type: 'string', maxLength: 256 } } } } }, annotations: DRAFT,
        },
        airk_explorer_handoff_get: {
            description: 'Read only a complete Studio prompt or Planner task explicitly shared by the user. Requires its expiring opaque token. Returns revision, workspace binding, evidence publication when present, text and SHA-256. Draft/binding/publication changes revoke access. No general draft access or execution.',
            inputSchema: { type: 'object', additionalProperties: false, required: ['token'], properties: { token: { type: 'string', pattern: '^ex-share-[0-9a-f]{64}$' } } }, annotations: READ_ONLY,
        },
    };
    return ASSISTANCE_TOOLS.map(name => ({ name, ...definitions[name] }));
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
const DRAFT_KINDS: Record<string, DraftKind> = { airk_explorer_suggest_search: 'search', airk_explorer_suggest_strategy: 'strategy', airk_explorer_suggest_plan: 'plan',
    airk_explorer_suggest_review: 'review', airk_explorer_suggest_management: 'management', airk_explorer_suggest_navigation: 'navigation' };

export class AssistanceTools {
    constructor(private readonly index: HelpIndex, private readonly window: WindowChannel) {}

    private identity() {
        return { server: ASSISTANCE_SERVER_NAME, extension_version: this.index.catalog.extensionVersion, catalog_version: this.index.catalog.catalogVersion };
    }

    async call(name: string, args: unknown, client: Client = { name: 'unknown client', version: '' }): Promise<ToolResult> {
        if (Object.hasOwn(DRAFT_KINDS, name)) { return this.suggest(DRAFT_KINDS[name], args, client); }
        switch (name) {
        case 'airk_explorer_help_search': return this.search(args);
        case 'airk_explorer_help_get': return this.get(args);
        case 'airk_explorer_ui_status': return this.status(args);
        case 'airk_explorer_proposal_status': return this.proposal(args);
        case 'airk_explorer_templates_list': return this.templates(args);
        case 'airk_explorer_template_get': return this.template(args);
        case 'airk_explorer_validate_draft': return this.validate(args);
        case 'airk_explorer_handoff_get': return this.handoff(args);
        default: return { ok: false, code: 'CAPABILITY_DISABLED', detail: 'Unknown Explorer Assistance tool.' };
        }
    }

    private templates(args: unknown): ToolResult {
        const input = parse(args, { offset: { kind: 'integer', min: 0, max: CORE_PACK.length }, limit: { kind: 'integer', min: 1, max: 25 }, max_bytes: { kind: 'integer', min: 1024, max: 32768 } }, []);
        if (typeof input === 'string') { return { ok: false, code: 'INVALID_SCHEMA', detail: input }; }
        const offset = Number(input.offset ?? 0), limit = Number(input.limit ?? 10), budget = Number(input.max_bytes ?? 8192);
        const value = { ...this.identity(), templates: [] as object[], total: CORE_PACK.length, next_offset: null as number | null };
        for (const entry of CORE_PACK.slice(offset, offset + limit)) {
            const item = { template_id: entry.strategy.id, version: entry.strategy.version, title: entry.strategy.title, summary: entry.strategy.summary,
                category: entry.strategy.category, runnable: entry.runnable, ...(entry.reason ? { reason: entry.reason } : {}) };
            if (bytes({ ...value, templates: [...value.templates, item], next_offset: CORE_PACK.length }) > budget) { break; }
            value.templates.push(item);
        }
        if (offset + value.templates.length < CORE_PACK.length) { value.next_offset = offset + value.templates.length; }
        return { ok: true, value };
    }
    private template(args: unknown): ToolResult {
        const input = parse(args, { template_id: { kind: 'string', max: 80, pattern: /^airk\.core\/[a-z][a-z0-9-]*$/ } }, ['template_id']);
        if (typeof input === 'string') { return { ok: false, code: 'INVALID_SCHEMA', detail: input }; }
        const entry = CORE_PACK.find(item => item.strategy.id === input.template_id);
        if (!entry) { return { ok: false, code: 'CAPABILITY_DISABLED', detail: 'Unknown template; use airk_explorer_templates_list.' }; }
        return { ok: true, value: { ...this.identity(), ...entry, operators: Object.fromEntries([...new Set(entry.strategy.steps.map(step => step.op))].map(op => [op, OPERATORS[op]])) } };
    }
    private async validate(args: unknown): Promise<ToolResult> {
        if (!args || typeof args !== 'object' || Array.isArray(args) || Object.keys(args).some(key => !['kind', 'draft'].includes(key))) { return { ok: false, code: 'INVALID_SCHEMA' }; }
        const { kind, draft } = args as { kind: DraftKind; draft: unknown };
        if (!KINDS.includes(kind)) { return { ok: false, code: 'INVALID_SCHEMA', detail: 'Unknown draft kind.' }; }
        const checked = validateDraft(kind, draft);
        if (!checked.ok) { return { ok: true, value: { ...this.identity(), valid: false, code: checked.code, errors: checked.errors.map(message => ({ field: message.split(':')[0], message })), live_checked: false, admitted: false, executed: false } }; }
        const reply = await this.window({ type: 'validate', kind, draft });
        return { ok: true, value: { ...this.identity(), valid: true, admitted: false, executed: false,
            ...(reply.ok && 'validation' in reply.reply ? { ...reply.reply.validation, live_checked: true } : { live_checked: false, live_check_code: reply.ok ? 'HOST_UNAVAILABLE' : reply.code }) } };
    }
    private async handoff(args: unknown): Promise<ToolResult> {
        const input = parse(args, { token: { kind: 'string', max: 73, pattern: /^ex-share-[0-9a-f]{64}$/ } }, ['token']);
        if (typeof input === 'string') { return { ok: false, code: 'INVALID_SCHEMA', detail: input }; }
        const reply = await this.window({ type: 'handoff_get', token: String(input.token) });
        if (!reply.ok) { return { ok: false, code: reply.code, detail: reply.message }; }
        return 'handoff' in reply.reply ? { ok: true, value: { ...reply.reply.handoff } } : { ok: false, code: 'HOST_UNAVAILABLE' };
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
            notes: [topic.area === 'engine' ? 'Bundled engine guidance, not live state. Use AIRK Retrieval airk_help command/tool selectors for the running binary\'s exact command flags and tool schemas.' : CLI_NOTE,
                ...(shipped ? [] : [`This screen is ${topic.applicability.replace('_', ' ')} in extension ${this.index.catalog.extensionVersion}; do not tell the user it exists yet.`])],
        } };
    }

    private async status(args: unknown): Promise<ToolResult> {
        const input = parse(args, {}, []);
        if (typeof input === 'string') { return { ok: false, code: 'INVALID_SCHEMA', detail: input }; }
        const reply = await this.window({ type: 'status' });
        const window = reply.ok && 'status' in reply.reply ? reply.reply.status : undefined;
        const code = reply.ok ? undefined : reply.code;
        return { ok: true, value: {
            ...this.identity(), schema_versions: { ui_status: 2, help: 1, drafts: [DRAFT_SCHEMA_VERSION], handoff: 1 },
            ui: window ? { attached: true, ...window }
                : { attached: false, code: code ?? 'UI_NOT_ATTACHED', blockers: [code ?? 'UI_NOT_ATTACHED'], message: EXPLORER_ERRORS[code ?? 'UI_NOT_ATTACHED'].message },
            routes: ROUTE_KEYS.map(route => ({ route, location: location(route), availability: FEATURES[route] })),
            draft_destinations: DESTINATIONS, tools: [...ASSISTANCE_TOOLS], help_uri_prefix: ASSISTANCE_HELP_URI,
            notes: ['Help works without an attached window. Drafts need an attached, trusted Explorer window and wait in its Suggestions Inbox for the user.'],
        } };
    }

    private async suggest(kind: DraftKind, args: unknown, client: Client): Promise<ToolResult> {
        // Fail fast here; the window validates the same draft again before admitting it.
        const checked = validateDraft(kind, args);
        if (!checked.ok) { return { ok: false, code: checked.code, detail: checked.errors.join('; ').slice(0, 2000) }; }
        const reply = await this.window({ type: 'suggest', kind, draft: args, client });
        if (!reply.ok) { return { ok: false, code: reply.code, ...(reply.message ? { detail: reply.message } : {}) }; }
        if (!('receipt' in reply.reply)) { return { ok: false, code: 'HOST_UNAVAILABLE' }; }
        return { ok: true, value: { ...reply.reply.receipt, message: 'Draft added to the Explorer Suggestions Inbox. The user decides whether to apply it; nothing was run or saved.' } };
    }

    private async proposal(args: unknown): Promise<ToolResult> {
        const input = parse(args, { proposal_id: { kind: 'string', max: 32, pattern: /^ex-prop-[0-9a-f]{16}$/ } }, ['proposal_id']);
        if (typeof input === 'string') { return { ok: false, code: 'INVALID_SCHEMA', detail: input }; }
        const reply = await this.window({ type: 'proposal_status', proposal_id: input.proposal_id as string });
        if (!reply.ok) { return { ok: false, code: reply.code, ...(reply.message ? { detail: reply.message } : {}) }; }
        return 'proposal' in reply.reply ? { ok: true, value: { ...reply.reply.proposal } } : { ok: false, code: 'HOST_UNAVAILABLE' };
    }
}
