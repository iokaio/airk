// SPDX-License-Identifier: Apache-2.0
//! Private publisher TRAIN/CAL preparation and complete-readout raw experiments.
//! None of these stages admits a production task or supplies human action gold.
use super::NativeInferenceOwner;
use airk_core::{
    arbiter::{Asset, ModelChoice, Profile},
    identity::{Sha256Digest, framed_digest},
    native_context_action::NativeContextActionRequest,
};
use airk_retrieval::arbiter::{
    context_action,
    runtime::{AssetSource, ExecutionProfile, Scorer},
};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::Instant,
};

const FACTS_SHA: &str = "115f9baacef8d9cac1c9602447a854a11787ed759e2f61897eb01009ae6d4254";
const PROFILES_SHA: &str = "ae0f2bd9f19e374190cf0afdb2b86100f7d53933d0654801c1f812be5e616474";

#[derive(Clone)]
struct ParagraphSpan {
    id: String,
    start: usize,
    end: usize,
}
fn covers(intervals: &[(usize, usize)], start: usize, end: usize) -> bool {
    let mut ordered = intervals.to_vec();
    ordered.sort_unstable();
    let mut cursor = start;
    for (left, right) in ordered {
        if left > cursor {
            break;
        }
        if right > cursor {
            cursor = right;
        }
        if cursor >= end {
            return true;
        }
    }
    false
}
fn mapped_hit(hit: &airk_core::Hit, source: &str) -> Result<Option<(usize, usize)>> {
    ensure!(
        hit.citation.path == "source.txt",
        "foreign publisher source path"
    );
    if hit.text.is_empty() {
        return Ok(None);
    }
    let mut matches = source.match_indices(&hit.text);
    let Some((start, _)) = matches.next() else {
        return Ok(None);
    };
    if matches.next().is_some() {
        return Ok(None);
    }
    Ok(Some((start, start + hit.text.len())))
}
fn intervals(body: &Value, source: &str) -> Result<Vec<(usize, usize)>> {
    let mut spans = Vec::new();
    if let Some(hits) = body["hits"].as_array() {
        for value in hits {
            let hit: airk_core::Hit = serde_json::from_value(value.clone())?;
            if let Some(span) = mapped_hit(&hit, source)? {
                spans.push(span);
            }
        }
    }
    Ok(spans)
}
fn paragraph_ids<'a>(spans: &'a [ParagraphSpan], ranges: &[(usize, usize)]) -> BTreeSet<&'a str> {
    spans
        .iter()
        .filter(|span| covers(ranges, span.start, span.end))
        .map(|span| span.id.as_str())
        .collect()
}
fn source_alignment(
    row: &Value,
    question: &Value,
    facts: &Value,
    request: &NativeContextActionRequest,
) -> Result<Value> {
    let source_path = PathBuf::from(text(&row["source"], "source_root")?).join("source.txt");
    let bytes = pinned(
        &source_path,
        text(&row["source"], "source_sha256")?.parse()?,
    )?;
    let source = std::str::from_utf8(&bytes)?;
    let documents: BTreeMap<_, _> = array(facts, "documents")?
        .iter()
        .map(|d| Ok((text(d, "id")?, d)))
        .collect::<Result<_>>()?;
    let mut spans = Vec::new();
    let mut previous_end = 0;
    let mut span_ids = BTreeSet::new();
    for value in array(&row["source"], "spans")? {
        let id = text(value, "document")?;
        let document = documents
            .get(id)
            .context("unknown publisher source paragraph")?;
        ensure!(span_ids.insert(id), "duplicate publisher paragraph span");
        let start = usize::try_from(value["start"].as_u64().context("source start")?)?;
        let end = usize::try_from(value["end"].as_u64().context("source end")?)?;
        ensure!(
            start >= previous_end
                && start < end
                && source.get(start..end) == Some(text(document, "text")?)
                && document["source_group"] == question["source_group"],
            "source span/family changed"
        );
        previous_end = end;
        spans.push(ParagraphSpan {
            id: id.into(),
            start,
            end,
        });
    }
    ensure!(
        span_ids
            == documents
                .iter()
                .filter(|(_, document)| document["source_group"] == question["source_group"])
                .map(|(id, _)| *id)
                .collect(),
        "publisher paragraph span roster changed"
    );
    let mut shown = Vec::new();
    let mut unmapped_units = 0;
    for unit in &request.roster.basis.source_units_presented {
        unit.validate(&request.records, &request.scope)?;
        let record = request
            .records
            .iter()
            .find(|record| record.id == unit.record)
            .context("source record")?;
        let hit: airk_core::Hit = serde_json::from_value(
            record
                .canonical_result
                .pointer(&unit.result_pointer)
                .context("native source unit")?
                .clone(),
        )?;
        if let Some((start, _)) = mapped_hit(&hit, source)? {
            shown.push((start + unit.utf8_start, start + unit.utf8_end));
        } else {
            unmapped_units += 1;
        }
    }
    let mut before = Vec::new();
    for record in &request.records {
        before.extend(intervals(&record.canonical_result, source)?);
    }
    let shown_ids = paragraph_ids(&spans, &shown);
    let before_ids = paragraph_ids(&spans, &before);
    let mut positive = Vec::new();
    let mut progress = Vec::new();
    for alternative in array(question, "human_alternatives")? {
        let mut support = BTreeSet::new();
        for requirement in array(alternative, "evidence")? {
            for id in array(requirement, "allowed_locations")? {
                support.insert(id.as_str().context("original evidence ID")?);
            }
        }
        let roles_supported = unmapped_units == 0
            && !shown.is_empty()
            && shown.iter().all(|(start, end)| {
                spans.iter().any(|span| {
                    support.contains(span.id.as_str()) && span.start <= *start && span.end >= *end
                })
            });
        if roles_supported && reference_complete(alternative, &shown_ids)? {
            for card in &request.roster.cards {
                if matches!(
                    card.action,
                    airk_core::arbiter::context_action::ContextAction::Sufficient
                ) {
                    positive.push(json!({"card":card.id,"annotation_id":alternative["human_annotation_id"],
                        "proxy":"one_original_alternative_reference_complete_with_all_document_useful_units",
                        "human_sufficiency_gold":false}));
                }
            }
        }
        for outcome in array(row, "counterfactual_choices")? {
            let choice: airk_core::arbiter::ReadChoice =
                serde_json::from_value(outcome["choice"].clone())?;
            let mut after = before.clone();
            after.extend(intervals(&outcome["canonical_result"], source)?);
            let after_ids = paragraph_ids(&spans, &after);
            let adds_reference = !reference_complete(alternative, &before_ids)?
                && array(alternative, "evidence")?.iter().any(|requirement| {
                    requirement["allowed_locations"]
                        .as_array()
                        .is_some_and(|ids| {
                            !ids.iter()
                                .any(|id| id.as_str().is_some_and(|id| before_ids.contains(id)))
                                && ids
                                    .iter()
                                    .any(|id| id.as_str().is_some_and(|id| after_ids.contains(id)))
                        })
                });
            progress.push(
                json!({"choice":choice.id,"annotation_id":alternative["human_annotation_id"],
                "adds_original_missing_reference":adds_reference,"global_usefulness_gold":false}),
            );
            if adds_reference {
                for card in &request.roster.cards {
                    if matches!(card.action,airk_core::arbiter::context_action::ContextAction::Read{choice:id} if id == choice.id)
                    {
                        positive.push(json!({"card":card.id,"annotation_id":alternative["human_annotation_id"],
                            "proxy":"generic_read_original_reference_complement","semantic_need":"unclassified"}));
                    }
                }
            }
        }
    }
    Ok(
        json!({"valid_reference_proxy_whole_cards":positive,"counterfactual_reference_progress":progress,
        "shown_complete_original_paragraphs":shown_ids,"retained_complete_original_paragraphs":before_ids,
        "unmapped_source_units":unmapped_units,
        "limits":"Annotation-derived reference projections, not human action/sufficiency/clarification gold; original alternatives remain separate."}),
    )
}
#[test]
fn context_action_exact_span_union_keeps_partial_and_already_retained_support_distinct() {
    let spans = vec![ParagraphSpan {
        id: "original".into(),
        start: 0,
        end: 300,
    }];
    assert!(paragraph_ids(&spans, &[(0, 192)]).is_empty());
    assert!(paragraph_ids(&spans, &[(0, 192), (193, 300)]).is_empty());
    let before = paragraph_ids(&spans, &[(0, 300)]);
    let after = paragraph_ids(&spans, &[(0, 300), (0, 192)]);
    assert_eq!(
        before, after,
        "a model excerpt cannot make retained full support newly acquired"
    );
    assert_eq!(
        paragraph_ids(&spans, &[(0, 192), (192, 300)]),
        BTreeSet::from(["original"])
    );
}

fn sha(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
fn env_path(name: &str) -> Result<PathBuf> {
    Ok(std::env::var_os(name)
        .with_context(|| format!("missing {name}"))?
        .into())
}
fn pinned(path: &Path, expected: Sha256Digest) -> Result<Vec<u8>> {
    let bytes = std::fs::read(path)?;
    ensure!(sha(&bytes) == expected, "private input identity changed");
    Ok(bytes)
}
fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value[field]
        .as_str()
        .with_context(|| format!("missing {field}"))
}
fn array<'a>(value: &'a Value, field: &str) -> Result<&'a Vec<Value>> {
    value[field]
        .as_array()
        .with_context(|| format!("missing {field}"))
}
fn paragraph_order(id: &str) -> Result<(usize, usize)> {
    let mut fields = id.rsplitn(3, ':');
    let paragraph = fields
        .next()
        .context("paragraph ordinal missing")?
        .parse()?;
    let section = fields.next().context("section ordinal missing")?.parse()?;
    ensure!(fields.next().is_some(), "paper identity missing");
    Ok((section, paragraph))
}
fn facts() -> Result<Value> {
    let value: Value = serde_json::from_slice(&pinned(
        &env_path("AIRK_CONTEXT_ACTION_FACTS")?,
        FACTS_SHA.parse()?,
    )?)?;
    let mut documents = BTreeMap::new();
    for document in array(&value, "documents")? {
        ensure!(
            documents.insert(text(document, "id")?, document).is_none(),
            "duplicate publisher document"
        );
        ensure!(
            sha(text(document, "text")?.as_bytes()).to_string()
                == text(document, "paragraph_sha256")?,
            "publisher paragraph bytes changed"
        );
    }
    let mut ids = BTreeSet::new();
    let mut families = BTreeMap::new();
    for question in array(&value, "questions")? {
        ensure!(
            ids.insert(text(question, "id")?),
            "duplicate publisher question"
        );
        let split = text(question, "split")?;
        ensure!(
            matches!(split, "train" | "calibration"),
            "Dev/FINAL is not TRAIN/CAL"
        );
        let family = text(question, "source_group")?;
        if let Some(prior) = families.insert(family, split) {
            ensure!(prior == split, "publisher family crosses TRAIN/CAL");
        }
        for alternative in array(question, "human_alternatives")? {
            for evidence in array(alternative, "evidence")? {
                let locations = array(evidence, "allowed_locations")?;
                // Empty alignment remains an unresolved original requirement;
                // reference_complete() cannot turn it into positive coverage.
                for location in locations {
                    let document = documents
                        .get(location.as_str().context("invalid original location")?)
                        .context("foreign original location")?;
                    ensure!(
                        text(document, "source_group")? == family,
                        "foreign publisher evidence family"
                    );
                }
            }
        }
    }
    Ok(value)
}

#[test]
#[ignore = "explicit private TRAIN/CAL source preparation; no models or FINAL inputs"]
fn prepare_context_action_publisher_sources() -> Result<()> {
    let facts = facts()?;
    let destination = env_path("AIRK_CONTEXT_ACTION_PREPARED")?;
    ensure!(
        destination.is_absolute(),
        "absolute private destination required"
    );
    std::fs::create_dir(&destination)?;
    let mut groups: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for document in array(&facts, "documents")? {
        groups
            .entry(text(document, "source_group")?.into())
            .or_default()
            .push(document);
    }
    let mut sources = BTreeMap::new();
    for (family, documents) in groups {
        let mut documents = documents
            .into_iter()
            .map(|document| Ok((paragraph_order(text(document, "id")?)?, document)))
            .collect::<Result<Vec<_>>>()?;
        documents.sort_by_key(|(order, _)| *order);
        let root = destination.join(format!("paper-{}", sha(family.as_bytes())));
        std::fs::create_dir(&root)?;
        let mut bytes = String::new();
        let mut spans = Vec::new();
        for (_, document) in documents {
            let start = bytes.len();
            bytes.push_str(text(document, "text")?);
            let end = bytes.len();
            bytes.push_str("\n\n");
            spans.push(json!({"document":document["id"],"start":start,"end":end}));
        }
        std::fs::write(root.join("source.txt"), bytes.as_bytes())?;
        sources.insert(
            family,
            json!({"source_root":root,"source_sha256":sha(bytes.as_bytes()),"spans":spans}),
        );
    }
    let mut questions = Vec::new();
    for question in array(&facts, "questions")? {
        let family = text(question, "source_group")?;
        // Requests structure has no independent structural CAL. It is retained
        // in publisher facts, but cannot be smuggled into the QASPER evaluator.
        if question["dataset"].as_str() != Some("QASPER") {
            continue;
        }
        let source = sources.get(family).context("question source missing")?;
        questions.push(json!({"id":question["id"],"question":question["question"],"split":question["split"],"source_group":family,"source":source}));
    }
    ensure!(!questions.is_empty(), "zero publisher questions selected");
    let input = serde_json::to_vec_pretty(&json!({
        "schema":"airk.arbiter.context-action.publisher-sources/1","facts_sha256":FACTS_SHA,
        "task_policy":context_action::policy_digest()?,"data_root":destination.join("indexes"),"questions":questions
    }))?;
    std::fs::write(destination.join("capture-input.json"), &input)?;
    std::fs::write(
        destination.join("capture-input.identity.json"),
        serde_json::to_vec_pretty(&json!({
            "sha256":sha(&input),"framed_digest":framed_digest(b"airk.context-action.publisher-source-input/1",&[&input])
        }))?,
    )?;
    Ok(())
}

/// Exact byte containment only. A partial prefix never inherits a paragraph's
/// reference-complete annotation, and a text alias outside its family is refused.
fn full_paragraphs<'a>(
    windows: &[(&str, &str)],
    documents: &'a [Value],
    family: &str,
) -> Result<BTreeSet<&'a str>> {
    let mut complete = BTreeSet::new();
    for (source_family, window) in windows {
        ensure!(*source_family == family, "foreign shown source family");
        for document in documents {
            if text(document, "source_group")? == family && window.contains(text(document, "text")?)
            {
                complete.insert(text(document, "id")?);
            }
        }
    }
    Ok(complete)
}
fn reference_complete(alternative: &Value, shown: &BTreeSet<&str>) -> Result<bool> {
    let evidence = array(alternative, "evidence")?;
    Ok(alternative["unanswerable"] == false
        && !evidence.is_empty()
        && evidence.iter().all(|item| {
            item["allowed_locations"]
                .as_array()
                .is_some_and(|locations| {
                    locations
                        .iter()
                        .any(|id| id.as_str().is_some_and(|id| shown.contains(id)))
                })
        }))
}
#[test]
fn native_context_action_labels_refuse_partial_prefix_foreign_family_and_alternative_union()
-> Result<()> {
    assert!(paragraph_order("paper:3:2")? < paragraph_order("paper:3:10")?);
    assert!(paragraph_order("paper:2:99")? < paragraph_order("paper:3:1")?);
    assert!(paragraph_order("unstructured").is_err());
    let long = "evidence ".repeat(40);
    let docs = vec![
        json!({"id":"a","source_group":"paper","text":long}),
        json!({"id":"b","source_group":"paper","text":"second original support"}),
    ];
    assert!(full_paragraphs(&[("paper", &long[..192])], &docs, "paper")?.is_empty());
    assert!(full_paragraphs(&[("other", &long)], &docs, "paper").is_err());
    let a = json!({"unanswerable":false,"evidence":[{"allowed_locations":["a"]},{"allowed_locations":["b"]}]});
    let b = json!({"unanswerable":false,"evidence":[{"allowed_locations":["b"]},{"allowed_locations":["c"]}]});
    let shown = BTreeSet::from(["a", "c"]);
    assert!(!reference_complete(&a, &shown)?);
    assert!(!reference_complete(&b, &shown)?);
    assert!(reference_complete(&a, &BTreeSet::from(["a", "b"]))?);
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelInput {
    profile: PathBuf,
    assets: PathBuf,
    fixtures: PathBuf,
}
fn validated_capture(facts: &Value, capture: &Value) -> Result<Vec<Value>> {
    ensure!(
        capture["schema"] == "airk.arbiter.context-action.publisher-capture/1"
            && capture["complete"] == true
            && capture["facts_sha256"] == FACTS_SHA
            && capture["nn_invocations"] == 0
            && capture["fixture_scope_and_clock"] == true,
        "foreign/incomplete publisher capture"
    );
    let questions: BTreeMap<_, _> = array(facts, "questions")?
        .iter()
        .filter(|question| question["dataset"] == "QASPER")
        .map(|question| Ok((text(question, "id")?, question)))
        .collect::<Result<_>>()?;
    let mut seen = BTreeSet::new();
    let mut rows = Vec::new();
    for row in array(capture, "rows")? {
        let id = text(row, "id")?;
        ensure!(seen.insert(id), "duplicate captured publisher question");
        let question = questions
            .get(id)
            .context("nonpublisher/Dev/FINAL question")?;
        ensure!(
            row["split"] == question["split"] && row["source_group"] == question["source_group"],
            "publisher partition/family changed"
        );
        let mut row = row.clone();
        if row["request"].is_null() {
            ensure!(
                row["status"] == "no_native_assessment_request",
                "missing native request is not a score"
            );
            let packet = airk_core::arbiter::context_action::ContextResponseV2::from_canonical(
                &row["canonical_packet"],
                &airk_core::arbiter::Budget::default(),
            )?;
            ensure!(
                packet
                    .assessment_history
                    .iter()
                    .all(|event| !airk_core::combined_context::has_native_receipt(event)),
                "uncaptured native assessment receipt"
            );
            rows.push(row);
            continue;
        }
        let request: NativeContextActionRequest = serde_json::from_value(row["request"].clone())?;
        ensure!(
            request.question == text(question, "question")? && request.ordinal == 1,
            "publisher question/first assessment changed"
        );
        request.validate(
            request.worker,
            request.authority,
            request.read_attempt.clock,
            0,
        )?;
        ensure!(
            serde_json::to_value(&request.parent_budget)?
                == serde_json::to_value(airk_core::arbiter::Budget::default())?,
            "original publisher budget changed"
        );
        let rebuilt = context_action::build_roster(
            &request.question,
            &request.scope,
            &request.records,
            &request.choices,
        )?;
        ensure!(
            serde_json::to_value(&rebuilt)? == serde_json::to_value(&request.roster)?,
            "current native 192B roster differs from capture"
        );
        context_action::prompt(&request)?;
        let mut probed = BTreeSet::new();
        for outcome in array(&row, "counterfactual_choices")? {
            let choice: airk_core::arbiter::ReadChoice =
                serde_json::from_value(outcome["choice"].clone())?;
            ensure!(probed.insert(choice.id), "duplicate counterfactual read");
            ensure!(
                request
                    .roster
                    .basis
                    .read_choices_scored
                    .contains(&choice.id)
                    && request
                        .choices
                        .iter()
                        .any(|original| serde_json::to_value(original).ok()
                            == Some(outcome["choice"].clone())),
                "foreign/unoffered counterfactual read"
            );
            let publication: airk_core::evidence::RevisionEnvelope =
                serde_json::from_value(outcome["canonical_result"]["publication"].clone())?;
            ensure!(
                publication == request.scope.publication,
                "foreign counterfactual publication"
            );
        }
        ensure!(
            probed
                == request
                    .roster
                    .basis
                    .read_choices_scored
                    .iter()
                    .copied()
                    .collect(),
            "offered counterfactual read omitted"
        );
        row["reference_proxies"] = source_alignment(&row, question, facts, &request)?;
        rows.push(row);
    }
    ensure!(
        seen == questions.keys().copied().collect(),
        "publisher capture silently omitted questions"
    );
    Ok(rows)
}

#[test]
#[ignore = "cheap explicit native TRAIN/CAL span preflight; no model load or qualification"]
fn preflight_context_action_publisher_capture_and_reference_spans() -> Result<()> {
    let facts = facts()?;
    let bytes = pinned(
        &env_path("AIRK_CONTEXT_ACTION_CAPTURE")?,
        std::env::var("AIRK_CONTEXT_ACTION_CAPTURE_SHA256")?.parse()?,
    )?;
    let capture: Value = serde_json::from_slice(&bytes)?;
    let rows = validated_capture(&facts, &capture)?;
    let output = env_path("AIRK_CONTEXT_ACTION_PREFLIGHT_OUTPUT")?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    use std::io::Write;
    file.write_all(&serde_json::to_vec_pretty(&json!({"schema":"airk.arbiter.context-action.reference-preflight/1","facts_sha256":FACTS_SHA,"capture_sha256":sha(&bytes),"task_policy":context_action::policy_digest()?,"complete":true,"qualified_profiles":[],"rows":rows}))?)?;
    Ok(())
}
struct Files(PathBuf);
impl AssetSource for Files {
    fn read_asset(&self, asset: &Asset) -> Result<Vec<u8>> {
        use std::io::Read;
        let file = std::fs::File::open(self.0.join(&asset.name))?;
        ensure!(
            file.metadata()?.len() == asset.bytes,
            "raw model asset length changed"
        );
        let mut bytes = Vec::new();
        file.take(
            asset
                .bytes
                .checked_add(1)
                .context("asset length overflow")?,
        )
        .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 == asset.bytes,
            "raw asset changed while reading"
        );
        Ok(bytes)
    }
}

#[test]
#[ignore = "explicit one-model complete-trained-readout TRAIN/CAL raw export; never release admission"]
fn context_action_train_cal_complete_readout_raw_scores() -> Result<()> {
    let facts = facts()?;
    let profiles_bytes = pinned(
        &env_path("AIRK_CONTEXT_ACTION_PROFILES")?,
        PROFILES_SHA.parse()?,
    )?;
    let inputs: Vec<ModelInput> = serde_json::from_slice(&profiles_bytes)?;
    let selected: ModelChoice =
        serde_json::from_value(Value::String(std::env::var("AIRK_CONTEXT_ACTION_MODEL")?))?;
    let mut models = BTreeMap::new();
    for input in inputs {
        let raw = std::fs::read(&input.profile)?;
        let profile: Profile = serde_json::from_slice(&raw)?;
        profile.validate()?;
        ensure!(
            models
                .insert(profile.model, (input, profile, sha(&raw)))
                .is_none(),
            "duplicate trained model slot"
        );
    }
    ensure!(
        models.keys().copied().collect::<BTreeSet<_>>()
            == BTreeSet::from([
                ModelChoice::SmallIntern,
                ModelChoice::SmallStrands,
                ModelChoice::LargeIntern,
                ModelChoice::LargeKev
            ]),
        "all four complete trained readouts required"
    );
    let (input, profile, profile_sha) =
        models.remove(&selected).context("selected model missing")?;
    // Read the independently retained fixture identity too; do not reinterpret
    // its old task scores as ContextAction expectations.
    let fixture_sha = sha(&std::fs::read(&input.fixtures)?);
    let transcript_bytes = pinned(
        &env_path("AIRK_CONTEXT_ACTION_CAPTURE")?,
        std::env::var("AIRK_CONTEXT_ACTION_CAPTURE_SHA256")?.parse()?,
    )?;
    let transcript: Value = serde_json::from_slice(&transcript_bytes)?;
    ensure!(
        transcript["schema"] == "airk.arbiter.context-action.publisher-capture/1"
            && transcript["complete"] == true
            && transcript["facts_sha256"] == FACTS_SHA,
        "incomplete/foreign native publisher capture"
    );
    // Validate every native recipe, original source byte span and split before
    // opening any multi-gigabyte graph or consuming inference resources.
    let validated_rows = validated_capture(&facts, &transcript)?;
    let output = env_path("AIRK_CONTEXT_ACTION_SCORE_OUTPUT")?;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&output)?;
    let binary_sha = sha(&std::fs::read(std::env::current_exe()?)?);
    let mut results = Vec::new();
    let report = |complete, rows: &Vec<Value>| json!({"schema":"airk.arbiter.context-action.raw-training-scores/1","complete":complete,"release_qualified":false,"task_qualified":false,"model":selected,"profile":profile.manifest,"profile_source_sha256":profile_sha,"old_compiler_fixture_sha256":fixture_sha,"facts_sha256":FACTS_SHA,"profiles_sha256":PROFILES_SHA,"capture_sha256":sha(&transcript_bytes),"task_policy":context_action::policy_digest().ok(),"test_binary_sha256":binary_sha,"rows":rows});
    std::fs::write(
        &output,
        serde_json::to_vec_pretty(&report(false, &results))?,
    )?;
    ensure!(
        validated_rows.iter().any(|row| !row["request"].is_null()),
        "no actual native prompts available; model must not load"
    );
    let owner = NativeInferenceOwner::admit_private_fixture(&env_path(
        "AIRK_CONTEXT_ACTION_PRIVATE_CONTROL",
    )?)?;
    owner.run(|| -> Result<()> {
        let scorer = Scorer::open(ExecutionProfile::for_qualification(profile.clone())?,&Files(input.assets))?;
        let questions: BTreeMap<_,_> = array(&facts,"questions")?.iter().map(|q| Ok((text(q,"id")?,q))).collect::<Result<_>>()?;
        let mut seen = BTreeSet::new();
        for row in &validated_rows {
            let id = text(row,"id")?;
            ensure!(seen.insert(id), "duplicate native publisher episode");
            let question = questions.get(id).context("nonpublisher/Dev/FINAL episode")?;
            ensure!(row["split"] == question["split"] && row["source_group"] == question["source_group"], "publisher split/family changed");
            if row["request"].is_null() {
                results.push(json!({"id":id,"split":question["split"],"source_group":question["source_group"],"status":"no_native_assessment_request","native_capture_status":row["status"]}));
                std::fs::write(&output,serde_json::to_vec_pretty(&report(false,&results))?)?;
                continue;
            }
            let request: NativeContextActionRequest = serde_json::from_value(row["request"].clone())?;
            ensure!(request.question == text(question,"question")?, "publisher question changed");
            let rebuilt = context_action::build_roster(&request.question,&request.scope,&request.records,&request.choices)?;
            ensure!(serde_json::to_value(&rebuilt)? == serde_json::to_value(&request.roster)?, "native roster/current192B policy differs from capture");
            let prompt = context_action::prompt(&request)?;
            let at = Instant::now();
            let compiled = scorer.compile(&prompt.state,&prompt.question,&prompt.candidates);
            let compile_us = at.elapsed().as_micros();
            let record = match compiled {
                Err(error) => json!({"id":id,"split":question["split"],"source_group":question["source_group"],"status":"compiler_refused","error":format!("{error:#}"),"compile_us":compile_us,"roster":request.roster}),
                Ok(compiled) => {
                    ensure!(compiled.input_ids.len() <= request.remaining_input_tokens && compiled.candidate_positions.len() <= 8, "original token/card budget exceeded");
                    let at = Instant::now();
                    let scores = scorer.score(&compiled)?;
                    ensure!(scores.len() == request.roster.cards.len() && scores.iter().all(|score| score.is_finite()), "raw score shape/nonfinite refusal");
                    json!({"id":id,"split":question["split"],"source_group":question["source_group"],"status":"raw_scores_only","compile_us":compile_us,"inference_us":at.elapsed().as_micros(),"compiled":compiled,"scores":scores,"roster":request.roster,"reference_proxies":row["reference_proxies"],"unsupported_classes":["clarify","no_justified_read","semantic_roles","semantic_gaps"]})
                }
            };
            results.push(record);
            std::fs::write(&output,serde_json::to_vec_pretty(&report(false,&results))?)?;
        }
        ensure!(!seen.is_empty(), "zero native episodes selected");
        Ok(())
    })??;
    std::fs::write(output, serde_json::to_vec_pretty(&report(true, &results))?)?;
    Ok(())
}
