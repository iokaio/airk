// SPDX-License-Identifier: Apache-2.0
//! Private Inno handoff facts. File ownership, actual bytes/signature and
//! finalized uninstall-log timing are native adapter obligations.
use super::{Executable, Installer, Refusal, decode, setup_recovery::InstallationImage};
use crate::identity::Incarnation;
use serde::{Deserialize, Serialize};

pub const HANDOFF_SCHEMA: &str = "airk-inno-handoff/1";
pub const HANDOFF_FILE: &str = "wrapper-handoff.json";
pub const ORIGINAL_OPERATION: &str = "original-operation/1";
pub const MAX_HANDOFF_BYTES: usize = 16 * 1024;

pub fn is_uninstaller_name(name: &str) -> bool {
    name.strip_prefix("unins")
        .and_then(|name| name.strip_suffix(".exe"))
        .is_some_and(|number| number.len() == 3 && number.bytes().all(|byte| byte.is_ascii_digit()))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InnoHandoff {
    pub schema: String,
    pub install_id: Incarnation,
    pub operation_id: Incarnation,
    pub version: String,
    pub capability: String,
    pub uninstaller: Executable,
    pub data: Executable,
}

impl InnoHandoff {
    pub fn decode(bytes: &[u8], image: &InstallationImage) -> Result<Self, Refusal> {
        let handoff: Self = decode(bytes, MAX_HANDOFF_BYTES, HANDOFF_SCHEMA, |record: &Self| {
            &record.schema
        })?;
        handoff.validate_for(image)?;
        Ok(handoff)
    }

    pub fn validate_for(&self, image: &InstallationImage) -> Result<(), Refusal> {
        image.validate()?;
        if self.schema != HANDOFF_SCHEMA {
            return Err(Refusal::UnknownSchema);
        }
        if self.install_id != image.record.install_id {
            return Err(Refusal::InstallIdMismatch);
        }
        if self.version != image.record.version {
            return Err(Refusal::VersionMismatch);
        }
        if self.operation_id != image.record.installed_by.operation_id
            || image.record.installed_by.installer != Installer::Inno
            || self.capability != ORIGINAL_OPERATION
            || self.uninstaller.bytes == 0
            || self.data.bytes == 0
        {
            return Err(Refusal::Malformed);
        }
        if !is_uninstaller_name(&self.uninstaller.path) {
            return Err(Refusal::Malformed);
        }
        let number = &self.uninstaller.path[5..8];
        if self.data.path != format!("unins{number}.dat") {
            return Err(Refusal::Malformed);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
