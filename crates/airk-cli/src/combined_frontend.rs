// SPDX-License-Identifier: Apache-2.0
//! Grant-bound combined reads. Native children retain their ordinary scoped IPC,
//! admission, revision, retry and delivery checks. This owner never indexes.
use airk_core::{
    catalog_management::{CatalogEntity, CatalogItem, CatalogSource},
    combined_reads::{MemberAvailability, QuerySelection, ResolvedSelection, SharedBudgets},
    grant_provenance::GrantProvenanceKind,
    identity::{Sha256Digest, framed_digest},
    index_catalog::{CatalogRevision, IndexDefinition, IndexId, QueryProject},
    scoped_reads::ScopeReadRefused,
    service::{CombinedConnectRequest, ScopedConnectRequest},
};
use airk_kernel::scoped_reads::ScopedReadAuthority;
use airk_mcp::remote::{RemoteFuture, RemoteTools};
use airk_store::service::registry::ServiceRegistry;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[path = "combined_frontend_cursor.rs"]
mod cursor;

// A canonical body is bounded separately from its public MCP presentation.
// structuredContent retains N bytes; quoting the identical JSON in content
// adds at most 2N bytes. Native child IPC and stdio input bounds stay unchanged.
// The existing HTTP/MCP public envelope ceiling remains independent of the
// selected canonical read/plan output budget (see airk_mcp::remote).
const MAX_PUBLIC_TOOL_ENVELOPE_BYTES: usize = 1024 * 1024;
const PUBLIC_TRANSPORT_RESERVE: usize = 2048;
const DELIVERY_OBSERVATION_RESERVE: usize = 1024;

#[derive(Clone, Copy)]
enum ReplyBudget {
    Read,
    Plan { max_bytes: usize },
}
impl ReplyBudget {
    fn body_bytes(self) -> Result<usize> {
        match self {
            Self::Read => Ok(airk_core::MAX_RESPONSE_BYTES),
            Self::Plan { max_bytes } => {
                // The canonical Workbench validator owns this existing range.
                ensure!(
                    (1024..=524288).contains(&max_bytes),
                    "invalid plan output budget"
                );
                Ok(max_bytes)
            }
        }
    }
}
struct ReadAdmission {
    remaining_deadline_ms: Option<u64>,
    capacity: usize,
    context: airk_mcp::read_attempt::ReadAttemptContext,
}

fn check_public_reply(result: &Value) -> Result<()> {
    check_public_reply_budget(result, ReplyBudget::Read)
}

fn check_public_reply_budget(result: &Value, budget: ReplyBudget) -> Result<()> {
    let maximum = budget.body_bytes()?;
    // Check actual serialized data before and after the typed delivery receipt.
    // A guessed receipt reservation must not reject a complete fitting reply.
    airk_core::index_catalog::encoded_size(&canonical_body(result), maximum)
        .map_err(|_| airk_kernel::combined_reads::CombinedReplyBudget)?;
    airk_core::index_catalog::encoded_size(
        result,
        (3 * maximum + 4096).min(MAX_PUBLIC_TOOL_ENVELOPE_BYTES - PUBLIC_TRANSPORT_RESERVE),
    )
    .map_err(|_| airk_kernel::combined_reads::CombinedReplyBudget)?;
    Ok(())
}

#[derive(Clone)]
struct Member {
    index: IndexDefinition,
    authority: ScopedReadAuthority,
    sources: Vec<CatalogSource>,
    selection: ScopedConnectRequest,
}

#[derive(Clone)]
struct Snapshot {
    revision: CatalogRevision,
    resolved: ResolvedSelection,
    projects: Vec<QueryProject>,
    members: Vec<Member>,
    identity: Sha256Digest,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |time| time.as_millis().min(u64::MAX as u128) as u64)
}

fn read_snapshot(
    root: &Path,
    session: &CombinedConnectRequest,
    selection: &QuerySelection,
) -> Result<Snapshot> {
    session.validate()?;
    selection.validate()?;
    ensure!(
        selection
            .project_ids
            .iter()
            .all(|id| session.project_ids.contains(id)),
        ScopeReadRefused
    );
    let mut registry =
        ServiceRegistry::open_saved_before(root, Some(Instant::now() + Duration::from_secs(2)))?;
    ensure!(
        registry.catalog_pending_fence()?.is_none(),
        ScopeReadRefused
    );
    let state = registry.catalog_state()?.ok_or(ScopeReadRefused)?;
    ensure!(
        state.activated
            && state.authority_contract == 1
            && state.policy_contract
                <= airk_store::service::registry::catalog::SUPPORTED_POLICY_CONTRACT
            && state.namespace == session.namespace
            && state.revision >= session.expected_catalog_revision,
        ScopeReadRefused
    );
    let context = crate::catalog_host_context::NativeHostContext::load(
        &mut registry,
        session.context_key,
        Some(state.revision),
    )?;
    let host = context.descriptor().host;
    ensure!(
        context.descriptor().namespace == session.namespace,
        ScopeReadRefused
    );
    let detail = registry.catalog_detail(
        &CatalogEntity::Grant {
            id: session.grant_id,
        },
        Some(state.revision),
    )?;
    let Some(CatalogItem::Grant(persisted)) = detail.item else {
        return Err(ScopeReadRefused.into());
    };
    persisted.validate()?;
    // An imported legacy snapshot never silently acquires the new Project API.
    ensure!(
        persisted.provenance.kind() == GrantProvenanceKind::UserReviewedProjectGrant
            && persisted.grant.host == host
            && persisted.grant.id == session.grant_id
            && persisted.grant.revision == session.expected_grant_revision
            && !persisted.grant.revoked,
        ScopeReadRefused
    );
    let frozen = session.selection().resolve(&persisted.grant)?;
    let resolved = selection.resolve(&persisted.grant)?;
    ensure!(
        resolved
            .members
            .iter()
            .all(|member| frozen.member(member.index_id).is_some()),
        ScopeReadRefused
    );
    let mut projects = Vec::new();
    for id in &resolved.project_ids {
        let Some(CatalogItem::Project(project)) = registry
            .catalog_detail(&CatalogEntity::Project { id: *id }, Some(state.revision))?
            .item
        else {
            return Err(ScopeReadRefused.into());
        };
        project.validate()?;
        projects.push(project);
    }
    let mut members = Vec::new();
    for member in &resolved.members {
        let child = ScopedConnectRequest {
            schema: 1,
            context_key: session.context_key,
            namespace: session.namespace,
            expected_catalog_revision: state.revision,
            grant_id: session.grant_id,
            expected_grant_revision: session.expected_grant_revision,
            index_id: member.index_id,
            project_ids: member.project_ids(),
        };
        let native = crate::catalog_coordinator::native_scoped_selection(&mut registry, &child)?;
        let authority =
            crate::catalog_authority::build_catalog_read_authority(&mut registry, &native)?;
        let Some(CatalogItem::Index(index)) = registry
            .catalog_detail(
                &CatalogEntity::Index {
                    id: member.index_id,
                },
                Some(state.revision),
            )?
            .item
        else {
            return Err(ScopeReadRefused.into());
        };
        let mut sources = Vec::new();
        for descriptor in &authority.sources {
            let Some(CatalogItem::Source(source)) = registry
                .catalog_detail(
                    &CatalogEntity::Source {
                        id: descriptor.source_root_id,
                    },
                    Some(state.revision),
                )?
                .item
            else {
                return Err(ScopeReadRefused.into());
            };
            ensure!(
                source.index_id == member.index_id
                    && source.enabled
                    && source.canonical_path == descriptor.original_root,
                ScopeReadRefused
            );
            sources.push(source);
        }
        members.push(Member {
            index,
            authority,
            sources,
            selection: child,
        });
    }
    ensure!(
        registry.catalog_state()?.as_ref() == Some(&state)
            && registry.catalog_pending_fence()?.is_none(),
        ScopeReadRefused
    );
    let fingerprints = members
        .iter()
        .map(|member| Ok((member.index.id, member.authority.fingerprint()?)))
        .collect::<Result<Vec<_>>>()?;
    let identity = framed_digest(
        b"airk/combined-native-authority/1",
        &[
            &serde_json::to_vec(&(
                session.namespace,
                session.context_key,
                session.grant_id,
                session.expected_grant_revision,
            ))?,
            &serde_json::to_vec(&fingerprints)?,
            &serde_json::to_vec(&resolved)?,
        ],
    );
    Ok(Snapshot {
        revision: state.revision,
        resolved,
        projects,
        members,
        identity,
    })
}

async fn snapshot(
    root: &Path,
    session: &CombinedConnectRequest,
    selection: &QuerySelection,
) -> Result<Snapshot> {
    let (root, session, selection) = (root.to_path_buf(), session.clone(), selection.clone());
    tokio::task::spawn_blocking(move || read_snapshot(&root, &session, &selection)).await?
}

/// Native final-delivery metadata proof. The authenticated service must hold
/// its in-memory admission ticket separately while validating and issuing it.
pub(crate) fn validate_delivery(
    root: &Path,
    session: &CombinedConnectRequest,
    selected: &QuerySelection,
    expected_scope: Sha256Digest,
    expected_catalog_revision: CatalogRevision,
) -> Result<()> {
    let snapshot = read_snapshot(root, session, selected)?;
    ensure!(
        snapshot.identity == expected_scope && snapshot.revision == expected_catalog_revision,
        ScopeReadRefused
    );
    Ok(())
}

/// Resolve metadata only. Frozen selectors confer no authority; every read
/// validates the current reviewed snapshot before any selected worker is used.
pub(crate) fn validate_http_selection(
    control: &Path,
    selection: &CombinedConnectRequest,
) -> Result<CombinedConnectRequest> {
    let snapshot = read_snapshot(control, selection, &selection.selection())?;
    let mut refreshed = selection.clone();
    refreshed.expected_catalog_revision = snapshot.revision;
    Ok(refreshed)
}

pub async fn resolve(
    install: Option<&Path>,
    control: Option<&Path>,
    selection: &CombinedConnectRequest,
) -> Result<Arc<dyn RemoteTools>> {
    selection.validate()?;
    let (install, control) = if let Some(control) = control {
        let control = control.canonicalize()?;
        crate::runtime_install::validate_control_root(&control)?;
        (None, control)
    } else {
        let install = install
            .map(Path::to_path_buf)
            .map(Ok)
            .unwrap_or_else(crate::runtime_install::caller_install_root)?;
        let control = crate::runtime_install::installed_control(&install).await?;
        (Some(install), control)
    };
    snapshot(&control, selection, &selection.selection()).await?;
    let backend: Arc<dyn RemoteTools> = Arc::new(Backend {
        control: control.clone(),
        selection: selection.clone(),
        children: Mutex::new(BTreeMap::new()),
        closed: std::sync::atomic::AtomicBool::new(false),
        client: Mutex::new(airk_core::usage::ClientFamily::Cli),
        service_instance: tokio::sync::OnceCell::new(),
    });
    Ok(crate::unified_mcp::with_runtime_readiness(
        install, control, backend,
    ))
}

struct Backend {
    control: PathBuf,
    selection: CombinedConnectRequest,
    children: Mutex<BTreeMap<Sha256Digest, Arc<crate::project_client::Backend>>>,
    closed: std::sync::atomic::AtomicBool,
    client: Mutex<airk_core::usage::ClientFamily>,
    service_instance: tokio::sync::OnceCell<airk_core::identity::Incarnation>,
}

fn failure(code: &str, submitted: Option<bool>, selection: &CombinedConnectRequest) -> Value {
    let body = json!({"error":{"code":code,"retryable":false,"query_submitted":submitted,"query_replay_safe":false},"requested_scope":{"confirmed":false,"selection":selection}});
    json!({"content":[{"type":"text","text":body.to_string()}],"structuredContent":body,"isError":true})
}

impl Backend {
    async fn finalize(
        &self,
        selected: &QuerySelection,
        scope_identity: Sha256Digest,
        result: Value,
        deadline: Instant,
    ) -> Result<Value> {
        self.finalize_budget(
            selected,
            scope_identity,
            result,
            deadline,
            ReplyBudget::Read,
        )
        .await
    }

    async fn finalize_budget(
        &self,
        selected: &QuerySelection,
        scope_identity: Sha256Digest,
        result: Value,
        deadline: Instant,
        budget: ReplyBudget,
    ) -> Result<Value> {
        use airk_core::service::{ControlMethod, ControlResponse};
        ensure!(
            !self.closed.load(std::sync::atomic::Ordering::Acquire),
            ScopeReadRefused
        );
        let was_error = result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut body = canonical_body(&result);
        let result_digest = framed_digest(
            b"airk/combined-delivery-result/1",
            &[&serde_json::to_vec(&body)?],
        );
        let instance = *tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            self.service_instance.get_or_try_init(|| async {
                let health =
                    crate::service_runtime::request(&self.control, ControlMethod::Health).await?;
                let ControlResponse::Health { observation } = health else {
                    return Err(anyhow::Error::from(ScopeReadRefused));
                };
                Ok(observation.instance)
            }),
        )
        .await
        .map_err(|_| ScopeReadRefused)??;
        let response = tokio::time::timeout_at(
            tokio::time::Instant::from_std(deadline),
            crate::service_runtime::request_for_instance(
                &self.control,
                ControlMethod::ValidateCombinedDelivery {
                    selection: self.selection.clone(),
                    query_selection: selected.clone(),
                    scope_identity,
                    result_digest,
                },
                instance,
            ),
        )
        .await
        .map_err(|_| ScopeReadRefused)??;
        let ControlResponse::CombinedDeliveryValidated { receipt } = response else {
            return Err(ScopeReadRefused.into());
        };
        ensure!(
            receipt.schema == 1
                && receipt.service_instance == instance
                && receipt.namespace == self.selection.namespace
                && receipt.catalog_revision >= self.selection.expected_catalog_revision
                && receipt.scope_identity == scope_identity
                && receipt.result_digest == result_digest,
            ScopeReadRefused
        );
        ensure!(
            !self.closed.load(std::sync::atomic::Ordering::Acquire),
            ScopeReadRefused
        );
        body["delivery_observation"] =
            json!({"receipt":receipt,"digest_basis":"canonical_body_without_delivery_observation"});
        let mut result = native_envelope(body);
        if was_error || result_failed(&result) {
            result["isError"] = true.into();
        }
        check_public_reply_budget(&result, budget)?;
        Ok(result)
    }
    fn request_selection(&self, args: &mut Map<String, Value>) -> Result<QuerySelection> {
        ensure!(
            !args.contains_key("project"),
            "legacy project cannot select combined reads"
        );
        let projects = args.remove("project_ids");
        let indexes = args.remove("index_ids");
        ensure!(
            projects.is_some() || indexes.is_none(),
            "index_ids requires project_ids"
        );
        ensure!(
            projects.is_some() || !args.contains_key("member_availability"),
            "member_availability requires project_ids"
        );
        let explicit_projects = projects.is_some();
        let project_ids = projects
            .map(serde_json::from_value)
            .transpose()?
            .unwrap_or_else(|| self.selection.project_ids.clone());
        let index_ids = indexes
            .map(serde_json::from_value)
            .transpose()?
            .or_else(|| {
                if explicit_projects {
                    None
                } else {
                    self.selection.index_ids.clone()
                }
            });
        let member_availability = args
            .remove("member_availability")
            .map(serde_json::from_value)
            .transpose()?
            .unwrap_or(MemberAvailability::Report);
        let selection = QuerySelection {
            project_ids,
            index_ids,
            member_availability,
        };
        selection.validate()?;
        Ok(selection)
    }

    fn child(&self, member: &Member) -> Result<Arc<crate::project_client::Backend>> {
        let fingerprint = member.authority.fingerprint()?;
        let mut children = self.children.lock().map_err(|_| ScopeReadRefused)?;
        if let Some(child) = children.get(&fingerprint) {
            return Ok(child.clone());
        }
        let child = crate::project_client::scoped_backend(
            self.control.clone(),
            member.index.backing.binding,
            member.selection.clone(),
        )?;
        child.client(*self.client.lock().map_err(|_| ScopeReadRefused)?);
        // A bounded cache retains ordinary native channel pools. New request
        // combinations beyond it use an ephemeral pool, never evict live work.
        if children.len() < 128 {
            children.insert(fingerprint, child.clone());
        }
        Ok(child)
    }

    async fn discover(
        &self,
        args: Map<String, Value>,
        selected: &QuerySelection,
        before: Snapshot,
        deadline: Instant,
    ) -> Result<Value> {
        use airk_mcp::combined::{QueryScopeArgs, QueryScopeView};
        let args: QueryScopeArgs = serde_json::from_value(Value::Object(args))?;
        args.validate()?;
        if args.view == QueryScopeView::Recommendations {
            let selected_scope = airk_core::combined_reads::recommendations::select(
                &before.resolved,
                args.selection.as_ref(),
            )?;
            let indexes = before
                .members
                .iter()
                .map(|member| member.index.clone())
                .collect::<Vec<_>>();
            let mut body = airk_core::combined_reads::recommendations::project(
                &before.resolved,
                &selected_scope,
                &before.projects,
                &indexes,
                args.situation.unwrap_or_default(),
                now_ms(),
            )?;
            body["catalog_revision"] = json!(before.revision);
            body["scope_identity"] = json!(before.identity);
            body["source_boundaries"] = json!(before.members.iter().flat_map(|member| member.sources.iter().map(move |source| json!({"index_id":member.index.id,"source_root_id":source.id,"original_root":source.canonical_path,"aliases":source.aliases}))).collect::<Vec<_>>());
            body["instructions"] = airk_mcp::scope_instructions::generate(&selected_scope, &body)?;
            let after = snapshot(&self.control, &self.selection, selected).await?;
            ensure!(
                before.identity == after.identity && before.revision == after.revision,
                ScopeReadRefused
            );
            airk_core::index_catalog::encoded_size(
                &body,
                args.max_bytes()
                    .saturating_sub(DELIVERY_OBSERVATION_RESERVE),
            )?;
            let result = self
                .finalize(selected, before.identity, native_envelope(body), deadline)
                .await?;
            airk_core::index_catalog::encoded_size(&canonical_body(&result), args.max_bytes())?;
            return Ok(result);
        }
        let page_bytes = args.max_bytes().saturating_sub(1024);
        let view = match args.view {
            QueryScopeView::Projects => "projects",
            QueryScopeView::Indexes => "indexes",
            QueryScopeView::Sources => "sources",
            QueryScopeView::Recommendations => unreachable!("handled above"),
        };
        let mut records = Vec::new();
        match args.view {
            QueryScopeView::Projects => {
                for project in &before.projects {
                    let permitted = before
                        .resolved
                        .members
                        .iter()
                        .filter(|member| member.project_ids().contains(&project.id))
                        .map(|member| member.index_id)
                        .collect::<Vec<_>>();
                    records.push(json!({"project_id":project.id,"name":project.name,"definition_revision":project.definition_revision,"scope_revision":project.scope_revision,"defaults_revision":project.defaults_revision,"default_indexes":project.default_indexes.iter().filter(|id|permitted.contains(id)).collect::<Vec<_>>(),"index_ids":permitted}));
                }
            }
            QueryScopeView::Indexes => {
                let project_id = args.project_id.ok_or(ScopeReadRefused)?;
                ensure!(
                    before.resolved.project_ids.contains(&project_id),
                    ScopeReadRefused
                );
                for member in &before.members {
                    let resolved = before
                        .resolved
                        .member(member.index.id)
                        .ok_or(ScopeReadRefused)?;
                    if let Some(membership) = resolved
                        .memberships
                        .iter()
                        .find(|item| item.membership.project_id == project_id)
                    {
                        records.push(json!({"index_id":member.index.id,"name":member.index.name,"definition_revision":member.index.definition_revision,"source_revision":member.index.source_revision,"membership":membership,"default":before.projects.iter().find(|project|project.id==project_id).is_some_and(|project|project.default_indexes.contains(&member.index.id))}));
                    }
                }
            }
            QueryScopeView::Sources => {
                let project_id = args.project_id.ok_or(ScopeReadRefused)?;
                let index_id = args.index_id.ok_or(ScopeReadRefused)?;
                let member = before
                    .members
                    .iter()
                    .find(|member| member.index.id == index_id)
                    .ok_or(ScopeReadRefused)?;
                let resolved = before.resolved.member(index_id).ok_or(ScopeReadRefused)?;
                let membership = resolved
                    .memberships
                    .iter()
                    .find(|item| item.membership.project_id == project_id)
                    .ok_or(ScopeReadRefused)?;
                for source in &member.sources {
                    let scope = membership
                        .membership
                        .scope
                        .iter()
                        .filter(|atom| atom.source_root_id == source.id)
                        .collect::<Vec<_>>();
                    if !scope.is_empty() {
                        records.push(json!({"source_root_id":source.id,"index_id":index_id,"project_id":project_id,"original_root":source.canonical_path,"aliases":source.aliases,"scope":scope,"source_revision":membership.source_revision}));
                    }
                }
            }
            QueryScopeView::Recommendations => unreachable!("handled above"),
        }
        let identity = framed_digest(
            b"airk/query-scope-page/1",
            &[&serde_json::to_vec(&(
                view,
                args.project_id,
                args.index_id,
                before.identity,
                before.revision,
                args.limit(),
                args.max_bytes(),
            ))?],
        );
        let offset = if let Some(cursor) = &args.cursor {
            let cursor: DiscoveryCursor = serde_json::from_str(cursor)?;
            ensure!(
                cursor.schema == 1 && cursor.identity == identity && cursor.offset < records.len(),
                "query_scope_cursor_expired"
            );
            cursor.offset
        } else {
            0
        };
        let mut page = Vec::new();
        let mut next = offset;
        let mut body;
        loop {
            let cursor = if next < records.len() {
                Some(serde_json::to_string(&DiscoveryCursor {
                    schema: 1,
                    identity,
                    offset: next,
                })?)
            } else {
                None
            };
            body = json!({"schema":"airk.query-scope/1","mode":"reviewed_grant","view":view,"grant":{"id":self.selection.grant_id,"revision":self.selection.expected_grant_revision},"catalog_revision":before.revision,"scope_identity":before.identity,"selection":before.resolved.observation()?,"observed_unix_ms":now_ms(),"items":page,"count":records.len(),"count_precision":"exact_selected_scope","truncated":next<records.len(),"cursor":cursor});
            if next == records.len() || page.len() == args.limit() {
                break;
            }
            let mut candidate = body.clone();
            candidate["items"]
                .as_array_mut()
                .unwrap()
                .push(records[next].clone());
            if airk_core::index_catalog::encoded_size(&candidate, page_bytes).is_err() {
                break;
            }
            page.push(records[next].clone());
            next += 1;
        }
        ensure!(
            next > offset || records.is_empty(),
            "query_scope_reply_budget"
        );
        airk_core::index_catalog::encoded_size(&body, page_bytes)?;
        let after = snapshot(&self.control, &self.selection, selected).await?;
        ensure!(
            before.identity == after.identity && before.revision == after.revision,
            ScopeReadRefused
        );
        let result = native_envelope(body);
        check_public_reply(&result)?;
        let result = self
            .finalize(selected, before.identity, result, deadline)
            .await?;
        airk_core::index_catalog::encoded_size(&canonical_body(&result), args.max_bytes())?;
        Ok(result)
    }

    async fn parse(
        &self,
        mut args: Map<String, Value>,
        selected: &QuerySelection,
        before: Snapshot,
        deadline: Instant,
        capacity: usize,
        context: airk_mcp::read_attempt::ReadAttemptContext,
    ) -> Result<Value> {
        args.remove("response_mode");
        args.remove("include_status");
        let request = airk_core::workbench::decode_request(Value::Object(args))?;
        let seconds = request
            .budgets
            .as_ref()
            .map(|budget| budget.seconds)
            .or_else(|| request.plan.as_ref().map(|plan| plan.budgets.seconds))
            .unwrap_or(30)
            .min(30);
        let deadline = deadline.min(Instant::now() + Duration::from_secs(seconds));
        let result = airk_kernel::workbench::combined::execute(
            request,
            selected.clone(),
            before.identity,
            |tool, mut args| {
                if tool == "__airk_verify_projection" {
                    return self.verify_final_callback(
                        args,
                        selected.clone(),
                        before.clone(),
                        context.narrowed(deadline),
                    );
                }
                // Discovery's public input remains the closed singular schema. Its
                // plan callback receives the already validated captured selection
                // through this private adapter context, after plan validation.
                if matches!(tool.as_str(), "query_scope" | "airk_query_scope") {
                    args.insert("project_ids".into(), json!(selected.project_ids));
                    if let Some(indexes) = &selected.index_ids {
                        args.insert("index_ids".into(), json!(indexes));
                    }
                    args.insert(
                        "member_availability".into(),
                        json!(selected.member_availability),
                    );
                }
                self.dispatch(tool, args, true, capacity, context.narrowed(deadline))
            },
        )
        .await?;
        let after = snapshot(&self.control, &self.selection, selected).await?;
        ensure!(before.identity == after.identity, ScopeReadRefused);
        let failed = result.status == "failed" || result.status == "requirements_unmet";
        let output_max_bytes = result.plan.output.max_bytes;
        let budget = ReplyBudget::Plan {
            max_bytes: output_max_bytes,
        };
        let body = serde_json::to_value(result)?;
        let result = json!({"content":[{"type":"text","text":body.to_string()}],"structuredContent":body,"isError":failed});
        check_public_reply_budget(&result, budget)?;
        let result = self
            .finalize_budget(selected, before.identity, result, deadline, budget)
            .await?;
        airk_core::index_catalog::encoded_size(&canonical_body(&result), output_max_bytes)
            .map_err(|_| airk_kernel::combined_reads::CombinedReplyBudget)?;
        Ok(result)
    }

    fn verify_final_callback(
        &self,
        mut args: Map<String, Value>,
        mut selected: QuerySelection,
        before: Snapshot,
        context: airk_mcp::read_attempt::ReadAttemptContext,
    ) -> RemoteFuture<'_> {
        Box::pin(async move {
            let mut requested = self.request_selection(&mut args)?;
            requested.project_ids.sort();
            selected.project_ids.sort();
            if let Some(indexes) = requested.index_ids.as_mut() {
                indexes.sort();
            }
            if let Some(indexes) = selected.index_ids.as_mut() {
                indexes.sort();
            }
            ensure!(
                requested == selected,
                "final verification cannot change the captured Project/index selection"
            );
            self.verify_projection(args, &selected, before, context)
                .await
        })
    }

    async fn verify_projection(
        &self,
        mut args: Map<String, Value>,
        _selected: &QuerySelection,
        before: Snapshot,
        context: airk_mcp::read_attempt::ReadAttemptContext,
    ) -> Result<Value> {
        use airk_core::verification::{VerificationAllocation, VerificationControls};
        let started = Instant::now();
        let projection = args
            .remove("projection")
            .context("final combined projection required")?;
        let remaining = args
            .remove("remaining_deadline_ms")
            .map(serde_json::from_value::<u64>)
            .transpose()?
            .unwrap_or(2000);
        let receipt_bytes = args
            .remove("receipt_bytes_budget")
            .map(serde_json::from_value::<usize>)
            .transpose()?
            .unwrap_or(32768);
        ensure!(
            (1024..=32768).contains(&receipt_bytes),
            "invalid combined verification receipt allocation"
        );
        ensure!(
            args.keys().all(|key| key.starts_with("verification")),
            "unknown final verification control"
        );
        let controls: VerificationControls = serde_json::from_value(Value::Object(args))?;
        let budget = controls
            .resolve()?
            .context("final verification requires enabled controls")?;
        let deadline = started + Duration::from_millis(budget.deadline_ms.min(remaining));
        airk_core::index_catalog::encoded_size(&projection, 524288)?;
        let total = SharedBudgets {
            limit: before.members.len(),
            max_bytes: 65536,
            deadline_ms: 30000,
            verification_max_sources: budget.max_sources,
            verification_max_bytes: budget.max_bytes,
            verification_deadline_ms: budget.deadline_ms.min(remaining),
        };
        let allocations = total.allocate(&before.resolved)?;
        let mut subsets = BTreeMap::<IndexId, MemberProjection>::new();
        if let Some(steps) = projection.get("steps").and_then(Value::as_array) {
            ensure!(
                steps.len() <= 72,
                "combined plan projection exceeds step bound"
            );
            for (offset, step) in steps.iter().enumerate() {
                if let Some(result) = step.get("result").filter(|result| !result.is_null()) {
                    collect_member_projections(
                        result,
                        &format!("/steps/{offset}/result"),
                        &mut subsets,
                        0,
                    )?;
                }
            }
        }
        if let Some(evidence) = projection.get("evidence").and_then(Value::as_array) {
            ensure!(
                evidence.len() <= 4096,
                "combined final evidence exceeds bound"
            );
            for (offset, record) in evidence.iter().enumerate() {
                let origin: airk_kernel::combined_reads::QualifiedOrigin = serde_json::from_value(
                    record
                        .get("origin")
                        .context("final evidence has no qualified origin")?
                        .clone(),
                )?;
                let index = origin.index_id;
                let member = before.resolved.member(index).ok_or(ScopeReadRefused)?;
                ensure!(
                    member.projects_for(origin.source_root_id, &origin.source_relative_path)?
                        == origin.project_ids,
                    ScopeReadRefused
                );
                let mut row = record.clone();
                row["publication"] = serde_json::to_value(&origin.publication)?;
                row.as_object_mut()
                    .ok_or(ScopeReadRefused)?
                    .remove("origin");
                let subset = subsets.entry(index).or_default();
                let position = subset.steps.len();
                subset.steps.push(json!({"result":row}));
                subset.bindings.push(json!({"member_pointer":format!("/steps/{position}/result"),"result_pointer":format!("/evidence/{offset}")}));
            }
        }
        ensure!(
            subsets
                .keys()
                .all(|id| before.resolved.member(*id).is_some()),
            ScopeReadRefused
        );
        let mut members = Vec::new();
        let mut failed = false;
        for (member, allocation) in before.members.iter().zip(allocations) {
            let Some(subset) = subsets.remove(&member.index.id) else {
                continue;
            };
            let remaining_ms = deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .min(u128::from(u64::MAX)) as u64;
            let allocation = VerificationAllocation {
                max_sources: allocation.verification_max_sources,
                max_bytes: allocation.verification_max_bytes,
                deadline_ms: allocation.verification_deadline_ms.min(remaining_ms),
            };
            let receipt_share = receipt_bytes / before.members.len();
            let Some(window) = verification_window(deadline, allocation.deadline_ms) else {
                failed = true;
                members.push(json!({"index_id":member.index.id,"outcome":"unqueried","cutoff":"verification_deadline_exhausted","result":{"steps":subset.steps,"error":{"code":"verification_deadline_exhausted","retryable":false,"verification_performed":false,"query_submitted":false,"query_replay_safe":false}},"reference_bindings":subset.bindings,"allocation":allocation}));
                continue;
            };
            if receipt_share < 1024 {
                failed = true;
                members.push(json!({"index_id":member.index.id,"result":{"error":{"code":"verification_receipt_budget_exhausted","retryable":false,"verification_performed":false}},"reference_bindings":subset.bindings,"allocation":allocation}));
                continue;
            }
            let child = self.child(member)?;
            let own = json!({"steps":subset.steps});
            let result = match tokio::time::timeout(
                window,
                child.call_native_with_context(
                    airk_core::native_reads::NativeRead::VerifyProjection {
                        value: own,
                        controls,
                        allocation: Some(allocation),
                        receipt_bytes_budget: Some(receipt_share),
                    },
                    context.narrowed(Instant::now() + window),
                ),
            )
            .await
            {
                Ok(Ok(result)) => result,
                Ok(Err(_)) | Err(_) => failure(
                    "member_verification_observation_unavailable",
                    None,
                    &self.selection,
                ),
            };
            failed |= result_failed(&result);
            members.push(json!({"index_id":member.index.id,"result":result,"reference_bindings":subset.bindings,"allocation":allocation}));
        }
        // This private callback never delivers evidence itself. The enclosing
        // parse result is checked by the native final-delivery gate; starting a
        // new metadata observation here could extend an exhausted deadline.
        let result = json!({"schema":"airk.combined-verification/1","scope_identity":before.identity,"members":members,"partial":failed,"error":if failed && budget.mode==airk_core::verification::VerificationMode::Require {Some(json!({"code":"member_verification_failed","retryable":false}))}else{None},"binding_basis":"member_pointer prefixes map canonical native receipt pointers to the final result projection"});
        airk_core::index_catalog::encoded_size(&result, receipt_bytes)
            .map_err(|_| airk_kernel::combined_reads::CombinedReplyBudget)?;
        Ok(result)
    }

    async fn execute_read(
        &self,
        tool: &str,
        mut args: Map<String, Value>,
        selected: &QuerySelection,
        before: Snapshot,
        admission: ReadAdmission,
    ) -> Result<Value> {
        let ReadAdmission {
            remaining_deadline_ms,
            capacity,
            context,
        } = admission;
        use airk_core::verification::{
            VerificationAllocation, VerificationControls, VerificationMode,
        };
        use airk_kernel::combined_reads::{
            CompositionRequest, MemberInput, compose, compose_capabilities,
        };
        let origin = match args
            .remove("origin_publication")
            .map(serde_json::from_value::<String>)
            .transpose()
        {
            Ok(origin) => origin,
            Err(_) => {
                return Ok(failure(
                    "invalid_native_arguments",
                    Some(false),
                    &self.selection,
                ));
            }
        };
        if origin.as_ref().is_some_and(|origin| {
            origin
                .parse::<airk_core::identity::RevisionToken>()
                .is_err()
        }) {
            return Ok(failure(
                "invalid_native_arguments",
                Some(false),
                &self.selection,
            ));
        }
        let multi = matches!(
            tool,
            "airk_search"
                | "airk_symbols"
                | "airk_code_facts"
                | "airk_document_facts"
                | "airk_references"
                | "airk_list_files"
                | "airk_status"
                | "airk_diagnostics"
                | "airk_capabilities"
        );
        if !multi && before.members.len() != 1 || origin.is_some() && before.members.len() != 1 {
            let mut body = canonical_body(&failure(
                "originating_index_required",
                Some(false),
                &self.selection,
            ));
            body["requires_input"] = json!({"field":"index_ids","choices":before.resolved.members.iter().map(|member|member.index_id).collect::<Vec<_>>(),"reason":"Choose one originating index from the selected Project union."});
            return Ok(native_envelope(body));
        }
        let catalog = airk_mcp::local::tools();
        let Some(definition) = catalog.iter().find(|entry| entry.name == tool) else {
            return Ok(failure(
                "unknown_combined_tool",
                Some(false),
                &self.selection,
            ));
        };
        let properties = definition
            .input_schema
            .get("properties")
            .and_then(Value::as_object)
            .context("native input catalog has no properties")?;
        if args.contains_key("cursor") && !properties.contains_key("cursor") {
            return Ok(failure(
                "invalid_native_arguments",
                Some(false),
                &self.selection,
            ));
        }
        let mut native_validation = args.clone();
        native_validation.remove("cursor");
        if airk_mcp::combined::validate_native_arguments(tool, &native_validation).is_err() {
            return Ok(failure(
                "invalid_native_arguments",
                Some(false),
                &self.selection,
            ));
        }
        let verification: VerificationControls = serde_json::from_value(Value::Object(
            args.iter()
                .filter(|(key, _)| key.starts_with("verification"))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
        ))?;
        let enabled = properties.contains_key("verification");
        let verification_budget = if enabled {
            verification.resolve()?
        } else {
            None
        };
        let limit = args
            .get("limit")
            .map(|value| serde_json::from_value::<usize>(value.clone()))
            .transpose()?
            .unwrap_or(if properties.contains_key("limit") {
                16
            } else {
                before.members.len()
            });
        let max_bytes = args
            .get("max_bytes")
            .map(|value| serde_json::from_value::<usize>(value.clone()))
            .transpose()?
            .unwrap_or(16384);
        let budgets = SharedBudgets {
            limit,
            max_bytes,
            deadline_ms: remaining_deadline_ms.unwrap_or(30000).clamp(1, 30000),
            verification_max_sources: verification_budget.map_or(0, |budget| budget.max_sources),
            verification_max_bytes: verification_budget.map_or(0, |budget| budget.max_bytes),
            verification_deadline_ms: verification_budget.map_or(0, |budget| budget.deadline_ms),
        };
        let allocations = budgets.allocate(&before.resolved)?;
        let started = Instant::now();
        let mut observed_from_unix_ms = now_ms();
        let mut observed_to_unix_ms = observed_from_unix_ms;
        let continuation = if let Some(token) = args.get("cursor") {
            let Some(token) = token.as_str() else {
                return Ok(failure(
                    "invalid_combined_cursor",
                    Some(false),
                    &self.selection,
                ));
            };
            let ids = before
                .members
                .iter()
                .map(|member| member.index.id)
                .collect::<Vec<_>>();
            match cursor::Continuation::decode(token, tool, &args, before.identity, &ids) {
                Ok(cursor) => Some(cursor),
                Err(_) => {
                    return Ok(failure(
                        "combined_cursor_expired",
                        Some(false),
                        &self.selection,
                    ));
                }
            }
        } else {
            None
        };
        let mut resume_status = BTreeMap::new();
        if let Some(continuation) = &continuation {
            for member in &before.members {
                let child = self.child(member)?;
                let remaining =
                    Duration::from_millis(budgets.deadline_ms).saturating_sub(started.elapsed());
                let expected = continuation
                    .publication(member.index.id)
                    .ok_or(ScopeReadRefused)?
                    .token;
                let status = match tokio::time::timeout(
                    remaining,
                    child.call_native_with_context(
                        airk_core::native_reads::NativeRead::SourceProjection {
                            publication: expected,
                        },
                        context.narrowed(Instant::now() + remaining),
                    ),
                )
                .await
                {
                    Ok(Ok(status)) => status,
                    Ok(Err(_)) | Err(_) => {
                        return Ok(failure(
                            "combined_cursor_publication_unavailable",
                            None,
                            &self.selection,
                        ));
                    }
                };
                let body = canonical_body(&status);
                if result_failed(&status)
                    || publication_token(&body).is_none_or(|token| {
                        continuation
                            .validate_publication(member.index.id, token)
                            .is_err()
                    })
                {
                    return Ok(failure(
                        "combined_cursor_expired",
                        Some(false),
                        &self.selection,
                    ));
                }
                resume_status.insert(member.index.id, status);
            }
        }
        let mut results = Vec::new();
        let mut child_cursors = BTreeMap::new();
        let mut resumable = properties.contains_key("cursor");
        let mut verification_failed_members = Vec::new();
        let mut verification_deadline = None;
        for (member, allocation) in before.members.iter().zip(&allocations) {
            if continuation
                .as_ref()
                .is_some_and(|cursor| cursor.is_exhausted(member.index.id))
            {
                let result = resume_status
                    .remove(&member.index.id)
                    .ok_or(ScopeReadRefused)?;
                child_cursors.insert(member.index.id, None);
                results.push(MemberInput {
                    index_id: member.index.id,
                    result: canonical_body(&result),
                    is_error: false,
                    exhausted: true,
                    sources: vec![],
                });
                continue;
            }
            if !allocation.admits_read()
                || started.elapsed() >= Duration::from_millis(budgets.deadline_ms)
            {
                continue;
            }
            let child = self.child(member)?;
            let mut child_args = args.clone();
            if let Some(continuation) = &continuation {
                child_args.remove("cursor");
                if let Some(cursor) = continuation.cursor(member.index.id) {
                    child_args.insert("cursor".into(), json!(cursor));
                }
            }
            if properties.contains_key("limit") {
                child_args.insert("limit".into(), json!(allocation.limit));
            }
            if properties.contains_key("max_bytes") {
                child_args.insert("max_bytes".into(), json!(allocation.max_bytes));
            }
            if properties.contains_key("response_mode") {
                child_args.insert("response_mode".into(), json!("full"));
            }
            if properties.contains_key("include_status") {
                child_args.insert("include_status".into(), json!(true));
            }
            if enabled {
                child_args.retain(|key, _| !key.starts_with("verification"));
                child_args.insert("verification".into(), json!("off"));
            }
            let remaining = Duration::from_millis(budgets.deadline_ms)
                .saturating_sub(started.elapsed())
                .min(Duration::from_millis(allocation.deadline_ms));
            let mut result = match tokio::time::timeout(
                remaining,
                child.call_with_read_context(
                    tool.into(),
                    child_args,
                    capacity,
                    context.narrowed(Instant::now() + remaining),
                ),
            )
            .await
            {
                Ok(Ok(result)) => result,
                Ok(Err(_)) => failure("member_observation_unavailable", None, &self.selection),
                Err(_) => failure("member_deadline_exhausted", None, &self.selection),
            };
            if let Some(expected) = &origin {
                let body = canonical_body(&result);
                let actual = publication_token(&body).map(str::to_owned);
                if actual.as_deref() != Some(expected) {
                    result = failure("origin_publication_expired", Some(true), &self.selection);
                }
            }
            if let Some(continuation) = &continuation {
                let body = canonical_body(&result);
                if publication_token(&body).is_none_or(|token| {
                    continuation
                        .validate_publication(member.index.id, token)
                        .is_err()
                }) {
                    return Ok(failure(
                        "combined_cursor_expired",
                        Some(true),
                        &self.selection,
                    ));
                }
            }
            let mut published_sources = Vec::new();
            if !result_failed(&result)
                && (origin.is_some()
                    || !airk_core::verification::ReferenceInventory::collect(&canonical_body(
                        &result,
                    ))
                    .references
                    .is_empty())
            {
                let body = canonical_body(&result);
                let expected = publication_token(&body)
                    .context("member evidence publication required")?
                    .parse()?;
                let remaining =
                    Duration::from_millis(budgets.deadline_ms).saturating_sub(started.elapsed());
                let projection = if let Some(projection) = resume_status.get(&member.index.id) {
                    Ok(Ok(projection.clone()))
                } else {
                    tokio::time::timeout(
                        remaining,
                        child.call_native_with_context(
                            airk_core::native_reads::NativeRead::SourceProjection {
                                publication: expected,
                            },
                            context.narrowed(Instant::now() + remaining),
                        ),
                    )
                    .await
                };
                match projection {
                    Ok(Ok(projection)) => {
                        match serde_json::from_value::<
                            airk_core::scoped_reads::ScopedSourceProjection,
                        >(canonical_body(&projection))
                        {
                            Ok(projection)
                                if projection.validate().is_ok()
                                    && projection.publication.token == expected
                                    && projection.scope_fingerprint
                                        == member.authority.fingerprint()? =>
                            {
                                published_sources = projection
                                    .sources
                                    .into_iter()
                                    .map(|source| airk_kernel::combined_reads::MemberSource {
                                        source_root_id: source.source_root_id,
                                        original_root: source.original_root,
                                        primary: source.primary,
                                        aliases: source.aliases,
                                    })
                                    .collect();
                            }
                            _ => result = source_projection_failure(&result, Some(projection)),
                        }
                    }
                    _ => result = source_projection_failure(&result, None),
                }
            }
            if enabled && verification_budget.is_some() && !result_failed(&result) {
                let projection = canonical_body(&result);
                let controls = VerificationControls {
                    verification: Some(
                        verification_budget.map_or(VerificationMode::Off, |budget| budget.mode),
                    ),
                    ..verification
                };
                let deadline = *verification_deadline.get_or_insert_with(|| {
                    Instant::now() + Duration::from_millis(budgets.verification_deadline_ms)
                });
                let deadline = deadline.min(started + Duration::from_millis(budgets.deadline_ms));
                let remaining_ms = deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    .min(u128::from(u64::MAX)) as u64;
                let allocation = VerificationAllocation {
                    max_sources: allocation.verification_max_sources,
                    max_bytes: allocation.verification_max_bytes,
                    deadline_ms: allocation.verification_deadline_ms.min(remaining_ms),
                };
                result = if let Some(window) = verification_window(deadline, allocation.deadline_ms)
                {
                    match tokio::time::timeout(
                        window,
                        child.call_native_with_context(
                            airk_core::native_reads::NativeRead::VerifyProjection {
                                value: projection,
                                controls,
                                allocation: Some(allocation),
                                receipt_bytes_budget: Some(32768 / before.members.len()),
                            },
                            context.narrowed(Instant::now() + window),
                        ),
                    )
                    .await
                    {
                        Ok(Ok(value)) => native_envelope(value),
                        Ok(Err(_)) | Err(_) => verification_uncertainty(
                            &result,
                            controls.verification.unwrap_or(VerificationMode::Report),
                        ),
                    }
                } else {
                    verification_not_submitted(
                        &result,
                        controls.verification.unwrap_or(VerificationMode::Report),
                    )
                };
            }
            let is_error = result_failed(&result);
            let result = canonical_body(&result);
            extend_observation_interval(
                &result,
                &mut observed_from_unix_ms,
                &mut observed_to_unix_ms,
            );
            if verification_budget.is_some_and(|budget| budget.mode == VerificationMode::Require)
                && !airk_core::verification::ReferenceInventory::collect(&result)
                    .references
                    .is_empty()
                && result
                    .pointer("/source_verification/requirements_satisfied")
                    .and_then(Value::as_bool)
                    != Some(true)
            {
                verification_failed_members.push(member.index.id);
            }
            if resumable {
                match cursor::child_cursor(&result) {
                    Ok(cursor) if !is_error => {
                        child_cursors.insert(member.index.id, cursor);
                    }
                    _ => resumable = false,
                }
            }
            results.push(MemberInput {
                index_id: member.index.id,
                result,
                is_error,
                exhausted: false,
                sources: published_sources,
            });
        }
        let after = snapshot(&self.control, &self.selection, selected).await?;
        ensure!(before.identity == after.identity, ScopeReadRefused);
        let request = CompositionRequest {
            selection: before.resolved,
            budgets,
            authority: before.identity,
            observed_from_unix_ms,
            observed_to_unix_ms: now_ms().max(observed_to_unix_ms),
        };
        let response = if tool == "airk_capabilities" {
            compose_capabilities(&request, results)?
        } else {
            compose(&request, results)?
        };
        let mut value = serde_json::to_value(&response)?;
        if matches!(tool, "airk_status" | "airk_diagnostics") {
            value["query_concurrency"] = json!(capacity);
        }
        let verification_failed = !verification_failed_members.is_empty();
        if verification_failed {
            let failure = json!({"code":"verification_required_unmet","retryable":false,"failed_members":verification_failed_members});
            value["verification_failure"] = failure.clone();
            if value.get("error").is_none_or(Value::is_null) {
                value["error"] = failure;
            }
        }
        if tool == "airk_capabilities" {
            value["workbench"] = airk_kernel::workbench::combined::operation_catalog();
        }
        if properties.contains_key("cursor") {
            if resumable && child_cursors.len() == before.members.len() {
                match cursor::Continuation::new(
                    tool,
                    &args,
                    before.identity,
                    response.publication_vector.clone(),
                    child_cursors,
                ) {
                    Ok(continuation) => {
                        value["cursor"] = if continuation.has_more() {
                            json!(continuation.encode()?)
                        } else {
                            Value::Null
                        }
                    }
                    Err(_) => {
                        value["continuation_unavailable"] = json!("combined_cursor_unavailable");
                        value["max_cursor_bytes"] = json!(32768);
                    }
                }
            } else {
                value["continuation_unavailable"] = json!("member_failure_or_budget_cutoff");
            }
        }
        let result = json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":response.error.is_some() || verification_failed});
        check_public_reply(&result)?;
        self.finalize(
            selected,
            before.identity,
            result,
            started + Duration::from_millis(budgets.deadline_ms),
        )
        .await
    }
}

#[cfg(test)]
#[path = "combined_frontend_tests.rs"]
mod tests;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DiscoveryCursor {
    schema: u32,
    identity: Sha256Digest,
    offset: usize,
}

#[derive(Default)]
struct MemberProjection {
    steps: Vec<Value>,
    bindings: Vec<Value>,
}

fn collect_member_projections(
    value: &Value,
    pointer: &str,
    subsets: &mut BTreeMap<IndexId, MemberProjection>,
    depth: usize,
) -> Result<()> {
    ensure!(
        depth <= 4,
        "combined result nesting exceeds projection bound"
    );
    if let Some(rows) = value.as_array() {
        ensure!(rows.len() <= 12, "combined plan fanout exceeds bound");
        for (offset, row) in rows.iter().enumerate() {
            collect_member_projections(row, &format!("{pointer}/{offset}"), subsets, depth + 1)?;
        }
        return Ok(());
    }
    let (body, pointer) = if let Some(body) = value.get("structuredContent") {
        (body, format!("{pointer}/structuredContent"))
    } else {
        (value, pointer.to_owned())
    };
    if body.get("schema").and_then(Value::as_str) != Some("airk.combined-read/1") {
        return Ok(());
    }
    let members = body
        .get("members")
        .and_then(Value::as_array)
        .context("combined member projection missing")?;
    ensure!(
        members.len() <= 32,
        "combined member projection exceeds bound"
    );
    for (offset, member) in members.iter().enumerate() {
        let index: IndexId = serde_json::from_value(member["index_id"].clone())?;
        let Some(result) = member.get("result").filter(|result| !result.is_null()) else {
            continue;
        };
        // Failed reads can retain attempted evidence. Those returned references
        // still need explicit verification accounting; a failure flag cannot
        // silently remove them from the final projection's inventory.
        let own = canonical_body(
            &airk_core::combined_reads::shared_values::materialize_combined_member_result(
                body, index,
            )?,
        );
        ensure!(
            own.get("schema").and_then(Value::as_str) != Some("airk.combined-read/1"),
            "nested combined member projection refused"
        );
        let actual = format!(
            "{pointer}/members/{offset}/result{}",
            if result.get("structuredContent").is_some() {
                "/structuredContent"
            } else {
                ""
            }
        );
        let subset = subsets.entry(index).or_default();
        let position = subset.steps.len();
        subset.steps.push(json!({"result":own}));
        subset.bindings.push(
            json!({"member_pointer":format!("/steps/{position}/result"),"result_pointer":actual}),
        );
    }
    Ok(())
}

fn canonical_body(result: &Value) -> Value {
    if let Some(body) = result.get("structuredContent") {
        return body.clone();
    }
    if let Some(text) = result.pointer("/content/0/text").and_then(Value::as_str)
        && let Ok(body) = serde_json::from_str::<Value>(text)
    {
        return body;
    }
    result.clone()
}

fn publication_token(body: &Value) -> Option<&str> {
    body.pointer("/publication/token")
        .or_else(|| body.pointer("/read/publication/token"))
        .or_else(|| body.pointer("/evidence/publication/token"))
        .and_then(Value::as_str)
}

fn extend_observation_interval(body: &Value, from: &mut u64, to: &mut u64) {
    for pointer in [
        "/observed_unix_ms",
        "/observed_from_unix_ms",
        "/observed_to_unix_ms",
        "/source_verification/observed_from_unix_ms",
        "/source_verification/observed_to_unix_ms",
    ] {
        if let Some(observed) = body.pointer(pointer).and_then(Value::as_u64) {
            *from = (*from).min(observed);
            *to = (*to).max(observed);
        }
    }
}

fn result_failed(result: &Value) -> bool {
    result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || canonical_body(result)
            .get("error")
            .is_some_and(|error| !error.is_null())
}

fn native_envelope(body: Value) -> Value {
    if body.get("content").is_some() {
        return body;
    }
    let failed = body.get("error").is_some_and(|error| !error.is_null());
    json!({"content":[{"type":"text","text":body.to_string()}],"structuredContent":body,"isError":failed})
}

fn verification_uncertainty(
    result: &Value,
    mode: airk_core::verification::VerificationMode,
) -> Value {
    let mut body = canonical_body(result);
    body["source_verification_observation"] = json!({"state":"unavailable","error":{"code":"member_verification_observation_unavailable","retryable":false,"query_submitted":null,"query_replay_safe":false},"requested_mode":mode});
    if mode == airk_core::verification::VerificationMode::Require {
        body["error"] = json!({"code":"verification_observation_unavailable","retryable":false,"query_replay_safe":false});
    }
    native_envelope(body)
}

fn verification_window(deadline: Instant, allocated_ms: u64) -> Option<Duration> {
    let window = deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_millis(allocated_ms));
    (!window.is_zero()).then_some(window)
}

fn verification_not_submitted(
    result: &Value,
    mode: airk_core::verification::VerificationMode,
) -> Value {
    let mut body = canonical_body(result);
    body["source_verification_observation"] = json!({"state":"not_submitted","error":{"code":"verification_deadline_exhausted","retryable":false,"query_submitted":false,"verification_performed":false,"query_replay_safe":false},"requested_mode":mode});
    if mode == airk_core::verification::VerificationMode::Require {
        body["error"] = json!({"code":"verification_deadline_exhausted","retryable":false,"query_submitted":false,"verification_performed":false,"query_replay_safe":false});
    }
    native_envelope(body)
}

fn source_projection_failure(result: &Value, observation: Option<Value>) -> Value {
    let mut body = canonical_body(result);
    body["source_projection_observation"]=observation.unwrap_or_else(||json!({"error":{"code":"source_projection_observation_unavailable","retryable":false,"query_submitted":null,"query_replay_safe":false}}));
    body["error"] = json!({"code":"source_projection_expired","retryable":false,"query_submitted":true,"query_replay_safe":false});
    native_envelope(body)
}

impl RemoteTools for Backend {
    fn combined_selection(&self) -> Option<&CombinedConnectRequest> {
        Some(&self.selection)
    }
    fn local_help(&self) -> bool {
        false
    }
    fn catalog(&self) -> Vec<airk_mcp::remote::Tool> {
        airk_mcp::combined::catalog()
    }
    fn query_concurrency(&self) -> usize {
        crate::service_resource_policy::query_concurrency(&self.control)
    }
    fn uses_response_mode(&self, tool: &str) -> bool {
        !matches!(tool, "airk_query_scope" | "query_scope")
    }
    fn client(&self, client: airk_core::usage::ClientFamily) {
        if let Ok(mut current) = self.client.lock() {
            *current = client;
        }
        if let Ok(children) = self.children.lock() {
            for child in children.values() {
                child.client(client);
            }
        }
    }
    fn close(&self) {
        self.closed
            .store(true, std::sync::atomic::Ordering::Release);
        if let Ok(children) = self.children.lock() {
            for child in children.values() {
                child.close();
            }
        }
    }
    fn shutdown(&self) -> airk_mcp::remote::ReadyFuture<'_> {
        Box::pin(async move {
            self.close();
            let children = self
                .children
                .lock()
                .map_err(|_| ScopeReadRefused)?
                .values()
                .cloned()
                .collect::<Vec<_>>();
            for child in children {
                child.shutdown().await?;
            }
            Ok(())
        })
    }
    fn call(&self, tool: String, args: Map<String, Value>) -> RemoteFuture<'_> {
        self.dispatch(
            tool,
            args,
            false,
            self.query_concurrency(),
            airk_mcp::read_attempt::ReadAttemptContext::new(
                Instant::now() + Duration::from_secs(125),
                Default::default(),
            ),
        )
    }
    fn call_with_capacity(
        &self,
        tool: String,
        args: Map<String, Value>,
        capacity: usize,
    ) -> RemoteFuture<'_> {
        self.dispatch(
            tool,
            args,
            false,
            capacity,
            airk_mcp::read_attempt::ReadAttemptContext::new(
                Instant::now() + Duration::from_secs(125),
                Default::default(),
            ),
        )
    }
    fn call_with_read_context(
        &self,
        tool: String,
        args: Map<String, Value>,
        capacity: usize,
        context: airk_mcp::read_attempt::ReadAttemptContext,
    ) -> RemoteFuture<'_> {
        self.dispatch(tool, args, false, capacity, context)
    }
}

impl Backend {
    fn dispatch(
        &self,
        tool: String,
        mut args: Map<String, Value>,
        internal_allowed: bool,
        capacity: usize,
        context: airk_mcp::read_attempt::ReadAttemptContext,
    ) -> RemoteFuture<'_> {
        Box::pin(async move {
            let dispatch_started = Instant::now();
            if self.closed.load(std::sync::atomic::Ordering::Acquire) {
                return Ok(failure(
                    "combined_admission_closed",
                    Some(false),
                    &self.selection,
                ));
            }
            let internal_verification = tool == "__airk_verify_projection";
            if internal_verification && !internal_allowed {
                return Ok(failure(
                    "unknown_combined_tool",
                    Some(false),
                    &self.selection,
                ));
            }
            if args.contains_key("__airk_plan_remaining_ms") && !internal_allowed {
                return Ok(failure(
                    "invalid_combined_argument",
                    Some(false),
                    &self.selection,
                ));
            }
            let remaining_deadline_ms = args
                .remove("__airk_plan_remaining_ms")
                .map(serde_json::from_value::<u64>)
                .transpose()?;
            if remaining_deadline_ms == Some(0) {
                return Ok(failure(
                    "combined_plan_deadline_exhausted",
                    Some(false),
                    &self.selection,
                ));
            }
            let native = if tool.starts_with("airk_") || internal_verification {
                tool
            } else {
                format!("airk_{tool}")
            };
            if native == "airk_query_scope"
                && !internal_allowed
                && [
                    "project_ids",
                    "index_ids",
                    "member_availability",
                    "origin_publication",
                ]
                .iter()
                .any(|key| args.contains_key(*key))
            {
                return Ok(failure(
                    "invalid_query_scope_arguments",
                    Some(false),
                    &self.selection,
                ));
            }
            let selected = match self.request_selection(&mut args) {
                Ok(selected) => selected,
                Err(_) => {
                    return Ok(failure(
                        "invalid_combined_selection",
                        Some(false),
                        &self.selection,
                    ));
                }
            };
            let before = match snapshot(&self.control, &self.selection, &selected).await {
                Ok(before) => before,
                Err(_) => {
                    return Ok(failure(
                        "combined_scope_unavailable",
                        Some(false),
                        &self.selection,
                    ));
                }
            };
            if internal_verification
                && let Some(remaining) = args.get("remaining_deadline_ms").and_then(Value::as_u64)
            {
                args.insert(
                    "remaining_deadline_ms".into(),
                    json!(
                        remaining.saturating_sub(
                            dispatch_started
                                .elapsed()
                                .as_millis()
                                .min(u128::from(u64::MAX)) as u64
                        )
                    ),
                );
            }
            let observation_budget = if native == "airk_status" { 2000 } else { 30000 };
            let deadline = context.deadline().min(
                dispatch_started
                    + Duration::from_millis(
                        remaining_deadline_ms
                            .unwrap_or(observation_budget)
                            .min(observation_budget),
                    ),
            );
            let remaining_deadline_ms = Some(
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    .min(u128::from(u64::MAX)) as u64,
            );
            if remaining_deadline_ms == Some(0) {
                return Ok(failure(
                    "combined_plan_deadline_exhausted",
                    Some(false),
                    &self.selection,
                ));
            }
            if native == "airk_help" {
                return Ok(serde_json::to_value(airk_mcp::help::call_with_catalog(
                    Value::Object(args),
                    airk_mcp::combined::catalog(),
                ))?);
            }
            let result = match native.as_str() {
                "airk_query_scope" => self.discover(args, &selected, before, deadline).await,
                "airk_parse" => {
                    self.parse(
                        args,
                        &selected,
                        before,
                        deadline,
                        capacity,
                        context.narrowed(deadline),
                    )
                    .await
                }
                "__airk_verify_projection" => {
                    self.verify_projection(args, &selected, before, context.narrowed(deadline))
                        .await
                }
                _ => {
                    self.execute_read(
                        &native,
                        args,
                        &selected,
                        before,
                        ReadAdmission {
                            remaining_deadline_ms,
                            capacity,
                            context: context.narrowed(deadline),
                        },
                    )
                    .await
                }
            };
            match result {
                Ok(result) => Ok(result),
                Err(error) if error.is::<airk_kernel::combined_reads::CombinedReplyBudget>() => Ok(
                    native_envelope(airk_kernel::combined_reads::CombinedReplyBudget.receipt()),
                ),
                Err(error) if error.is::<ScopeReadRefused>() => {
                    Ok(failure("combined_scope_expired", None, &self.selection))
                }
                Err(_) => Ok(failure("combined_read_unavailable", None, &self.selection)),
            }
        })
    }
}
