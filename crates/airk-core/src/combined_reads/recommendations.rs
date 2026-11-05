// SPDX-License-Identifier: Apache-2.0
//! Pure bounded advice over an already authorized metadata snapshot. No I/O.
use super::{QuerySelection, ResolvedSelection, canonical_scope_union};
use crate::index_catalog::{IndexDefinition, QueryProject, encoded_size};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Situation {
    #[default]
    Keep,
    Reuse,
    FocusedTask,
    SharedDependencies,
    DifferentLifecycles,
    Overlap,
    IrrelevantPath,
    NeverIndexPath,
}

/// Narrow only the captured connection, including each Project's own disjoint
/// prefix grants. Filtering a union after resolution would retain excess scope.
pub fn select(
    available: &ResolvedSelection,
    choice: Option<&QuerySelection>,
) -> Result<ResolvedSelection> {
    available.validate()?;
    let Some(choice) = choice else {
        return Ok(available.clone());
    };
    choice.validate()?;
    let indexes = choice
        .index_ids
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("explicit advice subset required"))?;
    ensure!(
        choice
            .project_ids
            .iter()
            .all(|id| available.project_ids.contains(id)),
        "advice Project outside connection"
    );
    let mut selected = available.clone();
    selected.project_ids = choice.project_ids.clone();
    selected.project_ids.sort();
    selected.member_availability = choice.member_availability;
    selected
        .members
        .retain(|member| indexes.contains(&member.index_id));
    for member in &mut selected.members {
        member
            .memberships
            .retain(|item| selected.project_ids.contains(&item.membership.project_id));
        ensure!(
            !member.memberships.is_empty(),
            "advice index outside selected Project union"
        );
        member.scope = canonical_scope_union(
            member
                .memberships
                .iter()
                .flat_map(|item| item.membership.scope.iter().cloned()),
        )?;
    }
    ensure!(
        selected.members.len() == indexes.len(),
        "unknown advice selection"
    );
    // Like QuerySelection::resolve, keep explicitly selected Projects even
    // when the index subset omits all of their members. They add no scope.
    selected.validate()?;
    Ok(selected)
}

/// Observations are deliberately unknown when this metadata owner has no dated
/// coverage/cost receipt. Opening advice must never probe workers to fill them.
pub fn project(
    available: &ResolvedSelection,
    selected: &ResolvedSelection,
    projects: &[QueryProject],
    indexes: &[IndexDefinition],
    situation: Situation,
    observed_unix_ms: u64,
) -> Result<Value> {
    available.validate()?;
    selected.validate()?;
    ensure!(
        indexes.len() <= 32 && projects.len() <= 32,
        "advice candidate budget"
    );
    encoded_size(&(available, selected, projects, indexes), 1024 * 1024)?;
    let choice = QuerySelection {
        project_ids: selected.project_ids.clone(),
        index_ids: Some(selected.members.iter().map(|m| m.index_id).collect()),
        member_availability: selected.member_availability,
    };
    ensure!(
        select(available, Some(&choice))? == *selected,
        "advice scope mismatch"
    );
    let selected_ids = selected
        .members
        .iter()
        .map(|m| m.index_id)
        .collect::<Vec<_>>();
    let members = available.members.iter().map(|member| {
        let index = indexes.iter().find(|index| index.id == member.index_id);
        json!({"index_id":member.index_id,"name":index.map(|index| &index.name),"selected":selected_ids.contains(&member.index_id),"project_ids":member.project_ids(),"scope":member.scope,
            "source_revision":member.memberships[0].source_revision,"policy_epoch":member.memberships[0].policy_epoch,
            "readiness":{"state":"unknown","reason":"no_dated_observation_in_catalog"},"source_lag":"unknown","semantic_availability":"unknown","counts":{"precision":"unknown","value":null},"query_cost":"unknown"})
    }).collect::<Vec<_>>();
    let (reason, action, benefit, tradeoff) = match situation {
        Situation::Keep => (
            "No demonstrated reason to change this layout.",
            "keep_layout",
            "Keep existing policies and memberships.",
            "Narrower selection is optional; fewer indexes do not guarantee better recall or latency.",
        ),
        Situation::Reuse => (
            "You requested reuse of existing authorized evidence.",
            "review_reuse",
            "An existing index may avoid duplicate scans and embeddings.",
            "Verify actual source scope, policy and readiness; names do not establish ownership or suitable coverage.",
        ),
        Situation::FocusedTask => (
            "You identified a focused task.",
            "use_query_scope",
            "An explicit index or supported query prefix can reduce unrelated evidence and fan-out.",
            "Dependencies or specifications may be omitted. Widen deliberately within the current grant.",
        ),
        Situation::SharedDependencies => (
            "You identified shared code or specifications.",
            "use_query_scope",
            "One combined request reads each shared index once.",
            "Include only the purposeful combination; budgets are shared across selected indexes.",
        ),
        Situation::DifferentLifecycles => (
            "You identified groups with different lifecycle or policy needs.",
            "split_pending",
            "Independent indexes could provide separate policy and budgets.",
            "Prefer query filters when sufficient. Split is unavailable until P10; destination build, temporary space and future jobs cost work. Size alone is not a reason to split.",
        ),
        Situation::Overlap => (
            "You asked to review possible overlapping indexes; physical overlap is unproven.",
            "review_reuse",
            "Choose one only after its profile and coverage are adequate.",
            "Different profiles can be intentional. Membership removal and index deletion are separate reviewed operations; no automatic merge or pruning.",
        ),
        Situation::IrrelevantPath => (
            "You identified evidence irrelevant to this task.",
            "use_query_scope",
            "Narrow the query or review Project defaults.",
            "Other Projects may need this evidence. Query omission does not permanently deny a path.",
        ),
        Situation::NeverIndexPath => (
            "You explicitly identified evidence that should never be indexed.",
            "removal_pending",
            "A reviewed permanent exclusion applies to every user of this index.",
            "Use the selected index's Sources controls to review permanent exclusion. Advice-linked management previews remain pending P10. Review all affected Projects and other indexes; exclusion, cleanup and unblock are separate. Source files are never removed.",
        ),
    };
    let selected_scope = selected
        .members
        .iter()
        .map(|m| json!({"index_id":m.index_id,"project_ids":m.project_ids(),"scope":m.scope}))
        .collect::<Vec<_>>();
    Ok(json!({
        "schema":"airk.recommendations/1","observed_unix_ms":observed_unix_ms,
        "observation":{"source":"authorized_catalog_snapshot","evidence":"observed_metadata","coverage_and_cost":"unknown","expires":"revalidate_before_action"},
        "available":members,"selected":selected.observation()?,"selected_scope":selected_scope,
        "omitted_index_ids":available.members.iter().filter(|m| !selected_ids.contains(&m.index_id)).map(|m|m.index_id).collect::<Vec<_>>(),
        "projects":projects.iter().filter(|p|available.project_ids.contains(&p.id)).map(|p|json!({"project_id":p.id,"name":p.name,"defaults_revision":p.defaults_revision,"default_indexes":p.default_indexes.iter().filter(|id|available.member(**id).is_some_and(|m|m.project_ids().contains(&p.id))).collect::<Vec<_>>()})).collect::<Vec<_>>(),
        "connection_state":"active","pending_scope":null,
        "cards":[{"id":"selection","reason":reason,"action":action,"expected_benefit":benefit,"tradeoff":tradeoff,"evidence":"user_stated_situation_and_authorized_metadata","affected_project_ids":selected.project_ids,"affected_index_ids":selected_ids},
            {"id":"unknown_observations","reason":"Saved dated coverage, source lag, semantic readiness, overlap and cost observations are unavailable here.","action":"manual_selection","expected_benefit":"Review an explicit scope without starting new work.","tradeoff":"No completeness, safe split or measured savings claim is possible. Request existing metadata observations separately."},
            {"id":"keep_layout","reason":"Recommendations confer no authority.","action":"keep_layout","expected_benefit":"Keep this layout or dismiss advice.","tradeoff":"Temporary selection never changes grants, indexing policy or saved defaults. Saving defaults requires separate native review and affects future selections for the Project's users."}],
        "limits":{"candidate_indexes":32,"advisory_rows":8,"evidence_queries":0,"source_scans":0,"worker_probes":0},
        "library_coverage":"connection_only","truncated":false,"continuation":null
    }))
}
