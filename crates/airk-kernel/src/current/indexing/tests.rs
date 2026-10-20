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
