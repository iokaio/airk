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
// request key and it never reaches the browser.
export const OPS = {
  health: 'service.health',
  resources: 'service.resources',
  workers: 'service.workers',
  indexesList: 'indexes.list',
  indexesPause: 'indexes.pause',
  catalogPage: 'catalog.page',
  eventsPage: 'events.page',
  jobsGet: 'jobs.get',
  jobsRetry: 'jobs.retry',
  jobsCancel: 'jobs.cancel',
  requestsList: 'requests.list',
  recoveryPlan: 'recovery.plan',
  recoveryPlanItems: 'recovery.plan_items',
  storageInventory: 'storage.inventory',
  logsRecent: 'logs.recent',
  agentsConnection: 'agents.connection',
  agentsOwnership: 'agents.ownership',
  agentsRevoke: 'agents.revoke',
  trustStatus: 'https.trust_status',
  rotationStatus: 'https.rotation_status',
  serviceStop: 'service.stop',
  serviceRestart: 'service.restart',
  toolsCatalog: 'tools.catalog',
  searchSelectScope: 'search.select_scope',
  searchCall: 'search.call',
} as const;

export type Operation = (typeof OPS)[keyof typeof OPS];

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

export const body = {
  health: () => 'health',
  resources: () => 'resources',
  workers: (projects: string[]) => ({ workers: { projects } }),
  indexesList: (after: string | null, limit: number) => ({
    projects: { after, revision: null, limit },
  }),
  indexesPause: (project: string, paused: boolean) => ({ pause: { project, paused } }),
  /** CatalogPageRequest; limit ≤ MAX_SELECTION (32), max_bytes ≤ 48 KiB. */
  catalogIndexes: (expectedRevision: number | null) => ({
    catalog_page: {
      request: {
        kind: 'indexes',
        index_id: null,
        project_id: null,
        after: null,
        expected_catalog_revision: expectedRevision,
        limit: 32,
        max_bytes: 48 * 1024,
      },
    },
  }),
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
  logsRecent: (limit: number) => ({ logs: { limit } }),
  agentsConnection: (connectionId: string) => ({ agent_connection_info: { connection_id: connectionId } }),
  agentsOwnership: (connectionId: string) => ({
    agent_connection_ownership: { connection_id: connectionId },
  }),
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
