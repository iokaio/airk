// SPDX-License-Identifier: Apache-2.0
//! Bounded retry identity for affected authenticated inputs, never read authority.
use super::{
    CurrentRead,
    map::{self, MapDomain},
};
use airk_core::identity::{Sha256Digest, framed_digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditInputWitness {
    version: u32,
    scope: Sha256Digest,
    keys: Vec<Sha256Digest>,
    values: Sha256Digest,
    policy: Sha256Digest,
    allowance: u64,
}
impl AuditInputWitness {
    pub(crate) fn allowance(&self) -> u64 {
        self.allowance
    }
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1
                && (1..=8).contains(&self.keys.len())
                && self.keys.windows(2).all(|pair| pair[0] < pair[1])
                && (1..=(512 << 20)).contains(&self.allowance),
            "invalid audit input retry witness"
        );
        Ok(())
    }
}
impl CurrentRead {
    fn audit_scope(&self) -> Result<Sha256Digest> {
        Ok(framed_digest(
            b"audit-input-scope/1",
            &[&serde_json::to_vec(&self.head().scope)?],
        ))
    }
    fn audit_values(&self, keys: &[Sha256Digest]) -> Result<Sha256Digest> {
        let mut digest = Sha256::new();
        digest.update(b"AIRK-AUDIT-INPUTS-1\0");
        self.with_connection(|connection| {
            for key in keys {
                digest.update(key.as_bytes());
                let value = map::get(
                    connection,
                    &self.head().scope,
                    self.head().artifact_root,
                    *key,
                )?;
                digest.update([u8::from(value.is_some())]);
                if let Some(value) = value {
                    digest.update((value.len() as u64).to_be_bytes());
                    digest.update(value);
                }
            }
            Ok(())
        })?;
        Ok(Sha256Digest::from_bytes(digest.finalize().into()))
    }
    fn make_audit_witness(
        &self,
        inputs: &[(MapDomain, &[u8])],
        policy: Sha256Digest,
        allowance: u64,
    ) -> Result<AuditInputWitness> {
        ensure!(
            (1..=8).contains(&inputs.len()),
            "invalid audit retry input count"
        );
        let mut keys = inputs
            .iter()
            .map(|(domain, identity)| map::key(&self.head().scope, *domain, identity))
            .collect::<Result<Vec<_>>>()?;
        keys.sort_unstable();
        keys.dedup();
        let witness = AuditInputWitness {
            version: 1,
            scope: self.audit_scope()?,
            values: self.audit_values(&keys)?,
            keys,
            policy,
            allowance,
        };
        witness.validate()?;
        Ok(witness)
    }
    pub(crate) fn file_audit_witness(
        &self,
        path: &str,
        policy: Sha256Digest,
        allowance: u64,
    ) -> Result<AuditInputWitness> {
        self.make_audit_witness(
            &[
                (MapDomain::Artifact, path.as_bytes()),
                (MapDomain::FileMetadata, path.as_bytes()),
            ],
            policy,
            allowance,
        )
    }
    pub(crate) fn import_audit_witness(
        &self,
        policy: Sha256Digest,
        allowance: u64,
    ) -> Result<AuditInputWitness> {
        self.make_audit_witness(
            &[
                (MapDomain::Metadata, b"analysis"),
                (MapDomain::Metadata, b"analysis-projection"),
            ],
            policy,
            allowance,
        )
    }
    pub(crate) fn recent_audit_witness(
        &self,
        policy: Sha256Digest,
        allowance: u64,
    ) -> Result<AuditInputWitness> {
        self.make_audit_witness(
            &[(MapDomain::RecentChanges, b"manifest")],
            policy,
            allowance,
        )
    }
    /// Compare only the affected proof-checked values. A different global head
    /// alone cannot revive work that made no progress under the same allowance.
    pub fn audit_inputs_changed(
        &self,
        saved: &AuditInputWitness,
        policy: Sha256Digest,
        allowance: u64,
    ) -> Result<bool> {
        saved.validate()?;
        if saved.scope != self.audit_scope()?
            || saved.policy != policy
            || saved.allowance != allowance
        {
            return Ok(true);
        }
        Ok(saved.values != self.audit_values(&saved.keys)?)
    }
}
