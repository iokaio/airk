// SPDX-License-Identifier: Apache-2.0
use super::*;

pub fn compatibility_policy_digest() -> Sha256Digest {
    airk_core::identity::framed_digest(b"airk.native.cuda.linux.compatibility/1", &[b"x86_64-unknown-linux-gnu;ort1.30;single-static-core;api22;cuda-driver-primary-context;held-elf64-needed-soname-closure;sm=86;driver-api>=12080"])
}

pub fn product_compatibility_policy_digest() -> Sha256Digest {
    airk_core::identity::framed_digest(b"airk.native.cuda.linux.compatibility/2", &[b"x86_64-unknown-linux-gnu;sm=86;dedicated;driver-api>=12080;held-elf64-needed-soname-closure;qualified-conservative-bounds;actual-load-final-map-independent-one-pass-numeric-sync;original-management-deadline;no-user-g4-or-permit"])
}

pub fn requirement_policy(row: &VariantRequirement) -> Result<Sha256Digest> {
    ensure!(
        row.target == TARGET && row.compute_capabilities == [[8, 6]],
        "unsupported Linux CUDA product class"
    );
    Ok(product_compatibility_policy_digest())
}

pub(super) fn asset_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.contains("..")
        && value.bytes().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'_' | b'.' | b'-')
        })
}

pub(super) fn validate_fits(fits: &[VariantFit], target: &str) -> Result<()> {
    ensure!(fits.len() <= 16, "CUDA scientific fit capacity");
    let mut manifests = BTreeSet::new();
    for fit in fits {
        ensure!(
            manifests.insert(fit.manifest) && !fit.execution_identity.is_empty(),
            "duplicate CUDA fit"
        );
        DeviceCapacityPlan::new(
            fit.cold_peak_bytes,
            fit.warm_extra_bytes,
            fit.headroom_bytes,
        )?;
        ensure!(
            fit.adapter_uuid != [0; 16]
                && fit.driver_api_version >= 12080
                && fit.evidence.numerical != fit.evidence.resources
                && fit.evidence.numerical != fit.evidence.placement
                && fit.evidence.resources != fit.evidence.placement
                && [
                    fit.evidence.numerical,
                    fit.evidence.resources,
                    fit.evidence.placement,
                    fit.driver_image_sha256
                ]
                .iter()
                .all(|v| *v != Sha256Digest::from_bytes([0; 32]))
                && fit.evidence.target == target
                && fit.evidence.execution_identity == fit.execution_identity,
            "invalid native GPU evidence/host phase fit"
        );
    }
    Ok(())
}

pub(super) fn validate_requirements(
    requirements: &[VariantRequirement],
    target: &str,
) -> Result<()> {
    ensure!(
        requirements.len() <= 16,
        "product CUDA requirement capacity"
    );
    let mut seen = BTreeSet::new();
    for row in requirements {
        ensure!(
            seen.insert(row.manifest)
                && row.manifest != Sha256Digest::from_bytes([0; 32])
                && !row.execution_identity.is_empty()
                && row.execution_identity.len() <= 512,
            "duplicate/invalid product variant"
        );
        ensure!(
            row.target == target
                && row.compute_capabilities == [[8, 6]]
                && row.minimum_driver_api >= 12080,
            "unqualified native target/SM/driver requirements"
        );
        DeviceCapacityPlan::new(
            row.cold_peak_limit_bytes,
            row.warm_extra_limit_bytes,
            row.headroom_bytes,
        )?;
        let e = &row.artifact_evidence;
        ensure!(
            e.target == row.target
                && e.execution_identity == row.execution_identity
                && e.numerical != e.resources
                && e.numerical != e.placement
                && e.resources != e.placement
                && [e.numerical, e.resources, e.placement]
                    .iter()
                    .all(|v| *v != Sha256Digest::from_bytes([0; 32])),
            "invalid scoped artifact evidence"
        );
        let f = &row.acceptance_fixture;
        ensure!(
            f.role == AssetRole::Fixture
                && asset_name(&f.name)
                && f.bytes > 0
                && f.bytes <= 16 << 20
                && f.sha256 != Sha256Digest::from_bytes([0; 32]),
            "complete independent eligibility fixture required"
        );
    }
    Ok(())
}

pub(super) fn required_variant<'a>(
    rows: &'a [VariantRequirement],
    profile: &Profile,
) -> Result<&'a VariantRequirement> {
    let identity = profile.device_execution_identity()?;
    let row = rows
        .iter()
        .find(|r| r.manifest == profile.manifest && r.execution_identity == identity)
        .context("product variant has no compiled conservative requirements")?;
    ensure!(
        row.target == TARGET
            && row.host_warm_extra_limit_bytes <= profile.memory_reservation_bytes
            && profile.assets.iter().any(|a| a == &row.acceptance_fixture),
        "product host/fixture/target requirements differ from profile"
    );
    ensure!(
        matches!(&profile.device_execution.as_ref().context("CPU product profile")?.memory,DeviceMemory::Dedicated{required_peak_bytes,headroom_bytes} if *required_peak_bytes==row.cold_peak_limit_bytes&&*headroom_bytes==row.headroom_bytes),
        "product conservative bounds differ from immutable profile"
    );
    Ok(row)
}
