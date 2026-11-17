# SPDX-License-Identifier: Apache-2.0
"""Stage open-filter diagnostics and bounded relationship disclosures."""
from pathlib import Path
import difflib
import json
import hashlib

ROOT=Path(__file__).resolve().parents[3]
OUT=Path(__file__).resolve().parent
changes={}
def edit(name,before,after):
    text=changes.get(name,(ROOT/name).read_text(encoding="utf-8"));assert before in text,(name,before[:80]);changes[name]=text.replace(before,after,1)
def append(name,text):changes[name]=changes.get(name,(ROOT/name).read_text(encoding="utf-8"))+text

edit("crates/airk-core/src/discovery.rs", "pub struct DiscoveryPage {\n", '''pub struct DiscoveryPage {
    /// Qualified observations about open exact labels, not a global catalog or
    /// a claim that an unobserved producer label is unsupported.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filter_observations: Vec<FilterObservation>,
''')
append("crates/airk-core/src/discovery.rs", '''

/// Open producer labels remain valid inputs. Empty complete queries disclose
/// their exact filters instead of silently making spelling mistakes look useful.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilterObservation {
    pub field: String,
    pub value: String,
    pub state: String,
    pub basis: String,
    pub message: String,
}
impl FilterObservation {
    pub fn no_matching_records(field: &str, value: &str) -> Self {
        Self {field:field.into(),value:value.into(),state:"not_observed_in_selected_query".into(),
            basis:"complete_selected_published_query".into(),
            message:"This exact filter produced no matching retained records in the selected index, prefix and other query filters. Check its spelling and file_info language/profile metadata. A valid absent native or retained producer label is not globally unsupported.".into()}
    }
}
''')

# Exact language postings are authoritative metadata selectors; filtering them
# first avoids scanning definition postings for an absent language label.
edit("crates/airk-store/src/current/discovery.rs", '''        let lookup = if let Some(name) = query.name.as_deref().filter(|_| exact_name) {''', '''        let lookup = if let Some(language) = query.language.as_deref() {
            Lookup::Language(language)
        } else if let Some(name) = query.name.as_deref().filter(|_| exact_name) {''')
edit("crates/airk-store/src/current/discovery.rs", '''        airk_core::validate_budget(query.limit, query.max_bytes)?;
''', '''        airk_core::validate_budget(query.limit, query.max_bytes)?;
        for (field, value) in [("kind", query.kind.as_deref()), ("language", query.language.as_deref())] {
            if value.is_some_and(|value| value.is_empty() || value.len() > 128) {
                return Err(airk_core::read_arguments::ReadArgumentError::new("invalid_filter",field,
                    format!("{field} must contain 1..128 UTF-8 bytes; native and retained producer labels are matched exactly")).into());
            }
        }
''')
edit("crates/airk-store/src/current/discovery.rs", '''                    Lookup::Files,
                    &query.path_prefix,''', '''                    query.language.as_deref().map_or(Lookup::Files, Lookup::Language),
                    &query.path_prefix,''')
edit("crates/airk-store/src/current/discovery.rs", '''            paths.extend(
                imported_facts
                    .keys()
                    .filter(|path| {
                        path.starts_with(&query.path_prefix)
                            && after.as_ref().is_none_or(|key| *path > &key.path)
                    })
                    .cloned(),
            );''', '''            for path in imported_facts.keys().filter(|path| {
                path.starts_with(&query.path_prefix)
                    && after.as_ref().is_none_or(|key| *path > &key.path)
            }) {
                let language_matches = match query.language.as_deref() {
                    None => true,
                    Some(language) => self.artifact(path)?.is_some_and(|artifact| artifact.language == language),
                };
                if language_matches { paths.push(path.clone()); }
            }''')
edit("crates/airk-store/src/current/discovery.rs", '''        if fact_options.include_count {
            let complete = after.is_none() && !response.truncated;''', '''        if after.is_none() && !response.truncated && matched == 0 {
            for (field, value) in [("kind", query.kind.as_deref()), ("language", query.language.as_deref())] {
                if let Some(value) = value {
                    response.page.filter_observations.push(airk_core::discovery::FilterObservation::no_matching_records(field, value));
                }
            }
        }
        if fact_options.include_count {
            let complete = after.is_none() && !response.truncated;''')
edit("crates/airk-store/src/current/discovery.rs", '''            let retained = compact
                .retained
                .as_ref()
                .context("current fact extraction missing")?;''', '''            if query.language.as_ref().is_some_and(|language| language != &compact.language) {
                last=Some(DiscoveryKey {path,ordinal:FILE_END,..Default::default()});
                continue;
            }
            let retained = compact
                .retained
                .as_ref()
                .context("current fact extraction missing")?;''')

edit("crates/airk-core/src/guidance.rs", '''    let page = get("page");''', '''    let page = get("page");
    if get("filter_observations").or_else(|| page.and_then(|value| value.get("filter_observations")))
        .and_then(Value::as_array).is_some_and(|rows| !rows.is_empty()) {
        advice.add(Reason::Unknown,"requested_filters","filter_observations",
            "The exact filter labels were not observed in this complete selected query. Check their spelling and file_info language/profile labels. Valid absent native and retained producer labels remain valid; this is not a global unsupported-language finding.",NextRead::InspectFileInfo);
    }
    if get("truncation_reasons").and_then(Value::as_array).is_some_and(|rows| !rows.is_empty()) {
        advice.add(Reason::Truncated,"relationship_traversal","truncation_reasons/bounds",
            "This bounded relationship traversal reports the actual limits or incomplete navigation it observed. There is no graph continuation cursor. Narrow path_prefix or the exact written target, reduce hops, or explicitly increase result/byte budgets within the reported bounds. Missing edges do not prove absence.",NextRead::ReadSelectedPassage);
    }''')

# Store-level focused evidence exercises open custom labels through real retained
# inputs and ensures an unrelated absent language never opens retained objects.
edit("crates/airk-store/src/current/tests.rs", '''    use airk_core::extraction::{ExtractionPayload, ExtractionProfile, LocalChunk, LocalSpan};
    use std::collections::BTreeMap;
    let profile = ExtractionProfile {''', '''    retained_input_with_facts_language_budget(fixture, path, text, facts, capture_bytes, "text")
}

fn retained_input_with_facts_language_budget(
    fixture: &Fixture, path: &str, text: &str,
    facts: Vec<airk_core::extraction::LocalFact>, capture_bytes: u64, language: &str,
) -> Result<CapturedInput> {
    use airk_core::extraction::{ExtractionPayload, ExtractionProfile, LocalChunk, LocalSpan};
    use std::collections::BTreeMap;
    let profile = ExtractionProfile {''')
edit("crates/airk-store/src/current/tests.rs", '''        adapter_version: "current-test/1".into(),
        language: "text".into(),''', '''        adapter_version: "current-test/1".into(),
        language: language.into(),''')
append("crates/airk-store/src/current/postings_tests.rs", '''

#[test]
fn open_fact_filters_report_absence_and_preserve_retained_custom_labels() -> Result<()> {
    let fixture=Fixture::new()?;
    let input=retained_input_with_facts_language_budget(&fixture,"custom.txt","body",
        vec![fact("Custom","producer_marker")],1024*1024,"producer_language")?;
    let writer=fixture.store.lock()?;
    let mut current=fixture.store.current_write(&writer,&fixture.quota,&fixture.scope,CAPACITY)?;
    current.commit_artifact(&fixture.head.token()?,&input.artifact.path,Some(&input))?;
    drop(current);drop(writer);
    let read=fixture.store.current_read(&fixture.scope)?;
    let mut q=query();q.limit=10;q.kind=Some("producer_marker".into());q.language=Some("producer_language".into());
    let valid=read.native_facts_page(&fixture.store,&q,false,&Default::default())?;
    assert_eq!(valid.facts.len(),1);assert!(valid.page.filter_observations.is_empty());
    for (kind,language,field) in [(Some("bogus_kind"),Some("producer_language"),"kind"),
        (Some("definition"),Some("klingon"),"language"),(Some("definition"),None,"kind")] {
        q.kind=kind.map(str::to_owned);q.language=language.map(str::to_owned);
        let empty=read.native_facts_page(&fixture.store,&q,false,&Default::default())?;
        assert!(empty.facts.is_empty() && !empty.truncated && empty.page.cursor.is_none());
        assert!(empty.page.filter_observations.iter().any(|item| item.field==field && item.state=="not_observed_in_selected_query"));
        assert!(empty.page.filter_observations.iter().all(|item| item.message.contains("not globally unsupported")));
    }
    let object=fixture.store.directory().join("objects/artifacts").join(&input.artifact.retained.as_ref().unwrap().object);
    fs::write(&object,"corrupt unrelated retained fixture")?;
    q.kind=Some("producer_marker".into());q.language=Some("klingon".into());
    let absent=read.native_facts_page(&fixture.store,&q,false,&Default::default())?;
    assert!(absent.facts.is_empty() && absent.page.cursor.is_none() && !absent.truncated);
    q.language=Some("producer_language".into());
    assert!(read.native_facts_page(&fixture.store,&q,false,&Default::default()).is_err(),"matching sources must retain integrity checks");
    Ok(())
}
''')

edit("crates/airk-cli/tests/combined_reads.rs", '''        (
            "airk_references",
            json!({"name":"boundaryneedle","limit":4,"max_bytes":4096}),''', '''        ("airk_symbols",json!({"language":"klingon","limit":4,"max_bytes":4096}),false),
        ("airk_code_facts",json!({"kind":"bogus_kind","limit":4,"max_bytes":4096}),false),
        ("airk_document_facts",json!({"kind":"bogus_kind","limit":4,"max_bytes":4096}),false),
        (
            "airk_references",
            json!({"name":"boundaryneedle","limit":4,"max_bytes":4096}),''')
edit("crates/airk-mcp/src/scoped_observation_tests.rs", '''    let help = scoped
''', '''    for (tool, args) in [("symbols",json!({"language":"klingon"})),("code_facts",json!({"kind":"bogus_kind"})),
        ("document_facts",json!({"kind":"bogus_kind"}))] {
        let mut wire_args=args.as_object().unwrap().clone();wire_args.insert("verification".into(),json!("off"));
        wire_args.insert("response_mode".into(),json!("full"));
        let actual=scoped.call(format!("airk_{tool}"),wire_args).await?;
        let body=payload(&actual)?;
        assert_ne!(actual.is_error,Some(true),"{body}");
        assert!(body["filter_observations"].as_array().is_some_and(|rows| !rows.is_empty()),"{body}");
        assert!(body.get("cursor").is_none());
        let plan=json!({"schema":"airk.parse.request/1","mode":"execute","verification":"off",
            "plan":{"steps":[{"id":"filtered","tool":tool,"args":args}]},"output":{"view":"raw","max_bytes":32768}});
        let direct=native.evidence_plan(serde_json::from_value(plan)?)?;
        assert_eq!(direct.steps[0].result.as_ref().unwrap()["filter_observations"],body["filter_observations"]);
    }
    let help = scoped
''')
append("crates/airk-cli/tests/combined_reads.rs", '''

#[tokio::test]
async fn exposed_code_facts_preserve_retained_producer_kind_and_language_labels() -> Result<()> {
    let owner=tempfile::tempdir()?;let root=owner.path().join("source");fs::create_dir(&root)?;
    let source="pub struct ProducerRecord;\\n";fs::write(root.join("producer.rs"),source)?;
    let kernel=airk_kernel::Kernel::open(&root,Some(owner.path().join("data")))?;
    kernel.configure_vectors(false)?;kernel.index()?;
    let store=airk_store::FileStore::new(kernel.data_dir().join("collections-1.2").join(kernel.collection()));
    let scope=store.current_scope(kernel.root().to_str().unwrap(),kernel.collection())?.context("current fixture scope")?;
    let read=store.current_read(&scope)?;
    let mut artifact=store.load_retained(&read.artifact("producer.rs")?.context("retained fixture")?)?;
    drop(read);
    ensure!(!artifact.facts.is_empty(),"fixture native declaration missing");
    for fact in &mut artifact.facts {fact.kind="producer_marker".into();}
    artifact.language="producer_language".into();artifact.retained=None;
    let quota=airk_store::quota::Quota::for_store(&store)?;
    let reservation=quota.reserve(kernel.collection(),4*1024*1024)?;
    let writer=store.lock()?;
    let retained=store.retain_artifact_with_budget_owned(&writer,&artifact,Some(source.as_bytes()),Some(&reservation))?;
    let capacity=store.current_database_capacity(&scope)?;
    let mut current=store.current_write(&writer,&quota,&scope,capacity)?;
    let token=current.head()?.token()?;
    current.commit_artifact(&token,"producer.rs",Some(&airk_store::current::CapturedInput::new(retained)))?;
    drop(current);drop(writer);drop(reservation);
    let args=json!({"kind":"producer_marker","language":"producer_language","compact":false,
        "verification":"off","response_mode":"full","limit":10,"max_bytes":16384});
    let mut tools=airk_mcp::local::LocalTools::new(kernel.clone()).await?;
    let reply=tools.call("airk_code_facts".into(),args.as_object().unwrap().clone()).await?;
    let actual:Value=serde_json::from_str(&reply.content[0].as_text().context("MCP JSON body")?.text)?;
    assert_ne!(reply.is_error,Some(true),"{actual}");
    assert!(actual["items"].as_array().is_some_and(|rows| !rows.is_empty()),"{actual}");
    assert!(actual["items"].as_array().unwrap().iter().all(|row| row["kind"]=="producer_marker"));
    assert!(actual.get("filter_observations").is_none());
    tools.close().await?;
    assert_eq!(fs::read_to_string(root.join("producer.rs"))?,source);
    Ok(())
}
''')

append("crates/airk-core/src/guidance.rs", '''

#[cfg(test)]
mod query_limit_guidance_tests {
    use super::*;
    #[test]
    fn qualified_filter_and_relationship_limits_remain_visible() {
        let mut value=serde_json::json!({"items":[],"filter_observations":[{"field":"language","value":"future_label"}],
            "truncated":true,"truncation_reasons":["reply_bytes"],"bounds":{"max_record_bytes":16}});
        annotate(&mut value);
        let guidance=value["guidance"]["observations"].as_array().unwrap();
        assert!(guidance.iter().any(|item| item["subject"]=="requested_filters"));
        assert!(guidance.iter().any(|item| item["subject"]=="relationship_traversal" && item["message"].as_str().unwrap().contains("no graph continuation")));
        assert_eq!(value["filter_observations"][0]["value"],"future_label");
        assert_eq!(value["truncation_reasons"][0],"reply_bytes");
    }
}
''')

for name,value in changes.items():
    target=OUT/"filter-proposed"/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_text(value,encoding="utf-8",newline="\n")
patch="".join("".join(difflib.unified_diff((ROOT/name).read_text(encoding="utf-8").splitlines(True),value.splitlines(True),fromfile="a/"+name,tofile="b/"+name)) for name,value in changes.items())
(OUT/"open-filter-diagnostics.patch").write_text(patch,encoding="utf-8",newline="\n")
print(f"Staged filter diagnostics in {len(changes)} files; production untouched")
