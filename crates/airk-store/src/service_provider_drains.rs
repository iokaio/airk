// SPDX-License-Identifier: Apache-2.0
//! Durable content-free provider ownership; no command, source or query payloads.
use airk_core::{identity::Incarnation, service::WorkerIdentity};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_PROVIDER_DRAINS: usize = 128;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProviderBoot {
    pub boot_id: Option<String>,
    pub process_scope: String,
    pub elapsed_ns: u64,
    pub monotonic_reset_proves_boot: bool,
}

impl ProviderBoot {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.boot_id.as_ref().is_none_or(|id| !id.is_empty()
                && id.len() <= 80
                && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')),
            "invalid native provider boot identity"
        );
        ensure!(
            !self.process_scope.is_empty()
                && self.process_scope.len() <= 128
                && !self.process_scope.chars().any(char::is_control),
            "invalid native provider process scope"
        );
        Ok(())
    }
    pub(super) fn changed_from(&self, old: &Self) -> bool {
        match (&self.boot_id, &old.boot_id) {
            (Some(current), Some(previous)) => current != previous,
            _ => {
                self.monotonic_reset_proves_boot
                    && old.monotonic_reset_proves_boot
                    && self.process_scope == old.process_scope
                    && self.elapsed_ns < old.elapsed_ns
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProviderDrain {
    pub version: u32,
    pub id: Incarnation,
    pub namespace: Incarnation,
    pub identity: WorkerIdentity,
    pub guardian_pid: u32,
    pub group: Option<u32>,
    pub boot: ProviderBoot,
}
impl ProviderDrain {
    pub(super) fn bytes(&self) -> Result<Vec<u8>> {
        ensure!(
            self.version == 1
                && self.guardian_pid > 1
                && self.guardian_pid <= i32::MAX as u32
                && self
                    .group
                    .is_none_or(|pid| pid > 1 && pid <= i32::MAX as u32)
                && self.identity.fencing_generation > 0,
            "invalid provider drain identity"
        );
        self.boot.validate()?;
        let bytes = serde_json::to_vec(self)?;
        ensure!(bytes.len() <= 4096, "provider drain metadata exceeds bound");
        Ok(bytes)
    }
}
