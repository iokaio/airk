// SPDX-License-Identifier: Apache-2.0
//! Additive, publication-bound textual-reference navigation contracts.
use crate::{discovery::MatchCount, search_filters::SearchFilters};
use serde::{Deserialize, Serialize};

/// `index` orders by stable candidate identity. Omitting all additive options
/// retains the legacy selection and envelope. Other explicit orders sort paths.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceOrder {
    #[default]
    Index,
    Path,
    CodeFirst,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceOptions {
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub order: ReferenceOrder,
    #[serde(default)]
    pub include_files: bool,
    #[serde(default)]
    pub include_count: bool,
    #[serde(flatten)]
    pub filters: SearchFilters,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceFileSummary {
    pub path: String,
    /// Matching records returned on this page, in ReferencePage.count_unit;
    /// never a whole-file total or a count of identifier token occurrences.
    pub count: usize,
}

/// Text bytes retain the legacy `max_bytes` meaning. This optional navigation
/// metadata is additional, bounded to 16 file summaries and 4 KiB of encoded
/// summary records. Totals are never obtained by an extra exhaustive scan.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReferencePage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    pub more: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incomplete_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_record_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<ReferenceFileSummary>>,
    pub files_truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_count: Option<MatchCount>,
    pub count_unit: String,
    pub coverage: ReferenceCoverage,
}

/// Exhausting lexical candidates is not proof of complete textual coverage:
/// identifiers can straddle the index's bounded chunk tokenization boundaries.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReferenceCoverage {
    pub basis: String,
    pub candidate_scan_complete: bool,
    pub query_complete: bool,
    pub query_total_matches: Option<usize>,
    pub token_boundary_complete: bool,
    pub line_completeness: String,
}
