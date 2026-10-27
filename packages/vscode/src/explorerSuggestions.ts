// SPDX-License-Identifier: Apache-2.0
// The Suggestions Inbox store. Drafts live in extension-host memory for this window only,
// bounded by TTL, count, bytes and rate. Admission is not acceptance: nothing here applies,
// runs or saves a draft; the user does that from the Inbox.
import { randomBytes } from 'node:crypto';
import { canonical, destinationOf, DRAFT_SCHEMA_VERSION, DraftKind, ProposalReceipt, ProposalStatus, validateDraft, ValidDraft } from './explorerDrafts';
import type { ExplorerErrorCode } from './explorerErrors';
import type { SuggestionCard, SuggestionsView } from './explorerContracts';

export const SUGGESTION_LIMITS = { pending: 50, bytes: 262144, perMinute: 30, retained: 200 } as const;

export interface DraftClient { readonly name: string; readonly version: string }
export interface Suggestion {
    readonly id: string;
    readonly kind: DraftKind;
    readonly draft: ValidDraft;
    /** Self-reported MCP client name; shown to the user, never treated as authenticated. */
    readonly client: DraftClient;
    readonly receivedAt: number;
    readonly expiresAt: number;
    readonly size: number;
    readonly bodyHash: string;
    readonly warnings: readonly string[];
    readonly targetRevision?: number;
    status: ProposalStatus;
}
export type Submission = { ok: true; receipt: ProposalReceipt } | { ok: false; code: ExplorerErrorCode; message: string };

export interface StoreContext {
    /** Opaque window binding fingerprint (never a path). */
    binding(): string;
    /** Current revisions of the Studio and Planner drafts, as last reported by the Explorer webview. */
    revisions(): { readonly studio?: number; readonly planner?: number };
    /** Publication of the selected project, to flag drafts that cite another one. */
    publication(): string | undefined;
    ttlMs(): number;
    now?(): number;
}

export class SuggestionStore {
    private readonly items = new Map<string, Suggestion>();
    private readonly keys = new Map<string, string>();
    private readonly recent: number[] = [];
    private readonly listeners = new Set<() => void>();

    constructor(private readonly context: StoreContext) {}

    private now(): number { return this.context.now?.() ?? Date.now(); }
    onDidChange(listener: () => void): { dispose(): void } { this.listeners.add(listener); return { dispose: () => { this.listeners.delete(listener); } }; }
    private changed(): void { for (const listener of this.listeners) { listener(); } }

    /** Expire lazily and drop the oldest resolved entries beyond the retention bound. */
    private sweep(): void {
        const now = this.now();
        let changed = false;
        for (const item of this.items.values()) {
            if ((item.status === 'pending_review' || item.status === 'deferred') && item.expiresAt <= now) { item.status = 'expired'; changed = true; }
        }
        const resolved = [...this.items.values()].filter(item => item.status !== 'pending_review' && item.status !== 'deferred').sort((a, b) => a.receivedAt - b.receivedAt);
        while (this.items.size > SUGGESTION_LIMITS.retained && resolved.length) {
            const oldest = resolved.shift()!;
            this.items.delete(oldest.id); this.keys.delete(oldest.draft.idempotencyKey); changed = true;
        }
        if (changed) { this.changed(); }
    }

    private receipt(item: Suggestion): ProposalReceipt {
        return { schema_version: DRAFT_SCHEMA_VERSION, proposal_id: item.id, proposal_type: item.kind, status: item.status, destination: destinationOf(item.draft.body),
            workspace_binding: this.context.binding(), ...(item.targetRevision !== undefined ? { target_draft_revision: item.targetRevision } : {}),
            accepted: false, applied: item.status === 'applied', executed: false, received_at: new Date(item.receivedAt).toISOString(),
            expires_at: new Date(item.expiresAt).toISOString(), warnings: item.warnings };
    }

    submit(kind: DraftKind, input: unknown, client: DraftClient): Submission {
        this.sweep();
        const now = this.now();
        const checked = validateDraft(kind, input);
        if (!checked.ok) { return { ok: false, code: checked.code, message: checked.errors.join('; ') }; }
        const draft = checked.draft;
        if (draft.expectedBinding && draft.expectedBinding !== this.context.binding()) {
            return { ok: false, code: 'STALE_BINDING', message: 'The Explorer window binding changed; call airk_explorer_ui_status again.' };
        }
        const bodyHash = canonical(input);
        const existing = this.keys.get(draft.idempotencyKey);
        if (existing) {
            const item = this.items.get(existing)!;
            if (item.kind === kind && item.bodyHash === bodyHash) { return { ok: true, receipt: this.receipt(item) }; }
            return { ok: false, code: 'PROPOSAL_CONFLICT', message: 'This idempotency key was already used with a different draft.' };
        }
        const target = kind === 'search' ? undefined : this.context.revisions()[kind === 'strategy' ? 'studio' : 'planner'];
        if (draft.expectedTargetRevision !== undefined && target !== undefined && draft.expectedTargetRevision !== target) {
            return { ok: false, code: 'PROPOSAL_CONFLICT', message: `The target draft is at revision ${target}, not ${draft.expectedTargetRevision}.` };
        }
        while (this.recent.length && this.recent[0] <= now - 60_000) { this.recent.shift(); }
        if (this.recent.length >= SUGGESTION_LIMITS.perMinute) { return { ok: false, code: 'RATE_LIMITED', message: `At most ${SUGGESTION_LIMITS.perMinute} drafts per minute.` }; }
        const open = [...this.items.values()].filter(item => item.status === 'pending_review' || item.status === 'deferred');
        const size = new TextEncoder().encode(bodyHash).length;
        if (open.length >= SUGGESTION_LIMITS.pending) { return { ok: false, code: 'RATE_LIMITED', message: `The Inbox already holds ${SUGGESTION_LIMITS.pending} open drafts; ask the user to review or discard them.` }; }
        if (open.reduce((total, item) => total + item.size, 0) + size > SUGGESTION_LIMITS.bytes) { return { ok: false, code: 'RATE_LIMITED', message: 'The Inbox is full; ask the user to review or discard drafts.' }; }
        this.recent.push(now);
        const warnings: string[] = [];
        const publication = this.context.publication();
        if (draft.evidence && publication && draft.evidence.publication !== publication) {
            warnings.push(`EVIDENCE_MISMATCH: the draft cites publication ${draft.evidence.publication.slice(0, 80)}, but the index is now ${publication.slice(0, 80)}.`);
        }
        if (draft.body.kind === 'search' && draft.body.search.mode !== 'lexical' && draft.body.search.mode !== 'policy') {
            warnings.push('Vector and hybrid modes depend on the project\'s vector settings; the Explorer reports the mode it applies.');
        }
        const item: Suggestion = { id: `ex-prop-${randomBytes(8).toString('hex')}`, kind, draft, client: { name: client.name.slice(0, 64) || 'unknown client', version: client.version.slice(0, 32) },
            receivedAt: now, expiresAt: now + this.context.ttlMs(), size, bodyHash, warnings, ...(target !== undefined ? { targetRevision: target } : {}), status: 'pending_review' };
        this.items.set(item.id, item); this.keys.set(draft.idempotencyKey, item.id);
        this.changed();
        return { ok: true, receipt: this.receipt(item) };
    }

    /** Status without the body, for airk_explorer_proposal_status. */
    status(id: string): Pick<ProposalReceipt, 'proposal_id' | 'status' | 'destination' | 'applied' | 'executed' | 'expires_at'> | undefined {
        this.sweep();
        const item = this.items.get(id);
        return item && { proposal_id: item.id, status: item.status, destination: destinationOf(item.draft.body), applied: item.status === 'applied', executed: false,
            expires_at: new Date(item.expiresAt).toISOString() };
    }

    list(): readonly Suggestion[] {
        this.sweep();
        return [...this.items.values()].sort((a, b) => b.receivedAt - a.receivedAt);
    }
    get(id: string): Suggestion | undefined { this.sweep(); return this.items.get(id); }

    /** Record the user's decision. Only open drafts change; expired drafts cannot be applied. */
    resolve(id: string, status: 'applied' | 'rejected' | 'deferred' | 'conflicted'): Suggestion | undefined {
        this.sweep();
        const item = this.items.get(id);
        if (!item || (item.status !== 'pending_review' && item.status !== 'deferred')) { return undefined; }
        item.status = status;
        this.changed();
        return item;
    }

    /** "Discard all agent suggestions for this workspace": open drafts become rejected. */
    discardAll(): number {
        let count = 0;
        for (const item of this.items.values()) { if (item.status === 'pending_review' || item.status === 'deferred') { item.status = 'rejected'; count++; } }
        if (count) { this.changed(); }
        return count;
    }

    openCount(): number { return this.list().filter(item => item.status === 'pending_review' || item.status === 'deferred').length; }
}

const VISIBLE_CARDS = 20;
function summary(item: Suggestion): string {
    const body = item.draft.body;
    if (body.kind === 'search') { return `"${body.search.query.slice(0, 120)}" · ${body.search.mode}${body.search.pathPrefix ? ` · ${body.search.pathPrefix}` : ''}`; }
    if (body.kind === 'strategy') { return `${body.strategy.id}@${body.strategy.version} · ${body.strategy.steps.length} steps`; }
    if (body.kind === 'plan') { return `${body.plan.tasks.length} task(s)${body.pathHint ? ` · suggested path ${body.pathHint}` : ''}`; }
    if (body.kind === 'review') { return `Review › ${body.scope}${body.names.length ? ` · ${body.names.slice(0, 5).join(', ')}` : ''}`; }
    return `Guidance: ${body.intent}`;
}

/**
 * Inbox cards: open drafts first, newest first. Open drafts with the same destination and title
 * collapse into one card with a count, and at most 20 cards are shown, so a looping agent cannot
 * bury other proposals.
 */
export function suggestionsView(items: readonly Suggestion[]): SuggestionsView {
    const open = items.filter(item => item.status === 'pending_review' || item.status === 'deferred');
    const groups = new Map<string, Suggestion[]>();
    for (const item of open) { const key = `${item.kind}\u0000${item.draft.title}`; groups.set(key, [...(groups.get(key) ?? []), item]); }
    const cards: SuggestionCard[] = [...groups.values()].map(group => card(group[0], group.length));
    for (const item of items.filter(entry => !open.includes(entry))) { cards.push(card(item, 1)); }
    return { open: open.length, cards: cards.slice(0, VISIBLE_CARDS), hidden: Math.max(0, cards.length - VISIBLE_CARDS) };
}
function card(item: Suggestion, duplicates: number): SuggestionCard {
    return { id: item.id, kind: item.kind, title: item.draft.title, client: item.client.name, receivedAt: item.receivedAt, expiresAt: item.expiresAt,
        status: item.status, destination: destinationOf(item.draft.body), warnings: item.warnings, duplicates, summary: summary(item) };
}
