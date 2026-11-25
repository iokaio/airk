// SPDX-License-Identifier: Apache-2.0
//! Private calibrated Compact FP16 variation of an exact reviewed CUDA base.
use super::*;
use airk_core::arbiter::{
    answer_loop::LoopCalibrationSet,
    device::{DevicePrecision, GpuBackend, PlacementPolicy},
};
use std::io::Seek;

const OVERLAY: &str = "AIRK_ARBITER_EVALUATION_LOOP_CALIBRATION_OVERLAY";
const OVERLAY_SHA: &str = "AIRK_ARBITER_EVALUATION_LOOP_CALIBRATION_OVERLAY_SHA256";
const SCHEMA: &str = "airk.private.cuda-loop-calibration-overlay/1";
const CALIBRATION_FILE: &str = "loop-calibration.json";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Overlay {
    schema: String,
    base_profile_sha256: Sha256Digest,
    base_manifest: Sha256Digest,
    calibration_sha256: Sha256Digest,
    review_sha256: Sha256Digest,
    pass1_profile_sha256: Sha256Digest,
    adapter: [u8; 16],
}

pub(super) struct Bound {
    descriptor: Overlay,
    sha256: Sha256Digest,
    pub(super) base: Profile,
    pub(super) candidate: Sha256Digest,
}

impl Bound {
    pub(super) fn from_env(artifacts: &Path, cuda: &mut [Prepared]) -> Result<Option<Self>> {
        let (path, expected) = match (std::env::var_os(OVERLAY), std::env::var_os(OVERLAY_SHA)) {
            (None, None) => return Ok(None),
            (Some(path), Some(expected)) => (PathBuf::from(path), expected),
            _ => bail!("private CUDA loop calibration overlay requires both path and SHA-256"),
        };
        ensure!(
            std::env::var_os("AIRK_ARBITER_EVALUATION_DECISION_MODE").is_none(),
            "private CUDA loop calibration overlay requires the default calibrated decision mode"
        );
        let expected: Sha256Digest = expected
            .to_str()
            .context("private CUDA loop calibration overlay SHA-256 is not Unicode")?
            .parse()?;
        ensure!(
            path.is_absolute(),
            "private CUDA overlay path must be absolute"
        );
        let parent = path
            .parent()
            .context("overlay parent missing")?
            .canonicalize()?;
        require_staging_parent(&parent, artifacts)?;
        let directory = OpenedDirectory::open(&parent)?;
        let mut source = directory.open_file(path.file_name().context("overlay name missing")?)?;
        let raw = bounded(&mut source)?;
        ensure!(
            sha(&raw) == expected,
            "private CUDA overlay descriptor hash differs"
        );
        let descriptor: Overlay = serde_json::from_slice(&raw)?;
        ensure!(
            descriptor.schema == SCHEMA && descriptor.adapter != [0; 16],
            "private CUDA loop calibration overlay schema or adapter differs"
        );
        let entry = cuda
            .iter_mut()
            .find(|entry| {
                entry.sha256 == descriptor.base_profile_sha256
                    && entry.profile.manifest == descriptor.base_manifest
            })
            .context("private CUDA overlay base is missing from the pinned CUDA roster")?;
        let base = entry.profile.clone();
        airk_retrieval::arbiter::runtime::Scorer::validate_private_protected_mha_calibration_base(
            &base,
        )?;
        let assets = base
            .assets
            .iter()
            .filter(|asset| asset.role == AssetRole::Calibration)
            .collect::<Vec<_>>();
        ensure!(
            assets.len() == 1,
            "protected CUDA base must have one calibration asset"
        );
        let asset_name = assets[0].name.clone();
        let mut original = entry
            .sources
            .get(&asset_name)
            .context("protected CUDA base calibration descriptor missing")?
            .try_clone()?;
        let mut calibration = directory.open_file(OsStr::new(CALIBRATION_FILE))?;
        let replacement = bounded(&mut calibration)?;
        ensure!(
            sha(&replacement) == descriptor.calibration_sha256,
            "private CUDA overlay calibration hash differs"
        );
        let variant =
            candidate_profile(&base, entry.sha256, &bounded(&mut original)?, &replacement)?;
        calibration.rewind()?;
        entry.raw = serde_json::to_vec(&variant)?;
        entry.sha256 = sha(&entry.raw);
        let candidate = variant.manifest;
        entry.profile = variant;
        entry.sources.insert(asset_name, calibration);
        Ok(Some(Self {
            descriptor,
            sha256: expected,
            base,
            candidate,
        }))
    }

    pub(super) fn verify_review(
        &self,
        artifacts: &Path,
        path: &Path,
        review_sha256: Sha256Digest,
        pass1_profile_sha256: Sha256Digest,
        adapter: [u8; 16],
    ) -> Result<()> {
        ensure!(
            self.descriptor.review_sha256 == review_sha256
                && self.descriptor.pass1_profile_sha256 == pass1_profile_sha256
                && self.descriptor.adapter == adapter,
            "private CUDA overlay cannot change the base review or adapter"
        );
        let (_directory, file) = source_file(path, artifacts)?;
        let mut raw = Vec::new();
        file.take(MAX_GRAPH_BYTES + 1).read_to_end(&mut raw)?;
        ensure!(
            raw.len() as u64 <= MAX_GRAPH_BYTES && sha(&raw) == review_sha256,
            "private CUDA overlay review bytes differ"
        );
        let review: serde_json::Value = serde_json::from_slice(&raw)?;
        require_review_adapter(&review, adapter)
    }

    pub(super) fn receipt(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": SCHEMA,
            "descriptor_sha256": self.sha256,
            "base_profile_sha256": self.descriptor.base_profile_sha256,
            "base_manifest": self.base.manifest,
            "candidate_manifest": self.candidate,
            "calibration_sha256": self.descriptor.calibration_sha256,
            "review_sha256": self.descriptor.review_sha256,
            "pass1_profile_sha256": self.descriptor.pass1_profile_sha256,
            "adapter": self.descriptor.adapter,
        })
    }
}

fn require_staging_parent(parent: &Path, artifacts: &Path) -> Result<()> {
    let relative = parent.strip_prefix(artifacts)?;
    ensure!(
        !relative.as_os_str().is_empty()
            && !relative.components().any(|part| matches!(
                part.as_os_str()
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .as_str(),
                ".airk" | "control" | "data" | "indexes" | "state"
            )),
        "private CUDA overlay must be staged under artifacts outside service and index roots"
    );
    Ok(())
}

fn require_review_adapter(review: &serde_json::Value, adapter: [u8; 16]) -> Result<()> {
    let observed: [u8; 16] = serde_json::from_value(review["adapter_before"]["uuid"].clone())?;
    ensure!(
        adapter != [0; 16] && observed == adapter,
        "private CUDA overlay review names another adapter"
    );
    Ok(())
}

fn candidate_profile(
    base: &Profile,
    base_sha256: Sha256Digest,
    original: &[u8],
    replacement: &[u8],
) -> Result<Profile> {
    ensure!(
        base.model == ModelChoice::ContextQwenCompact
            && base.qualification.is_none()
            && base.qualified_targets.is_empty()
            && base.downloads.is_empty()
            && base.profile_id.starts_with("private-")
            && base.device_execution.as_ref().is_some_and(|device| {
                device.backend == GpuBackend::Cuda
                    && device.precision == DevicePrecision::Float16
                    && matches!(device.placement, PlacementPolicy::BoundedHybrid { .. })
            }),
        "calibration overlay requires a private CUDA FP16 bounded Compact base"
    );
    ensure!(
        LoopCalibrationSet::parse_and_validate(original, base)?.is_unruled(),
        "calibration overlay requires the original zero-rule base"
    );
    ensure!(
        replacement.len() <= airk_core::arbiter::answer_loop::MAX_LOOP_CALIBRATION_SET_BYTES,
        "private CUDA overlay calibration byte limit"
    );
    let rules: LoopCalibrationSet = serde_json::from_slice(replacement)?;
    rules.validate(base)?;
    ensure!(
        !rules.is_unruled(),
        "calibration overlay requires at least one native rule"
    );
    let mut variant = base.clone();
    let asset = variant
        .assets
        .iter_mut()
        .find(|asset| asset.role == AssetRole::Calibration)
        .context("CUDA calibration asset missing")?;
    asset.sha256 = LoopCalibrationSet::digest(replacement);
    asset.bytes = u64::try_from(replacement.len())?;
    variant.calibration = asset.sha256;
    variant.profile_id = format!("private-cs2-loop-calibrated-{}", variant.calibration);
    variant.manifest = Sha256Digest::from_bytes([0; 32]);
    variant.manifest = framed_digest(
        b"airk.private.cuda-loop-calibration-profile/1",
        &[
            base_sha256.as_bytes(),
            base.manifest.as_bytes(),
            &serde_json::to_vec(&variant)?,
        ],
    );
    variant.validate()?;
    LoopCalibrationSet::parse_and_validate(replacement, &variant)?;
    Ok(variant)
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::arbiter::{
        answer_loop::{CardClass, LoopRule},
        device::{DEVICE_EXECUTION_SCHEMA, DeviceExecution, DeviceMemory},
    };

    fn digest(byte: u8) -> Sha256Digest {
        Sha256Digest::from_bytes([byte; 32])
    }

    fn base() -> Result<(Profile, Vec<u8>)> {
        let mut profile = airk_retrieval::arbiter::runtime::catalog_profiles()?
            .into_iter()
            .find(|profile| {
                profile.model == ModelChoice::ContextQwenCompact && profile.cpu_fit.is_some()
            })
            .context("Compact synthetic base missing")?;
        profile.qualification = None;
        profile.qualified_targets.clear();
        profile.downloads.clear();
        profile.profile_id = "private-test-compact-protected-mha".into();
        profile.runtime = GpuBackend::Cuda.runtime().into();
        let host_fit = profile.cpu_fit.take().context("test host fit")?;
        let graph = profile
            .assets
            .iter()
            .find(|asset| asset.role == AssetRole::Model)
            .context("test graph missing")?
            .sha256;
        profile.device_execution = Some(DeviceExecution {
            schema: DEVICE_EXECUTION_SCHEMA.into(),
            backend: GpuBackend::Cuda,
            runtime_build: digest(1),
            compatibility_policy: digest(2),
            provider_options: digest(3),
            math_policy: digest(4),
            precision: DevicePrecision::Float16,
            tf32: false,
            placement: PlacementPolicy::BoundedHybrid { policy: digest(5) },
            graph,
            host_fit,
            memory: DeviceMemory::Dedicated {
                required_peak_bytes: 8 << 30,
                headroom_bytes: 2 << 30,
            },
        });
        let original = airk_retrieval::arbiter::runtime::admit_loop_tasks(&mut profile)?;
        profile.validate()?;
        Ok((profile, original))
    }

    fn fitted(base: &Profile, original: &[u8]) -> Result<LoopCalibrationSet> {
        let mut rules = LoopCalibrationSet::parse_and_validate(original, base)?;
        rules.entries[1].rules.push(LoopRule {
            class: CardClass::PlanNeed,
            minimum_raw_margin: 0.25,
        });
        Ok(rules)
    }

    #[test]
    fn loop_overlay_preserves_every_noncalibration_binding() -> Result<()> {
        let (base, original) = base()?;
        let replacement = serde_json::to_vec(&fitted(&base, &original)?)?;
        let variant = candidate_profile(&base, digest(6), &original, &replacement)?;
        let calibration = LoopCalibrationSet::parse_and_validate(&replacement, &variant)?;
        assert_eq!(calibration.entries[1].rules[0].minimum_raw_margin, 0.25);
        assert_ne!(variant.manifest, base.manifest);
        assert_ne!(variant.profile_id, base.profile_id);
        assert_ne!(variant.calibration, base.calibration);
        assert_ne!(
            variant.manifest,
            candidate_profile(&base, digest(7), &original, &replacement)?.manifest
        );
        let mut restored = variant;
        restored.manifest = base.manifest;
        restored.profile_id = base.profile_id.clone();
        restored.calibration = base.calibration;
        let original_asset = base
            .assets
            .iter()
            .find(|asset| asset.role == AssetRole::Calibration)
            .context("original calibration missing")?;
        *restored
            .assets
            .iter_mut()
            .find(|asset| asset.role == AssetRole::Calibration)
            .context("variant calibration missing")? = original_asset.clone();
        assert_eq!(serde_json::to_value(restored)?, serde_json::to_value(base)?);
        Ok(())
    }

    #[test]
    fn loop_overlay_refuses_wrong_calibration_device_receipt_and_root() -> Result<()> {
        let (base, original) = base()?;
        let fitted = fitted(&base, &original)?;
        let replacement = serde_json::to_vec(&fitted)?;
        assert!(candidate_profile(&base, digest(6), &original, &original).is_err());
        assert!(candidate_profile(&base, digest(6), &replacement, &original).is_err());
        let mut crossed = fitted.clone();
        crossed.entries[1].task_policy = digest(8);
        assert!(
            candidate_profile(&base, digest(6), &original, &serde_json::to_vec(&crossed)?).is_err()
        );
        let mut crossed = fitted;
        crossed.entries[1].rules[0].class = CardClass::Covered;
        assert!(
            candidate_profile(&base, digest(6), &original, &serde_json::to_vec(&crossed)?).is_err()
        );
        let mut cpu = base;
        let device = cpu.device_execution.take().context("test device missing")?;
        cpu.cpu_fit = Some(device.host_fit);
        assert!(candidate_profile(&cpu, digest(6), &original, &replacement).is_err());

        let adapter = [12u8; 16];
        let review = serde_json::json!({"adapter_before":{"uuid":adapter}});
        require_review_adapter(&review, adapter)?;
        assert!(require_review_adapter(&review, [13; 16]).is_err());
        assert!(require_review_adapter(&serde_json::json!({}), adapter).is_err());

        let root = Path::new("D:/workspace/artifacts");
        require_staging_parent(&root.join("campaign/training/overlay"), root)?;
        for parent in [
            root.to_owned(),
            root.join("campaign/CONTROL/private"),
            root.join("campaign/data/private"),
            root.join("campaign/indexes/private"),
            PathBuf::from("D:/workspace/elsewhere"),
        ] {
            assert!(require_staging_parent(&parent, root).is_err());
        }
        Ok(())
    }
}
