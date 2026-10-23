// SPDX-License-Identifier: Apache-2.0
//! Bounded configuration under the authenticated private service runtime directory.
use airk_core::service_resources::ResourceConfiguration;
use anyhow::{Result, ensure};
use std::{
    io::{Read, Write},
    path::Path,
};
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
    #[test]
    fn bounded_configuration_round_trips_and_rejects_invalid_values_without_replacement()
    -> Result<()> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().join("runtime");
        crate::local_ipc::prepare_root(&root)?;
        assert!(load(&root)?.memory_bytes.is_none());
        let first = ResourceConfiguration {
            memory_bytes: Some(2 << 30),
            ..Default::default()
        };
        save(&root, &first)?;
        let second = ResourceConfiguration {
            extraction_threads: Some(3),
            ..Default::default()
        };
        save(&root, &second)?;
        assert_eq!(load(&root)?.extraction_threads, Some(3));
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
