// SPDX-License-Identifier: Apache-2.0
//! Actual I3 admission and envelopes over complete native-owner packets.
use super::*;

#[tokio::test]
async fn ui_pilot_i3_semantic_cases() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/split/ui_pilot/native_reads.json");
    let mut saved: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(saved["schema"], "airk.split.ui-pilot.reads/1");
    let mut projections = Vec::new();
    for case in saved["cases"].as_array().unwrap() {
        let response: ControlResponse =
            serde_json::from_value(case["native_response"].clone()).unwrap();
        let (plane, sent, _) = scripted_plane(vec![Some(response)]);
        let (cookie, csrf) = sign_in(&plane).await;
        let (status, body) = envelope(
            &plane,
            api_call(
                &cookie,
                &csrf,
                case["request"]["admin_operation"].as_str().unwrap(),
                &case["request"]["admin_body"].to_string(),
            ),
        )
        .await;
        if matches!(
            case["name"].as_str(),
            Some("usage_days_31_refused" | "usage_native_failure")
        ) {
            assert_eq!(status, 409, "{} {body}", case["name"]);
            assert_eq!(body["error"]["code"], "usage_read_failed");
            assert_eq!(body["error"]["native"], case["native_response"]);
        } else {
            assert_eq!(status, 200, "{} {body}", case["name"]);
            assert_eq!(
                body["result"], case["native_response"],
                "I3 must retain complete native response"
            );
        }
        assert_eq!(
            *sent.lock().unwrap(),
            vec![case["request"]["admin_body"].clone()]
        );
        let serialized = body.to_string();
        for secret in [&cookie, &csrf] {
            assert!(
                !serialized.contains(secret),
                "native-held session material escaped"
            );
        }
        assert!(body.get("request").is_none());
        assert!(body.get("review").is_none());
        projections.push(json!({"status":status,"body":body}));
    }
    assert_eq!(projections.len(), 12);
    let mut refusals = Vec::new();
    for (name, surface, field) in [
        ("help_caller_refused", "help", "caller"),
        ("help_request_key_refused", "help", "request_key"),
        ("usage_caller_refused", "usage", "caller"),
        ("usage_request_key_refused", "usage", "request_key"),
        ("help_project_selection_refused", "help", "selection"),
    ] {
        let case = saved["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["surface"] == surface)
            .unwrap();
        let mut request = case["request"]["admin_body"].clone();
        let key = if surface == "help" {
            "tools_call"
        } else {
            "usage_document"
        };
        let sentinel = "pilot-forbidden-capability-sentinel";
        request[key][field] = if field == "selection" {
            json!({"project":{"binding":"4b".repeat(32)}})
        } else {
            json!(sentinel)
        };
        let (plane, sent, _) = scripted_plane(vec![]);
        let (cookie, csrf) = sign_in(&plane).await;
        let operation = case["request"]["admin_operation"].as_str().unwrap();
        let (status, body) = envelope(
            &plane,
            api_call(&cookie, &csrf, operation, &request.to_string()),
        )
        .await;
        assert_eq!(status, 400, "{name} {body}");
        assert!(
            sent.lock().unwrap().is_empty(),
            "forbidden packet reached native dispatch"
        );
        for secret in [sentinel, cookie.as_str(), csrf.as_str()] {
            assert!(!body.to_string().contains(secret));
        }
        refusals.push(json!({"name":name,"admin_operation":operation,"admin_body":request,"status":status,"body":body,"dispatched":0}));
    }
    if std::env::var_os("AIRK_WRITE_UI_PILOT_I3").as_deref() == Some(std::ffi::OsStr::new("1")) {
        for (case, projection) in saved["cases"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .zip(&projections)
        {
            case["i3"] = projection.clone();
        }
        saved["request_refusals"] = json!(refusals);
        std::fs::write(&path, serde_json::to_string_pretty(&saved).unwrap() + "\n").unwrap();
    }
    for (case, projection) in saved["cases"].as_array().unwrap().iter().zip(projections) {
        assert_eq!(case["i3"], projection, "I3 drift {}", case["name"]);
    }
    assert_eq!(saved["request_refusals"], json!(refusals));
}
