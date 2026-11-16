// SPDX-License-Identifier: Apache-2.0
//! Explicit native terminal fitter. No scoring, invented gold or release authority.
use super::*;
use airk_core::arbiter::context_action::calibration::{
    CALIBRATION_SCHEMA_V2, ClassDisposition, ClassSupport,
    terminal::{
        IndependenceAnalysis, TerminalAnnotations, TerminalQualification, TerminalRawCapture,
        fit_terminal_rule,
    },
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    file: PathBuf,
    sha256: Sha256Digest,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClassInput {
    template: Source,
    criteria_freeze: Source,
    independence_analysis: Source,
    human_annotations: Vec<Source>,
    native_captures: Vec<Source>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalInput {
    profile: Source,
    calibration: Source,
    classes: Vec<ClassInput>,
}
fn read(source: &Source) -> Result<Vec<u8>> {
    use std::io::Read;
    let meta = std::fs::symlink_metadata(&source.file)?;
    ensure!(
        source.file.is_absolute()
            && meta.is_file()
            && !meta.file_type().is_symlink()
            && meta.len() <= 32 << 20,
        "bounded regular terminal source required"
    );
    let mut raw = Vec::new();
    std::fs::File::open(&source.file)?
        .take((32 << 20) + 1)
        .read_to_end(&mut raw)?;
    ensure!(
        raw.len() as u64 == meta.len() && sha(&raw) == source.sha256,
        "terminal source changed"
    );
    Ok(raw)
}
#[test]
#[ignore = "explicit exact human-backed terminal CAL/confirmation sources and fresh output required; never qualification"]
fn fit_selected_context_terminal_calibration() -> Result<()> {
    let path = PathBuf::from(
        std::env::var_os("AIRK_CONTEXT_TERMINAL_FIT_INPUT").context("terminal input missing")?,
    );
    let input_sha: Sha256Digest =
        std::env::var("AIRK_CONTEXT_TERMINAL_FIT_INPUT_SHA256")?.parse()?;
    let input: TerminalInput = serde_json::from_slice(&read(&Source {
        file: path,
        sha256: input_sha,
    })?)?;
    let mut profile: Profile = serde_json::from_slice(&read(&input.profile)?)?;
    profile.validate()?;
    let policy = context_action::policy_digest()?;
    ensure!(
        matches!(
            profile.model,
            ModelChoice::ContextQwenCompact | ModelChoice::ContextQwenLarge
        ) && profile.qualification.is_none()
            && profile.qualified_targets.is_empty()
            && profile.downloads.is_empty()
            && profile.task_policies.get(&Task::ContextAction) == Some(&policy)
            && profile.policy == airk_retrieval::arbiter::compiler::qwen::policy_digest()?
            && input.classes.len() <= 3,
        "current selected unqualified Context candidate required"
    );
    let raw = read(&input.calibration)?;
    ensure!(
        profile.calibration == input.calibration.sha256,
        "starting calibration is not the profile asset"
    );
    let mut calibration = JointCalibration::parse_and_validate(&raw, &profile, policy)?;
    ensure!(
        calibration
            .rules
            .iter()
            .all(|rule| rule.action == ContextActionClass::Read),
        "terminal fit starts from independently retained Read-only controls"
    );
    calibration.schema = CALIBRATION_SCHEMA_V2.into();
    calibration.class_support = Some(vec![
        ClassDisposition {
            action: ContextActionClass::Read,
            support: ClassSupport::Supported {
                qualification: None,
            },
        },
        ClassDisposition {
            action: ContextActionClass::Sufficient,
            support: ClassSupport::Unsupported {
                reason: "No suitable human-backed terminal evidence supplied".into(),
            },
        },
        ClassDisposition {
            action: ContextActionClass::Clarify,
            support: ClassSupport::Unsupported {
                reason: "No suitable human-backed terminal evidence supplied".into(),
            },
        },
        ClassDisposition {
            action: ContextActionClass::NoJustifiedRead,
            support: ClassSupport::Unsupported {
                reason: "No suitable human-backed terminal evidence supplied".into(),
            },
        },
    ]);
    let mut seen = Vec::new();
    let mut staged = Vec::new();
    let mut reports = Vec::new();
    for class in &input.classes {
        let mut qualification: TerminalQualification =
            serde_json::from_slice(&read(&class.template)?)?;
        ensure!(
            qualification.rule.action != ContextActionClass::Read
                && !seen.contains(&qualification.rule.action),
            "duplicate/nonterminal input class"
        );
        seen.push(qualification.rule.action);
        ensure!(
            qualification.criteria.freeze == class.criteria_freeze.sha256
                && qualification.independence_analysis == class.independence_analysis.sha256,
            "terminal freeze/analysis source differs"
        );
        qualification.validate_criteria_freeze(&read(&class.criteria_freeze)?)?;
        let analysis: IndependenceAnalysis =
            serde_json::from_slice(&read(&class.independence_analysis)?)?;
        let mut annotations = BTreeMap::new();
        let mut captures = BTreeMap::new();
        for source in &class.human_annotations {
            ensure!(
                annotations
                    .insert(
                        source.sha256,
                        serde_json::from_slice::<TerminalAnnotations>(&read(source)?)?
                    )
                    .is_none(),
                "duplicate human source"
            );
        }
        for source in &class.native_captures {
            ensure!(
                captures
                    .insert(
                        source.sha256,
                        serde_json::from_slice::<TerminalRawCapture>(&read(source)?)?
                    )
                    .is_none(),
                "duplicate native source"
            );
        }
        qualification.validate_sources(&annotations, &captures)?;
        let Some(rule) =
            fit_terminal_rule(&qualification.rule, policy, &qualification.calibration)?
        else {
            reports.push(json!({"action":qualification.rule.action,"status":"unsupported","reason":"No positive CAL threshold with zero false or UNKNOWN admissions"}));
            continue;
        };
        qualification.rule = rule;
        let bytes = serde_json::to_vec_pretty(&qualification)?;
        let proof_sha = sha(&bytes);
        let mut prospective = calibration.clone();
        prospective.rules.push(qualification.rule.clone());
        prospective
            .class_support
            .as_mut()
            .context("class dispositions")?
            .iter_mut()
            .find(|value| value.action == qualification.rule.action)
            .context("terminal disposition")?
            .support = ClassSupport::Supported {
            qualification: Some(proof_sha),
        };
        match qualification.validate(&prospective,&analysis) {
            Ok(summary)=>{
                calibration=prospective;
                reports.push(json!({"action":qualification.rule.action,"status":"experimental_confirmation_accepted","qualification_sha256":proof_sha,"summary":summary,"release_qualified":false}));
                staged.push((format!("terminal-{}.json",serde_json::to_value(qualification.rule.action)?.as_str().context("class name")?),bytes));
            },
            Err(error)=>reports.push(json!({"action":qualification.rule.action,"status":"unsupported","reason":error.to_string(),"release_qualified":false})),
        }
    }
    let calibration_bytes = serde_json::to_vec_pretty(&calibration)?;
    let calibration_sha = sha(&calibration_bytes);
    let asset = profile
        .assets
        .iter_mut()
        .find(|asset| asset.role == AssetRole::Calibration)
        .context("original calibration asset")?;
    asset.name = "context-action-calibration.json".into();
    asset.bytes = calibration_bytes.len() as u64;
    asset.sha256 = calibration_sha;
    profile.calibration = calibration_sha;
    profile.profile_id = format!(
        "private-selected-terminal-candidate-{}",
        serde_json::to_value(profile.model)?
            .as_str()
            .context("model name")?
    );
    profile.manifest = Sha256Digest::from_bytes([0; 32]);
    profile.manifest = framed_digest(
        b"airk.private.context-action-terminal-evaluation-profile/1",
        &[
            input.profile.sha256.as_bytes(),
            &serde_json::to_vec(&profile)?,
        ],
    );
    profile.validate()?;
    JointCalibration::parse_and_validate(&calibration_bytes, &profile, policy)?;
    let output = PathBuf::from(
        std::env::var_os("AIRK_CONTEXT_TERMINAL_FIT_OUTPUT")
            .context("terminal fit output missing")?,
    );
    ensure!(
        output.is_absolute() && !output.exists(),
        "fresh absolute terminal output required"
    );
    std::fs::create_dir(&output)?;
    for (name, bytes) in staged {
        std::fs::write(output.join(name), bytes)?;
    }
    std::fs::write(
        output.join("context-action-calibration.json"),
        &calibration_bytes,
    )?;
    std::fs::write(
        output.join("experimental-profile.json"),
        serde_json::to_vec_pretty(&profile)?,
    )?;
    std::fs::write(
        output.join("terminal-fit-report.private.json"),
        serde_json::to_vec_pretty(
            &json!({"schema":"airk.arbiter.context-terminal-fit-report/1","complete":true,"input_sha256":input_sha,"profile":profile,"calibration_sha256":calibration_sha,"classes":reports,"task_qualified":false,"release_qualified":false,"public_authority":false,"limits":"Exact original human/native sources still need separate recorded approval, native custody/independence replay and every final gate. This experimental fitter grants no runtime or release authority."}),
        )?,
    )?;
    Ok(())
}
