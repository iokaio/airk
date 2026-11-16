// SPDX-License-Identifier: Apache-2.0
//! Strict source-observation projection of C7; never release qualification.
use airk_core::{
    identity::Sha256Digest, runtime_metadata::SplitRuntimeMetadata, service::RuntimeBuild,
};
use anyhow::{Result, ensure};
use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    sha256: Sha256Digest,
    files: u64,
}

// Deserialize original bytes directly: typed fields retain duplicate-key refusal.
// Report blocks must be original objects but their contents are inert here.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Receipt {
    schema: u32,
    source: Source,
    profile: String,
    pub(crate) runtime: SplitRuntimeMetadata,
    rustc: String,
    static_ort: Map<String, Value>,
    static_imports: Map<String, Value>,
    airkctl_static_imports: Map<String, Value>,
    embedded_model_smoke: Map<String, Value>,
    admin_assets: Map<String, Value>,
}
impl Receipt {
    pub(crate) fn observation(&self) -> RuntimeBuild {
        RuntimeBuild {
            source_sha256: self.source.sha256,
            source_files: self.source.files,
            profile: self.profile.clone(),
            rustc: self.rustc.clone(),
        }
    }
}

pub(crate) fn decode(bytes: &[u8]) -> Result<Receipt> {
    ensure!(bytes.len() <= 16384, "split build receipt exceeds bound");
    let receipt: Receipt = serde_json::from_slice(bytes)?;
    ensure!(
        receipt.schema == 2
            && (1..=1_000_000).contains(&receipt.source.files)
            && matches!(
                receipt.profile.as_str(),
                "debug" | "release" | "release-local"
            ),
        "invalid split build source/schema/profile"
    );
    // Match the existing C7 writer: Unicode characters, C0 only, multiline.
    ensure!(
        !receipt.rustc.is_empty()
            && receipt.rustc.chars().count() <= 4096
            && receipt
                .rustc
                .chars()
                .all(|c| c >= ' ' || matches!(c, '\r' | '\n' | '\t')),
        "invalid split build compiler observation"
    );
    let _ = (
        &receipt.static_ort,
        &receipt.static_imports,
        &receipt.airkctl_static_imports,
        &receipt.embedded_model_smoke,
        &receipt.admin_assets,
    );
    Ok(receipt)
}
