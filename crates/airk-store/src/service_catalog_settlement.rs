// SPDX-License-Identifier: Apache-2.0
//! Readiness records, gated settlement and snapshot restoration before new writes.
use super::*;

/// CLI owns process execution and payload verification. Store persists its exact
/// refusal evidence; a command-parser error is not a registry refusal. This is
/// an installer contract, not an untrusted MCP or editor request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeRefusal {
    pub payload_identity: Sha256Digest,
    pub read_refusal_digest: Sha256Digest,
    pub write_refusal_digest: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_provenance: Option<NativeRefusalProvenance>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeReadRefusalEntrypoint {
    ServiceRequestsList,
    HiddenUpgradeProbe,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeWriteRefusalEntrypoint {
    ServiceRequestsForget,
    HiddenUpgradeMigrate,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRefusalEntrypoints {
    pub read: NativeReadRefusalEntrypoint,
    pub write: NativeWriteRefusalEntrypoint,
}
/// Digests of actual native diagnostics. The executing producer must classify
/// the final diagnostic as a format/authority refusal, never parser failure or
/// AdmissionPending. Store validates binding, not the truth of raw diagnostics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRefusalDiagnostic {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_admission_pending_digest: Option<Sha256Digest>,
    pub final_classified_diagnostic_digest: Sha256Digest,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRefusalProvenance {
    pub native_entrypoints: NativeRefusalEntrypoints,
    pub read: NativeRefusalDiagnostic,
    pub write: NativeRefusalDiagnostic,
}
impl NativeRefusalProvenance {
    fn validate(&self) -> Result<()> {
        ensure!(
            matches!(
                self.native_entrypoints.read,
                NativeReadRefusalEntrypoint::HiddenUpgradeProbe
            ) == self.read.initial_admission_pending_digest.is_some()
                && matches!(
                    self.native_entrypoints.write,
                    NativeWriteRefusalEntrypoint::HiddenUpgradeMigrate
                ) == self.write.initial_admission_pending_digest.is_some(),
            "native refusal entrypoint/fallback provenance differs"
        );
        airk_core::index_catalog::encoded_size(self, 4096)?;
        Ok(())
    }
}
impl RuntimeRefusal {
    pub fn from_native(
        payload_identity: Sha256Digest,
        provenance: NativeRefusalProvenance,
    ) -> Result<Self> {
        provenance.validate()?;
        let bytes = serde_json::to_vec(&provenance)?;
        let bind = |direction: &[u8]| {
            framed_digest(
                b"airk/catalog-native-refusal-provenance/1",
                &[payload_identity.as_bytes(), &bytes, direction],
            )
        };
        Ok(Self {
            payload_identity,
            read_refusal_digest: bind(b"read"),
            write_refusal_digest: bind(b"write"),
            native_provenance: Some(provenance),
        })
    }
    pub fn validate(&self) -> Result<()> {
        if let Some(provenance) = &self.native_provenance {
            let expected = Self::from_native(self.payload_identity, provenance.clone())?;
            ensure!(
                self.read_refusal_digest == expected.read_refusal_digest
                    && self.write_refusal_digest == expected.write_refusal_digest,
                "native refusal digest does not bind its full provenance"
            );
        }
        // None is an existing P5 proof: preserve its exact bytes and historical
        // digest meaning. Never reinterpret or rewrite stored predecessor proof.
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadinessEvidence {
    pub schema: u32,
    pub candidate: CatalogUpgradeObservation,
    pub predecessor_refusals: Vec<RuntimeRefusal>,
}
impl ReadinessEvidence {
    pub(super) fn validate(&self, receipt: &UpgradeReceipt) -> Result<()> {
        let observation = &self.candidate;
        ensure!(
            self.schema == 1
                && observation.schema == 1
                && observation.plan_digest == receipt.plan_digest
                && observation.installer_identity == receipt.plan.installer_identity
                && observation.catalog.namespace == receipt.plan.namespace
                && observation.catalog.upgrade_id == receipt.plan.id
                && observation.catalog.schema == 12
                && !observation.catalog.activated
                && !observation.catalog.accepted_new_writes,
            "catalog readiness is not bound to this inactive upgrade"
        );
        ensure!(
            !self.predecessor_refusals.is_empty() && self.predecessor_refusals.len() <= 8,
            "catalog readiness requires bounded predecessor read/write refusal evidence"
        );
        let mut identities = BTreeSet::new();
        for predecessor in &self.predecessor_refusals {
            predecessor.validate()?;
            ensure!(
                predecessor.payload_identity != observation.installer_identity
                    && identities.insert(predecessor.payload_identity),
                "ambiguous predecessor compatibility evidence"
            );
        }
        airk_core::index_catalog::encoded_size(self, 16384)?;
        Ok(())
    }
}

impl CatalogUpgrade {
    pub fn record_readiness(&self, evidence: ReadinessEvidence) -> Result<UpgradeReceipt> {
        let mut receipt = self.receipt()?;
        ensure!(
            matches!(
                receipt.stage,
                UpgradeStage::Imports | UpgradeStage::Readiness
            ),
            "catalog readiness is not admitted in this stage"
        );
        evidence.validate(&receipt)?;
        let current = observe(&self.root, receipt.plan.id, evidence.candidate.nonce)?;
        ensure!(
            current == evidence.candidate,
            "candidate catalog observation is stale or incomplete"
        );
        if let Some(previous) = &receipt.readiness {
            ensure!(previous == &evidence, "catalog readiness evidence changed");
        }
        receipt.readiness = Some(evidence);
        receipt.stage = UpgradeStage::Readiness;
        self.save(&receipt)?;
        Ok(receipt)
    }

    pub fn settle(&self) -> Result<UpgradeReceipt> {
        anyhow::bail!(
            "P5 readiness cannot activate a production catalog; qualify and settle authority contract 1"
        )
    }

    #[cfg(test)]
    fn settle_at(&self, checkpoint: impl Fn(&str) -> Result<()>) -> Result<UpgradeReceipt> {
        let mut receipt = pending(&self.root)?.context("catalog upgrade intent missing")?;
        ensure!(
            receipt.plan.namespace == self.namespace
                && matches!(
                    receipt.stage,
                    UpgradeStage::Readiness | UpgradeStage::Settled
                ),
            "catalog settlement is not admitted in this stage"
        );
        let read = registry_read(&self.root)?;
        super::super::validate_existing(&read)?;
        let state = super::super::state(&read)?;
        ensure!(
            state.namespace == self.namespace && state.upgrade_id == receipt.plan.id,
            "catalog settlement identity changed"
        );
        let owner = super::super::super::owner_record(&self.root, true, self.namespace)?;
        ensure!(
            owner.catalog_upgrade.is_none() || owner.catalog_upgrade == Some(receipt.plan.id),
            "catalog settlement owner fence changed"
        );
        if owner.catalog_upgrade.is_none() {
            // A crash after clearing admission may be followed by legitimate new
            // writes. The durable activation and saved readiness prove cutover;
            // never restore or demand that legacy rows remain frozen afterwards.
            ensure!(
                state.activated && receipt.readiness.is_some(),
                "catalog settlement lost admission before activation"
            );
            receipt.stage = UpgradeStage::Settled;
            self.save(&receipt)?;
            return Ok(receipt);
        }
        ensure!(
            !state.accepted_new_writes,
            "catalog accepted new writes before settlement; forward repair required"
        );
        Self::validate_backfill(&self.root, self.namespace, &read, &receipt)?;
        let evidence = receipt
            .readiness
            .as_ref()
            .context("catalog readiness evidence missing")?;
        ensure!(
            state.revision == evidence.candidate.catalog.revision,
            "catalog changed after candidate readiness probe"
        );
        drop(read);
        if !state.activated {
            let connection = Connection::open_with_flags(
                self.root.join(DATABASE),
                OpenFlags::SQLITE_OPEN_READ_WRITE,
            )?;
            connection.execute_batch("PRAGMA synchronous=FULL; BEGIN IMMEDIATE; UPDATE index_catalog_meta SET activated=1 WHERE id=1 AND activated=0 AND accepted_new_writes=0; COMMIT;")?;
        }
        checkpoint("after_catalog_activation")?;
        self.clear_fence(receipt.plan.id)?;
        checkpoint("after_admission_clear")?;
        receipt.stage = UpgradeStage::Settled;
        self.save(&receipt)?;
        Ok(receipt)
    }

    fn clear_fence(&self, id: Incarnation) -> Result<()> {
        let mut owner = super::super::super::owner_record(&self.root, true, self.namespace)?;
        ensure!(
            owner.catalog_upgrade == Some(id),
            "catalog admission fence changed before settlement"
        );
        owner.catalog_upgrade = None;
        persist(
            &self.root.join("service-1.4.owner.json"),
            &serde_json::to_vec(&owner)?,
        )
    }

    /// Before any settled new-format write, restore only the qualified private
    /// registry snapshot. Source directories and collection bytes are untouched.
    /// After the write barrier, refuse downgrade and require forward repair.
    pub fn restore(&self) -> Result<UpgradeReceipt> {
        self.restore_at(|_| Ok(()))
    }

    fn restore_at(&self, checkpoint: impl Fn(&str) -> Result<()>) -> Result<UpgradeReceipt> {
        let mut receipt = pending(&self.root)?.context("catalog upgrade intent missing")?;
        ensure!(
            receipt.plan.namespace == self.namespace,
            "catalog restore namespace changed"
        );
        let read = registry_read(&self.root)?;
        let version: u32 = read.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let mut owner = super::super::super::owner_record(&self.root, true, self.namespace)?;
        ensure!(
            owner.catalog_upgrade.is_none() || owner.catalog_upgrade == Some(receipt.plan.id),
            "catalog restore owner changed"
        );
        if receipt.stage == UpgradeStage::Restored {
            ensure!(
                version == 11 && owner.catalog_upgrade.is_none(),
                "restored catalog no longer matches its supported format"
            );
            return Ok(receipt);
        }
        if version == 11
            && owner.catalog_upgrade.is_none()
            && receipt.stage == UpgradeStage::Restoring
        {
            // Admission is cleared only after successful snapshot restore.
            // Legacy writes may have resumed before the external receipt was
            // saved. Finalize it without replaying or freezing those rows.
            super::super::backup::integrity(&read)?;
            receipt.stage = UpgradeStage::Restored;
            self.save(&receipt)?;
            return Ok(receipt);
        }
        if version == 12 {
            super::super::validate_existing(&read)?;
            let state = super::super::state(&read)?;
            ensure!(
                state.namespace == self.namespace && state.upgrade_id == receipt.plan.id,
                "catalog restore identity changed"
            );
            ensure!(
                !state.accepted_new_writes,
                "catalog has accepted new writes; automatic restore or downgrade refused; forward repair required"
            );
            Self::validate_backfill(&self.root, self.namespace, &read, &receipt)?;
            self.qualified_backup(&receipt)?;
        } else {
            super::super::backup::integrity(&read)?;
            if let Some(digest) = receipt.legacy_data_digest {
                ensure!(
                    super::super::backup::legacy_data_digest(&read)? == digest,
                    "legacy rows changed before restore settlement"
                );
            }
        }
        drop(read);
        owner.catalog_upgrade = Some(receipt.plan.id);
        persist(
            &self.root.join("service-1.4.owner.json"),
            &serde_json::to_vec(&owner)?,
        )?;
        receipt.stage = UpgradeStage::Restoring;
        self.save(&receipt)?;
        checkpoint("restore_intent")?;
        if version == 12 {
            let attempt = self.qualified_backup(&receipt)?;
            super::super::backup::restore(
                &self.root,
                receipt.plan.id,
                attempt,
                receipt
                    .legacy_data_digest
                    .context("catalog restore row identity missing")?,
            )?;
        }
        checkpoint("after_snapshot_restore")?;
        self.clear_fence(receipt.plan.id)?;
        checkpoint("after_restore_admission_clear")?;
        receipt.stage = UpgradeStage::Restored;
        self.save(&receipt)?;
        Ok(receipt)
    }
}

#[cfg(test)]
#[path = "service_catalog_settlement_tests.rs"]
mod tests;
