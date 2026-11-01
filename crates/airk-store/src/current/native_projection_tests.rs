// SPDX-License-Identifier: Apache-2.0
use super::*;
use airk_core::{
    changes::{FileStamps, FileState},
    publication::InventoryFile,
};
use std::collections::BTreeMap;
fn fact(name: &str, id: u32) -> airk_core::extraction::LocalFact {
    airk_core::extraction::LocalFact {
        id,
        owner: None,
        kind: "heading".into(),
        name: name.into(),
        span: airk_core::extraction::LocalSpan { start: 0, end: 4 },
        syntax_kind: "heading".into(),
        start_line: 1,
        end_line: 1,
        location: airk_core::DocumentLocation::Lines { start: 1, end: 1 },
        document: None,
        code: None,
        region: None,
        attributes: Default::default(),
    }
}
fn inventory(fixture: &Fixture, input: &CapturedInput, sequence: u64) -> Result<InventoryFile> {
    let artifact = &input.artifact;
    let retained = artifact.retained.as_ref().unwrap();
    let digest = Sha256::digest(artifact.path.as_bytes());
    let mut id = [0; 16];
    id.copy_from_slice(&digest[..16]);
    Ok(InventoryFile {
        continuity: Vec::new(),
        continuity_coverage: Default::default(),
        state: FileState {
            artifact_id: airk_core::identity::ArtifactId::from_bytes(id),
            stamps: FileStamps {
                first_generation: 1,
                path_generation: sequence,
                content_generation: sequence,
                evidence_generation: sequence,
            },
            source_hash: artifact.content_hash.parse()?,
            profile_id: retained.profile.as_ref().unwrap().identity()?,
            payload_digest: retained.object.parse()?,
            evidence_digest: Sha256Digest::from_bytes([49; 32]),
            root: fixture.scope.root.clone(),
            path: artifact.path.clone(),
        },
        source_bytes: artifact.bytes,
        payload_bytes: retained.object_bytes,
        extracted_bytes: retained.extracted_bytes,
        chunks: retained.chunks,
        indexed_lines: 1,
        facts: retained.facts,
        raw_source: true,
        language: artifact.language.clone(),
        extraction_method: artifact.extraction_method.clone(),
        parse_status: artifact.parse_status.clone(),
    })
}
/// Uses the actual map, metadata certification and durable journal; no public
/// query consumer is switched to the new representation by this fixture.
fn publish(
    fixture: &Fixture,
    path: &str,
    input: Option<&CapturedInput>,
    reopen: bool,
) -> Result<CurrentToken> {
    publish_options(
        fixture,
        path,
        input,
        reopen,
        None,
        airk_core::compression::CompressionMode::Off,
        true,
    )
}
fn publish_options(
    fixture: &Fixture,
    path: &str,
    input: Option<&CapturedInput>,
    reopen: bool,
    prepared_file: Option<InventoryFile>,
    mode: airk_core::compression::CompressionMode,
    projected: bool,
) -> Result<CurrentToken> {
    let read = fixture.store.current_read(&fixture.scope)?;
    let base = read.head().clone();
    let previous = read.captured_input(path)?;
    let reservation = fixture.quota.reserve(&fixture.scope.collection, 64 << 20)?;
    let references = if let Some(input) = input {
        let full = fixture.store.load_projection_artifact(&input.artifact)?;
        let file = prepared_file.unwrap_or(inventory(fixture, input, base.sequence + 1)?);
        let reference = if projected {
            fixture.store.retain_current_metadata_with_facts_encoded(
                path,
                &file,
                &full,
                &reservation,
                mode,
            )?
        } else {
            let mut legacy =
                fixture
                    .store
                    .retain_current_metadata_encoded(path, &file, &reservation, mode)?;
            legacy.scalars = None;
            legacy
        };
        vec![reference]
    } else {
        Vec::new()
    };
    let sealed = graph::GraphOverlay::new(
        &reservation,
        read,
        graph::GraphOverlayLimits {
            maximum_bytes: 1 << 20,
            ..Default::default()
        },
    )?
    .seal()?;
    let connection = database::open_reader(&fixture.store.directory().join("current.sqlite3"))?;
    let mut plan = map::MapPlan::new(&connection, &fixture.scope, base.artifact_root)?;
    let configuration = collection::CollectionConfiguration {
        roots: BTreeMap::from([("workspace".into(), fixture.scope.root.clone())]),
        extractor_version: "projection-fixture".into(),
        extraction_policy_digest: Sha256Digest::from_bytes([55; 32]),
        index_detail: Default::default(),
        vector_selection: Default::default(),
        model: None,
    };
    let mut collection =
        collection::CollectionPlan::load(&plan, &fixture.scope, Some(&configuration))?;
    collection.update(
        previous.as_ref().map(|input| &input.artifact),
        input.map(|input| &input.artifact),
    )?;
    let mut postings = postings::PostingChanges::new(base.sequence == 0);
    postings.update(
        &mut plan,
        &fixture.scope,
        &fixture.store,
        previous.as_ref(),
        input,
    )?;
    let first = map::MapChange::new(
        &fixture.scope,
        map::MapDomain::Artifact,
        path.as_bytes(),
        input.map(CapturedInput::bytes).transpose()?,
    )?;
    plan.put(first.key(), first.value())?;
    let mut changes = vec![first];
    let artifacts = vec![match input {
        Some(input) => ArtifactChange::Upsert(Box::new(input.clone())),
        None => ArtifactChange::Delete(path.into()),
    }];
    changes.extend(metadata::prepare(
        &fixture.store,
        &fixture.scope,
        &mut plan,
        &artifacts,
        &references,
        base.sequence + 1,
    )?);
    changes.extend(postings.finish(&mut plan, &fixture.scope)?);
    changes.extend(collection.finish(&mut plan, &fixture.scope)?);
    changes.sort_by_key(map::MapChange::key);
    let mut target = base.clone();
    target.sequence += 1;
    target.artifact_root = plan.root()?;
    drop(connection);
    let inputs = input.map(|input| vec![input.clone()]).unwrap_or_default();
    let lock = fixture.store.lock()?;
    let mut writer =
        fixture
            .store
            .current_write(&lock, &fixture.quota, &fixture.scope, CAPACITY)?;
    let pending =
        writer.prepare_mapped(&base.token()?, target, &sealed, &inputs, None, &changes)?;
    writer.mark_applying(&pending)?;
    if reopen {
        drop(writer);
        drop(lock);
        let lock = fixture.store.lock()?;
        let mut writer =
            fixture
                .store
                .current_write(&lock, &fixture.quota, &fixture.scope, CAPACITY)?;
        let replay = writer
            .pending()?
            .context("projection pending journal lost")?;
        writer.mark_applying(&replay)?;
        Ok(writer.commit(&replay, replay.target.lexical_commit)?.token)
    } else {
        Ok(writer
            .commit(&pending, pending.target.lexical_commit)?
            .token)
    }
}
#[test]
fn resource_pages_preserve_ordinals_strict_bytes_and_retention_through_replay() -> Result<()> {
    let fixture = Fixture::new()?;
    let input = retained_input_with_facts(
        &fixture,
        "one.txt",
        "body",
        vec![fact("Zulu", 0), fact("Alpha", 1), fact("Alpha", 2)],
    )?;
    publish(&fixture, "one.txt", Some(&input), true)?;
    let read = fixture.store.current_read(&fixture.scope)?;
    let projection = read
        .native_fact_projection(&fixture.store, "one.txt")?
        .context("projection absent")?;
    let page = projection.page(0)?;
    assert_eq!(
        page.rows.iter().map(|row| row.ordinal).collect::<Vec<_>>(),
        vec![1, 2, 0]
    );
    assert_eq!(projection.page_for_ordinal(2)?, 0);
    let full = fixture.store.load_projection_artifact(&input.artifact)?;
    let projected = page
        .rows
        .iter()
        .map(|row| super::super::discovery::fact_hit(&full, &row.fact))
        .collect::<Vec<_>>();
    let mut query = airk_core::FactQuery {
        path_prefix: "one.txt".into(),
        name: None,
        kind: None,
        language: None,
        exact_name: false,
        limit: 100,
        max_bytes: 65536,
        path_exact: true,
        document_only: false,
    };
    let strict = read.native_facts_page(&fixture.store, &query, false, &Default::default())?;
    assert_eq!(
        serde_json::to_value(&strict.facts)?,
        serde_json::to_value(&projected)?
    );
    query.max_bytes = 1;
    assert_eq!(
        read.native_facts_page(&fixture.store, &query, false, &Default::default())?
            .page
            .required_record_bytes,
        Some(serde_json::to_vec(&projected[0])?.len())
    );
    let stats = projection.stats();
    assert_eq!(stats.leaf_objects, 1);
    assert_eq!(stats.manifest_objects, 1);
    let plan = read.reclamation_plan(&fixture.store)?;
    assert_eq!(plan.projection_counts().0, 1);
    assert_eq!(plan.projection_counts().1, 1);
    assert!(read.audit_file_scalars(&fixture.store, "one.txt")?);
    assert!(read.audit_native_fact_projection(&fixture.store, "one.txt")?);
    let source = fixture
        .store
        .directory()
        .join("objects/sources")
        .join(&input.artifact.content_hash);
    let held = source.with_extension("held");
    fs::rename(&source, &held)?;
    assert_eq!(projection.page(0)?.rows.len(), 3);
    assert!(
        read.audit_native_fact_projection(&fixture.store, "one.txt")
            .is_err()
    );
    fs::rename(held, source)?;
    drop(projection);
    drop(read);
    publish(&fixture, "one.txt", None, false)?;
    let read = fixture.store.current_read(&fixture.scope)?;
    assert!(
        read.native_fact_projection(&fixture.store, "one.txt")?
            .is_none()
    );
    Ok(())
}
#[test]
fn producer_max_has_bounded_byte_pages_and_exact_logical_storage_counts() -> Result<()> {
    let fixture = Fixture::new()?;
    let facts = (0..10_000).map(|id| fact("SameName", id)).collect();
    let input = retained_input_with_facts_budget(&fixture, "max.txt", "body", facts, 32 << 20)?;
    let full = fixture.store.load_projection_artifact(&input.artifact)?;
    let native_file = inventory(&fixture, &input, 1)?;
    let mut created = resources::ResourceWriteCounts::default();
    {
        let reservation = fixture.quota.reserve(&fixture.scope.collection, 32 << 20)?;
        fixture.store.retain_native_fact_projection_counted(
            &full,
            &native_file,
            &reservation,
            airk_core::compression::CompressionMode::Space,
            Some(&mut created),
        )?;
    }
    publish_options(
        &fixture,
        "max.txt",
        Some(&input),
        false,
        None,
        airk_core::compression::CompressionMode::Space,
        true,
    )?;
    let read = fixture.store.current_read(&fixture.scope)?;
    let projection = read
        .native_fact_projection(&fixture.store, "max.txt")?
        .unwrap();
    let mut count = 0usize;
    for page in 0..projection.header().pages.len() {
        let selected = projection.page(page)?;
        assert!((1..=256).contains(&selected.rows.len()));
        for row in selected.rows {
            assert_eq!(row.ordinal, count);
            assert_eq!(projection.page_for_ordinal(count)?, page);
            count += 1;
        }
    }
    assert_eq!(count, 10_000);
    let stats = projection.stats();
    assert!(stats.leaf_objects < 313, "tiny fixed pages returned");
    assert_eq!(stats.leaf_objects, 10_000usize.div_ceil(256));
    assert_eq!(stats.manifest_objects, 1);
    assert!(stats.leaf_encoded_bytes >= stats.row_encoded_bytes);
    // Content-free counters are observed only when this actual fixture runs.
    let aggregate = read.native_projection_stats_page(&fixture.store, None, 1)?;
    assert_eq!(aggregate.counts.files, 1);
    assert_eq!(aggregate.counts.facts, 10_000);
    assert_eq!(aggregate.counts.leaf_objects, stats.leaf_objects as u64);
    assert_eq!(created.created_objects, stats.leaf_objects as u64 + 1);
    assert_eq!(created.reused_objects, 0);
    assert_eq!(
        created.created_logical_bytes,
        stats.leaf_encoded_bytes + stats.manifest_encoded_bytes as u64
    );
    assert_eq!(
        created.created_stored_bytes,
        stats.leaf_stored_bytes + stats.manifest_stored_bytes
    );
    assert!(
        stats.leaf_stored_bytes < stats.leaf_encoded_bytes,
        "native pages ignored reviewed compression policy"
    );
    let mut reused = resources::ResourceWriteCounts::default();
    {
        let reservation = fixture.quota.reserve(&fixture.scope.collection, 32 << 20)?;
        fixture.store.retain_native_fact_projection_counted(
            &full,
            &native_file,
            &reservation,
            airk_core::compression::CompressionMode::Space,
            Some(&mut reused),
        )?;
    }
    assert_eq!(reused.created_objects, 0);
    assert_eq!(reused.reused_objects, created.created_objects);
    assert_eq!(reused.reused_logical_bytes, created.created_logical_bytes);
    assert_eq!(reused.reused_stored_bytes, created.created_stored_bytes);
    eprintln!(
        "native_projection_counts {}",
        serde_json::to_string(&stats)?
    );
    eprintln!(
        "native_projection_created {}",
        serde_json::to_string(&created)?
    );
    eprintln!(
        "native_projection_reused {}",
        serde_json::to_string(&reused)?
    );
    assert!(read.audit_native_fact_projection(&fixture.store, "max.txt")?);
    Ok(())
}

#[test]
fn owner_capability_handoff_preserves_old_head_resize_and_refuses_unsupported() -> Result<()> {
    #[derive(serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct LegacyOwner {
        version: u32,
        scope: CurrentScope,
        capacity: u64,
    }
    #[derive(serde::Serialize)]
    struct FutureOwner {
        version: u32,
        scope: CurrentScope,
        capacity: u64,
        query_projection_version: u32,
    }
    let fixture = Fixture::new()?;
    let path = fixture.store.directory().join(database::OWNER_FILE);
    let original = fs::read(&path)?;
    assert!(
        serde_json::from_slice::<LegacyOwner>(&original).is_err(),
        "old owner parser accepted new capability"
    );
    let value: serde_json::Value = serde_json::from_slice(&original)?;
    let old = LegacyOwner {
        version: value["version"].as_u64().unwrap() as u32,
        scope: fixture.scope.clone(),
        capacity: CAPACITY,
    };
    fs::write(&path, serde_json::to_vec(&old)?)?;
    let before = {
        let read = fixture.store.current_read(&fixture.scope)?;
        assert_eq!(read.query_projection_version()?, None);
        read.token()?
    };
    {
        let lock = fixture.store.lock()?;
        let writer =
            fixture
                .store
                .current_write(&lock, &fixture.quota, &fixture.scope, CAPACITY)?;
        assert!(writer.pending()?.is_none());
        // Drop immediately after capability handoff, before preparing a journal.
    }
    let read = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(read.token()?, before);
    assert_eq!(
        read.query_projection_version()?,
        Some(airk_core::storage_format::NATIVE_QUERY_PROJECTION_VERSION)
    );
    assert!(read.file_scalars("missing")?.is_none());
    drop(read);
    assert!(serde_json::from_slice::<LegacyOwner>(&fs::read(&path)?).is_err());
    {
        let lock = fixture.store.lock()?;
        let mut writer =
            fixture
                .store
                .current_write(&lock, &fixture.quota, &fixture.scope, CAPACITY)?;
        writer.grow_capacity(CAPACITY * 2)?;
    }
    let grown: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
    assert_eq!(
        grown["query_projection_version"],
        airk_core::storage_format::NATIVE_QUERY_PROJECTION_VERSION
    );
    assert_eq!(grown["capacity"], CAPACITY * 2);
    let database = fixture.store.directory().join("current.sqlite3");
    let untouched = fs::read(&database)?;
    fs::write(
        &path,
        serde_json::to_vec(&FutureOwner {
            version: old.version,
            scope: fixture.scope.clone(),
            capacity: CAPACITY * 2,
            query_projection_version: 99,
        })?,
    )?;
    assert!(fixture.store.current_read(&fixture.scope).is_err());
    let lock = fixture.store.lock()?;
    assert!(
        fixture
            .store
            .current_write(&lock, &fixture.quota, &fixture.scope, CAPACITY * 2)
            .is_err()
    );
    assert_eq!(
        fs::read(database)?,
        untouched,
        "unsupported owner reached database write/recovery"
    );
    Ok(())
}

fn stored_totals(directory: &std::path::Path) -> Result<(u64, u64)> {
    let mut files = 0u64;
    let mut bytes = 0u64;
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let metadata = fs::symlink_metadata(&path)?;
        ensure!(!metadata.file_type().is_symlink(), "linked fixture storage");
        if metadata.is_dir() {
            let (child_files, child_bytes) = stored_totals(&path)?;
            files += child_files;
            bytes += child_bytes;
        } else if metadata.is_file() {
            files += 1;
            bytes += metadata.len();
        }
    }
    Ok((files, bytes))
}
#[test]
fn optional_projection_admission_preserves_mandatory_budget_and_stages_exact_new_objects()
-> Result<()> {
    let fixture = Fixture::new()?;
    let input =
        retained_input_with_facts(&fixture, "optional.txt", "body", vec![fact("Original", 0)])?;
    let full = fixture.store.load_projection_artifact(&input.artifact)?;
    let file = inventory(&fixture, &input, 1)?;
    let before = stored_totals(fixture.store.directory())?;
    let plan = fixture.store.prepare_native_fact_projection(
        &full,
        &file,
        airk_core::compression::CompressionMode::Off,
    )?;
    assert_eq!(
        stored_totals(fixture.store.directory())?,
        before,
        "preparation wrote optional state"
    );
    let writer = fixture.store.lock()?;
    let mandatory = fixture.quota.reserve(&fixture.scope.collection, 4 << 20)?;
    let old_policy = fixture.quota.policy()?;
    let status = fixture.quota.status()?;
    let mut tight = old_policy.clone();
    tight.root_bytes = status.used_bytes + status.reserved_bytes;
    fixture.quota.configure(tight)?;
    let declined = fixture.store.stage_optional_native_projections(
        &writer,
        &fixture.quota,
        std::slice::from_ref(&plan),
    )?;
    assert!(declined.roots.is_none());
    assert!(declined.reservation.is_none());
    assert_eq!(
        mandatory.charged(),
        0,
        "optional admission consumed mandatory allowance"
    );
    assert!(
        !fixture.store.directory().join("current-resources").exists(),
        "declined projection staged objects"
    );
    fixture.quota.configure(old_policy)?;
    let admitted = fixture.store.stage_optional_native_projections(
        &writer,
        &fixture.quota,
        std::slice::from_ref(&plan),
    )?;
    let allowance = admitted
        .reservation
        .as_ref()
        .context("new resources lacked auxiliary allowance")?;
    assert!(allowance.limit() > 0);
    assert_eq!(allowance.charged(), allowance.limit());
    assert_eq!(mandatory.charged(), 0);
    let repeated = fixture.store.stage_optional_native_projections(
        &writer,
        &fixture.quota,
        std::slice::from_ref(&plan),
    )?;
    assert!(
        repeated.reservation.is_none(),
        "existing objects were reserved again"
    );
    assert_eq!(
        serde_json::to_value(&admitted.roots)?,
        serde_json::to_value(&repeated.roots)?
    );
    drop(writer);
    publish(&fixture, "optional.txt", Some(&input), true)?;
    let read = fixture.store.current_read(&fixture.scope)?;
    assert_eq!(
        read.native_fact_projection(&fixture.store, "optional.txt")?
            .context("published projection missing")?
            .page(0)?
            .rows
            .len(),
        1
    );
    // The auxiliary guard remains alive through the actual publication above.
    assert!(admitted.reservation.is_some());
    Ok(())
}

#[test]
fn native_empty_edge_declaration_cannot_hide_actual_retained_relationships() -> Result<()> {
    let fixture = Fixture::new()?;
    let input = retained_input_with_facts(
        &fixture,
        "call.txt",
        "body",
        vec![super::postings_tests::written_call("written_target")],
    )?;
    let full = fixture.store.load_projection_artifact(&input.artifact)?;
    let file = inventory(&fixture, &input, 1)?;
    let reservation = fixture.quota.reserve(&fixture.scope.collection, 4 << 20)?;
    let mut forged = fixture.store.retain_current_metadata_with_facts_encoded(
        "call.txt",
        &file,
        &full,
        &reservation,
        airk_core::compression::CompressionMode::Off,
    )?;
    assert!(forged.native_edges.is_some());
    forged.native_edges = None;
    forged.native_edges_empty = true;
    let read = fixture.store.current_read(&fixture.scope)?;
    let token = read.token()?;
    let error = read
        .with_connection(|connection| {
            let mut plan =
                map::MapPlan::new(connection, &fixture.scope, read.head().artifact_root)?;
            metadata::prepare(
                &fixture.store,
                &fixture.scope,
                &mut plan,
                &[ArtifactChange::Upsert(Box::new(input))],
                &[forged],
                1,
            )
        })
        .err()
        .context("false empty declaration was accepted")?;
    assert!(error.to_string().contains("empty native edge declaration"));
    assert_eq!(read.token()?, token);
    Ok(())
}

#[test]
fn actual_projection_storage_overhead_and_batch_counter_admission_are_visible() -> Result<()> {
    let mut totals = Vec::new();
    for projected in [false, true] {
        let fixture = Fixture::new()?;
        let facts = (0..1024).map(|id| fact("Repeated", id)).collect();
        let input =
            retained_input_with_facts_budget(&fixture, "batch.txt", "body", facts, 8 << 20)?;
        let before = stored_totals(fixture.store.directory())?;
        publish_options(
            &fixture,
            "batch.txt",
            Some(&input),
            false,
            None,
            airk_core::compression::CompressionMode::Space,
            projected,
        )?;
        let after = stored_totals(fixture.store.directory())?;
        let read = fixture.store.current_read(&fixture.scope)?;
        let observed = read.native_projection_stats_page(&fixture.store, None, 1)?;
        assert_eq!(observed.counts.files, 1);
        if projected {
            assert_eq!(observed.counts.facts, 1024);
            assert_eq!(observed.counts.missing_files, 0);
        } else {
            assert_eq!(observed.counts.facts, 0);
            assert_eq!(observed.counts.missing_files, 1);
        }
        let counts = serde_json::json!({"projected":projected,"before_files":before.0,"before_stored_bytes":before.1,
            "after_files":after.0,"after_stored_bytes":after.1,"allocated_bytes":"unmeasured","projection":observed.counts});
        eprintln!("native_projection_total_storage {counts}");
        totals.push(after);
    }
    assert!(
        totals[1].0 > totals[0].0,
        "new retained objects were hidden"
    );
    let mut overflow = super::super::native_projection::NativeProjectionBatchStats {
        files: u64::MAX,
        ..Default::default()
    };
    let original = serde_json::to_value(&overflow)?;
    assert!(overflow.add(None).is_err());
    assert_eq!(
        serde_json::to_value(&overflow)?,
        original,
        "failed accounting partially mutated counters"
    );
    Ok(())
}
#[test]
fn source_line_witness_streams_utf8_boundaries_raw_and_compressed_without_source_copy() -> Result<()>
{
    let fixture = Fixture::new()?;
    let text = format!(
        "{}雪\r\n{}\ntail",
        "a".repeat((1 << 20) - 1),
        "β".repeat(1000)
    );
    let input = retained_input_with_facts_budget(&fixture, "utf8.txt", &text, Vec::new(), 8 << 20)?;
    let expected = text.split_inclusive('\n').count();
    assert_eq!(
        fixture
            .store
            .verify_retained_source_utf8_lines(&input.artifact)?,
        expected
    );
    let retrieval = fixture.store.load_retrieval_artifact(&input.artifact)?;
    let reservation = fixture.quota.reserve(&fixture.scope.collection, 8 << 20)?;
    let encoded = fixture.store.retain_payload_encoded(
        &retrieval.artifact,
        &retrieval.payload,
        &retrieval.profile,
        Some(text.as_bytes()),
        Some(&reservation),
        airk_core::compression::CompressionMode::Space,
    )?;
    assert!(encoded.retained.as_ref().unwrap().source_encoding.is_some());
    assert_eq!(
        fixture.store.verify_retained_source_utf8_lines(&encoded)?,
        expected
    );
    let raw = fixture
        .store
        .directory()
        .join("objects/sources")
        .join(&input.artifact.content_hash);
    let mut corrupt = text.as_bytes().to_vec();
    corrupt[0] = 0xff;
    fs::write(&raw, &corrupt)?;
    let error = fixture
        .store
        .verify_retained_source_utf8_lines(&input.artifact)
        .unwrap_err();
    assert!(
        format!("{error:#}").contains("digest mismatch"),
        "UTF-8 error masked failed source identity"
    );
    fs::write(raw, text.as_bytes())?;
    Ok(())
}

#[test]
fn physical_leaf_continuation_requires_exact_certificate_and_reports_no_progress_budget()
-> Result<()> {
    let fixture = Fixture::new()?;
    let input = retained_input_with_facts_budget(
        &fixture,
        "paged.txt",
        "body",
        (0..600).map(|id| fact("SameName", id)).collect(),
        8 << 20,
    )?;
    publish(&fixture, "paged.txt", Some(&input), true)?;
    let read = fixture.store.current_read(&fixture.scope)?;
    let certified = read.certify_file_projections(&fixture.store, "paged.txt")?;
    assert!(certified.scalars);
    let certificate = certified.native.context("native certification absent")?;
    let projection = read
        .native_fact_projection(&fixture.store, "paged.txt")?
        .unwrap();
    assert!(projection.header().pages.len() >= 3);
    let required =
        projection.manifest_bytes() as u64 + projection.header().pages[0].resource.bytes as u64;
    let refused = read.audit_native_fact_pages(
        &fixture.store,
        "paged.txt",
        &certificate,
        0,
        1,
        required - 1,
    )?;
    assert_eq!(refused.next_page, 0);
    assert_eq!(refused.checked_pages, 0);
    assert_eq!(refused.required_bytes, Some(required));
    let mut after = 0;
    while after < certificate.pages {
        let turn = read.audit_native_fact_pages(
            &fixture.store,
            "paged.txt",
            &certificate,
            after,
            1,
            64 << 20,
        )?;
        assert_eq!(turn.checked_pages, 1);
        assert_eq!(turn.next_page, after + 1);
        assert_eq!(turn.complete, turn.next_page == certificate.pages);
        after = turn.next_page;
    }
    let mut foreign = certificate.clone();
    foreign.profile = airk_core::identity::Sha256Digest::from_bytes([91; 32]);
    assert!(
        read.audit_native_fact_pages(&fixture.store, "paged.txt", &foreign, 0, 1, 64 << 20,)
            .is_err()
    );
    // Certification authenticates the producer; every later physical turn still
    // checks the selected bytes, including leaves not examined in an earlier turn.
    let resource = &projection.header().pages[1].resource;
    let path = fixture
        .store
        .directory()
        .join("current-resources")
        .join(format!("{}.json", resource.object_digest()?));
    let bytes = fs::read(&path)?;
    fs::write(&path, b"corrupt leaf")?;
    assert!(
        read.audit_native_fact_pages(&fixture.store, "paged.txt", &certificate, 1, 1, 64 << 20,)
            .is_err()
    );
    fs::write(path, bytes)?;
    Ok(())
}

#[test]
fn manifest_bound_covers_one_leaf_per_supported_fact_with_encoded_descriptors() -> Result<()> {
    use super::native_projection::{MAX_MANIFEST_BYTES, NativeFactHeader, NativeFactPageRef};
    let digest = Sha256Digest::from_bytes([255; 32]);
    let resource = resources::ResourceReference {
        digest,
        bytes: resources::MAX_RESOURCE_BYTES,
        encoding: Some(airk_core::compression::EncodedObject {
            version: 1,
            logical_digest: digest.to_string(),
            stored_digest: digest.to_string(),
            stored_bytes: u64::MAX,
            index_digest: digest.to_string(),
        }),
        projection: None,
    };
    // This intentionally overstates valid aggregate byte counters and path escape
    // expansion, while covering the producer's maximum distinct leaf references.
    let manifest = NativeFactHeader {
        version: 1,
        path: "\u{0001}".repeat(8192),
        source: digest,
        payload: digest,
        file_metadata: digest,
        profile: digest,
        facts: 10_000,
        pages: (0..10_000)
            .map(|start| NativeFactPageRef {
                start,
                rows: 1,
                resource: resource.clone(),
            })
            .collect(),
        rank_by_ordinal: (0..10_000).collect(),
        row_encoded_bytes: u64::MAX,
        leaf_encoded_bytes: u64::MAX,
        leaf_stored_bytes: u64::MAX,
    };
    let bytes = serde_json::to_vec(&manifest)?.len();
    assert!(bytes <= MAX_MANIFEST_BYTES, "manifest bytes={bytes}");
    eprintln!("native_projection_manifest_bound: facts=10000 leaves=10000 encoded_bytes={bytes}");
    Ok(())
}

#[test]
fn projected_replacement_certifies_new_input_before_publish_and_removes_old_handles() -> Result<()>
{
    let fixture = Fixture::new()?;
    let first = retained_input_with_facts(&fixture, "same.txt", "body", vec![fact("Old", 0)])?;
    let first_token = publish(&fixture, "same.txt", Some(&first), true)?;
    let replacement =
        retained_input_with_facts(&fixture, "same.txt", "body", vec![fact("New", 0)])?;
    {
        let read = fixture.store.current_read(&fixture.scope)?;
        let old_native = read
            .native_fact_reference("same.txt")?
            .context("original native reference missing")?
            .0;
        let file = inventory(&fixture, &replacement, read.head().sequence + 1)?;
        let full = fixture
            .store
            .load_projection_artifact(&replacement.artifact)?;
        let reservation = fixture.quota.reserve(&fixture.scope.collection, 64 << 20)?;
        let mut reference = fixture.store.retain_current_metadata_with_facts_encoded(
            "same.txt",
            &file,
            &full,
            &reservation,
            airk_core::compression::CompressionMode::Off,
        )?;
        // New metadata/scalars are valid, but this manifest certifies the OLD
        // retained extraction. It must fail the actual new-upsert producer check.
        reference.native_facts = Some(old_native);
        let connection = database::open_reader(&fixture.store.directory().join("current.sqlite3"))?;
        let mut plan = map::MapPlan::new(&connection, &fixture.scope, read.head().artifact_root)?;
        let changes = [ArtifactChange::Upsert(Box::new(replacement.clone()))];
        assert!(
            metadata::prepare(
                &fixture.store,
                &fixture.scope,
                &mut plan,
                &changes,
                &[reference],
                read.head().sequence + 1
            )
            .is_err()
        );
        assert_eq!(
            fixture.store.current_read(&fixture.scope)?.token()?,
            first_token
        );
        let native = read
            .native_fact_projection(&fixture.store, "same.txt")?
            .context("original projection lost after refused replacement")?;
        assert_eq!(native.page(0)?.rows[0].fact.name, "Old");
    }
    let second_token = publish(&fixture, "same.txt", Some(&replacement), true)?;
    assert_ne!(second_token, first_token);
    {
        let read = fixture.store.current_read(&fixture.scope)?;
        let native = read
            .native_fact_projection(&fixture.store, "same.txt")?
            .context("replacement projection missing")?;
        assert_eq!(native.page(0)?.rows[0].fact.name, "New");
        assert!(read.audit_native_fact_projection(&fixture.store, "same.txt")?);
        let query = |name: &str| airk_core::FactQuery {
            path_prefix: "same.txt".into(),
            name: Some(name.into()),
            kind: None,
            language: None,
            exact_name: true,
            limit: 10,
            max_bytes: 65536,
            path_exact: true,
            document_only: false,
        };
        assert!(
            read.native_facts_page(&fixture.store, &query("Old"), false, &Default::default())?
                .facts
                .is_empty()
        );
        assert_eq!(
            read.native_facts_page(&fixture.store, &query("New"), false, &Default::default())?
                .facts
                .len(),
            1
        );
    }
    publish(&fixture, "same.txt", None, true)?;
    let read = fixture.store.current_read(&fixture.scope)?;
    assert!(
        read.native_fact_projection(&fixture.store, "same.txt")?
            .is_none()
    );
    assert!(read.file_metadata(&fixture.store, "same.txt")?.is_none());
    Ok(())
}

#[test]
fn native_line_index_preserves_boundaries_and_strict_damage_refusal_through_replay() -> Result<()> {
    use airk_core::compression::CompressionMode;
    for mode in [
        CompressionMode::Off,
        CompressionMode::Automatic,
        CompressionMode::Space,
    ] {
        let fixture = Fixture::new()?;
        let text = format!(
            "{}🦀\r\n{}\n{}tail",
            "x".repeat(65_535),
            "y".repeat(2 << 20),
            "\n".repeat(10_000)
        );
        let initial =
            retained_input_with_facts_budget(&fixture, "lines.txt", &text, Vec::new(), 16 << 20)?;
        let mut retrieval = fixture.store.load_retrieval_artifact(&initial.artifact)?;
        retrieval.profile.contract_version = 2;
        retrieval.payload.contract_version = 2;
        let artifact = airk_core::binding::bind_payload(
            &retrieval.payload,
            &retrieval.profile,
            airk_core::binding::ArtifactBinding {
                path: "lines.txt",
                source_hash: initial.artifact.content_hash.parse()?,
                source_bytes: text.len(),
            },
        )?;
        let reservation = fixture.quota.reserve(&fixture.scope.collection, 16 << 20)?;
        let compact = fixture.store.retain_payload_encoded(
            &artifact,
            &retrieval.payload,
            &retrieval.profile,
            Some(text.as_bytes()),
            Some(&reservation),
            mode,
        )?;
        let input = CapturedInput::new(compact);
        let mut file = inventory(&fixture, &input, 1)?;
        file.indexed_lines = text.split_inclusive('\n').count();
        publish_options(
            &fixture,
            "lines.txt",
            Some(&input),
            true,
            Some(file.clone()),
            mode,
            true,
        )?;
        let read = fixture.store.current_read(&fixture.scope)?;
        let range = read
            .native_line_projection(&fixture.store, "lines.txt")?
            .context("native range missing")?;
        assert_eq!(range.lines(), file.indexed_lines);
        for (start, end) in [
            (1, 1),
            (2, 2),
            (3, file.indexed_lines),
            (file.indexed_lines, file.indexed_lines),
        ] {
            for budget in [1, 3, 65_536] {
                let expected: String = text
                    .split_inclusive('\n')
                    .skip(start - 1)
                    .take(end - start + 1)
                    .collect();
                let mut length = expected.len().min(budget);
                while !expected.is_char_boundary(length) {
                    length -= 1;
                }
                let selected = range.read_selected(start, end, budget)?;
                assert_eq!(selected.text, expected[..length]);
                assert_eq!(selected.truncated, length < expected.len());
            }
        }
        assert_eq!(
            range
                .read_strict(file.indexed_lines, file.indexed_lines, 100)?
                .text,
            "tail"
        );
        assert!(
            read.certify_file_projections(&fixture.store, "lines.txt")?
                .scalars
        );
        let retained = input.artifact.retained.as_ref().unwrap();
        if retained.source_encoding.is_none() {
            let source = fixture
                .store
                .directory()
                .join("objects/sources")
                .join(&input.artifact.content_hash);
            let mut corrupt = text.as_bytes().to_vec();
            corrupt[1 << 20] ^= 1;
            fs::write(&source, &corrupt)?;
            assert!(
                range.read_selected(1, 1, 100).is_ok(),
                "internal selected helper read an unrelated block"
            );
            assert!(
                range.read_strict(1, 1, 100).is_err(),
                "public strict reader ignored unselected damage"
            );
            corrupt[0] ^= 1;
            fs::write(&source, &corrupt)?;
            assert!(range.read_selected(1, 1, 100).is_err());
            fs::write(source, text.as_bytes())?;
        }
        let reference = read.native_line_reference("lines.txt")?.unwrap();
        let original_index = fixture.store.read_current_resource(&reference)?;
        let parsed: serde_json::Value = serde_json::from_slice(&original_index)?;
        let needle = format!(
            "\"sha256\":\"{}\"",
            parsed["blocks"][0]["sha256"].as_str().unwrap()
        );
        let forged_bytes = String::from_utf8(original_index)?.replacen(
            &needle,
            &format!("\"sha256\":\"{}\"", "00".repeat(32)),
            1,
        );
        let forged_index = fixture.store.retain_current_resource_encoded_counted(
            forged_bytes.as_bytes(),
            &reservation,
            mode,
            None,
        )?;
        let metadata_bytes = read
            .map_get(map::MapDomain::FileMetadata, b"lines.txt")?
            .unwrap();
        let mut forged: metadata::FileMetadataRef = serde_json::from_slice(&metadata_bytes)?;
        forged.native_lines = Some(forged_index);
        let error = read
            .with_connection(|connection| {
                let mut map =
                    map::MapPlan::new(connection, &fixture.scope, read.head().artifact_root)?;
                metadata::prepare(
                    &fixture.store,
                    &fixture.scope,
                    &mut map,
                    &[ArtifactChange::Upsert(Box::new(input.clone()))],
                    &[forged],
                    2,
                )
            })
            .err()
            .context("forged source block digest was accepted")?;
        assert!(
            format!("{error:#}").contains("differs from original source"),
            "{error:#}"
        );
        let object = fixture
            .store
            .directory()
            .join("current-resources")
            .join(format!("{}.json", reference.object_digest()?));
        let original = fs::read(&object)?;
        fs::write(&object, b"damaged line index")?;
        assert!(
            read.native_line_projection(&fixture.store, "lines.txt")
                .is_err()
        );
        assert!(
            read.certify_file_projections(&fixture.store, "lines.txt")
                .is_err()
        );
        fs::write(&object, &original)?;
        let plan = read.reclamation_plan(&fixture.store)?;
        drop(range);
        drop(read);
        let lock = fixture.store.lock()?;
        let mut writer =
            fixture
                .store
                .current_write(&lock, &fixture.quota, &fixture.scope, CAPACITY)?;
        writer.reclaim_objects(&plan)?;
        assert!(object.is_file(), "current line resource was reclaimed");
    }
    Ok(())
}

#[test]
fn compressed_resources_refuse_decoded_size_above_the_selected_reference() -> Result<()> {
    let fixture = Fixture::new()?;
    let reservation = fixture.quota.reserve(&fixture.scope.collection, 2 << 20)?;
    let bytes = vec![b'x'; 1 << 20];
    let mut reference = fixture.store.retain_current_resource_encoded_counted(
        &bytes,
        &reservation,
        airk_core::compression::CompressionMode::Space,
        None,
    )?;
    assert!(reference.encoding.is_some());
    reference.bytes = 1;
    let error = fixture.store.read_current_resource(&reference).unwrap_err();
    assert!(
        format!("{error:#}").contains("decoded bound mismatch"),
        "{error:#}"
    );
    Ok(())
}

#[test]
fn retained_text_ranges_preserve_overlap_original_locations_and_selected_damage_boundaries()
-> Result<()> {
    use airk_core::{
        DocumentLocation as L,
        compression::CompressionMode,
        extraction::{LocalChunk, LocalSpan},
        location::{ByteSpan, RegionLocation},
    };
    let locations = [
        L::Page { number: 1 },
        L::Block { number: 2 },
        L::Slide { number: 3 },
        L::TableRow { table: 4, row: 5 },
        L::WorksheetRow {
            sheet: "Original sheet".into(),
            row: 6,
        },
        L::Mapped {
            region: RegionLocation::Embedded {
                region_id: "decoded-region".into(),
                local_span: ByteSpan { start: 0, end: 4 },
                original_spans: vec![ByteSpan { start: 1, end: 5 }],
            },
        },
    ];
    for (index, location) in locations.into_iter().enumerate() {
        let fixture = Fixture::new()?;
        let text = "first\nshared\nlast🦀\nnext\n";
        let seed = retained_input_at(&fixture, "document.txt", text)?;
        let mut retrieval = fixture.store.load_retrieval_artifact(&seed.artifact)?;
        retrieval.payload.extraction_method = "document-range-fixture".into();
        retrieval.payload.chunks = vec![
            LocalChunk {
                span: LocalSpan { start: 0, end: 13 },
                start_line: 1,
                end_line: 2,
                location: location.clone(),
                region: None,
            },
            LocalChunk {
                span: LocalSpan { start: 6, end: 22 },
                start_line: 2,
                end_line: 3,
                location: location.clone(),
                region: None,
            },
            LocalChunk {
                span: LocalSpan {
                    start: 22,
                    end: text.len(),
                },
                start_line: 4,
                end_line: 4,
                location: L::Page { number: 99 },
                region: None,
            },
        ];
        let artifact = airk_core::binding::bind_payload(
            &retrieval.payload,
            &retrieval.profile,
            airk_core::binding::ArtifactBinding {
                path: "document.txt",
                source_hash: seed.artifact.content_hash.parse()?,
                source_bytes: text.len(),
            },
        )?;
        let reservation = fixture.quota.reserve(&fixture.scope.collection, 2 << 20)?;
        let mode = [
            CompressionMode::Off,
            CompressionMode::Automatic,
            CompressionMode::Space,
        ][index % 3];
        let compact = fixture.store.retain_payload_encoded(
            &artifact,
            &retrieval.payload,
            &retrieval.profile,
            Some(text.as_bytes()),
            Some(&reservation),
            mode,
        )?;
        let input = CapturedInput::new(compact);
        let mut file = inventory(&fixture, &input, 1)?;
        file.indexed_lines = 4;
        publish_options(
            &fixture,
            "document.txt",
            Some(&input),
            true,
            Some(file),
            mode,
            true,
        )?;
        let read = fixture.store.current_read(&fixture.scope)?;
        let projected = read
            .text_range_projection(&fixture.store, "document.txt")?
            .context("text projection missing")?;
        let full = projected.read_selected(1, 3, 100)?;
        assert_eq!(full.text, "first\nshared\nshared\nlast🦀\n");
        assert_eq!(full.location, location);
        assert!(!full.truncated);
        assert_eq!(projected.read_selected(2, 2, 100)?.text, "shared\nshared\n");
        let short = projected.read_selected(3, 3, 7)?;
        assert_eq!(short.text, "last");
        assert!(short.truncated);
        assert!(projected.read_selected(3, 4, 100).is_err());
        assert_eq!(projected.read_strict(1, 3, 100)?.text, full.text);
        let certificate = read
            .certify_file_projections(&fixture.store, "document.txt")?
            .text_ranges
            .context("text certificate missing")?;
        let root = projected.reference();
        let header: serde_json::Value =
            serde_json::from_slice(&fixture.store.read_current_resource(&root.root)?)?;
        let first: resources::ResourceReference =
            serde_json::from_value(header["chunks"][0]["blocks"][0]["resource"].clone())?;
        let third: resources::ResourceReference =
            serde_json::from_value(header["chunks"][2]["blocks"][0]["resource"].clone())?;
        let refused = projected.audit_physical(
            &certificate,
            0,
            1,
            root.root.bytes as u64 + first.bytes as u64 - 1,
            || Ok(()),
        )?;
        assert_eq!(refused.next_page, 0);
        assert_eq!(refused.checked_pages, 0);
        assert_eq!(
            refused.required_bytes,
            Some(root.root.bytes as u64 + first.bytes as u64)
        );
        let object = fixture
            .store
            .directory()
            .join("current-resources")
            .join(format!("{}.json", third.object_digest()?));
        let original = fs::read(&object)?;
        fs::write(&object, b"broken text")?;
        assert!(projected.read_selected(1, 1, 100).is_ok());
        assert!(projected.read_strict(1, 1, 100).is_err());
        assert!(
            read.certify_file_projections(&fixture.store, "document.txt")?
                .text_ranges
                .is_some(),
            "logical certification unexpectedly read unselected data pages"
        );
        assert!(
            projected
                .audit_physical(&certificate, 2, 1, 1 << 20, || Ok(()))
                .is_err()
        );
        fs::write(&object, original)?;
        for page in 0..3 {
            let turn = projected.audit_physical(&certificate, page, 1, 1 << 20, || Ok(()))?;
            assert_eq!(turn.next_page, page + 1);
            assert_eq!(turn.complete, page == 2);
        }
        let plan = read.reclamation_plan(&fixture.store)?;
        drop(projected);
        drop(read);
        let lock = fixture.store.lock()?;
        fixture
            .store
            .current_write(&lock, &fixture.quota, &fixture.scope, CAPACITY)?
            .reclaim_objects(&plan)?;
        assert!(object.is_file());
    }
    Ok(())
}

#[test]
fn optional_text_metadata_refusal_preserves_valid_overlapping_authoritative_chunks() -> Result<()> {
    let fixture = Fixture::new()?;
    let seed = retained_input_at(&fixture, "overlap.txt", "body")?;
    let mut retrieval = fixture.store.load_retrieval_artifact(&seed.artifact)?;
    retrieval.payload.chunks = vec![retrieval.payload.chunks[0].clone(); 100_000];
    retrieval.payload.validate()?;
    let artifact = airk_core::binding::bind_payload(
        &retrieval.payload,
        &retrieval.profile,
        airk_core::binding::ArtifactBinding {
            path: "overlap.txt",
            source_hash: seed.artifact.content_hash.parse()?,
            source_bytes: 4,
        },
    )?;
    let reservation = fixture.quota.reserve(&fixture.scope.collection, 64 << 20)?;
    let compact = fixture.store.retain_payload_with_budget(
        &artifact,
        &retrieval.payload,
        &retrieval.profile,
        Some(b"body"),
        Some(&reservation),
    )?;
    let full = fixture.store.load_projection_artifact(&compact)?;
    let before = stored_totals(fixture.store.directory())?;
    let error = fixture
        .store
        .prepare_text_ranges(&full, airk_core::compression::CompressionMode::Off)
        .err()
        .context("optional text metadata did not decline oversized representation")?;
    assert!(error.is::<native_projection::ProjectionPreparationLimit>());
    assert_eq!(
        stored_totals(fixture.store.directory())?,
        before,
        "optional preparation wrote resources before admission"
    );
    assert_eq!(fixture.store.load_retained(&compact)?.chunks.len(), 100_000);
    assert_eq!(fixture.store.read_retained_source(&compact)?, b"body");
    Ok(())
}

#[test]
fn audit_retry_witness_ignores_unrelated_publications_and_tracks_only_affected_values() -> Result<()>
{
    let fixture = Fixture::new()?;
    let one = retained_input_at(&fixture, "one.txt", "body")?;
    publish(&fixture, "one.txt", Some(&one), false)?;
    let policy = Sha256Digest::from_bytes([83; 32]);
    let read = fixture.store.current_read(&fixture.scope)?;
    let file = read.file_audit_witness("one.txt", policy, 64 << 20)?;
    let imports = read.import_audit_witness(policy, 64 << 20)?;
    assert!(serde_json::to_vec(&file)?.len() < 1024);
    assert!(!read.audit_inputs_changed(&file, policy, 64 << 20)?);
    drop(read);
    let two = retained_input_at(&fixture, "two.txt", "unrelated")?;
    publish(&fixture, "two.txt", Some(&two), false)?;
    let read = fixture.store.current_read(&fixture.scope)?;
    assert!(!read.audit_inputs_changed(&file, policy, 64 << 20)?);
    assert!(!read.audit_inputs_changed(&imports, policy, 64 << 20)?);
    assert!(read.audit_inputs_changed(&file, policy, 128 << 20)?);
    assert!(read.audit_inputs_changed(&file, Sha256Digest::from_bytes([84; 32]), 64 << 20)?);
    drop(read);
    let changed = retained_input_at(&fixture, "one.txt", "changed")?;
    publish(&fixture, "one.txt", Some(&changed), false)?;
    let read = fixture.store.current_read(&fixture.scope)?;
    assert!(read.audit_inputs_changed(&file, policy, 64 << 20)?);
    assert!(!read.audit_inputs_changed(&imports, policy, 64 << 20)?);
    Ok(())
}
