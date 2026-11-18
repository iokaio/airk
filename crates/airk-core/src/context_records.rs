// SPDX-License-Identifier: Apache-2.0
//! Shared context allowance, accounting, and model delivery records.
use crate::{
    arbiter::{Budget, context_action::has_native_receipt},
    identity::{Incarnation, Sha256Digest},
    index_catalog::IndexId,
    read_attempt::ReadAttemptBudget,
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const ALLOWANCE_SCHEMA: &str = "airk.context-partition-allowance/1";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    pub calls: usize,
    pub input_tokens: usize,
    pub candidates: usize,
    pub bytes: usize,
}
impl Resources {
    pub fn from_packet(packet: &crate::arbiter::context_action::ContextResponseV2) -> Result<Self> {
        Ok(Self {
            calls: packet.work.calls,
            input_tokens: packet.work.input_tokens,
            candidates: packet.candidate_count.max(packet.work.candidates),
            bytes: packet.work.bytes.max(serde_json::to_vec(packet)?.len()),
        })
    }
    pub fn checked_add(&self, other: &Self) -> Result<Self> {
        Ok(Self {
            calls: self.calls.checked_add(other.calls).ok_or_else(overflow)?,
            input_tokens: self
                .input_tokens
                .checked_add(other.input_tokens)
                .ok_or_else(overflow)?,
            candidates: self
                .candidates
                .checked_add(other.candidates)
                .ok_or_else(overflow)?,
            bytes: self.bytes.checked_add(other.bytes).ok_or_else(overflow)?,
        })
    }
    pub fn fits(&self, ceiling: &Self) -> bool {
        self.calls <= ceiling.calls
            && self.input_tokens <= ceiling.input_tokens
            && self.candidates <= ceiling.candidates
            && self.bytes <= ceiling.bytes
    }
    pub fn budget(budget: &Budget) -> Self {
        Self {
            calls: budget.calls,
            input_tokens: budget.input_tokens,
            candidates: budget.candidates,
            bytes: budget.max_bytes,
        }
    }
}

pub(super) fn overflow() -> anyhow::Error {
    anyhow::anyhow!("combined context accounting overflow")
}

/// Native adapter metadata, never public Answer arguments or a grant.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionAllowance {
    pub schema: String,
    pub attempt: Incarnation,
    pub selection: Sha256Digest,
    pub index_id: IndexId,
    pub parent_budget: Budget,
    pub resources: Resources,
    pub assessment_ordinals: Vec<u8>,
}
impl PartitionAllowance {
    /// Exact ordinary singleton Focus purpose. This is inert applicability,
    /// not proof of the actual Host selection, native grant or one-use charge.
    pub fn is_ordinary_focus(&self) -> bool {
        self.schema == ALLOWANCE_SCHEMA
            && self.parent_budget.validate().is_ok()
            && self.assessment_ordinals.is_empty()
            && self.parent_budget.calls == 1
            && self.parent_budget.rounds == 1
            && self.parent_budget.fanout == 1
            && self.resources.calls == 1
            && (1..=8192).contains(&self.resources.input_tokens)
            && (1..=64).contains(&self.resources.candidates)
            && (1024..=65_536).contains(&self.resources.bytes)
            && self.resources.fits(&Resources::budget(&self.parent_budget))
    }
    pub fn validate(&self) -> Result<()> {
        self.parent_budget.validate()?;
        ensure!(
            self.schema == ALLOWANCE_SCHEMA,
            "invalid context allowance schema"
        );
        ensure!(
            self.resources.fits(&Resources::budget(&self.parent_budget))
                && self.resources.calls > 0
                && self.resources.candidates > 0
                && self.resources.bytes >= 1024,
            "invalid context partition allocation"
        );
        let maximum = self.parent_budget.rounds.min(3);
        ensure!(
            self.assessment_ordinals.len() <= maximum
                && self
                    .assessment_ordinals
                    .windows(2)
                    .all(|pair| pair[0] < pair[1])
                && self
                    .assessment_ordinals
                    .iter()
                    .all(|ordinal| (1..=maximum).contains(&usize::from(*ordinal))),
            "invalid global assessment ordinals"
        );
        ensure!(
            self.is_ordinary_focus()
                || self.assessment_ordinals.is_empty() == (self.resources.input_tokens == 0),
            "assessment allowance and token allocation disagree"
        );
        Ok(())
    }
    pub fn validate_binding(
        &self,
        attempt: &ReadAttemptBudget,
        selection: Sha256Digest,
        index: IndexId,
    ) -> Result<()> {
        self.validate()?;
        ensure!(
            self.attempt == attempt.request_id
                && self.selection == selection
                && self.index_id == index,
            "context partition binding mismatch"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextDeliveryBinding {
    #[serde(default, skip_serializing_if = "crate::arbiter::Stage::is_context")]
    pub stage: crate::arbiter::Stage,
    pub scope: crate::arbiter::ScopeBinding,
    #[serde(deserialize_with = "crate::arbiter::context_action::required_nullable")]
    pub profile: Option<Sha256Digest>,
}
impl ContextDeliveryBinding {
    pub fn from_packet(
        packet: &crate::arbiter::context_action::ContextResponseV2,
    ) -> Result<Option<Self>> {
        use crate::arbiter::context_action::AssessmentResult;
        let actual = packet.assessment_history.iter().any(has_native_receipt);
        if !actual {
            let focus = crate::arbiter::focus::observed_focus_work(
                &packet.records,
                &crate::arbiter::focus::validation_budget(),
            )?;
            ensure!(
                packet.work.input_tokens == 0 || focus.has_native_receipt,
                "model work has no native receipt"
            );
            return Ok(None);
        }
        ensure!(
            packet.assessment.scope.configuration_revision > 0,
            "model receipt has no native policy revision"
        );
        let mut profiles = BTreeSet::new();
        for event in &packet.assessment_history {
            match &event.result {
                AssessmentResult::Selected { advice, .. }
                | AssessmentResult::Abstained { advice } => {
                    profiles.insert(Some(advice.profile));
                }
                AssessmentResult::Unavailable {
                    advice: Some(advice),
                    ..
                } => {
                    profiles.insert(Some(advice.profile));
                }
                AssessmentResult::Unavailable { advice: None, .. } => {}
            }
        }
        for application in &packet.applications {
            ensure!(
                application.scope == packet.assessment.scope,
                "foreign model application binding"
            );
            profiles.insert(application.profile);
        }
        ensure!(
            profiles.len() == 1,
            "inconsistent native model profile observations"
        );
        let profile = profiles.into_iter().next().unwrap();
        if profile.is_none() {
            use crate::arbiter::context_action::AssessmentUnavailable;
            ensure!(packet.work.input_tokens==0 && packet.assessment_history.iter().all(|event|
                matches!(&event.result,AssessmentResult::Unavailable {reason:AssessmentUnavailable::Disabled
                    |AssessmentUnavailable::NotEnrolled|AssessmentUnavailable::TaskUnsupported|AssessmentUnavailable::NoEvidence,
                    work,work_unknown:false,..} if work.input_tokens==0)),
                "absent profile is not a known no-model observation");
        }
        Ok(Some(Self {
            stage: crate::arbiter::Stage::Context,
            scope: packet.assessment.scope.clone(),
            profile,
        }))
    }
}
