// SPDX-License-Identifier: Apache-2.0
//! Native predecessor refusal evidence, persisted in authority-1 qualification.
use airk_core::identity::{Sha256Digest, framed_digest};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

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

#[cfg(test)]
mod tests;
