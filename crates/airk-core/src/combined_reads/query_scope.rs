// SPDX-License-Identifier: Apache-2.0
//! Bounded query-grant discovery inputs. Metadata identifiers confer no authority.
use crate::index_catalog::{IndexId, QueryProjectId};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryScopeView {
    Projects,
    Indexes,
    Sources,
    Recommendations,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QueryScopeArgs {
    pub view: QueryScopeView,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub selection: Option<super::QuerySelection>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub situation: Option<super::recommendations::Situation>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub project_id: Option<QueryProjectId>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub index_id: Option<IndexId>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub cursor: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub limit: Option<usize>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present"
    )]
    pub max_bytes: Option<usize>,
}
fn present<'de, D, T>(deserializer: D) -> std::result::Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}
impl QueryScopeArgs {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            match self.view {
                QueryScopeView::Projects => self.project_id.is_none() && self.index_id.is_none(),
                QueryScopeView::Indexes => self.project_id.is_some() && self.index_id.is_none(),
                QueryScopeView::Sources => self.project_id.is_some() && self.index_id.is_some(),
                QueryScopeView::Recommendations =>
                    self.project_id.is_none() && self.index_id.is_none() && self.cursor.is_none(),
            },
            "query_scope view requires its exact Project/index selectors"
        );
        if self.view == QueryScopeView::Recommendations {
            ensure!(
                self.limit.is_none(),
                "recommendations uses a fixed eight-card ceiling, not paged limits"
            );
            if let Some(selection) = &self.selection {
                selection.validate()?;
                ensure!(
                    selection.index_ids.is_some(),
                    "advice selection requires an explicit index subset"
                );
            }
        } else {
            ensure!(
                self.selection.is_none() && self.situation.is_none(),
                "advice controls require recommendations view"
            );
        }
        ensure!(
            (1..=32).contains(&self.limit())
                && (1..=65536).contains(&self.max_bytes())
                && self
                    .cursor
                    .as_ref()
                    .is_none_or(|cursor| !cursor.is_empty() && cursor.len() <= 32768),
            "query_scope pagination budget exceeded"
        );
        Ok(())
    }
    pub fn limit(&self) -> usize {
        self.limit.unwrap_or(16)
    }
    pub fn max_bytes(&self) -> usize {
        self.max_bytes.unwrap_or(16384)
    }
}

/// Object-shaped catalog for MCP clients; cross-field requirements are also
/// checked by the same native validator before metadata or member work.
pub fn schema() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["view"],"properties":{
        "view":{"type":"string","enum":["projects","indexes","sources","recommendations"],"description":"projects accepts no IDs; indexes requires project_id only; sources requires project_id and index_id. recommendations accepts optional selection/situation, no IDs or cursor, and reads only authorized saved metadata."},
        "selection":{"type":"object","additionalProperties":false,"required":["project_ids","index_ids"],"properties":{
            "project_ids":{"type":"array","minItems":1,"maxItems":32,"uniqueItems":true,"items":{"type":"string","pattern":"^prj_[0-9a-f]{32}$"}},
            "index_ids":{"type":"array","minItems":1,"maxItems":32,"uniqueItems":true,"items":{"type":"string","pattern":"^idx_[0-9a-f]{32}$"}},
            "member_availability":{"type":"string","enum":["report","require"],"default":"report"}
        }},
        "situation":{"type":"string","enum":["keep","reuse","focused_task","shared_dependencies","different_lifecycles","overlap","irrelevant_path","never_index_path"]},
        "project_id":{"type":"string","pattern":"^prj_[0-9a-f]{32}$"},
        "index_id":{"type":"string","pattern":"^idx_[0-9a-f]{32}$"},
        "cursor":{"type":"string","minLength":1,"maxLength":32768},
        "limit":{"type":"integer","minimum":1,"maximum":32,"default":16},
        "max_bytes":{"type":"integer","minimum":1,"maximum":65536,"default":16384}
    }})
}

/// The legacy configured-binding catalog cannot infer a reviewed Project grant.
pub fn legacy_body(args: &QueryScopeArgs) -> Result<Value> {
    args.validate()?;
    ensure!(
        args.cursor.is_none(),
        "legacy query_scope has no continuation"
    );
    let body = json!({"schema":"airk.query-scope/1","mode":"legacy","status":"configuration_required",
        "projects":[],"indexes":[],"sources":[],"truncated":false,"continuation":null,
        "grant":null,"scope_revision":null,
        "reason":"Connect using an explicitly reviewed Project/index grant to discover new query scope."});
    crate::index_catalog::encoded_size(&body, args.max_bytes())?;
    Ok(body)
}

/// An installed single-index authority is a frozen observation, not current
/// catalog discovery. Expose its real identity without inventing a fresh grant,
/// Project names, source paths, or permission to widen the selection.
pub fn installed_body(
    args: &QueryScopeArgs,
    observation: &crate::scoped_reads::ScopeObservation,
) -> Result<Value> {
    use crate::scoped_reads::ScopeReadRefused;
    use std::collections::BTreeSet;
    args.validate()?;
    observation.validate()?;
    ensure!(
        args.cursor.is_none(),
        "installed query_scope has no continuation"
    );
    let projects = observation
        .members
        .iter()
        .flat_map(|member| {
            member
                .memberships
                .iter()
                .map(|membership| membership.project_id)
        })
        .collect::<BTreeSet<_>>();
    let indexes = observation
        .members
        .iter()
        .map(|member| member.index_id)
        .collect::<BTreeSet<_>>();
    if let Some(project) = args.project_id {
        ensure!(projects.contains(&project), ScopeReadRefused);
    }
    if let Some(index) = args.index_id {
        ensure!(
            observation
                .members
                .iter()
                .any(|member| member.index_id == index
                    && args.project_id.is_none_or(|project| member
                        .memberships
                        .iter()
                        .any(|membership| membership.project_id == project))),
            ScopeReadRefused
        );
    }
    if let Some(selected) = &args.selection {
        ensure!(
            selected
                .project_ids
                .iter()
                .all(|project| projects.contains(project)),
            ScopeReadRefused
        );
        if let Some(selected_indexes) = &selected.index_ids {
            ensure!(
                selected_indexes
                    .iter()
                    .all(
                        |index| observation
                            .members
                            .iter()
                            .any(|member| member.index_id == *index
                                && member.memberships.iter().any(|membership| selected
                                    .project_ids
                                    .contains(&membership.project_id)))
                    ),
                ScopeReadRefused
            );
        }
    }
    let reason = match observation.provenance {
        crate::grant_provenance::GrantProvenanceKind::ImportedLegacySnapshot => {
            "This imported legacy grant authorizes only the frozen selection shown here. Current catalog discovery and widening require an explicitly reviewed Project/index session."
        }
        crate::grant_provenance::GrantProvenanceKind::UserReviewedProjectGrant => {
            "This installed single-index grant authorizes only the frozen selection shown here. Current catalog discovery and widening require a reviewed Project/index discovery session."
        }
    };
    let body = json!({"schema":"airk.query-scope/1","mode":"frozen_single_index","status":"configuration_required",
        "projects":[],"indexes":[],"sources":[],"truncated":false,"continuation":null,
        "grant":observation.grant,"scope_revision":null,"scope_observation":observation,
        "observed_selection":{"project_ids":projects,"index_ids":indexes},
        "catalog_discovery":"unavailable_on_this_connection","reason":reason});
    crate::index_catalog::encoded_size(&body, args.max_bytes())?;
    Ok(body)
}

#[cfg(test)]
mod installed_tests {
    use super::*;
    use crate::{
        identity::{Incarnation, Sha256Digest},
        index_catalog::*,
        scoped_reads::*,
    };
    fn observation() -> ScopeObservation {
        ScopeObservation {
            schema: 1,
            qualifier: ScopeQualifier::SelectedSubset,
            fingerprint: Sha256Digest::from_bytes([1; 32]),
            provenance: crate::grant_provenance::GrantProvenanceKind::ImportedLegacySnapshot,
            grant: ScopeGrantObservation {
                id: GrantId::from_identity(Incarnation::from_bytes([0x84; 16])),
                revision: CatalogRevision::INITIAL,
            },
            members: vec![ScopeMemberObservation {
                index_id: IndexId::from_identity(Incarnation::from_bytes([2; 16])),
                memberships: vec![ScopeMembershipObservation {
                    project_id: QueryProjectId::from_identity(Incarnation::from_bytes([3; 16])),
                    membership_revision: CatalogRevision::INITIAL,
                }],
                source_root_ids: vec![SourceRootId::from_identity(Incarnation::from_bytes(
                    [4; 16],
                ))],
                source_revision: CatalogRevision::INITIAL,
                policy_epoch: 0,
                policy_digest: None,
            }],
        }
    }
    #[test]
    fn installed_discovery_preserves_frozen_grant_without_inventing_catalog_rows() -> Result<()> {
        let observed = observation();
        let member = &observed.members[0];
        let project = member.memberships[0].project_id;
        for value in [
            json!({"view":"projects"}),
            json!({"view":"indexes","project_id":project}),
            json!({"view":"sources","project_id":project,"index_id":member.index_id}),
            json!({"view":"recommendations"}),
        ] {
            let args: QueryScopeArgs = serde_json::from_value(value)?;
            let body = installed_body(&args, &observed)?;
            assert_eq!(body["grant"], json!(observed.grant));
            assert_eq!(body["scope_observation"], json!(observed));
            assert_eq!(
                body["observed_selection"],
                json!({"project_ids":[project],"index_ids":[member.index_id]})
            );
            assert_eq!(body["mode"], "frozen_single_index");
            assert_eq!(body["catalog_discovery"], "unavailable_on_this_connection");
            assert!(body["projects"].as_array().unwrap().is_empty());
            assert!(body.get("publication").is_none());
        }
        Ok(())
    }
    #[test]
    fn installed_discovery_refuses_foreign_selectors_continuations_and_oversize() -> Result<()> {
        let observed = observation();
        for value in [
            json!({"view":"indexes","project_id":QueryProjectId::from_identity(Incarnation::from_bytes([9;16]))}),
            json!({"view":"projects","cursor":"not-a-current-catalog-page"}),
            json!({"view":"projects","max_bytes":1}),
        ] {
            assert!(installed_body(&serde_json::from_value(value)?, &observed).is_err());
        }
        Ok(())
    }
}
