// SPDX-License-Identifier: Apache-2.0
use super::*;
use crate::Kernel;
#[cfg(unix)]
use airk_core::{SearchQuery, identity::Sha256Digest};

#[cfg(unix)]
fn query() -> SearchQuery {
    SearchQuery {
        text: "copper".into(),
        path_prefix: None,
        limit: 5,
        max_bytes: 4096,
    }
}

#[cfg(unix)]
#[test]
fn denied_source_and_denied_ancestor_keep_owned_evidence_without_publishing_deletions() -> Result<()>
{
    use std::os::unix::fs::PermissionsExt;
    struct Restore(PathBuf, std::fs::Permissions);
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.0, self.1.clone());
        }
    }
    let owner = tempfile::tempdir()?;
    let parent = owner.path().join("source-parent");
    let source = parent.join("source");
    let data = owner.path().join("data");
    std::fs::create_dir_all(&source)?;
    std::fs::create_dir(&data)?;
    let original = "Original copper source remains unchanged.\n";
    std::fs::write(source.join("memo.txt"), original)?;
    let project = registration(&source, &data, "fixture")?;
    let kernel = Kernel::open_service_collection(&source, Some(data.clone()), "fixture")?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let token = kernel.status()?.publication.unwrap().token;
    for target in [&source, &parent] {
        let restore = Restore(target.clone(), std::fs::metadata(target)?.permissions());
        std::fs::set_permissions(target, std::fs::Permissions::from_mode(0o0))?;
        assert!(
            std::fs::read_dir(&source).is_err(),
            "permission fixture requires a non-root user"
        );
        validate_retained(&project)?;
        let retained = Kernel::open_service_collection_for_diagnostics(
            Path::new(&project.root),
            Some(data.clone()),
            "fixture",
        )?;
        assert!(!retained.search(&query())?.hits.is_empty());
        let status = retained.status()?;
        assert_eq!(status.publication.unwrap().token, token);
        let availability = status.source_availability.unwrap();
        assert!(matches!(availability.state, SourceState::Unavailable));
        assert_eq!(availability.reason, Some(SourceProblem::PermissionDenied));
        assert!(retained.index().unwrap_err().is::<SourceUnavailable>());
        assert!(
            retained
                .rebuild_service(Sha256Digest::from_bytes([91; 32]))
                .unwrap_err()
                .is::<SourceUnavailable>()
        );
        drop(restore);
        assert_eq!(std::fs::read_to_string(source.join("memo.txt"))?, original);
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn redirected_registered_root_reads_only_its_prior_evidence_and_preserves_both_sources()
-> Result<()> {
    struct Restore {
        source: PathBuf,
        saved: PathBuf,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            if std::fs::symlink_metadata(&self.source).is_ok_and(|m| m.file_type().is_symlink()) {
                let _ = std::fs::remove_file(&self.source);
            }
            if !self.source.exists() {
                let _ = std::fs::rename(&self.saved, &self.source);
            }
        }
    }
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    let saved = owner.path().join("original-source");
    let foreign = owner.path().join("foreign-source");
    let data = owner.path().join("data");
    for path in [&source, &foreign, &data] {
        std::fs::create_dir(path)?;
    }
    std::fs::write(
        source.join("memo.txt"),
        "Original copper retained source.\n",
    )?;
    std::fs::write(
        foreign.join("secret.txt"),
        "Foreign cobalt source must remain unread by indexing.\n",
    )?;
    let project = registration(&source, &data, "fixture")?;
    let kernel = Kernel::open_service_collection(&source, Some(data.clone()), "fixture")?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let token = kernel.status()?.publication.unwrap().token;
    std::fs::rename(&source, &saved)?;
    let restore = Restore {
        source: source.clone(),
        saved: saved.clone(),
    };
    std::os::unix::fs::symlink(&foreign, &source)?;
    assert!(validate(&project).is_err());
    validate_retained(&project)?;
    let retained = Kernel::open_service_collection_for_diagnostics(
        Path::new(&project.root),
        Some(data.clone()),
        "fixture",
    )?;
    assert!(!retained.search(&query())?.hits.is_empty());
    let status = retained.status()?;
    assert_eq!(status.publication.unwrap().token, token);
    assert_eq!(
        status.source_availability.unwrap().reason,
        Some(SourceProblem::Redirected)
    );
    assert!(retained.index().unwrap_err().is::<SourceUnavailable>());
    assert!(
        retained
            .rebuild_service(Sha256Digest::from_bytes([92; 32]))
            .unwrap_err()
            .is::<SourceUnavailable>()
    );
    assert_eq!(
        std::fs::read_to_string(saved.join("memo.txt"))?,
        "Original copper retained source.\n"
    );
    assert_eq!(
        std::fs::read_to_string(foreign.join("secret.txt"))?,
        "Foreign cobalt source must remain unread by indexing.\n"
    );
    drop(restore);
    retained.index()?;
    Ok(())
}

#[test]
fn unavailable_roots_never_initialize_an_unbound_collection_or_accept_path_traversal() -> Result<()>
{
    let owner = tempfile::tempdir()?;
    let data = owner.path().join("data");
    std::fs::create_dir(&data)?;
    let source = owner.path().join("source-is-now-a-file");
    std::fs::write(&source, b"Original user file.")?;
    assert!(
        Kernel::open_service_collection_for_diagnostics(&source, Some(data.clone()), "fixture")
            .is_err()
    );
    assert_eq!(std::fs::read_dir(&data)?.count(), 0);
    assert!(retained_root(&source.join("../missing")).is_err());
    assert_eq!(std::fs::read(source)?, b"Original user file.");
    Ok(())
}

#[test]
fn a_registered_directory_replaced_by_a_file_keeps_retained_evidence_and_the_replacement_bytes()
-> Result<()> {
    struct Restore {
        source: PathBuf,
        saved: PathBuf,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            if std::fs::symlink_metadata(&self.source)
                .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
            {
                let _ = std::fs::remove_file(&self.source);
            }
            if !self.source.exists() {
                let _ = std::fs::rename(&self.saved, &self.source);
            }
        }
    }
    let owner = tempfile::tempdir()?;
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    std::fs::create_dir(&source)?;
    std::fs::create_dir(&data)?;
    std::fs::write(
        source.join("memo.txt"),
        "Original copper retained fixture.\n",
    )?;
    let project = registration(&source, &data, "fixture")?;
    let kernel = Kernel::open_service_collection(&source, Some(data.clone()), "fixture")?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    let token = kernel.status()?.publication.unwrap().token;
    let saved = owner.path().join("saved-source");
    std::fs::rename(&source, &saved)?;
    let restore = Restore {
        source: source.clone(),
        saved,
    };
    std::fs::write(&source, b"Preserve this replacement user file.")?;
    validate_retained(&project)?;
    let retained = Kernel::open_service_collection_for_diagnostics(
        Path::new(&project.root),
        Some(data),
        "fixture",
    )?;
    let result = retained.search(&airk_core::SearchQuery {
        text: "copper".into(),
        path_prefix: None,
        limit: 5,
        max_bytes: 4096,
    })?;
    assert!(!result.hits.is_empty());
    let status = retained.status()?;
    assert_eq!(status.publication.unwrap().token, token);
    assert_eq!(
        status.source_availability.unwrap().reason,
        Some(SourceProblem::NotDirectory)
    );
    assert!(retained.index().unwrap_err().is::<SourceUnavailable>());
    assert_eq!(
        std::fs::read(&source)?,
        b"Preserve this replacement user file."
    );
    drop(restore);
    assert_eq!(
        std::fs::read_to_string(source.join("memo.txt"))?,
        "Original copper retained fixture.\n"
    );
    Ok(())
}
