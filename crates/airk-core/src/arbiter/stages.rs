// SPDX-License-Identifier: Apache-2.0
//! Independent stage configuration. Pure data never grants model/source authority.
use super::*;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    Focus,
    #[default]
    Context,
}
impl Stage {
    pub fn is_context(&self) -> bool {
        *self == Self::Context
    }
    /// The task an index is enrolled for at this stage, and the only one a
    /// persisted enrollment may name.
    pub fn permits(self, task: Task) -> bool {
        matches!(
            (self, task),
            (Self::Focus, Task::FocusRank) | (Self::Context, Task::ContextAction)
        )
    }
    /// A task a profile of this stage may declare and a status row may
    /// list: the stage's enrollment task, and for Context also the Answer
    /// loop's four card tasks. The card tasks run under the Context
    /// enrollment, so `permits`, enrollment and the persisted configuration
    /// do not name them (change-log entry 31). Which tasks a Context
    /// profile declares together (the one-turn task, or exactly the four
    /// card tasks) is the retrieval admission owner's rule.
    pub fn declares(self, task: Task) -> bool {
        self.permits(task) || (self == Self::Context && answer_loop::is_card_task(task))
    }
}
impl ModelChoice {
    pub fn stage(self) -> Option<Stage> {
        match self {
            Self::FocusQwenReranker06B => Some(Stage::Focus),
            Self::ContextQwenCompact | Self::ContextQwenLarge => Some(Stage::Context),
            Self::SmallIntern | Self::SmallStrands | Self::LargeIntern | Self::LargeKev => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StageConfiguration {
    pub enabled: bool,
    pub selected_profile: Option<Sha256Digest>,
    pub indexes: Vec<Enrollment>,
}
impl StageConfiguration {
    pub fn validate(&self, stage: Stage) -> Result<()> {
        ensure!(
            self.indexes.len() <= MAX_ENROLLMENTS,
            "Arbiter stage enrollment limit"
        );
        let mut seen = BTreeSet::new();
        for enrollment in &self.indexes {
            ensure!(
                !enrollment.tasks.is_empty()
                    && seen.insert(enrollment.index_id)
                    && enrollment.tasks.iter().all(|task| stage.permits(*task)),
                "empty, duplicate or wrong-stage Arbiter enrollment"
            );
        }
        ensure!(
            !self.enabled || (self.selected_profile.is_some() && !self.indexes.is_empty()),
            "Arbiter stage enablement requires its own profile and named indexes"
        );
        Ok(())
    }
    pub fn eligible(&self, index: IndexId, task: Task) -> bool {
        self.enabled
            && self.selected_profile.is_some()
            && self
                .indexes
                .iter()
                .any(|entry| entry.index_id == index && entry.tasks.contains(&task))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub schema: String,
    pub installation: Sha256Digest,
    pub catalog: Incarnation,
    pub revision: u64,
    pub focus: StageConfiguration,
    pub context: StageConfiguration,
}
impl Configuration {
    pub fn disabled(installation: Sha256Digest, catalog: Incarnation) -> Self {
        Self {
            schema: CONFIGURATION_SCHEMA.into(),
            installation,
            catalog,
            revision: 1,
            focus: StageConfiguration::default(),
            context: StageConfiguration::default(),
        }
    }
    pub fn stage(&self, stage: Stage) -> &StageConfiguration {
        match stage {
            Stage::Focus => &self.focus,
            Stage::Context => &self.context,
        }
    }
    pub fn stage_mut(&mut self, stage: Stage) -> &mut StageConfiguration {
        match stage {
            Stage::Focus => &mut self.focus,
            Stage::Context => &mut self.context,
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == CONFIGURATION_SCHEMA && self.revision > 0,
            "invalid Arbiter configuration identity"
        );
        self.focus.validate(Stage::Focus)?;
        self.context.validate(Stage::Context)?;
        let indexes: BTreeSet<_> = self
            .focus
            .indexes
            .iter()
            .chain(&self.context.indexes)
            .map(|enrollment| enrollment.index_id)
            .collect();
        ensure!(
            indexes.len() <= MAX_ENROLLMENTS,
            "Arbiter total immutable-index enrollment limit"
        );
        Ok(())
    }
    pub fn eligible_for(&self, stage: Stage, index: IndexId, task: Task) -> bool {
        stage.permits(task) && self.stage(stage).eligible(index, task)
    }
    pub fn eligible(&self, index: IndexId, task: Task) -> bool {
        match task {
            Task::FocusRank => self.eligible_for(Stage::Focus, index, task),
            Task::ContextAction => self.eligible_for(Stage::Context, index, task),
            _ => false,
        }
    }
    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut bytes = serde_json::to_vec(self)?;
        bytes.push(b'\n');
        ensure!(
            bytes.len() <= MAX_CONFIGURATION_BYTES,
            "Arbiter configuration byte limit"
        );
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8], installation: Sha256Digest, catalog: Incarnation) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_CONFIGURATION_BYTES,
            "Arbiter configuration byte limit"
        );
        let value: Self = serde_json::from_slice(bytes)?;
        ensure!(
            value.installation == installation && value.catalog == catalog,
            "Arbiter configuration belongs to another installation or catalog"
        );
        ensure!(
            value.canonical_bytes()? == bytes,
            "noncanonical Arbiter configuration"
        );
        Ok(value)
    }
}

impl Profile {
    pub fn declared_stage(&self) -> Option<Stage> {
        self.stage
    }
    pub fn validate_stage(&self, stage: Stage) -> Result<()> {
        ensure!(
            self.stage == Some(stage) && self.model.stage() == Some(stage),
            "Arbiter profile stage/model mismatch"
        );
        ensure!(
            !self.tasks.is_empty() && self.tasks.iter().all(|task| stage.declares(*task)),
            "Arbiter profile has unsupported stage task"
        );
        ensure!(
            self.task_policies.len() == self.tasks.len()
                && self
                    .tasks
                    .iter()
                    .all(|task| self.task_policies.contains_key(task)),
            "current Arbiter stage requires every exact task policy"
        );
        match stage {
            Stage::Focus => self.abi.validate_focus()?,
            Stage::Context => self.abi.validate_context_qwen()?,
        }
        Ok(())
    }
}
impl ScoreAbi {
    pub fn validate_context_qwen(&self) -> Result<()> {
        ensure!(
            self.readout == Readout::DecisionSymbol && self.inputs.len() == 6,
            "Context Qwen requires its six-input decision-symbol ABI"
        );
        let expected = [
            ("input_ids", TensorType::Int64, &["batch_1", "tokens"][..]),
            (
                "attention_mask",
                TensorType::Int64,
                &["batch_1", "tokens"][..],
            ),
            (
                "position_ids",
                TensorType::Int64,
                &["batch_1", "tokens"][..],
            ),
            ("decision_positions", TensorType::Int64, &["batch_1"][..]),
            (
                "candidate_positions",
                TensorType::Int64,
                &["batch_1", "options"][..],
            ),
            ("option_mask", TensorType::Bool, &["batch_1", "options"][..]),
        ];
        for (tensor, (name, element, axes)) in self.inputs.iter().zip(expected) {
            ensure!(
                tensor.name == name && tensor.element == element && tensor.axes == axes,
                "Context Qwen input ABI mismatch"
            );
        }
        ensure!(
            self.output.name == "scores"
                && self.output.element == TensorType::Float32
                && self.output.axes == ["batch_1", "options"],
            "Context Qwen output ABI mismatch"
        );
        Ok(())
    }
    pub fn validate_focus(&self) -> Result<()> {
        ensure!(
            self.readout == Readout::FocusYesNo && self.inputs.len() == 3,
            "Focus requires its distinct pairwise yes/no ABI"
        );
        for (tensor, name) in
            self.inputs
                .iter()
                .zip(["input_ids", "attention_mask", "position_ids"])
        {
            ensure!(
                tensor.name == name
                    && tensor.element == TensorType::Int64
                    && tensor.axes == ["batch", "tokens"],
                "Focus input ABI mismatch"
            );
        }
        ensure!(
            self.output.name == "logits"
                && self.output.element == TensorType::Float32
                && self.output.axes == ["batch", "labels"],
            "Focus output ABI mismatch"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StageStatus {
    pub selected_profile: Option<Sha256Digest>,
    pub active_profile: Option<Sha256Digest>,
    pub availability: Availability,
    pub reason: String,
    pub queued: usize,
    pub running: usize,
    pub profiles: Vec<ProfileStatus>,
}
impl StageStatus {
    pub fn validate(&self, stage: Stage, configuration: &StageConfiguration) -> Result<()> {
        configuration.validate(stage)?;
        ensure!(
            self.selected_profile == configuration.selected_profile
                && !self.reason.is_empty()
                && self.reason.len() <= 1024,
            "Arbiter stage status identity/reason mismatch"
        );
        ensure!(
            self.availability != Availability::Ready
                || (configuration.enabled
                    && self.active_profile.is_some()
                    && self.active_profile == self.selected_profile),
            "ready stage lacks active selected profile"
        );
        let mut models = BTreeSet::new();
        for profile in &self.profiles {
            ensure!(
                profile.model.stage() == Some(stage)
                    && models.insert(profile.model)
                    && profile.tasks.iter().all(|task| stage.declares(*task)),
                "foreign or duplicate Arbiter stage status profile"
            );
            if let Some(fit) = &profile.cpu_fit {
                fit.validate()?;
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InitializationStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_legacy_digest: Option<Sha256Digest>,
    pub reason: String,
}
impl InitializationStatus {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.reason.is_empty() && self.reason.len() <= 1024,
            "invalid Arbiter initialization status reason"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub schema: String,
    /// Presence marks configuration as a non-writable disabled placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initialization: Option<InitializationStatus>,
    pub configuration: Configuration,
    pub focus: StageStatus,
    pub context: StageStatus,
}
impl Status {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == STATUS_SCHEMA,
            "invalid Arbiter status schema"
        );
        self.configuration.validate()?;
        if let Some(initialization) = &self.initialization {
            initialization.validate()?;
            ensure!(
                self.configuration.focus == StageConfiguration::default()
                    && self.configuration.context == StageConfiguration::default()
                    && [&self.focus, &self.context]
                        .iter()
                        .all(|stage| stage.availability != Availability::Ready),
                "initialization status requires a disabled non-ready placeholder"
            );
        }
        self.focus
            .validate(Stage::Focus, &self.configuration.focus)?;
        self.context
            .validate(Stage::Context, &self.configuration.context)
    }
}
