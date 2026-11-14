// SPDX-License-Identifier: Apache-2.0
// Pure local UI operation catalog and C9 management contract. Native bodies remain opaque.
// Every operation is validated against the Rust-owned I3 table by the Admin build.
export const OPS = {
  health: 'service.health',
  resources: 'service.resources',
  workers: 'service.workers',
  indexesList: 'indexes.list',
  indexesPause: 'indexes.pause',
  indexesProgress: 'indexes.progress',
  catalogPage: 'catalog.page',
  catalogDetail: 'catalog.detail',
  catalogPreview: 'catalog.preview',
  catalogApply: 'catalog.apply',
  catalogReceipt: 'catalog.receipt',
  catalogIndexSetupDraft: 'catalog.index_setup_draft',
  catalogIndexSetupApply: 'catalog.index_setup_apply',
  catalogIndexSetupReceipt: 'catalog.index_setup_receipt',
  catalogSourceDraft: 'catalog.source_draft',
  catalogSourceObservation: 'catalog.source_observation',
  catalogPendingSource: 'catalog.pending_source',
  catalogSourceResolve: 'catalog.source_resolve',
  catalogSplitSourceState: 'catalog.split_source_state',
  catalogSplitDraft: 'catalog.split_draft',
  catalogSplitApply: 'catalog.split_apply',
  catalogSplitStatus: 'catalog.split_status',
  catalogSplitReceipt: 'catalog.split_receipt',
  catalogSplitRecover: 'catalog.split_recover',
  catalogSplitRecoveryReceipt: 'catalog.split_recovery_receipt',
  catalogPolicyStatus: 'catalog.policy_status',
  catalogPolicyResume: 'catalog.policy_resume',
  catalogPolicyResumeReceipt: 'catalog.policy_resume_receipt',
  catalogRetirementEvidence: 'catalog.retirement_evidence',
  catalogGrantDraft: 'catalog.grant_draft',
  usageDocument: 'usage.document',
  eventsPage: 'events.page',
  jobsGet: 'jobs.get',
  jobsRetry: 'jobs.retry',
  jobsCancel: 'jobs.cancel',
  requestsList: 'requests.list',
  recoveryPlan: 'recovery.plan',
  recoveryPlanCreate: 'recovery.plan_create',
  recoveryPlanItems: 'recovery.plan_items',
  storageInventory: 'storage.inventory',
  filesBrowse: 'files.browse',
  storagePurgePlan: 'storage.purge_plan',
  storagePurge: 'storage.purge',
  loginStatus: 'service.login_startup_status',
  loginInstall: 'service.login_startup_install',
  loginUninstall: 'service.login_startup_uninstall',
  logsRecent: 'logs.recent',
  agentsEndpoint: 'agents.endpoint',
  agentsConnection: 'agents.connection',
  agentsOwnership: 'agents.ownership',
  agentsVerify: 'agents.verify',
  agentsRevoke: 'agents.revoke',
  trustStatus: 'https.trust_status',
  rotationStatus: 'https.rotation_status',
  serviceStop: 'service.stop',
  serviceRestart: 'service.restart',
  arbiterStatus: 'arbiter.status',
  arbiterConfigure: 'arbiter.configure',
  arbiterProvision: 'arbiter.provision',
  arbiterCancel: 'arbiter.cancel',
  arbiterRemove: 'arbiter.remove',
  arbiterRecover: 'arbiter.recover',
  toolsCatalog: 'tools.catalog',
  searchSelectScope: 'search.select_scope',
  searchCall: 'search.call',
} as const;

export type Operation = (typeof OPS)[keyof typeof OPS];

export const ADMIN_UI_VERSION = 1;
/** Existing native I3 body budget; the C9 payload wrapper adds nine bytes. */
export const ADMIN_UI_BODY_BYTES = 64 * 1024;
export const ADMIN_UI_PAYLOAD_BYTES = ADMIN_UI_BODY_BYTES + 9;
const OPERATIONS = new Set<string>(Object.values(OPS));
export type AdminUiCall = { readonly v: 1; readonly type: 'admin.call'; readonly requestId: string;
    readonly epoch: number; readonly operation: Operation; readonly payload: Readonly<{ body: unknown }> };
export type AdminUiClose = { readonly v: 1; readonly type: 'admin.close'; readonly requestId: string; readonly epoch: number };
export type AdminUiSession = { readonly v: 1; readonly type: 'admin.session'; readonly requestId: string; readonly epoch: number };
export type AdminUiRequest = AdminUiCall | AdminUiClose | AdminUiSession;
/** Assembled local transport, not a mirror of chunked I2 wire DTOs. */
export type AdminUiReply = Readonly<{ httpStatus: number; body: string }>;
export type AdminUiObservation = Readonly<{ epoch: number; current: boolean;
    reply: AdminUiReply | Readonly<{ transport: true; httpStatus: number | null; message: string }> }>;
/** This context comes from the private host, never from a renderer request. */
export type AdminUiContext = { epoch: number; ready: boolean; trusted: boolean; nativeEnabled: boolean; management: boolean };
export type AdminUiRefusal = 'admin_ui_untrusted' | 'admin_ui_native_disabled' | 'admin_ui_not_ready' | 'admin_ui_unsupported' | 'admin_ui_context_changed';
const object = (value: unknown): Record<string, unknown> | null => value !== null && typeof value === 'object'
    && !Array.isArray(value) && (Object.getPrototypeOf(value) === Object.prototype || Object.getPrototypeOf(value) === null) ? value as Record<string, unknown> : null;
const fields = (value: Record<string, unknown>, expected: readonly string[]): boolean => Object.keys(value).length === expected.length && expected.every(key => Object.hasOwn(value, key));

/** Copy JSON semantics without interpreting native fields or stripping unknown values. */
export function captureAdminBody(value: unknown): { value: unknown; bytes: number } | null {
    const pending = [value], seen = new WeakSet<object>();
    while (pending.length) {
        const next = pending.pop();
        if (next === null || typeof next === 'string' || typeof next === 'boolean') continue;
        if (typeof next === 'number') { if (!Number.isFinite(next)) return null; continue; }
        if (typeof next !== 'object') return null;
        if (seen.has(next)) continue;
        seen.add(next);
        if (!Array.isArray(next) && object(next) === null) return null;
        if (Array.isArray(next) && (Object.keys(next).length !== next.length
            || Object.keys(next).some(key => !/^(0|[1-9][0-9]*)$/.test(key) || Number(key) >= next.length))) return null;
        const descriptors = Object.getOwnPropertyDescriptors(next);
        for (const [key, descriptor] of Object.entries(descriptors)) {
            if (Array.isArray(next) && key === 'length') continue;
            if (descriptor.get || descriptor.set || !descriptor.enumerable) return null;
            pending.push(descriptor.value);
        }
        if (Object.getOwnPropertySymbols(next).length) return null;
    }
    try {
        const text = JSON.stringify(value), bytes = new TextEncoder().encode(text).length;
        if (bytes > ADMIN_UI_BODY_BYTES) return null;
        const copy: unknown = JSON.parse(text);
        const freeze = [copy];
        while (freeze.length) {
            const next = freeze.pop();
            if (next === null || typeof next !== 'object') continue;
            freeze.push(...Object.values(next)); Object.freeze(next);
        }
        return { value: copy, bytes };
    } catch { return null; }
}

export function parseAdminUiRequest(value: unknown): AdminUiRequest | null {
    const input = object(value);
    if (!input || input.v !== ADMIN_UI_VERSION || typeof input.requestId !== 'string' || !/^[a-zA-Z0-9_-]{1,64}$/.test(input.requestId)
        || typeof input.epoch !== 'number' || !Number.isSafeInteger(input.epoch) || input.epoch < 0) return null;
    if (input.type === 'admin.close' || input.type === 'admin.session') {
        return fields(input, ['v', 'type', 'requestId', 'epoch'])
            ? Object.freeze({ v: 1, type: input.type, requestId: input.requestId, epoch: input.epoch }) : null;
    }
    if (input.type !== 'admin.call' || !fields(input, ['v', 'type', 'requestId', 'epoch', 'operation', 'payload'])
        || typeof input.operation !== 'string' || !OPERATIONS.has(input.operation)) return null;
    const payload = object(input.payload);
    if (!payload || !fields(payload, ['body'])) return null;
    const body = captureAdminBody(payload.body);
    if (body === null) return null;
    return Object.freeze({ v: 1, type: 'admin.call', requestId: input.requestId, epoch: input.epoch,
        operation: input.operation as Operation, payload: Object.freeze({ body: body.value }) });
}

export function admitAdminUiRequest(request: AdminUiRequest, context: AdminUiContext): AdminUiRefusal | null {
    if (request.epoch !== context.epoch) return 'admin_ui_context_changed';
    // Revocation closes only an already captured owner; it never starts/reopens native work.
    if (request.type === 'admin.close') return null;
    if (!context.trusted) return 'admin_ui_untrusted';
    if (!context.nativeEnabled) return 'admin_ui_native_disabled';
    if (!context.ready) return 'admin_ui_not_ready';
    if (!context.management) return 'admin_ui_unsupported';
    return null;
}
