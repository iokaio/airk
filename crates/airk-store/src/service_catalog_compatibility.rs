// SPDX-License-Identifier: Apache-2.0
//! Owned post-readiness compatibility sealing. Ordinary reads never create seals.
use super::*;
use airk_core::{
    grant_provenance::{
        GrantProvenance, MAX_IMPORTED_COMPAT_GRANTS, PersistedGrant, native_imported_grant_id,
    },
    index_catalog::{
        CatalogRevision, GrantedMembership, Membership, PathScope, ReviewedGrant, ScopeAtom,
    },
    scoped_reads::ScopeReadRefused,
    service::ProjectBinding,
};
use rusqlite::OptionalExtension;

#[path = "service_catalog_v1_settlement.rs"]
pub mod activation;

/// Read-only proof used by owned resume and the actual candidate probe. It
/// validates existing rows only; absent or revoked imports are never resealed.
pub(super) fn verify_recovered_authority_v1(
    root: &Path,
    receipt: &UpgradeReceipt,
) -> Result<LegacyPromotionObservation> {
    let native_host = receipt
        .authority_v1_host
        .context("owned compatibility host intent missing")?;
    let read = registry_read(root)?;
    super::super::validate_existing(&read)?;
    let state = super::super::state(&read)?;
    ensure!(
        state.namespace == receipt.plan.namespace
            && state.upgrade_id == receipt.plan.id
            && state.authority_contract == 1
            && state.policy_contract == 0,
        "recovered compatibility format/identity changed"
    );
    let prior = receipt
        .readiness
        .as_ref()
        .context("saved P5 readiness missing")?;
    prior.validate(receipt)?;
    ensure!(
        state.revision == prior.candidate.catalog.revision.next()? && !state.accepted_new_writes,
        "recovered compatibility revision/write fence changed"
    );
    if state.activated {
        let record = receipt
            .authority_v1
            .as_ref()
            .context("activated compatibility lacks native qualification")?;
        ensure!(
            record.phase == super::super::genesis::activation::AuthorityV1Phase::Activating,
            "activated compatibility phase changed"
        );
        super::super::genesis::activation::activated_identity(&state, record, false)?;
    }
    let attempt = receipt
        .backups
        .iter()
        .find(|b| b.complete)
        .context("qualified compatibility backup missing")?;
    super::super::backup::validate(root, receipt.plan.id, attempt)?;
    ensure!(
        Some(state.backup_digest) == attempt.sha256
            && Some(super::super::backup::legacy_data_digest(&read)?) == receipt.legacy_data_digest,
        "recovered compatibility original backup/data changed"
    );
    let mut expected = Vec::new();
    for index in &receipt.plan.indexes {
        let registered: bool = read.query_row(
            "SELECT registered FROM projects WHERE binding=?1",
            [index.registration.binding.to_string()],
            |r| r.get(0),
        )?;
        if !registered {
            continue;
        }
        let current = source_configuration(&index.registration);
        ensure!(
            current.source_import == SourceImportState::Complete
                && current.digest == index.configuration_digest,
            CompatibilityRecoveryRequired {
                binding: index.registration.binding,
                reason: CompatibilityRecoveryReason::ConfigurationChanged
            }
        );
        verify_original_sources(&read, index)?;
        expected.push(expected_grant(
            &receipt.plan,
            receipt.plan_digest,
            index,
            native_host,
        )?);
    }
    expected.sort_by_key(|g| g.grant.id);
    verify_sealed(&read, &expected)?;
    Ok(LegacyPromotionObservation {
        schema: 1,
        namespace: receipt.plan.namespace,
        upgrade_id: receipt.plan.id,
        plan_digest: receipt.plan_digest,
        catalog_revision: state.revision,
        imported_grants: expected.len(),
        seal_digest: framed_digest(
            b"airk/legacy-compatibility-seal/1",
            &[&serde_json::to_vec(&expected)?],
        ),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityRecoveryReason {
    ConfigurationUnavailable,
    ConfigurationChanged,
}
#[derive(Debug)]
pub struct CompatibilityRecoveryRequired {
    pub binding: ProjectBinding,
    pub reason: CompatibilityRecoveryReason,
}
impl std::fmt::Display for CompatibilityRecoveryRequired {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "legacy_compatibility_recovery_required: {} ({:?})",
            self.binding, self.reason
        )
    }
}
impl std::error::Error for CompatibilityRecoveryRequired {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LegacyPromotionObservation {
    pub schema: u32,
    pub namespace: Incarnation,
    pub upgrade_id: Incarnation,
    pub plan_digest: Sha256Digest,
    pub catalog_revision: CatalogRevision,
    pub imported_grants: usize,
    pub seal_digest: Sha256Digest,
}

fn expected_grant(
    plan: &UpgradePlan,
    plan_digest: Sha256Digest,
    index: &BackfillIndex,
    host: Sha256Digest,
) -> Result<PersistedGrant> {
    index.validate()?;
    let configuration_digest = index
        .configuration_digest
        .ok_or(CompatibilityRecoveryRequired {
            binding: index.registration.binding,
            reason: CompatibilityRecoveryReason::ConfigurationUnavailable,
        })?;
    ensure!(
        index.source_import == SourceImportState::Complete,
        CompatibilityRecoveryRequired {
            binding: index.registration.binding,
            reason: CompatibilityRecoveryReason::ConfigurationUnavailable,
        }
    );
    let provenance = GrantProvenance::ImportedLegacySnapshot {
        upgrade_id: plan.id,
        plan_digest,
        legacy_binding: index.registration.binding,
        configuration_digest,
    };
    let grant = ReviewedGrant {
        id: native_imported_grant_id(plan.namespace, host, &provenance)?,
        host,
        revision: CatalogRevision::INITIAL,
        revoked: false,
        memberships: vec![GrantedMembership {
            membership: Membership {
                project_id: index.project_id,
                index_id: index.index_id,
                revision: CatalogRevision::INITIAL,
                scope: index
                    .sources
                    .iter()
                    .map(|source| ScopeAtom {
                        source_root_id: source.id,
                        kind: PathScope::Subtree,
                        path: String::new(),
                    })
                    .collect(),
            },
            source_revision: CatalogRevision::INITIAL,
            policy_epoch: 0,
            policy_digest: None,
        }],
    };
    let persisted = PersistedGrant { grant, provenance };
    persisted.validate()?;
    airk_core::index_catalog::encoded_size(
        &persisted,
        airk_core::catalog_management::MAX_PAGE_BYTES,
    )?;
    Ok(persisted)
}

fn insert_import(db: &Connection, persisted: &PersistedGrant) -> Result<()> {
    persisted.validate()?;
    let GrantProvenance::ImportedLegacySnapshot {
        upgrade_id,
        plan_digest,
        legacy_binding,
        configuration_digest,
    } = &persisted.provenance
    else {
        anyhow::bail!("only native imported provenance can be sealed");
    };
    let g = &persisted.grant;
    db.execute("INSERT INTO index_access_grants(id,host,revision,revoked,provenance_kind,import_upgrade_id,import_plan_digest,import_legacy_binding,import_configuration_digest) VALUES(?1,?2,1,0,'imported_legacy',?3,?4,?5,?6)",
        params![g.id.to_string(),g.host.to_string(),upgrade_id.to_string(),plan_digest.to_string(),legacy_binding.to_string(),configuration_digest.to_string()])?;
    for frozen in &g.memberships {
        let m = &frozen.membership;
        db.execute(
            "INSERT INTO index_access_memberships VALUES(?1,?2,?3,?4,?5,0,NULL)",
            params![
                g.id.to_string(),
                m.project_id.to_string(),
                m.index_id.to_string(),
                i64::try_from(m.revision.get())?,
                i64::try_from(frozen.source_revision.get())?
            ],
        )?;
        for atom in &m.scope {
            ensure!(
                atom.kind == PathScope::Subtree && atom.path.is_empty(),
                "compatibility sealing requires the original whole-root atoms"
            );
            db.execute(
                "INSERT INTO index_access_scopes VALUES(?1,?2,?3,?4,'subtree','')",
                params![
                    g.id.to_string(),
                    m.project_id.to_string(),
                    m.index_id.to_string(),
                    atom.source_root_id.to_string()
                ],
            )?;
        }
    }
    Ok(())
}

fn verify_sealed(db: &Connection, expected: &[PersistedGrant]) -> Result<()> {
    let count: i64 = db.query_row("SELECT count(*) FROM index_access_grants", [], |r| r.get(0))?;
    ensure!(
        usize::try_from(count)? == expected.len(),
        "sealed compatibility rowset changed; resealing is forbidden"
    );
    for grant in expected {
        let actual = super::super::operations::read_persisted_grant(db, grant.grant.id)?;
        ensure!(
            &actual == grant,
            "sealed compatibility snapshot changed or was revoked; resealing is forbidden"
        );
    }
    Ok(())
}

fn verify_original_sources(db: &Connection, index: &BackfillIndex) -> Result<()> {
    let ready: bool = db.query_row("SELECT EXISTS(SELECT 1 FROM index_definitions i JOIN projects p ON p.binding=i.active_registration JOIN project_index_memberships m ON m.index_id=i.id AND m.project_id=?1 WHERE i.id=?2 AND p.registered=1 AND i.tombstone=0 AND m.revision=1)",
        params![index.project_id.to_string(),index.index_id.to_string()],|r|r.get(0))?;
    ensure!(
        ready,
        "sealed compatibility registration/membership changed"
    );
    let backing: (String,String,String,i64,i64,Option<String>,String) = db.query_row(
        "SELECT backing_root,data_root,collection,source_revision,policy_epoch,policy_digest,source_import_state FROM index_definitions WHERE id=?1 AND legacy_binding=?2",
        params![index.index_id.to_string(),index.registration.binding.to_string()],
        |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?)))?;
    ensure!(
        backing.0 == index.registration.root
            && backing.1 == index.registration.data_root
            && backing.2 == index.registration.collection
            && backing.3 == 1
            && backing.4 == 0
            && backing.5.is_none()
            && backing.6 == "complete",
        "sealed compatibility backing/source identity changed"
    );
    let mapped: (String, Option<String>) = db.query_row(
        "SELECT project_identity,project_id FROM legacy_index_projects WHERE legacy_binding=?1",
        [index.registration.binding.to_string()],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    ensure!(
        mapped.0 == index.project_id.to_string() && mapped.1.as_deref() == Some(mapped.0.as_str()),
        "sealed compatibility Project mapping changed"
    );
    let roots: i64 = db.query_row(
        "SELECT count(*) FROM index_sources WHERE index_id=?1",
        [index.index_id.to_string()],
        |r| r.get(0),
    )?;
    ensure!(
        usize::try_from(roots)? == index.sources.len(),
        "sealed compatibility source roster changed"
    );
    let scope_count: i64 = db.query_row(
        "SELECT count(*) FROM project_index_scopes WHERE project_id=?1 AND index_id=?2",
        params![index.project_id.to_string(), index.index_id.to_string()],
        |r| r.get(0),
    )?;
    ensure!(
        usize::try_from(scope_count)? == index.sources.len(),
        "sealed compatibility membership scope changed"
    );
    for source in &index.sources {
        let atom: bool=db.query_row("SELECT EXISTS(SELECT 1 FROM project_index_scopes WHERE project_id=?1 AND index_id=?2 AND source_id=?3 AND kind='subtree' AND path='')",
            params![index.project_id.to_string(),index.index_id.to_string(),source.id.to_string()],|r|r.get(0))?;
        ensure!(atom, "sealed compatibility membership atom changed");
        let stored: (String, bool, bool) = db.query_row(
            "SELECT canonical_path,owner,enabled FROM index_sources WHERE id=?1 AND index_id=?2",
            params![source.id.to_string(), index.index_id.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        ensure!(
            stored == (source.canonical_path.clone(), source.owner, true),
            "sealed compatibility source changed"
        );
        let mut stmt=db.prepare("SELECT alias FROM index_source_aliases WHERE source_id=?1 AND index_id=?2 ORDER BY alias")?;
        let aliases = stmt
            .query_map(
                params![source.id.to_string(), index.index_id.to_string()],
                |r| r.get::<_, String>(0),
            )?
            .collect::<rusqlite::Result<BTreeSet<_>>>()?;
        ensure!(
            aliases == source.aliases.iter().cloned().collect(),
            "sealed compatibility aliases changed"
        );
    }
    Ok(())
}

impl CatalogUpgrade {
    /// Bind the authenticated native host before candidate/predecessor probes.
    /// This only updates the existing owner intent; it creates no authority.
    /// The owner already retains service/helper/watcher/all-worker drain leases.
    pub fn bind_authority_v1_host(&self, native_host: Sha256Digest) -> Result<UpgradeReceipt> {
        let mut receipt = self.receipt()?;
        ensure!(
            matches!(
                receipt.stage,
                UpgradeStage::Imports | UpgradeStage::Readiness
            ),
            "native host binding requires completed inactive migration"
        );
        if let Some(host) = receipt.authority_v1_host {
            ensure!(
                host == native_host,
                "compatibility seal native host changed"
            );
        }
        self.qualified_backup(&receipt)?;
        let read = registry_read(&self.root)?;
        Self::validate_backfill(&self.root, self.namespace, &read, &receipt)?;
        let state = super::super::state(&read)?;
        ensure!(
            state.authority_contract == 0
                && !state.activated
                && !state.accepted_new_writes
                && state.policy_contract == 0,
            "native host binding requires inactive authority 0"
        );
        // Require the same actual, complete native source configuration and
        // original inventory used by sealing. Unknown inputs never get guessed.
        self.expected_imports(&read, &receipt, native_host)?;
        if receipt.authority_v1_host.is_none() {
            receipt.authority_v1_host = Some(native_host);
            self.save(&receipt)?;
        }
        Ok(receipt)
    }
    fn expected_imports(
        &self,
        read: &Connection,
        receipt: &UpgradeReceipt,
        native_host: Sha256Digest,
    ) -> Result<Vec<PersistedGrant>> {
        let mut expected = Vec::new();
        for index in &receipt.plan.indexes {
            let registered: bool = read.query_row(
                "SELECT registered FROM projects WHERE binding=?1",
                [index.registration.binding.to_string()],
                |r| r.get(0),
            )?;
            if !registered {
                continue;
            } // Removed bindings cannot regain access.
            let current = self.source_configuration(&index.registration)?;
            ensure!(
                current.source_import == SourceImportState::Complete
                    && index.source_import == SourceImportState::Complete,
                CompatibilityRecoveryRequired {
                    binding: index.registration.binding,
                    reason: CompatibilityRecoveryReason::ConfigurationUnavailable
                }
            );
            let aliases = index
                .sources
                .iter()
                .flat_map(|source| {
                    source
                        .aliases
                        .iter()
                        .map(|alias| (alias.clone(), source.canonical_path.clone()))
                })
                .collect::<BTreeMap<_, _>>();
            ensure!(
                current.digest == index.configuration_digest && current.aliases == aliases,
                CompatibilityRecoveryRequired {
                    binding: index.registration.binding,
                    reason: CompatibilityRecoveryReason::ConfigurationChanged
                }
            );
            verify_original_sources(read, index)?;
            expected.push(expected_grant(
                &receipt.plan,
                receipt.plan_digest,
                index,
                native_host,
            )?);
        }
        ensure!(
            expected.len() <= MAX_IMPORTED_COMPAT_GRANTS,
            "imported compatibility capacity exceeded"
        );
        expected.sort_by_key(|p| p.grant.id);
        airk_core::index_catalog::encoded_size(&expected, MAX_INTENT_BYTES)?;
        Ok(expected)
    }

    /// Native caller supplies its authenticated local OS host identity. This
    /// opaque owner retains service/helper/watcher/all original worker leases.
    /// P5's zero-grant readiness is validated before any contract-1 effect.
    pub fn promote_authority_v1(
        &self,
        native_host: Sha256Digest,
    ) -> Result<LegacyPromotionObservation> {
        ensure!(
            super::super::capabilities().supports(12, 1, 0),
            "native authority contract 1 is not qualified"
        );
        self.promote_authority_v1_at(native_host, |_, _| Ok(()))
    }
    fn promote_authority_v1_at(
        &self,
        native_host: Sha256Digest,
        checkpoint: impl Fn(&Connection, &str) -> Result<()>,
    ) -> Result<LegacyPromotionObservation> {
        let mut receipt = self.receipt()?;
        ensure!(
            receipt.stage == UpgradeStage::Readiness,
            "compatibility sealing requires saved P5 readiness"
        );
        if let Some(host) = receipt.authority_v1_host {
            ensure!(
                host == native_host,
                "compatibility seal native host changed"
            );
        }
        self.qualified_backup(&receipt)?;
        let evidence = receipt
            .readiness
            .clone()
            .context("saved candidate readiness missing")?;
        evidence.validate(&receipt)?;
        let read = registry_read(&self.root)?;
        super::super::validate_existing(&read)?;
        let state = super::super::state(&read)?;
        ensure!(
            !state.activated
                && !state.accepted_new_writes
                && state.namespace == self.namespace
                && state.upgrade_id == receipt.plan.id
                && state.policy_contract == 0,
            "authority promotion requires the original inactive owned catalog"
        );
        if state.authority_contract == 0 {
            Self::validate_backfill(&self.root, self.namespace, &read, &receipt)?;
            ensure!(
                observe(&self.root, receipt.plan.id, evidence.candidate.nonce)?
                    == evidence.candidate,
                "saved readiness changed before compatibility sealing"
            );
            receipt = self.bind_authority_v1_host(native_host)?;
        } else {
            ensure!(
                receipt.authority_v1_host == Some(native_host),
                "sealed compatibility host intent is missing"
            );
            ensure!(
                state.authority_contract == 1
                    && state.revision == evidence.candidate.catalog.revision.next()?,
                "sealed compatibility marker changed"
            );
            ensure!(
                Some(super::super::backup::legacy_data_digest(&read)?)
                    == receipt.legacy_data_digest,
                "legacy registry changed after compatibility sealing"
            );
        }
        let expected = self.expected_imports(&read, &receipt, native_host)?;
        let seal_digest = framed_digest(
            b"airk/legacy-compatibility-seal/1",
            &[&serde_json::to_vec(&expected)?],
        );
        let revision = evidence.candidate.catalog.revision.next()?;
        let observation = LegacyPromotionObservation {
            schema: 1,
            namespace: self.namespace,
            upgrade_id: receipt.plan.id,
            plan_digest: receipt.plan_digest,
            catalog_revision: revision,
            imported_grants: expected.len(),
            seal_digest,
        };
        airk_core::index_catalog::encoded_size(&observation, 16384)?;
        if state.authority_contract == 1 {
            verify_sealed(&read, &expected)?;
            return Ok(observation); // Existing seal recovery is read-only.
        }
        drop(read);
        crate::capacity::require_growth(&self.root, super::super::super::MAX_DATABASE_BYTES)?;
        let mut connection = Connection::open_with_flags(
            self.root.join(DATABASE),
            OpenFlags::SQLITE_OPEN_READ_WRITE,
        )?;
        connection.busy_timeout(Duration::from_millis(100))?;
        connection.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL; PRAGMA max_page_count=8192;",
        )?;
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = super::super::state(&tx)?;
        ensure!(
            current == state,
            "catalog changed before compatibility transaction"
        );
        super::super::apply_authority_v1_layout(&tx)?;
        for grant in &expected {
            insert_import(&tx, grant)?;
        }
        ensure!(tx.execute("UPDATE index_catalog_meta SET authority_contract=1,revision=?1 WHERE id=1 AND authority_contract=0 AND activated=0 AND accepted_new_writes=0",
            [i64::try_from(revision.get())?])?==1,"compatibility promotion marker changed");
        verify_sealed(&tx, &expected)?;
        super::super::validate_existing(&tx)?;
        checkpoint(&tx, "before_authority_commit")?;
        tx.commit()?;
        checkpoint(&connection, "after_authority_commit")?;
        Ok(observation)
    }
}

impl super::super::ServiceRegistry {
    /// Exact registered metadata only, with no filesystem inspection or claim.
    /// A missing row is a logical absence; SQLite/codec errors remain errors.
    pub fn legacy_binding_registration(
        &self,
        binding: ProjectBinding,
    ) -> Result<Option<ProjectRegistration>> {
        let row=self.connection.query_row(
            if self.registration_state {"SELECT substr(payload,1,20481),epoch,paused FROM projects WHERE binding=?1 AND registered=1"}
            else {"SELECT substr(payload,1,20481),epoch,paused FROM projects WHERE binding=?1"},
            [binding.to_string()],|row|Ok((row.get::<_,Vec<u8>>(0)?,row.get::<_,i64>(1)?,row.get::<_,bool>(2)?)),
        ).optional()?;
        row.map(|(payload, epoch, paused)| {
            let project = crate::service::registry::decode(&payload, epoch, paused)?;
            ensure!(
                project.registration.binding == binding,
                "foreign native legacy registration"
            );
            Ok(project.registration)
        })
        .transpose()
    }
    /// Exact metadata lookup only. Missing/revoked imports never create a grant
    /// or fall back to current Project defaults or unrestricted legacy access.
    pub fn legacy_compatibility_snapshot(
        &mut self,
        binding: ProjectBinding,
        host: Sha256Digest,
        expected_catalog_revision: CatalogRevision,
    ) -> Result<PersistedGrant> {
        let tx = self.connection.transaction()?;
        super::super::validate_existing(&tx)?;
        let state = super::super::state(&tx)?;
        ensure!(
            state.activated
                && state.authority_contract == 1
                && state.policy_contract <= super::super::SUPPORTED_POLICY_CONTRACT
                && state.revision == expected_catalog_revision,
            ScopeReadRefused
        );
        let id: String = tx.query_row("SELECT id FROM index_access_grants WHERE provenance_kind='imported_legacy' AND import_legacy_binding=?1 AND host=?2",
            params![binding.to_string(),host.to_string()], |r|r.get(0)).optional()?
            .ok_or(ScopeReadRefused)?;
        let snapshot = super::super::operations::read_persisted_grant(&tx, id.parse()?)?;
        snapshot.validate()?;
        ensure!(
            !snapshot.grant.revoked && snapshot.grant.host == host,
            ScopeReadRefused
        );
        let GrantProvenance::ImportedLegacySnapshot {
            upgrade_id,
            legacy_binding,
            ..
        } = &snapshot.provenance
        else {
            anyhow::bail!(ScopeReadRefused);
        };
        ensure!(
            *upgrade_id == state.upgrade_id && *legacy_binding == binding,
            ScopeReadRefused
        );
        let member = &snapshot.grant.memberships[0];
        let current: (String,Option<String>,i64,i64,bool,String,i64,Option<String>) = tx.query_row(
            "SELECT l.project_identity,l.project_id,m.revision,i.source_revision,i.tombstone,i.source_import_state,i.policy_epoch,i.policy_digest FROM legacy_index_projects l JOIN index_definitions i ON i.legacy_binding=l.legacy_binding JOIN project_index_memberships m ON m.project_id=l.project_id AND m.index_id=i.id JOIN projects p ON p.binding=i.active_registration AND p.registered=1 WHERE l.legacy_binding=?1 AND i.id=?2",
            params![binding.to_string(),member.membership.index_id.to_string()],
            |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?))).optional()?.ok_or(ScopeReadRefused)?;
        ensure!(
            current.0 == member.membership.project_id.to_string()
                && current.1.as_deref() == Some(current.0.as_str())
                && u64::try_from(current.2)? == member.membership.revision.get()
                && u64::try_from(current.3)? == member.source_revision.get()
                && !current.4
                && current.5 == "complete"
                && current.6 == 0
                && current.7.is_none(),
            ScopeReadRefused
        );
        for atom in &member.membership.scope {
            let enabled: Option<bool> = tx
                .query_row(
                    "SELECT enabled FROM index_sources WHERE id=?1 AND index_id=?2",
                    params![
                        atom.source_root_id.to_string(),
                        member.membership.index_id.to_string()
                    ],
                    |r| r.get(0),
                )
                .optional()?;
            ensure!(enabled == Some(true), ScopeReadRefused);
        }
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    // Physical normalized-storage fixtures only. No fake readiness is supplied
    // to the owned promotion API and no activation qualification is claimed.
    use super::*;
    use crate::service::registry::ServiceRegistry;
    use airk_core::index_catalog::{IndexId, QueryProjectId, SourceRootId};

    fn fixture() -> Result<(tempfile::TempDir, ServiceRegistry, UpgradePlan)> {
        let root = tempfile::tempdir()?;
        let namespace = Incarnation::from_bytes([1; 16]);
        let mut registry = ServiceRegistry::open(root.path(), namespace)?;
        let legacy = super::super::super::schema_digest(&registry.connection)?;
        let id = Incarnation::from_bytes([2; 16]);
        let registration = ProjectRegistration::new(
            "/compat/source".into(),
            "/compat/index".into(),
            "fixture".into(),
        )?;
        let index = BackfillIndex {
            registration: registration.clone(),
            index_id: IndexId::from_identity(id),
            project_id: QueryProjectId::from_identity(id),
            sources: vec![
                SourceRoot {
                    id: SourceRootId::from_identity(id),
                    canonical_path: registration.root.clone(),
                    aliases: Vec::new(),
                    owner: true,
                },
                SourceRoot {
                    id: SourceRootId::from_identity(Incarnation::from_bytes([3; 16])),
                    canonical_path: "/compat/external".into(),
                    aliases: vec!["external".into()],
                    owner: false,
                },
            ],
            source_import: SourceImportState::Complete,
            configuration_digest: Some(Sha256Digest::from_bytes([4; 32])),
        };
        let plan = UpgradePlan {
            schema: 1,
            id,
            control: root.path().canonicalize()?,
            namespace,
            installer_identity: Sha256Digest::from_bytes([5; 32]),
            legacy_schema_digest: legacy,
            registrations_digest: Sha256Digest::from_bytes([6; 32]),
            indexes: vec![index.clone()],
        };
        plan.validate()?;
        let tx = registry.connection.transaction()?;
        tx.execute(
            "INSERT INTO projects(binding,payload,epoch,paused,registered) VALUES(?1,?2,1,1,1)",
            params![
                registration.binding.to_string(),
                serde_json::to_vec(&registration)?
            ],
        )?;
        super::super::super::install_schema(&tx)?;
        tx.execute(
            "INSERT INTO index_catalog_meta VALUES(1,1,0,0,0,0,?1,?2,?3)",
            params![
                id.to_string(),
                legacy.to_string(),
                Sha256Digest::from_bytes([7; 32]).to_string()
            ],
        )?;
        tx.execute("INSERT INTO index_definitions VALUES(?1,?2,?2,?3,?4,'collections-1.4',?5,?5,1,1,0,NULL,0,'complete')",
            params![index.index_id.to_string(),registration.binding.to_string(),registration.root,registration.data_root,registration.collection])?;
        tx.execute(
            "INSERT INTO query_projects VALUES(?1,'Fixture',1,1,1)",
            [index.project_id.to_string()],
        )?;
        tx.execute(
            "INSERT INTO legacy_index_projects VALUES(?1,?2,?2)",
            params![
                registration.binding.to_string(),
                index.project_id.to_string()
            ],
        )?;
        tx.execute(
            "INSERT INTO project_index_memberships VALUES(?1,?2,1)",
            params![index.project_id.to_string(), index.index_id.to_string()],
        )?;
        for source in &index.sources {
            tx.execute(
                "INSERT INTO index_sources VALUES(?1,?2,?3,?4,1)",
                params![
                    source.id.to_string(),
                    index.index_id.to_string(),
                    source.canonical_path,
                    source.owner
                ],
            )?;
            for alias in &source.aliases {
                tx.execute(
                    "INSERT INTO index_source_aliases VALUES(?1,?2,?3)",
                    params![index.index_id.to_string(), source.id.to_string(), alias],
                )?;
            }
            tx.execute(
                "INSERT INTO project_index_scopes VALUES(?1,?2,?3,'subtree','')",
                params![
                    index.project_id.to_string(),
                    index.index_id.to_string(),
                    source.id.to_string()
                ],
            )?;
        }
        tx.pragma_update(None, "user_version", 12)?;
        tx.commit()?;
        Ok((root, registry, plan))
    }
    fn seal_fixture(registry: &mut ServiceRegistry, plan: &UpgradePlan) -> Result<PersistedGrant> {
        let expected = expected_grant(
            plan,
            plan.identity()?,
            &plan.indexes[0],
            Sha256Digest::from_bytes([8; 32]),
        )?;
        let tx = registry.connection.transaction()?;
        super::super::super::apply_authority_v1_layout(&tx)?;
        insert_import(&tx, &expected)?;
        tx.execute(
            "UPDATE index_catalog_meta SET authority_contract=1,revision=2 WHERE id=1",
            [],
        )?;
        verify_sealed(&tx, std::slice::from_ref(&expected))?;
        tx.commit()?;
        Ok(expected)
    }
    #[test]
    fn exact_original_external_sources_seal_without_shared_operation_receipts() -> Result<()> {
        let (_root, mut registry, plan) = fixture()?;
        let grant = seal_fixture(&mut registry, &plan)?;
        assert_eq!(grant.grant.memberships[0].membership.scope.len(), 2);
        assert!(
            grant.grant.memberships[0]
                .membership
                .scope
                .iter()
                .all(|atom| atom.kind == PathScope::Subtree && atom.path.is_empty())
        );
        verify_original_sources(&registry.connection, &plan.indexes[0])?;
        assert_eq!(
            registry.connection.query_row(
                "SELECT count(*) FROM index_operation_receipts",
                [],
                |r| r.get::<_, i64>(0)
            )?,
            0
        );
        assert!(!registry.connection.query_row(
            "SELECT activated FROM index_catalog_meta",
            [],
            |r| r.get::<_, bool>(0)
        )?);
        assert_eq!(
            super::super::super::operations::read_persisted_grant(
                &registry.connection,
                grant.grant.id
            )?,
            grant
        );
        Ok(())
    }
    #[test]
    fn rollback_keeps_exact_authority_zero_layout_and_no_grants() -> Result<()> {
        let (_root, mut registry, plan) = fixture()?;
        let expected = expected_grant(
            &plan,
            plan.identity()?,
            &plan.indexes[0],
            Sha256Digest::from_bytes([8; 32]),
        )?;
        {
            let tx = registry.connection.transaction()?;
            super::super::super::apply_authority_v1_layout(&tx)?;
            insert_import(&tx, &expected)?;
            tx.execute(
                "UPDATE index_catalog_meta SET authority_contract=1,revision=2 WHERE id=1",
                [],
            )?;
        }
        assert_eq!(
            super::super::super::validate_catalog_layout(&registry.connection)?,
            super::super::super::CatalogLayout::Schema12Authority0
        );
        assert_eq!(
            registry
                .connection
                .query_row("SELECT count(*) FROM index_access_grants", [], |r| r
                    .get::<_, i64>(0))?,
            0
        );
        Ok(())
    }
    #[test]
    fn committed_snapshot_is_exact_and_missing_or_revoked_records_never_reseal() -> Result<()> {
        for change in ["revoke", "remove"] {
            let (_root, mut registry, plan) = fixture()?;
            let expected = seal_fixture(&mut registry, &plan)?;
            verify_sealed(&registry.connection, std::slice::from_ref(&expected))?;
            if change == "revoke" {
                registry.connection.execute(
                    "UPDATE index_access_grants SET revoked=1,revision=2 WHERE id=?1",
                    [expected.grant.id.to_string()],
                )?;
            } else {
                registry.connection.execute(
                    "DELETE FROM index_access_grants WHERE id=?1",
                    [expected.grant.id.to_string()],
                )?;
            }
            assert!(verify_sealed(&registry.connection, std::slice::from_ref(&expected)).is_err());
            let count = registry.connection.query_row(
                "SELECT count(*) FROM index_access_grants",
                [],
                |r| r.get::<_, i64>(0),
            )?;
            assert_eq!(count, if change == "revoke" { 1 } else { 0 });
        }
        Ok(())
    }
    #[test]
    fn unknown_original_metadata_returns_explicit_recovery_requirement() -> Result<()> {
        let (_root, registry, mut plan) = fixture()?;
        plan.indexes[0].sources.truncate(1);
        plan.indexes[0].source_import = SourceImportState::PendingMetadata;
        plan.indexes[0].configuration_digest = None;
        let error = expected_grant(
            &plan,
            plan.identity()?,
            &plan.indexes[0],
            Sha256Digest::from_bytes([8; 32]),
        )
        .unwrap_err();
        assert!(
            error
                .downcast_ref::<CompatibilityRecoveryRequired>()
                .is_some()
        );
        assert_eq!(
            registry
                .connection
                .query_row("SELECT count(*) FROM index_access_grants", [], |r| r
                    .get::<_, i64>(0))?,
            0
        );
        Ok(())
    }
    #[test]
    fn sealed_lineage_and_original_source_changes_refuse_without_repair() -> Result<()> {
        let (_root, mut registry, plan) = fixture()?;
        let expected = seal_fixture(&mut registry, &plan)?;
        assert!(
            registry
                .connection
                .execute(
                    "UPDATE index_access_grants SET host=?1 WHERE id=?2",
                    params![
                        Sha256Digest::from_bytes([9; 32]).to_string(),
                        expected.grant.id.to_string()
                    ]
                )
                .is_err()
        );
        registry.connection.execute(
            "UPDATE index_sources SET enabled=0 WHERE id=?1",
            [plan.indexes[0].sources[1].id.to_string()],
        )?;
        assert!(verify_original_sources(&registry.connection, &plan.indexes[0]).is_err());
        Ok(())
    }
}
