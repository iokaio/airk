// SPDX-License-Identifier: Apache-2.0
//! The admission estimator and degrade-to-fit (plan section 12.4.1, ideas 1a
//! and 1b). Pure arithmetic over injected stage costs: no clock, no device.
use airk_core::arbiter::{
    Stage,
    answer_loop::{BudgetV3, Degradation, DegradationReason},
    device::SelectedModel,
};

/// The Context model rung an Answer plans with: the Arbiter, the one Context
/// model since Large was retired. `None` in a [`Shape`] is the no-model
/// loop, the bottom of the ladder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ContextRung {
    Compact,
}
impl ContextRung {
    pub(crate) fn model(self) -> SelectedModel {
        match self {
            Self::Compact => SelectedModel::Arbiter,
        }
    }
}

/// The planned size of one Answer: what the estimator prices and what
/// degrade-to-fit reduces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Shape {
    pub(crate) parts: usize,
    pub(crate) sub_needs: usize,
    pub(crate) rounds: usize,
    pub(crate) model: Option<ContextRung>,
}
impl Shape {
    pub(crate) fn requested(budget: &BudgetV3, model: Option<ContextRung>) -> Self {
        Self {
            parts: budget.max_parts,
            sub_needs: budget.max_sub_needs,
            rounds: budget.rounds,
            model,
        }
    }
    /// At least as much of every dimension and as good a rung as `other`:
    /// a caller wanting `other` loses nothing by receiving `self`.
    pub(crate) fn covers(&self, other: &Self) -> bool {
        self.parts >= other.parts
            && self.sub_needs >= other.sub_needs
            && self.rounds >= other.rounds
            && self.model >= other.model
    }
    /// The smallest disclosed answer: one part, one sub-need, one round and
    /// the no-model loop.
    pub(crate) const SMALLEST: Self = Self {
        parts: 1,
        sub_needs: 1,
        rounds: 1,
        model: None,
    };
}

/// Measured cost of one rung. Until L5b and L7 measure per-device figures,
/// [`CostTable::default`] holds placeholders (answer-loop-changes entry 19).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StageCosts {
    /// Admission, Split, Compose and Union.
    pub(crate) fixed_ms: u64,
    /// Plan, once per part.
    pub(crate) per_part_ms: u64,
    /// Retrieve, Read, Coverage and Follow-up, per sub-need and round.
    pub(crate) per_sub_need_round_ms: u64,
}
impl StageCosts {
    fn work_ms(&self, shape: &Shape) -> u64 {
        let per_part = self.per_part_ms.saturating_add(
            self.per_sub_need_round_ms
                .saturating_mul((shape.sub_needs as u64).saturating_mul(shape.rounds as u64)),
        );
        self.fixed_ms
            .saturating_add(per_part.saturating_mul(shape.parts as u64))
    }
}

/// Per-rung costs. A rung without costs is unavailable to the estimator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CostTable {
    pub(crate) no_model: StageCosts,
    pub(crate) compact: Option<StageCosts>,
}
impl Default for CostTable {
    fn default() -> Self {
        Self {
            no_model: StageCosts {
                fixed_ms: 2_000,
                per_part_ms: 500,
                per_sub_need_round_ms: 2_000,
            },
            compact: Some(StageCosts {
                fixed_ms: 4_000,
                per_part_ms: 2_000,
                per_sub_need_round_ms: 4_000,
            }),
        }
    }
}
impl CostTable {
    fn rung(&self, model: Option<ContextRung>) -> Option<&StageCosts> {
        match model {
            None => Some(&self.no_model),
            Some(ContextRung::Compact) => self.compact.as_ref(),
        }
    }
    /// Estimated work of `shape`, scaled by the learned correction in
    /// thousandths; `None` when its rung is unavailable.
    pub(crate) fn work_ms(&self, shape: &Shape, correction_permille: u64) -> Option<u64> {
        self.rung(shape.model)
            .map(|costs| costs.work_ms(shape).saturating_mul(correction_permille) / 1000)
    }
    /// The highest available rung at or below `requested`.
    fn available(&self, requested: Option<ContextRung>) -> Option<ContextRung> {
        match requested {
            Some(ContextRung::Compact) if self.compact.is_some() => Some(ContextRung::Compact),
            _ => None,
        }
    }
}

/// The shape an Answer runs with and the degradations its reply discloses.
#[derive(Clone, Debug)]
pub(crate) struct Fitted {
    pub(crate) shape: Shape,
    pub(crate) work_ms: u64,
    pub(crate) degradations: Vec<Degradation>,
}

/// One reduction step, cheapest loss first: rounds down to one, sub-needs and
/// parts down to two, then to one, and last the no-model loop. (The step from
/// Large to Compact left with the Large model.)
fn reduce(shape: &mut Shape) -> bool {
    if shape.rounds > 1 {
        shape.rounds -= 1;
    } else if shape.sub_needs > 2 {
        shape.sub_needs -= 1;
    } else if shape.parts > 2 {
        shape.parts -= 1;
    } else if shape.sub_needs > 1 {
        shape.sub_needs -= 1;
    } else if shape.parts > 1 {
        shape.parts -= 1;
    } else if shape.model.is_some() {
        shape.model = None;
    } else {
        return false;
    }
    true
}

/// The first shape on the ladder whose work fits `available_ms`, or `None`
/// when even [`Shape::SMALLEST`] does not. `deadline_ms` is the request's own
/// deadline: when the requested work alone exceeds it the reason is
/// `deadline_estimate`, otherwise the queue wait caused the reduction.
pub(crate) fn degrade_to_fit(
    costs: &CostTable,
    correction_permille: u64,
    requested: Shape,
    available_ms: u64,
    deadline_ms: u64,
) -> Option<Fitted> {
    fit(costs, correction_permille, requested, deadline_ms, |work| {
        work <= available_ms
    })
}

/// The smallest disclosed answer, whatever it costs: used only when even it
/// cannot fit the request's own deadline with no wait, where a ticket would
/// never help (the kernel then returns its disclosed partial at the deadline).
pub(crate) fn smallest(
    costs: &CostTable,
    correction_permille: u64,
    requested: Shape,
    deadline_ms: u64,
) -> Fitted {
    disclose(
        costs,
        correction_permille,
        requested,
        start_of(costs, requested),
        Shape::SMALLEST,
        deadline_ms,
    )
}

fn start_of(costs: &CostTable, requested: Shape) -> Shape {
    Shape {
        model: costs.available(requested.model),
        ..requested
    }
}

fn fit(
    costs: &CostTable,
    correction_permille: u64,
    requested: Shape,
    deadline_ms: u64,
    fits: impl Fn(u64) -> bool,
) -> Option<Fitted> {
    let start = start_of(costs, requested);
    let mut shape = start;
    loop {
        let work = costs
            .work_ms(&shape, correction_permille)
            .expect("ladder rungs are available");
        if fits(work) {
            return Some(disclose(
                costs,
                correction_permille,
                requested,
                start,
                shape,
                deadline_ms,
            ));
        }
        if !reduce(&mut shape) {
            return None;
        }
    }
}

fn disclose(
    costs: &CostTable,
    correction_permille: u64,
    requested: Shape,
    start: Shape,
    applied: Shape,
    deadline_ms: u64,
) -> Fitted {
    let work_ms = costs
        .work_ms(&applied, correction_permille)
        .expect("applied rung is available");
    let reason = if costs
        .work_ms(&start, correction_permille)
        .is_some_and(|work| work > deadline_ms)
    {
        DegradationReason::DeadlineEstimate
    } else {
        DegradationReason::QueueWait
    };
    let mut degradations = vec![];
    if applied.parts < requested.parts {
        degradations.push(Degradation::Parts {
            requested: requested.parts,
            applied: applied.parts,
            reason,
        });
    }
    if applied.sub_needs < requested.sub_needs {
        degradations.push(Degradation::SubNeeds {
            requested: requested.sub_needs,
            applied: applied.sub_needs,
            reason,
        });
    }
    if applied.rounds < requested.rounds {
        degradations.push(Degradation::Rounds {
            requested: requested.rounds,
            applied: applied.rounds,
            reason,
        });
    }
    if let Some(wanted) = requested.model
        && applied.model != Some(wanted)
    {
        let reason = if start.model != Some(wanted) {
            DegradationReason::ModelUnavailable
        } else {
            reason
        };
        degradations.push(Degradation::Model {
            stage: Stage::Context,
            requested: wanted.model(),
            applied: applied.model.map(ContextRung::model),
            reason,
        });
    }
    Fitted {
        shape: applied,
        work_ms,
        degradations,
    }
}
