// SPDX-License-Identifier: Apache-2.0
//! Readers of existing native compatibility grants: imported legacy snapshots
//! sealed by earlier releases and adopted standalone snapshots. No grant is
//! created, resealed or inferred here.
use airk_core::{
    grant_provenance::GrantProvenance,
    grant_provenance::PersistedGrant,
    identity::Sha256Digest,
    index_catalog::CatalogRevision,
    scoped_reads::ScopeReadRefused,
    service::{ProjectBinding, ProjectRegistration},
};
use anyhow::{Result, ensure};
use rusqlite::{OptionalExtension, params};

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
        let legacy: Option<String> = tx.query_row("SELECT id FROM index_access_grants WHERE provenance_kind='imported_legacy' AND import_legacy_binding=?1 AND host=?2",
            params![binding.to_string(),host.to_string()], |r|r.get(0)).optional()?;
        let adopted = super::super::adopted_authority::grant_for_binding(&tx, binding, host)?;
        ensure!(!(legacy.is_some() && adopted.is_some()), ScopeReadRefused);
        let id = legacy.or(adopted).ok_or(ScopeReadRefused)?;
        let snapshot = super::super::operations::read_persisted_grant(&tx, id.parse()?)?;
        snapshot.validate()?;
        ensure!(
            !snapshot.grant.revoked && snapshot.grant.host == host,
            ScopeReadRefused
        );
        if matches!(
            snapshot.provenance,
            GrantProvenance::ImportedStandaloneAdoption { .. }
        ) {
            super::super::adopted_authority::validate_snapshot(&tx, &snapshot, binding, host)?;
            return Ok(snapshot);
        }
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
