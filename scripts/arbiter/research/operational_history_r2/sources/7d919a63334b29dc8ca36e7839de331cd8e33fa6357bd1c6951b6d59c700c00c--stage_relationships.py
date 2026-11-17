# SPDX-License-Identifier: Apache-2.0
"""Stage exact bounded-traversal observations without a graph cursor redesign."""
from pathlib import Path
import difflib
ROOT=Path(__file__).resolve().parents[3];OUT=Path(__file__).resolve().parent;changes={}
def edit(name,before,after):
    text=changes.get(name,(ROOT/name).read_text(encoding="utf-8"));assert before in text,(name,before[:100]);changes[name]=text.replace(before,after,1)
def append(name,text):changes[name]=changes.get(name,(ROOT/name).read_text(encoding="utf-8"))+text

edit("crates/airk-core/src/code.rs", '''pub struct RelationshipsResponse {
''', '''pub struct RelationshipsResponse {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub truncation_reasons: Vec<RelationshipTruncationReason>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<RelationshipBounds>,
''')
append("crates/airk-core/src/code.rs", '''

/// Actual stops observed by bounded graph traversal. Navigation incompleteness
/// can reflect ambiguous or omitted retained declarations, not only a budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipTruncationReason {
    ResultLimit, ReplyBytes, CandidateFiles, Symbols, RetainedEvidenceBytes,
    TypeNavigationIncomplete, Deadline,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelationshipBounds {
    pub max_results: usize,
    pub max_record_bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_candidate_files: Option<usize>,
    pub max_symbols: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_retained_evidence_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_navigation_evidence_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_navigation_facts: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deadline_ms: Option<u64>,
}
impl RelationshipQuery {
    pub fn validate_direction(&self) -> anyhow::Result<()> {
        if !matches!(self.direction.as_str(), "incoming" | "outgoing") {
            return Err(crate::read_arguments::ReadArgumentError::new("invalid_arguments","direction",
                "direction must be incoming or outgoing").into());
        }
        Ok(())
    }
}
impl RelationshipsResponse {
    pub fn mark_truncated(&mut self, reason: RelationshipTruncationReason) {
        self.truncated=true;
        if !self.truncation_reasons.contains(&reason) {self.truncation_reasons.push(reason);}
    }
}
''')

edit("crates/airk-store/src/current/relationships.rs", '''use airk_core::{Artifact, EvidenceBasis, RelationshipHit, RelationshipQuery};''', '''use airk_core::{Artifact, EvidenceBasis, RelationshipHit, RelationshipQuery};
use airk_core::code::{RelationshipBounds, RelationshipTruncationReason as Stop};''')
edit("crates/airk-store/src/current/relationships.rs", '''pub struct CurrentRelationships {
''', '''pub struct CurrentRelationships {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub truncation_reasons: Vec<Stop>,
    pub bounds: RelationshipBounds,
''')
edit("crates/airk-store/src/current/relationships.rs", '''pub(crate) fn visit_edges(
''', '''impl CurrentRelationships {
    fn mark_truncated(&mut self, reason: Stop) {
        self.truncated=true;
        if !self.truncation_reasons.contains(&reason) {self.truncation_reasons.push(reason);}
    }
}

pub(crate) fn visit_edges(
''')
edit("crates/airk-store/src/current/relationships.rs", '''        airk_core::validate_budget(query.limit, query.max_bytes)?;
''', '''        query.validate_direction()?;
        airk_core::validate_budget(query.limit, query.max_bytes)?;
''')
edit("crates/airk-store/src/current/relationships.rs", '''        let mut response = CurrentRelationships {
''', '''        let mut response = CurrentRelationships {
            truncation_reasons:Vec::new(),
            bounds:RelationshipBounds{max_results:query.limit,max_record_bytes:query.max_bytes,
                max_candidate_files:Some(MAX_FILES),max_symbols:MAX_SYMBOLS,
                max_retained_evidence_bytes:Some(MAX_EVIDENCE_BYTES),max_navigation_evidence_bytes:Some(MAX_NAVIGATION_EVIDENCE_BYTES),
                max_navigation_facts:Some(navigation::MAX_FACTS),deadline_ms:None},
''')
edit("crates/airk-store/src/current/relationships.rs", '''        response.truncated |= navigation.truncated;''', '''        if navigation.truncated {response.mark_truncated(Stop::TypeNavigationIncomplete);}''')
edit("crates/airk-store/src/current/relationships.rs", '''                if visited.len() == MAX_SYMBOLS || files == MAX_FILES {
                    response.truncated = true;''', '''                if visited.len() == MAX_SYMBOLS || files == MAX_FILES {
                    if visited.len()==MAX_SYMBOLS {response.mark_truncated(Stop::Symbols);}
                    if files==MAX_FILES {response.mark_truncated(Stop::CandidateFiles);}''')
edit("crates/airk-store/src/current/relationships.rs", '''                response.truncated |= page.next_after.is_some();''', '''                if page.next_after.is_some() {response.mark_truncated(Stop::CandidateFiles);}''')
edit("crates/airk-store/src/current/relationships.rs", '''                if paths.len() > MAX_FILES - files {
                    paths.truncate(MAX_FILES - files);
                    response.truncated = true;''', '''                if paths.len() > MAX_FILES - files {
                    paths.truncate(MAX_FILES - files);
                    response.mark_truncated(Stop::CandidateFiles);''')
edit("crates/airk-store/src/current/relationships.rs", '''                    if bytes > MAX_EVIDENCE_BYTES.saturating_sub(work) {
                        response.truncated = true;''', '''                    if bytes > MAX_EVIDENCE_BYTES.saturating_sub(work) {
                        response.mark_truncated(Stop::RetainedEvidenceBytes);''')
edit("crates/airk-store/src/current/relationships.rs", '''                        if response.relationships.len() == query.limit || encoded.len() > remaining
                        {
                            response.truncated = true;''', '''                        if response.relationships.len() == query.limit || encoded.len() > remaining
                        {
                            if response.relationships.len()==query.limit {response.mark_truncated(Stop::ResultLimit);}
                            if encoded.len()>remaining {response.mark_truncated(Stop::ReplyBytes);}''')
edit("crates/airk-store/src/current/relationships.rs", '''                            if next.len() == MAX_SYMBOLS {
                                response.truncated = true;''', '''                            if next.len() == MAX_SYMBOLS {
                                response.mark_truncated(Stop::Symbols);''')
edit("crates/airk-store/src/current/relationships.rs", '''            if response.relationships.len() + response.suggested_targets.len() == query.limit
                || bytes > remaining
            {
                response.truncated = true;''', '''            if response.relationships.len() + response.suggested_targets.len() == query.limit
                || bytes > remaining
            {
                if response.relationships.len()+response.suggested_targets.len()==query.limit {response.mark_truncated(Stop::ResultLimit);}
                if bytes>remaining {response.mark_truncated(Stop::ReplyBytes);}''')

edit("crates/airk-kernel/src/lib.rs", '''    ) -> Result<airk_core::RelationshipsResponse> {
        let read = self.current_reader()?;''', '''    ) -> Result<airk_core::RelationshipsResponse> {
        query.validate_direction()?;
        let read = self.current_reader()?;''')
edit("crates/airk-kernel/src/lib.rs", '''        Ok(airk_core::RelationshipsResponse {
''', '''        Ok(airk_core::RelationshipsResponse {
            truncation_reasons:result.truncation_reasons,
            bounds:Some(result.bounds),
''')

edit("crates/airk-store/src/sqlite/queries.rs", '''    ) -> Result<RelationshipsResponse> {
        let _lifecycle = self.lifecycle(false)?;''', '''    ) -> Result<RelationshipsResponse> {
        q.validate_direction()?;
        let _lifecycle = self.lifecycle(false)?;''')
edit("crates/airk-store/src/sqlite/queries.rs", '''relationships:Vec::new(),suggested_targets:Vec::new(),truncated:false,coverage:''', '''relationships:Vec::new(),suggested_targets:Vec::new(),truncation_reasons:Vec::new(),bounds:Some(airk_core::code::RelationshipBounds{max_results:q.limit,max_record_bytes:q.max_bytes,max_candidate_files:None,max_symbols:100,max_retained_evidence_bytes:None,max_navigation_evidence_bytes:None,max_navigation_facts:None,deadline_ms:Some(250)}),truncated:false,coverage:''')
edit("crates/airk-store/src/sqlite/queries.rs", '''                if visited.len() > 100 || Instant::now() > deadline {
                    response.truncated = true;''', '''                if visited.len() > 100 || Instant::now() > deadline {
                    if visited.len()>100 {response.mark_truncated(airk_core::code::RelationshipTruncationReason::Symbols);}
                    if Instant::now()>deadline {response.mark_truncated(airk_core::code::RelationshipTruncationReason::Deadline);}''')
edit("crates/airk-store/src/sqlite/queries.rs", '''                        response.truncated = true;
                        return Ok(response);
                    }
                    let edge: RelationshipHit''', '''                        if response.relationships.len()==q.limit {response.mark_truncated(airk_core::code::RelationshipTruncationReason::ResultLimit);}
                        if payload.len()>remaining {response.mark_truncated(airk_core::code::RelationshipTruncationReason::ReplyBytes);}
                        if Instant::now()>deadline {response.mark_truncated(airk_core::code::RelationshipTruncationReason::Deadline);}
                        return Ok(response);
                    }
                    let edge: RelationshipHit''')

append("crates/airk-store/src/current/postings_tests.rs", '''

#[test]
fn native_relationships_report_actual_result_and_record_budgets() -> Result<()> {
    use airk_core::code::RelationshipTruncationReason as Stop;
    let fixture=Fixture::new()?;
    let writer=fixture.store.lock()?;
    let mut current=fixture.store.current_write(&writer,&fixture.quota,&fixture.scope,CAPACITY)?;
    let mut token=fixture.head.token()?;
    for path in ["one.txt","two.txt"] {
        let input=retained_input_with_facts(&fixture,path,"body",vec![written_call("target")])?;
        token=current.commit_artifact(&token,path,Some(&input))?.token;
    }
    drop(current);drop(writer);
    let read=fixture.store.current_read(&fixture.scope)?;
    let mut q=airk_core::RelationshipQuery{symbol:"target".into(),direction:"incoming".into(),kind:Some("call".into()),
        path_prefix:String::new(),hops:1,limit:1,max_bytes:65536};
    let limited=read.native_relationships(&fixture.store,&q)?;
    assert!(limited.truncated);assert_eq!(limited.relationships.len(),1);
    assert!(limited.truncation_reasons.contains(&Stop::ResultLimit));
    assert_eq!(limited.bounds.max_results,1);assert_eq!(limited.bounds.max_candidate_files,Some(256));
    q.limit=8;q.max_bytes=1;
    let bytes=read.native_relationships(&fixture.store,&q)?;
    assert!(bytes.truncated && bytes.relationships.is_empty());
    assert!(bytes.truncation_reasons.contains(&Stop::ReplyBytes));
    assert!(!bytes.truncation_reasons.contains(&Stop::ResultLimit));
    q.direction="sideways".into();
    let error=read.native_relationships(&fixture.store,&q).unwrap_err();
    assert_eq!(error.downcast_ref::<airk_core::read_arguments::ReadArgumentError>().unwrap().field,"direction");
    Ok(())
}
''')
edit("crates/airk-cli/tests/combined_reads.rs", '''        (
            "airk_changes",
            json!({"selectors":[{"kind":"path","path":"api/types.rs"}],"limit":4,"max_bytes":4096}),''', '''        ("airk_relationships",json!({"symbol":"file:api/types.rs","direction":"outgoing","limit":1,"max_bytes":4096}),true),
        (
            "airk_changes",
            json!({"selectors":[{"kind":"path","path":"api/types.rs"}],"limit":4,"max_bytes":4096}),''')
edit("crates/airk-mcp/src/scoped_observation_tests.rs", '''    let binding = airk_core::service::ProjectRegistration::new(
''', '''    let relationship_args=json!({"symbol":"file:public/a.rs","direction":"outgoing","limit":4,"max_bytes":4096});
    let mut wire_args=relationship_args.as_object().unwrap().clone();
    wire_args.insert("verification".into(),json!("off"));wire_args.insert("response_mode".into(),json!("full"));
    let actual=scoped.call("airk_relationships".into(),wire_args).await?;
    let relationship_body=payload(&actual)?;
    assert!(relationship_body["bounds"].is_object(),"{relationship_body}");
    let plan=json!({"schema":"airk.parse.request/1","mode":"execute","verification":"off",
        "plan":{"steps":[{"id":"edges","tool":"relationships","args":relationship_args}]},"output":{"view":"raw","max_bytes":32768}});
    let plan=native.evidence_plan(serde_json::from_value(plan)?)?;
    assert_eq!(plan.steps[0].result.as_ref().unwrap()["bounds"],relationship_body["bounds"]);
    assert_eq!(plan.steps[0].result.as_ref().unwrap()["truncation_reasons"],relationship_body["truncation_reasons"]);
    let binding = airk_core::service::ProjectRegistration::new(
''')

for name,value in changes.items():
    target=OUT/"relationships-proposed"/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_text(value,encoding="utf-8",newline="\n")
patch="".join("".join(difflib.unified_diff((ROOT/name).read_text(encoding="utf-8").splitlines(True),value.splitlines(True),fromfile="a/"+name,tofile="b/"+name)) for name,value in changes.items())
(OUT/"relationship-limits.patch").write_text(patch,encoding="utf-8",newline="\n")
print(f"Staged relationship observations in {len(changes)} files; production untouched")
