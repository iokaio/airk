// SPDX-License-Identifier: Apache-2.0
//! Closed pre-1.9 CPU Focus source compatibility, never task qualification.
use super::*;
use airk_core::identity::framed_digest;
use serde::Serialize;

#[cfg(test)]
mod tests;

const LEGACY_MANIFEST: &str = "04821edaed45ac10aa5b0b7b2c0b7b28bf5d04a06c8b623cfe6d762f77d5e23b";
const LEGACY_PROFILE: &str = "73d884fb1c2b37709d4d1b1daa93fd65687ce310cbd4096af0ef106a615e608c";
const LEGACY_CATALOG: &str = "3b257248a435e8e2fb0112165fbe6870ae1db29cbbaece4d05fe375e8d8c3fe4";
const LEGACY_TASK: &str = "cfbcbe4cce111b9f802918488dae82feefb075666a0a7e7a9f93dd082b71c331";
const LEGACY_RUNTIME_SOURCE: &str =
    "d2879546ba9e949fdd8b135a244923de2adb781e8598539cc33f4d7d6f0794eb";
// Exact runtime owner hook bytes for the reviewed G5 supplement. A different source requires a new row.
const CURRENT_RUNTIME_SOURCE: Option<&str> =
    Some("42c0592fac9071066abb8cac765c7fe45d665c6df183c5e12aca7c8caa95981b");
const LEGACY_CORE: &str = "ddd9dbc76ae2c7e92c3b57b58a9443161fb045bbc175596f57bb41c9b1b5bbe6";
const CURRENT_CORE: &str = "914692048e6d833e0d18c10485e7d9f57f2e0c8874f428b31002bcf7c09cdeef";
const CURRENT_TASK: &str = "25c96233573bd34cd71d03317bc757677d31e48ffbe362c462499d5fb1b7eff7";
const COMPILER_SOURCE: &str = "09c3120dd83152db417a03959d7cdf1c241f43eb7699154ec3b43d976b951627";
const FOCUS_COMPILER_SOURCE: &str =
    "a58243020b534c08c7cc049cb9155d4bdfe4e220f96112967a048692a0813015";

/// Native owner-only witness. A decoded Profile alone is not execution authority.
/// Actual release/evaluation ownership, admission, custody and fit remain required.
#[derive(Clone)]
pub(super) struct CpuSourceTransition {
    profile: Sha256Digest,
    row: Sha256Digest,
}

#[derive(Serialize)]
pub(super) struct SourceTransitionRecord {
    schema: &'static str,
    row: Sha256Digest,
    transition_source: Sha256Digest,
    legacy_catalog: Sha256Digest,
    legacy_manifest: Sha256Digest,
    legacy_profile: Sha256Digest,
    legacy_task_policy: Sha256Digest,
    legacy_core_semantics: Sha256Digest,
    current_task_policy: Sha256Digest,
    current_core_semantics: Sha256Digest,
    runtime: &'static str,
    native_version: &'static str,
    legacy_runtime_source: Sha256Digest,
    current_runtime_source: Sha256Digest,
    task_qualification_transferred: bool,
    public_source_rewritten: bool,
}

#[derive(Serialize)]
pub(super) struct SourceTransitionInput {
    schema: &'static str,
    transition: SourceTransitionRecord,
    current_compiled_input: Sha256Digest,
    binding: Sha256Digest,
}

fn digest(value: &str) -> Result<Sha256Digest> {
    value.parse()
}
fn source_sha(bytes: &[u8]) -> Result<Sha256Digest> {
    let source = std::str::from_utf8(bytes)?.replace("\r\n", "\n");
    ensure!(!source.contains('\r'), "lone CR in CPU transition source");
    Ok(Sha256Digest::from_bytes(
        Sha256::digest(source.as_bytes()).into(),
    ))
}
fn profile_sha(profile: &Profile) -> Result<Sha256Digest> {
    // serde_json::Value canonically orders object keys. None/empty fields retain
    // the existing Profile serializer's omission rules; no descriptor is changed.
    Ok(Sha256Digest::from_bytes(
        Sha256::digest(serde_json::to_vec(&serde_json::to_value(profile)?)?).into(),
    ))
}
fn validate_current_sources(core: Sha256Digest, task: Sha256Digest) -> Result<()> {
    let expected_runtime = CURRENT_RUNTIME_SOURCE
        .context("closed CPU Focus runtime source mapping awaits reviewed owner hooks")?;
    ensure!(
        source_sha(include_bytes!("../runtime.rs"))? == digest(expected_runtime)?,
        "CPU Focus native runtime changed outside the closed source row"
    );
    ensure!(
        core == digest(CURRENT_CORE)? && task == digest(CURRENT_TASK)?,
        "CPU Focus compatibility row does not name the current compiled source"
    );
    ensure!(
        source_sha(include_bytes!("../compiler.rs"))? == digest(COMPILER_SOURCE)?
            && source_sha(include_bytes!("../compiler/focus.rs"))?
                == digest(FOCUS_COMPILER_SOURCE)?,
        "CPU Focus compiler changed outside the closed compatibility row"
    );
    Ok(())
}
fn validate_descriptor(profile: &Profile) -> Result<()> {
    profile.validate()?;
    profile.validate_release_downloads()?;
    ensure!(
        profile.model == ModelChoice::FocusQwenReranker06B
            && profile.manifest == digest(LEGACY_MANIFEST)?
            && profile.runtime == RUNTIME
            && crate::model::ONNX_RUNTIME_ENGINE == RUNTIME
            && profile.device_execution.is_none()
            && profile_sha(profile)? == digest(LEGACY_PROFILE)?,
        "CPU Focus compatibility requires the exact immutable legacy descriptor"
    );
    // The full descriptor hash above binds every asset, ABI, thread/fit bound,
    // token/candidate/reservation limit, qualification field and public URL.
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Catalog {
        schema: String,
        profiles: Vec<Profile>,
    }
    let catalog: Catalog = serde_json::from_slice(include_bytes!("../release-catalog.json"))?;
    let matches: Vec<_> = catalog
        .profiles
        .iter()
        .filter(|p| p.manifest == profile.manifest)
        .collect();
    ensure!(
        catalog.schema == CATALOG_SCHEMA
            && matches.len() == 1
            && profile_sha(matches[0])? == digest(LEGACY_PROFILE)?,
        "legacy CPU Focus descriptor is absent or changed in the native catalog"
    );
    Ok(())
}
impl CpuSourceTransition {
    /// Called only by the native ExecutionProfile owner after authentic catalog
    /// admission. This witness cannot replace its lease, CPU fit or reservation.
    pub(super) fn admit(profile: &Profile) -> Result<Self> {
        ensure!(
            super::local_test_series(env!("CARGO_PKG_VERSION")),
            "legacy CPU Focus source transition is limited to pre-1.9 native builds"
        );
        validate_descriptor(profile)?;
        validate_current_sources(
            airk_core::arbiter::focus::semantic_policy_digest()?,
            crate::arbiter::compiler::focus::policy_digest()?,
        )?;
        let record = Self::record_for_row()?;
        Ok(Self {
            profile: digest(LEGACY_PROFILE)?,
            row: record.row,
        })
    }
    pub(super) fn validate(&self, profile: &Profile) -> Result<()> {
        let current = Self::admit(profile)?;
        ensure!(
            self.profile == current.profile && self.row == current.row,
            "CPU Focus source transition witness changed"
        );
        Ok(())
    }
    fn record_for_row() -> Result<SourceTransitionRecord> {
        let current_runtime_source = digest(
            CURRENT_RUNTIME_SOURCE
                .context("closed CPU Focus runtime source mapping unavailable")?,
        )?;
        let transition_source = source_sha(include_bytes!("cpu_source_transition.rs"))?;
        let row = framed_digest(
            b"airk.arbiter.cpu-focus-source-transition/1",
            &[
                digest(LEGACY_MANIFEST)?.as_bytes(),
                digest(LEGACY_PROFILE)?.as_bytes(),
                digest(LEGACY_TASK)?.as_bytes(),
                digest(LEGACY_CORE)?.as_bytes(),
                digest(CURRENT_TASK)?.as_bytes(),
                digest(CURRENT_CORE)?.as_bytes(),
                digest(COMPILER_SOURCE)?.as_bytes(),
                digest(FOCUS_COMPILER_SOURCE)?.as_bytes(),
                digest(LEGACY_RUNTIME_SOURCE)?.as_bytes(),
                current_runtime_source.as_bytes(),
                transition_source.as_bytes(),
                RUNTIME.as_bytes(),
            ],
        );
        Ok(SourceTransitionRecord {
            schema: "airk.arbiter.cpu-focus-source-transition/1",
            row,
            transition_source,
            legacy_catalog: digest(LEGACY_CATALOG)?,
            legacy_manifest: digest(LEGACY_MANIFEST)?,
            legacy_profile: digest(LEGACY_PROFILE)?,
            legacy_task_policy: digest(LEGACY_TASK)?,
            legacy_core_semantics: digest(LEGACY_CORE)?,
            current_task_policy: digest(CURRENT_TASK)?,
            current_core_semantics: digest(CURRENT_CORE)?,
            runtime: RUNTIME,
            native_version: env!("CARGO_PKG_VERSION"),
            legacy_runtime_source: digest(LEGACY_RUNTIME_SOURCE)?,
            current_runtime_source,
            task_qualification_transferred: false,
            public_source_rewritten: false,
        })
    }
    pub(super) fn record(&self, profile: &Profile) -> Result<SourceTransitionRecord> {
        self.validate(profile)?;
        Self::record_for_row()
    }
    /// The native caller supplies a digest just produced by the current Rust
    /// Compiler. This records provenance; it grants no inference or read permit.
    pub(super) fn bind_input(
        &self,
        profile: &Profile,
        current_compiled_input: Sha256Digest,
    ) -> Result<SourceTransitionInput> {
        let transition = self.record(profile)?;
        ensure!(
            current_compiled_input.as_bytes().iter().any(|b| *b != 0),
            "missing current compiled input identity"
        );
        let binding = framed_digest(
            b"airk.arbiter.cpu-focus-source-transition-input/1",
            &[transition.row.as_bytes(), current_compiled_input.as_bytes()],
        );
        Ok(SourceTransitionInput {
            schema: "airk.arbiter.cpu-focus-source-transition-input/1",
            transition,
            current_compiled_input,
            binding,
        })
    }
}
