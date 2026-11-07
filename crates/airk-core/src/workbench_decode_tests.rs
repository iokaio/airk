// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::read_arguments::ReadArgumentError;
use serde_json::json;

#[test]
fn decode_retains_nested_collection_paths_and_never_executes() {
    for (field, value) in [
        ("plan.steps", json!({"plan":{"steps":"one"}})),
        (
            "plan.steps[0].bind",
            json!({"plan":{"steps":[{"id":"s","tool":"status","bind":"one"}]}}),
        ),
        (
            "plan.steps[0].args",
            json!({"plan":{"steps":[{"id":"s","tool":"status","args":[]}]}}),
        ),
        (
            "plan.query_scope.project_ids",
            json!({"plan":{"query_scope":{"project_ids":"one"}}}),
        ),
        (
            "recipe.parameters",
            json!({"recipe":{"id":"definition","parameters":[]}}),
        ),
    ] {
        let mut request = json!({"schema":REQUEST_SCHEMA});
        request
            .as_object_mut()
            .unwrap()
            .extend(value.as_object().unwrap().clone());
        let error = decode_request(request).unwrap_err();
        let invalid = error.downcast_ref::<ReadArgumentError>().unwrap();
        assert_eq!(invalid.code, "invalid_arguments");
        assert_eq!(invalid.field, field);
        assert_eq!(invalid.receipt()["error"]["retryable"], false);
    }
    let request = json!({"schema":REQUEST_SCHEMA,"text":"/status"});
    assert_eq!(
        serde_json::to_value(decode_request(request.clone()).unwrap()).unwrap(),
        serde_json::to_value(serde_json::from_value::<Request>(request).unwrap()).unwrap()
    );
}
