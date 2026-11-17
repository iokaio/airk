# SPDX-License-Identifier: Apache-2.0
"""Prepare reviewable public-source edits without changing the running source tree."""
import difflib
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).resolve().parent / 'transport-staged'
changes = {}


def edit(name, before, after):
    original, value = changes.get(name, ((ROOT / name).read_text(encoding='utf-8'), None))
    value = original if value is None else value
    assert value.count(before) == 1, (name, before[:100], value.count(before))
    changes[name] = original, value.replace(before, after, 1)


edit('crates/airk-mcp/src/response.rs',
'''pub fn error_result(error: &anyhow::Error) -> rmcp::model::CallToolResult {
    let result = bare_error_result(error);
    if let Some(scope) = error.downcast_ref::<airk_core::scoped_reads::ScopeErrorObservation>() {
        return with_scope_observation(&scope.observation, result);
    }
    result
}''',
'''pub fn error_result(error: &anyhow::Error) -> rmcp::model::CallToolResult {
    typed_error_result(error).unwrap_or_else(|| {
        let raw=error.to_string();
        let mut end=raw.len().min(1024);
        while !raw.is_char_boundary(end){end-=1;}
        let mut body=serde_json::json!({"error":{
            "code":"read_failed","message":&raw[..end],"retryable":false
        }});
        if end<raw.len(){body["error"]["details_truncated"]=true.into();body["error"]["message_truncated"]=true.into();}
        let result = rmcp::model::CallToolResult::error(vec![rmcp::model::ContentBlock::text(body.to_string())]);
        match error.downcast_ref::<airk_core::scoped_reads::ScopeErrorObservation>() {
            Some(scope) => with_scope_observation(&scope.observation, result),
            None => result,
        }
    })
}
/// Preserve recognized native refusal/retry contracts at every transport boundary.
/// Unrecognized text never becomes a retry hint.
pub fn typed_error_result(error: &anyhow::Error) -> Option<rmcp::model::CallToolResult> {
    let result = typed_bare_error_result(error)?;
    Some(match error.downcast_ref::<airk_core::scoped_reads::ScopeErrorObservation>() {
        Some(scope) => with_scope_observation(&scope.observation, result),
        None => result,
    })
}''')

name = 'crates/airk-mcp/src/response.rs'
original, value = changes[name]
start = value.index('fn bare_error_result(')
end = value.index('\n#[derive(', start)
old = value[start:end]
new = old.replace('fn bare_error_result(error: &anyhow::Error) -> rmcp::model::CallToolResult {',
                  'fn typed_bare_error_result(error: &anyhow::Error) -> Option<rmcp::model::CallToolResult> {')
new = new.replace('return rmcp::model::CallToolResult::error(', 'return Some(rmcp::model::CallToolResult::error(')
# All current recognized return expressions end with ]);. The compact projection
# branch has its own same terminator, so this closes only the added Some wrapper.
new = new.replace(']);', ']));')
tail = '''    let text = airk_kernel::retrieval_retry::read_retry_hint(error)
        .and_then(|hint| serde_json::to_string(&hint).ok())
        .unwrap_or_else(|| error.to_string());
    rmcp::model::CallToolResult::error(vec![rmcp::model::ContentBlock::text(text)])
}'''
assert new.count(tail) == 1
new = new.replace(tail, '''    let hint = airk_kernel::retrieval_retry::read_retry_hint(error)?;
    Some(rmcp::model::CallToolResult::error(vec![rmcp::model::ContentBlock::text(
        serde_json::to_string(&hint).expect("scalar read retry receipt"),
    )]))
}''')
changes[name] = original, value[:start] + new + value[end:]

edit('crates/airk-mcp/src/remote.rs',
'''/// A lost outer composite exchange is uncertain; never replay its child reads.
pub fn combined_observation_failure(''',
'''/// No exact dispatch receipt exists for this failed direct exchange. The caller
/// can obtain fresh brokerage, but cannot infer that replaying the query is safe.
pub fn direct_observation_failure(code: &str) -> serde_json::Value {
    scoped_failure_envelope(serde_json::json!({"error":{
        "code":code,
        "message":"The native read could not be completed. Its submission state is unknown; obtain a fresh connection before deciding whether to repeat it.",
        "retryable":false,"query_submitted":null,
        "fresh_brokerage_required":true,"query_replay_safe":false
    }}))
}

/// A lost outer composite exchange is uncertain; never replay its child reads.
pub fn combined_observation_failure(''')

edit('crates/airk-mcp/src/remote.rs',
'''            CallToolResult::error(vec![ContentBlock::text("AIRK direct project connection failed. Retry through fresh brokerage with the same revision selector; use the service console for indexing or repair.")])''',
'''            serde_json::from_value(direct_observation_failure("direct_observation_unavailable"))
                .expect("bounded direct failure envelope")''')

edit('crates/airk-mcp/src/remote.rs',
'''        Ok(value) => value,
        Err(_) if backend.scoped_selection().is_some() => scoped_observation_failure(''',
'''        Ok(value) => value,
        Err(error) if crate::response::typed_error_result(&error).is_some() => {
            return Ok(crate::response::typed_error_result(&error)
                .expect("recognized native read failure"));
        }
        Err(_) if backend.scoped_selection().is_some() => scoped_observation_failure(''')

edit('crates/airk-mcp/src/remote.rs',
'''        Err(error) => return Err(error),
    };
    let mut result = serde_json::from_value::<CallToolResult>(value)?;''',
'''        Err(_) => direct_observation_failure("direct_observation_unavailable"),
    };
    let mut result = serde_json::from_value::<CallToolResult>(value).unwrap_or_else(|_| {
        serde_json::from_value(direct_observation_failure("direct_response_invalid"))
            .expect("bounded direct failure envelope")
    });''')

edit('crates/airk-cli/src/status_observation.rs',
'''            Err(error) => {
                Err(anyhow::Error::new(ObservationFailure(error))
                    .context("status observation failed"))
            }''',
'''            Err(error) => match airk_mcp::response::typed_error_result(error.as_ref()) {
                // Project the original typed cause before sharing hides its
                // downcast identity. No accepted query is replayed.
                Some(result) => Ok(serde_json::to_value(result)?),
                None => Err(anyhow::Error::new(ObservationFailure(error))
                    .context("status observation failed")),
            }''')

edit('crates/airk-cli/src/project_worker.rs',
'''                else { serde_json::json!({"content":[{"type":"text","text":"AIRK read-only tool failed or is unavailable."}],"isError":true}) }''',
'''                else { unscoped_worker_error(&error) }''')

def append(name, value):
    if name in changes:
        original, current = changes[name]
    elif (ROOT / name).exists():
        original = current = (ROOT / name).read_text(encoding='utf-8')
    else:
        original = current = ''
    changes[name] = original, current + value


append('crates/airk-mcp/src/response_identity_tests.rs', r'''

#[test]
fn typed_contention_and_generic_read_failures_keep_distinct_machine_readable_contracts() {
    for error in [
        anyhow::Error::new(airk_store::service::collection::SelectionContention),
        anyhow::Error::new(airk_store::quota::QuotaContention),
        anyhow::Error::new(airk_store::current::CurrentReadContention),
    ] {
        let expected = serde_json::to_value(airk_kernel::retrieval_retry::read_retry_hint(&error).unwrap()).unwrap();
        let reply = super::typed_error_result(&error.context("outer read boundary")).unwrap();
        let body: Value = serde_json::from_str(&reply.content[0].as_text().unwrap().text).unwrap();
        assert_eq!(body, expected);
        assert_eq!(body["action"], "repeat_same_request");
    }
    for text in ["selection_busy", "quota_ledger_busy", "invalid current relationship query"] {
        let error = anyhow::anyhow!(text);
        assert!(super::typed_error_result(&error).is_none());
        let reply = super::error_result(&error);
        let body: Value = serde_json::from_str(&reply.content[0].as_text().unwrap().text).unwrap();
        assert_eq!(body["error"]["code"], "read_failed");
        assert_eq!(body["error"]["retryable"], false);
        assert_eq!(body["error"]["message"], text);
    }
    let error=anyhow::anyhow!("{}tail-sentinel","🙂".repeat(2000));
    let reply=super::error_result(&error);
    let body:Value=serde_json::from_str(&reply.content[0].as_text().unwrap().text).unwrap();
    assert!(body["error"]["message"].as_str().unwrap().len()<=1024);
    assert_eq!(body["error"]["message_truncated"],true);
    assert!(!body.to_string().contains("tail-sentinel"));
}
''')

append('crates/airk-mcp/src/remote_scoped_tests.rs', r'''

struct BoundaryFailure {
    mode: AtomicUsize,
    calls: AtomicUsize,
}
impl RemoteTools for BoundaryFailure {
    fn call(&self, _: String, _: serde_json::Map<String, Value>) -> RemoteFuture<'_> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            match self.mode.load(Ordering::SeqCst) {
                0 => Err(airk_store::service::collection::SelectionContention.into()),
                1 => anyhow::bail!("private/transport/path must not leak"),
                _ => Ok(json!("invalid MCP result")),
            }
        })
    }
}

#[tokio::test]
async fn actual_remote_and_cli_boundaries_preserve_typed_retry_and_unknown_dispatch_without_replay()
-> anyhow::Result<()> {
    let backend = Arc::new(BoundaryFailure {mode: AtomicUsize::new(0), calls: AtomicUsize::new(0)});
    let bridge = Bridge {backend: backend.clone(), admission: Arc::new(Admission::default()),
        response_mode: crate::response::ResponseMode::Full};
    let (client_transport, server_transport) = tokio::io::duplex(64 * 1024);
    let (mut client, mut server) = tokio::try_join!(
        async { Ok::<_, anyhow::Error>(().serve(client_transport).await?) },
        async { Ok::<_, anyhow::Error>(bridge.serve(server_transport).await?) }
    )?;
    for mode in 0..3 {
        backend.mode.store(mode, Ordering::SeqCst);
        let before = backend.calls.load(Ordering::SeqCst);
        let mcp = client.call_tool(CallToolRequestParams::new("airk_symbols")).await?;
        let cli = call_observed(backend.as_ref(), "airk_symbols".into(), Default::default()).await?;
        assert_eq!(serde_json::to_value(&mcp)?, cli);
        assert_eq!(backend.calls.load(Ordering::SeqCst), before + 2, "each explicit caller dispatches once");
        assert_eq!(mcp.is_error, Some(true));
        let body: Value = serde_json::from_str(&mcp.content[0].as_text().unwrap().text)?;
        if mode == 0 {
            assert_eq!(body["code"], "selection_busy");
            assert_eq!(body["action"], "repeat_same_request");
            assert_eq!(body["retryable"], true);
        } else {
            assert_eq!(body["error"]["code"], if mode == 1 {"direct_observation_unavailable"} else {"direct_response_invalid"});
            assert_eq!(body["error"]["query_submitted"], Value::Null);
            assert_eq!(body["error"]["retryable"], false);
            assert_eq!(body["error"]["query_replay_safe"], false);
            assert_eq!(body["error"]["fresh_brokerage_required"], true);
            assert!(!body.to_string().contains("private/transport/path"));
        }
    }
    client.close().await?;server.close().await?;
    Ok(())
}
''')

append('crates/airk-core/src/read_arguments.rs', r'''// SPDX-License-Identifier: Apache-2.0
//! Machine-readable read-input validation shared by native transports.
#[derive(Clone, Debug)]
pub struct ReadArgumentError {
    pub code: &'static str,
    pub field: String,
    pub message: String,
}
impl ReadArgumentError {
    pub fn new(code: &'static str, field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {code, field: field.into(), message: message.into()}
    }
    pub fn receipt(&self) -> serde_json::Value {
        let (field,field_truncated)=bounded(&self.field,256);
        let (message,message_truncated)=bounded(&self.message,1024);
        let mut body=serde_json::json!({"error":{"code":self.code,"field":field,
            "message":message,"retryable":false}});
        if field_truncated||message_truncated {
            body["error"]["details_truncated"]=true.into();
            body["error"]["field_truncated"]=field_truncated.into();
            body["error"]["message_truncated"]=message_truncated.into();
        }
        body
    }
}
fn bounded(value:&str,maximum:usize)->(&str,bool){
    let mut end=value.len().min(maximum);
    while !value.is_char_boundary(end){end-=1;}
    (&value[..end],end<value.len())
}
impl std::fmt::Display for ReadArgumentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (field,field_truncated)=bounded(&self.field,256);
        let (message,message_truncated)=bounded(&self.message,1024);
        write!(formatter, "{field}: {message}{}",if field_truncated||message_truncated {" [truncated]"}else{""})
    }
}
impl std::error::Error for ReadArgumentError {}
''')
edit('crates/airk-core/src/lib.rs', 'pub mod readiness;', 'pub mod read_arguments;\npub mod readiness;')
edit('crates/airk-mcp/src/response.rs',
'''fn typed_bare_error_result(error: &anyhow::Error) -> Option<rmcp::model::CallToolResult> {''',
'''fn typed_bare_error_result(error: &anyhow::Error) -> Option<rmcp::model::CallToolResult> {
    if let Some(invalid) = error.downcast_ref::<airk_core::read_arguments::ReadArgumentError>() {
        return Some(rmcp::model::CallToolResult::error(vec![rmcp::model::ContentBlock::text(
            invalid.receipt().to_string(),
        )]));
    }''')

append('crates/airk-core/src/workbench.rs', r'''

/// Decode the one canonical request shape with precise paths for nested
/// collections before Serde's value decoder loses those locations. This adds
/// diagnostics only: semantic plan validation and budgets retain their owners.
pub fn decode_request(value: Value) -> anyhow::Result<Request> {
    use crate::read_arguments::ReadArgumentError;
    fn object<'a>(value: &'a Value, path: &str) -> anyhow::Result<&'a Map<String, Value>> {
        value.as_object().ok_or_else(|| ReadArgumentError::new(
            "invalid_arguments", path, "expected an object").into())
    }
    fn array<'a>(value: &'a Value, path: &str) -> anyhow::Result<&'a Vec<Value>> {
        value.as_array().ok_or_else(|| ReadArgumentError::new(
            "invalid_arguments", path, "expected an array").into())
    }
    fn shape_plan(plan: &Value) -> anyhow::Result<()> {
        let plan = object(plan, "plan")?;
        if let Some(steps) = plan.get("steps") {
            for (index, step) in array(steps, "plan.steps")?.iter().enumerate() {
                let path = format!("plan.steps[{index}]");
                let step = object(step, &path)?;
                if let Some(args) = step.get("args") { object(args, &format!("{path}.args"))?; }
                if let Some(bind) = step.get("bind") {
                    for (index, binding) in array(bind, &format!("{path}.bind"))?.iter().enumerate() {
                        let path = format!("{path}.bind[{index}]");
                        object(binding, &path)?;
                        serde_json::from_value::<Binding>(binding.clone()).map_err(|error|
                            ReadArgumentError::new("invalid_arguments", &path, error.to_string()))?;
                    }
                }
                serde_json::from_value::<Step>(Value::Object(step.clone())).map_err(|error|
                    ReadArgumentError::new("invalid_arguments", &path, error.to_string()))?;
            }
        }
        if let Some(selection) = plan.get("query_scope").filter(|value| !value.is_null()) {
            let selection = object(selection, "plan.query_scope")?;
            for key in ["project_ids", "index_ids"] {
                if let Some(ids) = selection.get(key) {
                    array(ids, &format!("plan.query_scope.{key}"))?;
                }
            }
        }
        for key in ["budgets", "output"] {
            if let Some(value) = plan.get(key) { object(value, &format!("plan.{key}"))?; }
        }
        Ok(())
    }
    let fields = object(&value, "request")?;
    if let Some(plan) = fields.get("plan").filter(|value| !value.is_null()) { shape_plan(plan)?; }
    if let Some(recipe) = fields.get("recipe").filter(|value| !value.is_null()) {
        let recipe = object(recipe, "recipe")?;
        if let Some(parameters) = recipe.get("parameters") { object(parameters, "recipe.parameters")?; }
    }
    for key in ["budgets", "output"] {
        if let Some(value) = fields.get(key).filter(|value| !value.is_null()) { object(value, key)?; }
    }
    serde_json::from_value(value).map_err(|error|
        ReadArgumentError::new("invalid_arguments", "request", error.to_string()).into())
}

#[cfg(test)]
#[path = "workbench_decode_tests.rs"]
mod decode_tests;
''')
edit('crates/airk-mcp/src/lib.rs',
'''                kernel.evidence_plan_with_reader(
                    serde_json::from_value(value)?,''',
'''                kernel.evidence_plan_with_reader(
                    airk_core::workbench::decode_request(value)?,''')
edit('crates/airk-cli/src/combined_frontend.rs',
'''        let request: airk_core::workbench::Request = serde_json::from_value(Value::Object(args))?;''',
'''        let request = airk_core::workbench::decode_request(Value::Object(args))?;''')
append('crates/airk-core/src/workbench_decode_tests.rs', r'''// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::read_arguments::ReadArgumentError;
use serde_json::json;

#[test]
fn decode_retains_nested_collection_paths_and_never_executes() {
    for (field, value) in [
        ("plan.steps", json!({"plan":{"steps":"one"}})),
        ("plan.steps[0].bind", json!({"plan":{"steps":[{"id":"s","tool":"status","bind":"one"}]}})),
        ("plan.steps[0].args", json!({"plan":{"steps":[{"id":"s","tool":"status","args":[]}]}})),
        ("plan.query_scope.project_ids", json!({"plan":{"query_scope":{"project_ids":"one"}}})),
        ("recipe.parameters", json!({"recipe":{"id":"definition","parameters":[]}})),
    ] {
        let mut request = json!({"schema":REQUEST_SCHEMA});
        request.as_object_mut().unwrap().extend(value.as_object().unwrap().clone());
        let error = decode_request(request).unwrap_err();
        let invalid = error.downcast_ref::<ReadArgumentError>().unwrap();
        assert_eq!(invalid.code, "invalid_arguments");
        assert_eq!(invalid.field, field);
        assert_eq!(invalid.receipt()["error"]["retryable"], false);
    }
    let request = json!({"schema":REQUEST_SCHEMA,"text":"/status"});
    assert_eq!(serde_json::to_value(decode_request(request.clone()).unwrap()).unwrap(),
        serde_json::to_value(serde_json::from_value::<Request>(request).unwrap()).unwrap());
}
''')

edit('crates/airk-mcp/src/unified.rs',
'''                                Ok(Err(_)) => crate::remote::scoped_observation_failure(
                                    selection,
                                    "scoped_status_unavailable",
                                    true,
                                ),''',
'''                                Ok(Err(error)) => crate::response::typed_error_result(&error)
                                    .map(|reply| serde_json::to_value(reply).expect("typed read receipt"))
                                    .unwrap_or_else(|| crate::remote::scoped_observation_failure(
                                        selection,"scoped_status_unavailable",true)),''')
edit('crates/airk-mcp/src/unified.rs',
'''                            Err(_) => crate::remote::scoped_observation_failure(
                                selection,
                                "scoped_observation_unavailable",
                                true,
                            ),''',
'''                            Err(error) => crate::response::typed_error_result(&error)
                                .map(|reply| serde_json::to_value(reply).expect("typed read receipt"))
                                .unwrap_or_else(|| crate::remote::scoped_observation_failure(
                                    selection,"scoped_observation_unavailable",true)),''')
edit('crates/airk-mcp/src/unified.rs',
'''                            Ok(Err(_)) => {
                                return Ok(success(
                                    json!({"version":env!("CARGO_PKG_VERSION"),"health":"unknown","project":project.binding,"index_state":"unknown","files":null,"chunks":null,"publication":null,"index_error":{"code":"status_unavailable","stage":"observation","message":"Basic status could not be observed. Call diagnostics for troubleshooting."}}),
                                ));
                            }''',
'''                            Ok(Err(error)) => {
                                if let Some(reply) = crate::response::typed_error_result(&error) {
                                    return Ok(serde_json::to_value(reply)?);
                                }
                                return Ok(success(
                                    json!({"version":env!("CARGO_PKG_VERSION"),"health":"unknown","project":project.binding,"index_state":"unknown","files":null,"chunks":null,"publication":null,"index_error":{"code":"status_unavailable","stage":"observation","message":"Basic status could not be observed. Call diagnostics for troubleshooting."}}),
                                ));
                            }''')
edit('crates/airk-mcp/src/unified.rs',
'''                        Err(error) => {
                            let code = if error.to_string().contains("service_explicitly_stopped") {''',
'''                        Err(error) => {
                            if let Some(reply) = crate::response::typed_error_result(&error) {
                                return Ok(serde_json::to_value(reply)?);
                            }
                            let code = if error.to_string().contains("service_explicitly_stopped") {''')

append('crates/airk-mcp/src/workbench.rs', r'''

#[cfg(test)]
#[tokio::test]
async fn actual_native_parse_reports_nested_input_path_without_querying_sources() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("source");
    std::fs::create_dir(&root)?;
    let kernel = airk_kernel::Kernel::open(&root, Some(directory.path().join("data")))?;
    let mut tools = crate::local::LocalTools::new(kernel).await?;
    let request = serde_json::json!({"schema":"airk.parse.request/1","mode":"prepare",
        "verification":"off","plan":{"steps":"not an array"}});
    let reply = tools.call("airk_parse".into(), request.as_object().unwrap().clone()).await?;
    assert_eq!(reply.is_error, Some(true));
    let body: serde_json::Value = serde_json::from_str(&reply.content[0].as_text().unwrap().text)?;
    assert_eq!(body["error"]["code"], "invalid_arguments");
    assert_eq!(body["error"]["field"], "plan.steps");
    assert_eq!(body["error"]["retryable"], false);
    assert_eq!(std::fs::read_dir(&root)?.count(), 0);
    tools.close().await?;
    Ok(())
}

#[cfg(test)]
#[tokio::test]
async fn actual_native_parse_bounds_oversized_malformed_field_diagnostics() -> anyhow::Result<()> {
    let directory=tempfile::tempdir()?;
    let root=directory.path().join("source");std::fs::create_dir(&root)?;
    let kernel=airk_kernel::Kernel::open(&root,Some(directory.path().join("data")))?;
    let mut tools=crate::local::LocalTools::new(kernel).await?;
    let oversized=format!("{}private-query-tail-sentinel","x".repeat(50_000));
    let mut step=serde_json::json!({"id":"s","tool":"status","args":{}});
    step[oversized]=serde_json::json!(true);
    let request=serde_json::json!({"schema":"airk.parse.request/1","verification":"off","plan":{"steps":[step]}});
    let reply=tools.call("airk_parse".into(),request.as_object().unwrap().clone()).await?;
    assert_eq!(reply.is_error,Some(true));
    let body:serde_json::Value=serde_json::from_str(&reply.content[0].as_text().unwrap().text)?;
    assert_eq!(body["error"]["code"],"invalid_arguments");
    assert_eq!(body["error"]["field"],"plan.steps[0]");
    assert_eq!(body["error"]["message_truncated"],true);
    assert_eq!(body["error"]["field_truncated"],false);
    assert!(!serde_json::to_string(&reply)?.contains("private-query-tail-sentinel"));
    assert!(serde_json::to_vec(&reply)?.len()<8192);
    assert_eq!(std::fs::read_dir(&root)?.count(),0);
    tools.close().await?;
    Ok(())
}
''')

name = 'crates/airk-cli/src/status_observation.rs'
original, current = changes[name]
marker = '''    #[tokio::test]
    async fn panic_fans_out_then_next_owned_observation_can_succeed()'''
assert current.count(marker) == 1
test = r'''    #[tokio::test]
    async fn shared_status_typed_admission_retains_retry_without_replaying_observer() -> Result<()> {
        let project = "ab".repeat(32).parse()?;
        let owner = Arc::new(StatusOwner::default());
        let key = Key {project, source:None, expires:None, args:serde_json::json!({})};
        let (entered, started) = tokio::sync::oneshot::channel();
        let (release, wait) = tokio::sync::oneshot::channel();
        let first = owner.observe(key.clone(), None, |_| async move {
            let _ = entered.send(());let _ = wait.await;
            Err(anyhow::Error::new(airk_store::quota::QuotaContention).context("private ledger admission"))
        });
        tokio::pin!(first);
        tokio::select! {result=&mut first=>anyhow::bail!("premature result: {result:?}"),result=started=>{result?;}}
        let second = owner.observe(key, None, |_| async { panic!("duplicate typed-status producer") });
        tokio::pin!(second);
        assert!(std::future::poll_fn(|cx| std::task::Poll::Ready(second.as_mut().poll(cx))).await.is_pending());
        release.send(()).map_err(|_| anyhow::anyhow!("driver disappeared"))?;
        let (first,second)=tokio::join!(first,second);
        assert_eq!(first.as_ref().unwrap(),second.as_ref().unwrap());
        let reply=first?;
        let body:serde_json::Value=serde_json::from_str(reply["content"][0]["text"].as_str().unwrap())?;
        assert_eq!(body["code"],"selection_busy");
        assert_eq!(body["retryable"],true);
        assert_eq!(body["action"],"repeat_same_request");
        assert!(!body.to_string().contains("private ledger"));
        owner.shutdown().await?;
        Ok(())
    }

'''
changes[name] = original, current.replace(marker, test + marker, 1)

edit('crates/airk-mcp/src/execution.rs',
'''            return Err(error("AIRK server is closing"));''',
'''            return Err(admission_error(false));''')
edit('crates/airk-mcp/src/execution.rs',
'''        Err(error(
            "AIRK request queue is full or timed out; retry after outstanding requests complete",
        ))''',
'''        Err(admission_error(true))''')
edit('crates/airk-mcp/src/execution.rs',
'''fn error(message: &str) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(message)])
}''',
'''fn error(message: &str) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(serde_json::json!({"error":{
        "code":"read_failed","message":message,"retryable":false
    }}).to_string())])
}
fn admission_error(busy: bool) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(serde_json::json!({"error":{
        "code":if busy {"direct_query_admission"} else {"request_draining"},
        "reason":if busy {"busy"} else {"draining"},
        "message":if busy {"AIRK request queue is full or timed out; retry after outstanding requests complete"}
            else {"AIRK server is closing"},
        "query_submitted":false,"retryable":busy,
        "retry_after_ms":if busy {Some(25)} else {None},
        "fresh_brokerage_required":!busy
    }}).to_string())])
}''')
append('crates/airk-mcp/src/execution.rs', r'''

#[cfg(test)]
#[test]
fn admission_rejections_are_typed_and_generic_errors_never_authorize_replay() {
    for busy in [false,true] {
        let reply = admission_error(busy);
        let body:serde_json::Value=serde_json::from_str(&reply.content[0].as_text().unwrap().text).unwrap();
        assert_eq!(body["error"]["query_submitted"],false);
        assert_eq!(body["error"]["retryable"],busy);
        assert_eq!(body["error"]["fresh_brokerage_required"],!busy);
        if busy {assert_eq!(body["error"]["retry_after_ms"],25);}
        else {assert_eq!(body["error"]["code"],"request_draining");}
    }
    let reply=error("selection_busy");
    let body:serde_json::Value=serde_json::from_str(&reply.content[0].as_text().unwrap().text).unwrap();
    assert_eq!(body["error"]["code"],"read_failed");
    assert_eq!(body["error"]["retryable"],false);
}
''')

edit('crates/airk-cli/src/project_worker.rs',
'''/// Preserve the direct transport's native/MCP envelope distinction while using
/// the same bounded, typed refusal factory as canonical MCP reads.''',
'''/// This worker boundary previously exposed only a fixed public failure. Keep
/// unknown internal paths/content private while retaining recognized receipts.
fn unscoped_worker_error(error: &anyhow::Error) -> serde_json::Value {
    if let Some(reply) = airk_mcp::response::typed_error_result(error) {
        return serde_json::to_value(reply).expect("typed worker failure receipt");
    }
    let body = serde_json::json!({"error":{"code":"read_failed",
        "message":"AIRK read-only tool failed or is unavailable.","retryable":false}});
    serde_json::json!({"content":[{"type":"text","text":body.to_string()}],"structuredContent":body,"isError":true})
}

/// Preserve the direct transport's native/MCP envelope distinction while using
/// the same bounded, typed refusal factory as canonical MCP reads.''')
append('crates/airk-cli/src/project_worker_response_tests.rs',r'''

#[test]
fn actual_unscoped_worker_boundary_redacts_unknown_internal_failure_and_preserves_typed_retry() -> Result<()> {
    let sentinel="private/source/secret-content-sentinel";
    let reply=unscoped_worker_error(&anyhow::anyhow!(sentinel));
    assert_eq!(reply["isError"],true);
    assert_eq!(body(&reply)["error"]["code"],"read_failed");
    assert_eq!(body(&reply)["error"]["retryable"],false);
    assert!(!reply.to_string().contains(sentinel));
    let typed=anyhow::Error::new(airk_store::quota::QuotaContention).context(sentinel);
    let reply=unscoped_worker_error(&typed);
    assert_eq!(body(&reply)["code"],"selection_busy");
    assert_eq!(body(&reply)["action"],"repeat_same_request");
    assert!(!reply.to_string().contains(sentinel));
    Ok(())
}
''')
edit('crates/airk-mcp/src/response.rs',
'''/// Preserve recognized native refusal/retry contracts at every transport boundary.
/// Unrecognized text never becomes a retry hint.''',
'''/// A trusted internal owner may retain a recognized typed receipt before an
/// Arc/shared error wrapper obscures the original downcast identity. There is
/// deliberately no JSON/request constructor or Deserialize implementation.
#[derive(Debug)]
pub struct PreservedReadFailure(rmcp::model::CallToolResult);
impl PreservedReadFailure {
    pub fn from_error(error: &anyhow::Error) -> Option<Self> {
        typed_error_result(error).map(Self)
    }
}
impl std::fmt::Display for PreservedReadFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("native read refused; inspect its complete typed receipt")
    }
}
impl std::error::Error for PreservedReadFailure {}

/// Preserve recognized native refusal/retry contracts at every transport boundary.
/// Unrecognized text never becomes a retry hint.''')
edit('crates/airk-mcp/src/response.rs',
'''pub fn typed_error_result(error: &anyhow::Error) -> Option<rmcp::model::CallToolResult> {
    let result = typed_bare_error_result(error)?;''',
'''pub fn typed_error_result(error: &anyhow::Error) -> Option<rmcp::model::CallToolResult> {
    let result = match error.downcast_ref::<PreservedReadFailure>() {
        Some(preserved) => preserved.0.clone(),
        None => typed_bare_error_result(error)?,
    };''')
edit('crates/airk-cli/src/unified_mcp.rs',
'''            } else {
                anyhow::Error::new(SharedReadinessError(error))
            }
        })''',
'''            } else {
                // This is the original typed cause, before the sharing wrapper
                // intentionally hides its root to preserve display chains.
                let preserved = airk_mcp::response::PreservedReadFailure::from_error(error.as_ref());
                let wrapped = anyhow::Error::new(SharedReadinessError(error));
                match preserved {
                    Some(receipt) => wrapped.context(receipt),
                    None => wrapped,
                }
            }
        })''')
edit('crates/airk-mcp/src/unified.rs',
'''                            Ok(Err(error)) => {
                                return Ok(crate::remote::scoped_setup_failure(
                                    selection,''',
'''                            Ok(Err(error)) => {
                                if let Some(reply) = crate::response::typed_error_result(&error) {
                                    return Ok(serde_json::to_value(reply)?);
                                }
                                return Ok(crate::remote::scoped_setup_failure(
                                    selection,''')
edit('crates/airk-cli/src/unified_mcp.rs',
'''                        Ok(Err(error)) => {
                            return Ok(combined_status_unavailable(
                                selection,''',
'''                        Ok(Err(error)) => {
                            if let Some(reply) = airk_mcp::response::typed_error_result(&error) {
                                return Ok(serde_json::to_value(reply)?);
                            }
                            return Ok(combined_status_unavailable(
                                selection,''')

name='crates/airk-cli/src/unified_mcp.rs'
original,current=changes[name]
marker='''    #[test]
    fn shared_error_preserves_exact_plain_and_nested_chains()'''
assert current.count(marker)==1
new=r'''    #[tokio::test]
    async fn readiness_prepare_and_ensure_modes_preserve_known_typed_cause_without_text_guessing() -> Result<()> {
        for status in [false,true] {
            let ready=RuntimeReadiness::default();
            let error=ready.run_inner_status(
                |_|async {Err(airk_store::service::collection::SelectionContention.into())},false,status).await.unwrap_err();
            let reply=airk_mcp::response::typed_error_result(&error).context("typed readiness receipt lost")?;
            let body:Value=serde_json::from_str(&reply.content[0].as_text().unwrap().text)?;
            assert_eq!(body["code"],"selection_busy");
            assert_eq!(body["action"],"repeat_same_request");
            assert_eq!(body["retryable"],true);
            let error=ready.run_inner_status(
                |_|async {anyhow::bail!("service incarnation selection is busy; retry after 25 ms")},false,status).await.unwrap_err();
            assert!(airk_mcp::response::typed_error_result(&error).is_none(),"lookalike text is never retry authority");
            ready.shutdown().await?;
        }
        Ok(())
    }

'''
changes[name]=original,current.replace(marker,new+marker,1)

append('crates/airk-mcp/src/scoped_observation_tests.rs',r'''

#[tokio::test]
async fn preserved_read_failure_retains_current_scope_fence_and_rejects_foreign_observation() -> Result<()> {
    let temporary=tempfile::tempdir()?;
    let root=temporary.path().join("source");std::fs::create_dir_all(root.join("public"))?;
    std::fs::write(root.join("public/a.rs"),"pub struct Public;\n")?;
    let kernel=Kernel::open(&root,Some(temporary.path().join("data")))?;
    kernel.configure_vectors(false)?;kernel.index()?;
    let frozen=authority(&kernel)?;
    let native=kernel.with_scoped_reads(frozen)?;
    let first=native.scope_observation().unwrap().clone();
    let typed=anyhow::Error::new(airk_store::quota::QuotaContention).context(
        airk_core::scoped_reads::ScopeErrorObservation{observation:first.clone(),message:"first native owner".into()});
    let preserved=response::PreservedReadFailure::from_error(&typed).unwrap();
    let same=typed.context(preserved);
    assert_eq!(payload(&response::error_result(&same))?["scope_observation"],serde_json::to_value(&first)?);
    let mut current=first.clone();current.fingerprint=Sha256Digest::from_bytes([98;32]);current.grant.revision=CatalogRevision::INITIAL.next()?;
    let changed=same.context(airk_core::scoped_reads::ScopeErrorObservation{observation:current.clone(),message:"replacement native owner".into()});
    let reply=response::error_result(&changed);
    let body=payload(&reply)?;
    assert_eq!(body["scope_observation"],serde_json::to_value(current)?);
    assert_eq!(body["error"]["code"],"scoped_read_unavailable");
    assert_eq!(body["error"]["retryable"],false);
    assert_eq!(reply.is_error,Some(true));
    Ok(())
}
''')

edit('crates/airk-cli/src/project_supervisor.rs',
'''struct ChildWorker {''',
'''/// Explicit pre-query setup admission. Accepted catalog exchanges and unknown
/// worker outcomes never construct this type.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ConnectionSetupPending {
    Queue,
    Draining,
    ResidentCapacity,
    RestartBackoff,
}
impl ConnectionSetupPending {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::Queue => "catalog_connection_busy",
            Self::Draining => "previous_workers_draining",
            Self::ResidentCapacity => "project_capacity_busy",
            Self::RestartBackoff => "project_restart_backoff",
        }
    }
}
impl std::fmt::Display for ConnectionSetupPending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.code())
    }
}
impl std::error::Error for ConnectionSetupPending {}

struct ChildWorker {''')
edit('crates/airk-cli/src/project_supervisor.rs',
'''ensure!(!worker.exiting, "project owner resources are draining");''',
'''ensure!(!worker.exiting, ConnectionSetupPending::Draining);''')
edit('crates/airk-cli/src/project_supervisor.rs',
'''        drains.for_project(project).is_empty(),
        "project owner resources are draining"''',
'''        drains.for_project(project).is_empty(),
        ConnectionSetupPending::Draining''')
edit('crates/airk-cli/src/project_supervisor.rs',
'''ensure!(resources.may_start_resident(), "project capacity busy");''',
'''ensure!(resources.may_start_resident(), ConnectionSetupPending::ResidentCapacity);''')
edit('crates/airk-cli/src/project_supervisor.rs',
'''    ensure!(
        backoff
            .get(&project)
            .is_none_or(|(until, attempts)| *attempts < 3 && Instant::now() >= *until),
        "project restart backoff"
    );''',
'''    if let Some((until,attempts))=backoff.get(&project) {
        ensure!(*attempts<3,"project restart circuit exhausted");
        ensure!(Instant::now()>=*until,ConnectionSetupPending::RestartBackoff);
    }''')
edit('crates/airk-cli/src/catalog_control.rs',
'''        sender
            .try_send(Envelope::Catalog {''',
'''        let connection_setup=matches!(&operation,Operation::Connect{..});
        sender
            .try_send(Envelope::Catalog {''')
edit('crates/airk-cli/src/catalog_control.rs',
'''            .map_err(|_| anyhow::anyhow!("catalog supervisor busy"))?;''',
'''            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) if connection_setup => anyhow::Error::new(ConnectionSetupPending::Queue),
                _ => anyhow::anyhow!("catalog supervisor unavailable"),
            })?;''')
edit('crates/airk-cli/src/catalog_control.rs',
'''                !lifetime.is_zero() && lifetime <= Duration::from_secs(300) && may_allocate,
                "native scoped connection unavailable"
            );''',
'''                !lifetime.is_zero() && lifetime <= Duration::from_secs(300),
                "invalid native scoped connection budget"
            );
            ensure!(may_allocate, ConnectionSetupPending::Draining);''')
edit('crates/airk-cli/src/service_management.rs',
'''        if self.sender.as_ref().is_none_or(|queue| {
            queue
                .try_send(Envelope::Request(
                    Box::new(method),
                    ManagementReply::Scoped(sender),
                ))
                .is_err()
        }) {
            self.pending.fetch_sub(1, Ordering::AcqRel);
            anyhow::bail!("catalog_connection_busy");
        }''',
'''        let sent=self.sender.as_ref().map(|queue|queue.try_send(Envelope::Request(
            Box::new(method),ManagementReply::Scoped(sender))));
        match sent {
            Some(Ok(()))=>{},
            Some(Err(std::sync::mpsc::TrySendError::Full(_)))=>{
                self.pending.fetch_sub(1,Ordering::AcqRel);
                return Err(crate::project_supervisor::ConnectionSetupPending::Queue.into());
            },
            _=>{
                self.pending.fetch_sub(1,Ordering::AcqRel);
                anyhow::bail!("catalog_connection_owner_closed");
            },
        }''')
edit('crates/airk-cli/src/service_runtime.rs',
'''    } else if error.is::<airk_core::scoped_reads::ScopeReadRefused>() {''',
'''    } else if let Some(pending)=error.downcast_ref::<crate::project_supervisor::ConnectionSetupPending>() {
        pending.code()
    } else if error.is::<airk_core::scoped_reads::ScopeReadRefused>() {''')
# Both initial health and ConnectScoped failure branches classify only native
# fixed pre-query codes; unknown/refused/protocol failures remain terminal.
name='crates/airk-cli/src/project_client.rs'
original,value=changes.get(name,((ROOT/name).read_text(encoding='utf-8'),None));value=original if value is None else value
before='''| "previous_workers_draining" | "catalog_revision_changed" | "catalog_clock_pending"'''
assert value.count(before)==2
value=value.replace(before,before+''' | "catalog_connection_busy" | "project_capacity_busy" | "project_restart_backoff"''')
changes[name]=original,value
edit('crates/airk-cli/src/project_client.rs',
'''                        | "catalog_clock_pending"
                )''',
'''                        | "catalog_clock_pending"
                        | "catalog_connection_busy"
                        | "project_capacity_busy"
                        | "project_restart_backoff"
                )''')
edit('crates/airk-mcp/src/remote.rs',
'''    if let Some(response) = native_control_response {
        body["error"]["native_control_response"] = response;
    }
    scoped_failure_envelope(body)''',
'''    if let Some(response) = native_control_response {
        body["error"]["native_control_response"] = response;
    }
    if retryable {
        body["error"]["retry_after_ms"]=100.into();
        body["error"]["fresh_brokerage_required"]=true.into();
    }
    scoped_failure_envelope(body)''')
append('crates/airk-cli/src/service_runtime.rs',r'''

#[cfg(test)]
mod connection_setup_contract_tests {
    use super::*;
    #[test]
    fn only_known_typed_pre_query_pending_errors_become_busy_codes() {
        use crate::project_supervisor::ConnectionSetupPending as Pending;
        for pending in [Pending::Queue,Pending::Draining,Pending::ResidentCapacity,Pending::RestartBackoff] {
            let expected=pending.code();
            let error=anyhow::Error::new(pending).context("connection admission");
            assert_eq!(connection_failure_code(&error,"scoped_connection_unavailable"),expected);
            assert_eq!(connection_failure_code(&anyhow::anyhow!(expected),"scoped_connection_unavailable"),"scoped_connection_unavailable");
        }
        for message in ["catalog native outcome unknown","native catalog exchange outcome unknown","project owner role is draining","project restart circuit exhausted","native scoped worker role changed"] {
            assert_eq!(connection_failure_code(&anyhow::anyhow!(message),"scoped_connection_unavailable"),"scoped_connection_unavailable");
        }
    }
}
''')

edit('crates/airk-cli/src/catalog_control.rs',
'''    #[test]
    fn actual_actor_handoff_refuses_close_between_connection_build_and_send()''',
'''    #[test]
    fn actual_connection_queue_refusal_is_retryable_only_before_acceptance() -> Result<()> {
        let (gate,worker,_,_)=fixture()?;
        let (sender,receiver)=mpsc::sync_channel(0);
        let owner=Arc::new(sender);
        let handle=CatalogControlHandle{sender:Arc::downgrade(&owner),gate,source:None};
        let connect=||Operation::Connect{selection:ConnectionSelection::Legacy(NativeLegacyAuthoritySelection{
            namespace:Incarnation::from_bytes([4;16]),host:airk_core::identity::Sha256Digest::from_bytes([7;32]),
            catalog_revision:airk_core::index_catalog::CatalogRevision::INITIAL,binding:worker.project}),lifetime:Duration::from_secs(300)};
        let rejected=handle.call(connect()).err().context("full queue must reject")?;
        assert!(rejected.is::<ConnectionSetupPending>());
        assert_eq!(rejected.downcast_ref::<ConnectionSetupPending>().unwrap().code(),"catalog_connection_busy");
        let other=handle.call(Operation::Attach).err().context("full queue must reject Attach")?;
        assert!(!other.is::<ConnectionSetupPending>(),"other actor operations gain no replay permission");
        drop(receiver);
        let closed=handle.call(connect()).err().context("closed queue must reject")?;
        assert!(!closed.is::<ConnectionSetupPending>(),"a disconnected owner is not known busy");
        Ok(())
    }
    #[test]
    fn actual_actor_handoff_refuses_close_between_connection_build_and_send()''')
edit('crates/airk-cli/src/service_management.rs',
'''    #[test]
    fn catalog_reads_do_not_activate_legacy_registry()''',
'''    #[tokio::test]
    async fn actual_connection_queue_distinguishes_full_from_closed_before_acceptance() -> Result<()> {
        let (sender,receiver)=sync_channel(0);
        let owner=Management{sender:Some(Arc::new(sender)),thread:None,pending:Arc::new(AtomicUsize::new(0)),
            plans:Arc::new(AtomicBool::new(false)),missing:Arc::new(AtomicUsize::new(0))};
        let request=||ControlMethod::Connect{project:airk_core::identity::Sha256Digest::from_bytes([3;32])};
        let full=owner.request_connection(request()).await.err().expect("full queue must reject");
        assert!(full.is::<crate::project_supervisor::ConnectionSetupPending>());
        assert_eq!(owner.pending.load(Ordering::Acquire),0);
        drop(receiver);
        let closed=owner.request_connection(request()).await.err().expect("closed queue must reject");
        assert!(!closed.is::<crate::project_supervisor::ConnectionSetupPending>());
        assert_eq!(owner.pending.load(Ordering::Acquire),0);
        Ok(())
    }
    #[test]
    fn catalog_reads_do_not_activate_legacy_registry()''')
edit('crates/airk-mcp/src/unified_scoped_tests.rs',
'''        assert_eq!(value["error"]["query_submitted"], submitted);
        assert_eq!(value["requested_scope"]["confirmed"], false);''',
'''        assert_eq!(value["error"]["query_submitted"], submitted);
        if submitted==false {
            assert_eq!(value["error"]["retry_after_ms"],100);
            assert_eq!(value["error"]["fresh_brokerage_required"],true);
        } else {
            assert_eq!(value["error"]["query_replay_safe"],false);
        }
        assert_eq!(value["requested_scope"]["confirmed"], false);''')

OUT.mkdir(parents=True, exist_ok=True)
patch = []
manifest = []
for name, (original, value) in changes.items():
    destination = OUT / name
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(value, encoding='utf-8', newline='\n')
if '--format' in sys.argv:
    formatter = 'C:/Users/tdotn/.rustup/toolchains/1.98.0-x86_64-pc-windows-msvc/bin/rustfmt.exe'
    subprocess.run([formatter, '--edition', '2024', '--config', 'skip_children=true',
                    *(str(OUT / name) for name in changes if name.endswith('.rs'))], check=True)
for name, (original, _value) in changes.items():
    destination = OUT / name
    value = destination.read_text(encoding='utf-8')
    patch.extend(difflib.unified_diff(original.splitlines(True), value.splitlines(True),
                                    fromfile=('a/' + name) if original else '/dev/null', tofile='b/' + name))
    manifest.append({'path': name, 'original_sha256': hashlib.sha256((ROOT / name).read_bytes()).hexdigest() if original else None,
                     'staged_sha256': hashlib.sha256(destination.read_bytes()).hexdigest()})
(OUT.parent / 'transport.patch').write_text(''.join(patch), encoding='utf-8', newline='\n')
(OUT.parent / 'transport-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf-8')
print(json.dumps({'files': len(changes), 'patch': str(OUT.parent / 'transport.patch')}))
