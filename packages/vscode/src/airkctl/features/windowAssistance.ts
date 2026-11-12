// SPDX-License-Identifier: Apache-2.0
//! Explorer Assistance drafts and handoffs over the window session (contracts
//! §2.5 W6–W8, §3.7). Agents submit drafts to this window through the service;
//! the session delivers them as `assistance_draft` events, which feed the
//! existing Suggestions Inbox (admission is not acceptance: nothing is applied
//! or run) and are acknowledged with `assistance_ack` (acknowledgement is the
//! back-pressure; the service keeps unacknowledged drafts). A handoff is one
//! reviewed snapshot published with `handoff_publish`; agents read it by its
//! ID until it expires. Each works only when its capability is advertised.
import { createHash } from 'node:crypto';
import type { EventTopic, SessionCursor, SessionOutcome } from '../../generated/i2';
import { DRAFT_KINDS, type DraftKind } from '../../explorerDrafts';
import type { ExplorerHandoff } from '../../explorerContracts';
import { HANDOFF_CONTENT_BYTES, HANDOFF_TTL_MS } from '../../explorerHandoffs';
import type { DraftClient, Submission } from '../../explorerSuggestions';
import type { ClientEvent } from '../session';

export const ASSISTANCE_CAPABILITY = 'assistance/1';
export const HANDOFF_CAPABILITY = 'handoff/1';
/** Shown in the Inbox; the submitter is reported by the service, never authenticated here. */
export const SERVICE_DRAFT_CLIENT: DraftClient = Object.freeze({ name: 'coding agent (AIRK Service)', version: '' });

export interface AssistanceSession {
    subscribe(topics: EventTopic[], cursor?: SessionCursor): Promise<SessionOutcome>;
    assistanceAck(through: number): Promise<SessionOutcome>;
}
export interface DraftSink { submit(kind: DraftKind, input: unknown, client: DraftClient): Submission }

const object = (value: unknown): Record<string, unknown> | undefined => value && typeof value === 'object' && !Array.isArray(value) ? value as Record<string, unknown> : undefined;

export class WindowAssistance {
    private subscribed = false;
    private subscribing?: Promise<void>;
    /** Highest sequence handled in this window (sequences restart with a new window). */
    private handled = 0;
    private acknowledged = 0;
    private acking: Promise<void> = Promise.resolve();

    constructor(private readonly session: AssistanceSession, private readonly capabilities: () => readonly string[] | undefined,
        private readonly sink: DraftSink, private readonly log: (line: string) => void = () => {}) {}

    available(): boolean { return this.capabilities()?.includes(ASSISTANCE_CAPABILITY) === true; }

    /** Subscribe to this window's drafts once per session child (only with `assistance/1`). */
    start(): Promise<void> {
        if (this.subscribed || !this.available()) { return Promise.resolve(); }
        this.subscribing ??= this.session.subscribe(['assistance']).then(outcome => {
            if (outcome.status === 'subscribed') { this.subscribed = true; return; }
            this.log(`AIRK: Explorer Assistance drafts are unavailable (${outcome.status === 'refused' ? outcome.code : outcome.status}).`);
        }, error => { this.log(`AIRK: Explorer Assistance subscription failed: ${error instanceof Error ? error.message : String(error)}`); })
            .finally(() => { this.subscribing = undefined; });
        return this.subscribing;
    }

    /** One session event. Returns the acknowledgement in flight (tests await it). */
    event(event: ClientEvent): Promise<void> {
        if (event.kind === 'session_lost') { this.subscribed = false; this.handled = this.acknowledged = 0; return this.acking; }
        // A restarted service re-opens the window: a new queue whose sequences start at 1.
        if (event.kind === 'resync' && event.reason === 'service_restarted') { this.handled = this.acknowledged = 0; return this.acking; }
        if (event.kind !== 'assistance_draft') { return this.acking; }
        const delivered = object(event.draft), sequence = delivered?.sequence;
        if (!delivered || !Number.isSafeInteger(sequence) || (sequence as number) < 1) { this.log('AIRK: ignored a malformed Explorer Assistance draft event.'); return this.acking; }
        // The session suppresses repeats; after an overflow it re-drains from the last acknowledgement.
        if ((sequence as number) <= this.handled) { return this.ack(sequence as number); }
        this.handled = sequence as number;
        const kind = delivered.kind;
        if (typeof kind !== 'string' || !(DRAFT_KINDS as readonly string[]).includes(kind)) {
            this.log(`AIRK: Explorer Assistance draft ${String(delivered.draft_id)} has an unsupported kind and was not shown.`);
        } else {
            const result = this.sink.submit(kind as DraftKind, delivered.draft, SERVICE_DRAFT_CLIENT);
            if (!result.ok) { this.log(`AIRK: Explorer Assistance draft ${String(delivered.draft_id)} was not admitted (${result.code}): ${result.message}`); }
        }
        // Admitted or refused, the draft is handled: acknowledge it so the queue moves.
        return this.ack(sequence as number);
    }

    private ack(through: number): Promise<void> {
        this.acking = this.acking.then(async () => {
            if (through <= this.acknowledged) { return; }
            try {
                const outcome = await this.session.assistanceAck(through);
                if (outcome.status === 'acknowledged') { this.acknowledged = through; }
                else { this.log(`AIRK: Explorer Assistance acknowledgement refused (${outcome.status === 'refused' ? outcome.code : outcome.status}).`); }
            } catch (error) { this.log(`AIRK: Explorer Assistance acknowledgement failed: ${error instanceof Error ? error.message : String(error)}`); }
        });
        return this.acking;
    }
}

export interface HandoffSession { handoffPublish(handoff: unknown, ttlMs: number): Promise<SessionOutcome> }
export class HandoffRefused extends Error {
    constructor(readonly code: string, message: string) { super(message); this.name = 'HandoffRefused'; }
}

/** Publishes one reviewed snapshot (W8); the token an agent uses is the service's `handoff_id`. */
export class WindowHandoffs {
    constructor(private readonly session: HandoffSession, private readonly binding: () => string) {}
    async share(kind: 'studio' | 'planner', revision: number, content: string, publication?: string): Promise<ExplorerHandoff> {
        if (!content.trim() || Buffer.byteLength(content) > HANDOFF_CONTENT_BYTES || /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(content)) {
            throw new HandoffRefused('handoff_content_invalid', `A handoff must contain complete text of at most ${HANDOFF_CONTENT_BYTES} UTF-8 bytes.`);
        }
        const content_sha256 = createHash('sha256').update(content).digest('hex');
        // No workspace path or window binding leaves the editor: only the reviewed text and its identity.
        const handoff = { kind, draft_revision: revision, content, content_sha256, ...(publication ? { evidence_publication: publication } : {}) };
        const outcome = await this.session.handoffPublish(handoff, HANDOFF_TTL_MS);
        if (outcome.status === 'refused') { throw new HandoffRefused(outcome.code, `The AIRK Service refused the handoff (${outcome.code}).`); }
        if (outcome.status !== 'handoff') { throw new HandoffRefused('session_invalid_response', 'The AIRK session answered the handoff unexpectedly.'); }
        return { token: outcome.handoff_id, kind, draft_revision: revision, workspace_binding: this.binding(), ...(publication ? { evidence_publication: publication } : {}),
            content, content_sha256, expires_unix_ms: outcome.expires_at_ms, executed: false };
    }
}
