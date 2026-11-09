// SPDX-License-Identifier: Apache-2.0
//! Native authority for an actual adopted standalone snapshot, never a reviewed
//! grant or a fabricated managed-upgrade import.
use super::*;
use airk_core::{
    catalog_legacy_adoption::LegacyAdoptionReceipt,
    catalog_legacy_authority::{StandaloneAdoptionProvenance, StandaloneConfigurationClaim},
    grant_provenance::{GrantProvenance, PersistedGrant, native_imported_grant_id},
    index_catalog::{
        GrantedMembership, Membership, PathScope, QueryProjectId, ReviewedGrant, ScopeAtom,
    },
};
use rusqlite::{TransactionBehavior, params};

/// The caller retains the actual origin capability as well as this actual
/// Current pin until the transaction commits. There is no wire constructor.
pub struct NativeStandaloneAuthority {
    native: setup::NativeLegacyAdoptionCompletion,
    receipt: LegacyAdoptionReceipt,
    provenance: StandaloneAdoptionProvenance,
}
impl NativeStandaloneAuthority {
    /// Metadata-only convenience for isolated tests; production supplies real claims.
    #[cfg(test)]
    pub fn from_completed_adoption(
        native: setup::NativeLegacyAdoptionCompletion,
        receipt: LegacyAdoptionReceipt,
        original_server: String,
        no_default_project: bool,
    ) -> Result<Self> {
        let plan = native.plan();
        let claims = vec![StandaloneConfigurationClaim {
            schema: 1,
            origin_sha256: plan.origin_sha256,
            config_sha256: plan.config_sha256,
            executable_sha256: plan.executable_sha256,
            selected_entry_sha256: Sha256Digest::from_bytes([99; 32]),
            container_path: vec!["mcp_servers".into()],
            original_server,
            no_default_project,
        }];
        Self::from_completed_adoption_claims(native, receipt, claims)
    }
    pub fn from_completed_adoption_claims(
        native: setup::NativeLegacyAdoptionCompletion,
        receipt: LegacyAdoptionReceipt,
        mut configuration_claims: Vec<StandaloneConfigurationClaim>,
    ) -> Result<Self> {
        receipt.validate()?;
        native.revalidate()?;
        ensure!(
            receipt.plan_digest == native.plan().identity()?
                && receipt.setup.request == native.plan().setup
                && receipt
                    .setup
                    .completion
                    .as_ref()
                    .and_then(|c| c.legacy_adoption.as_ref())
                    == Some(native.publication()),
            "standalone authority requires its exact native completed adoption"
        );
        configuration_claims.sort_by_key(|c| c.origin_sha256);
        let primary = configuration_claims
            .iter()
            .find(|c| c.origin_sha256 == native.plan().origin_sha256)
            .context("native primary configuration claim missing")?;
        let original_server = primary.original_server.clone();
        let no_default_project = primary.no_default_project;
        let provenance = StandaloneAdoptionProvenance {
            schema: 1,
            operation_id: receipt.setup.operation_id,
            plan: Box::new(native.plan().clone()),
            publication: native.publication().clone(),
            original_server,
            no_default_project,
            configuration_claims,
        };
        provenance.validate()?;
        Ok(Self {
            native,
            receipt,
            provenance,
        })
    }
    pub fn provenance(&self) -> &StandaloneAdoptionProvenance {
        &self.provenance
    }
    fn revalidate(&self) -> Result<()> {
        self.native.revalidate()?;
        self.receipt.validate()?;
        self.provenance.validate()?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthorityRecord {
    schema: u32,
    provenance: StandaloneAdoptionProvenance,
    receipt: LegacyAdoptionReceipt,
    project_id: QueryProjectId,
}
impl AuthorityRecord {
    fn validate(&self) -> Result<()> {
        self.provenance.validate()?;
        self.receipt.validate()?;
        ensure!(
            self.schema == 1
                && self.receipt.setup.operation_id == self.provenance.operation_id
                && self.receipt.setup.request == self.provenance.plan.setup
                && self.receipt.plan_digest == self.provenance.plan.identity()?
                && self
                    .receipt
                    .setup
                    .completion
                    .as_ref()
                    .and_then(|c| c.legacy_adoption.as_ref())
                    == Some(&self.provenance.publication)
                && self.project_id == project_id(&self.provenance)?,
            "crossed standalone adoption authority proof"
        );
        airk_core::index_catalog::encoded_size(self, 65536)?;
        Ok(())
    }
}
fn project_id(provenance: &StandaloneAdoptionProvenance) -> Result<QueryProjectId> {
    let digest = framed_digest(
        b"airk/standalone-adoption-project/1",
        &[provenance.identity()?.as_bytes()],
    );
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    Ok(QueryProjectId::from_identity(Incarnation::from_bytes(
        bytes,
    )))
}
fn canonical_source_scope(ids: &[airk_core::index_catalog::SourceRootId]) -> Vec<ScopeAtom> {
    // Persisted scope rows are read in source_id/kind/path order. Setup order
    // instead retains owner/alias provenance and is not a lexical ID ordering.
    let mut scope = ids
        .iter()
        .map(|id| ScopeAtom {
            source_root_id: *id,
            kind: PathScope::Subtree,
            path: String::new(),
        })
        .collect::<Vec<_>>();
    scope.sort();
    scope
}
fn expected_grant(record: &AuthorityRecord, host: Sha256Digest) -> Result<PersistedGrant> {
    record.validate()?;
    let provenance = GrantProvenance::ImportedStandaloneAdoption {
        adoption: Box::new(record.provenance.clone()),
    };
    let grant = ReviewedGrant {
        id: native_imported_grant_id(
            record.provenance.plan.setup.key.namespace,
            host,
            &provenance,
        )?,
        host,
        revision: CatalogRevision::INITIAL,
        revoked: false,
        memberships: vec![GrantedMembership {
            membership: Membership {
                project_id: record.project_id,
                index_id: record.receipt.setup.index_id,
                revision: CatalogRevision::INITIAL,
                scope: canonical_source_scope(&record.receipt.setup.source_ids),
            },
            source_revision: record.receipt.setup.source_revision,
            policy_epoch: 0,
            policy_digest: None,
        }],
    };
    let result = PersistedGrant { grant, provenance };
    result.validate()?;
    Ok(result)
}
fn record(db: &Connection, id: airk_core::index_catalog::GrantId) -> Result<AuthorityRecord> {
    let (operation, adopted, original, config, bytes):(String,String,String,String,Vec<u8>)=db.query_row(
        "SELECT operation_id,adopted_binding,original_binding,config_sha256,substr(payload,1,65537) FROM index_adopted_authorities WHERE grant_id=?1",
        [id.to_string()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)))?;
    ensure!(
        bytes.len() <= 65536,
        "standalone adoption proof exceeds bound"
    );
    let proof: AuthorityRecord = serde_json::from_slice(&bytes)?;
    proof.validate()?;
    ensure!(
        operation == proof.provenance.operation_id.to_string()
            && adopted == proof.provenance.plan.setup.registration.binding.to_string()
            && original
                == proof
                    .provenance
                    .plan
                    .original_registration
                    .binding
                    .to_string()
            && config == proof.provenance.plan.config_sha256.to_string(),
        "standalone adoption proof row crossed"
    );
    let mut statement=db.prepare("SELECT origin_sha256,config_sha256 FROM index_adopted_configuration_claims WHERE grant_id=?1 ORDER BY origin_sha256 LIMIT 33")?;
    let claims = statement
        .query_map([id.to_string()], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let expected = proof
        .provenance
        .configuration_claims
        .iter()
        .map(|c| (c.origin_sha256.to_string(), c.config_sha256.to_string()))
        .collect::<Vec<_>>();
    ensure!(
        claims == expected,
        "standalone configuration claim index differs from frozen evidence"
    );
    Ok(proof)
}
pub(super) fn read_provenance(
    db: &Connection,
    id: airk_core::index_catalog::GrantId,
) -> Result<GrantProvenance> {
    let proof = record(db, id)?;
    Ok(GrantProvenance::ImportedStandaloneAdoption {
        adoption: Box::new(proof.provenance),
    })
}
fn require_adopted_index(db: &Connection, record: &AuthorityRecord) -> Result<()> {
    let setup = &record.receipt.setup;
    let expected = &record.provenance.plan.setup.registration;
    let saved:Vec<u8>=db.query_row("SELECT substr(payload,1,16385) FROM index_operation_receipts WHERE id=?1 AND kind='catalog_index_setup' AND stage='ready'",[setup.operation_id.to_string()],|row|row.get(0))?;
    ensure!(
        saved.len() <= airk_core::catalog_management::MAX_RECEIPT_BYTES
            && serde_json::from_slice::<airk_core::catalog_setup::IndexSetupReceipt>(&saved)?
                == *setup,
        "completed standalone adoption receipt changed"
    );
    let current:(String,String,String,i64,bool,String,i64,Option<String>)=db.query_row(
        "SELECT i.backing_root,i.data_root,i.collection,i.source_revision,i.tombstone,i.source_import_state,i.policy_epoch,i.policy_digest FROM index_definitions i JOIN projects p ON p.binding=i.active_registration AND p.registered=1 AND p.cleanup_guard IS NULL WHERE i.id=?1 AND i.legacy_binding=?2 AND i.active_registration=?2",
        params![setup.index_id.to_string(),expected.binding.to_string()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?)))?;
    ensure!(
        current.0 == expected.root
            && current.1 == expected.data_root
            && current.2 == expected.collection
            && u64::try_from(current.3)? == setup.source_revision.get()
            && !current.4
            && current.5 == "complete"
            && current.6 == 0
            && current.7.is_none(),
        "adopted native index scope changed"
    );
    let count: i64 = db.query_row(
        "SELECT count(*) FROM index_sources WHERE index_id=?1",
        [setup.index_id.to_string()],
        |row| row.get(0),
    )?;
    ensure!(
        usize::try_from(count)? == setup.source_ids.len(),
        "adopted source inventory changed"
    );
    for (source, id) in record
        .provenance
        .plan
        .setup
        .sources
        .iter()
        .zip(&setup.source_ids)
    {
        let observed: (String, bool, bool) = db.query_row(
            "SELECT canonical_path,owner,enabled FROM index_sources WHERE index_id=?1 AND id=?2",
            params![setup.index_id.to_string(), id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        ensure!(
            observed == (source.canonical_path.clone(), source.owner, true),
            "adopted source identity or admission changed"
        );
        let mut query=db.prepare("SELECT alias FROM index_source_aliases WHERE index_id=?1 AND source_id=?2 ORDER BY alias")?;
        let aliases = query
            .query_map(params![setup.index_id.to_string(), id.to_string()], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut expected_aliases = source.aliases.clone();
        expected_aliases.sort();
        ensure!(
            aliases == expected_aliases,
            "adopted source aliases changed"
        );
    }
    Ok(())
}

pub(super) fn validate_snapshot(
    db: &Connection,
    snapshot: &PersistedGrant,
    binding: airk_core::service::ProjectBinding,
    host: Sha256Digest,
) -> Result<()> {
    let proof = record(db, snapshot.grant.id)?;
    ensure!(
        proof.provenance.plan.setup.registration.binding == binding
            && *snapshot == expected_grant(&proof, host)?,
        "standalone imported authority changed or was revoked"
    );
    require_adopted_index(db, &proof)?;
    let m = &snapshot.grant.memberships[0].membership;
    let revision:i64=db.query_row("SELECT m.revision FROM project_index_memberships m JOIN query_projects p ON p.id=m.project_id WHERE m.project_id=?1 AND m.index_id=?2",params![m.project_id.to_string(),m.index_id.to_string()],|row|row.get(0))?;
    ensure!(
        u64::try_from(revision)? == m.revision.get(),
        "standalone imported membership changed"
    );
    let mut query=db.prepare("SELECT source_id,kind,path FROM project_index_scopes WHERE project_id=?1 AND index_id=?2 ORDER BY source_id,kind,path")?;
    let actual = query
        .query_map(
            params![m.project_id.to_string(), m.index_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut expected = m
        .scope
        .iter()
        .map(|s| {
            (
                s.source_root_id.to_string(),
                "subtree".to_string(),
                String::new(),
            )
        })
        .collect::<Vec<_>>();
    expected.sort();
    ensure!(
        actual == expected,
        "standalone imported membership scope changed"
    );
    Ok(())
}

impl ServiceRegistry {
    pub fn import_standalone_authority_checked(
        &mut self,
        native: &NativeStandaloneAuthority,
        host: Sha256Digest,
        check_origin: &mut dyn FnMut() -> Result<()>,
    ) -> Result<PersistedGrant> {
        check_origin()?;
        native.revalidate()?;
        let proof = AuthorityRecord {
            schema: 1,
            provenance: native.provenance.clone(),
            receipt: native.receipt.clone(),
            project_id: project_id(&native.provenance)?,
        };
        let grant = expected_grant(&proof, host)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        super::validate_existing(&tx)?;
        let state = super::state(&tx)?;
        ensure!(
            state.activated
                && state.authority_contract == 1
                && state.namespace == native.provenance.plan.setup.key.namespace,
            "standalone authority requires its active native catalog"
        );
        require_adopted_index(&tx, &proof)?;
        super::super::lifecycle::require_catalog_admission(&tx)?;
        super::apply_adopted_authority_layout(&tx)?;
        if tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM index_adopted_authorities WHERE operation_id=?1)",
            [proof.provenance.operation_id.to_string()],
            |row| row.get::<_, bool>(0),
        )? {
            let saved = super::operations::read_persisted_grant(&tx, grant.grant.id)?;
            validate_snapshot(
                &tx,
                &saved,
                proof.provenance.plan.setup.registration.binding,
                host,
            )?;
            ensure!(
                record(&tx, saved.grant.id)? == proof,
                "standalone import retry changed original evidence"
            );
            native.revalidate()?;
            check_origin()?;
            tx.commit()?;
            return Ok(saved);
        }
        let next = state.revision.next()?;
        // Preflight the actual public frozen selector before creating authority.
        airk_core::service_http::NativeImportedHttpSelection {
            namespace: state.namespace,
            expected_catalog_revision: next,
            registration: proof.provenance.plan.setup.registration.clone(),
            grant: grant.clone(),
            configuration_receipt: proof.provenance.plan.config_sha256,
        }
        .validate()?;
        let m = &grant.grant.memberships[0];
        tx.execute(
            "INSERT INTO query_projects VALUES(?1,?2,1,1,1)",
            params![
                proof.project_id.to_string(),
                format!(
                    "Imported {}",
                    proof.provenance.plan.original_registration.collection
                )
            ],
        )?;
        tx.execute(
            "INSERT INTO project_index_memberships VALUES(?1,?2,1)",
            params![
                proof.project_id.to_string(),
                proof.receipt.setup.index_id.to_string()
            ],
        )?;
        tx.execute("INSERT INTO index_access_grants(id,host,revision,revoked,provenance_kind) VALUES(?1,?2,1,0,'imported_standalone_adoption')",params![grant.grant.id.to_string(),host.to_string()])?;
        tx.execute(
            "INSERT INTO index_access_memberships VALUES(?1,?2,?3,1,?4,0,NULL)",
            params![
                grant.grant.id.to_string(),
                proof.project_id.to_string(),
                proof.receipt.setup.index_id.to_string(),
                i64::try_from(m.source_revision.get())?
            ],
        )?;
        for atom in &m.membership.scope {
            tx.execute(
                "INSERT INTO project_index_scopes VALUES(?1,?2,?3,'subtree','')",
                params![
                    proof.project_id.to_string(),
                    proof.receipt.setup.index_id.to_string(),
                    atom.source_root_id.to_string()
                ],
            )?;
            tx.execute(
                "INSERT INTO index_access_scopes VALUES(?1,?2,?3,?4,'subtree','')",
                params![
                    grant.grant.id.to_string(),
                    proof.project_id.to_string(),
                    proof.receipt.setup.index_id.to_string(),
                    atom.source_root_id.to_string()
                ],
            )?;
        }
        tx.execute(
            "INSERT INTO index_adopted_authorities VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                grant.grant.id.to_string(),
                proof.provenance.operation_id.to_string(),
                proof.provenance.plan.setup.registration.binding.to_string(),
                proof
                    .provenance
                    .plan
                    .original_registration
                    .binding
                    .to_string(),
                proof.provenance.plan.config_sha256.to_string(),
                serde_json::to_vec(&proof)?
            ],
        )?;
        for claim in &proof.provenance.configuration_claims {
            tx.execute(
                "INSERT INTO index_adopted_configuration_claims VALUES(?1,?2,?3)",
                params![
                    grant.grant.id.to_string(),
                    claim.origin_sha256.to_string(),
                    claim.config_sha256.to_string()
                ],
            )?;
        }
        ensure!(tx.execute("UPDATE index_catalog_meta SET revision=?1,accepted_new_writes=1 WHERE id=1 AND revision=?2",params![i64::try_from(next.get())?,i64::try_from(state.revision.get())?])?==1,"standalone authority catalog CAS changed");
        native.revalidate()?;
        validate_snapshot(
            &tx,
            &grant,
            proof.provenance.plan.setup.registration.binding,
            host,
        )?;
        check_origin()?;
        tx.commit()?;
        Ok(grant)
    }
}

fn has_adopted_layout(db: &Connection) -> Result<bool> {
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='index_adopted_authorities')",[],|r|r.get(0))?)
}
pub(super) fn grant_for_binding(
    db: &Connection,
    binding: airk_core::service::ProjectBinding,
    host: Sha256Digest,
) -> Result<Option<String>> {
    if !has_adopted_layout(db)? {
        return Ok(None);
    }
    Ok(db.query_row("SELECT a.grant_id FROM index_adopted_authorities a JOIN index_access_grants g ON g.id=a.grant_id WHERE a.adopted_binding=?1 AND g.host=?2",params![binding.to_string(),host.to_string()],|r|r.get(0)).optional()?)
}
#[derive(Debug)]
pub struct AmbiguousStandaloneClaim;
impl std::fmt::Display for AmbiguousStandaloneClaim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ambiguous original standalone adoption authority; exact native origin claim is required")
    }
}
impl std::error::Error for AmbiguousStandaloneClaim {}

impl ServiceRegistry {
    /// Exact original declaration/config preimage to retained adopted authority.
    /// No directory inference, grant creation, clock change or filesystem I/O.
    pub fn adopted_legacy_registration(
        &self,
        original: &airk_core::service::ProjectRegistration,
        configuration_receipt: Sha256Digest,
        host: Sha256Digest,
    ) -> Result<Option<airk_core::service::ProjectRegistration>> {
        self.adopted_legacy_registration_claim(original, configuration_receipt, host, None)
    }
    /// An authenticated origin selector disambiguates multiple original entries
    /// in the same config file without inferring which physical namespace they used.
    pub fn adopted_legacy_registration_claim(
        &self,
        original: &airk_core::service::ProjectRegistration,
        configuration_receipt: Sha256Digest,
        host: Sha256Digest,
        origin_sha256: Option<Sha256Digest>,
    ) -> Result<Option<airk_core::service::ProjectRegistration>> {
        super::validate_existing(&self.connection)?;
        if !has_adopted_layout(&self.connection)? {
            return Ok(None);
        }
        let mut query=self.connection.prepare("SELECT DISTINCT a.grant_id FROM index_adopted_authorities a JOIN index_adopted_configuration_claims c ON c.grant_id=a.grant_id JOIN index_access_grants g ON g.id=a.grant_id WHERE a.original_binding=?1 AND c.config_sha256=?2 AND g.host=?3 AND (?4 IS NULL OR c.origin_sha256=?4) ORDER BY a.grant_id LIMIT 2")?;
        let ids = query
            .query_map(
                params![
                    original.binding.to_string(),
                    configuration_receipt.to_string(),
                    host.to_string(),
                    origin_sha256.map(|id| id.to_string())
                ],
                |r| r.get::<_, String>(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ensure!(ids.len() <= 1, AmbiguousStandaloneClaim);
        let Some(id) = ids.first() else {
            return Ok(None);
        };
        let grant = super::operations::read_persisted_grant(&self.connection, id.parse()?)?;
        let proof = record(&self.connection, grant.grant.id)?;
        ensure!(
            proof.provenance.plan.original_registration == *original
                && proof
                    .provenance
                    .configuration_claims
                    .iter()
                    .any(|claim| claim.config_sha256 == configuration_receipt
                        && origin_sha256.is_none_or(|origin| claim.origin_sha256 == origin)),
            "original standalone declaration differs from adopted proof"
        );
        let registration = proof.provenance.plan.setup.registration;
        validate_snapshot(&self.connection, &grant, registration.binding, host)?;
        Ok(Some(registration))
    }
}

#[cfg(test)]
mod tests;
