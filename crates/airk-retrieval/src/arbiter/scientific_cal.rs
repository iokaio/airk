// SPDX-License-Identifier: Apache-2.0
//! Reviewed artifact identities for private scientific Compact CAL producers and fits.
//! This table has no product catalog authority.
use airk_core::{
    arbiter::{AssetRole, ModelChoice, Profile},
    identity::Sha256Digest,
};
use anyhow::{Result, ensure};

pub struct CompactArtifact {
    pub profile_file: &'static str,
    pub manifest: &'static str,
    pub graph: &'static str,
    pub weights: &'static str,
    pub origin: &'static str,
    /// Frozen independent matching-reference recipe basis for this candidate.
    pub matching_basis: Option<&'static str>,
}

pub const ORIGINAL: CompactArtifact = CompactArtifact {
    profile_file: "13085b0cbb6ed15bb4d6fb0318aad59b60e9c0d9c9ddb3e19f5b9f14be52f620",
    manifest: "6e7520aa29076f0958353a158e9be3390caef48675188e14e53660d035c1068f",
    graph: "c79c672513b874ab773ee7df8d4518e110d1e6196ae45076ce718f5b5dc6f7b4",
    weights: "7300a9202a4696f023799985131f6c4599ef9caaaa9b0d5b336f1c229574fbb4",
    origin: "native_original_compact_cpu",
    matching_basis: None,
};

pub const WINDOWS_QKV: CompactArtifact = CompactArtifact {
    profile_file: "67cc4ddcd715c20daf9ed7462b03777002ef2c06db59f9069575b1c5d14bc6bc",
    manifest: "dabbc7e062f4f5a350ed486c43c4af2a5cfe5db9dd6c1693739c03a70a5603cf",
    graph: "e621cf6bffad1f0a27e3cc1ecba5b4379949295498f6585963620d71cfe4ff14",
    weights: "0c4bd89d032e7dd821071c3559776cd3da50d54d84b6e575373fd8cf7003a7c3",
    origin: "native_compact_qkv_cuda",
    matching_basis: Some("f0d0e01fa28e7a876f1c57bc9894dff7e2a915ac8bf8150885b2aac2fdff2f14"),
};

impl CompactArtifact {
    pub fn registered(file: Sha256Digest, profile: &Profile) -> Result<&'static Self> {
        profile.validate()?;
        let file = file.to_string();
        let candidate = [&ORIGINAL, &WINDOWS_QKV]
            .into_iter()
            .find(|a| a.profile_file == file)
            .ok_or_else(|| anyhow::anyhow!("unregistered scientific Compact profile bytes"))?;
        ensure!(
            profile.model == ModelChoice::ContextQwenCompact
                && profile.qualification.is_none()
                && profile.manifest.to_string() == candidate.manifest
                && Self::one_asset(profile, AssetRole::Model, candidate.graph)
                && Self::one_asset(profile, AssetRole::ExternalWeights, candidate.weights),
            "scientific Compact manifest, consumed model or external weights differ"
        );
        Ok(candidate)
    }

    fn one_asset(profile: &Profile, role: AssetRole, digest: &str) -> bool {
        let mut assets = profile.assets.iter().filter(|a| a.role == role);
        assets
            .next()
            .is_some_and(|a| a.sha256.to_string() == digest)
            && assets.next().is_none()
    }
}
