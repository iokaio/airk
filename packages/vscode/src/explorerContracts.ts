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

// ---- Webview → host messages. Each field is validated strictly; unknown keys are rejected.
type FieldSpec =
    | { readonly kind: 'string'; readonly max: number; readonly min?: number; readonly pattern?: RegExp; readonly optional?: boolean }
    | { readonly kind: 'int'; readonly min: number; readonly max: number; readonly optional?: boolean }
    | { readonly kind: 'bool'; readonly optional?: boolean }
    | { readonly kind: 'enum'; readonly values: readonly string[]; readonly optional?: boolean };

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
    'manage.index': { project: PROJECT },
    'code.symbols': { name: { kind: 'string', max: 256, optional: true }, pathPrefix: { kind: 'string', max: 1024, optional: true }, limit: { kind: 'int', min: 1, max: 100 } },
    'code.facts': { name: { kind: 'string', min: 1, max: 512 }, pathPrefix: { kind: 'string', max: 1024, optional: true }, limit: { kind: 'int', min: 1, max: 100 } },
    'code.references': { name: { kind: 'string', min: 1, max: 256 }, pathPrefix: { kind: 'string', max: 1024, optional: true }, limit: { kind: 'int', min: 1, max: 100 } },
    'code.relationships': { symbol: REF, direction: { kind: 'enum', values: ['incoming', 'outgoing'] }, hops: { kind: 'int', min: 1, max: 3 }, limit: { kind: 'int', min: 1, max: 100 } },
    'code.outline': { ref: { ...REF, optional: true } },
    'list.more': { more: REF },
    'manage.refresh': {},
} as const satisfies Record<string, Record<string, FieldSpec>>;

export type WebviewMessageType = keyof typeof WEBVIEW_MESSAGES;
type FieldValue<S> = S extends { kind: 'string' } ? string : S extends { kind: 'int' } ? number : S extends { kind: 'bool' } ? boolean
    : S extends { kind: 'enum'; values: readonly (infer V)[] } ? V : never;
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
