// SPDX-License-Identifier: Apache-2.0
//! Advertised public Answer inputs through Unified and the actual native SDK.
use crate::{
    local,
    remote::{RemoteFuture, RemoteTools},
    unified,
};
use airk_core::{
    arbiter::{
        Budget,
        context_action::{
            CONTEXT_SCHEMA_V2 as CONTEXT_SCHEMA, ContextRequestV2 as ContextRequest,
            REQUEST_SCHEMA_V2 as REQUEST_SCHEMA,
        },
    },
    identity::Incarnation,
    service::{ScopedReadRef, WorkerIdentity},
};
use airk_kernel::Kernel;
use anyhow::{Context, Result};
use serde_json::{Map, Value, json};
use std::sync::{Arc, Mutex};

struct Native {
    kernel: Kernel,
    calls: Mutex<Vec<(String, Map<String, Value>)>>,
    results: Mutex<Vec<Value>>,
}
impl RemoteTools for Native {
    fn call(&self, tool: String, args: Map<String, Value>) -> RemoteFuture<'_> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push((tool.clone(), args.clone()));
            let mut sdk = local::LocalTools::new(self.kernel.clone()).await?;
            let result = sdk.call(tool, args).await;
            sdk.close().await?;
            let value = serde_json::to_value(result?)?;
            self.results.lock().unwrap().push(value.clone());
            Ok(value)
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_answer_sdk_preserves_advertised_schema_full_utf8_question_and_budget() -> Result<()>
{
    let fixture = tempfile::tempdir()?;
    let root = fixture.path().join("source");
    std::fs::create_dir_all(root.join("public"))?;
    std::fs::write(
        root.join("public/policy.txt"),
        "amber_marker owns this request deadline.\n",
    )?;
    let base = Kernel::open(&root, Some(fixture.path().join("data")))?;
    base.configure_vectors(false)?;
    base.index()?;
    let authority = crate::scoped_observation_tests::authority(&base)?;
    let kernel = base.with_scoped_reads(authority.clone())?;
    let reference = ScopedReadRef {
        schema: 1,
        lease_generation: 1,
        index_id: authority.index_id,
        grant_id: Some(authority.grant.id),
        grant_revision: Some(authority.grant.revision),
        selection: None,
        fingerprint: kernel.scoped_read_fingerprint()?.unwrap(),
    };
    let worker = WorkerIdentity {
        service: Incarnation::from_bytes([8; 16]),
        worker: Incarnation::from_bytes([9; 16]),
        project: authority.backing,
        fencing_generation: 1,
    };
    let kernel = kernel.with_native_arbiter(
        worker,
        reference,
        Arc::new(crate::arbiter_handler_tests::Disabled),
    )?;
    let native = Arc::new(Native {
        kernel,
        calls: Mutex::new(vec![]),
        results: Mutex::new(vec![]),
    });
    let project = authority.backing.to_string();
    let public = unified::Unified::new(
        vec![unified::Project {
            binding: project.clone(),
            name: "Public Answer fixture".into(),
            collection: "default".into(),
            backend: native.clone(),
        }],
        Some(project.clone()),
        None,
    )?;
    let tool = unified::tools()
        .into_iter()
        .find(|tool| tool.name == "answer")
        .unwrap();
    let schema = serde_json::to_value(&tool.input_schema)?;
    assert_eq!(schema["required"], json!(["schema", "question", "budget"]));
    for field in ["schema", "question", "budget"] {
        assert!(
            schema["properties"].get(field).is_some(),
            "advertised Answer lost {field}"
        );
    }
    let budget = Budget {
        calls: 3,
        rounds: 1,
        fanout: 2,
        input_tokens: 1024,
        candidates: 4,
        max_bytes: 65_536,
        deadline_ms: 20_000,
    };
    let short = ContextRequest {
        schema: REQUEST_SCHEMA.into(),
        question: "amber_marker".into(),
        budget: budget.clone(),
    };
    short.validate()?;
    let mut full_question = "amber_marker é中".to_owned();
    full_question.push_str(&" ".repeat(16_384 - full_question.len()));
    assert_eq!(full_question.len(), 16_384);
    for question in [&short.question, &full_question] {
        let request = ContextRequest {
            question: question.clone(),
            ..short.clone()
        };
        request.validate()?;
        let mut arguments = serde_json::to_value(&request)?.as_object().unwrap().clone();
        arguments.insert("response_mode".into(), json!("full"));
        let expected = arguments.clone();
        arguments.insert("project".into(), json!(project));
        let reply = public.call("answer".into(), arguments).await?;
        assert_ne!(reply["isError"], true, "{reply}");
        assert_eq!(Some(&reply), native.results.lock().unwrap().last());
        assert_eq!(reply["content"].as_array().unwrap().len(), 1);
        let packet: Value = serde_json::from_str(
            reply["content"][0]["text"]
                .as_str()
                .context("missing canonical text")?,
        )?;
        assert_eq!(packet["schema"], CONTEXT_SCHEMA);
        assert_eq!(packet["question"], *question);
        if let Some(structured) = reply.get("structuredContent") {
            assert_eq!(*structured, packet);
        }
        assert_eq!(
            native.calls.lock().unwrap().last(),
            Some(&("airk_answer".into(), expected))
        );
        assert!(packet["work"]["calls"].as_u64().unwrap() <= 3);
    }
    for missing in ["schema", "budget"] {
        let mut arguments = serde_json::to_value(&short)?.as_object().unwrap().clone();
        arguments.remove(missing);
        let refusal = public.call("answer".into(), arguments).await?;
        // The real SDK reports parameter decoding failures as error tool
        // results, carried intact by Unified, rather than transport errors.
        assert_eq!(refusal["isError"], true, "{refusal}");
        assert_eq!(Some(&refusal), native.results.lock().unwrap().last());
        let message = refusal["content"][0]["text"]
            .as_str()
            .context("SDK parameter refusal lost its canonical text")?;
        assert!(
            message.contains(&format!("missing field `{missing}`")),
            "{refusal}"
        );
    }
    let mut missing_schema = serde_json::to_value(&short)?;
    missing_schema.as_object_mut().unwrap().remove("schema");
    let value = json!({
        "schema":"airk.arbiter.public-answer-cases/1",
        "public_tool":"answer", "native_tool":"airk_answer", "input_schema":schema,
        "maximum_question_utf8_bytes":16_384,
        "examples":[{"name":"complete_native_request","arguments":short,"accepted":true},
                    {"name":"missing_required_schema","arguments":missing_schema,"accepted":false}]
    });
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/split/arbiter/public_answer_cases.json");
    if std::env::var_os("AIRK_WRITE_ARBITER_PUBLIC_ANSWER_CASES").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        std::fs::create_dir_all(path.parent().context("missing native fixture parent")?)?;
        std::fs::write(&path, serde_json::to_string_pretty(&value)? + "\n")?;
    }
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    assert_eq!(
        saved, value,
        "regenerate native public Answer cases and review the exact advertised contract"
    );
    Ok(())
}
