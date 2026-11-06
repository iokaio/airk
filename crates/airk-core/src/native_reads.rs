// SPDX-License-Identifier: Apache-2.0
//! Canonical native read requests shared by native transports and MCP adapters.
/// Native read capabilities are separate from the fixed MCP tool inventory.
#[derive(Clone, Copy, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportFormat {
    #[default]
    Json,
    Csv,
    Markdown,
}
impl ReportFormat {
    pub fn name(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Csv => "csv",
            Self::Markdown => "markdown",
        }
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum NativeRead {
    /// Pin an explicit bounded administrative input under the native owner's
    /// permanent-source policy and delivery fence. This is not an MCP tool or
    /// an evidence-scope permission to open arbitrary files.
    PrepareInputFile {
        path: String,
        max_source_bytes: u64,
    },
    /// Protected exact-publication source identity for combined composition.
    /// Administrative/legacy readers cannot obtain an installed scoped proof.
    SourceProjection {
        publication: crate::identity::RevisionToken,
    },
    /// Internal combined-plan final projection check. The originating scoped
    /// evidence session owns this request; administrative/legacy sessions may
    /// not use it to acquire source authority. This is not an MCP tool.
    VerifyProjection {
        value: serde_json::Value,
        controls: crate::verification::VerificationControls,
        #[serde(default)]
        allocation: Option<crate::verification::VerificationAllocation>,
        #[serde(default)]
        receipt_bytes_budget: Option<usize>,
    },
    /// A native file selector, opened only after read-scope admission.
    ValidateSettingsFile {
        view: SettingsView,
        path: String,
        cursor: Option<String>,
        max_bytes: usize,
    },
    ValidateSettings {
        view: SettingsView,
        input: crate::service_configuration::InputFile,
        cursor: Option<String>,
        max_bytes: usize,
    },
    Settings {
        view: SettingsView,
        cursor: Option<String>,
        max_bytes: usize,
    },
    UsageReport {
        days: u16,
        cursor: Option<String>,
        max_bytes: usize,
        #[serde(default)]
        format: ReportFormat,
        #[serde(default)]
        price: Option<crate::usage::PriceScenario>,
    },
    UsageReportFile {
        days: u16,
        price_scenario_path: String,
        cursor: Option<String>,
        max_bytes: usize,
        #[serde(default)]
        format: ReportFormat,
    },
    UsagePolicy,
}
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsView {
    ExtractionPolicy,
    VectorSelection,
    VectorStatus,
    Roots,
    ScanLimits,
    QuotaPolicy,
    QuotaStatus,
}

/// Shared input contract for the existing canonical FileInfo tool route. NativeRead
/// remains the settings/admin channel; there is no parallel FileInfo endpoint.
pub const FILE_INFO_PROFILE_FAMILIES_HELP: &str = "Include eighteen metadata-only effective profile family observations for this file. Collection policy is unavailable under partial Project scopes. Reporting adds no source reads or profile migration; existing verification remains independent.";
pub fn file_info_profile_families_schema() -> serde_json::Value {
    serde_json::json!({"type":"boolean","default":false,"description":FILE_INFO_PROFILE_FAMILIES_HELP})
}
