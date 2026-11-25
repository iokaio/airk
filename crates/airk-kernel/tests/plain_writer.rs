// SPDX-License-Identifier: Apache-2.0
//! End-to-end writes of the base pass into the plain index (`airk-index/1`),
//! observed through the engine's own `IndexReader`, never the old store.
//! Expectations are computed from the fixture bytes, not from the writer.
use airk_core::{Citation, publication::InventoryFile};
use airk_kernel::Kernel;
use airk_store::{
    current::{CapturedInput, collection::CollectionState},
    index::{
        Handle, HandleKind, IndexReader, LexicalQuery, PathLookup, Selector, SelectorKind,
        VectorState,
    },
};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// The collection directory that holds the plain index.
fn index_directory(state: &Path) -> Result<PathBuf> {
    let mut stack = vec![state.to_path_buf()];
    while let Some(directory) = stack.pop() {
        if directory.join("index.sqlite3").is_file() {
            return Ok(directory);
        }
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                stack.push(entry.path());
            }
        }
    }
    anyhow::bail!("no plain index under the state directory")
}

fn reader(state: &Path) -> Result<IndexReader> {
    IndexReader::open(&index_directory(state)?)
}

fn record(reader: &IndexReader, path: &str) -> Result<CapturedInput> {
    let row = reader.file(path)?.context("file row missing")?;
    Ok(serde_json::from_slice(&row.record)?)
}

fn inventory(reader: &IndexReader, path: &str) -> Result<InventoryFile> {
    let row = reader.file(path)?.context("file row missing")?;
    Ok(serde_json::from_slice(&row.metadata)?)
}

fn state(reader: &IndexReader) -> Result<CollectionState> {
    CollectionState::decode_canonical(
        &reader
            .meta("collection_state")?
            .context("collection state missing")?,
    )
}

fn token(term: &str) -> String {
    format!("x{term}")
}

fn lexical(reader: &IndexReader, words: &[&str]) -> Result<Vec<String>> {
    let terms = words
        .iter()
        .flat_map(|word| airk_core::lexical::terms(word))
        .map(|term| token(&term))
        .collect();
    let (hits, _) = reader.lexical(&LexicalQuery {
        terms,
        path_prefix: None,
        paths: None,
        limit: 100,
        ranked: true,
    })?;
    Ok(hits.into_iter().map(|hit| hit.path).collect())
}

const ALPHA: &str = "pub fn alpha_function_needle() -> u32 {\n    let shared_value = 41;\n    shared_value + 1\n}\n";
const NOTES: &str =
    "# Notes\n\nThe guide mentions pinecone pinecone pinecone and other shared words.\n";
const PLAIN: &str =
    "A plain text file that mentions pinecone once among many other unrelated words here.\n";
const MOVED: &str = "Quarterly orchard inventory lists apples pears plums cherries apricots and quinces for the cooperative ledger.\n";

fn fixture() -> Result<(tempfile::TempDir, tempfile::TempDir, Kernel)> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    fs::create_dir_all(root.path().join("src"))?;
    fs::create_dir_all(root.path().join("docs"))?;
    fs::write(root.path().join("src/alpha.rs"), ALPHA)?;
    fs::write(root.path().join("docs/notes.md"), NOTES)?;
    fs::write(root.path().join("plain.txt"), PLAIN)?;
    fs::write(root.path().join("docs/orchard.txt"), MOVED)?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    kernel.configure_vectors(false)?;
    Ok((root, state, kernel))
}

#[test]
fn cold_index_writes_records_payloads_chunks_lexical_selectors_and_state() -> Result<()> {
    let (_root, state_dir, kernel) = fixture()?;
    kernel.index()?;
    let reader = reader(state_dir.path())?;
    let paths = reader.lookup(&PathLookup::Files, "", None, 100)?.paths;
    assert_eq!(
        paths,
        [
            "docs/notes.md",
            "docs/orchard.txt",
            "plain.txt",
            "src/alpha.rs"
        ]
    );
    for (path, source) in [
        ("src/alpha.rs", ALPHA),
        ("docs/notes.md", NOTES),
        ("plain.txt", PLAIN),
        ("docs/orchard.txt", MOVED),
    ] {
        let row = reader.file(path)?.context("row")?;
        let input = record(&reader, path)?;
        assert_eq!(input.artifact.path, path);
        assert_eq!(input.artifact.content_hash, sha256_hex(source.as_bytes()));
        assert_eq!(input.artifact.bytes, source.len());
        assert!(input.artifact.chunks.is_empty(), "records are compact");
        assert_eq!(row.source_hash.to_string(), sha256_hex(source.as_bytes()));
        assert_eq!(row.language, input.artifact.language);
        let retained = input.artifact.retained.as_ref().context("retained")?;
        assert_eq!(row.payload.to_string(), retained.object);
        assert_eq!(row.vector_state, VectorState::NotSelected);
        // The payload is the path-free extraction; its digest names its bytes.
        let payload = reader.payload(&row.payload)?;
        assert_eq!(sha256_hex(&payload), retained.object);
        assert_eq!(payload.len(), retained.object_bytes);
        let payload: airk_core::extraction::ExtractionPayload = serde_json::from_slice(&payload)?;
        assert_eq!(payload.extraction_method, "utf8");
        assert_eq!(payload.text, source, "a UTF-8 source is the payload text");
        let file = inventory(&reader, path)?;
        assert_eq!(file.state.path, path);
        assert_eq!(
            file.state.source_hash.to_string(),
            sha256_hex(source.as_bytes())
        );
        assert_eq!(file.state.payload_digest, row.payload);
        assert_eq!(file.source_bytes, source.len());
        assert_eq!(
            file.indexed_lines,
            source.lines().count(),
            "indexed lines of {path}"
        );
        assert_eq!(
            reader.handle(&Handle {
                kind: HandleKind::Artifact,
                value: file.state.artifact_id.as_bytes().to_vec(),
            })?,
            Some(path.to_owned())
        );
        // One stored chunk per extraction chunk, each citing this file.
        let mut ordinal = 0u32;
        while let Some((_, citation)) = reader.chunk(path, ordinal)? {
            let citation: Citation = serde_json::from_slice(&citation)?;
            assert_eq!(citation.path, path);
            assert_eq!(citation.content_hash, sha256_hex(source.as_bytes()));
            assert!(citation.start_line >= 1 && citation.end_line >= citation.start_line);
            ordinal += 1;
        }
        assert_eq!(ordinal as usize, retained.chunks, "chunk rows of {path}");
        assert!(ordinal >= 1);
        let measurement = reader
            .resource(&format!("source-measurement/{path}"))?
            .context("source measurement")?;
        serde_json::from_slice::<airk_core::usage::SourceContextSize>(&measurement)?;
    }
    // Lexical: path terms and text terms; BM25 ranks the denser file first.
    assert_eq!(
        lexical(&reader, &["alpha_function_needle"])?,
        ["src/alpha.rs"]
    );
    assert_eq!(lexical(&reader, &["orchard"])?, ["docs/orchard.txt"]);
    assert_eq!(
        lexical(&reader, &["pinecone"])?,
        ["docs/notes.md", "plain.txt"]
    );
    assert!(lexical(&reader, &["absentword"])?.is_empty());
    // A Rust function is a named fact.
    let named = reader.lookup(
        &PathLookup::Selector(Selector {
            kind: SelectorKind::FactName,
            value: "alpha_function_needle".into(),
        }),
        "",
        None,
        10,
    )?;
    assert_eq!(named.paths, ["src/alpha.rs"]);
    assert_eq!(
        reader
            .lookup(&PathLookup::Language("rust".into()), "", None, 10)?
            .paths,
        ["src/alpha.rs"]
    );
    let state = state(&reader)?;
    assert_eq!(state.counts.files, 4);
    let source_bytes = [ALPHA, NOTES, PLAIN, MOVED]
        .iter()
        .map(|s| s.len() as u64)
        .sum::<u64>();
    assert_eq!(state.counts.source_bytes, source_bytes);
    assert!(state.configuration.model.is_none());
    let scope: serde_json::Value =
        serde_json::from_slice(&reader.meta("scope")?.context("scope")?)?;
    assert_eq!(scope["collection"], "default");
    assert_eq!(
        reader
            .meta("lexical_commit")?
            .context("lexical commit")?
            .len(),
        32
    );
    let scan: serde_json::Value =
        serde_json::from_slice(&reader.resource("scan-work")?.context("scan work")?)?;
    assert_eq!(scan["total_artifacts"], 4);
    assert!(reader.resource("embedding-cache")?.is_none());
    // No per-file object files and no old store.
    let directory = index_directory(state_dir.path())?;
    for old in [
        "objects",
        "current-metadata",
        "current.sqlite3",
        "sources",
        "artifacts",
    ] {
        assert!(!directory.join(old).exists(), "{old} written");
    }
    Ok(())
}

#[test]
fn rescans_publish_only_what_changed_and_keep_identities() -> Result<()> {
    let (root, state_dir, kernel) = fixture()?;
    kernel.index()?;
    let first = reader(state_dir.path())?;
    let sequence = first.sequence();
    let alpha_id = inventory(&first, "src/alpha.rs")?.state.artifact_id;
    let orchard_id = inventory(&first, "docs/orchard.txt")?.state.artifact_id;
    let notes_changed = first
        .file("docs/notes.md")?
        .context("notes")?
        .changed_sequence;
    drop(first);

    kernel.index()?;
    let unchanged = reader(state_dir.path())?;
    assert_eq!(
        unchanged.sequence(),
        sequence,
        "nothing changed, nothing published"
    );
    drop(unchanged);

    let edited = ALPHA.replace("41", "42");
    fs::write(root.path().join("src/alpha.rs"), &edited)?;
    kernel.index()?;
    let after_edit = reader(state_dir.path())?;
    assert!(after_edit.sequence() > sequence);
    let alpha = after_edit.file("src/alpha.rs")?.context("alpha")?;
    assert_eq!(alpha.source_hash.to_string(), sha256_hex(edited.as_bytes()));
    assert_eq!(alpha.changed_sequence, after_edit.sequence());
    assert_eq!(
        inventory(&after_edit, "src/alpha.rs")?.state.artifact_id,
        alpha_id,
        "an edit keeps the artifact id"
    );
    assert_eq!(
        after_edit
            .file("docs/notes.md")?
            .context("notes")?
            .changed_sequence,
        notes_changed,
        "only the edited file was published"
    );
    drop(after_edit);

    fs::rename(
        root.path().join("docs/orchard.txt"),
        root.path().join("orchard-moved.txt"),
    )?;
    kernel.index()?;
    let after_move = reader(state_dir.path())?;
    assert!(after_move.file("docs/orchard.txt")?.is_none());
    assert_eq!(
        inventory(&after_move, "orchard-moved.txt")?
            .state
            .artifact_id,
        orchard_id,
        "a move keeps the artifact id"
    );
    assert_eq!(
        after_move.handle(&Handle {
            kind: HandleKind::Artifact,
            value: orchard_id.as_bytes().to_vec(),
        })?,
        Some("orchard-moved.txt".to_owned())
    );
    drop(after_move);

    let removed = reader(state_dir.path())?
        .file("plain.txt")?
        .context("plain")?
        .payload;
    fs::remove_file(root.path().join("plain.txt"))?;
    kernel.index()?;
    let after_delete = reader(state_dir.path())?;
    assert!(after_delete.file("plain.txt")?.is_none());
    assert!(after_delete.payload(&removed).is_err(), "payload reclaimed");
    assert!(
        after_delete
            .resource("source-measurement/plain.txt")?
            .is_none()
    );
    assert_eq!(lexical(&after_delete, &["pinecone"])?, ["docs/notes.md"]);
    assert_eq!(state(&after_delete)?.counts.files, 3);
    Ok(())
}

#[test]
fn full_rebuild_replaces_every_file_and_keeps_identities() -> Result<()> {
    let (_root, state_dir, kernel) = fixture()?;
    kernel.index()?;
    let before = reader(state_dir.path())?;
    let id = inventory(&before, "src/alpha.rs")?.state.artifact_id;
    let sequence = before.sequence();
    drop(before);
    kernel.reindex()?;
    let after = reader(state_dir.path())?;
    assert!(after.sequence() > sequence);
    assert_eq!(after.file_count()?, 4);
    assert_eq!(inventory(&after, "src/alpha.rs")?.state.artifact_id, id);
    for path in [
        "src/alpha.rs",
        "docs/notes.md",
        "plain.txt",
        "docs/orchard.txt",
    ] {
        assert!(after.file(path)?.context("row")?.changed_sequence > sequence);
    }
    assert_eq!(state(&after)?.counts.files, 4);
    Ok(())
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A minimal DOCX: one stored (uncompressed) ZIP entry, `word/document.xml`.
fn docx(text: &str) -> Vec<u8> {
    let name = b"word/document.xml";
    let body = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body></w:document>"
    );
    let data = body.as_bytes();
    let crc = crc32(data);
    let size = data.len() as u32;
    let mut out = Vec::new();
    let header = |out: &mut Vec<u8>, signature: u32, central: bool| {
        out.extend_from_slice(&signature.to_le_bytes());
        if central {
            out.extend_from_slice(&20u16.to_le_bytes());
        }
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&0u16.to_le_bytes()); // stored
        out.extend_from_slice(&0u16.to_le_bytes()); // time
        out.extend_from_slice(&0x21u16.to_le_bytes()); // date
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra
    };
    header(&mut out, 0x0403_4b50, false);
    out.extend_from_slice(name);
    out.extend_from_slice(data);
    let central = out.len() as u32;
    header(&mut out, 0x0201_4b50, true);
    out.extend_from_slice(&0u16.to_le_bytes()); // comment
    out.extend_from_slice(&0u16.to_le_bytes()); // disk
    out.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
    out.extend_from_slice(&0u32.to_le_bytes()); // external attributes
    out.extend_from_slice(&0u32.to_le_bytes()); // local header offset
    out.extend_from_slice(name);
    let central_size = out.len() as u32 - central;
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&central_size.to_le_bytes());
    out.extend_from_slice(&central.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}
#[test]
fn a_document_stores_its_extraction_and_no_source() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state_dir = tempfile::tempdir()?;
    let bytes = docx("Hello DocxNeedle document");
    fs::write(root.path().join("guide.docx"), &bytes)?;
    let kernel = Kernel::open(root.path(), Some(state_dir.path().into()))?;
    kernel.configure_vectors(false)?;
    let report = kernel.index()?;
    let reader = reader(state_dir.path())?;
    let row = reader.file("guide.docx")?.with_context(|| {
        format!(
            "docx row missing; skipped: {}",
            serde_json::to_string(&report.skipped).unwrap_or_default()
        )
    })?;
    let input = record(&reader, "guide.docx")?;
    assert_eq!(input.artifact.content_hash, sha256_hex(&bytes));
    assert_ne!(input.artifact.extraction_method, "utf8");
    let payload = reader.payload(&row.payload)?;
    assert!(
        !payload
            .windows(bytes.len())
            .any(|window| window == bytes.as_slice()),
        "the original document bytes are not stored"
    );
    let payload: airk_core::extraction::ExtractionPayload = serde_json::from_slice(&payload)?;
    assert!(payload.text.contains("DocxNeedle"));
    assert_eq!(lexical(&reader, &["DocxNeedle"])?, ["guide.docx"]);
    let directory = index_directory(state_dir.path())?;
    let mut stored = Vec::new();
    for name in ["index.sqlite3", "index.sqlite3-wal"] {
        if let Ok(bytes) = fs::read(directory.join(name)) {
            stored.extend(bytes);
        }
    }
    assert!(
        !stored
            .windows(bytes.len())
            .any(|window| window == bytes.as_slice()),
        "the index database holds no copy of the document"
    );
    Ok(())
}

/// One cold index of a private source copy, timed; runs only when
/// `AIRK_PLAIN_TIMING_ROOT` and `AIRK_PLAIN_TIMING_STATE` name the folders.
#[test]
fn timed_cold_index_of_a_private_tree() -> Result<()> {
    let (Some(root), Some(state)) = (
        std::env::var_os("AIRK_PLAIN_TIMING_ROOT"),
        std::env::var_os("AIRK_PLAIN_TIMING_STATE"),
    ) else {
        return Ok(());
    };
    let kernel = Kernel::open(Path::new(&root), Some(PathBuf::from(&state)))?;
    kernel.configure_vectors(false)?;
    let started = std::time::Instant::now();
    let report = kernel.index()?;
    let elapsed = started.elapsed();
    let reader = reader(Path::new(&state))?;
    println!(
        "plain cold index: {:.1} s, {} files indexed, {} stored, {} skipped",
        elapsed.as_secs_f64(),
        report.indexed,
        reader.file_count()?,
        report.skipped.len()
    );
    Ok(())
}

#[test]
fn an_old_format_collection_is_discarded_and_rebuilt() -> Result<()> {
    let (_root, state_dir, kernel) = fixture()?;
    kernel.index()?;
    let directory = index_directory(state_dir.path())?;
    drop(kernel);
    for name in ["index.sqlite3", "index.sqlite3-wal", "index.sqlite3-shm"] {
        let _ = fs::remove_file(directory.join(name));
    }
    fs::write(directory.join("current.sqlite3"), b"old current store")?;
    fs::create_dir_all(directory.join("objects/artifacts"))?;
    fs::write(directory.join("objects/artifacts/old.json"), b"{}")?;
    let kernel = Kernel::open(_root.path(), Some(state_dir.path().into()))?;
    kernel.index()?;
    assert!(!directory.join("current.sqlite3").exists());
    assert!(!directory.join("objects").exists());
    let reader = reader(state_dir.path())?;
    assert_eq!(reader.file_count()?, 4);
    Ok(())
}
