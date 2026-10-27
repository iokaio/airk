// SPDX-License-Identifier: Apache-2.0
// Pure, versioned contracts between the extension host and the AIRK Explorer webview.
// No Node, vscode or host imports: the webview bundle and the host share this file.
import type { Availability } from './explorerFeatures';
import type { ExplorerError } from './explorerErrors';

export const MESSAGE_VERSION = 1;

// ---- Shared state vocabulary (design §9.2). Words are used identically on every screen.
export type ConnectionState = 'connecting' | 'ready' | 'degraded' | 'disconnected' | 'incompatible' | 'untrusted';
export type IndexState = 'not_registered' | 'not_indexed' | 'indexing' | 'available' | 'unavailable' | 'unknown';
/** Native scheduler freshness (airk_status.freshness.state); never proof that live source bytes match. */
export type FreshnessState = 'unregistered' | 'unavailable' | 'idle' | 'pending' | 'running' | 'paused' | 'blocked' | 'failed' | 'unknown';
export type SemanticState = 'pending' | 'partial' | 'ready' | 'failed' | 'disabled' | 'unknown';
/** E1 indexed text, E2 syntax fact, E3 rule observation, E4 provider-asserted (design §9.1). */
export type EvidenceClass = 'E1' | 'E2' | 'E3' | 'E4';
export type SearchMode = 'policy' | 'lexical' | 'vector' | 'hybrid';
export type LiveCheck = 'matches_index' | 'changed_since_index' | 'not_saved' | 'missing' | 'outside_workspace' | 'unchecked';

/** A registered project as observed through the cached native console page. */
export interface ProjectSummary {
    readonly project: string;
    readonly collection: string;
    readonly name: string;
    readonly root: string;
    readonly current: boolean;
    readonly paused?: boolean;
    readonly watching?: boolean;
    readonly publication?: string;
    readonly indexedFiles?: number;
    readonly semantic?: SemanticState;
    readonly indexState?: string;
    readonly jobState?: string;
    readonly jobKind?: string;
    readonly phase?: string;
    readonly observedAt?: number;
}

/** Published readiness of the bound project from airk_status; absent fields stay unknown. */
export interface StatusSummary {
    readonly indexState: IndexState;
    readonly indexError?: string;
    readonly publication?: string;
    readonly files?: number;
    readonly chunks?: number;
    readonly semantic: SemanticState;
    readonly semanticCompleted?: number;
    readonly semanticSelected?: number;
    readonly freshness: FreshnessState;
    readonly liveSourceVerified?: boolean;
    readonly vectorsEnabled?: boolean;
    readonly localModelReady?: boolean;
    readonly versionsMatch?: boolean | null;
    readonly workPhase?: string;
    readonly workPaused?: boolean;
    readonly observedAt: number;
}

export interface ExplorerViewState {
    readonly revision: number;
    readonly extensionVersion: string;
    readonly trusted: boolean;
    readonly connection: ConnectionState;
    readonly projects: readonly ProjectSummary[];
    readonly projectsObservedAt?: number;
    readonly selectedProject?: string;
    readonly status?: StatusSummary;
    readonly features: Readonly<Record<string, Availability>>;
    readonly notice?: ExplorerError;
    /** Last outcome code from the shared control plane (native ServiceError codes pass through). */
    readonly controlMessage?: string;
    /** More registered projects exist beyond the cached page shown. */
    readonly moreProjects?: boolean;
    /** Present while a Management tab is open; built from the shared console state. */
    readonly management?: ManagementView;
}

/** One search hit. `ref` is an opaque host-issued handle; the webview never sends paths back. */
export interface HitView {
    readonly ref: string;
    readonly path: string;
    readonly startLine: number;
    readonly endLine: number;
    readonly text: string;
    readonly truncated: boolean;
    readonly contentHash: string;
    readonly evidenceClass: EvidenceClass;
    readonly location?: string;
}
export interface SearchResultView {
    readonly query: string;
    readonly appliedMode: string;
    readonly ordering: 'index_identity' | 'fused_rank' | 'unknown';
    readonly returned: number;
    readonly limit: number;
    readonly truncated: boolean;
    readonly publication?: string;
    readonly semantic?: SemanticState;
    readonly hits: readonly HitView[];
    readonly warnings: readonly string[];
}
export interface PassageView {
    readonly ref: string;
    readonly path: string;
    readonly startLine: number;
    readonly endLine: number;
    readonly text: string;
    readonly truncated: boolean;
    readonly publication?: string;
}
export interface OpenSourceResult { readonly live: LiveCheck; readonly opened: boolean; readonly message: string }

/** Who produced a fact and what that producer covers (verbatim from AIRK). */
export interface ProvenanceView { readonly producer: string; readonly basis?: string; readonly coverage?: string }
/** One structural or document fact with its cited location. Handles are opaque and host-issued. */
export interface FactItemView {
    readonly ref?: string;
    readonly path: string;
    readonly startLine?: number;
    readonly endLine?: number;
    readonly kind: string;
    readonly name: string;
    readonly signature?: string;
    readonly qualifiedName?: string;
    /** Handle for a native symbol id; pass it back to inspect relationships. */
    readonly symbolRef?: string;
    readonly evidenceClass: EvidenceClass;
    readonly basis?: string;
    readonly producer?: string;
    readonly resolution?: string;
    readonly target?: string;
    readonly heading?: readonly string[];
    /** Original document location (page, slide, sheet row) when the citation is not plain lines. */
    readonly location?: string;
    readonly detail?: string;
}
export interface FactListView {
    readonly tool: string;
    readonly items: readonly FactItemView[];
    readonly returned: number;
    readonly limit: number;
    readonly truncated: boolean;
    readonly publication?: string;
    /** Continuation handle when AIRK returned a cursor; absent for tools without cursors. */
    readonly more?: string;
    readonly provenance: readonly ProvenanceView[];
    readonly notes: readonly string[];
}

/** One file's indexed record compared with its saved bytes (Evidence › Inspector). */
export interface InspectView {
    readonly ref: string;
    readonly path: string;
    readonly startLine: number;
    readonly endLine: number;
    readonly contentHash: string;
    readonly publication?: string;
    readonly live: LiveCheck;
    readonly liveMessage: string;
    readonly language?: string;
    readonly parseStatus?: string;
    readonly lineBasis?: string;
    readonly extractionMethod?: string;
    readonly indexedLines?: number;
    readonly sourceBytes?: number;
    readonly chunks?: number;
    readonly facts?: number;
    readonly sourceRoot?: string;
}

export type CitationStatus = 'valid' | 'changed_since_index' | 'not_saved' | 'missing' | 'not_found' | 'line_out_of_range' | 'outside_collection';
export interface CitationResult {
    readonly text: string;
    readonly path: string;
    readonly startLine: number;
    readonly endLine: number;
    readonly status: CitationStatus;
    readonly message: string;
    readonly ref?: string;
}
export interface CitationCheckView {
    /** False when the text contains no supported citation: prose alone is never a pass. */
    readonly supported: boolean;
    readonly citations: readonly CitationResult[];
    /** True when more citations were present than one check examines. */
    readonly limited: boolean;
    readonly publication?: string;
}

export interface ChangeItemView { readonly path?: string; readonly subject: string; readonly causes: readonly string[]; readonly dimensions: readonly string[]; readonly observations?: number }
export interface ChangesView {
    readonly since: string;
    readonly baseline: string;
    readonly status: string;
    readonly items: readonly ChangeItemView[];
    readonly truncated: boolean;
    readonly publication?: string;
    readonly coverage: readonly string[];
}

// ---- Management (shared Service Console controller, organized into tabs).
/** A labeled observation; values are already formatted text. Unknown stays "Not observed". */
export type FactRow = readonly [string, string];
export interface ManagedProject {
    readonly summary: ProjectSummary;
    readonly rows: readonly FactRow[];
    readonly activity: readonly FactRow[];
    readonly selected: boolean;
    readonly jobId?: string;
    readonly jobState?: string;
    readonly pendingJobId?: string;
    readonly configurationResult: boolean;
    readonly canDeleteDerived: boolean;
    readonly uncertainConfiguration: boolean;
}
export interface PlanItemView { readonly project: string; readonly phase: string; readonly action: string; readonly availability: string; readonly estimate: string; readonly source: string; readonly fallback: string; readonly result: string; readonly cleanup: string }
export interface PlanView {
    readonly rows: readonly FactRow[];
    readonly items: readonly PlanItemView[];
    readonly morePages: boolean;
    readonly paused: boolean;
    /** True only when every preview page was read and the plan is ready: approval is allowed. */
    readonly approvable: boolean;
}
export interface PendingIntentView { readonly id: string; readonly title: string; readonly unknown: boolean; readonly lifecycle: boolean; readonly forceAvailable: boolean }
export interface ManagementView {
    readonly busy: boolean;
    readonly connected: boolean;
    readonly trusted: boolean;
    readonly authority: string;
    readonly lifecycle?: string;
    readonly responseMode: string;
    readonly message?: string;
    readonly service: readonly FactRow[];
    readonly runtime: readonly FactRow[];
    readonly resources: readonly FactRow[];
    readonly inference: readonly FactRow[];
    /** Which lifecycle and runtime actions the console would currently accept. */
    readonly enabled: Readonly<Record<string, boolean>>;
    readonly projects: readonly ManagedProject[];
    readonly inactive: boolean;
    readonly morePages: boolean;
    readonly pageNote: string;
    readonly selectedCount: number;
    readonly plan?: PlanView;
    readonly pending: readonly PendingIntentView[];
    readonly events: readonly string[];
    readonly processEvents: readonly string[];
    readonly historyOmitted: boolean;
    readonly legacy?: { readonly rows: readonly FactRow[]; readonly canRemove: boolean; readonly reviewed: boolean; readonly receipts: readonly { id: string; phase: string; service: string }[] };
    readonly preview?: string;
    readonly includePaths: boolean;
}

/** Console action names the Explorer may send; identical to the console's own allowlist (tested). */
export const MANAGE_ACTIONS = [
    'reviewLegacyStartup', 'legacyStartupReceipts', 'removeLegacyStartup', 'copyLegacyStartup', 'registeredProjects', 'inactiveProjects', 'registerWorkspace',
    'refresh', 'connect', 'start', 'stop', 'restart', 'repairRuntime', 'upgradeRuntime', 'rollbackRuntime', 'resumeActivation', 'logs', 'resources',
    'responseMode', 'recoverLifecycle', 'abandonLifecycle', 'startEmbedding', 'stopEmbedding', 'forceEmbedding', 'firstPage', 'nextPage', 'morePlan',
    'refreshPlan', 'pausePlan', 'resumePlan', 'cancelPlan', 'approvePlan', 'previewReport', 'copyReport',
    'previewCleanup', 'deleteDerived', 'inspect', 'index', 'pause', 'resume', 'restartProject', 'watch', 'modelSetup', 'configureProject', 'copyConfiguration',
    'select', 'repair', 'resumeLegacyStartup', 'retryJob', 'cancelJob', 'retryIntent', 'forgetIntent', 'forceIntent',
] as const;
/** Existing AIRK commands that Management may open; each keeps its own dialogs and confirmations. */
export const MANAGE_COMMANDS = ['airk.setupAgent', 'airk.verifyMcp', 'airk.manageVscodeMcp', 'airk.installCli', 'airk.openTools', 'airk.chooseStorage',
    'airk.indexMode', 'airk.usage', 'airk.logs'] as const;

/** One Explorer help topic, compiled from src/explorerHelp at build time and versioned with the VSIX. */
export interface HelpTopic {
    readonly id: string;
    readonly title: string;
    readonly route?: string;
    readonly area: string;
    /** From the feature manifest at build time: never claims a screen that is not shipped. */
    readonly applicability: 'implemented' | 'prototype_only' | 'unsupported' | 'disabled';
    readonly aliases: readonly string[];
    /** airk_help topic ids for engine-level detail (served by AIRK Retrieval, not proxied). */
    readonly relatedAirkHelp: readonly string[];
    /** Repository-relative topic file, reported as the help source identity. */
    readonly source: string;
    readonly body: string;
}
export interface HelpCatalog { readonly catalogVersion: 1; readonly extensionVersion: string; readonly topics: readonly HelpTopic[] }

/** The extension-owned Explorer Assistance MCP server. It never calls AIRK Retrieval or the service. */
export const ASSISTANCE_SERVER_NAME = 'airk-explorer';
/** Assistance tools shipped in this version, in tools/list order. Draft tools arrive with the Suggestions Inbox. */
export const ASSISTANCE_TOOLS = ['airk_explorer_help_search', 'airk_explorer_help_get', 'airk_explorer_ui_status'] as const;
export const ASSISTANCE_HELP_URI = 'airk-explorer-help://';

/**
 * Written by each trusted Explorer window to <assistance dir>/windows/<session_id>.json
 * (user-only). The Assistance server reads it at connect time; the credential never
 * appears in agent configuration, process arguments or the environment.
 */
export const WINDOW_SCHEMA = 'airk-explorer-window/1';
export const WINDOW_DESCRIPTOR_LIMIT = 16384;
/** Largest single IPC line in either direction; drafts are at most 16 KiB. */
export const ASSISTANCE_IPC_LIMIT = 65536;
export interface WindowDescriptor {
    readonly schema: typeof WINDOW_SCHEMA;
    readonly session_id: string;
    readonly endpoint: string;
    readonly credential: string;
    /** Canonical workspace folder paths; an agent attaches when its working directory is inside one. */
    readonly roots: readonly string[];
    readonly binding_fingerprint: string;
    readonly extension_version: string;
    readonly expires_unix_ms: number;
}
const WINDOW_KEYS = ['schema', 'session_id', 'endpoint', 'credential', 'roots', 'binding_fingerprint', 'extension_version', 'expires_unix_ms'];
const PIPE = /^\\\\\.\\pipe\\airk-explorer-[0-9a-f]{32}$/;
const SOCKET = /^\/(?:[^/\0]+\/)*airk-explorer\.sock$/;

export function parseWindowDescriptor(value: unknown): WindowDescriptor | undefined {
    if (!value || typeof value !== 'object' || Array.isArray(value)) { return undefined; }
    const entry = value as Record<string, unknown>;
    if (Object.keys(entry).some(key => !WINDOW_KEYS.includes(key))) { return undefined; }
    const hex = (text: unknown, length: number) => typeof text === 'string' && new RegExp(`^[0-9a-f]{${length}}$`).test(text);
    const roots = entry.roots;
    const valid = entry.schema === WINDOW_SCHEMA && hex(entry.session_id, 32) && hex(entry.credential, 64) && hex(entry.binding_fingerprint, 16)
        && typeof entry.endpoint === 'string' && entry.endpoint.length <= 1024 && (PIPE.test(entry.endpoint) || (SOCKET.test(entry.endpoint) && !entry.endpoint.includes('/../')))
        && Array.isArray(roots) && roots.length >= 1 && roots.length <= 32 && roots.every(root => typeof root === 'string' && root.length > 0 && root.length <= 4096)
        && typeof entry.extension_version === 'string' && entry.extension_version.length <= 64
        && typeof entry.expires_unix_ms === 'number' && Number.isSafeInteger(entry.expires_unix_ms);
    return valid ? entry as unknown as WindowDescriptor : undefined;
}

/** One request per connection: the window credential and a typed request. */
export interface AssistanceIpcRequest { readonly v: 1; readonly credential: string; readonly type: 'status' }
export type AssistanceIpcReply =
    | { readonly v: 1; readonly ok: true; readonly status: { readonly binding_fingerprint: string; readonly explorer_open: boolean } }
    | { readonly v: 1; readonly ok: false; readonly code: 'STALE_BINDING' | 'SCHEMA_UNSUPPORTED' | 'INVALID_SCHEMA' };
/** airk_help reply as shown in the Explorer. */
export interface EngineHelpView { readonly topic: string; readonly title: string; readonly content: string; readonly version?: string; readonly topics: readonly { id: string; title: string }[]; readonly truncated: boolean }

/** A citation parsed from free text, e.g. "src/a.ts:12", "src/a.ts:12-20", "src/a.ts#L12-L20". */
export interface ParsedCitation { readonly text: string; readonly path: string; readonly startLine: number; readonly endLine: number; readonly absolute: boolean }
const CITATION = /(?<![\w@.\\/-])((?:[A-Za-z]:[\\/]|\/)?(?:[\w@.-]+[\\/])*[\w@.-]+\.[A-Za-z0-9]+)(?::(\d{1,7})(?:\s*[-–]\s*(\d{1,7}))?|#L(\d{1,7})(?:-L?(\d{1,7}))?)/g;

/** Extract supported citations; URLs and bare prose are ignored. At most `limit` unique citations are returned. */
export function parseCitations(text: string, limit = 50): { citations: ParsedCitation[]; limited: boolean } {
    const seen = new Set<string>(), citations: ParsedCitation[] = [];
    let limited = false;
    const input = text.slice(0, 65536);
    for (const match of input.matchAll(CITATION)) {
        const at = match.index ?? 0;
        // Skip anything inside a URL ("https://host/path/file.ts:3").
        if (/[a-z][a-z0-9+.-]*:\/\/\S*$/i.test(input.slice(Math.max(0, at - 200), at))) { continue; }
        const raw = match[1].replaceAll('\\', '/');
        const start = Number(match[2] ?? match[4]), end = Number(match[3] ?? match[5] ?? start);
        if (!start) { continue; }
        const absolute = /^([A-Za-z]:\/|\/)/.test(raw) || raw.split('/').includes('..');
        const key = `${raw}:${start}:${end}`;
        if (seen.has(key)) { continue; }
        if (citations.length >= limit) { limited = true; break; }
        seen.add(key);
        citations.push({ text: match[0], path: raw.replace(/^\.\//, ''), startLine: start, endLine: Math.max(start, end), absolute });
    }
    return { citations, limited };
}

// ---- Webview → host messages. Each field is validated strictly; unknown keys are rejected.
type FieldSpec =
    | { readonly kind: 'string'; readonly max: number; readonly min?: number; readonly pattern?: RegExp; readonly optional?: boolean }
    | { readonly kind: 'int'; readonly min: number; readonly max: number; readonly optional?: boolean }
    | { readonly kind: 'bool'; readonly optional?: boolean }
    | { readonly kind: 'enum'; readonly values: readonly string[]; readonly optional?: boolean }
    /** A plain JSON object of bounded size; the host validates its schema again before use. */
    | { readonly kind: 'json'; readonly max: number; readonly optional?: boolean };

const PROJECT = { kind: 'string', min: 64, max: 64, pattern: /^[0-9a-f]{64}$/ } as const;
const REF = { kind: 'string', min: 1, max: 64, pattern: /^[A-Za-z0-9_-]+$/ } as const;

export const WEBVIEW_MESSAGES = {
    'ready': {},
    'nav.go': { route: { kind: 'string', min: 1, max: 128 } },
    'project.select': { project: PROJECT },
    'status.refresh': {},
    'search.run': {
        query: { kind: 'string', min: 1, max: 4096 },
        mode: { kind: 'enum', values: ['policy', 'lexical', 'vector', 'hybrid'] },
        pathPrefix: { kind: 'string', max: 1024, optional: true },
        limit: { kind: 'int', min: 1, max: 100 },
        maxBytes: { kind: 'int', min: 1024, max: 65536 },
        fullChunks: { kind: 'bool' },
    },
    'evidence.expand': { ref: REF, before: { kind: 'int', min: 0, max: 100 }, after: { kind: 'int', min: 0, max: 100 } },
    'source.open': { ref: REF },
    'code.symbols': { name: { kind: 'string', max: 256, optional: true }, pathPrefix: { kind: 'string', max: 1024, optional: true }, limit: { kind: 'int', min: 1, max: 100 } },
    'code.facts': { name: { kind: 'string', min: 1, max: 512 }, pathPrefix: { kind: 'string', max: 1024, optional: true }, limit: { kind: 'int', min: 1, max: 100 } },
    'code.references': { name: { kind: 'string', min: 1, max: 256 }, pathPrefix: { kind: 'string', max: 1024, optional: true }, limit: { kind: 'int', min: 1, max: 100 } },
    'code.relationships': { symbol: REF, direction: { kind: 'enum', values: ['incoming', 'outgoing'] }, hops: { kind: 'int', min: 1, max: 3 }, limit: { kind: 'int', min: 1, max: 100 } },
    'code.outline': { ref: { ...REF, optional: true } },
    'list.more': { more: REF },
    'help.engine': { topic: { kind: 'string', min: 1, max: 64, pattern: /^[a-z][a-z0-9-]*$/, optional: true } },
    'evidence.inspect': { ref: REF },
    'evidence.check': { text: { kind: 'string', min: 1, max: 65536 } },
    'evidence.changes': { since: { kind: 'string', min: 9, max: 256, pattern: /^airk-r1:[0-9A-Za-z:_-]+$/ }, limit: { kind: 'int', min: 1, max: 100 } },
    'docs.facts': {
        kind: { kind: 'enum', values: ['any', 'requirement', 'decision', 'design', 'architecture', 'feature', 'acceptance', 'date', 'section', 'table_row', 'table_cell'] },
        name: { kind: 'string', max: 256, optional: true }, pathPrefix: { kind: 'string', max: 1024, optional: true }, limit: { kind: 'int', min: 1, max: 100 },
    },
    'manage.refresh': {},
    'manage.action': {
        action: { kind: 'enum', values: MANAGE_ACTIONS },
        project: { ...PROJECT, optional: true },
        id: { kind: 'string', min: 32, max: 64, pattern: /^[0-9a-f]{32}(?:[0-9a-f]{32})?$/, optional: true },
        scope: { kind: 'enum', values: ['one', 'selected', 'all'], optional: true },
        operation: { kind: 'enum', values: ['auto', 'verify', 'recover', 'reconcile', 'vectors', 'rebuild'], optional: true },
        selected: { kind: 'bool', optional: true },
        includePaths: { kind: 'bool', optional: true },
        allowFallback: { kind: 'bool', optional: true },
    },
    'manage.command': { command: { kind: 'enum', values: MANAGE_COMMANDS } },
    'manage.observe': { open: { kind: 'bool' } },
    'strategy.run': { strategy: { kind: 'json', max: 32768 }, params: { kind: 'json', max: 8192 }, scope: { kind: 'json', max: 2048 } },
    'strategy.export': { strategy: { kind: 'json', max: 32768 } },
    'strategy.import': {},
    'clipboard.write': { text: { kind: 'string', min: 1, max: 262144 } },
} as const satisfies Record<string, Record<string, FieldSpec>>;

export type WebviewMessageType = keyof typeof WEBVIEW_MESSAGES;
type FieldValue<S> = S extends { kind: 'string' } ? string : S extends { kind: 'int' } ? number : S extends { kind: 'bool' } ? boolean
    : S extends { kind: 'enum'; values: readonly (infer V)[] } ? V : S extends { kind: 'json' } ? Record<string, unknown> : never;
type Fields<T> = { -readonly [K in keyof T as T[K] extends { optional: true } ? never : K]: FieldValue<T[K]> }
    & { -readonly [K in keyof T as T[K] extends { optional: true } ? K : never]?: FieldValue<T[K]> };
export type WebviewMessage = { [T in WebviewMessageType]: { v: 1; type: T; requestId: string } & Fields<(typeof WEBVIEW_MESSAGES)[T]> }[WebviewMessageType];
export type WebviewMessageOf<T extends WebviewMessageType> = Extract<WebviewMessage, { type: T }>;

const REQUEST_ID = /^[A-Za-z0-9_-]{1,64}$/;

function valid(spec: FieldSpec, value: unknown): boolean {
    switch (spec.kind) {
        case 'string': return typeof value === 'string' && value.length >= (spec.min ?? 0) && value.length <= spec.max
            && !/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(value) && (!spec.pattern || spec.pattern.test(value));
        case 'int': return typeof value === 'number' && Number.isSafeInteger(value) && value >= spec.min && value <= spec.max;
        case 'bool': return typeof value === 'boolean';
        case 'enum': return typeof value === 'string' && spec.values.includes(value);
        case 'json': {
            if (!value || typeof value !== 'object' || Array.isArray(value) || Object.getPrototypeOf(value) !== Object.prototype) { return false; }
            try { return JSON.stringify(value).length <= spec.max; } catch { return false; }
        }
    }
}

/** Accept only a known message type with exactly its declared fields; anything else is undefined. */
export function parseWebviewMessage(input: unknown): WebviewMessage | undefined {
    if (!input || typeof input !== 'object' || Array.isArray(input)) { return undefined; }
    const message = input as Record<string, unknown>;
    if (message.v !== MESSAGE_VERSION || typeof message.type !== 'string' || !Object.hasOwn(WEBVIEW_MESSAGES, message.type)
        || typeof message.requestId !== 'string' || !REQUEST_ID.test(message.requestId)) { return undefined; }
    const fields = WEBVIEW_MESSAGES[message.type as WebviewMessageType] as Record<string, FieldSpec>;
    for (const key of Object.keys(message)) {
        if (key !== 'v' && key !== 'type' && key !== 'requestId' && !Object.hasOwn(fields, key)) { return undefined; }
    }
    for (const [key, spec] of Object.entries(fields)) {
        if (message[key] === undefined) { if (spec.optional) { continue; } return undefined; }
        if (!valid(spec, message[key])) { return undefined; }
    }
    return message as WebviewMessage;
}

// ---- Host → webview messages.
export type HostMessage =
    | { v: 1; type: 'state'; state: ExplorerViewState }
    | { v: 1; type: 'navigate'; route: string }
    /** A query entered outside the panel (sidebar quick search or a command); the webview runs it. */
    | { v: 1; type: 'search'; query: string }
    | { v: 1; type: 'result'; requestId: string; ok: true; data: unknown }
    | { v: 1; type: 'result'; requestId: string; ok: false; error: ExplorerError };
