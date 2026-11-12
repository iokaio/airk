// SPDX-License-Identifier: Apache-2.0
//! Actual native/CLI/MCP rejection phases retain exact single-member replay facts.
use super::*;

fn hold_current_gate(path: &Path) -> Result<fs::File> {
    let file = fs::OpenOptions::new().read(true).write(true).open(path)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => return Err(error.into()),
        }
    }
}

#[tokio::test]
async fn single_required_member_busy_read_keeps_exact_retry_basis_through_native_cli_and_mcp()
-> Result<()> {
    let f = Fixture::new(false)?;
    let _service = f.start().await?;
    let backend = airk_cli::combined_frontend::resolve(None, Some(&f.control), &f.session).await?;
    let mut mcp = McpSession::start(&f)?;
    let arguments = json!({"project_ids":[f.projects[0]],"index_ids":[f.indexes[0]],"member_availability":"require",
        "path":"api/policy.md","start_line":1,"end_line":1,"max_bytes":4096,
        "verification":"off","response_mode":"full"});
    let warm = native_read(backend.as_ref(), "airk_read_file", &arguments).await?;
    ensure!(warm["isError"] != true, "warm fixture read failed: {warm}");
    let registration = &f.registrations[0];
    let collection = airk_store::service::collection::Collection::existing(
        Path::new(&registration.data_root),
        &registration.root,
        &registration.collection,
    )?;
    let store = collection.selected_store()?;
    let directory = store.directory();
    let gate = directory
        .parent()
        .context("incarnation parent missing")?
        .parent()
        .context("collection parent missing")?
        .join(".current-gates")
        .join(directory.file_name().context("incarnation missing")?);
    let held = hold_current_gate(&gate)?;
    // native read: each held-gate rejection must execute exactly once to retain
    // its actual phase; the retry helper is the behavior under examination.
    let native = tokio::time::timeout(
        Duration::from_secs(15),
        backend.call(
            "airk_read_file".into(),
            arguments.as_object().unwrap().clone(),
        ),
    )
    .await??;
    let mut command = Ctl::new();
    command
        .arg("--combined-session")
        .arg(serde_json::to_string(&f.session)?)
        .arg("--service-runtime-dir")
        .arg(&f.control)
        .args(["tools", "call", "airk_read_file", "--args"])
        .arg(arguments.to_string());
    // native read: one deliberate negative CLI observation, never a replay loop.
    let cli = tokio::task::spawn_blocking(move || {
        read_retry::once_until(&command, Instant::now() + Duration::from_secs(15))
    })
    .await??;
    ensure!(
        cli.status.code() == Some(1),
        "blocked CLI unexpectedly succeeded: {cli:?}"
    );
    let cli: Value = serde_json::from_slice(&cli.stdout)?;
    // native read: one deliberate negative wire observation of the exact read.
    let wire = mcp.request(
        2,
        "tools/call",
        json!({"name":"read_file","arguments":arguments}),
    )?;
    for (route, envelope) in [("native", &native), ("CLI", &cli), ("MCP", &wire["result"])] {
        assert_eq!(envelope["isError"], true, "{route}: {envelope}");
        let canonical = body(envelope)?;
        let untouched = canonical.clone();
        let retry = airk_core::combined_reads::read_retry::validated_retry(&canonical)
            .with_context(|| {
                format!("{route}: exact single-member retry metadata absent: {canonical}")
            })?;
        assert_eq!(
            canonical, untouched,
            "retry validation changed the original failure"
        );
        assert_eq!(retry.index_id, f.indexes[0]);
        assert_eq!(
            canonical["selection"]["identity"],
            json!(retry.selection_identity)
        );
        assert_eq!(canonical["error"]["code"], "member_unavailable");
        assert_eq!(canonical["error"]["retryable"], false);
        let original = canonical
            .pointer(&retry.result_pointer)
            .context("original retry basis missing")?;
        assert_eq!(
            original
                .get("code")
                .or_else(|| original.get("error").and_then(|value| value.get("code"))),
            Some(&json!(retry.code))
        );
        let mut forged = canonical.clone();
        forged["read_retry"]["selection_identity"] = json!("ff".repeat(32));
        assert!(airk_core::combined_reads::read_retry::validated_retry(&forged).is_none());
    }
    drop(held);
    let recovered = native_read(backend.as_ref(), "airk_read_file", &arguments).await?;
    assert_ne!(
        recovered["isError"], true,
        "read did not recover after exact gate release: {recovered}"
    );
    backend.close();
    backend.shutdown().await?;
    f.assert_sources_unchanged()?;
    Ok(())
}
