// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_mcp::remote::RemoteFuture;
use serde_json::json;

/// Echoes the arguments it received as the envelope's structured content.
struct Echo;
impl RemoteTools for Echo {
    fn call(&self, _tool: String, arguments: Map<String, Value>) -> RemoteFuture<'_> {
        Box::pin(async move {
            Ok(airk_mcp::unified::success(
                json!({ "arguments": arguments }),
            ))
        })
    }
}

fn object(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

async fn received(tool: &str, arguments: Value, mode: ResponseMode) -> Value {
    let reply = call(&Echo, tool.into(), object(arguments), mode)
        .await
        .unwrap();
    reply["structuredContent"]["arguments"].clone()
}

#[tokio::test]
async fn the_response_mode_is_inserted_unless_named_or_not_taken() {
    assert_eq!(
        received("airk_search", json!({"query": "q"}), ResponseMode::Full).await,
        json!({"query": "q", "response_mode": "full"})
    );
    assert_eq!(
        received("airk_search", json!({}), ResponseMode::Terse).await,
        json!({"response_mode": "terse"})
    );
    assert_eq!(
        received(
            "airk_search",
            json!({"response_mode": "terse"}),
            ResponseMode::Full
        )
        .await,
        json!({"response_mode": "terse"})
    );
    for tool in ["query_scope", "airk_query_scope"] {
        assert_eq!(
            received(tool, json!({"view": "projects"}), ResponseMode::Full).await,
            json!({"view": "projects"})
        );
    }
}

#[test]
fn an_envelope_needs_an_object_with_a_content_array() {
    assert!(require_envelope(&json!({"content": []})).is_ok());
    for invalid in [json!([]), json!({}), json!({"content": {}}), json!(null)] {
        assert_eq!(
            require_envelope(&invalid).unwrap_err().to_string(),
            "invalid native tool response"
        );
    }
}
