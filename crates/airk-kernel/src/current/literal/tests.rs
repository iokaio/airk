// SPDX-License-Identifier: Apache-2.0
//! Literal scans over small real retained publications and deterministic text windows.

use super::*;
use crate::scoped_reads::SelectionReadAuthority;
use airk_core::{
    identity::{Incarnation, Sha256Digest},
    index_catalog::{CatalogRevision, IndexId, PathScope, QueryProjectId, ScopeAtom, SourceRootId},
    scoped_reads::{ScopeReadRefused, SelectionOwnerKind, SelectionScopeRef, SourceDescriptor},
    service::ProjectRegistration,
};
use std::{fs, path::Path};
use tempfile::TempDir;

fn fixture(files: &[(&str, &[u8])]) -> Result<(TempDir, Kernel)> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().join("source");
    fs::create_dir(&root)?;
    for (path, bytes) in files {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, bytes)?;
    }
    let kernel = Kernel::open(&root, Some(directory.path().join("state")))?;
    kernel.configure_vectors(false)?;
    kernel.index()?;
    Ok((directory, kernel))
}

fn version(kernel: &Kernel) -> Result<String> {
    Ok(revision(kernel.current_reader()?.token()?).to_string())
}

fn limits() -> LiteralLimits {
    LiteralLimits {
        max_files: 20,
        max_bytes: 1 << 20,
        max_hits: 20,
    }
}

fn selected(kernel: &Kernel, root: &Path) -> Result<Kernel> {
    let project = QueryProjectId::from_identity(Incarnation::from_bytes([1; 16]));
    let source = SourceRootId::from_identity(Incarnation::from_bytes([2; 16]));
    kernel.with_selection_reads(SelectionReadAuthority {
        schema: 1,
        backing: ProjectRegistration::new(
            kernel.root_id(),
            kernel
                .data_root
                .to_str()
                .context("fixture data path")?
                .into(),
            kernel.collection().into(),
        )?
        .binding,
        index_id: IndexId::from_identity(Incarnation::from_bytes([3; 16])),
        project_ids: vec![project],
        selection: SelectionScopeRef {
            owner: SelectionOwnerKind::Window,
            grant_nonce: Incarnation::from_bytes([4; 16]),
            service_instance: Incarnation::from_bytes([5; 16]),
            scope_identity: Sha256Digest::from_bytes([6; 32]),
            catalog_revision: CatalogRevision::INITIAL,
        },
        atoms: vec![(
            project,
            ScopeAtom {
                source_root_id: source,
                kind: PathScope::Subtree,
                path: "public".into(),
            },
        )],
        sources: vec![SourceDescriptor {
            source_root_id: source,
            original_root: root.to_str().context("fixture source path")?.into(),
        }],
        membership_revisions: vec![(project, CatalogRevision::INITIAL)],
        source_revision: CatalogRevision::INITIAL,
        policy_epoch: 0,
        policy_digest: None,
    })
}

#[test]
fn exact_literals_use_retained_source_and_original_coordinates() -> Result<()> {
    let text = "π\nui.action-x ui.action-x\r\nUI.action-x\n10.0";
    let (_directory, kernel) = fixture(&[
        ("first/entry.txt", text.as_bytes()),
        ("second/entry.txt", b"ui.action-y"),
    ])?;
    let expected_revision = version(&kernel)?;
    fs::write(kernel.root().join("first/entry.txt"), "replaced live bytes")?;
    let scan = kernel.scan_retained_literal(
        "ui.action-x",
        "first/",
        &expected_revision,
        LiteralLimits {
            max_hits: 2,
            ..limits()
        },
        || Ok(true),
    )?;
    assert_eq!(scan.stop, LiteralStop::Complete);
    assert_eq!(scan.files_examined, 1);
    assert_eq!(scan.bytes_examined, text.len());
    assert_eq!(scan.files_excluded, 0);
    assert!(scan.retained_bytes_loaded >= text.len());
    let matches: Vec<_> = scan
        .hits
        .iter()
        .map(|hit| {
            (
                hit.path.as_str(),
                hit.start_line,
                hit.end_line,
                hit.start_byte,
                hit.end_byte,
            )
        })
        .collect();
    assert_eq!(
        matches,
        [
            ("first/entry.txt", 2, 2, 3, 14),
            ("first/entry.txt", 2, 2, 15, 26)
        ]
    );
    assert_eq!(scan.hits[0].content_hash.len(), 64);
    assert_eq!(scan.hits[0].content_hash, scan.hits[1].content_hash);
    let number =
        kernel
            .scan_retained_literal("10.0", "first/", &expected_revision, limits(), || Ok(true))?;
    assert_eq!(number.hits.len(), 1);
    assert_eq!(number.hits[0].start_line, 4);
    Ok(())
}

#[test]
fn complete_zero_differs_from_each_bounded_exit() -> Result<()> {
    let (_directory, kernel) = fixture(&[("a.txt", b"abba abba"), ("b.txt", b"other")])?;
    let expected_revision = version(&kernel)?;
    let run = |literal, limits| {
        kernel.scan_retained_literal(literal, "", &expected_revision, limits, || Ok(true))
    };
    let empty = run("absent", limits())?;
    assert_eq!(empty.stop, LiteralStop::Complete);
    assert_eq!(empty.files_examined, 2);
    assert!(empty.hits.is_empty());
    let file_limit = run(
        "absent",
        LiteralLimits {
            max_files: 1,
            ..limits()
        },
    )?;
    assert_eq!(file_limit.stop, LiteralStop::FileLimit);
    assert_eq!(file_limit.files_examined, 1);
    let byte_limit = run(
        "abba",
        LiteralLimits {
            max_bytes: 1,
            ..limits()
        },
    )?;
    assert_eq!(byte_limit.stop, LiteralStop::ByteLimit);
    assert_eq!(byte_limit.retained_bytes_loaded, 0);
    let hit_limit = run(
        "abba",
        LiteralLimits {
            max_hits: 1,
            ..limits()
        },
    )?;
    assert_eq!(hit_limit.stop, LiteralStop::HitLimit);
    assert_eq!(hit_limit.hits.len(), 1);
    assert_eq!(hit_limit.hits[0].start_byte, 0);
    let cancelled =
        kernel.scan_retained_literal("abba", "", &expected_revision, limits(), || Ok(false))?;
    assert_eq!(cancelled.stop, LiteralStop::Interrupted);
    assert_eq!(cancelled.files_examined, 0);
    let stale =
        kernel.scan_retained_literal("abba", "", "wrong-revision", limits(), || Ok(true))?;
    assert_eq!(stale.stop, LiteralStop::Stale);
    assert!(stale.hits.is_empty());
    Ok(())
}

#[test]
fn selection_scope_refuses_disallowed_prefixes_and_revocation() -> Result<()> {
    let (_directory, kernel) = fixture(&[
        ("public/entry.txt", b"same needle here"),
        ("private/entry.txt", b"same needle here"),
    ])?;
    let (_other_directory, other) = fixture(&[("public/entry.txt", b"other needle")])?;
    let scoped = selected(&kernel, kernel.root())?;
    let expected_revision = version(&scoped)?;
    let scan =
        scoped.scan_retained_literal("needle", "", &expected_revision, limits(), || Ok(true))?;
    assert_eq!(scan.stop, LiteralStop::Complete);
    assert_eq!(scan.files_examined, 1);
    assert_eq!(scan.hits.len(), 1);
    assert_eq!(scan.hits[0].path, "public/entry.txt");
    let error = scoped
        .scan_retained_literal("needle", "private/", &expected_revision, limits(), || {
            Ok(true)
        })
        .unwrap_err();
    assert!(error.is::<ScopeReadRefused>());
    let foreign =
        other.scan_retained_literal("needle", "", &expected_revision, limits(), || Ok(true))?;
    assert_eq!(foreign.stop, LiteralStop::Stale);
    assert!(foreign.hits.is_empty());
    let error = scoped
        .scan_retained_literal("needle", "", &expected_revision, limits(), || {
            scoped.revoke_scoped_reads();
            Ok(true)
        })
        .unwrap_err();
    assert!(error.is::<ScopeReadRefused>());
    Ok(())
}

#[test]
fn window_overlap_preserves_utf8_crlf_and_nonoverlapping_matches() -> Result<()> {
    let prefix = "x".repeat(16_383);
    let text = format!("{prefix}π\r\nπ\r\n");
    let mut matches = Vec::new();
    let (examined, stop) = scan_text(&text, "π\r\n", &mut || Ok(true), |position| {
        matches.push(position);
        true
    })?;
    assert_eq!((examined, stop), (text.len(), LiteralStop::Complete));
    assert_eq!(
        matches,
        [
            MatchPosition {
                start_line: 1,
                end_line: 1,
                start_byte: 16_383,
                end_byte: 16_387
            },
            MatchPosition {
                start_line: 2,
                end_line: 2,
                start_byte: 16_387,
                end_byte: 16_391
            },
        ]
    );
    let mut matches = Vec::new();
    scan_text("aaaaa", "aa", &mut || Ok(true), |position| {
        matches.push(position.start_byte);
        true
    })?;
    assert_eq!(matches, [0, 2]);
    let mut checkpoints = 0;
    let (examined, stop) = scan_text(
        &"x".repeat(40_000),
        "missing",
        &mut || {
            checkpoints += 1;
            Ok(checkpoints == 1)
        },
        |_| panic!("should never find a match"),
    )?;
    assert_eq!((examined, stop), (16_390, LiteralStop::Interrupted));
    Ok(())
}

/// One uncompressed OOXML member, word/document.xml, with a single paragraph.
const DOCUMENT: &[u8] = b"\x50\x4b\x03\x04\x14\x00\x00\x00\x00\x00\x00\x00\x00\x00\xad\x4d\x9e\x82\x98\x00\x00\x00\x98\x00\x00\x00\x11\x00\x00\x00word/document.xml\
    <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p><w:r><w:t>needle</w:t></w:r></w:p></w:body></w:document>\
    \x50\x4b\x01\x02\x14\x00\x14\x00\x00\x00\x00\x00\x00\x00\x00\x00\xad\x4d\x9e\x82\x98\x00\x00\x00\
    \x98\x00\x00\x00\x11\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00word/document.xml\
    \x50\x4b\x05\x06\x00\x00\x00\x00\x01\x00\x01\x00\x3f\x00\x00\x00\xc7\x00\x00\x00\x00\x00";

#[test]
fn mapped_document_exclusion_is_never_a_complete_zero() -> Result<()> {
    let (_directory, kernel) = fixture(&[("a.txt", b"other"), ("mapped.docx", DOCUMENT)])?;
    let expected_revision = version(&kernel)?;
    let scan =
        kernel.scan_retained_literal("needle", "", &expected_revision, limits(), || Ok(true))?;
    assert_eq!(scan.stop, LiteralStop::UnsupportedRetainedText);
    assert_eq!((scan.files_examined, scan.files_excluded), (1, 1));
    assert!(scan.hits.is_empty());
    Ok(())
}
