// SPDX-License-Identifier: Apache-2.0
//! Reviewed terminal evidence is additional to every existing final-stage gate.
use super::*;
use airk_core::arbiter::context_action::calibration::{
    CALIBRATION_SCHEMA_V2, ClassSupport, ContextActionClass,
    terminal::{
        IndependenceAnalysis, TerminalAnnotations, TerminalQualification, TerminalRawCapture,
        TerminalRawRow,
    },
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalProofSources {
    pub qualification: PinnedFile,
    pub independence_analysis: PinnedFile,
    pub criteria_freeze: PinnedFile,
    pub human_annotations: Vec<PinnedFile>,
    pub native_captures: Vec<PinnedFile>,
    pub native_execution_receipts: Vec<PinnedFile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeExecutionClosure {
    schema: String,
    model: ModelChoice,
    compiler: Sha256Digest,
    task_policy: Sha256Digest,
    assets: BTreeMap<String, airk_core::arbiter::context_action::calibration::JointAssetBinding>,
    execution_identity: String,
    raw_rows_sha256: Sha256Digest,
    actual_native_execution: bool,
    complete: bool,
    settled: bool,
    original_receipts: Vec<PinnedFile>,
}
fn approved(pin: &PinnedFile, approvals: &BTreeSet<Sha256Digest>, what: &str) -> Result<Vec<u8>> {
    ensure!(
        approvals.contains(&pin.sha256),
        "terminal original {what} lacks exact recorded approval"
    );
    pinned(pin)
}
fn exact_roster(
    expected: &BTreeSet<Sha256Digest>,
    provided: &[PinnedFile],
    what: &str,
) -> Result<()> {
    let actual: BTreeSet<_> = provided.iter().map(|pin| pin.sha256).collect();
    ensure!(
        actual.len() == provided.len() && &actual == expected,
        "missing/duplicate/extra terminal {what}"
    );
    Ok(())
}

pub(super) fn validate(
    profile: &Profile,
    candidate: &Candidate,
    decision: &RecordedDecision,
    proof: &FinalProof,
) -> Result<bool> {
    let Some(calibration) = context_calibration(profile, &candidate.files)? else {
        ensure!(
            proof.terminal_qualifications.is_empty(),
            "Focus cannot inherit terminal qualification"
        );
        return Ok(false);
    };
    if calibration.schema != CALIBRATION_SCHEMA_V2 {
        ensure!(
            proof.terminal_qualifications.is_empty(),
            "historical generic proof cannot acquire terminal scope"
        );
        return Ok(false);
    }
    calibration.validate_class_support()?;
    let classes = calibration
        .class_support
        .as_ref()
        .context("/2 class dispositions absent")?;
    let expected: BTreeSet<_> = classes
        .iter()
        .filter_map(|class| match class.support {
            ClassSupport::Supported {
                qualification: Some(pin),
            } if class.action != ContextActionClass::Read => Some(pin),
            _ => None,
        })
        .collect();
    let pins: Vec<_> = proof
        .terminal_qualifications
        .iter()
        .map(|source| source.qualification.clone())
        .collect();
    exact_roster(&expected, &pins, "class qualification")?;
    let mut actions = Vec::new();
    for source in &proof.terminal_qualifications {
        let raw = approved(
            &source.qualification,
            &decision.approved_terminal_qualifications,
            "qualification",
        )?;
        let qualification: TerminalQualification = serde_json::from_slice(&raw)?;
        ensure!(
            !actions.contains(&qualification.rule.action),
            "duplicate terminal qualification class"
        );
        actions.push(qualification.rule.action);
        ensure!(classes.iter().any(|class| class.action == qualification.rule.action
            && matches!(class.support, ClassSupport::Supported { qualification: Some(pin) } if pin == source.qualification.sha256)), "terminal proof is not the supported exact class");
        ensure!(
            source.independence_analysis.sha256 == qualification.independence_analysis
                && source.criteria_freeze.sha256 == qualification.criteria.freeze,
            "terminal criteria/independence original identity mismatch"
        );
        let analysis: IndependenceAnalysis = serde_json::from_slice(&approved(
            &source.independence_analysis,
            &decision.approved_terminal_independence_analyses,
            "repository-cluster analysis",
        )?)?;
        let freeze = approved(
            &source.criteria_freeze,
            &decision.approved_terminal_criteria_freezes,
            "pre-score criteria",
        )?;
        qualification.validate_criteria_freeze(&freeze)?;
        let expected_annotations = qualification
            .calibration
            .iter()
            .chain(&qualification.confirmation)
            .map(|row| row.human_annotations)
            .collect();
        exact_roster(
            &expected_annotations,
            &source.human_annotations,
            "human annotation source",
        )?;
        let mut annotations = BTreeMap::new();
        for pin in &source.human_annotations {
            let value: TerminalAnnotations = serde_json::from_slice(&approved(
                pin,
                &decision.approved_terminal_human_annotations,
                "human annotation",
            )?)?;
            annotations.insert(pin.sha256, value);
        }
        let expected_captures = qualification
            .calibration
            .iter()
            .chain(&qualification.confirmation)
            .map(|row| row.native_capture)
            .collect();
        exact_roster(
            &expected_captures,
            &source.native_captures,
            "native capture",
        )?;
        let mut captures = BTreeMap::new();
        for pin in &source.native_captures {
            let value: TerminalRawCapture = serde_json::from_slice(&approved(
                pin,
                &decision.approved_terminal_native_captures,
                "native raw capture",
            )?)?;
            captures.insert(pin.sha256, value);
        }
        let expected_closures = captures
            .values()
            .map(|capture| capture.native_execution_receipt)
            .collect();
        exact_roster(
            &expected_closures,
            &source.native_execution_receipts,
            "actual native execution closure",
        )?;
        for pin in &source.native_execution_receipts {
            let closure: NativeExecutionClosure = serde_json::from_slice(&approved(
                pin,
                &decision.approved_terminal_native_execution_receipts,
                "native execution closure",
            )?)?;
            ensure!(
                closure.schema == "airk.arbiter.context-terminal-native-execution/1"
                    && closure.model == qualification.model
                    && closure.compiler == qualification.compiler
                    && closure.task_policy == qualification.task_policy
                    && closure.assets == qualification.assets
                    && closure.actual_native_execution
                    && closure.complete
                    && closure.settled
                    && !closure.original_receipts.is_empty()
                    && closure.original_receipts.len() <= 64,
                "terminal capture lacks actual complete settled native execution"
            );
            for original in &closure.original_receipts {
                pinned(original)?;
            }
            for capture in captures
                .values()
                .filter(|capture| capture.native_execution_receipt == pin.sha256)
            {
                ensure!(
                    capture.execution_identity == closure.execution_identity
                        && sha(&serde_json::to_vec(&capture.rows)?) == closure.raw_rows_sha256,
                    "terminal native closure does not bind original raw rows/execution identity"
                );
            }
        }
        validate_compiled_inputs(profile, candidate, &qualification, &captures)?;
        qualification.validate_sources(&annotations, &captures)?;
        qualification.validate(&calibration, &analysis)?;
    }
    Ok(true)
}

fn compiler_asset(
    profile: &Profile,
    candidate: &Candidate,
    role: AssetRole,
    maximum: u64,
) -> Result<Vec<u8>> {
    let asset = profile
        .assets
        .iter()
        .find(|asset| asset.role == role)
        .context("terminal compiler/tokenizer asset absent")?;
    ensure!(
        asset.bytes > 0 && asset.bytes <= maximum,
        "terminal compiler/tokenizer byte bound"
    );
    let path = candidate
        .files
        .get(&asset.name)
        .context("terminal compiler/tokenizer file absent")?;
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        path.is_absolute()
            && meta.is_file()
            && !meta.file_type().is_symlink()
            && meta.len() == asset.bytes,
        "terminal compiler/tokenizer object differs"
    );
    let mut raw = Vec::new();
    fs::File::open(path)?
        .take(asset.bytes + 1)
        .read_to_end(&mut raw)?;
    ensure!(
        raw.len() as u64 == asset.bytes && sha(&raw) == asset.sha256,
        "terminal compiler/tokenizer bytes differ"
    );
    Ok(raw)
}
fn compiled_binding(
    raw: &TerminalRawRow,
    actual: &airk_retrieval::arbiter::compiler::CompiledInput,
) -> Result<()> {
    ensure!(
        raw.compiled_input == Some(actual.input_digest)
            && raw.input_tokens == Some(actual.input_ids.len()),
        "terminal original compiled digest/token count differs from actual native compiler"
    );
    Ok(())
}
fn validate_compiled_inputs(
    profile: &Profile,
    candidate: &Candidate,
    qualification: &TerminalQualification,
    captures: &BTreeMap<Sha256Digest, TerminalRawCapture>,
) -> Result<()> {
    use airk_retrieval::arbiter::{compiler::Compiler, context_action};
    let compiler = Compiler::from_verified(
        &compiler_asset(profile, candidate, AssetRole::Compiler, 64 << 10)?,
        &compiler_asset(profile, candidate, AssetRole::Tokenizer, 64 << 20)?,
    )?;
    for row in qualification
        .calibration
        .iter()
        .chain(&qualification.confirmation)
    {
        let prompt = context_action::prompt_from_parts(
            &row.question,
            &row.records,
            &row.choices,
            &row.roster,
        )?;
        let raw = captures
            .get(&row.native_capture)
            .context("terminal native capture absent")?
            .rows
            .iter()
            .find(|raw| raw.id == row.id)
            .context("terminal native row absent")?;
        ensure!(
            raw.compiled_input.is_some() == raw.input_tokens.is_some(),
            "partial terminal compiled binding"
        );
        let compiled = compiler.compile(&prompt.state, &prompt.question, &prompt.candidates);
        if row.scores.is_some() || raw.compiled_input.is_some() {
            compiled_binding(
                raw,
                &compiled.context("scored terminal original input cannot compile")?,
            )?;
        }
        // Unknown/unscored work remains in the denominator. It never supplies
        // an accepted terminal or a fabricated successful compilation witness.
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn refusal_cases() -> Result<Vec<serde_json::Value>> {
    use serde_json::json;
    let pin = PinnedFile {
        path: PathBuf::from("synthetic-never-read"),
        bytes: 1,
        sha256: Sha256Digest::from_bytes([71; 32]),
    };
    let mut cases = Vec::new();
    for source in [
        "qualification",
        "human annotation",
        "native raw capture",
        "native execution closure",
        "repository-cluster analysis",
        "pre-score criteria",
    ] {
        ensure!(
            approved(&pin, &BTreeSet::new(), source).is_err(),
            "unapproved terminal source accepted"
        );
        cases.push(json!({"id":format!("terminal_unapproved_{source}"),"accepted":false}));
    }
    let expected = BTreeSet::from([pin.sha256]);
    for (name, provided) in [
        ("missing", vec![]),
        ("duplicate", vec![pin.clone(), pin.clone()]),
        (
            "foreign",
            vec![PinnedFile {
                sha256: Sha256Digest::from_bytes([72; 32]),
                ..pin.clone()
            }],
        ),
    ] {
        ensure!(
            exact_roster(&expected, &provided, "synthetic source").is_err(),
            "terminal source roster accepted"
        );
        cases.push(json!({"id":format!("terminal_{name}_source_roster"),"accepted":false}));
    }
    exact_roster(&expected, &[pin], "synthetic source")?;
    Ok(cases)
}

#[test]
fn terminal_compiled_binding_refuses_changed_or_missing_native_input() -> Result<()> {
    use airk_core::arbiter::context_action::calibration::terminal::TerminalRawRow;
    let digest = Sha256Digest::from_bytes([81; 32]);
    let actual = airk_retrieval::arbiter::compiler::CompiledInput {
        prompt: "synthetic".into(),
        input_ids: vec![1, 2],
        attention_mask: vec![1, 1],
        position_ids: vec![0, 1],
        decision_position: 1,
        candidate_positions: vec![7, 8],
        option_mask: vec![true, true],
        input_digest: digest,
    };
    let row = TerminalRawRow {
        id: "synthetic".into(),
        visible_input: Sha256Digest::from_bytes([82; 32]),
        compiled_input: Some(digest),
        input_tokens: Some(2),
        scores: Some(vec![1.0, 0.0]),
    };
    compiled_binding(&row, &actual)?;
    let mut changed = row.clone();
    changed.compiled_input = Some(Sha256Digest::from_bytes([83; 32]));
    assert!(compiled_binding(&changed, &actual).is_err());
    let mut changed = row.clone();
    changed.input_tokens = Some(1);
    assert!(compiled_binding(&changed, &actual).is_err());
    let mut changed = row;
    changed.compiled_input = None;
    assert!(compiled_binding(&changed, &actual).is_err());
    Ok(())
}
