// SPDX-License-Identifier: Apache-2.0
//! Response contracts retained for explicitly unsupported historical APIs.
use airk_core::identity::RevisionToken;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryRemovalStage {
    IntentDurable,
    PackageDetached,
    PhysicalRemoved,
    ReceiptRetired,
    IntentComplete,
}

#[derive(Clone, Debug, Serialize)]
pub struct HistoryRemoval {
    pub token: RevisionToken,
    pub status: ConversionStatus,
    pub stage: Option<HistoryRemovalStage>,
    pub removed_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversionStage {
    IntentDurable,
    PackageVerified,
    PackagePublished,
    BindingCommitted,
    FullDetached,
    FullRemoved,
    IntentComplete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversionStatus {
    Complete,
    Pending,
    Protected,
}

#[derive(Clone, Debug, Serialize)]
pub struct HistoryConversion {
    pub token: RevisionToken,
    pub status: ConversionStatus,
    pub stage: Option<ConversionStage>,
    pub removed_bytes: u64,
}
