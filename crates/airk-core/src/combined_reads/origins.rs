// SPDX-License-Identifier: Apache-2.0
//! Exact member-publication references. These are bounded local identities,
//! never arbitrary JSON pointers or permission to select another publication.
use super::{COMBINED_READ_SCHEMA, MemberPublication, PublicationVector};
use crate::{
    evidence::RevisionEnvelope,
    index_catalog::{
        IndexId, MAX_SELECTION, QueryProjectId, SourceRootId, relative_path, unique_bounded,
    },
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualifiedOrigin {
    pub index_id: IndexId,
    pub source_root_id: SourceRootId,
    pub project_ids: Vec<QueryProjectId>,
    pub source_relative_path: String,
    pub publication: RevisionEnvelope,
}
impl QualifiedOrigin {
    pub fn validate(&self) -> Result<()> {
        unique_bounded(&self.project_ids, MAX_SELECTION)?;
        ensure!(
            !self.project_ids.is_empty(),
            "qualified origin has no selected Project"
        );
        relative_path(&self.source_relative_path, false)?;
        validate_publication(&self.publication)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationMemberRef {
    pub member: usize,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RankedOrigin {
    pub index_id: IndexId,
    pub source_root_id: SourceRootId,
    pub project_ids: Vec<QueryProjectId>,
    pub source_relative_path: String,
    pub publication_ref: PublicationMemberRef,
}
impl RankedOrigin {
    pub fn from_full(
        origin: QualifiedOrigin,
        member: usize,
        members: &[MemberPublication],
    ) -> Result<Self> {
        origin.validate()?;
        ensure!(
            member < MAX_SELECTION
                && members
                    .get(member)
                    .is_some_and(|entry| entry.index_id == origin.index_id
                        && entry.publication.as_ref() == Some(&origin.publication)),
            "ranked publication does not match its vector member"
        );
        Ok(Self {
            index_id: origin.index_id,
            source_root_id: origin.source_root_id,
            project_ids: origin.project_ids,
            source_relative_path: origin.source_relative_path,
            publication_ref: PublicationMemberRef { member },
        })
    }
}
fn validate_publication(publication: &RevisionEnvelope) -> Result<()> {
    ensure!(
        !publication.collection.is_empty()
            && publication.collection.len() <= 128
            && publication.incarnation == publication.token.incarnation
            && publication.generation == publication.token.generation
            && publication.revision == publication.token.revision,
        "inconsistent originating publication"
    );
    Ok(())
}
fn publication_at(
    combined: &Value,
    index: IndexId,
    reference: PublicationMemberRef,
) -> Result<RevisionEnvelope> {
    ensure!(
        combined.get("schema").and_then(Value::as_str) == Some(COMBINED_READ_SCHEMA)
            && reference.member < MAX_SELECTION,
        "invalid publication reference namespace or offset"
    );
    let members = combined
        .pointer("/publication_vector/members")
        .and_then(Value::as_array)
        .context("publication reference has no vector")?;
    ensure!(
        !members.is_empty() && members.len() <= MAX_SELECTION,
        "invalid publication vector size"
    );
    let ids = members
        .iter()
        .map(|entry| serde_json::from_value::<IndexId>(entry["index_id"].clone()))
        .collect::<Result<Vec<_>, _>>()?;
    unique_bounded(&ids, MAX_SELECTION)?;
    ensure!(
        ids.windows(2).all(|pair| pair[0] < pair[1]),
        "publication vector is not canonical"
    );
    let member = members
        .get(reference.member)
        .context("publication reference is outside the vector")?;
    ensure!(
        ids[reference.member] == index,
        "publication reference names another index"
    );
    let publication: RevisionEnvelope = serde_json::from_value(member["publication"].clone())
        .context("publication reference has no complete member identity")?;
    validate_publication(&publication)?;
    Ok(publication)
}
pub fn materialize_ranked_origin(combined: &Value, rank: &Value) -> Result<QualifiedOrigin> {
    ensure!(
        combined.get("schema").and_then(Value::as_str) == Some(COMBINED_READ_SCHEMA),
        "invalid ranked origin namespace"
    );
    let raw = rank.get("origin").context("ranked origin is missing")?;
    let origin = if raw.get("publication_ref").is_some() {
        let compact: RankedOrigin =
            serde_json::from_value(raw.clone()).context("invalid ranked origin reference")?;
        QualifiedOrigin {
            publication: publication_at(combined, compact.index_id, compact.publication_ref)?,
            index_id: compact.index_id,
            source_root_id: compact.source_root_id,
            project_ids: compact.project_ids,
            source_relative_path: compact.source_relative_path,
        }
    } else {
        serde_json::from_value::<QualifiedOrigin>(raw.clone())
            .context("invalid retained inline origin")?
    };
    origin.validate()?;
    ensure!(
        rank.get("index_id") == Some(&serde_json::to_value(origin.index_id)?),
        "ranked origin index mismatch"
    );
    if let Some(entries) = combined
        .pointer("/publication_vector/members")
        .and_then(Value::as_array)
        && let Some(entry) = entries
            .iter()
            .find(|entry| entry["index_id"] == rank["index_id"])
        && let Some(token) = entry.pointer("/publication/token").and_then(Value::as_str)
    {
        ensure!(
            token == origin.publication.token.to_string(),
            "retained inline origin differs from vector"
        );
        if let Ok(publication) =
            serde_json::from_value::<RevisionEnvelope>(entry["publication"].clone())
        {
            ensure!(
                publication == origin.publication,
                "retained inline publication envelope differs from vector"
            );
        }
    }
    Ok(origin)
}

const PUBLICATION_PATHS: &[&str] = &[
    "/publication",
    "/read/publication",
    "/read/current_publication",
    "/evidence/publication",
    "/evidence/read/publication",
    "/evidence/read/current_publication",
];

pub fn factor_member_publications(
    body: &mut Value,
    index: IndexId,
    member: usize,
    vector: &PublicationVector,
) -> Result<()> {
    let expected = vector
        .members
        .get(member)
        .context("native publication vector member is missing")?;
    ensure!(
        expected.index_id == index && member < MAX_SELECTION,
        "native publication vector index mismatch"
    );
    let Some(publication) = &expected.publication else {
        return Ok(());
    };
    for path in PUBLICATION_PATHS {
        if let Some(value) = body.pointer_mut(path)
            && serde_json::from_value::<RevisionEnvelope>(value.clone())
                .as_ref()
                .ok()
                == Some(publication)
        {
            *value = serde_json::to_value(PublicationMemberRef { member })?;
        }
    }
    Ok(())
}
pub fn materialize_member_publications(
    combined: &Value,
    index: IndexId,
    body: &mut Value,
) -> Result<()> {
    for path in PUBLICATION_PATHS {
        if let Some(value) = body.pointer_mut(path)
            && value.get("member").is_some()
        {
            let reference: PublicationMemberRef = serde_json::from_value(value.clone())
                .context("invalid native publication reference")?;
            *value = serde_json::to_value(publication_at(combined, index, reference)?)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        combined_reads::MemberPublication,
        identity::{Incarnation, RevisionToken, Sha256Digest},
    };
    use serde_json::json;
    fn index(n: u8) -> IndexId {
        IndexId::from_identity(Incarnation::from_bytes([n; 16]))
    }
    fn publication(n: u8) -> RevisionEnvelope {
        RevisionEnvelope::new(
            format!("fixture{n}"),
            RevisionToken {
                incarnation: Incarnation::from_bytes([n; 16]),
                generation: 2,
                revision: Sha256Digest::from_bytes([n; 32]),
            },
        )
    }
    fn vector() -> PublicationVector {
        PublicationVector {
            authority: Sha256Digest::from_bytes([1; 32]),
            observed_from_unix_ms: 1,
            observed_to_unix_ms: 2,
            members: vec![
                MemberPublication {
                    index_id: index(1),
                    publication: Some(publication(1)),
                },
                MemberPublication {
                    index_id: index(2),
                    publication: Some(publication(2)),
                },
            ],
        }
    }
    fn origin() -> QualifiedOrigin {
        QualifiedOrigin {
            index_id: index(1),
            source_root_id: SourceRootId::from_identity(Incarnation::from_bytes([1; 16])),
            project_ids: vec![QueryProjectId::from_identity(Incarnation::from_bytes(
                [1; 16],
            ))],
            source_relative_path: "api/one.rs".into(),
            publication: publication(1),
        }
    }
    #[test]
    fn ranked_and_native_publications_roundtrip_exactly_with_retained_inline_support() -> Result<()>
    {
        let vector = vector();
        let full = origin();
        let compact = RankedOrigin::from_full(full.clone(), 0, &vector.members)?;
        let mut root = json!({"schema":COMBINED_READ_SCHEMA,"publication_vector":vector});
        let rank = json!({"index_id":index(1),"origin":compact});
        assert_eq!(materialize_ranked_origin(&root, &rank)?, full);
        assert_eq!(
            materialize_ranked_origin(&root, &json!({"index_id":index(1),"origin":full}))?,
            full
        );
        let original = json!({"publication":publication(1),"read":{"publication":publication(1),"current_publication":publication(2)},
            "evidence":{"publication":publication(1),"hits":[{"text":"publication source text stays inline"}]}});
        let mut body = original.clone();
        factor_member_publications(&mut body, index(1), 0, &vector)?;
        assert_eq!(body["publication"], json!({"member":0}));
        assert_eq!(
            body["read"]["current_publication"],
            serde_json::to_value(publication(2))?
        );
        materialize_member_publications(&root, index(1), &mut body)?;
        assert_eq!(body, original);
        root.as_object_mut().unwrap().remove("publication_vector");
        assert_eq!(
            materialize_ranked_origin(&root, &json!({"index_id":index(1),"origin":full}))?,
            full
        );
        Ok(())
    }
    #[test]
    fn publication_references_refuse_missing_foreign_out_of_range_and_mixed_identities()
    -> Result<()> {
        let full = origin();
        let compact = RankedOrigin::from_full(full.clone(), 0, &vector().members)?;
        let root = json!({"schema":COMBINED_READ_SCHEMA,"publication_vector":vector()});
        let rank = json!({"index_id":index(1),"origin":compact});
        for offset in [1, 2, 32, usize::MAX] {
            let mut bad = rank.clone();
            bad["origin"]["publication_ref"]["member"] = json!(offset);
            assert!(materialize_ranked_origin(&root, &bad).is_err());
        }
        let mut bad = rank.clone();
        bad["origin"]["publication"] = serde_json::to_value(publication(1))?;
        assert!(materialize_ranked_origin(&root, &bad).is_err());
        let mut badroot = root.clone();
        badroot
            .as_object_mut()
            .unwrap()
            .remove("publication_vector");
        assert!(materialize_ranked_origin(&badroot, &rank).is_err());
        let mut bad = rank.clone();
        bad["index_id"] = json!(index(2));
        assert!(materialize_ranked_origin(&root, &bad).is_err());
        let mut inline = full.clone();
        inline.publication.collection = "foreign".into();
        assert!(
            materialize_ranked_origin(&root, &json!({"index_id":index(1),"origin":inline}))
                .is_err()
        );
        let mut body = json!({"publication":{"member":0,"token":"mixed identity"}});
        assert!(materialize_member_publications(&root, index(1), &mut body).is_err());
        let mut body = json!({"publication":{"member":0}});
        assert!(materialize_member_publications(&root, index(2), &mut body).is_err());
        Ok(())
    }
}
