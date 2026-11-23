// SPDX-License-Identifier: Apache-2.0
//! Native policy views preserve typed3/4. These data grant no execution capability.
use airk_core::{
    arbiter::{
        Configuration, Enrollment, Profile, Stage, Task,
        device::{DeviceConfiguration, SelectedModel},
    },
    identity::{Incarnation, Sha256Digest},
    index_catalog::IndexId,
};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub enum NativeConfiguration {
    Legacy(Configuration),
    Device {
        document: DeviceConfiguration,
        selected: BTreeMap<Stage, Profile>,
    },
}
impl NativeConfiguration {
    pub fn device(document: DeviceConfiguration, selected: Vec<Profile>) -> Result<Self> {
        document.validate()?;
        let mut variants = BTreeMap::new();
        for profile in selected {
            profile.validate()?;
            let model = SelectedModel::from_model(profile.model)?;
            let stage = model.stage();
            let configured = document.stage(stage);
            ensure!(
                configured.enabled && configured.selected_model == Some(model),
                "variant differs from enabled selected model"
            );
            if let Some(execution) = &profile.device_execution {
                use airk_core::arbiter::device::{
                    DevicePolicy, DevicePrecision, GpuPrecisionPreference,
                };
                ensure!(
                    configured.device_policy != DevicePolicy::Cpu,
                    "forced CPU cannot select GPU"
                );
                ensure!(
                    match configured.gpu_precision {
                        GpuPrecisionPreference::Auto => true,
                        GpuPrecisionPreference::Fp32 =>
                            execution.precision == DevicePrecision::Float32,
                        GpuPrecisionPreference::Fp16 =>
                            execution.precision == DevicePrecision::Float16,
                    },
                    "GPU precision differs from current consent"
                );
            }
            ensure!(
                variants.insert(stage, profile).is_none(),
                "duplicate selected native stage"
            );
        }
        Ok(Self::Device {
            document,
            selected: variants,
        })
    }
    pub fn installation(&self) -> Sha256Digest {
        match self {
            Self::Legacy(v) => v.installation,
            Self::Device { document, .. } => document.installation,
        }
    }
    pub fn catalog(&self) -> Incarnation {
        match self {
            Self::Legacy(v) => v.catalog,
            Self::Device { document, .. } => document.catalog,
        }
    }
    pub fn revision(&self) -> u64 {
        match self {
            Self::Legacy(v) => v.revision,
            Self::Device { document, .. } => document.revision,
        }
    }
    pub fn enabled(&self, stage: Stage) -> bool {
        match self {
            Self::Legacy(v) => v.stage(stage).enabled,
            Self::Device { document, .. } => document.stage(stage).enabled,
        }
    }
    pub fn indexes(&self, stage: Stage) -> &[Enrollment] {
        match self {
            Self::Legacy(v) => &v.stage(stage).indexes,
            Self::Device { document, .. } => &document.stage(stage).indexes,
        }
    }
    pub fn selected_profile(&self, stage: Stage) -> Option<Sha256Digest> {
        if !self.enabled(stage) || self.indexes(stage).is_empty() {
            return None;
        }
        match self {
            Self::Legacy(v) => v.stage(stage).selected_profile,
            Self::Device { selected, .. } => selected.get(&stage).map(|v| v.manifest),
        }
    }
    pub fn eligible_for(&self, stage: Stage, index: IndexId, task: Task) -> bool {
        self.selected_profile(stage).is_some()
            && self
                .indexes(stage)
                .iter()
                .any(|v| v.index_id == index && v.tasks.contains(&task))
    }
    pub fn eligible(&self, index: IndexId, task: Task) -> bool {
        self.eligible_for(Stage::Context, index, task)
    }
    pub fn profile_for_index(&self, stage: Stage, index: IndexId) -> Option<Sha256Digest> {
        self.indexes(stage)
            .iter()
            .any(|v| v.index_id == index)
            .then(|| self.selected_profile(stage))
            .flatten()
    }
    pub fn same_document(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Legacy(a), Self::Legacy(b)) => a == b,
            (Self::Device { document: a, .. }, Self::Device { document: b, .. }) => a == b,
            _ => false,
        }
    }
    pub fn same_selection(&self, other: &Self) -> bool {
        self.same_document(other)
            && [Stage::Focus, Stage::Context]
                .into_iter()
                .all(|stage| self.selected_profile(stage) == other.selected_profile(stage))
    }
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::Legacy(v) => v.validate(),
            Self::Device { document, selected } => {
                Self::device(document.clone(), selected.values().cloned().collect())?;
                Ok(())
            }
        }
    }
    pub fn check_profile(&self, profile: &Profile) -> Result<()> {
        let stage = profile.declared_stage().context("native stage missing")?;
        ensure!(
            self.selected_profile(stage) == Some(profile.manifest),
            "native variant not selected"
        );
        if let Self::Device { selected, .. } = self {
            let admitted = selected
                .get(&stage)
                .context("selected native description missing")?;
            ensure!(
                serde_json::to_vec(admitted)? == serde_json::to_vec(profile)?,
                "native variant description changed"
            );
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::arbiter::device::{DeviceCatalogBinding, DeviceStageConfiguration};
    #[test]
    fn device_document_remains_typed_and_disabled_never_selects_a_variant() -> Result<()> {
        let install = Sha256Digest::from_bytes([1; 32]);
        let catalog = Incarnation::from_bytes([2; 16]);
        let binding = DeviceCatalogBinding {
            catalog: Sha256Digest::from_bytes([3; 32]),
            source: Sha256Digest::from_bytes([4; 32]),
        };
        let device = DeviceConfiguration::disabled(install, catalog, binding);
        let policy = NativeConfiguration::device(device.clone(), vec![])?;
        assert_eq!(policy.installation(), install);
        assert_eq!(policy.catalog(), catalog);
        assert_eq!(policy.revision(), 1);
        assert!(policy.same_document(&NativeConfiguration::device(device, vec![])?));
        assert!(
            !policy.same_document(&NativeConfiguration::Legacy(Configuration::disabled(
                install, catalog
            )))
        );
        assert!(policy.selected_profile(Stage::Context).is_none());
        assert!(!policy.enabled(Stage::Focus));
        assert_eq!(
            DeviceStageConfiguration::disabled().device_policy,
            airk_core::arbiter::device::DevicePolicy::Auto
        );
        Ok(())
    }
    #[test]
    fn source_binding_and_revision_are_part_of_current_document_not_advisory_selection()
    -> Result<()> {
        let binding = DeviceCatalogBinding {
            catalog: Sha256Digest::from_bytes([3; 32]),
            source: Sha256Digest::from_bytes([4; 32]),
        };
        let old = DeviceConfiguration::disabled(
            Sha256Digest::from_bytes([1; 32]),
            Incarnation::from_bytes([2; 16]),
            binding,
        );
        let policy = NativeConfiguration::device(old.clone(), vec![])?;
        let mut changed = old.clone();
        changed.device_catalog.source = Sha256Digest::from_bytes([9; 32]);
        assert!(!policy.same_document(&NativeConfiguration::device(changed, vec![])?));
        let mut changed = old;
        changed.revision = 2;
        assert!(!policy.same_document(&NativeConfiguration::device(changed, vec![])?));
        Ok(())
    }
}
