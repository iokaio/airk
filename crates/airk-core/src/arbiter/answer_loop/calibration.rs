// SPDX-License-Identifier: Apache-2.0
//! Authenticated loop-card acceptance policy, not probability calibration.
//! The first release ships it with zero rules, so every card decision abstains.
use super::*;
use crate::arbiter::context_action::calibration::JointAssetBinding;

pub const LOOP_CALIBRATION_SCHEMA: &str = "airk.arbiter.loop-card.calibration/1";
pub const MAX_LOOP_CALIBRATION_BYTES: usize = 65_536;
pub const LOOP_CALIBRATION_SET_SCHEMA: &str = "airk.arbiter.loop-card.calibration-set/1";
/// Four card tasks, each at most one entry of the per-task byte limit.
pub const MAX_LOOP_CALIBRATION_SET_BYTES: usize = 4 * MAX_LOOP_CALIBRATION_BYTES;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoopRule {
    pub class: CardClass,
    /// Raw top-runner margin the unique global top card must reach.
    pub minimum_raw_margin: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoopCalibration {
    pub schema: String,
    pub model: ModelChoice,
    /// The card task this policy admits decisions for.
    pub task: Task,
    pub compiler: Sha256Digest,
    pub task_policy: Sha256Digest,
    pub score_abi: ScoreAbi,
    #[serde(deserialize_with = "unique_assets")]
    pub assets: BTreeMap<String, JointAssetBinding>,
    pub rules: Vec<LoopRule>,
}
impl LoopCalibration {
    /// The caller authenticates these exact raw bytes against the selected
    /// profile's Calibration asset SHA before invoking this parser.
    pub fn parse_and_validate(raw: &[u8], profile: &Profile) -> Result<Self> {
        ensure!(
            raw.len() <= MAX_LOOP_CALIBRATION_BYTES,
            "loop calibration byte limit"
        );
        let value: Self = serde_json::from_slice(raw)?;
        value.validate(profile)?;
        Ok(value)
    }
    pub fn validate(&self, profile: &Profile) -> Result<()> {
        ensure!(
            self.schema == LOOP_CALIBRATION_SCHEMA
                && is_card_task(self.task)
                && self.model == ModelChoice::ContextQwenCompact
                && self.model == profile.model
                && self.compiler == profile.compiler
                && profile.tasks.contains(&self.task)
                && profile.task_policies.get(&self.task) == Some(&self.task_policy)
                && self.score_abi == profile.abi
                && self.rules.len() <= usize::from(MAX_RULES),
            "loop calibration profile/task/ABI binding mismatch"
        );
        let mut expected = BTreeMap::new();
        for asset in &profile.assets {
            if matches!(
                asset.role,
                AssetRole::Model | AssetRole::ExternalWeights | AssetRole::Tokenizer
            ) {
                ensure!(
                    expected
                        .insert(
                            asset.name.clone(),
                            JointAssetBinding {
                                role: asset.role.clone(),
                                sha256: asset.sha256,
                            },
                        )
                        .is_none(),
                    "duplicate loop calibration profile asset name"
                );
            }
        }
        ensure!(
            self.assets == expected,
            "loop calibration complete trained asset map mismatch"
        );
        let stage = card_stage(self.task);
        for rule in &self.rules {
            ensure!(
                rule.minimum_raw_margin.is_finite()
                    && rule.minimum_raw_margin >= 0.0
                    && Some(rule.class.stage()) == stage,
                "invalid loop calibration rule"
            );
        }
        ensure!(
            unique(self.rules.iter().map(|rule| rule.class)),
            "duplicate loop calibration class rule"
        );
        Ok(())
    }
    /// The rule admitting `class` at `margin`, by index. No rule abstains.
    pub fn admits(&self, class: CardClass, margin: f32) -> Option<u8> {
        self.rules
            .iter()
            .position(|rule| {
                rule.class == class && margin.is_finite() && margin >= rule.minimum_raw_margin
            })
            .and_then(|index| u8::try_from(index).ok())
    }
}

fn card_stage(task: Task) -> Option<LoopStage> {
    [
        LoopStage::Split,
        LoopStage::Plan,
        LoopStage::Coverage,
        LoopStage::FollowUp,
    ]
    .into_iter()
    .find(|stage| card_task(*stage) == Some(task))
}

fn unique_assets<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, JointAssetBinding>, D::Error> {
    struct Assets;
    impl<'de> serde::de::Visitor<'de> for Assets {
        type Value = BTreeMap<String, JointAssetBinding>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a complete map with unique native asset names")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> std::result::Result<Self::Value, M::Error> {
            let mut assets = BTreeMap::new();
            while let Some((name, binding)) = map.next_entry::<String, JointAssetBinding>()? {
                if assets.insert(name, binding).is_some() {
                    return Err(serde::de::Error::custom("duplicate loop calibration asset"));
                }
            }
            Ok(assets)
        }
    }
    deserializer.deserialize_map(Assets)
}

/// One document per Context profile, holding a LoopCalibration per admitted
/// card task. The profile keeps its single Calibration asset: that asset's
/// bytes are this document and its SHA-256 is the profile's `calibration`.
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoopCalibrationSet {
    pub schema: String,
    pub model: ModelChoice,
    pub compiler: Sha256Digest,
    /// In card-task order: Split, Plan, Coverage, FollowUp, each at most once.
    pub entries: Vec<LoopCalibration>,
}
impl LoopCalibrationSet {
    /// SHA-256 of the exact raw document bytes, as every asset is identified.
    pub fn digest(raw: &[u8]) -> Sha256Digest {
        use sha2::{Digest, Sha256};
        Sha256Digest::from_bytes(Sha256::digest(raw).into())
    }
    /// Authenticates the raw bytes against the profile's single Calibration
    /// asset and its `calibration` digest before parsing.
    pub fn parse_and_validate(raw: &[u8], profile: &Profile) -> Result<Self> {
        ensure!(
            raw.len() <= MAX_LOOP_CALIBRATION_SET_BYTES,
            "loop calibration set byte limit"
        );
        let digest = Self::digest(raw);
        let assets: Vec<_> = profile
            .assets
            .iter()
            .filter(|asset| asset.role == AssetRole::Calibration)
            .collect();
        ensure!(
            assets.len() == 1
                && assets[0].sha256 == digest
                && assets[0].bytes == u64::try_from(raw.len())?
                && profile.calibration == digest,
            "loop calibration set differs from the profile's one calibration asset"
        );
        let value: Self = serde_json::from_slice(raw)?;
        value.validate(profile)?;
        Ok(value)
    }
    pub fn validate(&self, profile: &Profile) -> Result<()> {
        ensure!(
            self.schema == LOOP_CALIBRATION_SET_SCHEMA
                && self.model == profile.model
                && self.compiler == profile.compiler,
            "loop calibration set profile binding mismatch"
        );
        let admitted: Vec<Task> = [
            LoopStage::Split,
            LoopStage::Plan,
            LoopStage::Coverage,
            LoopStage::FollowUp,
        ]
        .into_iter()
        .filter_map(card_task)
        .filter(|task| profile.tasks.contains(task))
        .collect();
        let tasks: Vec<Task> = self.entries.iter().map(|entry| entry.task).collect();
        ensure!(
            tasks == admitted,
            "loop calibration set must hold exactly one entry per admitted card task"
        );
        for entry in &self.entries {
            entry.validate(profile)?;
        }
        Ok(())
    }
    /// The shipped state: every entry has zero rules, so every decision abstains.
    pub fn is_unruled(&self) -> bool {
        self.entries.iter().all(|entry| entry.rules.is_empty())
    }
}
