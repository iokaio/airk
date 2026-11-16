// SPDX-License-Identifier: Apache-2.0
//! PRIVATE PROPOSAL under service_runtime::arbiter. No runtime/wire authority.
//! Final proof schema requires Root review and emitted refusal cases before mount.
use airk_core::{arbiter::*, identity::Sha256Digest};
use airk_retrieval::arbiter::publication::Publication;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

mod build_target;
mod calibration_equivalence;
mod composed_validation;
mod software_v2;
mod terminal;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedFile {
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: Sha256Digest,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Gate {
    NativeNumerical,
    NativeResource,
    ActualComposed,
    HeldoutA6,
    RightsClosure,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub model: ModelChoice,
    pub candidate_profile_sha256: Sha256Digest,
    pub assets_sha256: Sha256Digest,
    pub compiler: Sha256Digest,
    pub calibration: Sha256Digest,
    pub task_policy: Sha256Digest,
    pub runtime: String,
    pub execution_identity: String,
    pub target: String,
    pub task_scope: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateReceipt {
    pub schema: String,
    pub gate: Gate,
    pub binding: Binding,
    pub complete: bool,
    pub passed: bool,
    pub final_for_release: bool,
    pub synthetic_only: bool,
    pub actual_native_execution: bool,
    pub independent_heldout: bool,
    pub licensed_payloads: bool,
    /// Original independently retained records, never a copied pass flag.
    pub original_receipts: Vec<PinnedFile>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinalProof {
    pub schema: String,
    pub binding: Binding,
    pub gates: BTreeMap<Gate, PinnedFile>,
    pub component_target_provenance: PinnedFile,
    #[serde(default)]
    pub disk_budget_compatibility: Option<PinnedFile>,
    #[serde(default)]
    pub calibration_component_equivalence: Option<PinnedFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terminal_qualifications: Vec<terminal::TerminalProofSources>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_policy: Option<PinnedFile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiskBudgetCompatibility {
    pub schema: String,
    pub original_profile: PinnedFile,
    pub corrected_profile_sha256: Sha256Digest,
    pub source_formula: PinnedFile,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedDecision {
    pub schema: String,
    pub decision: String,
    pub approved_final_proofs: BTreeSet<Sha256Digest>,
    #[serde(default)]
    pub approved_disk_compatibilities: BTreeSet<Sha256Digest>,
    #[serde(default)]
    pub approved_rights_rosters: BTreeSet<Sha256Digest>,
    #[serde(default)]
    pub approved_calibration_component_equivalences: BTreeSet<Sha256Digest>,
    #[serde(default)]
    pub approved_component_target_provenances: BTreeSet<Sha256Digest>,
    #[serde(default)]
    pub approved_download_source_maps: BTreeSet<Sha256Digest>,
    #[serde(default)]
    pub approved_terminal_qualifications: BTreeSet<Sha256Digest>,
    #[serde(default)]
    pub approved_terminal_human_annotations: BTreeSet<Sha256Digest>,
    #[serde(default)]
    pub approved_terminal_native_captures: BTreeSet<Sha256Digest>,
    #[serde(default)]
    pub approved_terminal_native_execution_receipts: BTreeSet<Sha256Digest>,
    #[serde(default)]
    pub approved_terminal_independence_analyses: BTreeSet<Sha256Digest>,
    #[serde(default)]
    pub approved_terminal_criteria_freezes: BTreeSet<Sha256Digest>,
    #[serde(default)]
    pub approved_resource_policies: BTreeSet<Sha256Digest>,
    #[serde(default)]
    pub approved_software_v2_executions: BTreeSet<Sha256Digest>,
    #[serde(default)]
    pub approved_software_v2_reference_producers: BTreeSet<Sha256Digest>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub profile: PinnedFile,
    pub final_proof: PinnedFile,
    pub download_sources: PinnedFile,
    pub files: BTreeMap<String, PathBuf>,
}
#[derive(Serialize)]
#[serde(deny_unknown_fields)]
pub struct OfflineBundle {
    pub profile: Sha256Digest,
    pub qualification: Sha256Digest,
    pub directory: String,
    pub model: ModelChoice,
    pub stage: Stage,
    pub qualified_targets: Vec<QualifiedTarget>,
    pub cpu_fit: CpuFit,
    pub memory_reservation_bytes: u64,
    pub files: Vec<Asset>,
}
#[derive(Serialize)]
#[serde(deny_unknown_fields)]
pub struct OfflinePlan {
    pub schema: String,
    pub tag: String,
    pub catalog_sha256: Sha256Digest,
    pub promotion_decision_sha256: Sha256Digest,
    pub bundles: Vec<OfflineBundle>,
}
#[derive(Serialize)]
#[serde(deny_unknown_fields)]
pub struct PartSource {
    pub profile: Sha256Digest,
    pub asset_name: String,
    pub offset: u64,
    pub bytes: u64,
    pub sha256: Sha256Digest,
}
#[derive(Serialize)]
#[serde(deny_unknown_fields)]
pub struct Sources {
    pub schema: String,
    pub plan_sha256: Sha256Digest,
    pub publication_sha256: Sha256Digest,
    pub parts: BTreeMap<String, PartSource>,
    pub files: BTreeMap<Sha256Digest, BTreeMap<String, PathBuf>>,
}
pub struct Promoted {
    pub catalog: Vec<u8>,
    pub plan: Vec<u8>,
    pub sources_private: Vec<u8>,
    pub publication: Publication,
}
fn sha(raw: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(raw).into())
}
fn pinned(input: &PinnedFile) -> Result<Vec<u8>> {
    ensure!(
        input.path.is_absolute() && input.path.as_os_str().len() <= 4096,
        "native absolute bounded proof input path required"
    );
    ensure!(
        input.bytes > 0 && input.bytes <= 32 << 20,
        "proof/profile metadata bound"
    );
    let meta = fs::symlink_metadata(&input.path)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink() && meta.len() == input.bytes,
        "proof/profile object differs"
    );
    let mut file = fs::File::open(&input.path)?;
    let mut raw = Vec::new();
    (&mut file).take(input.bytes + 1).read_to_end(&mut raw)?;
    ensure!(
        raw.len() as u64 == input.bytes
            && sha(&raw) == input.sha256
            && file.metadata()?.len() == input.bytes,
        "proof/profile bytes differ"
    );
    Ok(raw)
}
fn binding(profile: &Profile, profile_sha: Sha256Digest, target: &str) -> Result<Binding> {
    let stage = profile.declared_stage().context("selected stage missing")?;
    let task = if stage == Stage::Focus {
        Task::FocusRank
    } else {
        Task::ContextAction
    };
    Ok(Binding {
        model: profile.model,
        candidate_profile_sha256: profile_sha,
        assets_sha256: sha(&serde_json::to_vec(&profile.assets)?),
        compiler: profile.compiler,
        calibration: profile.calibration,
        task_policy: *profile
            .task_policies
            .get(&task)
            .context("task policy missing")?,
        runtime: profile.runtime.clone(),
        execution_identity: profile
            .cpu_fit
            .as_ref()
            .context("CPU tier missing")?
            .runtime_identity()?,
        target: target.into(),
        task_scope: if stage == Stage::Focus {
            "relevance_ranking_only"
        } else {
            "generic_read_unknown_uncertain"
        }
        .into(),
    })
}
fn validate_gate(gate: &GateReceipt, kind: Gate, expected: &Binding) -> Result<()> {
    ensure!(
        gate.schema == "airk.private-final-stage-gate/1"
            && gate.gate == kind
            && gate.binding == *expected
            && gate.complete
            && gate.passed
            && gate.final_for_release
            && !gate.synthetic_only
            && !gate.original_receipts.is_empty(),
        "nonfinal, failed, synthetic-only or foreign gate"
    );
    ensure!(
        !matches!(
            kind,
            Gate::NativeNumerical | Gate::NativeResource | Gate::ActualComposed
        ) || gate.actual_native_execution,
        "native execution evidence missing"
    );
    ensure!(
        kind != Gate::HeldoutA6 || gate.independent_heldout,
        "held-out source evidence missing"
    );
    ensure!(
        kind != Gate::RightsClosure || gate.licensed_payloads,
        "exact licensed payload approval missing"
    );
    Ok(())
}

fn verify_named_seals(seals: &serde_json::Map<String, serde_json::Value>) -> Result<()> {
    ensure!(
        !seals.is_empty() && seals.len() <= 16384,
        "input seal census bound"
    );
    let mut buffer = vec![0u8; 1 << 20];
    for (name, pin) in seals {
        let path = Path::new(name);
        ensure!(
            path.is_absolute() && name.len() <= 4096,
            "original seal path must be bounded native absolute"
        );
        let expected: Sha256Digest = serde_json::from_value(pin.clone())?;
        let before = fs::symlink_metadata(path)?;
        ensure!(
            before.is_file() && !before.file_type().is_symlink() && before.len() <= (16u64 << 30),
            "original sealed object differs"
        );
        let mut file = fs::File::open(path)?;
        let mut hash = Sha256::new();
        let mut count = 0u64;
        loop {
            let got = file.read(&mut buffer)?;
            if got == 0 {
                break;
            }
            count = count
                .checked_add(got as u64)
                .context("seal size overflow")?;
            ensure!(count <= before.len(), "original sealed object grew");
            hash.update(&buffer[..got]);
        }
        ensure!(
            count == before.len()
                && file.metadata()?.len() == before.len()
                && Sha256Digest::from_bytes(hash.finalize().into()) == expected,
            "original heldout input closure drifted"
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rational {
    n: i128,
    d: i128,
}
impl Rational {
    fn reduced(n: i128, d: i128) -> Result<Self> {
        ensure!(d > 0 && n != i128::MIN, "rational metadata bound");
        let mut a = n.abs();
        let mut b = d;
        while b != 0 {
            let remainder = a % b;
            a = b;
            b = remainder;
        }
        Ok(Self {
            n: n / a.max(1),
            d: d / a.max(1),
        })
    }
    fn parse(value: &serde_json::Value) -> Result<Self> {
        let pair = value.as_array().context("original rational pair absent")?;
        ensure!(pair.len() == 2, "original rational pair cardinality");
        let n = i128::from(pair[0].as_i64().context("original rational numerator")?);
        let d = i128::from(pair[1].as_i64().context("original rational denominator")?);
        ensure!(
            d > 0 && n.abs() <= d,
            "original recall fraction outside[-1,1]"
        );
        Self::reduced(n, d)
    }
    fn plus(self, other: Self) -> Result<Self> {
        let left = self
            .n
            .checked_mul(other.d)
            .context("rational numerator overflow")?;
        let right = other
            .n
            .checked_mul(self.d)
            .context("rational numerator overflow")?;
        Self::reduced(
            left.checked_add(right).context("rational sum overflow")?,
            self.d
                .checked_mul(other.d)
                .context("rational denominator overflow")?,
        )
    }
    fn minus(self, other: Self) -> Result<Self> {
        self.plus(Self {
            n: -other.n,
            d: other.d,
        })
    }
    fn mean100(self) -> Result<Self> {
        Self::reduced(
            self.n,
            self.d.checked_mul(100).context("rational mean overflow")?,
        )
    }
}
fn critical_mean(
    row: &serde_json::Value,
    current: &serde_json::Value,
    baseline: &serde_json::Value,
    key: &str,
) -> Result<()> {
    let questions = row["per_question"]
        .as_array()
        .context("actual original critical population")?;
    ensure!(questions.len() == 100, "actual critical population count");
    let mut sum = Rational { n: 0, d: 1 };
    for question in questions {
        sum = sum.plus(Rational::parse(
            &question[format!("{key}_recall_delta_fraction")],
        )?)?;
    }
    let mean = sum.mean100()?;
    let difference = Rational::parse(&current[format!("{key}_mean_recall_fraction")])?.minus(
        Rational::parse(&baseline[format!("{key}_mean_recall_fraction")])?,
    )?;
    let reported = row[format!("{key}_mean_recall_delta")]
        .as_f64()
        .context("reported critical mean absent")?;
    ensure!(
        mean == difference
            && mean.n >= 0
            && reported.is_finite()
            && (reported - mean.n as f64 / mean.d as f64).abs() <= 1e-12,
        "recomputed original critical recall regressed or contradicts aggregate"
    );
    Ok(())
}

fn validate_quality_metrics(report: &serde_json::Value) -> Result<()> {
    let conditions = report["conditions"]
        .as_object()
        .context("actual four condition metrics absent")?;
    let candidates = [
        serde_json::to_value(ModelChoice::ContextQwenCompact)?,
        serde_json::to_value(ModelChoice::ContextQwenLarge)?,
    ];
    let names = candidates
        .iter()
        .map(|value| value.as_str().context("Context model name"))
        .collect::<Result<Vec<_>>>()?;
    let expected_names = BTreeSet::from([
        "ordinary_followup",
        "deterministic_packet",
        names[0],
        names[1],
    ]);
    ensure!(
        conditions
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            == expected_names,
        "actual four condition census differs"
    );
    let mut counts = BTreeMap::new();
    for (name, row) in conditions {
        let success = row["success_count"]
            .as_u64()
            .context("actual success count absent")?;
        ensure!(
            row["questions"] == 100 && success <= 100 && row["citation_identity_errors"] == 0,
            "population/citation accounting differs"
        );
        bounded_number(&row["completion"], 1.0)?;
        ensure!(
            (row["completion"].as_f64().context("completion")? - success as f64 / 100.0).abs()
                < 1e-12,
            "completion count/rate differs"
        );
        counts.insert(name.as_str(), success);
    }

    let critical_rows = report["critical_evidence_comparisons"]
        .as_array()
        .context("critical population absent")?;
    let original_ids = critical_rows
        .first()
        .context("critical population absent")?["per_question"]
        .as_array()
        .context("critical population absent")?;
    let all_ids = original_ids
        .iter()
        .map(|row| {
            row["id"]
                .as_str()
                .context("original task identifier")
                .map(str::to_owned)
        })
        .collect::<Result<BTreeSet<_>>>()?;
    ensure!(
        original_ids.len() == 100 && all_ids.len() == 100,
        "original fixed100 IDs differ"
    );
    let mut success_populations = BTreeMap::new();
    let mut success_turns = BTreeMap::new();
    for (name, row) in conditions {
        let failures = row["failures"]
            .as_array()
            .context("original failure population absent")?;
        let mut failed_ids = BTreeSet::new();
        let mut failed_turns = 0u64;
        for failure in failures {
            let id = failure["task_id"]
                .as_str()
                .context("original failed task ID")?;
            let turns = failure["external_model_turns"]
                .as_u64()
                .context("original failed task turns")?;
            ensure!(
                all_ids.contains(id) && failed_ids.insert(id.to_owned()) && turns <= 8,
                "original failure population/turn ceiling differs"
            );
            failed_turns += turns;
        }
        ensure!(
            failed_ids.len() as u64 + counts[name.as_str()] == 100,
            "actual success/failure count contradicts fixed population"
        );
        let mean = row["all_task_mean_external_turns"]
            .as_f64()
            .context("original all-task turn mean absent")?;
        ensure!(
            mean.is_finite()
                && (0.0..=8.0).contains(&mean)
                && (mean * 100.0 - (mean * 100.0).round()).abs() < 1e-9,
            "original all-task turn integer census differs"
        );
        let total = (mean * 100.0).round() as u64;
        let completed = total
            .checked_sub(failed_turns)
            .context("completed task turn sum contradicts failures")?;
        ensure!(
            completed <= 8 * counts[name.as_str()],
            "original completed task turn ceiling differs"
        );
        success_turns.insert(name.as_str(), completed);
        success_populations.insert(
            name.as_str(),
            all_ids
                .difference(&failed_ids)
                .cloned()
                .collect::<BTreeSet<_>>(),
        );
    }
    let strongest = counts["ordinary_followup"].max(counts["deterministic_packet"]);
    let selected = report["selected_strongest_baseline"]
        .as_str()
        .context("actual baseline selection absent")?;
    ensure!(
        ["ordinary_followup", "deterministic_packet"].contains(&selected)
            && counts[selected] == strongest,
        "selected baseline is not strongest normal control"
    );

    let expected_baseline = if counts["ordinary_followup"] > counts["deterministic_packet"]
        || (counts["ordinary_followup"] == counts["deterministic_packet"]
            && success_turns["ordinary_followup"] < success_turns["deterministic_packet"])
    {
        "ordinary_followup"
    } else {
        "deterministic_packet"
    };
    ensure!(
        selected == expected_baseline,
        "actual strongest baseline tie rule differs"
    );
    for name in &names {
        ensure!(
            counts[name] >= 75 && counts[name] + 2 >= strongest,
            "actual completion floor/regression"
        );
    }
    let comparisons = report["paired_common_success"]
        .as_array()
        .context("actual paired turn observations absent")?;
    ensure!(
        comparisons.len() == 4,
        "both controls for both models required"
    );
    let mut pairs = BTreeSet::new();
    for row in comparisons {
        let candidate = row["candidate"].as_str().context("paired candidate")?;
        let baseline = row["baseline"].as_str().context("paired baseline")?;
        ensure!(
            names.contains(&candidate)
                && ["ordinary_followup", "deterministic_packet"].contains(&baseline)
                && pairs.insert((candidate, baseline)),
            "paired roster differs"
        );
        let ids = row["common_success_ids"]
            .as_array()
            .context("paired original identifiers absent")?;
        ensure!(
            row["common_success_questions"].as_u64() == Some(ids.len() as u64)
                && ids.len() <= 100
                && ids
                    .iter()
                    .map(|id| id.as_str())
                    .collect::<BTreeSet<_>>()
                    .len()
                    == ids.len()
                && ids.iter().all(|id| id.is_string()),
            "paired fixed-population identities differ"
        );

        let original_shared = success_populations[candidate]
            .intersection(&success_populations[baseline])
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        ensure!(
            ids.iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<BTreeSet<_>>()
                == original_shared,
            "paired IDs/counts do not match original common-success population"
        );
        let candidate_turns = row["candidate_turns"]
            .as_u64()
            .context("candidate turns absent")?;
        let baseline_turns = row["baseline_turns"]
            .as_u64()
            .context("baseline turns absent")?;
        ensure!(
            candidate_turns <= 8 * ids.len() as u64 && baseline_turns <= 8 * ids.len() as u64,
            "original turn ceiling exceeded"
        );

        for (name, paired) in [(candidate, candidate_turns), (baseline, baseline_turns)] {
            let excluded = success_populations[name].len() - original_shared.len();
            ensure!(
                paired <= success_turns[name]
                    && success_turns[name] - paired <= 8 * excluded as u64,
                "paired turn sum contradicts original completed task census"
            );
        }
        if baseline == selected {
            ensure!(
                baseline_turns > 0 && candidate_turns * 10 <= baseline_turns * 9,
                "actual paired primary turn reduction below10percent"
            );
        }
    }
    let critical = report["critical_evidence_comparisons"]
        .as_array()
        .context("actual critical evidence metrics absent")?;
    ensure!(critical.len() == 2, "critical comparison census differs");
    let mut critical_candidates = BTreeSet::new();
    for row in critical {
        let candidate = row["candidate"].as_str().context("critical candidate")?;
        ensure!(
            names.contains(&candidate)
                && critical_candidates.insert(candidate)
                && row["baseline"] == selected
                && row["citation_comparison_state"] == "observed",
            "critical baseline/observation differs"
        );
        for key in ["shown_mean_recall_delta", "cited_mean_recall_delta"] {
            bounded_number(&row[key], 1.0)?;
        }

        ensure!(
            row["per_question"]
                .as_array()
                .context("critical original IDs")?
                .iter()
                .filter_map(|value| value["id"].as_str())
                .collect::<BTreeSet<_>>()
                == all_ids.iter().map(String::as_str).collect::<BTreeSet<_>>(),
            "critical comparison original task IDs differ"
        );
        let current = &conditions[candidate]["critical_evidence"];
        let baseline = &conditions[selected]["critical_evidence"];
        critical_mean(row, current, baseline, "shown")?;
        critical_mean(row, current, baseline, "cited")?;
        let current_valid = current["agent_valid_references"]
            .as_u64()
            .context("candidate citation census")?;
        let current_total = current["agent_total_references"]
            .as_u64()
            .context("candidate citation total")?;
        let baseline_valid = baseline["agent_valid_references"]
            .as_u64()
            .context("baseline citation census")?;
        let baseline_total = baseline["agent_total_references"]
            .as_u64()
            .context("baseline citation total")?;
        ensure!(
            current_total > 0
                && baseline_total > 0
                && current_valid <= current_total
                && baseline_valid <= baseline_total
                && u128::from(current_valid) * u128::from(baseline_total)
                    >= u128::from(baseline_valid) * u128::from(current_total),
            "actual final citation precision regressed/unobserved"
        );
        let questions = row["per_question"]
            .as_array()
            .context("original critical task census absent")?;
        ensure!(
            questions.len() == 100
                && questions
                    .iter()
                    .map(|row| row["id"].as_str())
                    .collect::<BTreeSet<_>>()
                    .len()
                    == 100
                && questions.iter().all(|row| row["id"].is_string()),
            "critical fixed100 population differs"
        );
    }
    Ok(())
}

fn validate_quality(
    model: ModelChoice,
    manifest: Sha256Digest,
    expected: &Binding,
    originals: &[(Sha256Digest, serde_json::Value)],
) -> Result<()> {
    let reports = originals
        .iter()
        .filter(|(_, value)| value["schema"] == "airk.private-native-goal-comparison/1")
        .collect::<Vec<_>>();
    ensure!(
        reports.len() == 1,
        "actual complete A6 report required, not numeric/resource/CAL evidence"
    );
    let report = &reports[0].1;
    validate_quality_metrics(report)?;
    for field in [
        "a6_goal_comparison_pass",
        "all_optional_completion_floors_pass",
        "all_candidate_completion_comparisons_pass",
        "primary_turn_floor_pass",
        "zero_invalid_citation_references",
    ] {
        ensure!(
            report[field] == true,
            "actual held-out comparison acceptance failed or missing: {field}"
        );
    }
    ensure!(
        report["evaluation"] == true && report["release_qualified"] == false,
        "raw held-out evidence must retain its unqualified identity"
    );
    let seals = report["input_seals"]
        .as_object()
        .context("held-out input seals absent")?;
    ensure!(
        !seals.is_empty() && report["actual_execution_identity"].is_object(),
        "actual execution/input closure absent"
    );
    verify_named_seals(seals)?;
    let ready = originals
        .iter()
        .filter(|(_, value)| value["schema"] == "airk.private-arbiter-fixture/1")
        .collect::<Vec<_>>();
    ensure!(
        ready.len() == 1
            && seals
                .values()
                .any(|value| value == &serde_json::json!(ready[0].0)),
        "actual Ready snapshot not sealed by held-out report"
    );
    let model_value = serde_json::to_value(model)?;
    ensure!(
        ready[0].1["profiles"]
            .as_array()
            .context("Ready profiles absent")?
            .iter()
            .any(|row| row["model"] == model_value
                && row["profile_sha256"] == serde_json::json!(expected.candidate_profile_sha256)
                && row["manifest"] == serde_json::json!(manifest)),
        "held-out actual profile snapshot differs"
    );
    let contributions = report["selected_Context_native_contribution"]
        .as_object()
        .context("actual Context contribution absent")?;
    let models = if model.stage() == Some(Stage::Focus) {
        vec![
            ModelChoice::ContextQwenCompact,
            ModelChoice::ContextQwenLarge,
        ]
    } else {
        vec![model]
    };
    for model in models {
        let name = serde_json::to_value(model)?;
        let row = contributions
            .get(name.as_str().context("model name")?)
            .context("model contribution absent")?;
        ensure!(
            row["contribution_observed"] == true
                && row["human_semantic_gold"] == false
                && row["causal_choice_attribution"] == false
                && row["packets_with_qualified_generic_Read_selected"]
                    .as_u64()
                    .is_some_and(|value| value > 0)
                && row["tasks_with_actual_additional_Read_original_reference_gain"]
                    .as_u64()
                    .is_some_and(|value| value > 0),
            "actual generic Read/reference gain absent"
        );
    }
    let critical = report["critical_evidence_comparisons"]
        .as_array()
        .context("critical-evidence comparisons absent")?;
    ensure!(
        critical.len() == 2
            && critical
                .iter()
                .all(|row| row["shown_critical_evidence_floor_pass"] == true
                    && row["cited_critical_evidence_floor_pass"] == true
                    && row["agent_citation_precision_no_regression"] == true),
        "critical evidence/citation regression or missing comparison"
    );
    Ok(())
}

type Original = (Sha256Digest, serde_json::Value);
fn original_schema<'a>(records: &'a [Original], schema: &str) -> Result<&'a Original> {
    let rows = records
        .iter()
        .filter(|(_, value)| value["schema"] == schema)
        .collect::<Vec<_>>();
    ensure!(
        rows.len() == 1,
        "exactly one actual original {schema} required"
    );
    Ok(rows[0])
}
fn finite_array(value: &serde_json::Value, count: usize) -> Result<()> {
    let values = value.as_array().context("numeric scores absent")?;
    ensure!(
        values.len() == count
            && values
                .iter()
                .all(|value| value.as_f64().is_some_and(f64::is_finite)),
        "score count/nonfinite original evidence"
    );
    Ok(())
}
fn bounded_number(value: &serde_json::Value, maximum: f64) -> Result<()> {
    ensure!(
        value
            .as_f64()
            .is_some_and(|value| value.is_finite() && value >= 0.0 && value <= maximum),
        "missing/nonfinite/out-of-bound original measurement"
    );
    Ok(())
}
fn actual_identity(value: &serde_json::Value, profile: &Profile, expected: &Binding) -> Result<()> {
    ensure!(
        value["profile"] == serde_json::json!(profile.manifest)
            && value["profile_source_sha256"]
                == serde_json::json!(expected.candidate_profile_sha256)
            && value["execution_identity"][0] == expected.execution_identity
            && value["execution_identity"][1]
                .as_u64()
                .is_some_and(|cores| profile
                    .cpu_fit
                    .as_ref()
                    .is_some_and(|fit| usize::try_from(cores)
                        .is_ok_and(|cores| cores >= fit.minimum_physical_cores))),
        "actual native profile/CPU identity differs"
    );
    Ok(())
}
fn settled_owner(value: &serde_json::Value) -> Result<()> {
    ensure!(
        value["complete"] == true
            && value.get("failure") == Some(&serde_json::Value::Null)
            && value["post_input_drift"] == serde_json::json!([])
            && value["whole_phase_deadline_not_refreshed"] == true
            && value["whole_phase_expired"] == false
            && value["no_automatic_retry"] == true
            && value["other_processes_touched"] == false,
        "original phase incomplete/drifted/renewed/foreign process"
    );
    natural_owned_process(&value["owned"])
}
fn natural_owned_process(owned: &serde_json::Value) -> Result<()> {
    ensure!(
        owned["exit_code"] == 0
            && owned["timed_out"] == false
            && owned["log_oversized"] == false
            && owned["owned_descendants_settled"] == true
            && owned["owned_job_forced_termination"] == false
            && owned["owned_job_final_active_processes"] == 0
            && owned["owned_job_final_member_pids"] == serde_json::json!([])
            && owned["parent_assigned_to_job"] == false
            && owned["child_assigned_before_resume"] == true
            && owned["owned_total_processes"] == 1
            && owned["unexpected_third_process"] == false
            && owned["whole_process_deadline_not_refreshed"] == true,
        "actual owned process did not settle naturally"
    );
    let exits = owned["retained_member_exits"]
        .as_array()
        .context("owned exits absent")?;
    ensure!(
        exits.len() == 1 && exits[0]["exit_observed"] == true && exits[0]["exit_code"] == 0,
        "actual member exit absent"
    );
    let close = &owned["handle_close_proof"];
    ensure!(
        close["forced"] == false
            && close["settled"] == true
            && close["final_active"] == 0
            && close["members_before"] == serde_json::json!([])
            && close["final_member_pids"] == serde_json::json!([]),
        "handle close did not prove empty owned job"
    );
    Ok(())
}
fn measured_twenty(resource: &serde_json::Value, peak_key: &str, reservation: u64) -> Result<()> {
    let samples = resource["samples_us"]
        .as_array()
        .context("twenty resource samples absent")?;
    ensure!(samples.len() == 20, "partial resource samples");
    let mut sorted = samples
        .iter()
        .map(|value| value.as_u64().context("noninteger resource duration"))
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        sorted
            .iter()
            .all(|value| *value > 0 && *value <= 30_000_000),
        "original 30s resource deadline exceeded"
    );
    sorted.sort_unstable();
    ensure!(
        resource["nearest_rank_p95_us"] == sorted[18]
            && resource["maximum_us"] == sorted[19]
            && resource[peak_key]
                .as_u64()
                .is_some_and(|peak| peak > 0 && peak <= reservation),
        "resource quantile or actual resident limit differs"
    );
    Ok(())
}

fn numeric_values(value: &serde_json::Value, count: usize) -> Result<Vec<f32>> {
    finite_array(value, count)?;
    let values: Vec<f32> = serde_json::from_value(value.clone())?;
    ensure!(
        values.iter().all(|value| value.is_finite()),
        "original score overflows float32"
    );
    Ok(values)
}
fn winner(values: &[f32]) -> usize {
    values.iter().enumerate().fold(
        0,
        |best, (index, value)| if *value > values[best] { index } else { best },
    )
}
fn compare_focus_values(
    actual: &[f32],
    reference: &[f32],
    raw_bound: f64,
    relevance_bound: f64,
) -> Result<()> {
    let relevance = |values: &[f32]| {
        let peak = f64::from(values[0].max(values[1]));
        let no = (f64::from(values[0]) - peak).exp();
        let yes = (f64::from(values[1]) - peak).exp();
        yes / (yes + no)
    };
    let error = actual
        .iter()
        .zip(reference)
        .map(|(a, b)| (f64::from(*a) - f64::from(*b)).abs())
        .fold(0.0, f64::max);
    ensure!(
        error <= raw_bound
            && (relevance(actual) - relevance(reference)).abs() <= relevance_bound
            && (actual[1] > actual[0]) == (reference[1] > reference[0]),
        "recomputed actual Focus logits/relevance/winner differs"
    );
    Ok(())
}

fn validate_focus_native(
    kind: Gate,
    profile: &Profile,
    expected: &Binding,
    records: &[Original],
) -> Result<()> {
    ensure!(
        profile.max_tokens == 512,
        "historical Focus evidence is static512 only"
    );
    let numerical = kind == Gate::NativeNumerical;
    let schema = if numerical {
        "airk.private-focus-native-parity/1"
    } else {
        "airk.private-focus-native-resource/1"
    };
    let (capture_sha, capture) = original_schema(records, schema)?;
    actual_identity(capture, profile, expected)?;
    ensure!(
        capture["complete"] == true
            && capture["evaluation"] == true
            && capture["release_qualified"] == false,
        "component evidence identity lost"
    );
    let (_, execution) = original_schema(records, "airk.private-focus-native-execution-receipt/1")?;
    ensure!(
        execution["stage"] == if numerical { "parity" } else { "resource" }
            && execution["capture_sha256"] == serde_json::json!(capture_sha)
            && execution["quality_or_release_qualified"] == false,
        "native execution does not bind actual component capture"
    );
    settled_owner(execution)?;
    let requests = records
        .iter()
        .filter(|(_, value)| {
            value["packet_sha256"] == execution["packet_sha256"] && value["command"].is_array()
        })
        .collect::<Vec<_>>();
    ensure!(requests.len() == 1, "original frozen native request absent");
    let command = requests[0].1["command"]
        .as_array()
        .context("native command")?;
    let request = &requests[0].1;
    let pins = request["pins"]
        .as_object()
        .context("original native input seals absent")?;
    let image = &execution["owned"]["suspended_assignment"]["image"];
    let executable = command
        .first()
        .and_then(|value| value.as_str())
        .context("actual native executable absent")?;
    ensure!(
        image["path"] == executable && pins.get(executable) == image.get("sha256"),
        "actual native image not sealed by request"
    );
    ensure!(
        pins.values()
            .any(|value| value == &serde_json::json!(expected.candidate_profile_sha256))
            && pins
                .values()
                .any(|value| value == &capture["fixture_sha256"]),
        "actual profile/fixture missing from original frozen inputs"
    );
    for asset in &profile.assets {
        ensure!(
            pins.values()
                .any(|value| value == &serde_json::json!(asset.sha256)),
            "actual asset missing from original frozen inputs"
        );
    }
    let selector = if numerical {
        "inference_engine::pool::arbiter::qualification::focus::focus_native_weight8_static512_matches_publisher_and_compiled_model_views"
    } else {
        "inference_engine::pool::arbiter::qualification::focus::focus_native_cpu4_tier_records_maximum512_pair_resources"
    };
    ensure!(
        command.iter().any(|value| value == selector)
            && command.iter().any(|value| value == "--exact"),
        "wrong actual native test selector"
    );
    if numerical {
        ensure!(
            capture["policy"] == serde_json::json!(expected.task_policy),
            "actual numeric task policy differs"
        );
        let rows = capture["rows"].as_array().context("numeric rows absent")?;
        ensure!(
            rows.len() == 12
                && rows
                    .iter()
                    .filter(|row| row["native_model_view"] == true)
                    .count()
                    == 10,
            "frozen 12/10 numeric fixture coverage absent"
        );
        for row in rows {
            for key in ["raw_scores", "publisher_scores", "python_weight8_scores"] {
                finite_array(&row[key], 2)?;
            }
            let actual = numeric_values(&row["raw_scores"], 2)?;
            compare_focus_values(
                &actual,
                &numeric_values(&row["publisher_scores"], 2)?,
                0.25,
                0.05,
            )?;
            compare_focus_values(
                &actual,
                &numeric_values(&row["python_weight8_scores"], 2)?,
                0.001,
                0.00001,
            )?;
            for (key, bound) in [
                ("publisher_raw_error", 0.25),
                ("publisher_relevance_error", 0.05),
                ("runtime_raw_error", 0.001),
                ("runtime_relevance_error", 0.00001),
            ] {
                bounded_number(&row[key], bound)?;
            }
            ensure!(
                row["publisher_argmax_matches"] == true && row["runtime_argmax_matches"] == true,
                "actual native winner differs"
            );
        }
    } else {
        measured_twenty(
            capture,
            "peak_process_rss_bytes",
            profile.memory_reservation_bytes,
        )?;
        ensure!(
            execution["owned"]["peak_host_committed_bytes"]
                .as_u64()
                .is_some_and(|peak| peak > 0 && peak <= profile.memory_reservation_bytes),
            "actual committed memory exceeds candidate reservation"
        );
        ensure!(
            capture["input"]["input_ids"]
                .as_array()
                .is_some_and(|ids| ids.len() == 512)
                && capture["input"]["candidate_positions"] == serde_json::json!([2152, 9693]),
            "actual resource shape differs"
        );
        let mask = capture["input"]["attention_mask"]
            .as_array()
            .context("actual Focus resource mask absent")?;
        ensure!(
            mask.len() == 512
                && mask
                    .iter()
                    .all(|value| matches!(value.as_u64(), Some(0 | 1)))
                && mask
                    .iter()
                    .filter(|value| value.as_u64() == Some(1))
                    .count()
                    >= 480
                && capture["input"]["option_mask"] == serde_json::json!([true, true]),
            "actual Focus maximum live input absent"
        );
    }
    Ok(())
}
fn validate_context_native(
    profile: &Profile,
    expected: &Binding,
    records: &[Original],
) -> Result<()> {
    ensure!(
        profile.max_tokens == 2048,
        "historical Context evidence cannot establish8192 fit"
    );
    let (capture_sha, capture) =
        original_schema(records, "airk.private-context-native-numeric-resource/1")?;
    actual_identity(capture, profile, expected)?;
    ensure!(
        capture["complete"] == true
            && capture["model"] == serde_json::json!(profile.model)
            && capture["candidate_cpu_fit"] == serde_json::to_value(&profile.cpu_fit)?
            && capture["candidate_memory_reservation_bytes"] == profile.memory_reservation_bytes
            && capture["semantic_or_CAL_or_publication_qualified"] == false
            && capture["combined_service_admission_verified"] == false,
        "actual Context component identity differs"
    );
    let fixture_digest: Sha256Digest = serde_json::from_value(capture["fixture_sha256"].clone())?;
    let fixtures = records
        .iter()
        .filter(|(digest, _)| *digest == fixture_digest)
        .collect::<Vec<_>>();
    ensure!(
        fixtures.len() == 1,
        "actual frozen Context numeric fixture bytes absent"
    );
    let fixture_rows = fixtures[0].1["rows"]
        .as_array()
        .context("actual Context fixture rows absent")?;
    ensure!(
        fixture_rows.len() == 3,
        "actual Context fixture census differs"
    );
    let rows = capture["numeric_rows"]
        .as_array()
        .context("Context numeric rows absent")?;
    ensure!(rows.len() == 3, "Context all-live numeric coverage absent");
    for (row_index, (row, (tokens, options))) in
        rows.iter().zip([(17, 3), (128, 8), (2048, 8)]).enumerate()
    {
        ensure!(
            row["tokens"] == tokens
                && row["options"] == options
                && row["all_live"] == true
                && row["argmax_matches"] == true,
            "Context frozen shape/winner differs"
        );

        let fixture = &fixture_rows[row_index];
        let actual = numeric_values(&row["raw_scores"], options)?;
        let python = numeric_values(&fixture["python_fp32_known_live"], options)?;
        let original = numeric_values(&fixture["original_fp32_all_live"], options)?;
        let mask: Vec<bool> = serde_json::from_value(fixture["known_live_mask"].clone())?;
        ensure!(
            mask.len() == options && mask.iter().any(|live| *live),
            "actual known-live reference mask absent"
        );
        let actual_known = actual
            .iter()
            .zip(&mask)
            .filter_map(|(value, live)| live.then_some(*value))
            .collect::<Vec<_>>();
        let python_known = python
            .iter()
            .zip(&mask)
            .filter_map(|(value, live)| live.then_some(*value))
            .collect::<Vec<_>>();
        let error = actual_known
            .iter()
            .zip(&python_known)
            .map(|(a, b)| (a - b).abs())
            .fold(0_f32, f32::max);
        let original_error = actual
            .iter()
            .zip(&original)
            .map(|(a, b)| (a - b).abs())
            .fold(0_f32, f32::max);
        ensure!(
            error <= 0.001
                && original_error <= 0.001
                && winner(&actual_known) == winner(&python_known)
                && winner(&actual) == winner(&original),
            "recomputed actual Context numeric reference/winner differs"
        );
        finite_array(&row["raw_scores"], options)?;
        bounded_number(&row["native_python_fp32_max_error"], 0.001)?;
        bounded_number(&row["native_original_fp32_max_error"], 0.001)?;
    }
    let resource = &capture["resource"];
    ensure!(
        resource["input"]["input_ids"]
            .as_array()
            .is_some_and(|ids| ids.len() == 2048)
            && resource["input"]["candidate_positions"]
                .as_array()
                .is_some_and(|positions| positions.len() == 8)
            && resource["input"]["option_mask"] == serde_json::json!(vec![true; 8]),
        "actual maximum Context resource shape absent"
    );
    measured_twenty(
        resource,
        "peak_process_lifetime_rss_bytes",
        profile.memory_reservation_bytes,
    )?;
    ensure!(
        resource["measurement_complete"] == true
            && resource["candidate_numeric_resource_passed"] == true
            && resource["completed_samples"] == 20
            && resource["required_samples"] == 20
            && resource["original_product_deadline_us"] == 30_000_000,
        "partial/failed Context resource work"
    );

    let (_, execution) =
        original_schema(records, "airk.private-context-native-execution-receipt/1")?;
    ensure!(
        execution["model"] == serde_json::json!(profile.model)
            && execution["capture_sha256"] == serde_json::json!(capture_sha)
            && execution["quality_or_CAL_or_combined_or_release_qualified"] == false,
        "actual Context owned execution does not bind component capture"
    );
    settled_owner(execution)?;
    let requests = records
        .iter()
        .filter(|(_, value)| {
            value["packet_sha256"] == execution["packet_sha256"] && value["command"].is_array()
        })
        .collect::<Vec<_>>();
    ensure!(
        requests.len() == 1,
        "original Context frozen request absent"
    );
    let request = &requests[0].1;
    let command = request["command"]
        .as_array()
        .context("Context command absent")?;
    ensure!(command.iter().any(|value|value=="inference_engine::pool::arbiter::qualification::context::selected_context_fp32_native_numeric_and_resource_candidate")
        &&command.iter().any(|value|value=="--exact"),"wrong actual Context native selector");
    let pins = request["pins"]
        .as_object()
        .context("Context original input seals absent")?;
    let executable = command
        .first()
        .and_then(|value| value.as_str())
        .context("Context executable absent")?;
    let image = &execution["owned"]["suspended_assignment"]["image"];
    ensure!(
        image["path"] == executable && pins.get(executable) == image.get("sha256"),
        "Context executable lacks original seal"
    );
    ensure!(
        pins.values()
            .any(|value| value == &serde_json::json!(expected.candidate_profile_sha256))
            && pins
                .values()
                .any(|value| value == &capture["fixture_sha256"]),
        "Context profile/fixture not frozen"
    );
    for asset in &profile.assets {
        ensure!(
            pins.values()
                .any(|value| value == &serde_json::json!(asset.sha256)),
            "Context logical asset missing from original seals"
        );
    }
    let host_limit = request["limits"]["host_committed_bytes"]
        .as_u64()
        .context("original Context process owner limit absent")?;
    ensure!(
        host_limit >= profile.memory_reservation_bytes
            && execution["owned"]["peak_host_committed_bytes"]
                .as_u64()
                .is_some_and(|peak| peak > 0 && peak <= host_limit),
        "Context native process exceeded original owner limit"
    );
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RightsAsset {
    asset: Asset,
    provenance: String,
    license_spdx: String,
    /// Explicitly names the actual model-card/README/license basis; a base
    /// model LICENSE is never relabelled as the reranker's same-repo LICENSE.
    license_basis: Vec<Sha256Digest>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RightsRoster {
    schema: String,
    model: ModelChoice,
    assets_sha256: Sha256Digest,
    assets: Vec<RightsAsset>,
    license_and_notice_files: Vec<PinnedFile>,
}
fn validate_rights(
    profile: &Profile,
    expected: &Binding,
    decision: &RecordedDecision,
    records: &[Original],
) -> Result<()> {
    let (record_sha, value) = original_schema(records, "airk.private-reviewed-model-rights/1")?;
    ensure!(
        decision.approved_rights_rosters.contains(record_sha),
        "actual exact rights roster has no recorded Root approval"
    );
    let roster: RightsRoster = serde_json::from_value(value.clone())?;
    ensure!(
        roster.schema == "airk.private-reviewed-model-rights/1"
            && roster.model == profile.model
            && roster.assets_sha256 == expected.assets_sha256,
        "foreign rights model/assets"
    );
    ensure!(
        roster.assets.len() == profile.assets.len() && !roster.license_and_notice_files.is_empty(),
        "incomplete rights/notice closure"
    );
    let mut notices = BTreeSet::new();
    for file in &roster.license_and_notice_files {
        ensure!(notices.insert(file.sha256), "duplicate rights/notice pin");
        pinned(file)?;
    }
    validate_rights_assets(&profile.assets, &roster.assets, &notices)
}
fn validate_rights_assets(
    assets: &[Asset],
    rows: &[RightsAsset],
    notices: &BTreeSet<Sha256Digest>,
) -> Result<()> {
    ensure!(assets.len() == rows.len(), "incomplete rights asset roster");
    let mut names = BTreeSet::new();
    for row in rows {
        let actual = assets
            .iter()
            .find(|asset| asset.name == row.asset.name)
            .context("extraneous rights asset")?;
        ensure!(
            names.insert(&row.asset.name)
                && serde_json::to_value(actual)? == serde_json::to_value(&row.asset)?,
            "rights logical asset differs"
        );
        ensure!(
            !row.provenance.trim().is_empty()
                && row.provenance.len() <= 4096
                && !row.license_spdx.trim().is_empty()
                && row.license_spdx.len() <= 256
                && !row.license_basis.is_empty()
                && row.license_basis.iter().all(|pin| notices.contains(pin)),
            "actual provenance/SPDX/license basis missing"
        );
    }
    Ok(())
}

fn verify_proof(
    profile: &Profile,
    candidate: &Candidate,
    decision: &RecordedDecision,
    require_current_policy: bool,
) -> Result<(FinalProof, Sha256Digest)> {
    ensure!(
        decision
            .approved_final_proofs
            .contains(&candidate.final_proof.sha256),
        "final proof is not in the recorded promotion decision"
    );
    let proof: FinalProof = serde_json::from_slice(&pinned(&candidate.final_proof)?)?;
    ensure!(
        matches!(
            proof.schema.as_str(),
            "airk.private-final-stage-qualification/1" | "airk.private-final-stage-qualification/2"
        ),
        "unknown/nonfinal qualification proof"
    );
    let resource_policy = software_v2::policy(profile, &proof, decision)?;
    ensure!(
        !require_current_policy || resource_policy.is_some(),
        "current final-plan catalog requires the approved software-v2 resource policy for every profile"
    );
    let extended_scope = terminal::validate(profile, candidate, decision, &proof)?;
    ensure!(
        proof.schema
            == if extended_scope || resource_policy.is_some() {
                "airk.private-final-stage-qualification/2"
            } else {
                "airk.private-final-stage-qualification/1"
            },
        "unknown/nonfinal qualification proof or class scope"
    );
    if proof.schema == "airk.private-final-stage-qualification/2" {
        ensure!(
            resource_policy.is_some(),
            "final proof/2 requires explicit software-v2 resource policy"
        );
        ensure!(
            proof.disk_budget_compatibility.is_none()
                && proof.calibration_component_equivalence.is_none(),
            "software-v2 primitives cannot inherit historical primitive equivalence"
        );
    }
    let mut expected = binding(profile, candidate.profile.sha256, &proof.binding.target)?;
    if extended_scope {
        expected.task_scope = "context_action_class_rules/2".into();
        ensure!(
            proof.calibration_component_equivalence.is_none(),
            "generic-only calibration equivalence cannot transfer /2 terminal scope"
        );
    }
    ensure!(
        proof.binding == expected,
        "model/assets/compiler/calibration/runtime/tier/platform/task scope mismatch"
    );
    let component_targets = build_target::validate(
        &proof.component_target_provenance,
        decision,
        &proof.binding.target,
    )?;
    let mut original_profile = None;
    let mut original_binding = if let Some(pin) = &proof.disk_budget_compatibility {
        ensure!(
            decision.approved_disk_compatibilities.contains(&pin.sha256),
            "disk-only compatibility is not explicitly approved"
        );
        let compatibility: DiskBudgetCompatibility = serde_json::from_slice(&pinned(pin)?)?;
        ensure!(
            compatibility.schema == "airk.private-profile-disk-budget-only/1"
                && compatibility.corrected_profile_sha256 == candidate.profile.sha256,
            "foreign disk-only compatibility record"
        );
        let original: Profile = serde_json::from_slice(&pinned(&compatibility.original_profile)?)?;
        original.validate()?;
        let minimum = profile
            .assets
            .iter()
            .try_fold(4 * 131072 + (16u64 << 20), |sum, asset| {
                sum.checked_add(asset.bytes)
            })
            .context("disk budget overflow")?;
        ensure!(
            profile.peak_disk_bytes >= minimum
                && profile.peak_disk_bytes > original.peak_disk_bytes,
            "corrected disk-only budget does not cover the owning formula"
        );
        let mut normalized = original.clone();
        normalized.peak_disk_bytes = profile.peak_disk_bytes;
        normalized.manifest = profile.manifest;
        ensure!(
            serde_json::to_value(&normalized)? == serde_json::to_value(profile)?,
            "compatibility changes more than declared disk budget and its derived identity"
        );
        let formula: serde_json::Value =
            serde_json::from_slice(&pinned(&compatibility.source_formula)?)?;
        ensure!(
            formula["schema"] == "airk.private-source-disk-accounting/1",
            "owning disk-formula evidence missing"
        );
        let mut original_binding = binding(
            &original,
            compatibility.original_profile.sha256,
            &proof.binding.target,
        )?;
        if extended_scope {
            original_binding.task_scope = expected.task_scope.clone();
        }
        original_profile = Some(original);
        Some(original_binding)
    } else {
        None
    };
    if let Some(pin) = &proof.calibration_component_equivalence {
        ensure!(
            proof.disk_budget_compatibility.is_none(),
            "ambiguous component origin: calibration equivalence must carry its own approved disk correction"
        );
        let (original, binding) = calibration_equivalence::validate(
            profile,
            candidate,
            decision,
            pin,
            &proof.binding.target,
            &component_targets.targets,
        )?;
        original_profile = Some(original);
        original_binding = Some(binding);
    }
    let required = BTreeSet::from([
        Gate::NativeNumerical,
        Gate::NativeResource,
        Gate::ActualComposed,
        Gate::HeldoutA6,
        Gate::RightsClosure,
    ]);
    ensure!(
        proof.gates.keys().copied().collect::<BTreeSet<_>>() == required,
        "missing/extra final acceptance class"
    );
    for (kind, pin) in &proof.gates {
        let gate: GateReceipt = serde_json::from_slice(&pinned(pin)?)?;
        let selected_binding = if matches!(kind, Gate::NativeNumerical | Gate::NativeResource) {
            original_binding
                .as_ref()
                .filter(|original| gate.binding == **original)
                .unwrap_or(&expected)
        } else {
            &expected
        };
        validate_gate(&gate, *kind, selected_binding)?;
        let mut originals = Vec::new();
        for original in &gate.original_receipts {
            let raw = pinned(original)?;
            if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&raw) {
                originals.push((original.sha256, value));
            }
        }
        match kind {
            Gate::NativeNumerical | Gate::NativeResource => {
                if let Some(policy) = &resource_policy {
                    software_v2::validate(&software_v2::Evidence {
                        kind: *kind,
                        profile,
                        candidate,
                        expected: &expected,
                        records: &originals,
                        policy,
                        targets: &component_targets.targets,
                        source_groups: &component_targets.source_groups,
                        build_receipts: &component_targets.build_receipts,
                        decision,
                    })?;
                    continue;
                }
                let owners = originals
                    .iter()
                    .filter(|(_, value)| {
                        value["schema"] == "airk.private-focus-native-execution-receipt/1"
                            || value["schema"] == "airk.private-context-native-execution-receipt/1"
                    })
                    .collect::<Vec<_>>();
                ensure!(
                    owners.len() == 1,
                    "actual primitive owned image selector absent/ambiguous"
                );
                let image: Sha256Digest = serde_json::from_value(
                    owners[0].1["owned"]["suspended_assignment"]["image"]["sha256"].clone(),
                )?;
                ensure!(
                    component_targets.targets.get(&image) == Some(&expected.target),
                    "primitive actual image target lacks reviewed native build evidence"
                );
                if profile.declared_stage() == Some(Stage::Focus) {
                    validate_focus_native(
                        *kind,
                        if selected_binding == &expected {
                            profile
                        } else {
                            original_profile
                                .as_ref()
                                .context("original evidence Profile absent")?
                        },
                        selected_binding,
                        &originals,
                    )?;
                } else {
                    validate_context_native(
                        if selected_binding == &expected {
                            profile
                        } else {
                            original_profile
                                .as_ref()
                                .context("original evidence Profile absent")?
                        },
                        selected_binding,
                        &originals,
                    )?;
                }
            }
            Gate::HeldoutA6 => {
                validate_quality(profile.model, profile.manifest, &expected, &originals)?
            }
            Gate::ActualComposed => composed_validation::validate(
                profile,
                &expected,
                &originals,
                resource_policy.as_ref(),
            )?,
            Gate::RightsClosure => validate_rights(profile, &expected, decision, &originals)?,
        }
    }
    // This projection is Root-reviewed final evidence. Original unqualified
    // reports remain unmodified; a raw numeric/resource/CAL/A6 bool is insufficient.
    Ok((proof, candidate.final_proof.sha256))
}
fn logical_asset(asset: &Asset, path: &Path, download: &AssetDownload) -> Result<()> {
    ensure!(
        download.asset_name == asset.name,
        "source map asset identity differs"
    );
    download.source_count()?; // Core owns origin, part bounds and complete backups.
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.is_file() && !meta.file_type().is_symlink() && meta.len() == asset.bytes,
        "logical source object differs"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            meta.file_attributes() & 0x400 == 0,
            "logical source reparse refused"
        );
    }
    let mut file = fs::File::open(path)?;
    let mut full = Sha256::new();
    let mut consumed = 0_u64;
    let mut buffer = vec![0_u8; 4 << 20];
    for part in &download.parts {
        let mut remaining = part.bytes;
        let mut digest = Sha256::new();
        while remaining > 0 {
            let wanted = usize::try_from(remaining.min(buffer.len() as u64))?;
            let count = file.read(&mut buffer[..wanted])?;
            ensure!(count > 0, "logical source shortened");
            digest.update(&buffer[..count]);
            full.update(&buffer[..count]);
            consumed = consumed
                .checked_add(count as u64)
                .context("source byte overflow")?;
            remaining -= count as u64;
        }
        ensure!(
            Sha256Digest::from_bytes(digest.finalize().into()) == part.sha256,
            "approved ordered local part digest differs"
        );
    }
    let mut extra = [0_u8; 1];
    ensure!(
        consumed == asset.bytes
            && file.read(&mut extra)? == 0
            && file.metadata()?.len() == asset.bytes
            && Sha256Digest::from_bytes(full.finalize().into()) == asset.sha256,
        "complete logical asset source changed"
    );
    Ok(())
}
fn verified_download_sources(
    profile: &Profile,
    candidate: &Candidate,
    decision: &RecordedDecision,
) -> Result<Vec<AssetDownload>> {
    ensure!(
        decision
            .approved_download_source_maps
            .contains(&candidate.download_sources.sha256),
        "download source map absent from recorded approval"
    );
    ensure!(
        candidate.files.keys().collect::<BTreeSet<_>>()
            == profile
                .assets
                .iter()
                .map(|a| &a.name)
                .collect::<BTreeSet<_>>(),
        "local source map census differs"
    );
    let downloads: Vec<AssetDownload> =
        serde_json::from_slice(&pinned(&candidate.download_sources)?)?;
    ensure!(
        downloads.len() == profile.assets.len()
            && downloads
                .iter()
                .map(|d| &d.asset_name)
                .collect::<BTreeSet<_>>()
                == profile
                    .assets
                    .iter()
                    .map(|a| &a.name)
                    .collect::<BTreeSet<_>>(),
        "approved source map has missing/duplicate/foreign logical assets"
    );
    let mut proposed = profile.clone();
    proposed.downloads = downloads.clone();
    proposed.validate_release_downloads()?;
    for asset in &profile.assets {
        let download = downloads
            .iter()
            .find(|d| d.asset_name == asset.name)
            .context("approved asset source missing")?;
        logical_asset(
            asset,
            candidate
                .files
                .get(&asset.name)
                .context("local logical asset source missing")?,
            download,
        )?;
    }
    Ok(downloads)
}
fn context_calibration(
    profile: &Profile,
    files: &BTreeMap<String, PathBuf>,
) -> Result<Option<airk_core::arbiter::context_action::calibration::JointCalibration>> {
    use airk_core::arbiter::context_action::calibration::JointCalibration;
    if profile.declared_stage() != Some(Stage::Context) {
        return Ok(None);
    }
    let asset = profile
        .assets
        .iter()
        .find(|asset| asset.role == AssetRole::Calibration)
        .context("calibration asset absent")?;
    let raw = pinned(&PinnedFile {
        path: files
            .get(&asset.name)
            .context("calibration source absent")?
            .clone(),
        bytes: asset.bytes,
        sha256: asset.sha256,
    })?;
    let policy = *profile
        .task_policies
        .get(&Task::ContextAction)
        .context("Context task policy absent")?;
    Ok(Some(JointCalibration::parse_and_validate(
        &raw, profile, policy,
    )?))
}
fn validate_task_scope(profile: &Profile, files: &BTreeMap<String, PathBuf>) -> Result<()> {
    use airk_core::arbiter::context_action::{
        ContextReason, RelevanceHypothesis,
        calibration::{CALIBRATION_SCHEMA_V2, ContextActionClass},
    };
    if let Some(calibration) = context_calibration(profile, files)? {
        ensure!(
            calibration.rules.iter().all(|rule| {
                if rule.action != ContextActionClass::Read {
                    return calibration.schema == CALIBRATION_SCHEMA_V2;
                }
                rule.allowed_gaps == [Gap::Unsupported]
                    && rule.allowed_reasons == [ContextReason::AdditionalEvidenceMayHelp]
                    && rule.allowed_roles == [EvidenceRole::Unknown]
                    && rule.allowed_relevance == [RelevanceHypothesis::Uncertain]
            }),
            "calibration broadens the actually qualified class scope"
        );
    }
    Ok(())
}
/// PRIVATE build preparation only. Runtime authority still requires embedding
/// the reviewed catalog and the existing release/package qualification workflow.
pub fn promote(tag: &str, decision_pin: &PinnedFile, candidates: &[Candidate]) -> Result<Promoted> {
    ensure!(
        tag == concat!("v", env!("CARGO_PKG_VERSION")) && candidates.len() == 3,
        "final native version/exact three profiles required"
    );
    let decision: RecordedDecision = serde_json::from_slice(&pinned(decision_pin)?)?;
    ensure!(
        decision.schema == "airk.private-recorded-stage-promotion/1"
            && decision.decision == "approved_final_qualification",
        "actual recorded final promotion decision required"
    );
    let mut profiles = Vec::new();
    let mut models = BTreeSet::new();
    let mut sources = BTreeMap::new();
    let mut bundles = Vec::new();
    let mut parts = BTreeMap::<String, PartSource>::new();
    for candidate in candidates {
        let mut profile: Profile = serde_json::from_slice(&pinned(&candidate.profile)?)?;
        profile.validate()?;
        airk_retrieval::arbiter::publication::validate_candidate_for_distribution(&profile)?;
        ensure!(
            profile.qualification.is_none()
                && profile.qualified_targets.is_empty()
                && models.insert(profile.model),
            "already-promoted/duplicate candidate"
        );
        ensure!(
            decision
                .approved_download_source_maps
                .contains(&candidate.download_sources.sha256),
            "download source map absent from recorded approval"
        );
        let (proof, proof_sha) = verify_proof(&profile, candidate, &decision, true)?;
        ensure!(
            candidate.files.keys().cloned().collect::<BTreeSet<_>>()
                == profile
                    .assets
                    .iter()
                    .map(|asset| asset.name.clone())
                    .collect(),
            "incomplete/extraneous source map"
        );
        validate_task_scope(&profile, &candidate.files)?;
        profile.downloads = verified_download_sources(&profile, candidate, &decision)?;
        profile.qualification = Some(proof_sha);
        profile.qualified_targets = vec![QualifiedTarget {
            target: proof.binding.target,
            execution_identity: proof.binding.execution_identity,
            receipt: proof.gates[&Gate::ActualComposed].sha256,
        }];
        profile.manifest = Sha256Digest::from_bytes([0; 32]);
        profile.manifest = airk_core::identity::framed_digest(
            b"airk.arbiter.release-profile/1",
            &[&serde_json::to_vec(&profile)?],
        );
        profile.validate_release_downloads()?;
        let stage = profile.declared_stage().context("selected stage missing")?;
        bundles.push(OfflineBundle {
            profile: profile.manifest,
            qualification: proof_sha,
            directory: profile.manifest.to_string(),
            model: profile.model,
            stage,
            qualified_targets: profile.qualified_targets.clone(),
            cpu_fit: profile.cpu_fit.clone().context("CPU fit missing")?,
            memory_reservation_bytes: profile.memory_reservation_bytes,
            files: profile.assets.clone(),
        });
        for download in &profile.downloads {
            let mut offset = 0u64;
            for part in &download.parts {
                for filename in
                    airk_retrieval::arbiter::publication::github_part_filenames(part, tag)?
                {
                    if let Some(old) = parts.get(&filename) {
                        ensure!(
                            old.bytes == part.bytes && old.sha256 == part.sha256,
                            "conflicting native part slice"
                        );
                    } else {
                        parts.insert(
                            filename,
                            PartSource {
                                profile: profile.manifest,
                                asset_name: download.asset_name.clone(),
                                offset,
                                bytes: part.bytes,
                                sha256: part.sha256,
                            },
                        );
                    }
                }
                offset = offset
                    .checked_add(part.bytes)
                    .context("part slice offset overflow")?;
            }
        }
        sources.insert(profile.manifest, candidate.files.clone());
        profiles.push(profile);
    }
    ensure!(
        models
            == BTreeSet::from([
                ModelChoice::FocusQwenReranker06B,
                ModelChoice::ContextQwenCompact,
                ModelChoice::ContextQwenLarge
            ]),
        "selected roster mismatch"
    );
    let catalog = serde_json::to_vec_pretty(
        &serde_json::json!({"schema":CATALOG_SCHEMA,"profiles":profiles}),
    )?;
    let publication =
        airk_retrieval::arbiter::publication::project_distribution_candidate(&catalog, tag)?;
    let plan = serde_json::to_vec_pretty(&OfflinePlan {
        schema: "airk.arbiter.offline-bundle-plan/1".into(),
        tag: tag.into(),
        catalog_sha256: sha(&catalog),
        promotion_decision_sha256: decision_pin.sha256,
        bundles,
    })?;
    let sources_private = serde_json::to_vec_pretty(&Sources {
        schema: "airk.private-offline-bundle-sources/1".into(),
        plan_sha256: sha(&plan),
        publication_sha256: sha(&serde_json::to_vec_pretty(&publication)?),
        parts,
        files: sources,
    })?;
    Ok(Promoted {
        catalog,
        plan,
        sources_private,
        publication,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    #[test]
    #[ignore = "explicit actual Rust Profile files only; emits declared facts, never qualification"]
    fn emit_final_distribution_candidate_facts() -> Result<()> {
        let path = PathBuf::from(
            std::env::var_os("AIRK_FINAL_DISTRIBUTION_FACT_INPUT")
                .context("actual pinned profile file array required")?,
        );
        let digest: Sha256Digest = serde_json::from_value(json!(std::env::var(
            "AIRK_FINAL_DISTRIBUTION_FACT_INPUT_SHA256"
        )?))?;
        let inputs: Vec<PinnedFile> = serde_json::from_slice(&pinned(&PinnedFile {
            bytes: fs::symlink_metadata(&path)?.len(),
            path,
            sha256: digest,
        })?)?;
        ensure!(
            inputs.len() == 3,
            "exact three selected actual profiles required"
        );
        let mut models = BTreeSet::new();
        let mut rows = Vec::new();
        for input in inputs {
            let profile: Profile = serde_json::from_slice(&pinned(&input)?)?;
            airk_retrieval::arbiter::publication::validate_candidate_for_distribution(&profile)?;
            ensure!(models.insert(profile.model), "duplicate model facts");
            rows.push(json!({"profile_sha256":input.sha256,"manifest":profile.manifest,"model":profile.model,"stage":profile.declared_stage(),"assets_sha256":sha(&serde_json::to_vec(&profile.assets)?),"assets":profile.assets,"compiler":profile.compiler,"calibration":profile.calibration,"task_policies":profile.task_policies,"runtime":profile.runtime,"cpu_fit":profile.cpu_fit,"declared_only":true,"qualification_granted":false}));
        }
        ensure!(
            models
                == BTreeSet::from([
                    ModelChoice::FocusQwenReranker06B,
                    ModelChoice::ContextQwenCompact,
                    ModelChoice::ContextQwenLarge
                ]),
            "unselected facts roster"
        );
        let output = PathBuf::from(
            std::env::var_os("AIRK_FINAL_DISTRIBUTION_FACT_OUTPUT")
                .context("fresh native facts output required")?,
        );
        ensure!(output.is_absolute(), "absolute facts output required");
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)?;
        file.write_all(&serde_json::to_vec_pretty(&json!({"schema":"airk.private-final-distribution-candidate-facts/1","input_sha256":digest,"qualification_granted":false,"profiles":rows}))?)?;
        file.sync_all()?;
        Ok(())
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct FixtureProfile {
        profile: PinnedFile,
        base_files: BTreeMap<String, PathBuf>,
        calibration_override: Option<PathBuf>,
    }
    #[test]
    #[ignore = "explicit actual native Profile overlays; emits private fixture filemap only, never qualification"]
    fn emit_final_distribution_fixture_master() -> Result<()> {
        let path = PathBuf::from(
            std::env::var_os("AIRK_FINAL_DISTRIBUTION_FIXTURE_INPUT")
                .context("actual native fixture input required")?,
        );
        let digest: Sha256Digest = serde_json::from_value(json!(std::env::var(
            "AIRK_FINAL_DISTRIBUTION_FIXTURE_INPUT_SHA256"
        )?))?;
        let inputs: Vec<FixtureProfile> = serde_json::from_slice(&pinned(&PinnedFile {
            bytes: fs::symlink_metadata(&path)?.len(),
            path,
            sha256: digest,
        })?)?;
        ensure!(inputs.len() == 3, "three actual native profiles required");
        let mut models = BTreeSet::new();
        let mut rows = Vec::new();
        for input in inputs {
            let profile: Profile = serde_json::from_slice(&pinned(&input.profile)?)?;
            airk_retrieval::arbiter::publication::validate_candidate_for_distribution(&profile)?;
            ensure!(
                models.insert(profile.model),
                "duplicate native fixture profile"
            );
            let mut files = BTreeMap::new();
            for asset in &profile.assets {
                let path = if asset.role == AssetRole::Calibration {
                    input
                        .calibration_override
                        .as_ref()
                        .or_else(|| input.base_files.get(&asset.name))
                } else {
                    input.base_files.get(&asset.name)
                }
                .context("exact actual asset source absent")?;
                let meta = fs::symlink_metadata(path)?;
                ensure!(
                    path.is_absolute()
                        && meta.is_file()
                        && !meta.file_type().is_symlink()
                        && meta.len() == asset.bytes,
                    "actual mapped asset identity/length differs"
                );
                files.insert(asset.name.clone(), path.clone());
            }
            rows.push(json!({"model":profile.model,"profile":input.profile.path,"profile_sha256":input.profile.sha256,"files":files}));
        }
        ensure!(
            models
                == BTreeSet::from([
                    ModelChoice::FocusQwenReranker06B,
                    ModelChoice::ContextQwenCompact,
                    ModelChoice::ContextQwenLarge
                ]),
            "unselected fixture master roster"
        );
        let output = PathBuf::from(
            std::env::var_os("AIRK_FINAL_DISTRIBUTION_FIXTURE_OUTPUT")
                .context("fresh native fixture master output")?,
        );
        ensure!(output.is_absolute(), "absolute native output");
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)?;
        file.write_all(&serde_json::to_vec_pretty(&json!({"schema":"airk.private.selected-native-profile-master/1","profiles":rows,"input_sha256":digest,"qualification_granted":false,"payload_hashes_checked":false,"scope":"Actual Profile bytes preserved unchanged. Existing byte stager and native Store independently authenticate all mapped payload hashes before use."}))?)?;
        file.sync_all()?;
        Ok(())
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct PromotionInput {
        schema: String,
        tag: String,
        decision: PinnedFile,
        candidates: Vec<Candidate>,
    }
    #[test]
    #[ignore = "Root-owned actual final proofs/rights/platform/quality required; no qualification from templates"]
    fn emit_final_selected_distribution_from_recorded_proofs() -> Result<()> {
        let input = PathBuf::from(
            std::env::var_os("AIRK_FINAL_DISTRIBUTION_INPUT")
                .context("explicit final input required")?,
        );
        let digest: Sha256Digest = serde_json::from_value(serde_json::json!(std::env::var(
            "AIRK_FINAL_DISTRIBUTION_INPUT_SHA256"
        )?))?;
        let meta = fs::symlink_metadata(&input)?;
        let spec: PromotionInput = serde_json::from_slice(&pinned(&PinnedFile {
            path: input,
            bytes: meta.len(),
            sha256: digest,
        })?)?;
        ensure!(
            spec.schema == "airk.private-final-distribution-input/1",
            "foreign build input"
        );
        let output = PathBuf::from(
            std::env::var_os("AIRK_FINAL_DISTRIBUTION_OUTPUT")
                .context("fresh private output required")?,
        );
        ensure!(
            output.is_absolute() && !output.exists(),
            "fresh native absolute output required"
        );
        let prepared = promote(&spec.tag, &spec.decision, &spec.candidates)?;
        // Every proof/payload is reconciled before any output. Failure while
        // writing remains incomplete; no completion marker is manufactured.
        fs::create_dir(&output)?;
        use std::io::Write;
        let publication = serde_json::to_vec_pretty(&prepared.publication)?;
        for (name, raw) in [
            ("release-catalog.json", &prepared.catalog),
            ("offline-bundle-plan.json", &prepared.plan),
            ("sources.private.json", &prepared.sources_private),
            ("publication.json", &publication),
        ] {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(output.join(name))?;
            file.write_all(raw)?;
            file.sync_all()?;
        }
        Ok(())
    }

    /// Independent synthetic metadata controls, never actual model evidence.
    fn owner() -> Value {
        json!({"complete":true,"failure":null,"post_input_drift":[],
          "whole_phase_deadline_not_refreshed":true,"whole_phase_expired":false,
          "no_automatic_retry":true,"other_processes_touched":false,
          "owned":{"exit_code":0,"timed_out":false,"log_oversized":false,
            "owned_descendants_settled":true,"owned_job_forced_termination":false,
            "owned_job_final_active_processes":0,"owned_job_final_member_pids":[],
            "parent_assigned_to_job":false,"child_assigned_before_resume":true,
            "owned_total_processes":1,"unexpected_third_process":false,
            "whole_process_deadline_not_refreshed":true,
            "retained_member_exits":[{"exit_observed":true,"exit_code":0}],
            "handle_close_proof":{"forced":false,"settled":true,"final_active":0,"members_before":[],"final_member_pids":[]}}})
    }
    #[test]
    fn final_proof_refusal_cases() -> Result<()> {
        let mut cases = calibration_equivalence::refusal_cases()?;
        cases.extend(build_target::refusal_cases()?);
        cases.extend(terminal::refusal_cases()?);
        cases.extend(software_v2::refusal_cases()?);

        let digest = Sha256Digest::from_bytes([1; 32]);
        let expected = Binding {
            model: ModelChoice::FocusQwenReranker06B,
            candidate_profile_sha256: digest,
            assets_sha256: digest,
            compiler: digest,
            calibration: digest,
            task_policy: digest,
            runtime: RUNTIME.into(),
            execution_identity: "actual-test-cpu-owner".into(),
            target: "x86_64-pc-windows-msvc".into(),
            task_scope: "relevance_ranking_only".into(),
        };
        let gate = GateReceipt {
            schema: "airk.private-final-stage-gate/1".into(),
            gate: Gate::NativeNumerical,
            binding: expected.clone(),
            complete: true,
            passed: true,
            final_for_release: true,
            synthetic_only: false,
            actual_native_execution: true,
            independent_heldout: false,
            licensed_payloads: false,
            original_receipts: vec![PinnedFile {
                path: PathBuf::from("synthetic-unused-original"),
                bytes: 1,
                sha256: digest,
            }],
        };
        assert!(validate_gate(&gate, Gate::NativeNumerical, &expected).is_ok());
        for (pointer, replacement) in [
            ("/schema", json!("foreign")),
            ("/gate", json!("native_resource")),
            ("/binding/model", json!("arbiter-context-qwen-compact")),
            (
                "/binding/candidate_profile_sha256",
                json!(Sha256Digest::from_bytes([2; 32])),
            ),
            ("/complete", json!(false)),
            ("/passed", json!(false)),
            ("/final_for_release", json!(false)),
            ("/synthetic_only", json!(true)),
            ("/actual_native_execution", json!(false)),
            ("/original_receipts", json!([])),
        ] {
            let mut changed = serde_json::to_value(&gate)?;
            *changed.pointer_mut(pointer).context("gate test pointer")? = replacement;
            let refused = serde_json::from_value::<GateReceipt>(changed)
                .map(|changed| validate_gate(&changed, Gate::NativeNumerical, &expected).is_err())
                .unwrap_or(true);
            assert!(refused, "{pointer}");
            cases.push(json!({"id":format!("gate{pointer}"),"accepted":false}));
        }

        let historical: Profile = serde_json::from_slice(include_bytes!(
            "../../../../../fixtures/arbiter/profile-downloads.json"
        ))?;
        let assets = historical.assets;
        let rights = assets
            .iter()
            .map(|asset| RightsAsset {
                asset: asset.clone(),
                provenance: "synthetic metadata source, not actual licensing".into(),
                license_spdx: "Apache-2.0".into(),
                license_basis: vec![digest],
            })
            .collect::<Vec<_>>();
        let notices = BTreeSet::from([digest]);
        assert!(validate_rights_assets(&assets, &rights, &notices).is_ok());
        assert!(validate_rights_assets(&assets, &rights, &BTreeSet::new()).is_err());
        assert!(validate_rights_assets(&assets, &rights[..rights.len() - 1], &notices).is_err());
        for key in ["provenance", "license_spdx", "license_basis", "asset"] {
            let mut changed = serde_json::to_value(&rights[0].asset)?;
            if key == "asset" {
                changed["sha256"] = json!(Sha256Digest::from_bytes([2; 32]));
            }
            let mut rows = assets
                .iter()
                .map(|asset| RightsAsset {
                    asset: asset.clone(),
                    provenance: "synthetic".into(),
                    license_spdx: "Apache-2.0".into(),
                    license_basis: vec![digest],
                })
                .collect::<Vec<_>>();
            match key {
                "provenance" => rows[0].provenance.clear(),
                "license_spdx" => rows[0].license_spdx.clear(),
                "license_basis" => rows[0].license_basis.clear(),
                _ => rows[0].asset = serde_json::from_value(changed)?,
            }
            assert!(
                validate_rights_assets(&assets, &rows, &notices).is_err(),
                "{key}"
            );
            cases.push(json!({"id":format!("rights_{key}_refused"),"accepted":false}));
        }

        let mut report = json!({"conditions":{},"selected_strongest_baseline":"deterministic_packet","paired_common_success":[],"critical_evidence_comparisons":[]});
        let models = [
            serde_json::to_value(ModelChoice::ContextQwenCompact)?,
            serde_json::to_value(ModelChoice::ContextQwenLarge)?,
        ];
        for name in [
            "ordinary_followup",
            "deterministic_packet",
            models[0].as_str().context("Compact")?,
            models[1].as_str().context("Large")?,
        ] {
            report["conditions"][name] = json!({"questions":100,"success_count":80,"completion":0.8,"citation_identity_errors":0,"all_task_mean_external_turns":if name=="ordinary_followup"||name=="deterministic_packet" {1.4} else {1.2},"failures":(80..100).map(|id|json!({"task_id":id.to_string(),"external_model_turns":1})).collect::<Vec<_>>(),"critical_evidence":{"agent_valid_references":100,"agent_total_references":100,"shown_mean_recall_fraction":[1,1],"cited_mean_recall_fraction":[1,1]}});
        }
        for model in &models {
            for baseline in ["ordinary_followup", "deterministic_packet"] {
                report["paired_common_success"].as_array_mut().context("test pairs")?.push(json!({"candidate":model,"baseline":baseline,"common_success_questions":80,"common_success_ids":(0..80).map(|id|id.to_string()).collect::<Vec<_>>(),"candidate_turns":100,"baseline_turns":120}));
            }
            report["critical_evidence_comparisons"].as_array_mut().context("test critical")?.push(json!({"candidate":model,"baseline":"deterministic_packet","citation_comparison_state":"observed","shown_mean_recall_delta":0.0,"cited_mean_recall_delta":0.0,"per_question":(0..100).map(|id|json!({"id":id.to_string(),"shown_recall_delta_fraction":[0,1],"cited_recall_delta_fraction":[0,1]})).collect::<Vec<_>>()}));
        }
        assert!(validate_quality_metrics(&report).is_ok());
        for (pointer, replacement) in [
            (
                format!(
                    "/conditions/{}/success_count",
                    models[0].as_str().context("Compact")?
                ),
                json!(74),
            ),
            (
                "/paired_common_success/1/candidate_turns".into(),
                json!(115),
            ),
            (
                "/critical_evidence_comparisons/0/shown_mean_recall_delta".into(),
                json!(-0.01),
            ),
            (
                format!(
                    "/conditions/{}/critical_evidence/agent_valid_references",
                    models[0].as_str().context("Compact")?
                ),
                json!(99),
            ),
            (
                "/conditions/ordinary_followup/citation_identity_errors".into(),
                json!(1),
            ),
        ] {
            let mut changed = report.clone();
            *changed
                .pointer_mut(&pointer)
                .context("quality test pointer")? = replacement;
            assert!(validate_quality_metrics(&changed).is_err(), "{pointer}");
            cases.push(json!({"id":format!("quality{pointer}"),"accepted":false}));
        }

        for count in [74u64, 77u64] {
            let mut changed = report.clone();
            let model = models[0].as_str().context("Compact")?;
            changed["conditions"][model]["success_count"] = json!(count);
            changed["conditions"][model]["completion"] = json!(count as f64 / 100.0);
            changed["conditions"][model]["failures"] = json!(
                (count..100)
                    .map(|id| json!({"task_id":id.to_string(),"external_model_turns":1}))
                    .collect::<Vec<_>>()
            );
            changed["conditions"][model]["all_task_mean_external_turns"] =
                json!((200 - count) as f64 / 100.0);
            assert!(validate_quality_metrics(&changed).is_err());
            cases.push(json!({"id":format!("quality_actual_count_floor_{count}_refused"),"accepted":false}));
        }
        let mut tie = report.clone();
        tie["selected_strongest_baseline"] = json!("ordinary_followup");
        assert!(validate_quality_metrics(&tie).is_err());
        cases.push(json!({"id":"quality_baseline_tie_rule_refused","accepted":false}));
        let mut wrong_shared = report.clone();
        wrong_shared["paired_common_success"][0]["common_success_ids"][0] = json!("99");
        assert!(validate_quality_metrics(&wrong_shared).is_err());
        cases.push(
            json!({"id":"quality_foreign_common_success_population_refused","accepted":false}),
        );
        let mut contradictory = report.clone();
        contradictory["critical_evidence_comparisons"][0]["per_question"][0]["shown_recall_delta_fraction"] =
            json!([-1, 1]);
        assert!(validate_quality_metrics(&contradictory).is_err());
        cases.push(json!({"id":"quality_original_critical_fraction_contradicts_pass_flags","accepted":false}));
        let control = owner();
        assert!(settled_owner(&control).is_ok());
        let mut absent = control.clone();
        absent
            .as_object_mut()
            .context("owner object")?
            .remove("failure");
        assert!(settled_owner(&absent).is_err());
        cases.push(json!({"id":"missing_required_failure_observation_refused","accepted":false}));
        cases.push(json!({"id":"natural_owner_control","accepted":true}));
        for (pointer, replacement) in [
            ("/complete", json!(false)),
            ("/post_input_drift", json!(["changed"])),
            ("/whole_phase_deadline_not_refreshed", json!(false)),
            ("/owned/owned_job_forced_termination", json!(true)),
            ("/owned/owned_job_final_active_processes", json!(1)),
            ("/owned/owned_total_processes", json!(2)),
            ("/owned/retained_member_exits", json!([])),
            ("/owned/handle_close_proof/settled", json!(false)),
        ] {
            let mut changed = control.clone();
            *changed
                .pointer_mut(pointer)
                .context("independent test pointer")? = replacement;
            assert!(settled_owner(&changed).is_err(), "{pointer}");
            cases.push(json!({"id":pointer,"accepted":false}));
        }
        let resource =
            json!({"samples_us":vec![7;20],"nearest_rank_p95_us":7,"maximum_us":7,"rss":40});
        assert!(measured_twenty(&resource, "rss", 50).is_ok());
        for (key, replacement) in [
            ("samples_us", json!(vec![7; 19])),
            ("samples_us", json!(vec![30_000_001; 20])),
            ("nearest_rank_p95_us", json!(6)),
            ("maximum_us", json!(8)),
            ("rss", json!(51)),
        ] {
            let mut changed = resource.clone();
            changed[key] = replacement;
            assert!(measured_twenty(&changed, "rss", 50).is_err(), "{key}");
            cases.push(json!({"id":format!("resource_{key}_{}",cases.len()),"accepted":false}));
        }
        assert!(finite_array(&json!([0.1]), 2).is_err());
        assert!(bounded_number(&json!(0.0011), 0.001).is_err());
        assert!(bounded_number(&Value::Null, 0.001).is_err());
        let opaque = vec![(Sha256Digest::from_bytes([1; 32]), json!({"passed":true}))];
        assert!(original_schema(&opaque, "airk.private-focus-native-parity/1").is_err());
        cases.push(json!({"id":"opaque_wrapper_flags_refused","accepted":false}));
        let duplicate = vec![
            (
                Sha256Digest::from_bytes([1; 32]),
                json!({"schema":"actual"}),
            ),
            (
                Sha256Digest::from_bytes([2; 32]),
                json!({"schema":"actual"}),
            ),
        ];
        assert!(original_schema(&duplicate, "actual").is_err());
        cases.push(json!({"id":"duplicate_original_schema_refused","accepted":false}));
        if let Some(output) = std::env::var_os("AIRK_WRITE_FINAL_DISTRIBUTION_CASES") {
            fs::write(
                output,
                serde_json::to_vec_pretty(&json!({
                    "schema":"airk.private-final-distribution-refusal-cases/1",
                    "synthetic_metadata_only":true,"model_or_release_qualified":false,"cases":cases
                }))?,
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod source_map_tests;
