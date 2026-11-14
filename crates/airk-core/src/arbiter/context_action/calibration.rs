// SPDX-License-Identifier: Apache-2.0
//! Authenticated joint-card acceptance policy, not probability calibration.
use super::*;

pub const CALIBRATION_SCHEMA: &str = "airk.arbiter.context-action.calibration/1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextActionClass {
    Read,
    Sufficient,
    Clarify,
    NoJustifiedRead,
}
impl ContextActionClass {
    pub fn of(action: ContextAction) -> Self {
        match action {
            ContextAction::Read { .. } => Self::Read,
            ContextAction::Sufficient => Self::Sufficient,
            ContextAction::Clarify => Self::Clarify,
            ContextAction::NoJustifiedRead => Self::NoJustifiedRead,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JointAssetBinding {
    pub role: AssetRole,
    pub sha256: Sha256Digest,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JointRule {
    pub action: ContextActionClass,
    pub allowed_gaps: Vec<Gap>,
    pub allowed_reasons: Vec<ContextReason>,
    pub allowed_roles: Vec<EvidenceRole>,
    pub allowed_relevance: Vec<RelevanceHypothesis>,
    pub minimum_raw_margin: f32,
}
impl JointRule {
    pub fn supports(&self, card: &ContextActionCard) -> bool {
        self.action == ContextActionClass::of(card.action)
            && self.allowed_gaps.contains(&card.gap)
            && self.allowed_reasons.contains(&card.reason)
            && card.assessments.iter().all(|unit| {
                self.allowed_roles.contains(&unit.role)
                    && self.allowed_relevance.contains(&unit.relevance)
            })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JointCalibration {
    pub schema: String,
    pub model: ModelChoice,
    pub compiler: Sha256Digest,
    pub task_policy: Sha256Digest,
    pub score_abi: ScoreAbi,
    #[serde(deserialize_with = "unique_assets")]
    pub assets: BTreeMap<String, JointAssetBinding>,
    pub rules: Vec<JointRule>,
}
impl JointCalibration {
    /// The caller authenticates these exact raw bytes against the selected
    /// profile's Calibration asset SHA before invoking this parser.
    pub fn parse_and_validate(
        raw: &[u8],
        profile: &Profile,
        task_policy: Sha256Digest,
    ) -> Result<Self> {
        ensure!(raw.len() <= 65_536, "joint calibration byte limit");
        let value: Self = serde_json::from_slice(raw)?;
        value.validate(profile, task_policy)?;
        Ok(value)
    }
    pub fn validate(&self, profile: &Profile, task_policy: Sha256Digest) -> Result<()> {
        ensure!(
            self.schema == CALIBRATION_SCHEMA
                && self.model == profile.model
                && self.compiler == profile.compiler
                && self.task_policy == task_policy
                && profile.tasks.contains(&Task::ContextAction)
                && profile.task_policies.get(&Task::ContextAction) == Some(&task_policy)
                && self.score_abi == profile.abi
                && !self.rules.is_empty()
                && self.rules.len() <= 32,
            "joint calibration profile/task/ABI binding mismatch"
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
                                sha256: asset.sha256
                            }
                        )
                        .is_none(),
                    "duplicate joint profile asset name"
                );
            }
        }
        ensure!(
            expected
                .values()
                .filter(|asset| asset.role == AssetRole::Model)
                .count()
                == 1
                && expected
                    .values()
                    .filter(|asset| asset.role == AssetRole::Tokenizer)
                    .count()
                    == 1
                && self.assets == expected,
            "joint calibration complete trained asset map mismatch"
        );
        for rule in &self.rules {
            ensure!(
                rule.minimum_raw_margin.is_finite()
                    && rule.minimum_raw_margin >= 0.0
                    && !rule.allowed_gaps.is_empty()
                    && !rule.allowed_reasons.is_empty()
                    && rule.allowed_gaps.len() <= 8
                    && rule.allowed_reasons.len() <= 10
                    && rule.allowed_roles.len() <= 7
                    && rule.allowed_relevance.len() <= 4,
                "invalid joint calibration rule"
            );
            unique(&rule.allowed_gaps)?;
            unique(&rule.allowed_reasons)?;
            unique(&rule.allowed_roles)?;
            unique(&rule.allowed_relevance)?;
        }
        Ok(())
    }
    pub fn accepts(&self, card: &ContextActionCard, margin: f32) -> bool {
        margin.is_finite()
            && self
                .rules
                .iter()
                .any(|rule| rule.supports(card) && margin >= rule.minimum_raw_margin)
    }
}
fn unique<T: PartialEq>(items: &[T]) -> Result<()> {
    for (index, item) in items.iter().enumerate() {
        ensure!(
            !items[..index].contains(item),
            "duplicate joint calibration class"
        );
    }
    Ok(())
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
                    return Err(serde::de::Error::custom(
                        "duplicate joint calibration asset",
                    ));
                }
            }
            Ok(assets)
        }
    }
    deserializer.deserialize_map(Assets)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Result<(Profile, JointCalibration)> {
        let mut profile: Profile = serde_json::from_slice(include_bytes!(
            "../../../../../fixtures/arbiter/profile-downloads.json"
        ))?;
        let digest = Sha256Digest::from_bytes([0x11; 32]);
        profile.tasks = BTreeSet::from([Task::ContextAction]);
        profile.task_policies = BTreeMap::from([(Task::ContextAction, digest)]);
        let assets = BTreeMap::from([
            (
                "model.bin".into(),
                JointAssetBinding {
                    role: AssetRole::Model,
                    sha256: digest,
                },
            ),
            (
                "weights.data".into(),
                JointAssetBinding {
                    role: AssetRole::ExternalWeights,
                    sha256: digest,
                },
            ),
            (
                "tokenizer.bin".into(),
                JointAssetBinding {
                    role: AssetRole::Tokenizer,
                    sha256: digest,
                },
            ),
        ]);
        // Synthetic structural policy, never fitted data or a qualified profile.
        let calibration = JointCalibration {
            schema: CALIBRATION_SCHEMA.into(),
            model: ModelChoice::SmallIntern,
            compiler: digest,
            task_policy: digest,
            score_abi: profile.abi.clone(),
            assets,
            rules: vec![JointRule {
                action: ContextActionClass::Sufficient,
                allowed_gaps: vec![Gap::Sufficient],
                allowed_reasons: vec![ContextReason::RetainedEvidenceCoversQuestion],
                allowed_roles: vec![EvidenceRole::Document],
                allowed_relevance: vec![RelevanceHypothesis::Useful],
                minimum_raw_margin: 0.5,
            }],
        };
        Ok((profile, calibration))
    }
    #[test]
    fn calibration_binds_all_weight_tokenizer_names_roles_and_abi() -> Result<()> {
        let (profile, calibration) = fixture()?;
        JointCalibration::parse_and_validate(
            &serde_json::to_vec(&calibration)?,
            &profile,
            calibration.task_policy,
        )?;
        for name in ["model.bin", "weights.data", "tokenizer.bin"] {
            let mut changed = calibration.clone();
            changed.assets.remove(name);
            assert!(changed.validate(&profile, calibration.task_policy).is_err());
            let mut changed = calibration.clone();
            changed.assets.get_mut(name).unwrap().sha256 = Sha256Digest::from_bytes([99; 32]);
            assert!(changed.validate(&profile, calibration.task_policy).is_err());
        }
        let mut changed = calibration.clone();
        changed.score_abi.readout = Readout::PointerHead;
        assert!(changed.validate(&profile, calibration.task_policy).is_err());
        changed = calibration.clone();
        changed.model = ModelChoice::LargeKev;
        assert!(changed.validate(&profile, calibration.task_policy).is_err());
        changed = calibration.clone();
        changed.rules[0].minimum_raw_margin = f32::NAN;
        assert!(changed.validate(&profile, calibration.task_policy).is_err());
        changed.rules[0].minimum_raw_margin = -1.0;
        assert!(changed.validate(&profile, calibration.task_policy).is_err());
        assert!(
            calibration
                .validate(&profile, Sha256Digest::from_bytes([99; 32]))
                .is_err()
        );
        let mut raw = serde_json::to_value(&calibration)?;
        raw["temperature"] = 2.41.into();
        assert!(
            JointCalibration::parse_and_validate(
                &serde_json::to_vec(&raw)?,
                &profile,
                calibration.task_policy
            )
            .is_err()
        );
        let raw = serde_json::to_string(&calibration)?;
        let duplicate = raw.replacen(
            "\"assets\":{",
            &format!(
                "\"assets\":{{\"model.bin\":{},",
                serde_json::to_string(&calibration.assets["model.bin"])?
            ),
            1,
        );
        assert!(
            JointCalibration::parse_and_validate(
                duplicate.as_bytes(),
                &profile,
                calibration.task_policy
            )
            .is_err()
        );
        Ok(())
    }
    #[test]
    fn whole_card_qualification_never_falls_back_to_action_only() -> Result<()> {
        let (_, calibration) = fixture()?;
        let fixture: serde_json::Value = serde_json::from_slice(include_bytes!(
            "../../../../../fixtures/split/arbiter/context_action_cases.json"
        ))?;
        let response: ContextResponseV2 =
            serde_json::from_value(fixture["cases"][0]["input"].clone())?;
        let AssessmentResult::Selected { mut card, .. } =
            response.assessment_history[0].result.clone()
        else {
            anyhow::bail!("fixture changed")
        };
        card.assessments[0].role = EvidenceRole::Document;
        assert!(calibration.accepts(&card, 0.5));
        assert!(!calibration.accepts(&card, 0.49));
        card.assessments[0].relevance = RelevanceHypothesis::Redundant;
        assert!(!calibration.accepts(&card, 100.0));
        Ok(())
    }
}
