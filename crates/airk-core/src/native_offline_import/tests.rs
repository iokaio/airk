// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::{
    arbiter::{Availability, ManagementRequest},
    service::{ControlMethod, ControlResponse},
};
use serde_json::json;
fn request() -> NativeOfflineImportRequest {
    NativeOfflineImportRequest {
        expected_revision: 7,
        profile: Sha256Digest::from_bytes([8; 32]),
        directory: if cfg!(windows) {
            r"D:\AIRK Models\verified bundle".into()
        } else {
            "/opt/airk-models/verified bundle".into()
        },
    }
}
#[test]
fn offline_import_requires_exact_native_local_bounded_directory_and_revision() -> Result<()> {
    let original = request();
    original.validate()?;
    let mut changed = original.clone();
    changed.expected_revision = 0;
    assert!(changed.validate().is_err());
    for path in [
        "",
        "relative/bundle",
        "https://models.example/bundle",
        "file:///tmp/bundle",
        "smb://server/bundle",
        "nfs://server/bundle",
        "//server/share",
        r"\\server\share\bundle",
        r"\\?\UNC\server\share\bundle",
    ] {
        let mut changed = original.clone();
        changed.directory = path.into();
        assert!(changed.validate().is_err(), "{path}");
    }
    let mut changed = original.clone();
    changed.directory.push('\n');
    assert!(changed.validate().is_err());
    let mut changed = original;
    changed.directory = "a".repeat(4097);
    assert!(changed.validate().is_err());
    Ok(())
}
#[test]
fn native_local_long_paths_are_supported_but_traversal_devices_and_streams_refuse() -> Result<()> {
    let mut value = request();
    #[cfg(windows)]
    {
        value.directory = r"\\?\D:\AIRK Models\verified bundle".into();
        value.validate()?;
        for path in [
            r"D:\AIRK\..\bundle",
            r"D:\AIRK\.\bundle",
            r"\\.\C:\bundle",
            r"\\?\GLOBALROOT\Device\bundle",
            r"D:\bundle:stream",
            r"C:relative",
        ] {
            value.directory = path.into();
            assert!(value.validate().is_err(), "{path}");
        }
    }
    #[cfg(not(windows))]
    for path in ["/opt/airk/../bundle", "/opt/airk/./bundle"] {
        value.directory = path.into();
        assert!(value.validate().is_err());
    }
    Ok(())
}
#[test]
fn offline_import_is_private_native_control_and_absent_from_public_management_schema() -> Result<()>
{
    let value = request();
    value.validate()?;
    let method = ControlMethod::ArbiterImport {
        request: Box::new(value.clone()),
    };
    assert_eq!(
        method.metadata_relay_refusal(),
        Some("console_private_setup_forbidden")
    );
    assert_eq!(
        method.request_frame_budget(),
        crate::service::MAX_TOOLS_CALL_REQUEST_BYTES
    );
    assert_eq!(
        method.response_frame_budget(),
        crate::service::MAX_TOOLS_CALL_RESPONSE_BYTES
    );
    let wire = serde_json::to_value(&method)?;
    let _: ControlMethod = serde_json::from_value(wire)?;
    assert!(serde_json::from_value::<ManagementRequest>(json!({"operation":"import","expected_revision":value.expected_revision,"profile":value.profile,"directory":value.directory})).is_err());
    let public = crate::arbiter::schema::document();
    assert!(public["$defs"].get("NativeOfflineImportRequest").is_none());
    assert!(
        !public["$defs"]["ManagementRequest"]
            .to_string()
            .contains("directory")
    );
    Ok(())
}
#[test]
fn offline_import_closed_request_and_required_nonnullable_response_refuse_unknowns() -> Result<()> {
    let original = request();
    let mut raw = serde_json::to_value(&original)?;
    raw["download_url"] = "https://example.invalid/model".into();
    assert!(serde_json::from_value::<NativeOfflineImportRequest>(raw).is_err());
    for name in ["expected_revision", "profile", "directory"] {
        let mut raw = serde_json::to_value(&original)?;
        raw.as_object_mut().unwrap().remove(name);
        assert!(serde_json::from_value::<NativeOfflineImportRequest>(raw).is_err());
    }
    let status = crate::arbiter::stage_tests::status(
        crate::arbiter::stage_tests::configuration(false, false),
        Availability::Disabled,
    );
    let response = ControlResponse::ArbiterImport {
        observation: Box::new(status),
    };
    let mut raw = serde_json::to_value(response)?;
    let _: ControlResponse = serde_json::from_value(raw.clone())?;
    raw["observation"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<ControlResponse>(raw.clone()).is_err());
    raw.as_object_mut().unwrap().remove("observation");
    assert!(serde_json::from_value::<ControlResponse>(raw).is_err());
    Ok(())
}
#[test]
fn native_offline_import_fixture_emitter() -> Result<()> {
    let request = request();
    request.validate()?;
    let method = ControlMethod::ArbiterImport {
        request: Box::new(request.clone()),
    };
    let document = json!({"schema":"airk.native-offline-import-cases/1","qualification":false,
        "request":request,"control_method":method,"metadata_relay_refusal":method.metadata_relay_refusal(),
        "request_frame_budget":method.request_frame_budget(),"response_frame_budget":method.response_frame_budget(),
        "public_management_schema_contains_native_import":false});
    if let Some(path) = std::env::var_os("AIRK_WRITE_NATIVE_OFFLINE_IMPORT_CASES") {
        std::fs::write(path, serde_json::to_vec_pretty(&document)?)?;
    }
    Ok(())
}
