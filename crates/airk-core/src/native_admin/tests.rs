// SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::{Value, json};

fn chunk(data: &[u8], offset: u64, total: u64, done: bool) -> Value {
    json!({"http_status":200,"total_bytes":total,"offset":offset,"data":data,"done":done})
}

#[test]
fn native_admin_chunk_cases_match_complete_bounded_streams() {
    let payload = json!({"future_native":{"opaque":["é中",1,true]},"ok":true});
    let bytes = serde_json::to_vec(&payload).unwrap();
    let total = bytes.len() as u64;
    let split = bytes.iter().position(|byte| *byte == 0xc3).unwrap() + 1;
    let first = chunk(&bytes[..split], 0, total, false);
    let last = chunk(&bytes[split..], split as u64, total, true);
    let complete = chunk(&bytes, 0, total, true);
    let mut status = last.clone();
    status["http_status"] = json!(409);
    let mut changed_total = last.clone();
    changed_total["total_bytes"] = json!(total + 1);
    changed_total["done"] = json!(false);
    let mut early = first.clone();
    early["done"] = json!(true);
    let mut late = complete.clone();
    late["done"] = json!(false);
    let mut bad_status = complete.clone();
    bad_status["http_status"] = json!(99);
    let mut too_large = first.clone();
    too_large["total_bytes"] = json!(MAX_RESPONSE_BYTES + 1);
    let cases = vec![
        json!({"name":"complete_opaque_json","chunks":[complete],"accepted":true,"complete":true,"payload":payload}),
        json!({"name":"multibyte_split","chunks":[first,last],"accepted":true,"complete":true,"payload":payload}),
        json!({"name":"incomplete","chunks":[first],"accepted":false,"complete":false}),
        json!({"name":"empty_stream","chunks":[],"accepted":false,"complete":false}),
        json!({"name":"duplicate_chunk","chunks":[first,first,last],"accepted":false,"complete":false}),
        json!({"name":"reordered_chunk","chunks":[last,first],"accepted":false,"complete":false}),
        json!({"name":"changed_status","chunks":[first,status],"accepted":false,"complete":false}),
        json!({"name":"changed_total","chunks":[first,changed_total],"accepted":false,"complete":false}),
        json!({"name":"done_before_end","chunks":[early],"accepted":false,"complete":false}),
        json!({"name":"missing_done_at_end","chunks":[late],"accepted":false,"complete":false}),
        json!({"name":"invalid_status","chunks":[bad_status],"accepted":false,"complete":false}),
        json!({"name":"over_result_limit","chunks":[too_large],"accepted":false,"complete":false}),
        json!({"name":"empty_chunk","chunks":[chunk(&[],0,1,false)],"accepted":false,"complete":false}),
        json!({"name":"offset_overflow","chunks":[chunk(&[1],u64::MAX,1,true)],"accepted":false,"complete":false}),
        json!({"name":"invalid_utf8","chunks":[chunk(&[255],0,1,true)],"accepted":false,"complete":false}),
        json!({"name":"invalid_json","chunks":[chunk(b"{",0,1,true)],"accepted":false,"complete":false}),
    ];
    assert_eq!(cases.len(), 16);
    for case in &cases {
        let mut assembly = NativeAdminAssembly::default();
        let mut valid = true;
        let mut result = None;
        for raw in case["chunks"].as_array().unwrap() {
            match serde_json::from_value::<NativeAdminChunk>(raw.clone())
                .map_err(anyhow::Error::from)
                .and_then(|chunk| assembly.push(&chunk))
            {
                Ok(value) => result = value,
                Err(_) => {
                    valid = false;
                    break;
                }
            }
        }
        assert_eq!(
            valid && assembly.complete(),
            case["accepted"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            assembly.complete(),
            case["complete"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        if case["accepted"] == true {
            assert_eq!(result.unwrap(), case["payload"]);
        }
    }
    let value = json!({"schema":"airk.native-admin.chunks/1","max_body_bytes":MAX_BODY_BYTES,"max_response_bytes":MAX_RESPONSE_BYTES,"max_chunk_bytes":MAX_CHUNK_BYTES,"chunk_frame_bytes":CHUNK_FRAME_BYTES,"cases":cases});
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/split/native_admin/chunks.json");
    if std::env::var_os("AIRK_WRITE_NATIVE_ADMIN_CASES").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string_pretty(&value).unwrap() + "\n").unwrap();
    }
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    assert_eq!(
        saved, value,
        "regenerate AIRK_WRITE_NATIVE_ADMIN_CASES=1 and review chunk semantics"
    );
}

#[test]
fn native_admin_secret_and_private_responses_never_enter_public_projection() {
    use crate::{
        identity::Incarnation,
        service::{ControlResponse, WindowLease},
    };
    let cookie = "A".repeat(43);
    let csrf = "E".repeat(43);
    let secret = NativeAdminSessionSecret::new(cookie.clone(), csrf.clone()).unwrap();
    let debug = format!("{secret:?}");
    assert!(!debug.contains(&cookie) && !debug.contains(&csrf));
    for response in [
        ControlResponse::AdminNativeOpened {
            session: secret,
            service_instance: Incarnation::from_bytes([1; 16]),
        },
        ControlResponse::AdminNativeChunk {
            chunk: NativeAdminChunk {
                http_status: 200,
                total_bytes: cookie.len() as u64,
                offset: 0,
                data: cookie.as_bytes().to_vec(),
                done: true,
            },
        },
        ControlResponse::AdminNativeCancelled {},
        ControlResponse::AdminNativeClosed {},
        ControlResponse::WindowOpened {
            window_id: Incarnation::from_bytes([2; 16]),
            service_instance: Incarnation::from_bytes([1; 16]),
            lease: WindowLease::from_bytes([3; 32]),
            ttl_ms: 30000,
        },
        ControlResponse::AdminLaunchCode {
            code: cookie.clone().try_into().unwrap(),
            port: 41234,
            expires_in_ms: 60000,
        },
    ] {
        assert!(response.carries_private_transport_secret());
        let projected = serde_json::to_value(response.public_metadata()).unwrap();
        assert_eq!(
            projected,
            json!({"status":"failure","code":"console_private_response_forbidden"})
        );
        let raw = serde_json::to_string(&projected).unwrap();
        assert!(!raw.contains(&cookie) && !raw.contains(&csrf) && !raw.contains(&"03".repeat(32)));
        assert!(raw.contains("console_private_response_forbidden"));
    }
    let public = ControlResponse::ToolResult {
        result: json!({"documentation":{"caller":"request_key","csrf":"public schema name"},"future_native":{"retained":true}}),
    };
    let original = serde_json::to_value(&public).unwrap();
    assert_eq!(
        serde_json::to_value(public.public_metadata()).unwrap(),
        original
    );
}
