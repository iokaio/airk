// SPDX-License-Identifier: Apache-2.0
//! Scoped descriptive snapshots; never model, source or execution authority.
use super::*;
pub const SCOPED_DEVICE_STATUS_SCHEMA: &str = "airk.arbiter.scoped-device-status/1";
pub const MAX_SCOPED_DEVICE_STATUS_BYTES: usize = 256 << 10;
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScopedDeviceStage {
    pub application: IndexApplication,
    pub status: DeviceStageStatus,
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScopedDeviceObservation {
    Observed {
        focus: Box<ScopedDeviceStage>,
        context: Box<ScopedDeviceStage>,
    },
    Unavailable {
        reason: String,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScopedDeviceStatus {
    pub schema: String,
    pub scope: ScopeBinding,
    pub observation: ScopedDeviceObservation,
}
impl ScopedDeviceStatus {
    pub fn unavailable(mut scope: ScopeBinding, reason: &str) -> Result<Self> {
        scope.configuration_revision = 0;
        let value = Self {
            schema: SCOPED_DEVICE_STATUS_SCHEMA.into(),
            scope,
            observation: ScopedDeviceObservation::Unavailable {
                reason: reason.into(),
            },
        };
        value.validate()?;
        Ok(value)
    }
    /// Validate the complete native snapshot BEFORE projecting away all enrollment lists.
    pub fn from_status(status: DeviceStatus, mut scope: ScopeBinding) -> Result<Self> {
        status.validate()?;
        ensure!(
            status.initialization.is_none()
                && status.configuration.installation == scope.installation
                && status.configuration.catalog == scope.catalog,
            "scoped device snapshot differs from native installation/catalog"
        );
        scope.configuration_revision = status.configuration.revision;
        let stage = |kind: Stage, observed: DeviceStageStatus| -> Result<ScopedDeviceStage> {
            let configured = status.configuration.stage(kind);
            let enrolled = configured
                .indexes
                .iter()
                .find(|row| row.index_id == scope.index_id);
            let (availability, reason) = if !configured.enabled {
                (Availability::Disabled, "Optional stage is disabled.".into())
            } else if enrolled.is_none() {
                (
                    Availability::IndexDisabled,
                    "This immutable index is not enrolled.".into(),
                )
            } else {
                (observed.availability, observed.reason.clone())
            };
            let profile = if configured.enabled && enrolled.is_some() {
                observed.active_profile
            } else {
                None
            };
            Ok(ScopedDeviceStage {
                application: IndexApplication {
                    scope: scope.clone(),
                    profile,
                    availability,
                    reason,
                },
                status: observed,
            })
        };
        let focus = stage(Stage::Focus, status.focus)?;
        let context = stage(Stage::Context, status.context)?;
        let value = Self {
            schema: SCOPED_DEVICE_STATUS_SCHEMA.into(),
            scope,
            observation: ScopedDeviceObservation::Observed {
                focus: Box::new(focus),
                context: Box::new(context),
            },
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(&self) -> Result<()> {
        let publication = &self.scope.publication;
        ensure!(
            self.schema == SCOPED_DEVICE_STATUS_SCHEMA
                && !publication.collection.is_empty()
                && publication.collection.len() <= 4096
                && publication.incarnation == publication.token.incarnation
                && publication.generation == publication.token.generation
                && publication.revision == publication.token.revision,
            "invalid scoped device identity"
        );
        match &self.observation {
            ScopedDeviceObservation::Unavailable { reason } => ensure!(
                self.scope.configuration_revision == 0
                    && !reason.trim().is_empty()
                    && reason.len() <= 1024,
                "invalid unavailable device observation"
            ),
            ScopedDeviceObservation::Observed { focus, context } => {
                ensure!(
                    self.scope.configuration_revision > 0,
                    "observed device revision absent"
                );
                for (kind, row) in [(Stage::Focus, focus), (Stage::Context, context)] {
                    row.application.validate()?;
                    ensure!(
                        row.application.scope == self.scope,
                        "foreign scoped device application"
                    );
                    row.status.validate_observation(kind)?;
                    ensure!(
                        row.application.availability != Availability::Ready
                            || row.status.availability == Availability::Ready
                                && row.application.profile == row.status.active_profile,
                        "scoped ready application differs from native resident"
                    );
                }
            }
        }
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_SCOPED_DEVICE_STATUS_BYTES,
            "scoped device snapshot byte limit"
        );
        Ok(())
    }
}
#[cfg(test)]
mod tests;
