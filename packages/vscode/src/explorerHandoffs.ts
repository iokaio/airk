// SPDX-License-Identifier: Apache-2.0
// Explicitly shared, volatile Explorer exports. Tokens never grant general draft access.
import { createHash, randomBytes } from 'node:crypto';
import type { ExplorerHandoff } from './explorerContracts';

export const HANDOFF_TTL_MS = 10 * 60_000;
export const HANDOFF_CONTENT_BYTES = 24576;
export interface HandoffContext {
    authorized(): boolean;
    binding(): string;
    revisions(): { studio?: number; planner?: number };
    publication(): string | undefined;
    now?(): number;
}
export class HandoffStore {
    private readonly entries = new Map<string, ExplorerHandoff>();
    constructor(private readonly context: HandoffContext) {}
    private now(): number { return this.context.now?.() ?? Date.now(); }
    private sweep(): void { for (const [token, value] of this.entries) { if (!this.current(value)) { this.entries.delete(token); } } }
    private current(value: ExplorerHandoff): boolean {
        return this.context.authorized() && value.workspace_binding === this.context.binding() && value.expires_unix_ms > this.now()
            && this.context.revisions()[value.kind] === value.draft_revision
            && (!value.evidence_publication || value.evidence_publication === this.context.publication());
    }
    share(kind: 'studio' | 'planner', revision: number, content: string, publication?: string): ExplorerHandoff {
        this.sweep();
        if (!this.context.authorized() || this.context.revisions()[kind] !== revision || !this.context.binding()
            || publication !== undefined && publication !== this.context.publication()) { throw new Error('The draft, authorization or evidence changed; review it again before sharing.'); }
        if (!content.trim() || Buffer.byteLength(content) > HANDOFF_CONTENT_BYTES || /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(content)) { throw new Error(`A handoff must contain complete text of at most ${HANDOFF_CONTENT_BYTES} UTF-8 bytes.`); }
        const value: ExplorerHandoff = { token: `ex-share-${randomBytes(32).toString('hex')}`, kind, draft_revision: revision, workspace_binding: this.context.binding(),
            ...(publication ? { evidence_publication: publication } : {}), content, content_sha256: createHash('sha256').update(content).digest('hex'),
            expires_unix_ms: this.now() + HANDOFF_TTL_MS, executed: false };
        const bytes = Buffer.byteLength(JSON.stringify(value));
        if (bytes > 49152 || this.entries.size >= 16 || [...this.entries.values()].reduce((total, item) => total + Buffer.byteLength(JSON.stringify(item)), bytes) > 262144) { throw new Error('The shared handoff budget is full. Wait for existing tokens to expire or reduce the text explicitly.'); }
        this.entries.set(value.token, value); return value;
    }
    get(token: string): ExplorerHandoff | undefined { this.sweep(); return this.entries.get(token); }
    clear(): void { this.entries.clear(); }
}
