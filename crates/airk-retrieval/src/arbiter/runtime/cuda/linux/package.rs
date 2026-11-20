// SPDX-License-Identifier: Apache-2.0
//! Observe files distributable with an already formed Linux product descriptor.
use super::*;
use airk_core::arbiter::Asset;
use manifest::linux::Descriptor;

fn product(raw: &[u8]) -> Result<Descriptor> {
    let descriptor = Descriptor::parse(raw)?;
    ensure!(
        descriptor.is_product(),
        "Linux CUDA package projection requires the product descriptor"
    );
    Ok(descriptor)
}

fn read_asset(
    root: &Path,
    asset: &Asset,
    limit: u64,
    deadline: Instant,
) -> Result<(Held, Vec<u8>)> {
    let mut file = Held::open(&root.join(&asset.name), false, asset.bytes.min(limit))?;
    let raw = file.metadata_bytes(deadline)?;
    ensure!(
        raw.len() as u64 == asset.bytes && file.sha == asset.sha256,
        "actual Linux package asset differs"
    );
    Ok((file, raw))
}

pub(in super::super) fn observe_package_projection(
    root: &Path,
    path: &Path,
    expected: Sha256Digest,
    deadline: Instant,
) -> Result<Vec<u8>> {
    let mut description = Held::open(path, false, 128 << 10)?;
    let raw = description.metadata_bytes(deadline)?;
    ensure!(
        description.sha == expected,
        "pinned Linux package descriptor differs"
    );
    let descriptor = product(&raw)?;
    let name = path
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .context("package descriptor basename")?;
    let descriptor_row =
        serde_json::json!({"name":name, "bytes":raw.len(), "sha256":description.sha});
    let mut rows = BTreeMap::from([(name.to_owned(), descriptor_row.clone())]);
    let mut held = vec![description];
    let provenance = descriptor.provenance();
    let (proof, bytes) = read_asset(root, provenance, 1 << 20, deadline)?;
    let legal = descriptor.verify_provenance(&bytes)?;
    let mut add = |name: &str, bytes: u64, file: Held| -> Result<()> {
        let row = serde_json::json!({"name":name, "bytes":bytes, "sha256":file.sha});
        ensure!(
            rows.insert(name.to_owned(), row).is_none(),
            "Linux package asset aliases another row"
        );
        held.push(file);
        Ok(())
    };
    add(&provenance.name, provenance.bytes, proof)?;
    for file in descriptor.files() {
        let mut object = Held::open(&root.join(&file.name), false, file.bytes.min(2_u64 << 30))?;
        let actual = object.inspect(&file.name, false, deadline)?;
        ensure!(&actual == file, "Linux package payload ELF differs");
        add(&file.name, actual.bytes, object)?;
    }
    for item in legal {
        let (file, _) = read_asset(root, &item, 16 << 20, deadline)?;
        add(&item.name, item.bytes, file)?;
    }
    for file in &held {
        file.recheck(deadline)?;
    }
    let bytes = serde_json::to_vec(&serde_json::json!({
        "schema":"airk.native.cuda-package-projection/1",
        "descriptor":descriptor_row,
        "static_core_build":descriptor.build(),
        "files":rows.into_values().collect::<Vec<_>>(),
        "scope":"metadata-only held package census; not activation, device eligibility or qualification",
        "qualification":false, "activation":false
    }))?;
    ensure!(
        bytes.len() <= 1 << 20,
        "Linux native package projection output cap"
    );
    in_time(deadline)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scientific_descriptor_cannot_project_distributable_product_files() -> Result<()> {
        let raw = serde_json::to_vec(&manifest::linux::scientific_test_manifest())?;
        Descriptor::parse(&raw)?;
        assert!(product(&raw).is_err());
        Ok(())
    }
}
