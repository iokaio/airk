// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::service::NativeNextReadRequest;
use airk_core::{
    DocumentLocation,
    evidence::RevisionEnvelope,
    identity::{Incarnation, RevisionToken, Sha256Digest},
    index_catalog::IndexId,
    service::{ScopedReadRef, WorkerIdentity},
};
use std::collections::BTreeMap;
fn request() -> NativeNextReadRequest {
    let scope = ScopeBinding {
        installation: Sha256Digest::from_bytes([1; 32]),
        catalog: Incarnation::from_bytes([2; 16]),
        index_id: IndexId::from_identity(Incarnation::from_bytes([3; 16])),
        authorization: Sha256Digest::from_bytes([4; 32]),
        configuration_revision: 1,
        publication: RevisionEnvelope::new(
            "fixture".into(),
            RevisionToken {
                incarnation: Incarnation::from_bytes([5; 16]),
                generation: 1,
                revision: Sha256Digest::from_bytes([6; 32]),
            },
        ),
    };
    let registration = airk_core::service::ProjectRegistration::new(
        "/source".into(),
        "/data".into(),
        "fixture".into(),
    )
    .unwrap();
    let hits:Vec<_>=(0..8).map(|i|json!({"citation":{"path":format!("source-{i}.md"),"content_hash":"fixture","start_line":10,"end_line":20,"location":DocumentLocation::Lines{start:10,end:20}},"text":format!("source-{i} {}","Ã©".repeat(400)),"truncated":false})).collect();
    let record = EvidenceRecord {
        id: Sha256Digest::from_bytes([7; 32]),
        scope: scope.clone(),
        operation: NativeRead::Search,
        arguments: BTreeMap::new(),
        role: EvidenceRole::Unknown,
        canonical_result: json!({"hits":hits}),
    };
    NativeNextReadRequest {
        read_attempt: None,
        worker: WorkerIdentity {
            service: scope.catalog,
            worker: Incarnation::from_bytes([8; 16]),
            project: registration.binding,
            fencing_generation: 1,
        },
        authority: ScopedReadRef {
            schema: 1,
            lease_generation: 1,
            index_id: scope.index_id,
            grant_id: None,
            grant_revision: None,
            selection: None,
            fingerprint: scope.authorization,
        },
        scope,
        question: "Which source defines the original constraint?".into(),
        records: vec![record],
        choices: (0..8)
            .map(|i| ReadChoice {
                id: Sha256Digest::from_bytes([i + 10; 32]),
                operation: NativeRead::ReadEvidence,
                target: format!("source-{i}.md:10-52"),
            })
            .collect(),
        remaining_ms: 30_000,
        remaining_input_tokens: 2048,
    }
}
#[test]
fn all_eight_choices_have_visible_utf8_bounded_excerpts_and_closed_aliases() -> Result<()> {
    let request = request();
    let prompt = context_next_read(&request)?;
    let state: serde_json::Value = serde_json::from_str(&prompt.state)?;
    assert_eq!(prompt.candidates.len(), 8);
    assert_eq!(prompt.aliases.len(), 8);
    let excerpts = state["indexed_evidence"].as_array().unwrap();
    assert_eq!(excerpts.len(), 8);
    let bytes: usize = excerpts
        .iter()
        .map(|e| e["text_excerpt"].as_str().unwrap().len())
        .sum();
    assert!(bytes <= 2048);
    for (i, excerpt) in excerpts.iter().enumerate() {
        assert_eq!(prompt.aliases[i].choice, request.choices[i].id);
        assert_eq!(prompt.candidates[i].id, format!("c{}", i + 1));
        assert_eq!(excerpt["path"], format!("source-{i}.md"));
        assert!(excerpt["partial"].as_bool().unwrap());
    }
    assert_eq!(state["omitted_excerpts"], 0);
    assert!(!prompt.state.contains(&request.choices[0].id.to_string()));
    Ok(())
}
#[test]
fn newest_actual_read_is_visible_without_reversing_rank_inside_records() -> Result<()> {
    let mut request = request();
    let mut newer = request.records[0].clone();
    newer.id = Sha256Digest::from_bytes([99; 32]);
    newer.operation = NativeRead::ReadEvidence;
    newer.canonical_result = json!({"hits":[request.records[0].canonical_result["hits"][7].clone(),request.records[0].canonical_result["hits"][6].clone()]});
    newer.canonical_result["hits"][0]["text"] = "NEW actual source seven".into();
    newer.canonical_result["hits"][1]["text"] = "NEW actual source six".into();
    request.records.push(newer);
    let prompt = context_next_read(&request)?;
    let state: serde_json::Value = serde_json::from_str(&prompt.state)?;
    assert_eq!(
        state["indexed_evidence"][0]["text_excerpt"],
        "NEW actual source seven"
    );
    assert_eq!(
        state["indexed_evidence"][1]["text_excerpt"],
        "NEW actual source six"
    );
    assert_eq!(prompt.aliases[7].record, Sha256Digest::from_bytes([99; 32]));
    assert_eq!(state["omitted_excerpts"], 2);
    Ok(())
}
#[test]
fn an_unrepresented_choice_refuses_and_equal_trained_scores_abstain() -> Result<()> {
    let mut request = request();
    request.choices[7].target = "unseen.md:10-52".into();
    assert!(context_next_read(&request).is_err());
    assert_eq!(selected_read(&[1.0, 1.0], &request.choices[..2])?, None);
    assert_eq!(
        selected_read(&[1.0, 2.0], &request.choices[..2])?,
        Some(request.choices[1].id)
    );
    assert!(selected_read(&[f32::NAN, 2.0], &request.choices[..2]).is_err());
    Ok(())
}

#[test]
#[ignore = "private actual canonical calibration capture and pinned compiler assets required"]
fn native_calibration_transcripts_bind_exact_task_policy_aliases_and_compiler_inputs() -> Result<()>
{
    use anyhow::Context;
    use serde::Deserialize;
    use sha2::{Digest, Sha256};
    use std::path::PathBuf;
    #[derive(Deserialize)]
    struct Assets {
        profile: PathBuf,
        assets: PathBuf,
    }
    let capture_bytes = std::fs::read(
        std::env::var_os("AIRK_ARBITER_CALIBRATION_TRANSCRIPT_INPUT")
            .context("missing actual canonical capture")?,
    )?;
    let capture: serde_json::Value = serde_json::from_slice(&capture_bytes)?;
    let profile_inputs_bytes = std::fs::read(
        std::env::var_os("AIRK_ARBITER_CALIBRATION_TRANSCRIPT_PROFILES")
            .context("missing pinned compiler profiles")?,
    )?;
    let profiles: Vec<Assets> = serde_json::from_slice(&profile_inputs_bytes)?;
    let output = std::env::var_os("AIRK_ARBITER_CALIBRATION_TRANSCRIPT_OUTPUT")
        .context("missing native transcript output")?;
    ensure!(
        profiles.len() == 4,
        "all four exact compiler profiles required"
    );
    let rows = capture["rows"]
        .as_array()
        .context("missing native capture rows")?;
    ensure!(
        !rows.is_empty() && rows.len() <= 29,
        "frozen calibration partition only"
    );
    let mut results = Vec::new();
    for input in profiles {
        let profile_bytes = std::fs::read(&input.profile)?;
        let profile: Profile = serde_json::from_slice(&profile_bytes)?;
        let read = |role| -> Result<Vec<u8>> {
            let asset = profile
                .assets
                .iter()
                .find(|a| a.role == role)
                .context("missing compiler asset")?;
            let bytes = std::fs::read(input.assets.join(&asset.name))?;
            ensure!(
                bytes.len() as u64 == asset.bytes
                    && Sha256Digest::from_bytes(Sha256::digest(&bytes).into()) == asset.sha256,
                "actual compiler asset differs from profile pin"
            );
            Ok(bytes)
        };
        let compiler = crate::arbiter::compiler::Compiler::from_verified(
            &read(AssetRole::Compiler)?,
            &read(AssetRole::Tokenizer)?,
        )?;
        let mut transcripts = Vec::new();
        for row in rows {
            let request: NativeNextReadRequest = serde_json::from_value(row["request"].clone())?;
            match context_next_read(&request) {
                Err(error) => transcripts
                    .push(json!({"id":row["id"],"applied":false,"reason":error.to_string()})),
                Ok(prompt) => {
                    match compiler.compile(&prompt.state,&prompt.question,&prompt.candidates) {
                        Err(error)=>transcripts.push(json!({"id":row["id"],"applied":false,"reason":error.to_string(),"prompt":prompt})),
                        Ok(compiled)=> {
                            ensure!(compiled.input_ids.len()<=2048,"native task token budget crossed");
                            // Independent boundary control: an oversized question must
                            // refuse rather than truncate into a scoreable prompt.
                            let mut oversized=request.clone();oversized.question=" source".repeat(2300);
                            let large=context_next_read(&oversized)?;
                            assert!(compiler.compile(&large.state,&large.question,&large.candidates).is_err(),"oversized native task must not silently truncate");
                            transcripts.push(json!({"id":row["id"],"applied":true,"prompt":prompt,"compiled":compiled}));
                        }
                    }
                }
            }
        }
        results.push(json!({"model":profile.model,"profile":profile.manifest,"profile_source_sha256":format!("{:x}",Sha256::digest(&profile_bytes)),"compiler":profile.compiler,"task_policy":policy_digest()?,"rows":transcripts}));
    }
    std::fs::write(
        output,
        serde_json::to_vec_pretty(
            &json!({"schema":"airk.arbiter.native-calibration-transcripts/1","evaluation":true,"release_qualified":false,"scope":"actual native task projection and compiler bytes; no model score/calibration/quality claim","capture_sha256":format!("{:x}",Sha256::digest(&capture_bytes)),"profile_inputs_sha256":format!("{:x}",Sha256::digest(&profile_inputs_bytes)),"profiles":results}),
        )?,
    )?;
    Ok(())
}

#[test]
fn task_source_policy_canonicalizes_only_crlf_and_refuses_lone_cr() -> Result<()> {
    let lf = b"fn policy() {\n    execute();\n}\n";
    let crlf = b"fn policy() {\r\n    execute();\r\n}\r\n";
    assert_eq!(source_policy_digest(lf)?, source_policy_digest(crlf)?);
    assert_ne!(
        source_policy_digest(lf)?,
        source_policy_digest(b"fn policy() {\n   execute();\n}\n")?
    );
    assert_ne!(
        source_policy_digest(lf)?,
        source_policy_digest(b"fn policy() {\n    refuse();\n}\n")?
    );
    assert!(source_policy_digest(b"fn policy() {\r execute(); }").is_err());
    Ok(())
}

#[test]
fn literal_search_option_has_exact_visible_byte_provenance_and_rejects_foreign_source() -> Result<()>
{
    let mut request = request();
    request.choices.truncate(1);
    request.records[0].canonical_result["hits"][0]["text"] =
        "prefix John Smith is the source actor".into();
    let source = literal::SourceLiteral {
        record: request.records[0].id,
        path: "source-0.md".into(),
        line: 10,
        returned_utf8_start: 7,
        returned_utf8_end: 17,
        query: "John Smith".into(),
    };
    request.choices[0].operation = NativeRead::Search;
    request.choices[0].target = source.target()?;
    let prompt = context_next_read(&request)?;
    assert!(
        prompt.candidates[0]
            .text
            .contains("search indexed sources for literal")
    );
    let state: serde_json::Value = serde_json::from_str(&prompt.state)?;
    assert!(
        state["indexed_evidence"][0]["text_excerpt"]
            .as_str()
            .unwrap()
            .contains("John Smith")
    );
    let mut foreign = source;
    foreign.record = Sha256Digest::from_bytes([88; 32]);
    request.choices[0].target = foreign.target()?;
    assert!(context_next_read(&request).is_err());
    Ok(())
}
