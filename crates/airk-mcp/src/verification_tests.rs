// SPDX-License-Identifier: Apache-2.0
use super::*;
use anyhow::{Result, ensure};
use serde_json::{Value, json};

fn body(reply: &CallToolResult) -> Result<Value> {
    Ok(serde_json::from_str(
        &reply.content[0].as_text().unwrap().text,
    )?)
}
fn controls(mode: &str) -> Value {
    json!({"verification":mode,"verification_max_sources":25,"verification_max_bytes":16777216,"verification_deadline_ms":2000})
}

#[tokio::test]
async fn verification_refusal_does_not_acknowledge_undelivered_observations() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("source");
    std::fs::create_dir(&root)?;
    std::fs::write(root.join("policy.md"), "# Queue\nSaved source.\n")?;
    let kernel = Kernel::open(&root, Some(temp.path().join("data")))?;
    kernel.index()?;
    let server = AirkServer::new(kernel);
    let before = server.responses.receipt();
    let reply = server
        .execute_verified_response(
            airk_core::usage::UsageOperation::FileInfo,
            Default::default(),
            Default::default(),
            |kernel| {
                let mut value = serde_json::to_value(
                    kernel.file_info_conditional("policy.md", &Default::default())?,
                )?;
                value["padding"] = "x".repeat(65536).into();
                Ok(value)
            },
        )
        .await;
    assert_eq!(reply.is_error, Some(true), "{reply:?}");
    assert_eq!(body(&reply)?["error"]["code"], "verification_reply_budget");
    assert_eq!(server.responses.receipt(), before);
    Ok(())
}

#[tokio::test]
async fn real_mcp_checks_saved_sources_preserves_failures_and_keeps_off_unchanged() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("source");
    std::fs::create_dir(&root)?;
    std::fs::write(
        root.join("policy.md"),
        "# Queue\nPreserve original revision identity.\n",
    )?;
    let kernel = Kernel::open(&root, Some(temp.path().join("data")))?;
    kernel.index()?;
    let mut client = local::LocalTools::new(kernel.clone()).await?;
    let mut args = controls("report");
    args["path"] = json!("policy.md");
    args["response_mode"] = json!("full");
    let checked = client
        .call("airk_file_info".into(), args.as_object().unwrap().clone())
        .await?;
    ensure!(checked.is_error != Some(true), "{checked:?}");
    let value = body(&checked)?;
    assert_eq!(value["source_verification"]["status"], "verified");
    assert_eq!(
        value["source_verification"]["counts"]["requested_sources"],
        1
    );
    assert_eq!(value["source_verification"]["counts"]["references"], 2);
    for reference in value["source_verification"]["references"]
        .as_array()
        .unwrap()
    {
        assert!(
            value
                .pointer(reference["result_pointer"].as_str().unwrap())
                .is_some()
        );
    }
    let publication = value["publication"].clone();
    let default = client
        .call(
            "airk_file_info".into(),
            json!({"path":"policy.md","response_mode":"full"})
                .as_object()
                .unwrap()
                .clone(),
        )
        .await?;
    ensure!(default.is_error != Some(true), "{default:?}");
    let default = body(&default)?;
    assert_eq!(default["source_verification"]["mode"], "report");
    assert_eq!(default["source_verification"]["status"], "verified");
    assert_eq!(default["source_verification"]["limits"]["max_sources"], 8);
    assert_eq!(
        default["source_verification"]["limits"]["max_bytes"],
        16777216
    );
    assert_eq!(
        default["source_verification"]["limits"]["deadline_ms"],
        2000
    );
    let terse = client
        .call(
            "airk_file_info".into(),
            json!({"path":"policy.md","response_mode":"terse"})
                .as_object()
                .unwrap()
                .clone(),
        )
        .await?;
    let terse = body(&terse)?;
    assert_eq!(terse["source_verification"]["status"], "verified");
    airk_core::verification::validate_projection(&terse)?;
    let metadata = client
        .call("airk_status".into(), Default::default())
        .await?;
    assert!(body(&metadata)?.get("source_verification").is_none());
    std::fs::remove_file(root.join("policy.md"))?;
    let off = client
        .call(
            "airk_file_info".into(),
            json!({"path":"policy.md","verification":"off","response_mode":"full"})
                .as_object()
                .unwrap()
                .clone(),
        )
        .await?;
    ensure!(off.is_error != Some(true), "{off:?}");
    assert!(body(&off)?.get("source_verification").is_none());
    args["verification"] = json!("require");
    let failed = client
        .call("airk_file_info".into(), args.as_object().unwrap().clone())
        .await?;
    assert_eq!(failed.is_error, Some(true));
    let failure = body(&failed)?;
    assert_eq!(
        failure["source_verification"]["status"],
        "requirements_unmet"
    );
    assert_eq!(
        failure["source_verification"]["sources"][0]["state"],
        "missing"
    );
    assert_eq!(
        failure["source_verification"]["error"]["code"],
        "verification_required_unmet"
    );
    assert!(
        failed.structured_content.is_none(),
        "native error text retains complete JSON without duplicating the bounded IPC body"
    );
    assert_eq!(failure["publication"], publication);
    assert_eq!(failure["content_hash"], value["content_hash"]);
    assert!(failure["source_verification"]["counts"]["bytes_read"] == 0);
    for bad in [
        json!({"verification":"report","verification_max_sources":0}),
        json!({"verification":"off","verification_max_bytes":1}),
        json!({"verification":null}),
    ] {
        let mut bad = bad.as_object().unwrap().clone();
        bad.insert("path".into(), json!("policy.md"));
        assert_eq!(
            client.call("airk_file_info".into(), bad).await?.is_error,
            Some(true)
        );
    }
    client.close().await?;
    Ok(())
}

#[tokio::test]
async fn global_and_step_verification_use_real_plan_handlers_and_bind_prepared_controls()
-> Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("source");
    std::fs::create_dir(&root)?;
    let source = "# Queue\nSaved source verification preserves receipts.\n";
    std::fs::write(root.join("policy.md"), source)?;
    let kernel = Kernel::open(&root, Some(temp.path().join("data")))?;
    kernel.index()?;
    let mut client = local::LocalTools::new(kernel).await?;
    let mut request = controls("report");
    request["schema"] = json!("airk.parse.request/1");
    request["plan"] = json!({"steps":[{"id":"first","tool":"file_info","args":{"path":"policy.md"}},{"id":"again","tool":"read_file","args":{"path":"policy.md","start_line":1,"end_line":2}}]});
    let prepared = client
        .call("airk_parse".into(), request.as_object().unwrap().clone())
        .await?;
    ensure!(prepared.is_error != Some(true), "{prepared:?}");
    let prepared = body(&prepared)?;
    assert!(prepared.get("source_verification").is_none());
    assert_eq!(prepared["plan"]["verification"], "report");
    let execute = json!({"schema":"airk.parse.request/1","mode":"execute","plan":prepared["plan"],"response_mode":"full"});
    let run = client
        .call("airk_parse".into(), execute.as_object().unwrap().clone())
        .await?;
    ensure!(run.is_error != Some(true), "{run:?}");
    let run = body(&run)?;
    assert_eq!(run["status"], "complete");
    assert_eq!(run["source_verification"]["status"], "verified");
    assert!(run["warnings"].as_array().unwrap().iter().any(|warning| {
        warning
            .as_str()
            .is_some_and(|text| text.contains("Saved sources verified at"))
    }));
    assert_eq!(run["source_verification"]["counts"]["requested_sources"], 1);
    assert_eq!(
        run["source_verification"]["counts"]["bytes_read"],
        source.len()
    );
    assert!(
        run["source_verification"]["counts"]["references"]
            .as_u64()
            .unwrap()
            > 2
    );
    for reference in run["source_verification"]["references"].as_array().unwrap() {
        assert!(
            run.pointer(reference["result_pointer"].as_str().unwrap())
                .is_some()
        );
    }
    let mut edited = execute.clone();
    edited["verification"] = json!("require");
    edited["verification_max_sources"] = json!(25);
    edited["verification_max_bytes"] = json!(16777216);
    edited["verification_deadline_ms"] = json!(2000);
    assert_eq!(
        client
            .call("airk_parse".into(), edited.as_object().unwrap().clone())
            .await?
            .is_error,
        Some(true)
    );
    // Preparing a metadata-only plan serializes the default controls but must
    // never turn its eventual execution into a source verification request.
    let metadata = json!({"schema":"airk.parse.request/1","plan":{"steps":[{"id":"health","tool":"status","args":{}}]}});
    let prepared = client
        .call("airk_parse".into(), metadata.as_object().unwrap().clone())
        .await?;
    let prepared = body(&prepared)?;
    let execute_metadata =
        json!({"schema":"airk.parse.request/1","mode":"execute","plan":prepared["plan"]});
    let metadata = client
        .call(
            "airk_parse".into(),
            execute_metadata.as_object().unwrap().clone(),
        )
        .await?;
    ensure!(metadata.is_error != Some(true), "{metadata:?}");
    let metadata = body(&metadata)?;
    assert!(metadata.get("source_verification").is_none());
    assert!(
        metadata["steps"][0]["result"]
            .get("source_verification")
            .is_none()
    );
    let mut step = controls("report");
    step["path"] = json!("policy.md");
    let per_step = json!({"schema":"airk.parse.request/1","mode":"execute","plan":{"steps":[{"id":"check","tool":"file_info","args":step}]}});
    let checked = client
        .call("airk_parse".into(), per_step.as_object().unwrap().clone())
        .await?;
    ensure!(checked.is_error != Some(true), "{checked:?}");
    let checked = body(&checked)?;
    assert_eq!(
        checked["steps"][0]["result"]["source_verification"]["status"],
        "verified"
    );
    let mut evidence_only = per_step.clone();
    evidence_only["plan"]["output"] = json!({"view":"evidence","max_bytes":262144});
    let rejected = client
        .call(
            "airk_parse".into(),
            evidence_only.as_object().unwrap().clone(),
        )
        .await?;
    assert_eq!(rejected.is_error, Some(true));
    assert!(
        rejected.content[0]
            .as_text()
            .unwrap()
            .text
            .contains("retain complete step receipts")
    );
    let mut conflict = per_step.clone();
    conflict
        .as_object_mut()
        .unwrap()
        .extend(controls("report").as_object().unwrap().clone());
    assert_eq!(
        client
            .call("airk_parse".into(), conflict.as_object().unwrap().clone())
            .await?
            .is_error,
        Some(true)
    );
    std::fs::write(root.join("policy.md"), "changed saved bytes\n")?;
    let required = json!({"schema":"airk.parse.request/1","mode":"execute","verification":"require",
        "plan":{"steps":[{"id":"check","tool":"file_info","args":{"path":"policy.md"}}]}});
    let failed = client
        .call("airk_parse".into(), required.as_object().unwrap().clone())
        .await?;
    assert_eq!(failed.is_error, Some(true));
    let failed = body(&failed)?;
    assert_eq!(failed["status"], "failed");
    assert_eq!(failed["error"]["code"], "verification_required_unmet");
    assert_eq!(
        failed["source_verification"]["sources"][0]["state"],
        "mismatch"
    );
    assert_eq!(
        failed["steps"][0]["result"]["content_hash"],
        run["steps"][0]["result"]["content_hash"]
    );
    client.close().await?;
    Ok(())
}

#[test]
fn only_evidence_and_parse_catalogs_offer_explicit_verification_controls() {
    for tool in local::tools() {
        let offered = tool.input_schema["properties"]
            .get("verification")
            .is_some();
        assert_eq!(
            offered,
            !matches!(
                tool.name.as_ref(),
                "airk_status" | "airk_diagnostics" | "airk_capabilities" | "airk_help"
            ),
            "{}",
            tool.name
        );
        if offered {
            assert_eq!(
                tool.input_schema["properties"]["verification"]["enum"],
                json!(["off", "report", "require"])
            );
            for field in [
                "verification_max_sources",
                "verification_max_bytes",
                "verification_deadline_ms",
            ] {
                assert_eq!(tool.input_schema["properties"][field]["type"], "integer");
            }
        }
    }
}

#[tokio::test]
async fn cancelling_an_accepted_verification_keeps_capacity_until_the_blocking_owner_returns()
-> Result<()> {
    use std::{
        sync::{Arc, mpsc},
        time::Duration,
    };
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("source");
    std::fs::create_dir(&root)?;
    std::fs::write(root.join("policy.md"), "# Queue\nSaved source.\n")?;
    let kernel = Kernel::open(&root, Some(temp.path().join("data")))?;
    kernel.index()?;
    let value =
        serde_json::to_value(kernel.file_info_conditional("policy.md", &Default::default())?)?;
    let server = Arc::new(AirkServer::new(kernel));
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (receipt_tx, receipt_rx) = tokio::sync::oneshot::channel();
    let controls: airk_core::verification::VerificationControls =
        serde_json::from_value(controls("report"))?;
    let budget = controls.resolve()?.unwrap();
    let request = tokio::spawn({
        let server = server.clone();
        async move {
            server
                .execute_verified_response(
                    UsageOperation::FileInfo,
                    Default::default(),
                    controls,
                    move |kernel| {
                        let _ = started_tx.send(());
                        release_rx.recv_timeout(Duration::from_secs(5))?;
                        let receipt = kernel.verify_returned_sources(&value, budget)?;
                        let _ = receipt_tx.send(receipt);
                        Ok(value)
                    },
                )
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(5), started_rx).await??;
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    // The blocking owner is still accepted; cancelling its waiter did not finish
    // or detach its capacity lifetime. It observes cancellation before source I/O.
    release_tx.send(())?;
    let receipt = tokio::time::timeout(Duration::from_secs(5), receipt_rx).await??;
    assert_eq!(receipt.counts.bytes_read, 0);
    assert_eq!(receipt.counts.checked_sources, 0);
    assert!(receipt.limits.hit.iter().any(|hit| hit == "cancellation"));
    server.admission.shutdown().await;
    Ok(())
}
