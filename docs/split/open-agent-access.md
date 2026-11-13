# Open agent access: specification for U105/U106

Status: written 13 November 2026 from a read-only source trace of `feat/split`
(after `6fc3d356`). Decisions: split-plan §2.1 U104 (items 3-5), U105, U106.
Line citations are from that trace; verify them before editing.

## Goal

- An **open** agent connection: the bearer admits the agent to the MCP endpoint
  without a per-connection reviewed grant. Existing grant-bound connections keep
  working unchanged.
- Each open connection has a service-stored **default scope** (indexes and index
  sets) and an **agent choice** flag, `off` by default. The Explorer's applied
  search scope and `airkctl agents default` set them; a change takes effect on
  the next call.
- Choice `off`: every call reads exactly the default scope. A call naming
  indexes or sets is refused `agent_scope_choice_disabled`; the list tool is not
  in `tools/list` and refuses if called.
- Choice `on`: read tools accept `index_ids` and `index_set_ids` (ids or exact
  names, union, at most 32 each), and `list_indexes` lists every index and set
  with id, name, description and indexed paths (a set lists its member indexes).
- `airkctl tools call --index/--set/--connection` reaches the same service path
  (parity with MCP).
- Indexes and sets gain an optional, user-editable description. An absent
  description is generated at list time from the name and source paths.
- A named index reads all of its enabled sources (Subtree atoms with an empty
  path); a set expands to its member indexes, read whole.
- An unset default scope with choice off refuses `agent_default_scope_unset`.

## 1. Wire and core types

- `HttpSelection` (`airk-core/src/service_http.rs` ~144): add
  `Open { selection: OpenHttpSelection { namespace } }`. Extend exhaustive
  matches: `validate`/`namespace`, `same_authority`
  (`service_http_credentials.rs` ~223, Open equals Open), `authority_digest`
  (`airk-endpoint/src/http_endpoint/store.rs` ~822), gateway `capture`
  (`airk-https/src/http_gateway.rs` ~978). The TypeScript validator
  (`packages/vscode/src/serviceHttp.ts`) accepts `kind:'open'`; add a golden
  vector to `packages/vscode/src/test/fixtures/httpAuthorityDigest.json`.
- `ToolSelection` (`airk-core/src/service.rs` ~975): add
  `Open { index_ids: Vec<String>, index_set_ids: Vec<String>, connection_id: Option<Incarnation> }`.
- `SelectionOwnerKind` (`airk-core/src/scoped_reads.rs` ~104): add `Agent`.
  `ScopeObservation::validate` allows empty memberships only for `Agent`.
- `ScopedReadRef`: no shape change; an open read carries
  `selection: Some(SelectionScopeRef { owner: Agent, … })`.
- Kernel: sibling `OpenReadAuthority { schema: 1, backing, index_id, selection,
  sources, source_revision, policy_epoch, policy_digest }`, domain
  `airk/scoped-read-authority/open/1`, in the kernel `Authority` enum and the
  host `InstallScope` carrier (`airk-host/src/project_protocol.rs`).
- New refusal codes: `agent_scope_choice_disabled`, `agent_default_scope_unset`,
  `scope_name_ambiguous`, `scope_name_unknown`.

## 2. Persisted connection records

- `credentials.json` `Ledger`/`Record` (`service_http_credentials.rs` ~65-189):
  `#[serde(default, skip_serializing_if = "Option::is_none")] open:
  Option<OpenConnectionState { defaults_revision, default_index_ids,
  default_index_set_ids, agent_choice }>`, present exactly when the selection is
  `Open`. Defaults changes never bump `generation`.
- Before any change, capture the current ledger bytes into
  `fixtures/split/agents/credentials_ledger_v1.json` and prove parse →
  serialize is byte-identical before and after.
- A predecessor refuses a ledger with an open record (fail closed, U106).

## 3. Descriptions

- `IndexDefinition` and `QueryProject` (`airk-core/src/index_catalog.rs`
  ~239/260): `description: Option<String>`, `default` +
  `skip_serializing_if`, so every digest input is unchanged when absent.
- Store: a lazily installed `catalog_descriptions(kind, id, description)` table
  under a new catalog layout step (pattern of the policy 1-3 lazy layouts,
  `airk-store/src/service/registry/catalog.rs`), installed by the first
  description write only; recognized layouts include it; untouched
  installations keep their identity. Proof against `fixtures/schema/airk-state-1/*`
  plus one new fixture with the table.
- Operations `SetIndexDescription { id, expected_definition_revision,
  description: Option<String> }` and `SetProjectDescription { … }`; bound 1 KiB,
  newlines allowed, no other control characters; no typed confirmation.

## 4. Read authority

- `build_open_read_authority` beside `build_selection_read_authority`
  (`airk-catalog-control/src/catalog_authority.rs` ~220-385): exact current
  revision, live, import complete, policy ready; one Subtree `""` atom per
  enabled source of the index.
- Single index: `tools_call` selection path → `project_client::selection_tools`.
  Several: `combined_frontend` gains an `open` member basis with no projects and
  its own snapshot/cursor domain (`QuerySelection::validate` currently requires
  projects).
- Nonces: own budget (`MAX_AGENT_SELECTIONS`), leased per (connection,
  resolved scope, catalog revision), revoked on idle (300 s), defaults change,
  connection revoke or revision move.

## 5. MCP and CLI surface

- `airk-mcp`: `open_catalog(choice)` — off: public tools without selectors and
  without `list_indexes`; on: `index_ids`/`index_set_ids` (id-or-name strings,
  ≤ 32) and `list_indexes`. Declare availability once in the tool registry
  (one handler, one authorization rule, one conformance entry).
- `list_indexes` body: a pure builder in `airk-core` over catalog pages
  (indexes, projects, memberships, sources), paginated under `MAX_PAGE_BYTES`.
- Name resolution: `idx_`/`prj_` ids, else exact name; ambiguous or unknown
  names refuse.
- `airkctl tools call --index/--set/--connection` → `ToolSelection::Open`;
  session value options; help catalog and `docs/cli-reference.md`.

## 6. Defaults path

- `ControlMethod::AgentConnectionDefaults { connection_id,
  expected_defaults_revision, index_ids?, index_set_ids?, agent_choice? }` →
  `ControlResponse::AgentConnectionDefaults`; I3 operation-table row with the
  `session` surface so the Explorer can relay it; `AgentConnectionInfo`
  reports defaults and choice; `HttpEnroll` accepts `Open` (choice off, empty
  default).
- `airkctl agents default --connection-id … [--index …] [--set …]
  [--agent-choice off|on]`; `airkctl agents connect --open` enrolls a new open
  connection and writes the entry.

## 7. HTTPS admission

- For `Open`: catalog ready and namespace match, no grant, no registration.
- `airk-https` cannot depend on `airk-service`: inject an `OpenReader` hook
  (pattern of `admin_selection_reader`). `OpenBackend: RemoteTools` reads the
  record's choice for `catalog()` and its defaults per `call()`, resolves and
  refuses per the choice rules, then calls the reader.
- Choice changes emit `notifications/tools/list_changed` where supported.

## Steps

1. Contract (types, kernel authority, carrier, TS validator and vector,
   fixtures, ledger golden). No behaviour.
2. Store (layout, description operations, load).
3. Authority and admission (single index).
4. Combined open basis.
5. Service and CLI (`ToolSelection::Open`, defaults method, `agents default`,
   `agents connect --open`, `tools call` flags, `list_indexes` body).
6. HTTPS and MCP (`OpenBackend`, `open_catalog`, enrollment).
7. Explorer (Agents screen: connect, default scope follows Apply, choice
   switch; description editing) and docs.

Risks: the combined path is project-centric (step 4 is the largest unknown);
MCP clients cache `tools/list`; description edits bump the catalog revision,
so window and admin scopes re-select.
