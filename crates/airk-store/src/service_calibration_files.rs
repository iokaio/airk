// SPDX-License-Identifier: Apache-2.0
//! Exact offline calibration inputs share the model owner's finite disk allowance.
use super::*;

const CALIBRATION_OWNER: &[u8] = b"airk-service-calibration-files-v1\n";
const MAX_DEFINITION: u64 = 1 << 20;
const MAX_TOKENIZER: u64 = 32 << 20;

fn read_definition(path: &Path) -> Result<Vec<u8>> {
    directories(
        path.parent().context("calibration input parent missing")?,
        false,
    )?;
    let metadata = fs::symlink_metadata(path)?;
    reject_file_aliases(path, &metadata)?;
    ensure!(
        metadata.len() <= MAX_DEFINITION,
        "calibration definition exceeds bound"
    );
    let mut bytes = Vec::new();
    File::open(path)?
        .take(MAX_DEFINITION + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_DEFINITION,
        "calibration definition grew beyond bound"
    );
    Ok(bytes)
}
fn input_name(bytes: &[u8]) -> Result<(String, Sha256Digest)> {
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    ensure!(
        value["schema"] == 2,
        "shared calibration requires a transformer definition"
    );
    let name = value["tokenizer_file"]
        .as_str()
        .context("tokenizer filename missing")?;
    ensure!(
        !name.is_empty()
            && name.len() <= 128
            && !matches!(name, "." | "..")
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
        "tokenizer_file must be a bounded sibling filename"
    );
    let digest = value["tokenizer_sha256"]
        .as_str()
        .context("tokenizer digest missing")?
        .parse()?;
    Ok((name.into(), digest))
}
fn bounded_children(path: &Path, allowed: &[&str]) -> Result<()> {
    directories(path, false)?;
    for (index, entry) in fs::read_dir(path)?.enumerate() {
        let entry = entry?;
        ensure!(
            index < allowed.len() && allowed.iter().any(|name| entry.file_name() == *name),
            "unknown calibration files preserved"
        );
    }
    Ok(())
}
fn write_bytes(
    path: &Path,
    partial: &Path,
    bytes: &[u8],
    reservation: &crate::quota::Reservation,
) -> Result<()> {
    if entry_exists(path)? {
        return verify_bytes(path, bytes);
    }
    if entry_exists(partial)? {
        verify_prefix(partial, bytes)?;
    }
    reservation.charge(bytes.len() as u64)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(partial)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(partial, path)?;
    sync_directory(path.parent().unwrap())
}
impl ServiceModelFiles {
    /// The trusted adapter submits an explicitly selected local definition. No
    /// sample text, project state or usage ledger is copied into the global store.
    pub fn retain_calibration(&mut self, source: &Path) -> Result<PathBuf> {
        ensure!(
            source.is_absolute() && source.to_str().is_some_and(|s| s.len() <= 4096),
            "calibration source must be a bounded absolute path"
        );
        let bytes = read_definition(source)?;
        let (name, digest) = input_name(&bytes)?;
        let source_asset = source.parent().unwrap().join(&name);
        let metadata = fs::symlink_metadata(&source_asset)?;
        reject_file_aliases(&source_asset, &metadata)?;
        ensure!(
            metadata.len() > 0 && metadata.len() <= MAX_TOKENIZER,
            "calibration tokenizer exceeds bound"
        );
        let reference = ModelObjectRef {
            sha256: digest,
            bytes: metadata.len(),
        };
        let key = framed_digest(b"airk-shared-calibration/1.4", &[&bytes]);
        let root = self.root.join("calibration");
        directories(&root, true)?;
        fs::create_dir_all(&root)?;
        let owner = root.join("owner.txt");
        if !entry_exists(&owner)? {
            bounded_children(&root, &["owner.part"])?;
            let reservation = self.quota.reserve_root(CALIBRATION_OWNER.len() as u64)?;
            write_bytes(
                &owner,
                &root.join("owner.part"),
                CALIBRATION_OWNER,
                &reservation,
            )?;
        }
        verify_bytes(&owner, CALIBRATION_OWNER)?;
        let target = root.join(key.to_string());
        let stage = root.join(format!("stage-{key}"));
        let mut count = 0;
        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            if entry.file_name() == "owner.txt" {
                continue;
            }
            count += 1;
            ensure!(count <= MAX_SETS, "calibration set limit exceeded");
            let text = entry.file_name();
            let text = text.to_str().context("invalid calibration set name")?;
            let hash = text.strip_prefix("stage-").unwrap_or(text);
            ensure!(
                hash.len() == 64
                    && hash
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "unknown calibration set preserved"
            );
            directories(&entry.path(), false)?;
        }
        let manifest = if name.eq_ignore_ascii_case("definition.json") {
            "alternate-definition.json"
        } else {
            "definition.json"
        };
        let receipt = serde_json::to_vec(
            &serde_json::json!({"format":1,"definition":key,"manifest":manifest,"tokenizer":reference,"name":name}),
        )?;
        let validate = |directory: &Path, partial: bool| -> Result<()> {
            bounded_children(
                directory,
                if partial {
                    &[
                        "receipt.json",
                        "receipt.part",
                        "assets",
                        "definition.part",
                        "tokenizer.part",
                    ]
                } else {
                    &["receipt.json", "assets"]
                },
            )?;
            verify_bytes(&directory.join("receipt.json"), &receipt)?;
            let assets = directory.join("assets");
            bounded_children(&assets, &[manifest, &name])?;
            if !partial || entry_exists(&assets.join(manifest))? {
                verify_bytes(&assets.join(manifest), &bytes)?;
            }
            if !partial || entry_exists(&assets.join(&name))? {
                verify_asset(&assets.join(&name), &reference)?;
            }
            Ok(())
        };
        if entry_exists(&target)? {
            verify_asset(&source_asset, &reference)?;
            validate(&target, false)?;
            return Ok(target.join("assets").join(manifest));
        }
        ensure!(
            count < MAX_SETS || entry_exists(&stage)?,
            "calibration set limit reached"
        );
        let required = bytes.len() as u64 + reference.bytes + receipt.len() as u64;
        let reservation = self.quota.reserve_root(required)?;
        if !entry_exists(&stage)? {
            fs::create_dir(&stage)?;
        }
        bounded_children(
            &stage,
            &[
                "receipt.json",
                "receipt.part",
                "assets",
                "definition.part",
                "tokenizer.part",
            ],
        )?;
        write_bytes(
            &stage.join("receipt.json"),
            &stage.join("receipt.part"),
            &receipt,
            &reservation,
        )?;
        let assets = stage.join("assets");
        directories(&assets, true)?;
        fs::create_dir_all(&assets)?;
        validate(&stage, true)?;
        write_bytes(
            &assets.join(manifest),
            &stage.join("definition.part"),
            &bytes,
            &reservation,
        )?;
        let asset = assets.join(&name);
        if !entry_exists(&asset)? {
            let partial = stage.join("tokenizer.part");
            if entry_exists(&partial)? {
                let metadata = fs::symlink_metadata(&partial)?;
                reject_file_aliases(&partial, &metadata)?;
                ensure!(
                    metadata.len() <= reference.bytes,
                    "oversized calibration partial preserved"
                );
            }
            reservation.charge(reference.bytes)?;
            let mut input = File::open(&source_asset)?.take(reference.bytes + 1);
            let mut output = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&partial)?;
            let mut hash = Sha256::new();
            let mut length = 0u64;
            let mut buffer = [0; 64 * 1024];
            loop {
                let count = input.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                length += count as u64;
                ensure!(
                    length <= reference.bytes,
                    "calibration input grew during copy"
                );
                output.write_all(&buffer[..count])?;
                hash.update(&buffer[..count]);
            }
            ensure!(
                length == reference.bytes
                    && Sha256Digest::from_bytes(hash.finalize().into()) == reference.sha256,
                "calibration tokenizer digest mismatch"
            );
            output.sync_all()?;
            drop(output);
            fs::rename(partial, &asset)?;
            sync_directory(&assets)?;
        }
        validate(&stage, false)?;
        fs::rename(stage, &target)?;
        sync_directory(&root)?;
        Ok(target.join("assets").join(manifest))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(root: &Path, name: &str) -> Result<PathBuf> {
        let directory = root.canonicalize()?.join(name);
        fs::create_dir(&directory)?;
        let asset = b"original tokenizer input";
        fs::write(directory.join("tokenizer.json"), asset)?;
        let path = directory.join("selected.json");
        fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({"schema":2,
            "tokenizer_file":"tokenizer.json", "tokenizer_sha256":format!("{:x}",Sha256::digest(asset)),
            "model_revision_sha256":"a".repeat(64),"add_special_tokens":false,"framing":{"prefix":"","suffix":""}}))?,
        )?;
        Ok(path)
    }
    #[test]
    fn exact_global_capture_reuses_after_restart_and_preserves_selected_inputs() -> Result<()> {
        let root = tempfile::tempdir()?;
        let source = fixture(root.path(), "selected")?;
        let bytes = fs::read(&source)?;
        let mut files = ServiceModelFiles::open(root.path())?;
        let first = files.retain_calibration(&source)?;
        assert_eq!(fs::read(&first)?, bytes);
        assert!(
            first.starts_with(
                root.path()
                    .canonicalize()?
                    .join("model-files-1.4/calibration")
            )
        );
        assert_eq!(files.retain_calibration(&source)?, first);
        drop(files);
        let mut files = ServiceModelFiles::open(root.path())?;
        assert_eq!(files.retain_calibration(&source)?, first);
        fs::write(
            source.parent().unwrap().join("tokenizer.json"),
            b"changed selected input",
        )?;
        assert!(files.retain_calibration(&source).is_err());
        assert_eq!(
            fs::read(first.parent().unwrap().join("tokenizer.json"))?,
            b"original tokenizer input"
        );
        assert_eq!(fs::read(source)?, bytes);
        Ok(())
    }
    #[test]
    fn interrupted_capture_resumes_but_unknown_or_aliased_files_are_preserved() -> Result<()> {
        let root = tempfile::tempdir()?;
        let source = fixture(root.path(), "selected")?;
        let mut files = ServiceModelFiles::open(root.path())?;
        let installed = files.retain_calibration(&source)?;
        let target = installed.parent().unwrap().parent().unwrap().to_path_buf();
        let stage = target.with_file_name(format!(
            "stage-{}",
            target.file_name().unwrap().to_str().unwrap()
        ));
        fs::rename(&target, &stage)?;
        fs::rename(
            stage.join("assets/definition.json"),
            stage.join("definition.part"),
        )?;
        fs::rename(
            stage.join("assets/tokenizer.json"),
            stage.join("tokenizer.part"),
        )?;
        fs::write(stage.join("tokenizer.part"), b"original")?;
        assert_eq!(files.retain_calibration(&source)?, installed);
        fs::write(target.join("user.txt"), b"preserve unknown")?;
        assert!(files.retain_calibration(&source).is_err());
        assert_eq!(fs::read(target.join("user.txt"))?, b"preserve unknown");
        fs::remove_file(target.join("user.txt"))?;
        let asset = installed.parent().unwrap().join("tokenizer.json");
        fs::hard_link(&asset, root.path().join("alias"))?;
        assert!(files.retain_calibration(&source).is_err());
        assert_eq!(
            fs::read(root.path().join("alias"))?,
            b"original tokenizer input"
        );
        Ok(())
    }
    #[test]
    fn finite_sets_and_invalid_sources_fail_before_unbounded_copy() -> Result<()> {
        let root = tempfile::tempdir()?;
        let source = fixture(root.path(), "selected")?;
        let original = fs::read(&source)?;
        let mut files = ServiceModelFiles::open(root.path())?;
        assert!(
            files
                .retain_calibration(Path::new("relative.json"))
                .is_err()
        );
        let mut value: serde_json::Value = serde_json::from_slice(&original)?;
        value["tokenizer_file"] = serde_json::json!("../outside.json");
        fs::write(&source, serde_json::to_vec(&value)?)?;
        assert!(files.retain_calibration(&source).is_err());
        let mut value: serde_json::Value = serde_json::from_slice(&original)?;
        let mut installed = Vec::new();
        for index in 0..MAX_SETS {
            value["model_revision_sha256"] = serde_json::json!(format!("{index:064x}"));
            fs::write(&source, serde_json::to_vec(&value)?)?;
            installed.push(files.retain_calibration(&source)?);
        }
        value["model_revision_sha256"] = serde_json::json!("f".repeat(64));
        fs::write(&source, serde_json::to_vec(&value)?)?;
        assert!(files.retain_calibration(&source).is_err());
        assert!(installed.iter().all(|path| path.is_file()));
        fs::write(&source, vec![b' '; MAX_DEFINITION as usize + 1])?;
        assert!(files.retain_calibration(&source).is_err());
        Ok(())
    }
    #[cfg(unix)]
    #[test]
    fn dangling_partial_never_writes_through_to_a_foreign_path() -> Result<()> {
        let root = tempfile::tempdir()?;
        let source = fixture(root.path(), "selected")?;
        let mut files = ServiceModelFiles::open(root.path())?;
        let calibration = files.root.join("calibration");
        fs::create_dir(&calibration)?;
        let foreign = root.path().join("foreign-input");
        std::os::unix::fs::symlink(&foreign, calibration.join("owner.part"))?;
        assert!(files.retain_calibration(&source).is_err());
        assert!(!foreign.exists());
        assert!(
            fs::symlink_metadata(calibration.join("owner.part"))?
                .file_type()
                .is_symlink()
        );
        Ok(())
    }
    #[test]
    fn manifest_names_and_tokenizer_size_limits_do_not_overwrite_assets() -> Result<()> {
        let root = tempfile::tempdir()?;
        let source = fixture(root.path(), "selected")?;
        let definition = fs::read(&source)?;
        let mut files = ServiceModelFiles::open(root.path())?;
        for name in [
            "definition.json",
            "definition.json.part",
            "alternate-definition.json",
        ] {
            let input = source.parent().unwrap().join(name);
            fs::copy(source.parent().unwrap().join("tokenizer.json"), &input)?;
            let mut value: serde_json::Value = serde_json::from_slice(&definition)?;
            value["tokenizer_file"] = serde_json::json!(name);
            fs::write(&source, serde_json::to_vec(&value)?)?;
            let installed = files.retain_calibration(&source)?;
            assert_ne!(installed.file_name().unwrap(), name);
            assert_eq!(
                fs::read(installed.parent().unwrap().join(name))?,
                b"original tokenizer input"
            );
        }
        fs::write(&source, definition)?;
        let asset = source.parent().unwrap().join("tokenizer.json");
        OpenOptions::new()
            .write(true)
            .open(&asset)?
            .set_len(MAX_TOKENIZER + 1)?;
        assert!(files.retain_calibration(&source).is_err());
        assert_eq!(fs::metadata(asset)?.len(), MAX_TOKENIZER + 1);
        Ok(())
    }
}
