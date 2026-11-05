// SPDX-License-Identifier: Apache-2.0
//! Inert, transport-independent bounded retrieval plan contracts.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const REQUEST_SCHEMA: &str = "airk.parse.request/1";
pub const PLAN_SCHEMA: &str = "airk.parse.plan/1";
pub const RESULT_SCHEMA: &str = "airk.workbench.result/1";
pub const CATALOG: &str = "airk.evidence-plans/1";
/// Reports are expanded by typed bindings; they are not limited to the six
/// authored plan steps. Native composition retains every bounded fanout report.
pub const MAX_STEP_REPORTS: usize = 6 * 12;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Prepare,
    Execute,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct Budgets {
    pub calls: usize,
    pub fanout: usize,
    pub seconds: u64,
    pub max_bytes: usize,
}
impl Default for Budgets {
    fn default() -> Self {
        Self {
            calls: 12,
            fanout: 12,
            seconds: 30,
            max_bytes: 524288,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct Output {
    pub view: String,
    pub max_bytes: usize,
}
impl Default for Output {
    fn default() -> Self {
        Self {
            view: "raw".into(),
            max_bytes: 262144,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub arg: String,
    pub from: String,
    pub select: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub id: String,
    pub tool: String,
    #[serde(default)]
    pub args: Map<String, Value>,
    #[serde(default)]
    pub bind: Vec<Binding>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields, default)]
pub struct Plan {
    /// Frozen explicit combined selection; context preferences cannot widen it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_scope: Option<crate::combined_reads::QuerySelection>,
    /// Native authority identity captured at preparation, separate from evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_identity: Option<crate::identity::Sha256Digest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_scope: Option<crate::verification::VerificationScope>,
    #[serde(flatten)]
    pub verification: crate::verification::VerificationControls,
    pub schema: String,
    pub collection: String,
    pub path_prefix: String,
    pub publication: Option<String>,
    pub catalog: String,
    pub digest: String,
    pub tier: String,
    pub steps: Vec<Step>,
    pub budgets: Budgets,
    pub output: Output,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub id: String,
    #[serde(default)]
    pub parameters: Map<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    #[serde(flatten)]
    pub verification: crate::verification::VerificationControls,
    pub schema: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_detail: Option<crate::reply::ReplyDetail>,
    #[serde(default)]
    pub mode: Mode,
    pub text: Option<String>,
    pub recipe: Option<Recipe>,
    pub plan: Option<Plan>,
    #[serde(default)]
    pub path_prefix: String,
    pub budgets: Option<Budgets>,
    pub output: Option<Output>,
    pub expected_digest: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StepReport {
    pub id: String,
    pub tool: String,
    pub arguments: Value,
    pub status: String,
    pub elapsed_ms: u64,
    pub result: Option<Value>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RequiredInput {
    pub field: String,
    pub step: Option<String>,
    pub reason: String,
    pub choices: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ResultEnvelope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query_scope: Option<crate::combined_reads::QuerySelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_identity: Option<crate::identity::Sha256Digest>,
    /// Complete native member verification outcomes for a combined response.
    /// A single-index receipt cannot describe several collection authorities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member_verification: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_observation: Option<crate::scoped_reads::ScopeObservation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_verification: Option<crate::verification::VerificationReceipt>,
    pub schema: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_detail: Option<crate::reply::ReplyDetail>,
    pub collection: String,
    pub publication: Option<String>,
    pub status: String,
    pub executed: bool,
    pub plan: Plan,
    pub requires_input: Vec<RequiredInput>,
    pub steps: Vec<StepReport>,
    pub evidence: Vec<Value>,
    pub warnings: Vec<String>,
    pub truncated: bool,
    pub completeness: Vec<String>,
    pub calls: usize,
    pub payload_bytes: usize,
}
