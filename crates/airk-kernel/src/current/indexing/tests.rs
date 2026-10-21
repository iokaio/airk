// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn many_progressive_publications_do_not_spend_the_capture_reservation() -> Result<()> {
    let root = tempfile::tempdir()?;
    let data = tempfile::tempdir()?;
    for index in 0..40 {
        std::fs::write(
            root.path().join(format!("file_{index:02}.txt")),
            format!("authenticate original fixture {index}\n"),
        )?;
    }
    let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let previous = kernel.load_optional()?.unwrap();
    let scope = kernel.current_scope()?.unwrap();
    let read = kernel.store.current_read(&scope)?;
    let configuration = read.collection_state()?.unwrap().configuration;
    drop(read);
    let quota = Quota::open(&kernel.data_root)?;
    let writer = kernel.store.lock()?;
    // Stand in for the scan's still-live capture allowance. Temporary graph
    // capacities must not accumulate here as small publications complete.
    let capture = quota.reserve(kernel.collection(), 1 << 20)?;
    let selection = kernel.vector_selection()?;
    let execution = kernel.execution_status()?;
    let mut stats = EmbeddingCacheStats::default();
    let mut progressive = ProgressiveBase {
        kernel: &kernel,
        quota: &quota,
        writer: &writer,
        scope: &scope,
        configuration: &configuration,
        model: None,
        objects: None,
        selection: &selection,
        stats: &mut stats,
        execution: &execution,
        previous: &previous,
        snapshot: previous.clone(),
        semantic_work: None,
        semantic_failed: false,
        scan_work: progress::ScanWork::start()?,
        batch: Default::default(),
        batch_bytes: 0,
        last_flush: std::time::Instant::now(),
        last_pause_check: std::time::Instant::now(),
        next_batch: 1,
        processed: 0,
        committed: 0,
        current: Default::default(),
        first_ready_us: None,
        first_vector_us: None,
        started: std::time::Instant::now(),
    };
    for artifact in previous.artifacts.values() {
        progressive.processed += 1;
        progressive.batch.insert(
            artifact.path.clone(),
            ArtifactChange::Upsert(Box::new(CapturedInput {
                artifact: artifact.clone(),
            })),
        );
        progressive.flush()?;
    }
    assert_eq!(progressive.committed, 40);
    assert_eq!(capture.charged(), 0);
    assert_eq!(kernel.status()?.files, 40);
    assert!(
        kernel.read_file("file_39.txt", 1, 1, 4096)?.hits[0]
            .text
            .contains("fixture 39")
    );
    Ok(())
}

#[test]
fn progressive_flush_preserves_the_next_captured_input() -> Result<()> {
    // The extraction callback retains the next file before observe can flush
    // the preceding batch. Exercise both pre-insertion flush boundaries.
    for boundary in ["time", "bytes"] {
        let donor_root = tempfile::tempdir()?;
        let donor_data = tempfile::tempdir()?;
        for name in ["first", "second"] {
            std::fs::write(
                donor_root.path().join(format!("{name}.txt")),
                format!("original {name} capture\n"),
            )?;
        }
        let donor = Kernel::open(donor_root.path(), Some(donor_data.path().into()))?;
        donor.configure_vectors(false)?;
        donor.index()?;
        let captured = donor.load_optional()?.unwrap();
        let root = tempfile::tempdir()?;
        let data = tempfile::tempdir()?;
        std::fs::write(root.path().join("first.txt"), "original committed seed\n")?;
        let kernel = Kernel::open(root.path(), Some(data.path().into()))?;
        kernel.configure_vectors(false)?;
        kernel.index()?;
        let previous = kernel.load_optional()?.unwrap();
        let scope = kernel.current_scope()?.unwrap();
        let read = kernel.store.current_read(&scope)?;
        let configuration = read.collection_state()?.unwrap().configuration;
        drop(read);
        let quota = Quota::open(&kernel.data_root)?;
        let writer = kernel.store.lock()?;
        let captured_objects = writer.capture_objects();
        let reservation = quota.reserve(kernel.collection(), 16 << 20)?;
        let selection = kernel.vector_selection()?;
        let execution = kernel.execution_status()?;
        let mut stats = EmbeddingCacheStats::default();
        let mut progressive = ProgressiveBase {
            kernel: &kernel,
            quota: &quota,
            writer: &writer,
            scope: &scope,
            configuration: &configuration,
            model: None,
            objects: None,
            selection: &selection,
            stats: &mut stats,
            execution: &execution,
            previous: &previous,
            snapshot: previous.clone(),
            semantic_work: None,
            semantic_failed: false,
            scan_work: progress::ScanWork::start()?,
            batch: Default::default(),
            batch_bytes: 0,
            last_flush: std::time::Instant::now(),
            last_pause_check: std::time::Instant::now(),
            next_batch: 32,
            processed: 0,
            committed: 0,
            current: Default::default(),
            first_ready_us: None,
            first_vector_us: None,
            started: std::time::Instant::now(),
        };

        for (index, artifact) in captured.artifacts.values().enumerate() {
            let profile = artifact
                .retained
                .as_ref()
                .unwrap()
                .profile
                .as_ref()
                .unwrap();
            let retained = kernel.store.retain_payload_with_budget(
                &donor.store.load_retained(artifact)?,
                &donor.store.load_extraction_payload(artifact)?,
                profile,
                Some(&donor.store.read_retained_source(artifact)?),
                Some(&reservation),
            )?;
            if index == 1 {
                if boundary == "time" {
                    progressive.last_flush =
                        std::time::Instant::now() - std::time::Duration::from_secs(3);
                } else {
                    progressive.batch_bytes = 64 << 20;
                }
            }
            progressive.observe(&retained)?;
        }
        progressive.flush()?;
        assert_eq!(progressive.committed, 2, "{boundary}");
        drop(progressive);
        // Prior-content reuse can still need a superseded source during this scan.
        assert_eq!(
            kernel
                .store
                .read_retained_source(&previous.artifacts["first.txt"])?,
            b"original committed seed\n"
        );
        let read = kernel.store.current_read(&scope)?;
        let plan = read.reclamation_plan(&kernel.store)?;
        drop(read);
        let capacity = kernel.store.current_database_capacity(&scope)?;
        let mut current = kernel
            .store
            .current_write(&writer, &quota, &scope, capacity)?;
        let nested_capture = writer.capture_objects();
        drop(captured_objects);
        let deferred = current.reclaim_objects(&plan)?;
        assert!(deferred.pending);
        assert_eq!(deferred.removed_files, 0);
        drop(nested_capture);
        let reclaimed = current.reclaim_objects(&plan)?;
        assert!(reclaimed.removed_files > 0);
        assert!(!reclaimed.pending);
        assert!(
            kernel
                .store
                .read_retained_source(&previous.artifacts["first.txt"])
                .is_err()
        );
        drop(current);
        drop(writer);
        for name in ["first", "second"] {
            let result = kernel.read_file(&format!("{name}.txt"), 1, 1, 4096)?;
            assert!(
                result.hits[0]
                    .text
                    .contains(&format!("original {name} capture"))
            );
        }
    }
    Ok(())
}
