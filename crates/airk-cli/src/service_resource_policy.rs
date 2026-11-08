// SPDX-License-Identifier: Apache-2.0
//! Bounded configuration under the authenticated private service runtime directory.
use airk_core::service_resources::ResourceConfiguration;
use anyhow::{Result, ensure};
use std::{
    future::Future,
    io::{Read, Write},
    path::Path,
    sync::Arc,
};
pub(crate) type QueryCapacityReader = Arc<dyn Fn() -> usize + Send + Sync>;
tokio::task_local! {
    static HTTP_QUERY_CAPACITY: QueryCapacityReader;
}
/// Keep nested canonical reads on the actual service controller's live capacity.
pub(crate) async fn with_query_capacity<F: Future>(
    reader: QueryCapacityReader,
    future: F,
) -> F::Output {
    HTTP_QUERY_CAPACITY.scope(reader, future).await
}
/// Capture before spawning: Tokio task locals do not propagate to child tasks.
pub(crate) fn inherit_query_capacity<F: Future>(future: F) -> impl Future<Output = F::Output> {
    let reader = HTTP_QUERY_CAPACITY.try_with(Clone::clone).ok();
    async move {
        match reader {
            Some(reader) => with_query_capacity(reader, future).await,
            None => future.await,
        }
    }
}
fn check(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) => {
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink() && metadata.len() <= 4096,
                "unsafe resource configuration"
            );
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    metadata.file_attributes() & 0x400 == 0,
                    "redirected resource configuration"
                );
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}
/// Reads only bounded service-owned scalar metadata; never opens a collection.
pub(crate) fn query_concurrency(root: &Path) -> usize {
    if let Ok(capacity) = HTTP_QUERY_CAPACITY.try_with(|reader| reader()) {
        return capacity;
    }
    load(root).map_or(
        airk_core::service_resources::DEFAULT_QUERY_CONCURRENCY,
        |c| c.query_concurrency,
    )
}
pub fn load(root: &Path) -> Result<ResourceConfiguration> {
    let path = root.join("resources-1.4.json");
    check(&path)?;
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Default::default()),
        Err(error) => return Err(error.into()),
    };
    let mut bytes = Vec::new();
    file.take(4097).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 4096, "oversize resource configuration");
    let configuration: ResourceConfiguration = serde_json::from_slice(&bytes)?;
    configuration.validate()?;
    Ok(configuration)
}
pub fn save(root: &Path, configuration: &ResourceConfiguration) -> Result<()> {
    configuration.validate()?;
    let path = root.join("resources-1.4.json");
    check(&path)?;
    let bytes = serde_json::to_vec(configuration)?;
    ensure!(bytes.len() <= 4096, "oversize resource configuration");
    let mut staged = tempfile::Builder::new()
        .prefix(".resources-")
        .tempfile_in(root)?;
    staged.write_all(&bytes)?;
    staged.as_file().sync_all()?;
    staged.persist(path)?;
    #[cfg(unix)]
    std::fs::File::open(root)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn http_capacity_is_live_isolated_and_inherited_without_reading_policy() -> Result<()> {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let directory = tempfile::tempdir()?;
        // A malformed persistent file must never reset the controller's value.
        std::fs::write(directory.path().join("resources-1.4.json"), b"invalid")?;
        let capacity = Arc::new(AtomicUsize::new(4));
        let observed = capacity.clone();
        let reader: QueryCapacityReader = Arc::new(move || observed.load(Ordering::Acquire));
        with_query_capacity(reader, async {
            assert_eq!(query_concurrency(directory.path()), 4);
            for limit in [8, 16, 32, 64, 4] {
                capacity.store(limit, Ordering::Release);
                assert_eq!(query_concurrency(directory.path()), limit);
                let root = directory.path().to_path_buf();
                assert_eq!(
                    tokio::spawn(inherit_query_capacity(async move {
                        tokio::task::yield_now().await;
                        query_concurrency(&root)
                    }))
                    .await
                    .expect("owned capacity task"),
                    limit
                );
            }
        })
        .await;
        assert_eq!(
            query_concurrency(directory.path()),
            airk_core::service_resources::DEFAULT_QUERY_CONCURRENCY
        );
        Ok(())
    }
    #[test]
    fn bounded_configuration_round_trips_and_rejects_invalid_values_without_replacement()
    -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("runtime");
        crate::local_ipc::prepare_root(&root)?;
        assert!(load(&root)?.memory_bytes.is_none());
        assert_eq!(load(&root)?.query_concurrency, 16);
        let first = ResourceConfiguration {
            memory_bytes: Some(2 << 30),
            ..Default::default()
        };
        save(&root, &first)?;
        let second = ResourceConfiguration {
            query_concurrency: 64,
            extraction_threads: Some(3),
            ..Default::default()
        };
        save(&root, &second)?;
        assert_eq!(load(&root)?.extraction_threads, Some(3));
        assert_eq!(load(&root)?.query_concurrency, 64);
        for query_concurrency in [0, 2, 5, 63, 66] {
            assert!(
                save(
                    &root,
                    &ResourceConfiguration {
                        query_concurrency,
                        ..Default::default()
                    }
                )
                .is_err()
            );
            assert_eq!(load(&root)?.query_concurrency, 64);
        }
        assert!(
            save(
                &root,
                &ResourceConfiguration {
                    memory_bytes: Some(1),
                    ..Default::default()
                }
            )
            .is_err()
        );
        assert_eq!(load(&root)?.extraction_threads, Some(3));
        std::fs::write(root.join("resources-1.4.json"), vec![b' '; 4097])?;
        assert!(load(&root).is_err());
        Ok(())
    }
}
