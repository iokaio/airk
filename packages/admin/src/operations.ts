// SPDX-License-Identifier: Apache-2.0
// The admin operations this shell calls and their request bodies.
//
// Every id below must be a row of fixtures/split/i3/admin_operations.json whose
// `surfaces` include `admin`; build.mjs refuses the build otherwise.
//
// Bodies are the externally tagged ControlMethod JSON of the row's `method`
// (crates/airk-core/src/service.rs, snake_case variants, deny_unknown_fields;
// fixtures/split/i1/control_methods.json). Journal fields (`caller`, `key`) are
// omitted: contracts §4.1 and RequestProgress say the admin handler generates the
// request key and it never reaches the browser. Rows with a `request_key` rule
// (contracts §4.3b, U98) omit exactly the fields at the rule's paths: a
// "generate" row is sent as the bare method and answers a `review` token; a
// "review" row is sent as a ReviewedCall (api.ts `reviewedCall`) naming it.
export { OPS, type Operation } from '../../vscode/src/adminUiContract';

const HEX64 = /^[0-9a-f]{64}$/;
const HEX32 = /^[0-9a-f]{32}$/;

/** ProjectBinding and Sha256Digest: 64 lowercase hex. */
export function isDigest(value: string): boolean {
  return HEX64.test(value);
}

/** Incarnation (connection ids, namespaces): 32 lowercase hex. */
export function isIncarnation(value: string): boolean {
  return HEX32.test(value);
}

export const MAX_PAGE_LIMIT = 500;
/** `indexes.list` pages (`ServiceRegistry::list_role`: 1..=32). */
export const MAX_INDEX_LIST_LIMIT = 32;

/** Catalog ids (`index_catalog.rs` catalog_id!): a prefix and 32 lowercase hex. */
export type CatalogPrefix = 'prj_' | 'idx_' | 'src_' | 'ctx_' | 'grt_' | 'deny_';

export function isCatalogId(prefix: CatalogPrefix, value: string): boolean {
  return value.startsWith(prefix) && HEX32.test(value.slice(prefix.length));
}

/** A fresh `prj_` id; the CLI allocates CreateProject ids on the client too. */
export function newProjectId(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return `prj_${[...bytes].map((byte) => byte.toString(16).padStart(2, '0')).join('')}`;
}

/** CatalogKind (catalog_management.rs). */
export type CatalogKind = 'projects' | 'indexes' | 'memberships' | 'contexts' | 'grants' | 'sources';

/** CatalogPageRequest budgets: limit ≤ MAX_SELECTION (32), max_bytes ≤ MAX_PAGE_BYTES. */
export const CATALOG_PAGE_LIMIT = 32;
export const CATALOG_PAGE_BYTES = 48 * 1024;

/** CatalogEntity (internally tagged by `kind`). */
export type CatalogEntity =
  | { kind: 'project'; id: string }
  | { kind: 'index'; id: string }
  | { kind: 'membership'; project_id: string; index_id: string }
  | { kind: 'grant'; id: string }
  | { kind: 'source'; id: string };

/** CatalogRequest without `caller`/`key` (the catalog.preview/apply rule paths). */
export type CatalogRequestBody = {
  expected_catalog_revision: number;
  operation: Record<string, unknown> & { operation: string };
};

/** ScopeAtom. */
export type ScopeAtom = { source_root_id: string; kind: 'exact' | 'subtree'; path: string };

/** IndexSetupSelection without `key` (catalog.index_setup_draft rule `selection.key`). */
export type IndexSetupSelectionBody = {
  expected_catalog_revision: number;
  name: string;
  primary_root: string;
  data_root: string;
  collection: string;
  additional_roots: { alias: string; path: string }[];
  setup_watch: boolean;
};

/** SourceMutationSelection (tag `operation`). */
export type SourceMutationSelection =
  | { operation: 'add_root'; alias: string; path: string }
  | { operation: 'remove_root'; alias: string }
  | { operation: 'trim_path'; source_root_id: string; kind: 'exact' | 'subtree'; path: string }
  | { operation: 'unblock_path'; rule_id: string }
  | { operation: 'retire_index' };

/** SplitDraftSelection without `key` (catalog.split_draft rule `selection.key`). */
export type SplitDraftSelectionBody = {
  expected_catalog_revision: number;
  index_id: string;
  selectors: ScopeAtom[];
  destination_name: string;
  destination_data_root: string;
  destination_collection: string;
  projects: unknown[];
  capacity: {
    destination_growth: number;
    destination_temporary: number;
    original_rewrite: number;
    cleanup_allowance: number;
  };
  require_semantic_ready: boolean;
  destination_watch_enabled: boolean;
};

/** SplitRecoveryRequest without `caller`/`key` (catalog.split_recover rule). */
export type SplitRecoveryBody = {
  operation_id: string;
  request_digest: string;
  expected_receipt_digest: string;
  expected_finalization_revision?: number;
  action: 'resume' | 'cancel';
};

/** UsageDocumentRead (externally tagged; `policy` is a unit variant). */
export type UsageRead =
  | { settings: { view: string } }
  | { report: { days: number; format: 'json' | 'csv' | 'markdown' } }
  | 'policy';

/**
 * CatalogPageRequest. `sources` needs exactly `index_id`; `memberships` takes
 * either filter; other kinds take none. A continuation repeats its revision.
 */
function catalogPage(
  kind: CatalogKind,
  filter: { indexId?: string | null; projectId?: string | null },
  after: unknown,
  expectedRevision: number | null,
) {
  return {
    catalog_page: {
      request: {
        kind,
        index_id: filter.indexId ?? null,
        project_id: filter.projectId ?? null,
        after: after ?? null,
        expected_catalog_revision: expectedRevision,
        limit: CATALOG_PAGE_LIMIT,
        max_bytes: CATALOG_PAGE_BYTES,
      },
    },
  };
}

export const body = {
  health: () => 'health',
  resources: () => 'resources',
  workers: (projects: string[]) => ({ workers: { projects } }),
  /** Registered indexes, 1..=32 per page; a continuation names the page's `revision`. */
  indexesList: (after: string | null, limit: number, revision: number | null = null) => ({
    projects: { after, revision, limit },
  }),
  indexesPause: (project: string, paused: boolean) => ({ pause: { project, paused } }),
  /** U103: live progress of one registered index (ControlMethod Progress). */
  indexesProgress: (project: string) => ({ progress: { project } }),
  /** CatalogPageRequest; limit ≤ MAX_SELECTION (32), max_bytes ≤ 48 KiB. */
  catalogIndexes: (expectedRevision: number | null) => catalogPage('indexes', {}, null, expectedRevision),
  catalogPage,
  catalogDetail: (entity: CatalogEntity, expectedRevision: number | null) => ({
    catalog_detail: { entity, expected_catalog_revision: expectedRevision },
  }),
  /** Generate row: the service fills `request.caller`/`request.key`. */
  catalogPreview: (request: CatalogRequestBody) => ({ catalog_preview: { request } }),
  /** Review row method; send inside `reviewedCall` with the preview's or draft's token. */
  catalogApply: (request: unknown, previewDigest: string) => ({
    catalog_apply: { request, preview_digest: previewDigest },
  }),
  catalogReceipt: () => ({ catalog_receipt: {} }),
  indexSetupDraft: (selection: IndexSetupSelectionBody) => ({ catalog_index_setup_draft: { selection } }),
  /** `draft` exactly as catalog.index_setup_draft returned it (identity removed). */
  indexSetupApply: (draft: unknown) => ({ catalog_index_setup_apply: { draft } }),
  indexSetupReceipt: () => ({ catalog_index_setup_receipt: {} }),
  sourceDraft: (expectedRevision: number, indexId: string, change: SourceMutationSelection) => ({
    catalog_source_mutation_draft: {
      selection: { expected_catalog_revision: expectedRevision, index_id: indexId, change },
    },
  }),
  sourceObservation: (beginOperation: string) => ({
    catalog_source_change_observation: { begin_operation: beginOperation },
  }),
  pendingSource: (indexId: string) => ({ catalog_pending_source_change: { index_id: indexId } }),
  sourceResolve: (beginOperation: string) => ({
    catalog_source_change_resolve: { begin_operation: beginOperation },
  }),
  splitSourceState: (indexId: string, expectedRevision: number) => ({
    catalog_split_source_state: { index_id: indexId, expected_catalog_revision: expectedRevision },
  }),
  splitDraft: (selection: SplitDraftSelectionBody) => ({ catalog_split_draft: { selection } }),
  /** `draft` exactly as catalog.split_draft returned it (identity removed). */
  splitApply: (draft: unknown) => ({ catalog_split_apply: { draft } }),
  splitStatus: (operationId: string) => ({ catalog_split_status: { operation_id: operationId } }),
  splitReceipt: () => ({ catalog_split_receipt: {} }),
  splitRecover: (request: SplitRecoveryBody) => ({ catalog_split_recover: { request } }),
  splitRecoveryReceipt: (operationId: string) => ({
    catalog_split_recovery_receipt: { operation_id: operationId },
  }),
  policyStatus: (indexId: string) => ({ catalog_policy_status: { index_id: indexId } }),
  /** Generate row: the service fills `caller`/`key`. */
  policyResume: (indexId: string, operationId: string) => ({
    catalog_policy_resume: { index_id: indexId, operation_id: operationId },
  }),
  policyResumeReceipt: (indexId: string, operationId: string) => ({
    catalog_policy_resume_receipt: { index_id: indexId, operation_id: operationId },
  }),
  retirementEvidence: (indexId: string) => ({ catalog_index_retirement_evidence: { index_id: indexId } }),
  grantDraft: (contextKey: string, expectedRevision: number, projectIds: string[], indexIds: string[] | null) => ({
    catalog_grant_draft: {
      context_key: contextKey,
      selection: { expected_catalog_revision: expectedRevision, project_ids: projectIds, index_ids: indexIds },
    },
  }),
  usageDocument: (project: string, read: UsageRead) => ({ usage_document: { project, read } }),
  eventsPage: (project: string | null, cursor: unknown, limit: number) => ({
    events: { project, cursor: cursor ?? null, limit },
  }),
  jobsGet: (id: string) => ({ job: { id } }),
  jobsRetry: (id: string) => ({ retry: { id } }),
  jobsCancel: (id: string) => ({ cancel: { id } }),
  requestsList: () => ({ client_request_list: {} }),
  recoveryPlan: (id: string) => ({ plan: { id } }),
  recoveryPlanItems: (id: string, after: string | null, limit: number) => ({
    plan_items: { id, after, limit },
  }),
  storageInventory: () => ({ storage_inventory: {} }),
  /** M24: the folders in `path` (the starting list without one). */
  folderBrowse: (path: string | null, showHidden: boolean, after: string | null) => {
    const request: { path?: string; show_hidden?: boolean; after?: string } = {};
    if (path !== null) request.path = path;
    if (showHidden) request.show_hidden = true;
    if (after !== null) request.after = after;
    return { folder_browse: request };
  },
  /** U100: only the cache and closed logs are purgeable. */
  storagePurgePlan: (kind: 'cache' | 'logs') => ({ storage_purge_plan: { kind } }),
  storagePurge: (kind: 'cache' | 'logs', planDigest: string) => ({
    storage_purge: { kind, plan_digest: planDigest },
  }),
  /** U101: Windows login startup. */
  loginStartup: (action: 'status' | 'install' | 'uninstall') => ({ login_startup: { action } }),
  logsRecent: (limit: number) => ({ logs: { limit } }),
  /** M15a: the checked local HTTPS endpoint (AgentEndpoint {}). */
  agentsEndpoint: () => ({ agent_endpoint: {} }),
  agentsConnection: (connectionId: string) => ({ agent_connection_info: { connection_id: connectionId } }),
  agentsOwnership: (connectionId: string) => ({
    agent_connection_ownership: { connection_id: connectionId },
  }),
  /** V1: probe /mcp with the stored credential; the token never leaves the service. */
  agentsVerify: (connectionId: string) => ({ agent_connection_verify: { connection_id: connectionId } }),
  agentsRevoke: (connectionId: string) => ({ http_revoke: { connection_id: connectionId } }),
  trustStatus: () => ({ http_trust: { action: 'inspect' } }),
  rotationStatus: () => ({ http_issuer_rotation: { action: 'status' } }),
  serviceStop: () => ({ lifecycle: { action: 'stop' } }),
  serviceRestart: () => ({ lifecycle: { action: 'restart' } }),
  toolsList: () => ({ tools_catalog: { detail: { list: { schemas: false } } } }),
  toolsHelp: (tool: string) => ({ tools_catalog: { detail: { help: { tool } } } }),
  /** W4-style selection; the grant stays in the service, bound to the session. */
  searchSelectScope: (
    namespace: string,
    expectedCatalogRevision: number,
    indexIds: string[],
    projectIds: string[],
  ) => ({
    admin_scope_grant: {
      selection: {
        namespace,
        expected_catalog_revision: expectedCatalogRevision,
        index_ids: indexIds,
        project_ids: projectIds,
      },
    },
  }),
  /** M3 over the admin session's grant (`Admin{..}` selection, contracts §2.2 M3). */
  searchCall: (tool: string, args: Record<string, unknown>, responseMode: 'terse' | 'full') => ({
    tools_call: { tool, arguments: args, selection: 'admin', response_mode: responseMode, usage_client: 'unknown' },
  }),
  /** The help tool needs no selection; `Unselected` serves only it. */
  helpTopic: (topic: string) => ({
    tools_call: {
      tool: 'help',
      arguments: topic.length > 0 ? { topic } : {},
      selection: 'unselected',
      response_mode: 'terse',
      usage_client: 'unknown',
    },
  }),
};
