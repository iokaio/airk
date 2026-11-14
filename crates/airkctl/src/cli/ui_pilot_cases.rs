// SPDX-License-Identifier: Apache-2.0
//! Actual I2 formatter projections of Rust-owned complete native packets.
use super::*;
use serde_json::json;

#[test]
fn ui_pilot_cli_semantic_cases() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/split/ui_pilot");
    let mut cases = Vec::new();
    for filename in ["usage_native.json", "help_native.json"] {
        let source: Value =
            serde_json::from_str(&std::fs::read_to_string(directory.join(filename)).unwrap())
                .unwrap();
        assert_eq!(source["schema"], "airk.split.ui-pilot.native-payloads/1");
        for original in source["cases"].as_array().unwrap() {
            let mut case = original.clone();
            let args = case["request"]["cli_args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect::<Vec<_>>();
            Cli::try_parse_from(std::iter::once("airkctl").chain(args.iter().copied())).unwrap();
            let mut stdin = std::io::empty();
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let mut io = Io {
                stdin: &mut stdin,
                stdout: &mut stdout,
                stderr: &mut stderr,
                full_responses: false,
                window: None,
            };
            let mut cx = Context {
                output: Output::Json,
                io: &mut io,
            };
            let response: ControlResponse =
                serde_json::from_value(case["native_response"].clone()).unwrap();
            let code = if case["surface"] == "usage" {
                usage_document_response(&mut cx, response)
            } else {
                let selection = Selection {
                    control: Some(PathBuf::from("fixture-control")),
                    project: None,
                    catalog: None,
                    combined: None,
                    project_ids: None,
                    index_ids: None,
                    member_availability: None,
                    root: PathBuf::from("."),
                    data_dir: None,
                    collection: "fixture".into(),
                };
                let (control, selection) = online_admission::with_probe(
                    |_| Ok(()),
                    || resolve_call(&mut cx, &selection, "airk_help"),
                )
                .unwrap();
                assert_eq!(control, PathBuf::from("fixture-control"));
                assert!(matches!(selection, ToolSelection::Unselected));
                for forbidden in [
                    "--root",
                    "--service-project",
                    "--catalog-session",
                    "--combined-session",
                ] {
                    assert!(!args.contains(&forbidden));
                }
                tool_call_response(&mut cx, response, true)
            };
            let stdout = String::from_utf8(stdout).unwrap();
            let stderr = String::from_utf8(stderr).unwrap();
            if case["name"] == "usage_unknown_fields" {
                assert_eq!(
                    serde_json::from_str::<Value>(&stdout).unwrap()["future_native"]["opaque"][0],
                    "retained"
                );
            }
            if case["surface"] == "usage" && case["native_response"]["status"] == "usage_document" {
                assert_eq!(
                    serde_json::from_str::<Value>(&stdout).unwrap(),
                    case["native_response"]["document"]
                );
            }
            if case["surface"] == "help" {
                assert_eq!(
                    serde_json::from_str::<Value>(&stdout).unwrap(),
                    case["native_response"]["result"]
                );
            }
            case["i2"] = json!({"exit_code":code,"stdout":stdout,"stderr":stderr});
            cases.push(case);
        }
    }
    assert_eq!(cases.len(), 12);
    let path = directory.join("native_reads.json");
    let writing =
        std::env::var_os("AIRK_WRITE_UI_PILOT_CLI").as_deref() == Some(std::ffi::OsStr::new("1"));
    if writing {
        let value = json!({"schema":"airk.split.ui-pilot.reads/1","qualification":"Actual I2 parser, unselected Help routing and output formatters; actual I3 handler with injected canonical packets. Not live IPC/HTTP or installed qualification.","cases":cases});
        std::fs::write(&path, serde_json::to_string_pretty(&value).unwrap() + "\n").unwrap();
    }
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(saved["schema"], "airk.split.ui-pilot.reads/1");
    assert_eq!(saved["cases"].as_array().unwrap().len(), cases.len());
    for (actual, expected) in saved["cases"].as_array().unwrap().iter().zip(cases) {
        for field in ["name", "surface", "request", "native_response", "i2"] {
            assert_eq!(
                actual[field], expected[field],
                "I2 drift in {} {field}",
                expected["name"]
            );
        }
    }
}
