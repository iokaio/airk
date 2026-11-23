// SPDX-License-Identifier: Apache-2.0
//! The one-way migration from `airk.arbiter.configuration/4`, written before
//! the Large Context model was retired, to configuration/5. Data only: the
//! caller supplies the compiled catalog transition; nothing here admits a
//! model, a source or execution.
use super::*;

/// A compiled device-catalog transition: the binding a configuration/4
/// document must carry, and the binding of the catalog that replaced it. The
/// service compiles both; a document bound to anything else is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Configuration4Transition {
    pub old: DeviceCatalogBinding,
    pub new: DeviceCatalogBinding,
}

/// The selectable models of configuration/4, as that schema wrote them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum SelectedModel4 {
    Focus,
    ContextCompact,
    ContextLarge,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stage4 {
    enabled: bool,
    selected_model: Option<SelectedModel4>,
    device_policy: DevicePolicy,
    gpu_precision: GpuPrecisionPreference,
    indexes: Vec<Enrollment>,
}

/// configuration/4 field for field, so its canonical bytes can be checked.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Configuration4 {
    schema: String,
    installation: Sha256Digest,
    catalog: Incarnation,
    revision: u64,
    device_catalog: DeviceCatalogBinding,
    focus: Stage4,
    context: Stage4,
}

impl Configuration4 {
    /// Decodes exact canonical configuration/4 bytes of this installation.
    fn decode(bytes: &[u8], installation: Sha256Digest, catalog: Incarnation) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_CONFIGURATION_BYTES,
            "device configuration byte limit"
        );
        let value: Self = serde_json::from_slice(bytes)?;
        ensure!(
            value.schema == DEVICE_CONFIGURATION_V4_SCHEMA && value.revision > 0,
            "not a configuration/4 document"
        );
        ensure!(
            value.installation == installation && value.catalog == catalog,
            "configuration/4 installation or source catalog differs"
        );
        let mut canonical = serde_json::to_vec(&value)?;
        canonical.push(b'\n');
        ensure!(canonical == bytes, "noncanonical configuration/4");
        Ok(value)
    }
}

/// The binding and revision of exact canonical configuration/4 bytes of this
/// installation. Diagnostic metadata: it selects no catalog.
pub fn configuration4_binding(
    bytes: &[u8],
    installation: Sha256Digest,
    catalog: Incarnation,
) -> Result<(DeviceCatalogBinding, u64)> {
    let value = Configuration4::decode(bytes, installation, catalog)?;
    Ok((value.device_catalog, value.revision))
}

impl DeviceConfiguration {
    /// Migrates exact configuration/4 bytes bound to `transition.old` to
    /// configuration/5 bound to `transition.new`, with the revision bumped.
    /// Focus is kept as it was. A Context stage that selected `context_compact`
    /// selects `arbiter`, as it was otherwise. A Context stage that selected
    /// the retired `context_large` becomes disabled with no model; its
    /// enrollments, device policy and precision are kept and
    /// `context_retired` says why. Arbiter is never selected for it.
    pub fn migrate_v4(
        bytes: &[u8],
        installation: Sha256Digest,
        catalog: Incarnation,
        transition: &Configuration4Transition,
    ) -> Result<Self> {
        ensure!(
            transition.old != transition.new,
            "a configuration/4 transition must change the catalog binding"
        );
        let old = Configuration4::decode(bytes, installation, catalog)?;
        ensure!(
            old.device_catalog == transition.old,
            "configuration/4 is bound to a catalog the compiled transition does not name"
        );
        let mut context_retired = None;
        let mut stage = |old: Stage4, expected: Stage| -> Result<DeviceStageConfiguration> {
            let mut enabled = old.enabled;
            let selected_model = match (expected, old.selected_model) {
                (_, None) => None,
                (Stage::Focus, Some(SelectedModel4::Focus)) => Some(SelectedModel::Focus),
                (Stage::Context, Some(SelectedModel4::ContextCompact)) => {
                    Some(SelectedModel::Arbiter)
                }
                (Stage::Context, Some(SelectedModel4::ContextLarge)) => {
                    context_retired = Some(RetiredModel::ContextLarge);
                    enabled = false;
                    None
                }
                _ => anyhow::bail!("configuration/4 names a model of another stage"),
            };
            let value = DeviceStageConfiguration {
                enabled,
                selected_model,
                device_policy: old.device_policy,
                gpu_precision: old.gpu_precision,
                indexes: old.indexes,
            };
            value.validate(expected)?;
            Ok(value)
        };
        let focus = stage(old.focus, Stage::Focus)?;
        let context = stage(old.context, Stage::Context)?;
        let value = Self {
            schema: DEVICE_CONFIGURATION_SCHEMA.into(),
            installation: old.installation,
            catalog: old.catalog,
            revision: old
                .revision
                .checked_add(1)
                .context("configuration revision exhausted")?,
            device_catalog: transition.new.clone(),
            focus,
            context,
            context_retired,
        };
        value.validate()?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests;
