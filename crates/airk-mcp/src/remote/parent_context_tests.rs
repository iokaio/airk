// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::read_attempt::ReadAttemptContext;
use std::{sync::Mutex, time::Instant};

struct Observation {
    tool: String,
    arguments: serde_json::Map<String, serde_json::Value>,
    deadline: Instant,
}
#[derive(Default)]
struct Observed(Mutex<Vec<Observation>>);
impl RemoteTools for Observed {
    fn call(
        &self,
        _tool: String,
        _arguments: serde_json::Map<String, serde_json::Value>,
    ) -> RemoteFuture<'_> {
        panic!("contextless backend path must not be used")
    }
    fn call_with_read_context(
        &self,
        tool: String,
        arguments: serde_json::Map<String, serde_json::Value>,
        _limit: usize,
        context: ReadAttemptContext,
    ) -> RemoteFuture<'_> {
        self.0.lock().unwrap().push(Observation {
            tool,
            arguments,
            deadline: context.deadline(),
        });
        Box::pin(async {
            Ok(
                serde_json::json!({"content":[{"type":"text","text":"{\"provenance\":\"original\",\"continuation\":null}"}],"structuredContent":{"provenance":"original","continuation":null},"isError":false}),
            )
        })
    }
}
#[tokio::test]
async fn canonical_transport_keeps_short_and_long_original_context_and_full_result()
-> anyhow::Result<()> {
    let backend = Observed::default();
    let arguments =
        serde_json::json!({"query":"original input","response_mode":"full","include_status":true})
            .as_object()
            .unwrap()
            .clone();
    let mut replies = Vec::new();
    for seconds in [2, 90] {
        let deadline = Instant::now() + Duration::from_secs(seconds);
        let context = ReadAttemptContext::new(deadline, Default::default());
        replies.push(
            call_observed_with_context(&backend, "airk_search".into(), arguments.clone(), context)
                .await?,
        );
        let observations = backend.0.lock().unwrap();
        let observed = observations.last().unwrap();
        assert_eq!(observed.tool, "airk_search");
        assert_eq!(observed.arguments, arguments);
        assert_eq!(observed.deadline, deadline);
    }
    assert_eq!(replies[0], replies[1]);
    assert_eq!(replies[0]["structuredContent"]["provenance"], "original");
    assert_eq!(
        replies[0]["structuredContent"]["continuation"],
        serde_json::Value::Null
    );
    let expired = ReadAttemptContext::new(Instant::now(), Default::default());
    assert!(
        call_observed_with_context(&backend, "airk_answer".into(), arguments.clone(), expired)
            .await
            .is_err()
    );
    let cancelled =
        ReadAttemptContext::new(Instant::now() + Duration::from_secs(2), Default::default());
    cancelled.cancellation().cancel();
    assert!(
        call_observed_with_context(&backend, "airk_answer".into(), arguments, cancelled)
            .await
            .is_err()
    );
    assert_eq!(
        backend.0.lock().unwrap().len(),
        2,
        "expired/cancelled parent must not execute backend work"
    );
    Ok(())
}

#[tokio::test]
async fn routed_answer_admission_preserves_complete_canonical_arguments_and_reply()
-> anyhow::Result<()> {
    let backend = Observed::default();
    let mut request = serde_json::to_value(airk_core::arbiter::context_action::ContextRequestV2 {
        schema: airk_core::arbiter::context_action::REQUEST_SCHEMA_V2.into(),
        question: "routed original allowance".into(),
        budget: airk_core::arbiter::Budget {
            deadline_ms: 2000,
            ..Default::default()
        },
    })?
    .as_object()
    .unwrap()
    .clone();
    request.insert("response_mode".into(), "full".into());
    let mut replies = Vec::new();
    for routing in [
        serde_json::json!({"project":"ab".repeat(32)}),
        serde_json::json!({"project_ids":[format!("prj_{}","ab".repeat(16))],"index_ids":[format!("idx_{}","cd".repeat(16))],"member_availability":"require"}),
    ] {
        let mut arguments = request.clone();
        arguments.extend(routing.as_object().unwrap().clone());
        let original = Instant::now() + Duration::from_secs(90);
        replies.push(
            call_observed_with_context(
                &backend,
                "answer".into(),
                arguments.clone(),
                ReadAttemptContext::new(original, Default::default()),
            )
            .await?,
        );
        let observations = backend.0.lock().unwrap();
        let observed = observations.last().unwrap();
        assert_eq!(
            observed.arguments, arguments,
            "canonical handler must retain every original routing/input field"
        );
        assert!(observed.deadline < original);
        assert!(observed.deadline <= Instant::now() + Duration::from_secs(2));
    }
    assert_eq!(
        replies[0], replies[1],
        "budget projection must preserve the complete canonical reply"
    );
    assert_eq!(replies[0]["structuredContent"]["provenance"], "original");
    Ok(())
}
