// SPDX-License-Identifier: Apache-2.0
//! Normal catalog transactions, separate from the upgrade-stage import adapter.
use super::{ServiceRegistry, state, validate_existing};
use airk_core::{
    catalog_management::*,
    grant_provenance::{
        GrantProvenance, PersistedGrant, is_imported_grant_id, native_imported_grant_id,
    },
    identity::{Incarnation, Sha256Digest},
    index_catalog::*,
    service::{ProjectRegistration, RequestKey},
};
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

#[path = "service_catalog_fences.rs"]
mod fences;
pub use fences::{
    CatalogFenceAbsence, CatalogFenceProof, CatalogFenceUnpreparedAbsence, CatalogSavedObservation,
    FenceConfirmation, FenceConfirmationMethod, FenceIntent, FenceObservation, FencePlan,
    FencePreparation, FencePrepareRejected, FenceStage, FenceTarget, FencedReceipt,
    WorkerDrainObligation, WorkerFenceConfirmation, WorkerFenceConfirmationMethod,
    WorkerProviderHistory, WorkerProviderObligation,
};

/// Canonical native operation identity; calculating this does not reserve a receipt.
pub fn catalog_operation_id(caller: Incarnation, key: &RequestKey) -> Result<Incarnation> {
    operation_id(caller, key)
}

fn revision(value: i64) -> Result<CatalogRevision> {
    u64::try_from(value)?.try_into()
}
fn number(value: CatalogRevision) -> i64 {
    i64::try_from(value.get()).expect("CatalogRevision validates the signed SQLite range")
}
fn ready(db: &Connection) -> Result<super::CatalogState> {
    validate_existing(db)?;
    let current = state(db)?;
    ensure!(
        current.activated,
        "normal catalog management requires qualified activation"
    );
    Ok(current)
}
fn operation_id(caller: Incarnation, key: &RequestKey) -> Result<Incarnation> {
    airk_core::catalog_management::catalog_operation_id(caller, key)
}
fn saved(db: &Connection, caller: Incarnation, key: &RequestKey) -> Result<Option<CatalogReceipt>> {
    if let Some(observation) = fences::observation(db, caller, key)? {
        return match observation {
            FenceObservation::Applied(receipt) => Ok(Some(receipt.metadata)),
            FenceObservation::Pending(_) => {
                anyhow::bail!("catalog mutation awaits its native authority fence")
            }
        };
    }
    let payload: Option<Vec<u8>> = db.query_row("SELECT substr(payload,1,16385) FROM index_operation_receipts WHERE id=?1 AND kind='catalog_management'",[operation_id(caller,key)?.to_string()],|r|r.get(0)).optional()?;
    payload
        .map(|bytes| {
            ensure!(
                bytes.len() <= MAX_RECEIPT_BYTES,
                "catalog receipt exceeds bound"
            );
            let receipt: CatalogReceipt = serde_json::from_slice(&bytes)?;
            ensure!(
                receipt.schema == 1
                    && receipt.caller == caller
                    && receipt.key == *key
                    && receipt.operation_id == operation_id(caller, key)?,
                "catalog receipt identity conflict"
            );
            Ok(receipt)
        })
        .transpose()
}
fn current_key(db: &Connection, key: &RequestKey) -> Result<()> {
    let (namespace, epoch): (String, i64) = db.query_row(
        "SELECT incarnation,request_epoch FROM service_meta WHERE id=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    ensure!(
        namespace == key.namespace.to_string() && u64::try_from(epoch)? == key.epoch,
        "catalog request key expired"
    );
    Ok(())
}
fn strings(db: &Connection, sql: &str, id: &str) -> Result<Vec<String>> {
    let mut stmt = db.prepare(sql)?;
    Ok(stmt
        .query_map([id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?)
}
fn scopes(
    db: &Connection,
    project: &str,
    index: &str,
    grant: Option<&str>,
) -> Result<Vec<ScopeAtom>> {
    let bound: i64 = if let Some(g) = grant {
        db.query_row("SELECT coalesce(sum(length(CAST(path AS BLOB))*6+160),0) FROM index_access_scopes WHERE project_id=?1 AND index_id=?2 AND grant_id=?3",params![project,index,g],|r|r.get(0))?
    } else {
        db.query_row("SELECT coalesce(sum(length(CAST(path AS BLOB))*6+160),0) FROM project_index_scopes WHERE project_id=?1 AND index_id=?2",params![project,index],|r|r.get(0))?
    };
    ensure!(
        bound <= i64::try_from(MAX_PAGE_BYTES)?,
        "catalog scope exceeds conservative encoded record budget"
    );
    let (sql, args) = if let Some(g) = grant {
        (
            "SELECT source_id,kind,path FROM index_access_scopes WHERE project_id=?1 AND index_id=?2 AND grant_id=?3 ORDER BY source_id,kind,path",
            vec![project, index, g],
        )
    } else {
        (
            "SELECT source_id,kind,path FROM project_index_scopes WHERE project_id=?1 AND index_id=?2 ORDER BY source_id,kind,path",
            vec![project, index],
        )
    };
    let mut stmt = db.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(args), |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    rows.map(|row| {
        let (root, kind, path) = row?;
        Ok(ScopeAtom {
            source_root_id: root.parse()?,
            kind: match kind.as_str() {
                "exact" => PathScope::Exact,
                "subtree" => PathScope::Subtree,
                _ => anyhow::bail!("invalid scope kind"),
            },
            path,
        })
    })
    .collect()
}
fn membership(db: &Connection, p: &str, i: &str) -> Result<Membership> {
    let rev = db.query_row(
        "SELECT revision FROM project_index_memberships WHERE project_id=?1 AND index_id=?2",
        params![p, i],
        |r| r.get(0),
    )?;
    let value = Membership {
        project_id: p.parse()?,
        index_id: i.parse()?,
        revision: revision(rev)?,
        scope: scopes(db, p, i, None)?,
    };
    value.validate()?;
    Ok(value)
}
fn item(db: &Connection, kind: CatalogKind, id: &str) -> Result<CatalogItem> {
    Ok(match kind {
        CatalogKind::Projects => {
            let (name,a,b,c):(String,i64,i64,i64)=db.query_row("SELECT name,definition_revision,scope_revision,defaults_revision FROM query_projects WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
            let value=QueryProject{id:id.parse()?,name,definition_revision:revision(a)?,scope_revision:revision(b)?,defaults_revision:revision(c)?,default_indexes:strings(db,"SELECT index_id FROM project_default_indexes WHERE project_id=?1 ORDER BY index_id",id)?.into_iter().map(|s|s.parse()).collect::<Result<_>>()?};
            value.validate()?;
            CatalogItem::Project(value)
        }
        CatalogKind::Indexes => {
            let (name,root,data,collection,a,b,epoch,digest,tombstone,import):(String,String,String,String,i64,i64,i64,Option<String>,bool,String)=db.query_row("SELECT name,backing_root,data_root,collection,definition_revision,source_revision,policy_epoch,policy_digest,tombstone,source_import_state FROM index_definitions WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?,r.get(9)?)))?;
            let value = IndexDefinition {
                id: id.parse()?,
                name,
                backing: ProjectRegistration::new(root, data, collection)?,
                definition_revision: revision(a)?,
                source_revision: revision(b)?,
                policy: if epoch == 0 {
                    None
                } else {
                    Some(PolicyReference {
                        epoch: revision(epoch)?,
                        digest: digest.context("missing policy digest")?.parse()?,
                    })
                },
                tombstone,
                source_import: match import.as_str() {
                    "complete" => SourceImportState::Complete,
                    "pending_metadata" => SourceImportState::PendingMetadata,
                    _ => anyhow::bail!("invalid source import"),
                },
            };
            value.validate()?;
            CatalogItem::Index(value)
        }
        CatalogKind::Memberships => {
            let (p, i) = id.split_once('/').context("invalid membership cursor")?;
            CatalogItem::Membership(membership(db, p, i)?)
        }
        CatalogKind::Contexts => {
            let (host, key, kind, name, rev): (String, String, String, String, i64) = db
                .query_row(
                    "SELECT host,context_key,kind,name,revision FROM index_contexts WHERE id=?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )?;
            let value=ContextChoices{id:id.parse()?,host:host.parse()?,key:key.parse()?,kind:serde_json::from_value(kind.into())?,name,revision:revision(rev)?,project_ids:strings(db,"SELECT project_id FROM context_projects WHERE context_id=?1 ORDER BY project_id",id)?.into_iter().map(|s|s.parse()).collect::<Result<_>>()?,index_ids:strings(db,"SELECT index_id FROM context_default_indexes WHERE context_id=?1 ORDER BY index_id",id)?.into_iter().map(|s|s.parse()).collect::<Result<_>>()?};
            value.validate()?;
            CatalogItem::Context(value)
        }
        CatalogKind::Grants => {
            let bound:i64=db.query_row("SELECT (SELECT count(*)*400 FROM index_access_memberships WHERE grant_id=?1)+(SELECT coalesce(sum(length(CAST(path AS BLOB))*6+160),0) FROM index_access_scopes WHERE grant_id=?1)",[id],|r|r.get(0))?;
            ensure!(
                bound <= i64::try_from(MAX_PAGE_BYTES)?,
                "catalog grant exceeds conservative encoded record budget"
            );
            let (host, rev, revoked): (String, i64, bool) = db.query_row(
                "SELECT host,revision,revoked FROM index_access_grants WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            let mut stmt=db.prepare("SELECT project_id,index_id,membership_revision,source_revision,policy_epoch,policy_digest FROM index_access_memberships WHERE grant_id=?1 ORDER BY project_id,index_id")?;
            let mut memberships = Vec::new();
            for row in stmt.query_map([id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                    r.get::<_, Option<String>>(5)?,
                ))
            })? {
                let (p, i, m, s, epoch, digest) = row?;
                memberships.push(GrantedMembership {
                    membership: Membership {
                        project_id: p.parse()?,
                        index_id: i.parse()?,
                        revision: revision(m)?,
                        scope: scopes(db, &p, &i, Some(id))?,
                    },
                    source_revision: revision(s)?,
                    policy_epoch: u64::try_from(epoch)?,
                    policy_digest: digest.map(|s| s.parse()).transpose()?,
                });
            }
            let value = ReviewedGrant {
                id: id.parse()?,
                host: host.parse()?,
                revision: revision(rev)?,
                memberships,
                revoked,
            };
            value.validate()?;
            let authority: u32 = db.query_row(
                "SELECT authority_contract FROM index_catalog_meta WHERE id=1",
                [],
                |r| r.get(0),
            )?;
            let provenance = if authority == 0 {
                GrantProvenance::UserReviewedProjectGrant
            } else {
                let raw: (String, Option<String>, Option<String>, Option<String>, Option<String>) = db.query_row(
                    "SELECT provenance_kind,import_upgrade_id,import_plan_digest,import_legacy_binding,import_configuration_digest FROM index_access_grants WHERE id=?1",
                    [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
                match raw {
                    (kind, None, None, None, None) if kind == "reviewed" => {
                        GrantProvenance::UserReviewedProjectGrant
                    }
                    (kind, Some(upgrade), Some(plan), Some(binding), Some(config))
                        if kind == "imported_legacy" =>
                    {
                        let provenance = GrantProvenance::ImportedLegacySnapshot {
                            upgrade_id: upgrade.parse()?,
                            plan_digest: plan.parse()?,
                            legacy_binding: binding.parse()?,
                            configuration_digest: config.parse()?,
                        };
                        let namespace: String = db.query_row(
                            "SELECT incarnation FROM service_meta WHERE id=1",
                            [],
                            |r| r.get(0),
                        )?;
                        ensure!(
                            native_imported_grant_id(namespace.parse()?, value.host, &provenance)?
                                == value.id,
                            "imported grant identity does not match sealed lineage"
                        );
                        provenance
                    }
                    _ => anyhow::bail!("invalid persisted grant provenance"),
                }
            };
            let persisted = PersistedGrant {
                grant: value,
                provenance,
            };
            persisted.validate()?;
            CatalogItem::Grant(persisted)
        }
        CatalogKind::Sources => {
            let (index, path, owner, enabled): (String, String, bool, bool) = db.query_row(
                "SELECT index_id,canonical_path,owner,enabled FROM index_sources WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )?;
            let aliases = strings(
                db,
                "SELECT alias FROM index_source_aliases WHERE source_id=?1 ORDER BY alias",
                id,
            )?;
            ensure!(
                path.len() <= 32768
                    && !path.is_empty()
                    && aliases.len() <= 16
                    && aliases.iter().all(|a| !a.is_empty() && a.len() <= 128),
                "invalid catalog source metadata"
            );
            CatalogItem::Source(CatalogSource {
                id: id.parse()?,
                index_id: index.parse()?,
                canonical_path: path,
                owner,
                enabled,
                aliases,
            })
        }
    })
}
pub(super) fn read_persisted_grant(db: &Connection, id: GrantId) -> Result<PersistedGrant> {
    super::validate_catalog_layout(db)?;
    let CatalogItem::Grant(grant) = item(db, CatalogKind::Grants, &id.to_string())? else {
        anyhow::bail!("expected persisted grant");
    };
    Ok(grant)
}
fn target(op: &CatalogOperation) -> (CatalogKind, String) {
    match op {
        CatalogOperation::CreateProject { id, .. }
        | CatalogOperation::RenameProject { id, .. }
        | CatalogOperation::DeleteProject { id, .. } => (CatalogKind::Projects, id.to_string()),
        CatalogOperation::RenameIndex { id, .. }
        | CatalogOperation::ChangeIndexPolicy { id, .. }
        | CatalogOperation::BeginIndexSourceChange { id, .. }
        | CatalogOperation::CompleteIndexSourceChange { id, .. }
        | CatalogOperation::RetireIndex { id, .. } => (CatalogKind::Indexes, id.to_string()),
        CatalogOperation::SetMembership { membership, .. } => (
            CatalogKind::Memberships,
            format!("{}/{}", membership.project_id, membership.index_id),
        ),
        CatalogOperation::RemoveMembership {
            project_id,
            index_id,
            ..
        } => (CatalogKind::Memberships, format!("{project_id}/{index_id}")),
        CatalogOperation::SetProjectDefaults { project_id, .. } => {
            (CatalogKind::Projects, project_id.to_string())
        }
        CatalogOperation::SetContext { context, .. } => {
            (CatalogKind::Contexts, context.id.to_string())
        }
        CatalogOperation::DeleteContext { id, .. } => (CatalogKind::Contexts, id.to_string()),
        CatalogOperation::CreateGrant { grant } => (CatalogKind::Grants, grant.id.to_string()),
        CatalogOperation::RevokeGrant { id, .. } => (CatalogKind::Grants, id.to_string()),
    }
}
fn optional_item(db: &Connection, kind: CatalogKind, id: &str) -> Result<Vec<CatalogItem>> {
    match item(db, kind, id) {
        Ok(value) => Ok(vec![value]),
        Err(e)
            if e.downcast_ref::<rusqlite::Error>()
                .is_some_and(|e| matches!(e, rusqlite::Error::QueryReturnedNoRows)) =>
        {
            Ok(vec![])
        }
        Err(e) => Err(e),
    }
}
fn changed(count: usize) -> Result<()> {
    ensure!(count == 1, "catalog review is stale or target unavailable");
    Ok(())
}
pub(super) fn index_record(db: &Connection, index: IndexId) -> Result<IndexDefinition> {
    let CatalogItem::Index(value) = item(db, CatalogKind::Indexes, &index.to_string())? else {
        unreachable!()
    };
    Ok(value)
}
fn index_ready(db: &Connection, index: IndexId) -> Result<IndexDefinition> {
    let value = index_record(db, index)?;
    ensure!(
        !value.tombstone && value.source_import == SourceImportState::Complete,
        "index source metadata unavailable"
    );
    Ok(value)
}
fn roots_ready(db: &Connection, m: &Membership) -> Result<()> {
    index_ready(db, m.index_id)?;
    for atom in &m.scope {
        let enabled: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM index_sources WHERE id=?1 AND index_id=?2 AND enabled=1)",
            params![atom.source_root_id.to_string(), m.index_id.to_string()],
            |r| r.get(0),
        )?;
        ensure!(
            enabled,
            "scope source unavailable or belongs to another index"
        );
    }
    Ok(())
}
fn put_scopes(db: &Connection, m: &Membership, grant: Option<GrantId>) -> Result<()> {
    for atom in &m.scope {
        let kind = match atom.kind {
            PathScope::Exact => "exact",
            PathScope::Subtree => "subtree",
        };
        if let Some(g) = grant {
            db.execute(
                "INSERT INTO index_access_scopes VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    g.to_string(),
                    m.project_id.to_string(),
                    m.index_id.to_string(),
                    atom.source_root_id.to_string(),
                    kind,
                    atom.path
                ],
            )?;
        } else {
            db.execute(
                "INSERT INTO project_index_scopes VALUES(?1,?2,?3,?4,?5)",
                params![
                    m.project_id.to_string(),
                    m.index_id.to_string(),
                    atom.source_root_id.to_string(),
                    kind,
                    atom.path
                ],
            )?;
        }
    }
    Ok(())
}
/// Canonical current-reference removal; frozen grant snapshots are never rewritten.
pub(super) fn remove_membership_references(
    db: &Connection,
    p: &QueryProjectId,
    i: &IndexId,
    expected_scope_revision: &CatalogRevision,
    expected_membership_revision: &CatalogRevision,
) -> Result<()> {
    changed(db.execute("UPDATE query_projects SET scope_revision=?1,defaults_revision=defaults_revision+EXISTS(SELECT 1 FROM project_default_indexes WHERE project_id=?2 AND index_id=?3) WHERE id=?2 AND scope_revision=?4 AND (defaults_revision<9223372036854775807 OR NOT EXISTS(SELECT 1 FROM project_default_indexes WHERE project_id=?2 AND index_id=?3))",params![number(expected_scope_revision.next()?),p.to_string(),i.to_string(),number(*expected_scope_revision)])?)?;
    changed(db.execute(
        "DELETE FROM project_index_memberships WHERE project_id=?1 AND index_id=?2 AND revision=?3",
        params![
            p.to_string(),
            i.to_string(),
            number(*expected_membership_revision)
        ],
    )?)?;
    // Remove only defaults no longer backed by any selected Project.
    let exhausted:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM index_contexts WHERE revision=9223372036854775807 AND id IN(SELECT context_id FROM context_default_indexes WHERE index_id=?1 AND NOT EXISTS(SELECT 1 FROM context_projects cp JOIN project_index_memberships m ON m.project_id=cp.project_id WHERE cp.context_id=context_default_indexes.context_id AND m.index_id=context_default_indexes.index_id)))",[i.to_string()],|row|row.get(0))?;
    ensure!(!exhausted, "context revision exhausted");
    db.execute("UPDATE index_contexts SET revision=revision+1 WHERE id IN(SELECT context_id FROM context_default_indexes WHERE index_id=?1 AND NOT EXISTS(SELECT 1 FROM context_projects cp JOIN project_index_memberships m ON m.project_id=cp.project_id WHERE cp.context_id=context_default_indexes.context_id AND m.index_id=context_default_indexes.index_id))",[i.to_string()])?;
    db.execute("DELETE FROM context_default_indexes WHERE index_id=?1 AND NOT EXISTS(SELECT 1 FROM context_projects cp JOIN project_index_memberships m ON m.project_id=cp.project_id WHERE cp.context_id=context_default_indexes.context_id AND m.index_id=context_default_indexes.index_id)",[i.to_string()])?;
    Ok(())
}

fn retirement_memberships(
    db: &Connection,
    index: IndexId,
) -> Result<Vec<(QueryProjectId, CatalogRevision, CatalogRevision)>> {
    let mut statement = db.prepare("SELECT m.project_id,m.revision,p.scope_revision FROM project_index_memberships m JOIN query_projects p ON p.id=m.project_id WHERE m.index_id=?1 ORDER BY m.project_id LIMIT 1025")?;
    let rows = statement
        .query_map([index.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ensure!(
        rows.len() <= 1024,
        "retirement Project census exceeds bound"
    );
    rows.into_iter()
        .map(|(project, member, scope)| Ok((project.parse()?, revision(member)?, revision(scope)?)))
        .collect()
}

/// Complete current Project/default/context effect disclosure, subject to the
/// existing whole-preview byte budget. Refuse oversize; never select a subset.
pub(super) fn retirement_reference_targets(
    db: &Connection,
    index: IndexId,
) -> Result<Vec<(CatalogKind, String)>> {
    let mut targets = Vec::new();
    for (project, _, _) in retirement_memberships(db, index)? {
        targets.push((CatalogKind::Projects, project.to_string()));
        targets.push((CatalogKind::Memberships, format!("{project}/{index}")));
    }
    let contexts = strings(
        db,
        "SELECT context_id FROM context_default_indexes WHERE index_id=?1 ORDER BY context_id LIMIT 1025",
        &index.to_string(),
    )?;
    ensure!(
        contexts.len() <= 1024,
        "retirement context census exceeds bound"
    );
    for context in contexts {
        targets.push((CatalogKind::Contexts, context));
    }
    Ok(targets)
}

pub(super) fn detach_retired_index_references(db: &Connection, index: IndexId) -> Result<()> {
    for (project, member, scope) in retirement_memberships(db, index)? {
        remove_membership_references(db, &project, &index, &scope, &member)?;
    }
    super::source_change::require_no_retired_references(db, index)
}

pub(super) fn mutate(
    db: &Connection,
    request: &CatalogRequest,
    runtime: &std::path::Path,
) -> Result<()> {
    mutate_with_context(db, request, runtime, MutationContext::Commit)
}

/// Native implementation context only. Preview always runs inside the owning
/// savepoint and rolls back; it cannot attest a completed runtime fence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MutationContext {
    Commit,
    Preview,
}

fn mutate_with_context(
    db: &Connection,
    request: &CatalogRequest,
    runtime: &std::path::Path,
    context: MutationContext,
) -> Result<()> {
    let op = &request.operation;
    match op {
        CatalogOperation::ChangeIndexPolicy { .. } => super::policy::mutate(db,request,context)?,
        CatalogOperation::BeginIndexSourceChange {..}
        | CatalogOperation::CompleteIndexSourceChange {..}
        | CatalogOperation::RetireIndex {..} => super::source_change::mutate(db,request,runtime)?,
        CatalogOperation::CreateProject{id,name}=>{db.execute("INSERT INTO query_projects VALUES(?1,?2,1,1,1)",params![id.to_string(),name])?;}
        CatalogOperation::RenameProject{id,expected_definition_revision,name}=>changed(db.execute("UPDATE query_projects SET name=?1,definition_revision=?2 WHERE id=?3 AND definition_revision=?4",params![name,number(expected_definition_revision.next()?),id.to_string(),number(*expected_definition_revision)])?)?,
        CatalogOperation::RenameIndex{id,expected_definition_revision,name}=>changed(db.execute("UPDATE index_definitions SET name=?1,definition_revision=?2 WHERE id=?3 AND definition_revision=?4 AND tombstone=0",params![name,number(expected_definition_revision.next()?),id.to_string(),number(*expected_definition_revision)])?)?,
        CatalogOperation::DeleteProject{id,expected_definition_revision,expected_scope_revision,expected_defaults_revision}=>{
            // Context joins/defaults are preferences. Deleting a Project cannot leave
            // implicit defaults for a no-longer-selected Project.
            let contexts=strings(db,"SELECT context_id FROM context_projects WHERE project_id=?1 ORDER BY context_id",&id.to_string())?;
            changed(db.execute("DELETE FROM query_projects WHERE id=?1 AND definition_revision=?2 AND scope_revision=?3 AND defaults_revision=?4",params![id.to_string(),number(*expected_definition_revision),number(*expected_scope_revision),number(*expected_defaults_revision)])?)?;
            for c in contexts {
                db.execute("DELETE FROM context_default_indexes WHERE context_id=?1 AND NOT EXISTS(SELECT 1 FROM context_projects cp JOIN project_index_memberships m ON m.project_id=cp.project_id WHERE cp.context_id=context_default_indexes.context_id AND m.index_id=context_default_indexes.index_id)",[&c])?;
                changed(db.execute("UPDATE index_contexts SET revision=revision+1 WHERE id=?1 AND revision<9223372036854775807",[c])?)?;
            }
        }
        CatalogOperation::SetMembership{membership:m,expected_scope_revision,expected_membership_revision}=>{
            roots_ready(db,m)?;
            changed(db.execute("UPDATE query_projects SET scope_revision=?1 WHERE id=?2 AND scope_revision=?3",params![number(expected_scope_revision.next()?),m.project_id.to_string(),number(*expected_scope_revision)])?)?;
            if let Some(old)=expected_membership_revision {
                changed(db.execute("UPDATE project_index_memberships SET revision=?1 WHERE project_id=?2 AND index_id=?3 AND revision=?4",params![number(m.revision),m.project_id.to_string(),m.index_id.to_string(),number(*old)])?)?;
                db.execute("DELETE FROM project_index_scopes WHERE project_id=?1 AND index_id=?2",params![m.project_id.to_string(),m.index_id.to_string()])?;
            } else {db.execute("INSERT INTO project_index_memberships VALUES(?1,?2,?3)",params![m.project_id.to_string(),m.index_id.to_string(),number(m.revision)])?;}
            put_scopes(db,m,None)?;
        }
        CatalogOperation::RemoveMembership{project_id:p,index_id:i,expected_scope_revision,expected_membership_revision}=>{
            remove_membership_references(db,p,i,expected_scope_revision,expected_membership_revision)?;
        }
        CatalogOperation::SetProjectDefaults{project_id,expected_defaults_revision,index_ids}=>{
            changed(db.execute("UPDATE query_projects SET defaults_revision=?1 WHERE id=?2 AND defaults_revision=?3",params![number(expected_defaults_revision.next()?),project_id.to_string(),number(*expected_defaults_revision)])?)?;
            db.execute("DELETE FROM project_default_indexes WHERE project_id=?1",[project_id.to_string()])?;
            for id in index_ids {index_ready(db,*id)?;db.execute("INSERT INTO project_default_indexes VALUES(?1,?2)",params![project_id.to_string(),id.to_string()])?;}
        }
        CatalogOperation::SetContext{context:c,expected_revision}=>{
            let kind=serde_json::to_value(c.kind)?.as_str().context("invalid context kind")?.to_owned();
            if let Some(old)=expected_revision {
                changed(db.execute("UPDATE index_contexts SET kind=?1,name=?2,revision=?3 WHERE id=?4 AND revision=?5 AND host=?6 AND context_key=?7",params![kind,c.name,number(c.revision),c.id.to_string(),number(*old),c.host.to_string(),c.key.to_string()])?)?;
                db.execute("DELETE FROM context_projects WHERE context_id=?1",[c.id.to_string()])?;db.execute("DELETE FROM context_default_indexes WHERE context_id=?1",[c.id.to_string()])?;
            }else{db.execute("INSERT INTO index_contexts VALUES(?1,?2,?3,?4,?5,?6)",params![c.id.to_string(),c.host.to_string(),c.key.to_string(),kind,c.name,number(c.revision)])?;}
            for p in &c.project_ids {db.execute("INSERT INTO context_projects VALUES(?1,?2)",params![c.id.to_string(),p.to_string()])?;}
            for i in &c.index_ids {
                index_ready(db,*i)?;
                let allowed:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM context_projects cp JOIN project_index_memberships m ON m.project_id=cp.project_id WHERE cp.context_id=?1 AND m.index_id=?2)",params![c.id.to_string(),i.to_string()],|r|r.get(0))?;
                ensure!(allowed,"context default outside selected Projects");db.execute("INSERT INTO context_default_indexes VALUES(?1,?2)",params![c.id.to_string(),i.to_string()])?;
            }
        }
        CatalogOperation::DeleteContext{id,expected_revision}=>changed(db.execute("DELETE FROM index_contexts WHERE id=?1 AND revision=?2",params![id.to_string(),number(*expected_revision)])?)?,
        CatalogOperation::CreateGrant{grant:g}=>{
            ensure!(!is_imported_grant_id(g.id),"imported compatibility IDs are reserved for native cutover");
            // Validate the complete frozen review before inserting any grant row.
            for frozen in &g.memberships {
                let m=&frozen.membership;roots_ready(db,m)?;
                let current=membership(db,&m.project_id.to_string(),&m.index_id.to_string())?;
                ensure!(current.revision==m.revision && m.scope_is_subset_of(&current),"grant membership review stale or scope widened");
                let index=index_ready(db,m.index_id)?;
                ensure!(index.source_revision==frozen.source_revision && index.policy.as_ref().map_or(0,|p|p.epoch.get())==frozen.policy_epoch && index.policy.as_ref().map(|p|p.digest)==frozen.policy_digest,"grant source or policy review stale");
                if index.policy.is_some() {
                    ensure!(super::policy::snapshot(db,m.index_id)?.is_some_and(|policy|policy.application.is_some()),"grant awaits the admitted source policy removal publication");
                }
            }
            db.execute("INSERT INTO index_access_grants(id,host,revision,revoked) VALUES(?1,?2,1,0)",params![g.id.to_string(),g.host.to_string()])?;
            for f in &g.memberships {let m=&f.membership;db.execute("INSERT INTO index_access_memberships VALUES(?1,?2,?3,?4,?5,?6,?7)",params![g.id.to_string(),m.project_id.to_string(),m.index_id.to_string(),number(m.revision),number(f.source_revision),i64::try_from(f.policy_epoch)?,f.policy_digest.map(|d|d.to_string())])?;put_scopes(db,m,Some(g.id))?;}
        }
        CatalogOperation::RevokeGrant{id,expected_revision}=>changed(db.execute("UPDATE index_access_grants SET revoked=1,revision=?1 WHERE id=?2 AND revision=?3 AND revoked=0",params![number(expected_revision.next()?),id.to_string(),number(*expected_revision)])?)?,
    }
    Ok(())
}
fn fence(op: &CatalogOperation) -> bool {
    op.requires_authority_fence()
}
fn preview(
    db: &Connection,
    r: &CatalogRequest,
    runtime: &std::path::Path,
) -> Result<CatalogPreview> {
    preview_reserved(db, r, None, runtime)
}
fn preview_reserved(
    db: &Connection,
    r: &CatalogRequest,
    reserved: Option<Incarnation>,
    runtime: &std::path::Path,
) -> Result<CatalogPreview> {
    r.validate()?;
    let current = ready(db)?;
    // An already reserved exact native intent retains admission across request
    // epoch rotation. Public previews and new operations still require a live key.
    if reserved.is_none() {
        current_key(db, &r.key)?;
    } else {
        ensure!(
            current.namespace == r.key.namespace,
            "catalog fence namespace changed"
        );
    }
    if current.revision != r.expected_catalog_revision {
        if reserved.is_none() {
            anyhow::bail!(FencePrepareRejected::StaleReview);
        }
        anyhow::bail!("catalog review stale");
    }
    let receipts: i64 =
        db.query_row("SELECT count(*) FROM index_operation_receipts", [], |row| {
            row.get(0)
        })?;
    let owns_slot = if let Some(id) = reserved {
        db.query_row("SELECT EXISTS(SELECT 1 FROM index_operation_receipts WHERE id=?1 AND kind='catalog_management_fenced' AND stage='fenced')", [id.to_string()], |row| row.get::<_, bool>(0))?
    } else {
        false
    };
    if !(receipts < 1024 || receipts == 1024 && owns_slot) {
        if reserved.is_none() {
            anyhow::bail!(FencePrepareRejected::Capacity);
        }
        anyhow::bail!("catalog receipt capacity exceeded");
    }
    encoded_size(
        &CatalogReceipt {
            schema: 1,
            operation_id: operation_id(r.caller, &r.key)?,
            caller: r.caller,
            key: r.key.clone(),
            request_digest: r.identity()?,
            catalog_revision: current.revision.next()?,
            authority_fence_required: fence(&r.operation),
        },
        MAX_RECEIPT_BYTES,
    )?;
    let (kind, id) = target(&r.operation);
    let mut targets = vec![(kind, id)];
    if let CatalogOperation::RetireIndex { id, .. } = &r.operation {
        targets.extend(retirement_reference_targets(db, *id)?);
    }
    let project = match &r.operation {
        CatalogOperation::DeleteProject { id, .. } => Some(*id),
        CatalogOperation::SetMembership { membership, .. } => Some(membership.project_id),
        CatalogOperation::RemoveMembership { project_id, .. } => Some(*project_id),
        _ => None,
    };
    if let Some(p) = project {
        if kind != CatalogKind::Projects {
            targets.push((CatalogKind::Projects, p.to_string()));
        }
        let contexts = strings(
            db,
            "SELECT context_id FROM context_projects WHERE project_id=?1 ORDER BY context_id LIMIT 33",
            &p.to_string(),
        )?;
        ensure!(
            contexts.len() <= 32,
            "affected contexts exceed bounded review capacity"
        );
        for c in contexts {
            targets.push((CatalogKind::Contexts, c));
        }
        if matches!(r.operation, CatalogOperation::DeleteProject { .. }) {
            for id in strings(
                db,
                "SELECT project_id||'/'||index_id FROM project_index_memberships WHERE project_id=?1 ORDER BY index_id LIMIT 33",
                &p.to_string(),
            )? {
                targets.push((CatalogKind::Memberships, id));
            }
        }
    }
    let mut before = Vec::new();
    for (k, id) in &targets {
        before.extend(optional_item(db, *k, id)?);
        encoded_size(&before, MAX_PAGE_BYTES)?;
    }
    let source_policy_impact = super::policy::impact(db, r)?;
    // Savepoint rollback makes validation use the exact mutation path.
    db.execute_batch("SAVEPOINT catalog_preview")?;
    let result = (|| {
        mutate_with_context(db, r, runtime, MutationContext::Preview)?;
        let mut after = Vec::new();
        for (k, id) in &targets {
            after.extend(optional_item(db, *k, id)?);
            encoded_size(&after, MAX_PAGE_BYTES)?;
        }
        let result = CatalogPreview {
            request_digest: r.identity()?,
            catalog_revision: current.revision,
            next_revision: current.revision.next()?,
            operation: r.operation.clone(),
            before,
            after,
            authority_fence_required: fence(&r.operation),
            file_data_changes: matches!(r.operation, CatalogOperation::ChangeIndexPolicy { .. }),
            source_policy_impact,
        };
        result.identity()?;
        Ok(result)
    })();
    db.execute_batch("ROLLBACK TO catalog_preview; RELEASE catalog_preview")?;
    result
}
impl ServiceRegistry {
    pub fn catalog_receipt(
        &self,
        caller: Incarnation,
        key: &RequestKey,
    ) -> Result<Option<CatalogReceipt>> {
        ready(&self.connection)?;
        let result = saved(&self.connection, caller, key)?;
        if result.is_none() {
            current_key(&self.connection, key)?;
        }
        Ok(result)
    }
    pub fn catalog_preview(&mut self, request: &CatalogRequest) -> Result<CatalogPreview> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        preview(&tx, request, &self.root)
    }
    /// A retry returns its durable metadata receipt before stale-key/review checks.
    /// Native delivery fences remain the owning adapter's responsibility.
    pub fn catalog_apply(
        &mut self,
        request: &CatalogRequest,
        expected_preview_digest: Sha256Digest,
    ) -> Result<CatalogReceipt> {
        request.validate()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        ready(&tx)?;
        if let Some(receipt) = saved(&tx, request.caller, &request.key)? {
            ensure!(
                receipt.request_digest == request.identity()?,
                "catalog request key conflict"
            );
            return Ok(receipt);
        }
        ensure!(
            !matches!(
                request.operation,
                CatalogOperation::BeginIndexSourceChange { .. }
                    | CatalogOperation::CompleteIndexSourceChange { .. }
                    | CatalogOperation::RetireIndex { .. }
                    | CatalogOperation::ChangeIndexPolicy { .. }
            ),
            "source changes require their native fence/effect adapter"
        );
        fences::require_no_pending(&tx)?;
        let reviewed = preview(&tx, request, &self.root)?;
        ensure!(
            reviewed.identity()? == expected_preview_digest,
            "catalog preview changed"
        );
        let receipt = CatalogReceipt {
            schema: 1,
            operation_id: operation_id(request.caller, &request.key)?,
            caller: request.caller,
            key: request.key.clone(),
            request_digest: request.identity()?,
            catalog_revision: reviewed.next_revision,
            authority_fence_required: reviewed.authority_fence_required,
        };
        encoded_size(&receipt, MAX_RECEIPT_BYTES)?;
        let payload = serde_json::to_vec(&receipt)?;
        // Receipt capacity is reserved before applying metadata. No receipt is
        // evicted: old uncertain requests remain reconcilable or explicitly expire.
        tx.execute("INSERT INTO index_operation_receipts VALUES(?1,'catalog_management','metadata_applied',?2,?3)",params![receipt.operation_id.to_string(),number(receipt.catalog_revision),payload])?;
        mutate(&tx, request, &self.root)?;
        changed(tx.execute(
            "UPDATE index_catalog_meta SET revision=?1 WHERE id=1 AND revision=?2",
            params![
                number(receipt.catalog_revision),
                number(request.expected_catalog_revision)
            ],
        )?)?;
        tx.commit()?;
        Ok(receipt)
    }
    /// Exact administrative read, including lookup of a previously unseen
    /// window's saved preference by its immutable host/context-key pair.
    pub fn catalog_detail(
        &mut self,
        entity: &CatalogEntity,
        expected_catalog_revision: Option<CatalogRevision>,
    ) -> Result<CatalogDetail> {
        let tx = self.connection.transaction()?;
        let current = ready(&tx)?;
        ensure!(
            expected_catalog_revision.is_none_or(|r| r == current.revision),
            "catalog detail revision changed"
        );
        let target = match entity {
            CatalogEntity::Project { id } => Some((CatalogKind::Projects, id.to_string())),
            CatalogEntity::Index { id } => Some((CatalogKind::Indexes, id.to_string())),
            CatalogEntity::Membership {
                project_id,
                index_id,
            } => Some((CatalogKind::Memberships, format!("{project_id}/{index_id}"))),
            CatalogEntity::Context { id } => Some((CatalogKind::Contexts, id.to_string())),
            CatalogEntity::Grant { id } => Some((CatalogKind::Grants, id.to_string())),
            CatalogEntity::Source { id } => Some((CatalogKind::Sources, id.to_string())),
            CatalogEntity::ContextKey { host, key } => {
                let id: Option<String> = tx
                    .query_row(
                        "SELECT id FROM index_contexts WHERE host=?1 AND context_key=?2",
                        params![host.to_string(), key.to_string()],
                        |row| row.get(0),
                    )
                    .optional()?;
                id.map(|id| (CatalogKind::Contexts, id))
            }
        };
        let item = if let Some((kind, id)) = target {
            optional_item(&tx, kind, &id)?.pop()
        } else {
            None
        };
        let detail = CatalogDetail {
            namespace: current.namespace,
            revision: current.revision,
            item,
        };
        encoded_size(&detail, MAX_PAGE_BYTES)?;
        Ok(detail)
    }
    pub fn catalog_page(&mut self, request: &CatalogPageRequest) -> Result<CatalogPage> {
        request.validate()?;
        let tx = self.connection.transaction()?;
        let current = ready(&tx)?;
        ensure!(
            request
                .expected_catalog_revision
                .is_none_or(|r| r == current.revision),
            "catalog page revision changed"
        );
        if let Some(c) = &request.after {
            ensure!(
                c.namespace == current.namespace,
                "catalog continuation namespace changed"
            );
        }
        let sql = match request.kind {
            CatalogKind::Projects => {
                "SELECT id FROM query_projects WHERE id>?1 ORDER BY id LIMIT ?2"
            }
            CatalogKind::Indexes => {
                "SELECT id FROM index_definitions WHERE id>?1 ORDER BY id LIMIT ?2"
            }
            CatalogKind::Contexts => {
                "SELECT id FROM index_contexts WHERE id>?1 ORDER BY id LIMIT ?2"
            }
            CatalogKind::Grants => {
                "SELECT id FROM index_access_grants WHERE id>?1 ORDER BY id LIMIT ?2"
            }
            CatalogKind::Memberships => match (request.index_id, request.project_id) {
                (Some(_), Some(_)) => {
                    "SELECT project_id||'/'||index_id AS id FROM project_index_memberships WHERE index_id=?3 AND project_id=?4 AND id>?1 ORDER BY id LIMIT ?2"
                }
                (Some(_), None) => {
                    "SELECT project_id||'/'||index_id AS id FROM project_index_memberships WHERE index_id=?3 AND id>?1 ORDER BY id LIMIT ?2"
                }
                (None, Some(_)) => {
                    "SELECT project_id||'/'||index_id AS id FROM project_index_memberships WHERE project_id=?3 AND id>?1 ORDER BY id LIMIT ?2"
                }
                (None, None) => {
                    "SELECT project_id||'/'||index_id AS id FROM project_index_memberships WHERE id>?1 ORDER BY id LIMIT ?2"
                }
            },
            CatalogKind::Sources => {
                "SELECT id FROM index_sources WHERE id>?1 AND index_id=?3 ORDER BY id LIMIT ?2"
            }
        };
        let mut stmt = tx.prepare(sql)?;
        let after = request.after.as_ref().map_or("", |c| c.after.as_str());
        let maximum = i64::try_from(request.limit + 1)?;
        let mut args = vec![
            rusqlite::types::Value::Text(after.to_owned()),
            rusqlite::types::Value::Integer(maximum),
        ];
        if let Some(index) = request.index_id {
            args.push(rusqlite::types::Value::Text(index.to_string()));
        }
        if let Some(project) = request.project_id {
            args.push(rusqlite::types::Value::Text(project.to_string()));
        }
        let mut rows = stmt.query(rusqlite::params_from_iter(args))?;
        let mut ids = Vec::new();
        while let Some(row) = rows.next()? {
            ids.push(row.get::<_, String>(0)?);
        }
        let mut page = CatalogPage {
            namespace: current.namespace,
            revision: current.revision,
            items: Vec::new(),
            continuation: None,
        };
        for (position, id) in ids.iter().enumerate() {
            if position >= request.limit {
                break;
            }
            page.items.push(item(&tx, request.kind, id)?);
            page.continuation = Some(CatalogCursor {
                kind: request.kind,
                index_id: request.index_id,
                project_id: request.project_id,
                namespace: current.namespace,
                revision: current.revision,
                after: id.clone(),
            });
            if encoded_size(&page, request.max_bytes).is_err() {
                page.items.pop();
                ensure!(
                    !page.items.is_empty(),
                    "catalog item exceeds requested page byte budget"
                );
                page.continuation = Some(CatalogCursor {
                    kind: request.kind,
                    index_id: request.index_id,
                    project_id: request.project_id,
                    namespace: current.namespace,
                    revision: current.revision,
                    after: ids[position - 1].clone(),
                });
                break;
            }
        }
        if page.items.len() == ids.len() {
            page.continuation = None;
        }
        encoded_size(&page, request.max_bytes)?;
        Ok(page)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_details_filtered_memberships_and_context_key_lookup_are_bounded() -> Result<()> {
        let (_root, mut registry) = setup()?;
        let p1 = project();
        let p2 = QueryProjectId::from_identity(Incarnation::from_bytes([9; 16]));
        for p in [p1, p2] {
            registry.connection.execute(
                "INSERT INTO query_projects VALUES(?1,'Fixture',1,1,1)",
                [p.to_string()],
            )?;
        }
        let (i1, source) = seed_index(&registry, 20)?;
        let (i2, _) = seed_index(&registry, 22)?;
        for (p, i) in [(p1, i1), (p1, i2), (p2, i1)] {
            registry.connection.execute(
                "INSERT INTO project_index_memberships VALUES(?1,?2,1)",
                params![p.to_string(), i.to_string()],
            )?;
        }
        let request = CatalogPageRequest {
            kind: CatalogKind::Memberships,
            project_id: Some(p1),
            index_id: None,
            after: None,
            expected_catalog_revision: None,
            limit: 1,
            max_bytes: 4096,
        };
        let page = registry.catalog_page(&request)?;
        assert_eq!(page.items.len(), 1);
        assert!(page.continuation.is_some());
        let detail = registry.catalog_detail(
            &CatalogEntity::Membership {
                project_id: p1,
                index_id: i1,
            },
            Some(page.revision),
        )?;
        assert_eq!(detail.item.as_ref(), page.items.first());
        let next = CatalogPageRequest {
            after: page.continuation,
            expected_catalog_revision: Some(page.revision),
            ..request.clone()
        };
        assert_eq!(registry.catalog_page(&next)?.items.len(), 1);
        let mut rebound = next.clone();
        rebound.project_id = Some(p2);
        assert!(registry.catalog_page(&rebound).is_err());
        let by_index = CatalogPageRequest {
            project_id: None,
            index_id: Some(i1),
            limit: 32,
            ..request.clone()
        };
        assert_eq!(registry.catalog_page(&by_index)?.items.len(), 2);
        let pair = CatalogPageRequest {
            project_id: Some(p2),
            ..by_index
        };
        assert_eq!(registry.catalog_page(&pair)?.items.len(), 1);
        for entity in [
            CatalogEntity::Project { id: p1 },
            CatalogEntity::Index { id: i1 },
            CatalogEntity::Source { id: source },
        ] {
            assert!(registry.catalog_detail(&entity, None)?.item.is_some());
        }
        assert!(
            registry
                .catalog_detail(
                    &CatalogEntity::Project {
                        id: QueryProjectId::from_identity(Incarnation::from_bytes([90; 16]))
                    },
                    None
                )?
                .item
                .is_none()
        );
        let host = Sha256Digest::from_bytes([31; 32]);
        let key = Sha256Digest::from_bytes([32; 32]);
        let lookup = CatalogEntity::ContextKey { host, key };
        assert!(registry.catalog_detail(&lookup, None)?.item.is_none());
        let context = ContextChoices {
            id: ContextId::from_identity(Incarnation::from_bytes([30; 16])),
            host,
            key,
            kind: ContextKind::Workspace,
            name: "Saved window".into(),
            revision: CatalogRevision::INITIAL,
            project_ids: vec![p1],
            index_ids: vec![i1],
        };
        let r = request_for_context(&registry, context.clone())?;
        apply(&mut registry, &r)?;
        assert_eq!(
            registry.catalog_detail(&lookup, None)?.item,
            Some(CatalogItem::Context(context.clone()))
        );
        assert_eq!(
            registry
                .catalog_detail(&CatalogEntity::Context { id: context.id }, None)?
                .item,
            Some(CatalogItem::Context(context))
        );
        assert!(
            registry
                .catalog_detail(&lookup, Some(page.revision))
                .is_err()
        );
        Ok(())
    }
    fn request_for_context(
        registry: &ServiceRegistry,
        context: ContextChoices,
    ) -> Result<CatalogRequest> {
        request(
            registry,
            91,
            CatalogOperation::SetContext {
                context,
                expected_revision: None,
            },
        )
    }
    #[test]
    fn full_receipt_capacity_refuses_without_metadata_changes() -> Result<()> {
        let (_root, mut registry) = setup()?;
        for n in 0u128..1024 {
            registry.connection.execute("INSERT INTO index_operation_receipts VALUES(?1,'fixture','metadata_applied',1,X'7b7d')",[Incarnation::from_bytes(n.to_be_bytes()).to_string()])?;
        }
        let r = request(
            &registry,
            70,
            CatalogOperation::CreateProject {
                id: project(),
                name: "Capacity".into(),
            },
        )?;
        let before = state(&registry.connection)?;
        assert!(registry.catalog_preview(&r).is_err());
        assert!(
            registry
                .catalog_apply(&r, Sha256Digest::from_bytes([0; 32]))
                .is_err()
        );
        assert_eq!(state(&registry.connection)?, before);
        assert!(
            optional_item(
                &registry.connection,
                CatalogKind::Projects,
                &project().to_string()
            )?
            .is_empty()
        );
        Ok(())
    }
    #[test]
    fn source_pages_are_index_scoped_and_reject_records_larger_than_budget() -> Result<()> {
        let (_root, mut registry) = setup()?;
        let (index, source) = seed_index(&registry, 20)?;
        seed_index(&registry, 22)?;
        let request = CatalogPageRequest {
            kind: CatalogKind::Sources,
            index_id: Some(index),
            project_id: None,
            after: None,
            expected_catalog_revision: None,
            limit: 32,
            max_bytes: 1024,
        };
        let page = registry.catalog_page(&request)?;
        assert_eq!(page.items.len(), 1);
        assert!(page.continuation.is_none());
        let CatalogItem::Source(row) = &page.items[0] else {
            panic!()
        };
        assert_eq!(row.id, source);
        assert_eq!(row.index_id, index);
        // The immutable-root trigger intentionally protects existing identities.
        // A new fixture may carry a large original path, without opening that path.
        let huge = SourceRootId::from_identity(Incarnation::from_bytes([80; 16]));
        registry.connection.execute(
            "INSERT INTO index_sources VALUES(?1,?2,?3,0,1)",
            params![
                huge.to_string(),
                index.to_string(),
                format!("/{}", "x".repeat(2000))
            ],
        )?;
        let first = registry.catalog_page(&request)?;
        assert!(first.continuation.is_some());
        let next = CatalogPageRequest {
            after: first.continuation,
            expected_catalog_revision: Some(first.revision),
            ..request
        };
        assert!(registry.catalog_page(&next).is_err());
        Ok(())
    }
    fn seed_index(registry: &ServiceRegistry, n: u8) -> Result<(IndexId, SourceRootId)> {
        let index = IndexId::from_identity(Incarnation::from_bytes([n; 16]));
        let source = SourceRootId::from_identity(Incarnation::from_bytes([n + 1; 16]));
        let backing = ProjectRegistration::new(
            format!("/fixture/{n}"),
            format!("/fixture/data/{n}"),
            format!("index{n}"),
        )?;
        registry.connection.execute("INSERT INTO index_definitions VALUES(?1,?2,NULL,?3,?4,'collections-1.4',?5,'Fixture',1,1,0,NULL,0,'complete')",params![index.to_string(),backing.binding.to_string(),backing.root,backing.data_root,backing.collection])?;
        registry.connection.execute(
            "INSERT INTO index_sources VALUES(?1,?2,?3,1,1)",
            params![
                source.to_string(),
                index.to_string(),
                format!("/fixture/{n}")
            ],
        )?;
        Ok((index, source))
    }
    #[test]
    fn deleting_project_preserves_shared_and_independent_context_defaults() -> Result<()> {
        let (_root, mut registry) = setup()?;
        let p1 = project();
        let p2 = QueryProjectId::from_identity(Incarnation::from_bytes([9; 16]));
        for p in [p1, p2] {
            registry.connection.execute(
                "INSERT INTO query_projects VALUES(?1,'Fixture',1,1,1)",
                [p.to_string()],
            )?;
        }
        let (shared, _) = seed_index(&registry, 20)?;
        let (gone, _) = seed_index(&registry, 22)?;
        let (independent, _) = seed_index(&registry, 24)?;
        for (p, i) in [(p1, shared), (p1, gone), (p2, shared), (p2, independent)] {
            registry.connection.execute(
                "INSERT INTO project_index_memberships VALUES(?1,?2,1)",
                params![p.to_string(), i.to_string()],
            )?;
        }
        let context = ContextChoices {
            id: ContextId::from_identity(Incarnation::from_bytes([30; 16])),
            host: Sha256Digest::from_bytes([31; 32]),
            key: Sha256Digest::from_bytes([32; 32]),
            kind: ContextKind::Workspace,
            name: "Two Projects".into(),
            revision: CatalogRevision::INITIAL,
            project_ids: vec![p1, p2],
            index_ids: vec![shared, gone, independent],
        };
        let r = request(
            &registry,
            33,
            CatalogOperation::SetContext {
                context: context.clone(),
                expected_revision: None,
            },
        )?;
        apply(&mut registry, &r)?;
        let r = request(
            &registry,
            34,
            CatalogOperation::DeleteProject {
                id: p1,
                expected_definition_revision: CatalogRevision::INITIAL,
                expected_scope_revision: CatalogRevision::INITIAL,
                expected_defaults_revision: CatalogRevision::INITIAL,
            },
        )?;
        apply(&mut registry, &r)?;
        let CatalogItem::Context(saved) = item(
            &registry.connection,
            CatalogKind::Contexts,
            &context.id.to_string(),
        )?
        else {
            panic!()
        };
        assert_eq!(saved.project_ids, vec![p2]);
        assert_eq!(saved.index_ids, vec![shared, independent]);
        assert_eq!(saved.revision, CatalogRevision::INITIAL.next()?);
        assert_eq!(
            registry
                .connection
                .query_row("SELECT count(*) FROM index_definitions", [], |r| r
                    .get::<_, i64>(0))?,
            3
        );
        assert_eq!(
            registry
                .connection
                .query_row("SELECT count(*) FROM index_sources", [], |r| r
                    .get::<_, i64>(0))?,
            3
        );
        Ok(())
    }
    #[test]
    fn frozen_grants_allow_narrowing_and_refuse_sibling_widening() -> Result<()> {
        let (_root, mut registry) = setup()?;
        let (index, source) = seed_index(&registry, 20)?;
        registry.connection.execute(
            "INSERT INTO query_projects VALUES(?1,'Fixture',1,1,1)",
            [project().to_string()],
        )?;
        let base = Membership {
            project_id: project(),
            index_id: index,
            revision: CatalogRevision::INITIAL,
            scope: vec![ScopeAtom {
                source_root_id: source,
                kind: PathScope::Subtree,
                path: "src".into(),
            }],
        };
        let r = request(
            &registry,
            35,
            CatalogOperation::SetMembership {
                membership: base.clone(),
                expected_scope_revision: CatalogRevision::INITIAL,
                expected_membership_revision: None,
            },
        )?;
        apply(&mut registry, &r)?;
        let mut narrowed = base;
        narrowed.scope[0].path = "src/only".into();
        let grant = ReviewedGrant {
            id: GrantId::from_identity(Incarnation::from_bytes([40; 16])),
            host: Sha256Digest::from_bytes([41; 32]),
            revision: CatalogRevision::INITIAL,
            memberships: vec![GrantedMembership {
                membership: narrowed,
                source_revision: CatalogRevision::INITIAL,
                policy_epoch: 0,
                policy_digest: None,
            }],
            revoked: false,
        };
        let r = request(
            &registry,
            42,
            CatalogOperation::CreateGrant {
                grant: grant.clone(),
            },
        )?;
        apply(&mut registry, &r)?;
        let mut widened = grant.clone();
        widened.id = GrantId::from_identity(Incarnation::from_bytes([43; 16]));
        widened.memberships[0].membership.scope[0].path = "src-other".into();
        let r = request(
            &registry,
            44,
            CatalogOperation::CreateGrant { grant: widened },
        )?;
        assert!(registry.catalog_preview(&r).is_err());
        assert!(registry.catalog_receipt(r.caller, &r.key)?.is_none());
        let mut exact = grant.clone();
        exact.id = GrantId::from_identity(Incarnation::from_bytes([45; 16]));
        exact.memberships[0].membership.scope[0].kind = PathScope::Exact;
        let r = request(
            &registry,
            46,
            CatalogOperation::CreateGrant {
                grant: exact.clone(),
            },
        )?;
        apply(&mut registry, &r)?;
        let mut wrong = exact.clone();
        wrong.id = GrantId::from_identity(Incarnation::from_bytes([47; 16]));
        wrong.memberships[0].membership.scope[0].source_root_id =
            SourceRootId::from_identity(Incarnation::from_bytes([48; 16]));
        let r = request(
            &registry,
            49,
            CatalogOperation::CreateGrant { grant: wrong },
        )?;
        assert!(registry.catalog_preview(&r).is_err());
        registry.connection.execute("UPDATE project_index_scopes SET kind='exact',path='src/only' WHERE project_id=?1 AND index_id=?2",params![project().to_string(),index.to_string()])?;
        let mut descendants = exact;
        descendants.id = GrantId::from_identity(Incarnation::from_bytes([50; 16]));
        descendants.memberships[0].membership.scope[0].kind = PathScope::Subtree;
        let r = request(
            &registry,
            51,
            CatalogOperation::CreateGrant { grant: descendants },
        )?;
        assert!(registry.catalog_preview(&r).is_err());
        Ok(())
    }
    fn setup() -> Result<(tempfile::TempDir, ServiceRegistry)> {
        let root = tempfile::tempdir()?;
        let registry = ServiceRegistry::open(root.path(), Incarnation::from_bytes([1; 16]))?;
        let old = super::super::schema_digest(&registry.connection)?;
        super::super::install_schema(&registry.connection)?;
        registry.connection.execute(
            "INSERT INTO index_catalog_meta VALUES(1,1,1,0,0,0,?1,?2,?3)",
            params![
                Incarnation::from_bytes([2; 16]).to_string(),
                old.to_string(),
                Sha256Digest::from_bytes([3; 32]).to_string()
            ],
        )?;
        registry
            .connection
            .pragma_update(None, "user_version", 12)?;
        Ok((root, registry))
    }
    fn request(
        registry: &ServiceRegistry,
        nonce: u8,
        operation: CatalogOperation,
    ) -> Result<CatalogRequest> {
        let (namespace, epoch): (String, i64) = registry.connection.query_row(
            "SELECT incarnation,request_epoch FROM service_meta WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let namespace = namespace.parse()?;
        let epoch = u64::try_from(epoch)?;
        Ok(CatalogRequest {
            caller: Incarnation::from_bytes([5; 16]),
            key: RequestKey {
                namespace,
                epoch,
                nonce: Incarnation::from_bytes([nonce; 16]),
            },
            expected_catalog_revision: state(&registry.connection)?.revision,
            operation,
        })
    }
    fn project() -> QueryProjectId {
        QueryProjectId::from_identity(Incarnation::from_bytes([6; 16]))
    }
    fn apply(registry: &mut ServiceRegistry, request: &CatalogRequest) -> Result<CatalogReceipt> {
        let preview = registry.catalog_preview(request)?;
        registry.catalog_apply(request, preview.identity()?)
    }
    #[test]
    fn preview_rolls_back_write_barrier_and_apply_replays_after_delete_and_epoch_change()
    -> Result<()> {
        let (_root, mut registry) = setup()?;
        let create = request(
            &registry,
            7,
            CatalogOperation::CreateProject {
                id: project(),
                name: "Reviewed".into(),
            },
        )?;
        let original = state(&registry.connection)?;
        let preview = registry.catalog_preview(&create)?;
        assert!(preview.before.is_empty());
        assert_eq!(preview.after.len(), 1);
        assert_eq!(state(&registry.connection)?, original);
        assert!(
            optional_item(
                &registry.connection,
                CatalogKind::Projects,
                &project().to_string()
            )?
            .is_empty()
        );
        let receipt = registry.catalog_apply(&create, preview.identity()?)?;
        let delete = request(
            &registry,
            8,
            CatalogOperation::DeleteProject {
                id: project(),
                expected_definition_revision: CatalogRevision::INITIAL,
                expected_scope_revision: CatalogRevision::INITIAL,
                expected_defaults_revision: CatalogRevision::INITIAL,
            },
        )?;
        assert!(apply(&mut registry, &delete)?.authority_fence_required);
        registry.connection.execute(
            "UPDATE service_meta SET request_epoch=request_epoch+1 WHERE id=1",
            [],
        )?;
        assert_eq!(
            registry.catalog_receipt(create.caller, &create.key)?,
            Some(receipt.clone())
        );
        assert_eq!(
            registry.catalog_apply(&create, preview.identity()?)?,
            receipt
        );
        assert!(
            optional_item(
                &registry.connection,
                CatalogKind::Projects,
                &project().to_string()
            )?
            .is_empty()
        );
        let mut different = create.clone();
        different.operation = CatalogOperation::CreateProject {
            id: project(),
            name: "Changed".into(),
        };
        assert!(
            registry
                .catalog_apply(&different, preview.identity()?)
                .is_err()
        );
        Ok(())
    }
    #[test]
    fn stale_preview_and_definition_fences_leave_metadata_and_receipts_unchanged() -> Result<()> {
        let (_root, mut registry) = setup()?;
        let create = request(
            &registry,
            7,
            CatalogOperation::CreateProject {
                id: project(),
                name: "Original".into(),
            },
        )?;
        apply(&mut registry, &create)?;
        let rename = request(
            &registry,
            8,
            CatalogOperation::RenameProject {
                id: project(),
                expected_definition_revision: CatalogRevision::INITIAL,
                name: "Renamed".into(),
            },
        )?;
        let preview = registry.catalog_preview(&rename)?;
        assert!(
            registry
                .catalog_apply(&rename, Sha256Digest::from_bytes([0; 32]))
                .is_err()
        );
        assert!(
            registry
                .catalog_receipt(rename.caller, &rename.key)?
                .is_none()
        );
        apply(&mut registry, &rename)?;
        assert!(registry.catalog_apply(&create, preview.identity()?).is_ok());
        let stale = request(
            &registry,
            9,
            CatalogOperation::RenameProject {
                id: project(),
                expected_definition_revision: CatalogRevision::INITIAL,
                name: "Stale".into(),
            },
        )?;
        let before = state(&registry.connection)?;
        assert!(registry.catalog_preview(&stale).is_err());
        assert_eq!(state(&registry.connection)?, before);
        let CatalogItem::Project(p) = item(
            &registry.connection,
            CatalogKind::Projects,
            &project().to_string(),
        )?
        else {
            panic!()
        };
        assert_eq!(p.name, "Renamed");
        assert_eq!(p.scope_revision, CatalogRevision::INITIAL);
        assert_eq!(p.defaults_revision, CatalogRevision::INITIAL);
        Ok(())
    }
    #[test]
    fn catalog_pages_are_bounded_and_continuations_bind_revision_and_kind() -> Result<()> {
        let (_root, mut registry) = setup()?;
        for n in 10..13 {
            let r = request(
                &registry,
                n,
                CatalogOperation::CreateProject {
                    id: QueryProjectId::from_identity(Incarnation::from_bytes([n; 16])),
                    name: format!("Project {n}"),
                },
            )?;
            apply(&mut registry, &r)?;
        }
        let first = CatalogPageRequest {
            kind: CatalogKind::Projects,
            index_id: None,
            project_id: None,
            after: None,
            expected_catalog_revision: None,
            limit: 1,
            max_bytes: 1024,
        };
        let page = registry.catalog_page(&first)?;
        assert_eq!(page.items.len(), 1);
        assert!(page.continuation.is_some());
        let next = CatalogPageRequest {
            after: page.continuation,
            expected_catalog_revision: Some(page.revision),
            ..first
        };
        let page2 = registry.catalog_page(&next)?;
        assert_ne!(page.items, page2.items);
        let mut forged = next.clone();
        forged.kind = CatalogKind::Indexes;
        assert!(registry.catalog_page(&forged).is_err());
        let r = request(
            &registry,
            14,
            CatalogOperation::CreateProject {
                id: project(),
                name: "Later".into(),
            },
        )?;
        apply(&mut registry, &r)?;
        assert!(registry.catalog_page(&next).is_err());
        Ok(())
    }
}
