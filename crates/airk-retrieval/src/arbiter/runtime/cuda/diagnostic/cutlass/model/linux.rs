// SPDX-License-Identifier: Apache-2.0
//! Retain the authenticated graph and external weights as owned bytes throughout native session creation.
use super::{GRAPH, WEIGHT};
use anyhow::{Result, ensure};
use ort::session::{Session, builder::SessionBuilder};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    time::Instant,
};

pub(in super::super) struct Inputs {
    graph: Vec<u8>,
    weights: Vec<u8>,
}

impl Inputs {
    pub(super) fn open(stage: &Path, deadline: Instant) -> Result<Self> {
        Self::open_paths(
            (&stage.join("model.onnx"), 854_973, GRAPH),
            (&stage.join("weights.bin"), 3_441_397_760, WEIGHT),
            deadline,
        )
    }

    pub(in super::super) fn open_paths(
        graph: (&Path, usize, &str),
        weights: (&Path, usize, &str),
        deadline: Instant,
    ) -> Result<Self> {
        for path in [graph.0, weights.0] {
            ensure!(
                path.is_absolute() && path.canonicalize()? == path,
                "noncanonical GNU model asset"
            );
        }
        let graph = read_asset(graph.0, graph.1, graph.2, deadline)?;
        let sizes = std::collections::BTreeMap::from([(
            "weights.bin".to_owned(),
            u64::try_from(weights.1)?,
        )]);
        ensure!(
            crate::arbiter::external_data::half_initializers(&graph, &sizes)?.is_empty(),
            "unexpected >=2GiB individual initializer; native OrtValue binding required"
        );
        Ok(Self {
            graph,
            weights: read_asset(weights.0, weights.1, weights.2, deadline)?,
        })
    }

    pub(in super::super) fn session(self, builder: SessionBuilder) -> Result<Session> {
        Ok(builder
            .with_external_initializer_file_in_memory("weights.bin", self.weights.into())?
            .commit_from_memory(&self.graph)?)
    }
}

fn read_asset(path: &Path, size: usize, expected: &str, deadline: Instant) -> Result<Vec<u8>> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    let uid = unsafe { libc::geteuid() };
    ensure!(
        uid != 0 && metadata.uid() == uid && metadata.is_file() && metadata.len() == size as u64,
        "unexpected GNU Compact asset kind, owner or length"
    );
    let mut result = Vec::new();
    result.try_reserve_exact(size)?;
    let mut hash = Sha256::new();
    let mut chunk = vec![0; 1 << 20];
    loop {
        ensure!(
            Instant::now() < deadline,
            "GNU Compact asset authentication expired"
        );
        let read = file.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        ensure!(
            result.len().checked_add(read).is_some_and(|n| n <= size),
            "GNU Compact asset grew"
        );
        hash.update(&chunk[..read]);
        result.extend_from_slice(&chunk[..read]);
    }
    let current = fs::symlink_metadata(path)?;
    ensure!(
        current.is_file()
            && current.dev() == metadata.dev()
            && current.ino() == metadata.ino()
            && result.len() == size
            && format!("{:x}", hash.finalize()) == expected,
        "GNU Compact asset identity or authenticated contents changed"
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{os::unix::fs::symlink, time::Duration};

    #[test]
    fn retained_asset_survives_source_replacement_and_refuses_aliases() -> Result<()> {
        let root = tempfile::tempdir()?;
        let path = root.path().join("weights.bin");
        fs::write(&path, b"abc")?;
        let expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let deadline = Instant::now() + Duration::from_secs(5);
        let bytes = read_asset(&path, 3, expected, deadline)?;
        fs::write(&path, b"xyz")?;
        assert_eq!(bytes, b"abc");
        assert!(read_asset(&path, 3, expected, deadline).is_err());
        let alias = root.path().join("alias");
        symlink(&path, &alias)?;
        assert!(read_asset(&alias, 3, expected, deadline).is_err());
        Ok(())
    }
}
