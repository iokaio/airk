// SPDX-License-Identifier: Apache-2.0
//! Context task admission for the Answer loop. A Context profile declares
//! either the one-turn `airk.context_action.v1` task (until Answer v2 is
//! removed) or exactly the four loop card tasks, each under its native
//! retrieval task policy. A loop profile's single Calibration asset is one
//! `LoopCalibrationSet` holding one `LoopCalibration` per card task.
use super::*;
use airk_core::arbiter::{
    answer_loop::{
        LOOP_CALIBRATION_SCHEMA, LOOP_CALIBRATION_SET_SCHEMA, LoopCalibration, LoopCalibrationSet,
        MAX_LOOP_CALIBRATION_SET_BYTES, is_card_task,
    },
    context_action::calibration::JointAssetBinding,
};

/// Whether a profile declares any loop card task.
pub(super) fn declares_card_tasks(profile: &Profile) -> bool {
    profile.tasks.iter().any(|task| is_card_task(*task))
}

/// The exact task policies a Context profile must declare.
pub(super) fn context_task_policies(profile: &Profile) -> Result<BTreeMap<Task, Sha256Digest>> {
    if declares_card_tasks(profile) {
        return Ok(crate::arbiter::loop_cards::task_policies()?
            .into_iter()
            .collect());
    }
    Ok(BTreeMap::from([(
        Task::ContextAction,
        crate::arbiter::context_action::policy_digest()?,
    )]))
}

/// Authenticate and parse a loop profile's calibration set once, at open.
pub(super) fn calibration_set(
    profile: &Profile,
    read: impl FnOnce(u64) -> Result<Vec<u8>>,
) -> Result<Option<Arc<LoopCalibrationSet>>> {
    if !declares_card_tasks(profile) {
        return Ok(None);
    }
    let raw = read(MAX_LOOP_CALIBRATION_SET_BYTES as u64)?;
    Ok(Some(Arc::new(LoopCalibrationSet::parse_and_validate(
        &raw, profile,
    )?)))
}

/// Declare the four loop card tasks on a Context profile and bind it to a
/// zero-rule calibration set, which abstains on every decision so the
/// deterministic order decides until rules are fitted (L7). The profile's
/// single Calibration asset and its `calibration` digest are set to the
/// returned set bytes. The manifest is the emitter's to recompute.
pub fn admit_loop_tasks(profile: &mut Profile) -> Result<Vec<u8>> {
    ensure!(
        profile.model.stage() == Some(Stage::Context),
        "only a Context profile declares loop card tasks"
    );
    let policies = crate::arbiter::loop_cards::task_policies()?;
    profile.tasks = policies.iter().map(|(task, _)| *task).collect();
    profile.task_policies = policies.iter().copied().collect();
    let assets: BTreeMap<String, JointAssetBinding> = profile
        .assets
        .iter()
        .filter(|asset| {
            matches!(
                asset.role,
                AssetRole::Model | AssetRole::ExternalWeights | AssetRole::Tokenizer
            )
        })
        .map(|asset| {
            (
                asset.name.clone(),
                JointAssetBinding {
                    role: asset.role.clone(),
                    sha256: asset.sha256,
                },
            )
        })
        .collect();
    let set = LoopCalibrationSet {
        schema: LOOP_CALIBRATION_SET_SCHEMA.into(),
        model: profile.model,
        compiler: profile.compiler,
        entries: policies
            .iter()
            .map(|(task, task_policy)| LoopCalibration {
                schema: LOOP_CALIBRATION_SCHEMA.into(),
                model: profile.model,
                task: *task,
                compiler: profile.compiler,
                task_policy: *task_policy,
                score_abi: profile.abi.clone(),
                assets: assets.clone(),
                rules: vec![],
            })
            .collect(),
    };
    let raw = serde_json::to_vec(&set)?;
    let digest = LoopCalibrationSet::digest(&raw);
    let calibration: Vec<&mut Asset> = profile
        .assets
        .iter_mut()
        .filter(|asset| asset.role == AssetRole::Calibration)
        .collect();
    ensure!(
        calibration.len() == 1,
        "a loop profile keeps exactly one Calibration asset"
    );
    for asset in calibration {
        asset.sha256 = digest;
        asset.bytes = u64::try_from(raw.len())?;
    }
    profile.calibration = digest;
    LoopCalibrationSet::parse_and_validate(&raw, profile)?;
    Ok(raw)
}

#[cfg(test)]
mod tests;
