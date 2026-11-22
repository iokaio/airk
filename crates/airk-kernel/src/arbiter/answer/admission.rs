// SPDX-License-Identifier: Apache-2.0
//! The service's admission plan for one Answer (final plan section 12.4.1,
//! step L2b). The service's Answer job table decides how much of a request
//! fits its deadline; the loop is then bounded by that plan and its reply
//! discloses every reduction. A plan never widens a request, never changes
//! authority and never changes the Context rung: it can only drop to the
//! no-model loop.
use super::*;

/// What the service admitted for one Answer: the limits the loop may use and
/// the reductions its reply must disclose.
#[derive(Clone, Debug)]
pub struct AdmissionPlan {
    pub parts: usize,
    /// Per part.
    pub sub_needs: usize,
    /// Per part.
    pub rounds: usize,
    /// False runs the no-model loop: no Context card call is made.
    pub context_model: bool,
    /// The plan's own reductions: parts, sub-needs, rounds and the Context
    /// model. Every other kind is the loop's own to disclose.
    pub degradations: Vec<v3::Degradation>,
}

/// A request's budget after admission.
pub(super) struct Admitted {
    /// The budget as asked, before the plan: the reply is validated against
    /// it, and the disclosures say what the plan took away.
    pub(super) asked: v3::BudgetV3,
    pub(super) context_model: bool,
    pub(super) degradations: Vec<v3::Degradation>,
}

/// Bound `budget` by `plan` and return what the reply discloses. A limit the
/// budget already keeps (a combined member's allowance, say) is not a
/// reduction of the plan and is not disclosed as one; a reduction the plan
/// does not disclose is refused.
pub(super) fn apply(plan: Option<&AdmissionPlan>, budget: &mut v3::BudgetV3) -> Result<Admitted> {
    let asked = budget.clone();
    let Some(plan) = plan else {
        return Ok(Admitted {
            asked,
            context_model: true,
            degradations: vec![],
        });
    };
    ensure!(
        plan.parts >= 1 && plan.sub_needs >= 1 && plan.rounds >= 1,
        "invalid Answer admission plan"
    );
    let (mut parts, mut sub_needs, mut rounds, mut model) = (None, None, None, None);
    for degradation in &plan.degradations {
        match degradation {
            v3::Degradation::Parts { reason, .. } => parts = Some(*reason),
            v3::Degradation::SubNeeds { reason, .. } => sub_needs = Some(*reason),
            v3::Degradation::Rounds { reason, .. } => rounds = Some(*reason),
            v3::Degradation::Model { stage, applied, .. } => {
                ensure!(
                    stage.is_context() && applied.is_none() && !plan.context_model,
                    "an Answer admission plan cannot change the Context rung"
                );
                model = Some(degradation.clone());
            }
            _ => anyhow::bail!("an Answer admission plan discloses only its own reductions"),
        }
    }
    budget.max_parts = budget.max_parts.min(plan.parts);
    budget.max_sub_needs = budget.max_sub_needs.min(plan.sub_needs);
    budget.rounds = budget.rounds.min(plan.rounds);
    let mut degradations = vec![];
    if budget.max_parts < asked.max_parts {
        degradations.push(v3::Degradation::Parts {
            requested: asked.max_parts,
            applied: budget.max_parts,
            reason: parts.context("undisclosed Answer admission reduction of parts")?,
        });
    }
    if budget.max_sub_needs < asked.max_sub_needs {
        degradations.push(v3::Degradation::SubNeeds {
            requested: asked.max_sub_needs,
            applied: budget.max_sub_needs,
            reason: sub_needs.context("undisclosed Answer admission reduction of sub-needs")?,
        });
    }
    if budget.rounds < asked.rounds {
        degradations.push(v3::Degradation::Rounds {
            requested: asked.rounds,
            applied: budget.rounds,
            reason: rounds.context("undisclosed Answer admission reduction of rounds")?,
        });
    }
    degradations.extend(model);
    Ok(Admitted {
        asked,
        context_model: plan.context_model,
        degradations,
    })
}

impl crate::Kernel {
    /// Answer /3 as the service's job table admitted it: `answer_with`,
    /// bounded by `plan`, with the plan's reductions in the reply.
    pub fn answer_admitted(
        &self,
        request: v3::ContextRequestV3,
        env: AnswerEnv<'_>,
        plan: &AdmissionPlan,
    ) -> Result<AnswerOutcome> {
        self.answer_run(request, env, Some(plan))
    }

    /// The verified binding an Answer of this connection would run under
    /// now: the same owner observation and the same checks `answer_with`
    /// makes before it runs. The service keys its job table on this, never on
    /// anything a client sent. A combined member is not a job of its own.
    pub fn answer_scope(&self) -> Result<ScopeBinding> {
        let context = self
            .arbiter_context
            .as_ref()
            .context("Answer requires a native admitted index connection")?;
        ensure!(
            context.answer_partition.is_none(),
            "a combined Answer member is not a service job"
        );
        ensure!(
            self.scoped_read_fingerprint()? == Some(context.authority.fingerprint),
            "Answer source authority expired"
        );
        let publication = self.arbiter_publication()?;
        let scope = observed_application(context, &publication, None)?.scope;
        ensure!(
            scope.index_id == context.authority.index_id
                && scope.authorization == context.authority.fingerprint
                && scope.publication == publication,
            "Answer owner supplied a foreign binding"
        );
        Ok(scope)
    }

    /// The Context model an Answer's card calls would run on now; `None` is
    /// the no-model loop. For the service's admission estimate only.
    pub fn answer_context_model(&self) -> Result<Option<device::SelectedModel>> {
        let context = self
            .arbiter_context
            .as_ref()
            .context("Answer requires a native admitted index connection")?;
        let publication = self.arbiter_publication()?;
        Ok(context
            .owner
            .card_model(context.worker, context.authority, &publication)
            .ok()
            .flatten()
            .map(|model| model.model))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use v3::{Degradation, DegradationReason};

    fn plan(parts: usize, sub_needs: usize, rounds: usize) -> AdmissionPlan {
        AdmissionPlan {
            parts,
            sub_needs,
            rounds,
            context_model: true,
            degradations: vec![],
        }
    }

    #[test]
    fn no_plan_leaves_the_budget_and_discloses_nothing() -> Result<()> {
        let mut budget = v3::BudgetV3::default();
        let admitted = apply(None, &mut budget)?;
        assert_eq!(budget, v3::BudgetV3::default());
        assert_eq!(admitted.asked, v3::BudgetV3::default());
        assert!(admitted.context_model && admitted.degradations.is_empty());
        Ok(())
    }

    #[test]
    fn a_plan_bounds_the_budget_and_discloses_each_reduction_against_the_budget_asked() -> Result<()>
    {
        let mut budget = v3::BudgetV3::default();
        let mut reduced = plan(2, 4, 1);
        reduced.context_model = false;
        reduced.degradations = vec![
            Degradation::Parts {
                requested: 4,
                applied: 2,
                reason: DegradationReason::QueueWait,
            },
            Degradation::Rounds {
                requested: 3,
                applied: 1,
                reason: DegradationReason::DeadlineEstimate,
            },
            Degradation::Model {
                stage: Stage::Context,
                requested: device::SelectedModel::Arbiter,
                applied: None,
                reason: DegradationReason::QueueWait,
            },
        ];
        let admitted = apply(Some(&reduced), &mut budget)?;
        assert_eq!(
            (budget.max_parts, budget.max_sub_needs, budget.rounds),
            (2, 4, 1)
        );
        assert_eq!(admitted.asked, v3::BudgetV3::default());
        assert!(!admitted.context_model);
        let wire = serde_json::to_value(&admitted.degradations)?;
        assert_eq!(
            wire,
            serde_json::json!([
                {"kind":"parts","requested":4,"applied":2,"reason":"queue_wait"},
                {"kind":"rounds","requested":3,"applied":1,"reason":"deadline_estimate"},
                {"kind":"model","stage":"context","requested":"arbiter","reason":"queue_wait"}
            ])
        );
        Ok(())
    }

    #[test]
    fn a_limit_the_budget_already_keeps_is_not_disclosed_as_the_plans() -> Result<()> {
        // A request (or a combined member's allowance) already at two parts:
        // a plan that states four-to-two took nothing away.
        let mut budget = v3::BudgetV3 {
            max_parts: 2,
            ..v3::BudgetV3::default()
        };
        let mut stated = plan(2, 4, 3);
        stated.degradations = vec![Degradation::Parts {
            requested: 4,
            applied: 2,
            reason: DegradationReason::QueueWait,
        }];
        let admitted = apply(Some(&stated), &mut budget)?;
        assert_eq!(budget.max_parts, 2);
        assert!(admitted.degradations.is_empty());
        Ok(())
    }

    #[test]
    fn a_plan_never_widens_and_refuses_what_it_cannot_honour() {
        let mut budget = v3::BudgetV3 {
            max_parts: 1,
            max_sub_needs: 2,
            rounds: 1,
            ..v3::BudgetV3::default()
        };
        apply(Some(&plan(4, 4, 3)), &mut budget).unwrap();
        assert_eq!(
            (budget.max_parts, budget.max_sub_needs, budget.rounds),
            (1, 2, 1)
        );
        // A reduction without its disclosure.
        let mut budget = v3::BudgetV3::default();
        assert!(apply(Some(&plan(4, 4, 2)), &mut budget).is_err());
        // A zero limit.
        assert!(apply(Some(&plan(0, 4, 3)), &mut v3::BudgetV3::default()).is_err());
        // A change of rung, which the loop cannot make per Answer.
        let mut rung = plan(4, 4, 3);
        rung.degradations = vec![Degradation::Model {
            stage: Stage::Context,
            requested: device::SelectedModel::Arbiter,
            applied: Some(device::SelectedModel::Arbiter),
            reason: DegradationReason::QueueWait,
        }];
        assert!(apply(Some(&rung), &mut v3::BudgetV3::default()).is_err());
        // A no-model disclosure while the plan keeps the model.
        let mut kept = plan(4, 4, 3);
        kept.degradations = vec![Degradation::Model {
            stage: Stage::Context,
            requested: device::SelectedModel::Arbiter,
            applied: None,
            reason: DegradationReason::QueueWait,
        }];
        assert!(apply(Some(&kept), &mut v3::BudgetV3::default()).is_err());
        // A disclosure that is the loop's own.
        let mut foreign = plan(4, 4, 3);
        foreign.degradations = vec![Degradation::FocusAbsent];
        assert!(apply(Some(&foreign), &mut v3::BudgetV3::default()).is_err());
    }
}
