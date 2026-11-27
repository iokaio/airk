// SPDX-License-Identifier: Apache-2.0
//! The gathering rules of the Answer /3 loop (kernel CONTRACT "Gathering
//! rules"): what is fetched for a request that carries caller inputs, and how
//! the passages are offered and delivered. Each rule is a switch. Every
//! switch is on in a product build; the private evaluation fixture may bind
//! another set for its process (`bind_gathering_rules`, test-support builds
//! only), so one run can turn a rule off and measure what it adds.
use super::*;
#[cfg(feature = "test-support")]
use std::sync::OnceLock;

/// The switches, one per rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GatheringRules {
    /// Rule 1: each sub-need's round-one search also runs on source code
    /// only, and a standing sub-need's on configuration only.
    pub lanes: bool,
    /// Rule 2: standing sub-needs chosen by the request's `deliverable`.
    pub standing: bool,
    /// Rule 3a: project guidance files at the root, the top-level folder and
    /// the folders of each sub-need's best passages.
    pub guidance: bool,
    /// Rule 3b: files the retained evidence names by repository path.
    pub cited: bool,
    /// Rule 4: enclosing definitions, callees, call sites and users of an
    /// inherited type, from the index's code facts.
    pub structure: bool,
    /// Rule 5: a passage fetched for one sub-need is a candidate for every
    /// sub-need of the request.
    pub cross: bool,
    /// Rule 6: each sub-need's best passages inside one byte budget.
    pub delivery: bool,
}

impl Default for GatheringRules {
    fn default() -> Self {
        Self::all()
    }
}

impl GatheringRules {
    /// The switch names, in rule order.
    pub const NAMES: [&'static str; 7] = [
        "lanes",
        "standing",
        "guidance",
        "cited",
        "structure",
        "cross",
        "delivery",
    ];

    pub const fn all() -> Self {
        Self {
            lanes: true,
            standing: true,
            guidance: true,
            cited: true,
            structure: true,
            cross: true,
            delivery: true,
        }
    }

    pub const fn none() -> Self {
        Self {
            lanes: false,
            standing: false,
            guidance: false,
            cited: false,
            structure: false,
            cross: false,
            delivery: false,
        }
    }

    fn slot(&mut self, name: &str) -> Result<&mut bool> {
        Ok(match name {
            "lanes" => &mut self.lanes,
            "standing" => &mut self.standing,
            "guidance" => &mut self.guidance,
            "cited" => &mut self.cited,
            "structure" => &mut self.structure,
            "cross" => &mut self.cross,
            "delivery" => &mut self.delivery,
            _ => anyhow::bail!("unknown gathering rule {name:?}"),
        })
    }

    /// `all`, `none`, or a comma list of rule names; `-name` turns one off,
    /// so `all,-structure` runs every rule but structure following. A list
    /// starts from `none` unless it names `all` first.
    pub fn parse(text: &str) -> Result<Self> {
        let mut rules = Self::none();
        let mut seen = BTreeSet::new();
        for token in text.split(',').map(str::trim) {
            ensure!(
                !token.is_empty() && seen.insert(token),
                "empty or repeated gathering rule"
            );
            match token {
                "all" => rules = Self::all(),
                "none" => rules = Self::none(),
                _ => match token.strip_prefix('-') {
                    Some(name) => *rules.slot(name)? = false,
                    None => *rules.slot(token)? = true,
                },
            }
        }
        Ok(rules)
    }

    /// The rules that are on, in rule order.
    pub fn names(self) -> Vec<&'static str> {
        let mut copy = self;
        Self::NAMES
            .into_iter()
            .filter(|name| copy.slot(name).is_ok_and(|on| *on))
            .collect()
    }

    /// The wire names of the rules that are on for a request whose rules
    /// apply, in rule order.
    pub(crate) fn wire(self) -> Vec<v3::GatheringRule> {
        [
            (self.lanes, v3::GatheringRule::SearchLanes),
            (self.standing, v3::GatheringRule::StandingNeeds),
            (self.guidance, v3::GatheringRule::ProjectGuidance),
            (self.cited, v3::GatheringRule::CitedFiles),
            (self.structure, v3::GatheringRule::StructureFollowing),
            (self.cross, v3::GatheringRule::EveryPassageToEveryNeed),
            (self.delivery, v3::GatheringRule::DeliveryByNeed),
        ]
        .into_iter()
        .filter_map(|(on, rule)| on.then_some(rule))
        .collect()
    }
}

#[cfg(feature = "test-support")]
static RULES: OnceLock<GatheringRules> = OnceLock::new();

/// Bound once per process, only by the private evaluation fixture (from
/// `AIRK_ARBITER_EVALUATION_ANSWER_GATHERING`). A second binding must repeat
/// the same rules. A product build has no binder: every rule is on.
#[cfg(feature = "test-support")]
pub fn bind_gathering_rules(rules: GatheringRules) -> Result<()> {
    let bound = *RULES.get_or_init(|| rules);
    ensure!(
        bound == rules,
        "gathering rules are already bound differently"
    );
    Ok(())
}

pub(crate) fn gathering_rules() -> GatheringRules {
    #[cfg(feature = "test-support")]
    {
        RULES.get().copied().unwrap_or_default()
    }
    #[cfg(not(feature = "test-support"))]
    {
        GatheringRules::default()
    }
}

/// One standing sub-need: a need every piece of work of some kind has and
/// callers seldom state (Core `SubNeedOrigin::Standing`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Standing {
    Rules,
    RunTests,
    BuildFile,
    ErrorPath,
}

impl Standing {
    /// The sub-need's text, its own search query.
    pub(crate) const fn text(self) -> &'static str {
        match self {
            Self::Rules => {
                "Project rules that say what a change of this kind must include before it is done: required tests, test commands to run, logs or documents to update, review."
            }
            Self::RunTests => {
                "The script or command that runs this project's tests, its options, and which tests each option includes."
            }
            Self::BuildFile => {
                "The build or project file that configures which tests run and what they need."
            }
            Self::ErrorPath => {
                "Where an exception thrown by this code is caught and turned into the message the user sees."
            }
        }
    }

    /// Whether its evidence is seldom prose or code, so its search also runs
    /// on configuration only (rule 1).
    pub(crate) const fn configuration_lane(self) -> bool {
        matches!(self, Self::RunTests | Self::BuildFile)
    }
}

/// The standing sub-needs of a request: a code change takes all four, a test
/// the first three, and any other deliverable or none the first three only
/// when the caller stated needs. A bare task takes none.
pub(crate) fn standing(
    deliverable: Option<v3::Deliverable>,
    caller_needs: bool,
) -> &'static [Standing] {
    const ALL: [Standing; 4] = [
        Standing::Rules,
        Standing::RunTests,
        Standing::BuildFile,
        Standing::ErrorPath,
    ];
    match deliverable {
        Some(v3::Deliverable::CodeChange) => &ALL,
        Some(v3::Deliverable::Test) => &ALL[..3],
        _ if caller_needs => &ALL[..3],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rule_list_parses_removals_after_all_and_refuses_unknown_names() -> Result<()> {
        assert_eq!(GatheringRules::parse("all")?, GatheringRules::all());
        assert_eq!(GatheringRules::parse("none")?, GatheringRules::none());
        let rules = GatheringRules::parse("all,-structure,-cross")?;
        assert!(rules.lanes && rules.standing && rules.guidance && rules.cited && rules.delivery);
        assert!(!rules.structure && !rules.cross);
        assert_eq!(
            GatheringRules::parse("lanes,delivery")?.names(),
            ["lanes", "delivery"]
        );
        for bad in ["", "lanes,lanes", "lanes,,cited", "everything", "-nothing"] {
            assert!(GatheringRules::parse(bad).is_err(), "{bad:?}");
        }
        Ok(())
    }

    #[test]
    fn standing_needs_follow_the_deliverable_and_a_bare_task_takes_none() {
        use v3::Deliverable::*;
        assert_eq!(standing(Some(CodeChange), false).len(), 4);
        assert_eq!(standing(Some(CodeChange), true)[3], Standing::ErrorPath);
        assert_eq!(standing(Some(Test), false).len(), 3);
        assert!(standing(Some(Document), false).is_empty());
        assert!(standing(Some(Analysis), false).is_empty());
        assert_eq!(standing(Some(Analysis), true).len(), 3);
        assert_eq!(standing(None, true).len(), 3);
        assert!(standing(None, false).is_empty());
        assert!(
            standing(Some(CodeChange), false)
                .iter()
                .all(|need| !need.text().is_empty() && need.text().len() <= v3::MAX_NEED_BYTES)
        );
    }
}
