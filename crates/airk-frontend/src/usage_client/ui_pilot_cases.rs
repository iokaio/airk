// SPDX-License-Identifier: Apache-2.0
//! Deterministic native report semantics, without a clock or persisted fixture store.
use super::*;
use airk_core::{service::UsageDocumentRead, usage::*};
use serde_json::json;

fn report_document(populated: bool, project: ProjectBinding) -> Result<Value> {
    let scope = UsageScope {
        collection: "ui-pilot-fixture".into(),
        root_binding: [0x11; 32],
    };
    let observations = if populated {
        vec![UsageObservation {
            schema_version: USAGE_SCHEMA_VERSION,
            event_id: [1; 16],
            scope: scope.clone(),
            unix_ms: 1000,
            generation: Some(1),
            operation: UsageOperation::Search,
            client: ClientFamily::Explorer,
            content_kind: ContentKind::Code,
            outcome: UsageOutcome::Success,
            elapsed_us: 300,
            cache: CacheObservation::Warm,
            returned_items: 2,
            truncated: false,
            response_representation: ResponseRepresentation::McpToolResultJson,
            response_utf8_bytes: Some(1024),
            baseline: None,
            baseline_unavailable: None,
            index_work: None,
            resources: None,
        }]
    } else {
        vec![]
    };
    let report = summarize(
        &scope,
        0,
        7 * 86_400_000,
        observations.iter(),
        ByteTokenHeuristic::default(),
    )?;
    let history = vec![summarize(
        &scope,
        0,
        86_400_000,
        observations.iter(),
        ByteTokenHeuristic::default(),
    )?];
    let view = airk_kernel::usage::UsageView {
        policy: UsagePolicy::default(),
        status: if populated { "partial" } else { "empty" },
        report,
        history,
        lifetime: None,
        supplemental: vec![],
        price_scenario: None,
        process_cpu_us: None,
        physical_io_bytes: None,
        resources: ResourceSummary::default(),
        health: Default::default(),
        process_dropped_observations: 0,
        notice: "Deterministic native semantic fixture; resource and alternative-context measurements are unavailable.",
    };
    let value = serde_json::to_value(view)?;
    let wrapped = json!({"project":project,"value":value});
    let page = airk_kernel::usage_page::page(wrapped, None, 48 << 10)?;
    ensure!(
        page.cursor.is_none(),
        "fixture unexpectedly requires more pages"
    );
    let mut headers = None;
    observe_scope_headers(&page, &mut headers)?;
    let mut assembled = Value::Null;
    for record in page.items {
        airk_kernel::usage_page::assemble(&mut assembled, record)?;
    }
    let retained = finish_native_document(assembled, project, headers.as_ref().unwrap())?;
    ensure!(retained == value, "native page collector changed report");
    Ok(retained)
}

#[tokio::test]
async fn ui_pilot_usage_native_cases() -> Result<()> {
    let project: ProjectBinding = "4b".repeat(32).parse()?;
    let error = render_selected(
        Path::new("unused-before-validation"),
        project,
        31,
        airk_mcp::local::ReportFormat::Json,
        None,
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(error.to_string(), "usage report days must be 1..30");
    let mut unknown = report_document(true, project)?;
    unknown["future_native"] = json!({"opaque":["retained",{"supported":true}]});
    let responses = [
        (
            "usage_empty",
            7,
            ControlResponse::UsageDocument {
                document: report_document(false, project)?,
            },
        ),
        (
            "usage_populated",
            7,
            ControlResponse::UsageDocument {
                document: report_document(true, project)?,
            },
        ),
        (
            "usage_days_31_refused",
            31,
            ControlResponse::UsageReadFailed {
                message: error.to_string(),
                native: None,
            },
        ),
        (
            "usage_native_failure",
            7,
            ControlResponse::UsageReadFailed {
                message: "native usage read unavailable".into(),
                native: Some(
                    json!({"error":{"code":"capacity_limited","retry_after_ms":125},"future_native":{"retained":true}}),
                ),
            },
        ),
        (
            "usage_unknown_fields",
            7,
            ControlResponse::UsageDocument { document: unknown },
        ),
    ];
    let cases = responses.into_iter().map(|(name,days,response)| {
        let method = ControlMethod::UsageDocument{project,catalog:None,read:UsageDocumentRead::Report{days,format:airk_core::native_reads::ReportFormat::Json}};
        json!({"name":name,"surface":"usage","request":{"cli_args":["usage","report","--days",days.to_string(),"--format","json","--service-project",project.to_string(),"--output","json"],"admin_operation":"usage.document","admin_body":method},"native_response":response})
    }).collect::<Vec<_>>();
    assert_eq!(cases.len(), 5);
    let value = json!({"schema":"airk.split.ui-pilot.native-payloads/1","owner":"airk-frontend::usage_client",
        "qualification":"Actual native summary/types and page collector. Fixed observations replace persisted store and clock. Unknown fields and transport failure are explicit controls; days31 is the actual owner refusal.","cases":cases});
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/split/ui_pilot/usage_native.json");
    if std::env::var_os("AIRK_WRITE_UI_PILOT_USAGE").as_deref() == Some(std::ffi::OsStr::new("1")) {
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, serde_json::to_string_pretty(&value)? + "\n")?;
    }
    let saved: Value = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    assert_eq!(
        saved, value,
        "regenerate AIRK_WRITE_UI_PILOT_USAGE=1 and review native changes"
    );
    Ok(())
}
