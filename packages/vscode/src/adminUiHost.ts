// SPDX-License-Identifier: Apache-2.0
import { admitAdminUiRequest, parseAdminUiRequest, type AdminUiContext, type AdminUiObservation, type Operation } from './adminUiContract';
import { AirkctlSession, type AdminIdentity } from './airkctl/session';
import type { MutationPreparation, SuccessEnvelope } from './generated/i3';

type Pending = { operation: Operation; body: unknown; acknowledgment: { kind: 'ok'; status: number; envelope: SuccessEnvelope }; preparation: MutationPreparation };
const PREPARED = new Set<Operation>(['jobs.retry', 'catalog.policy_resume', 'catalog.split_recover', 'recovery.plan_create', 'service.stop', 'service.restart']);
/** Private host custody, deliberately absent from Memento and renderer state. */
export class AdminUiHost {
    private captured?: { identity: AdminIdentity; pending: Map<string, Pending> };
    constructor(private readonly session: AirkctlSession) {}
    close(): Promise<void> {
        const held = this.captured; this.captured = undefined;
        return held ? this.session.adminClose(held.identity) : Promise.resolve();
    }
    async request(input: unknown, context: () => AdminUiContext, onStatus?: (status: number) => void): Promise<unknown> {
        const request = parseAdminUiRequest(input);
        if (!request) throw Error('admin_ui_invalid_request');
        const refusal = admitAdminUiRequest(request, context());
        if (refusal) throw Error(refusal);
        if (request.type === 'admin.close') {
            await this.close();
            return { epoch: request.epoch, closed: true };
        }
        if (request.type === 'admin.session') {
            if (!this.session.hello) await this.session.control('ping');
            if (admitAdminUiRequest(request, context())) throw Error('admin_ui_context_changed');
            const identity = this.session.captureAdmin();
            if (this.captured && this.captured.identity !== identity) this.captured = undefined;
            this.captured ??= { identity, pending: new Map() };
            return { epoch: request.epoch, serviceInstance: identity.serviceInstance, pending: [...this.captured.pending.values()] };
        }
        const held = this.captured;
        if (!held || !this.session.adminCurrent(held.identity)) throw Error('admin_ui_context_changed');
        const requestBody = request.payload.body as Record<string, unknown> | null;
        if (PREPARED.has(request.operation)) {
            const handle = requestBody && typeof requestBody === 'object' ? requestBody.prepared ?? requestBody.recover ?? requestBody.acknowledged : undefined;
            if (handle !== undefined) {
                if (typeof handle !== 'string' || held.pending.get(handle)?.operation !== request.operation)
                    throw Error('admin_ui_original_preparation_required');
            } else if (held.pending.size >= 32) throw Error('admin_ui_preparation_capacity');
        }
        let observedStatus: number | null = null;
        try {
            const reply = await this.session.adminCall(held.identity, request.operation, request.payload.body,
                { deadlineMs: 120_000, admit: () => this.captured === held && admitAdminUiRequest(request, context()) === null, onStatus: status => {
                    observedStatus = status;
                    if (this.captured === held) onStatus?.(status);
                } });
            const envelope = JSON.parse(reply.body) as Record<string, unknown>;
            if (envelope.schema === 'airk-admin/1' && envelope.ok === true && envelope.operation === request.operation) {
                const result = envelope.result as Record<string, unknown> | null;
                if (result && typeof result.preparation === 'string' && /^[0-9a-f]{32}$/.test(result.preparation)
                    && (result.receipt_operation === null || typeof result.receipt_operation === 'string')) {
                    const preparation = result as MutationPreparation;
                    const previous = held.pending.get(preparation.preparation);
                    // READY recover retains the original immutable input, never {recover}.
                    if (!previous && held.pending.size < 32) held.pending.set(preparation.preparation, {
                        operation: request.operation, body: request.payload.body,
                        acknowledgment: { kind: 'ok', status: reply.httpStatus, envelope: envelope as unknown as SuccessEnvelope }, preparation,
                    });
                }
                if (result && typeof result.acknowledged === 'string' && requestBody?.acknowledged === result.acknowledged)
                    held.pending.delete(result.acknowledged);
            }
            return { epoch: request.epoch, current: context().epoch === request.epoch && this.captured === held, reply } satisfies AdminUiObservation;
        } catch {
            return { epoch: request.epoch, current: context().epoch === request.epoch && this.captured === held,
                reply: { transport: true, httpStatus: observedStatus, message: 'The original native Admin outcome was not received. Recover the original prepared action; do not submit a replacement.' } } satisfies AdminUiObservation;
        }
    }
}
