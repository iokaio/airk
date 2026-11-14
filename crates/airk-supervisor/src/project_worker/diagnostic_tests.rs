// SPDX-License-Identifier: Apache-2.0
//! Early native semantic preflight preserves the owning Kernel diagnostic stage.
use super::*;
use std::fs;

#[test]
fn corrupt_current_owner_keeps_exact_index_stage_before_source_preflight_and_execution()
-> Result<()> {
    let fixture = tempfile::tempdir()?;
    let base = fixture.path().canonicalize()?;
    let source = base.join("source");
    let data = base.join("data");
    fs::create_dir(&source)?;
    fs::create_dir(&data)?;
    let original_source = b"unchanged native diagnostic fixture\n";
    fs::write(source.join("memo.txt"), original_source)?;
    let kernel = Kernel::open_service_collection(&source, Some(data.clone()), "fixture")?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    // Match the actual vectors-enabled client fixture. No model is installed:
    // corrupt metadata must refuse before inference admission.
    kernel.configure_vectors(true)?;
    let publication = kernel.status()?.publication;
    let collection = airk_store::service::collection::Collection::existing(
        &data,
        source.to_str().context("fixture source is not UTF-8")?,
        "fixture",
    )?;
    let selected = collection.select()?;
    let owner = selected.store.directory().join("current-owner.json");
    let original_owner = fs::read(&owner)?;
    let store = selected.store.clone();
    drop(selected);
    fs::write(&owner, b"{")?;
    // Retain both observations before restoring; assertions cannot strand the
    // corrupt metadata. The bare native cause reproduces the original failure.
    let bare = store
        .current_scope(source.to_str().unwrap(), "fixture")
        .err();
    // vectors_enabled -> vector_policy validates current_scope before the
    // early indexing branch's explicit current_scope observation. This is the
    // first failing operation, so its owning stage is vector_settings.
    let vector_policy = kernel.vectors_enabled().err();
    let preflight = kernel.index_service_sources().err();
    let executions = [JobKind::Index, JobKind::Reconcile].map(|kind| {
        execute(
            &kernel,
            kind,
            airk_core::identity::Sha256Digest::from_bytes([7; 32]),
        )
        .err()
    });
    fs::write(&owner, &original_owner)?;
    let bare = bare.context("corrupt native owner was accepted")?;
    assert_eq!(
        airk_kernel::diagnostics::indexing_failure_code(&bare).as_deref(),
        Some("metadata_incomplete")
    );
    let vector_policy = vector_policy.context("vector policy admitted a corrupt current owner")?;
    assert_eq!(
        airk_kernel::diagnostics::indexing_failure_code(&vector_policy).as_deref(),
        Some("metadata_incomplete")
    );
    assert_eq!(
        airk_host::service_logs::FailureDetail::capture(&vector_policy).chain,
        "0: EOF while parsing an object at line 1 column 1\n"
    );
    for error in std::iter::once(preflight).chain(executions) {
        let error = error.context("corrupt native owner was accepted by an index path")?;
        assert_eq!(
            airk_kernel::diagnostics::indexing_failure_code(&error).as_deref(),
            Some("index_vector_settings_metadata_incomplete")
        );
        let detail = airk_host::service_logs::FailureDetail::capture(&error);
        assert_eq!(
            detail.chain,
            "0: vector_settings\n1: EOF while parsing an object at line 1 column 1\n"
        );
        assert!(!detail.truncated);
    }
    assert_eq!(fs::read(&owner)?, original_owner);
    assert_eq!(fs::read(source.join("memo.txt"))?, original_source);
    assert_eq!(kernel.status()?.publication, publication);
    kernel
        .refresh_service_snapshot()?
        .require_service_sources()?;
    let offline = base.join("offline");
    fs::rename(&source, &offline)?;
    let unavailable = kernel
        .refresh_service_snapshot()
        .and_then(|selected| selected.require_service_sources());
    fs::rename(&offline, &source)?;
    assert!(
        unavailable
            .err()
            .context("missing source was admitted")?
            .is::<airk_kernel::service_projects::SourceUnavailable>()
    );
    Ok(())
}
