// SPDX-License-Identifier: Apache-2.0
//! Canonical Help inputs/results, emitted by the native owner before UI adapters.
use airk_core::service::{ControlMethod, ControlResponse, ToolResponseMode, ToolSelection};
use serde_json::{Value, json};

#[test]
fn ui_pilot_help_native_cases() {
    let mut cases = Vec::new();
    for (name, args) in [
        ("help_landing", json!({"start_line":1,"max_bytes":32768})),
        (
            "help_topic",
            json!({"topic":"overview","start_line":1,"max_bytes":32768}),
        ),
        (
            "help_section",
            json!({"topic":"overview","start_line":2,"max_bytes":32768}),
        ),
        (
            "help_unknown_topic",
            json!({"topic":"no-such-pilot-topic","start_line":1,"max_bytes":32768}),
        ),
        (
            "help_bad_line",
            json!({"topic":"overview","start_line":100000,"max_bytes":32768}),
        ),
        ("help_no_project", json!({"start_line":1,"max_bytes":32768})),
        (
            "help_unknown_fields",
            json!({"topic":"overview","start_line":1,"max_bytes":32768}),
        ),
    ] {
        let mut result = serde_json::to_value(crate::help::call(args.clone())).unwrap();
        if name == "help_unknown_fields" {
            let mut body: Value =
                serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
            body["future_native"] = json!({"opaque":["retained",{"supported":true}],"qualifier":"unknown fields survive"});
            result["content"][0]["text"] = body.to_string().into();
        }
        let response = ControlResponse::ToolResult { result };
        let method = ControlMethod::ToolsCall {
            read_attempt: None,
            tool: "airk_help".into(),
            arguments: args.as_object().unwrap().clone(),
            selection: ToolSelection::Unselected,
            response_mode: ToolResponseMode::Terse,
            usage_client: airk_core::usage::ClientFamily::Unknown,
        };
        cases.push(json!({"name":name,"surface":"help","request":{
            "cli_args":["tools","call","airk_help","--args",args.to_string(),"--output","json"],
            "admin_operation":"search.call","admin_body":serde_json::to_value(method).unwrap()},
            "native_response":serde_json::to_value(response).unwrap()}));
    }
    let names = cases
        .iter()
        .map(|case| case["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        [
            "help_landing",
            "help_topic",
            "help_section",
            "help_unknown_topic",
            "help_bad_line",
            "help_no_project",
            "help_unknown_fields"
        ]
    );
    for case in &cases {
        let response: ControlResponse =
            serde_json::from_value(case["native_response"].clone()).unwrap();
        let ControlResponse::ToolResult { result } = response else {
            panic!("Help must retain its complete native tool envelope")
        };
        let body: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        match case["name"].as_str().unwrap() {
            "help_unknown_topic" => assert_eq!(body["error"]["code"], "unknown_help_topic"),
            "help_bad_line" => assert_eq!(body["error"]["code"], "help_start_line_out_of_range"),
            "help_section" => assert_eq!(body["start_line"], 2),
            "help_unknown_fields" => assert_eq!(body["future_native"]["opaque"][0], "retained"),
            _ => assert_ne!(result["isError"], true),
        }
    }
    let value = json!({"schema":"airk.split.ui-pilot.native-payloads/1","owner":"airk-mcp::help::call",
        "qualification":"Canonical native Help. Unknown-field control is an explicit post-emission extension; no live client or installed transport claim.","cases":cases});
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/split/ui_pilot/help_native.json");
    if std::env::var_os("AIRK_WRITE_UI_PILOT_HELP").as_deref() == Some(std::ffi::OsStr::new("1")) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string_pretty(&value).unwrap() + "\n").unwrap();
    }
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(
        saved, value,
        "regenerate with AIRK_WRITE_UI_PILOT_HELP=1 and review native Help changes"
    );
}
