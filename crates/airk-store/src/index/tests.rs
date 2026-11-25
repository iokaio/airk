// SPDX-License-Identifier: Apache-2.0
use super::*;
use anyhow::Result;
use sha2::{Digest, Sha256};

fn hash(text: &str) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(text.as_bytes()).into())
}

fn entry(path: &str, body: &str, language: &str, words: &[&str]) -> Result<FileEntry> {
    Ok(FileEntry {
        path: path.into(),
        source_hash: hash(body),
        language: language.into(),
        record: format!("record:{path}").into_bytes(),
        metadata: format!("metadata:{path}").into_bytes(),
        stat: Some(FileStat {
            size: body.len() as u64,
            modified_ns: 7,
            file_id: vec![1, 2],
        }),
        payload: EncodedPayload::encode(body.as_bytes())?,
        chunks: words
            .iter()
            .enumerate()
            .map(|(ordinal, terms)| ChunkEntry {
                stable_id: hash(&format!("{path}#{ordinal}")),
                citation: format!("{path}:{ordinal}").into_bytes(),
                terms: (*terms).into(),
            })
            .collect(),
        selectors: vec![Selector {
            kind: SelectorKind::FactName,
            value: format!("name-of-{language}"),
        }],
        handles: vec![Handle {
            kind: HandleKind::Artifact,
            value: path.as_bytes().to_vec(),
        }],
        vector_pending: true,
    })
}

fn upsert(entry: FileEntry) -> FileChange {
    FileChange::Upsert(Box::new(entry))
}

fn terms(words: &[&str]) -> Vec<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

fn query(words: &[&str], ranked: bool) -> LexicalQuery {
    LexicalQuery {
        terms: terms(words),
        path_prefix: None,
        paths: None,
        limit: 10,
        ranked,
    }
}

fn two_files(directory: &std::path::Path) -> Result<IndexWriter> {
    let mut writer = IndexWriter::create(directory)?;
    writer.apply(Batch {
        files: vec![
            upsert(entry(
                "src/a.rs",
                "fn alpha() {}",
                "rust",
                &["xalpha xshared", "xbeta"],
            )?),
            upsert(entry(
                "src/b.py",
                "def gamma(): pass",
                "python",
                &["xgamma xshared xshared"],
            )?),
        ],
        change: Some(b"first".to_vec()),
        ..Default::default()
    })?;
    Ok(writer)
}

#[test]
fn published_files_are_read_back_exactly() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let writer = two_files(directory.path())?;
    assert_eq!(writer.sequence()?, 1);
    let reader = IndexReader::open(directory.path())?;
    assert_eq!((reader.sequence(), reader.file_count()?), (1, 2));
    let file = reader.file("src/a.rs")?.expect("file stored");
    assert_eq!(file.record, b"record:src/a.rs");
    assert_eq!(file.metadata, b"metadata:src/a.rs");
    assert_eq!(file.source_hash, hash("fn alpha() {}"));
    assert_eq!(file.vector_state, VectorState::Pending);
    assert_eq!(file.stat.as_ref().map(|stat| stat.size), Some(13));
    assert_eq!(reader.payload(&file.payload)?, b"fn alpha() {}");
    assert!(reader.file("src/missing.rs")?.is_none());
    assert_eq!(
        reader.handle(&Handle {
            kind: HandleKind::Artifact,
            value: b"src/b.py".to_vec()
        })?,
        Some("src/b.py".into())
    );
    assert_eq!(
        reader.chunk("src/a.rs", 1)?.map(|(_, citation)| citation),
        Some(b"src/a.rs:1".to_vec())
    );
    assert_eq!(reader.changes_since(0, 10)?, vec![(1, b"first".to_vec())]);
    Ok(())
}

#[test]
fn lexical_hits_contain_every_term_and_rank_by_relevance() -> Result<()> {
    let directory = tempfile::tempdir()?;
    two_files(directory.path())?;
    let reader = IndexReader::open(directory.path())?;
    let (hits, more) = reader.lexical(&query(&["xshared"], true))?;
    assert!(!more);
    assert_eq!(
        hits.iter().map(|hit| hit.path.as_str()).collect::<Vec<_>>(),
        ["src/b.py", "src/a.rs"],
        "the chunk with the term twice ranks first"
    );
    assert!(hits[0].rank < hits[1].rank, "lower BM25 values are better");
    let (both, _) = reader.lexical(&query(&["xalpha", "xshared"], true))?;
    assert_eq!(both.len(), 1);
    assert_eq!((both[0].path.as_str(), both[0].ordinal), ("src/a.rs", 0));
    let (none, _) = reader.lexical(&query(&["xalpha", "xgamma"], true))?;
    assert!(none.is_empty(), "terms from different chunks never combine");
    let (unranked, _) = reader.lexical(&query(&["xshared"], false))?;
    let mut ids = unranked.iter().map(|hit| hit.stable_id).collect::<Vec<_>>();
    let sorted = {
        let mut copy = ids.clone();
        copy.sort();
        copy
    };
    assert_eq!(ids, sorted, "unranked results follow stable identity");
    ids.clear();
    let mut filtered = query(&["xshared"], true);
    filtered.path_prefix = Some("src/a".into());
    assert_eq!(reader.lexical(&filtered)?.0.len(), 1);
    filtered.path_prefix = None;
    filtered.paths = Some(vec!["src/b.py".into()]);
    assert_eq!(reader.lexical(&filtered)?.0[0].path, "src/b.py");
    let mut limited = query(&["xshared"], true);
    limited.limit = 1;
    let (one, more) = reader.lexical(&limited)?;
    assert_eq!((one.len(), more), (1, true));
    Ok(())
}

#[test]
fn path_lookups_page_in_byte_order() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut writer = IndexWriter::create(directory.path())?;
    writer.apply(Batch {
        files: ["doc", "doc/a.md", "doc/b.md", "docs.md", "src/x.rs"]
            .iter()
            .map(|path| {
                Ok(upsert(entry(
                    path,
                    path,
                    if path.ends_with(".rs") {
                        "rust"
                    } else {
                        "markdown"
                    },
                    &[],
                )?))
            })
            .collect::<Result<_>>()?,
        ..Default::default()
    })?;
    let reader = IndexReader::open(directory.path())?;
    let first = reader.lookup(&PathLookup::Files, "doc", None, 2)?;
    assert_eq!(
        first.paths,
        ["doc", "doc/a.md"],
        "a path equal to the prefix is listed"
    );
    let rest = reader.lookup(&PathLookup::Files, "doc", first.next_after.as_deref(), 10)?;
    assert_eq!(
        (rest.paths, rest.next_after),
        (vec!["doc/b.md".into(), "docs.md".into()], None)
    );
    let rust = reader.lookup(&PathLookup::Language("rust".into()), "", None, 10)?;
    assert_eq!(rust.paths, ["src/x.rs"]);
    let named = reader.lookup(
        &PathLookup::Selector(Selector {
            kind: SelectorKind::FactName,
            value: "name-of-markdown".into(),
        }),
        "",
        None,
        10,
    )?;
    assert_eq!(named.paths.len(), 4);
    Ok(())
}

#[test]
fn a_reader_keeps_its_snapshot_while_later_batches_commit() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut writer = two_files(directory.path())?;
    let before = IndexReader::open(directory.path())?;
    writer.apply(Batch {
        files: vec![FileChange::Delete("src/a.rs".into())],
        ..Default::default()
    })?;
    assert_eq!(before.sequence(), 1);
    assert!(before.file("src/a.rs")?.is_some());
    assert_eq!(before.lexical(&query(&["xalpha"], true))?.0.len(), 1);
    let after = IndexReader::open(directory.path())?;
    assert_eq!(after.sequence(), 2);
    assert!(after.file("src/a.rs")?.is_none());
    assert!(after.lexical(&query(&["xalpha"], true))?.0.is_empty());
    assert!(
        after
            .handle(&Handle {
                kind: HandleKind::Artifact,
                value: b"src/a.rs".to_vec()
            })?
            .is_none()
    );
    Ok(())
}

#[test]
fn replacing_a_file_removes_its_old_rows_and_unshared_payload() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut writer = two_files(directory.path())?;
    let old = IndexReader::open(directory.path())?
        .file("src/a.rs")?
        .expect("stored")
        .payload;
    // A second file shares the first file's payload; replacing the first keeps it.
    let mut copy = entry("src/c.rs", "fn alpha() {}", "rust", &["xcopy"])?;
    copy.handles.clear();
    writer.apply(Batch {
        files: vec![upsert(copy)],
        ..Default::default()
    })?;
    writer.apply(Batch {
        files: vec![upsert(entry(
            "src/a.rs",
            "fn delta() {}",
            "rust",
            &["xdelta"],
        )?)],
        ..Default::default()
    })?;
    let reader = IndexReader::open(directory.path())?;
    assert!(reader.lexical(&query(&["xalpha"], true))?.0.is_empty());
    assert!(reader.lexical(&query(&["xbeta"], true))?.0.is_empty());
    assert_eq!(reader.lexical(&query(&["xdelta"], true))?.0.len(), 1);
    assert_eq!(
        reader.payload(&old)?,
        b"fn alpha() {}",
        "still used by src/c.rs"
    );
    drop(reader);
    writer.apply(Batch {
        files: vec![FileChange::Delete("src/c.rs".into())],
        ..Default::default()
    })?;
    let reader = IndexReader::open(directory.path())?;
    assert!(
        reader.payload(&old).is_err(),
        "an unreferenced payload is removed"
    );
    drop(reader);
    writer.verify()
}

#[test]
fn a_failed_batch_leaves_the_previous_publication_untouched() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut writer = two_files(directory.path())?;
    let mut oversized = entry("src/z.rs", "z", "rust", &[])?;
    oversized.chunks = (0..writer::MAX_CHUNKS_PER_FILE + 1)
        .map(|ordinal| ChunkEntry {
            stable_id: hash(&ordinal.to_string()),
            citation: vec![],
            terms: "xz".into(),
        })
        .collect();
    let error = writer.apply(Batch {
        files: vec![
            FileChange::Delete("src/a.rs".into()),
            upsert(entry("src/m.rs", "m", "rust", &["xm"])?),
            upsert(oversized),
        ],
        ..Default::default()
    });
    assert!(error.is_err());
    let reader = IndexReader::open(directory.path())?;
    assert_eq!(reader.sequence(), 1);
    assert!(reader.file("src/a.rs")?.is_some(), "the delete rolled back");
    assert!(
        reader.file("src/m.rs")?.is_none(),
        "the earlier insert rolled back"
    );
    assert!(reader.lexical(&query(&["xm"], true))?.0.is_empty());
    Ok(())
}

#[test]
fn handles_and_payloads_can_move_to_an_earlier_path_in_one_batch() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut writer = IndexWriter::create(directory.path())?;
    let mut original = entry("src/z.rs", "moved", "rust", &["xmoved"])?;
    original.handles = vec![Handle {
        kind: HandleKind::Artifact,
        value: b"id-1".to_vec(),
    }];
    writer.apply(Batch {
        files: vec![upsert(original.clone())],
        ..Default::default()
    })?;
    let mut moved = original;
    moved.path = "src/a.rs".into();
    writer.apply(Batch {
        files: vec![upsert(moved), FileChange::Delete("src/z.rs".into())],
        ..Default::default()
    })?;
    let reader = IndexReader::open(directory.path())?;
    assert_eq!(
        reader.handle(&Handle {
            kind: HandleKind::Artifact,
            value: b"id-1".to_vec()
        })?,
        Some("src/a.rs".into())
    );
    let file = reader.file("src/a.rs")?.expect("moved");
    assert_eq!(reader.payload(&file.payload)?, b"moved");
    Ok(())
}

#[test]
fn stat_refreshes_do_not_publish_and_batches_must_be_sorted() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut writer = two_files(directory.path())?;
    let commit = writer.apply(Batch {
        stats: vec![(
            "src/a.rs".into(),
            FileStat {
                size: 13,
                modified_ns: 99,
                file_id: vec![3],
            },
        )],
        ..Default::default()
    })?;
    assert_eq!(commit.sequence, 1);
    let reader = IndexReader::open(directory.path())?;
    assert_eq!(
        reader
            .file("src/a.rs")?
            .and_then(|file| file.stat)
            .map(|stat| stat.modified_ns),
        Some(99)
    );
    drop(reader);
    let unsorted = writer.apply(Batch {
        files: vec![
            FileChange::Delete("src/b.py".into()),
            FileChange::Delete("src/a.rs".into()),
        ],
        ..Default::default()
    });
    assert!(unsorted.is_err());
    assert!(
        writer
            .apply(Batch {
                files: vec![FileChange::Delete("a\\b".into())],
                ..Default::default()
            })
            .is_err()
    );
    Ok(())
}

fn setup() -> VectorSetup {
    VectorSetup {
        dimensions: 2,
        pipeline: hash("pipeline-1"),
    }
}

fn unit(ordinal: u32, key: &str) -> UnitEntry {
    UnitEntry {
        ordinal,
        embedding_key: hash(key),
        members: format!("members:{key}").into_bytes(),
    }
}

#[test]
fn vectors_are_searchable_after_commit_and_shared_by_key() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut writer = two_files(directory.path())?;
    assert!(writer.configure_vectors(Some(&setup()))?);
    assert!(
        !writer.configure_vectors(Some(&setup()))?,
        "same pipeline changes nothing"
    );
    let commit = writer.commit_vectors(VectorBatch {
        setup: setup(),
        files: vec![
            FileVectors {
                path: "src/a.rs".into(),
                source_hash: hash("fn alpha() {}"),
                units: vec![unit(0, "north"), unit(1, "east")],
                failed: false,
            },
            FileVectors {
                path: "src/b.py".into(),
                source_hash: hash("def gamma(): pass"),
                units: vec![unit(0, "north")],
                failed: false,
            },
        ],
        vectors: vec![
            (hash("north"), vec![0.0, 1.0]),
            (hash("east"), vec![1.0, 0.0]),
            (hash("north"), vec![0.0, 1.0]),
        ],
    })?;
    assert_eq!(
        (commit.rows_added, commit.files_done, commit.files_stale),
        (2, 2, 0)
    );
    let reader = IndexReader::open(directory.path())?;
    assert_eq!(reader.vector_state_count(VectorState::Done)?, 2);
    let matrix = reader.load_vectors()?.expect("vectors configured");
    assert_eq!(matrix.rows(), 2);
    let hits = matrix.search(&[0.6, 0.8], 2, None)?;
    assert_eq!(
        hits.iter().map(|hit| hit.row).collect::<Vec<_>>(),
        [0, 1],
        "north is closer"
    );
    let units = reader.units_for_row(hits[0].row)?;
    assert_eq!(
        units
            .iter()
            .map(|unit| (unit.path.as_str(), unit.ordinal))
            .collect::<Vec<_>>(),
        [("src/a.rs", 0), ("src/b.py", 0)],
        "one row serves every unit with the same input"
    );
    assert_eq!(units[0].members, b"members:north");
    assert_eq!(reader.rows_for_paths(&["src/b.py".into()])?, [0]);
    let subset = matrix.search(&[1.0, 0.0], 5, Some(&[0]))?;
    assert_eq!(subset.iter().map(|hit| hit.row).collect::<Vec<_>>(), [0]);
    Ok(())
}

#[test]
fn stale_planning_and_rows_past_the_committed_count_are_ignored() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut writer = two_files(directory.path())?;
    writer.configure_vectors(Some(&setup()))?;
    let commit = writer.commit_vectors(VectorBatch {
        setup: setup(),
        files: vec![FileVectors {
            path: "src/a.rs".into(),
            source_hash: hash("an older version"),
            units: vec![unit(0, "north")],
            failed: false,
        }],
        vectors: vec![(hash("north"), vec![0.0, 1.0])],
    })?;
    assert_eq!((commit.files_done, commit.files_stale), (0, 1));
    // Leftover rows of an uncommitted batch sit past the committed count.
    let mut file = vectors::VectorFile::open_append(directory.path(), 1, &setup())?;
    file.write_rows(1, [[5.0f32, 5.0].as_slice()].into_iter())?;
    file.sync()?;
    let reader = IndexReader::open(directory.path())?;
    assert_eq!(reader.load_vectors()?.expect("configured").rows(), 1);
    assert_eq!(
        reader.file("src/a.rs")?.expect("stored").vector_state,
        VectorState::Pending
    );
    let wrong = writer.commit_vectors(VectorBatch {
        setup: VectorSetup {
            dimensions: 2,
            pipeline: hash("another"),
        },
        files: vec![],
        vectors: vec![],
    });
    assert!(wrong.is_err(), "a batch for another pipeline is refused");
    Ok(())
}

#[test]
fn a_new_pipeline_starts_an_empty_generation_and_requeues_files() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut writer = two_files(directory.path())?;
    writer.configure_vectors(Some(&setup()))?;
    writer.commit_vectors(VectorBatch {
        setup: setup(),
        files: vec![FileVectors {
            path: "src/a.rs".into(),
            source_hash: hash("fn alpha() {}"),
            units: vec![unit(0, "north")],
            failed: false,
        }],
        vectors: vec![(hash("north"), vec![0.0, 1.0])],
    })?;
    let next = VectorSetup {
        dimensions: 3,
        pipeline: hash("pipeline-2"),
    };
    assert!(writer.configure_vectors(Some(&next))?);
    let reader = IndexReader::open(directory.path())?;
    let (current, generation, rows) = reader.vector_setup()?.expect("configured");
    assert_eq!((current, generation, rows), (next, 2, 0));
    assert_eq!(reader.vector_state_count(VectorState::Pending)?, 2);
    assert!(reader.units("src/a.rs")?.is_empty());
    drop(reader);
    assert!(
        !directory.path().join("vectors-1.f32").exists(),
        "the retired generation is removed"
    );
    writer.configure_vectors(None)?;
    let reader = IndexReader::open(directory.path())?;
    assert!(reader.vector_setup()?.is_none());
    assert_eq!(reader.vector_state_count(VectorState::NotSelected)?, 2);
    Ok(())
}

#[test]
fn a_database_of_another_format_is_refused() -> Result<()> {
    let directory = tempfile::tempdir()?;
    rusqlite::Connection::open(directory.path().join(DATABASE_FILE))?
        .execute_batch("CREATE TABLE something(x);")?;
    let error = IndexReader::open(directory.path()).err().expect("refused");
    assert!(error.is::<IncompatibleIndex>());
    assert!(
        IndexWriter::open(directory.path())
            .err()
            .expect("refused")
            .is::<IncompatibleIndex>()
    );
    assert!(
        IndexWriter::create(directory.path()).is_err(),
        "never overwrites"
    );
    Ok(())
}

#[test]
fn payload_encoding_round_trips_and_detects_damage() -> Result<()> {
    let text = "repeat ".repeat(1000);
    let payload = EncodedPayload::encode(text.as_bytes())?;
    assert!(
        payload.stored_bytes() < text.len(),
        "repetitive text compresses"
    );
    assert_eq!(
        decode_payload(payload.encoding, payload.raw_bytes, &payload.body)?,
        text.as_bytes()
    );
    let tiny = EncodedPayload::encode(b"ab")?;
    assert_eq!(tiny.encoding, ENCODING_RAW);
    assert!(decode_payload(9, 2, b"ab").is_err());
    assert!(decode_payload(ENCODING_RAW, 3, b"ab").is_err());
    Ok(())
}

#[test]
fn exact_search_keeps_the_best_rows_with_ties_to_the_lower_row() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let setup = VectorSetup {
        dimensions: 2,
        pipeline: hash("p"),
    };
    vectors::VectorFile::create(directory.path(), 1, &setup)?;
    let rows: Vec<[f32; 2]> = (0..10_000)
        .map(|index| {
            if index % 2 == 0 {
                [1.0, 0.0]
            } else {
                [0.0, 1.0]
            }
        })
        .collect();
    let mut file = vectors::VectorFile::open_append(directory.path(), 1, &setup)?;
    file.write_rows(0, rows.iter().map(|row| row.as_slice()))?;
    file.sync()?;
    let matrix = VectorMatrix::load(directory.path(), 1, &setup, rows.len() as u64)?;
    let hits = matrix.search(&[0.0, 1.0], 3, None)?;
    assert_eq!(
        hits.iter().map(|hit| hit.row).collect::<Vec<_>>(),
        [1, 3, 5]
    );
    assert!(matrix.search(&[0.0, 1.0, 2.0], 3, None).is_err());
    assert!(
        VectorMatrix::load(directory.path(), 1, &setup, 20_000).is_err(),
        "rows past the file end"
    );
    Ok(())
}

#[test]
fn caller_meta_publishes_atomically_and_engine_keys_are_refused() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut writer = IndexWriter::create(directory.path())?;
    let commit = writer.apply(Batch {
        meta: vec![("scope".into(), b"{\"a\":1}".to_vec())],
        ..Default::default()
    })?;
    assert_eq!(commit.sequence, 1, "a meta-only batch publishes");
    let reader = IndexReader::open(directory.path())?;
    assert_eq!(
        reader.meta("scope")?.as_deref(),
        Some(b"{\"a\":1}".as_slice())
    );
    assert_eq!(reader.meta("collection_state")?, None);
    for key in ["sequence", "format", "vector_rows"] {
        assert!(
            writer
                .apply(Batch {
                    meta: vec![(key.into(), b"1".to_vec())],
                    ..Default::default()
                })
                .is_err(),
            "engine key {key} accepted"
        );
    }
    assert_eq!(writer.sequence()?, 1);
    Ok(())
}
