// SPDX-License-Identifier: Apache-2.0
//! Compare authenticated vector candidate pointer resolution on a private index snapshot.
use airk_core::graph_storage::GraphRecordReader;
use airk_store::FileStore;
use airk_store::current::CurrentRead;
use airk_store::current::map::MapDomain;
use airk_store::current::vector_map::{OccurrencePointer, VectorBinding};
use anyhow::{Context, Result, ensure};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{path::PathBuf, time::Instant};

fn original(read: &CurrentRead, id: u32) -> Result<Vec<OccurrencePointer>> {
    let binding = read.vector_for_graph_id(id)?;
    read.vector_occurrence_pointers(binding.embedding_key)?
        .context("missing candidate pointers")
}

fn one_binding(read: &CurrentRead, id: u32) -> Result<Vec<OccurrencePointer>> {
    let record = read.read(id)?;
    ensure!(record.live, "candidate no longer live");
    let key = record.embedding_key.context("candidate has no key")?;
    let bytes = read
        .map_get(MapDomain::Vector, key.as_bytes())?
        .context("missing candidate binding")?;
    let binding = VectorBinding::decode(&bytes, read.head(), key)?;
    ensure!(
        binding.graph_id == id && !binding.occurrence_ids.is_empty(),
        "candidate binding differs"
    );
    let mut pointers = Vec::with_capacity(binding.occurrence_ids.len());
    for occurrence in binding.occurrence_ids {
        let bytes = read
            .map_get(MapDomain::Occurrence, occurrence.as_bytes())?
            .context("missing occurrence")?;
        let pointer = OccurrencePointer::decode(&bytes, occurrence)?;
        if !read.accepts_path(&pointer.path) {
            continue;
        }
        ensure!(
            pointer.embedding_key == key,
            "different occurrence vector key"
        );
        let path = read
            .path_occurrences(&pointer.path)?
            .context("missing path occurrence index")?;
        ensure!(
            path.occurrence_id(&read.head().scope, pointer.path_ordinal)? == occurrence
                && path.source_hash == pointer.source_hash,
            "path ordinal or source mismatch"
        );
        pointers.push(pointer);
    }
    Ok(pointers)
}

fn main() -> Result<()> {
    let [clone, original_root, incarnation, sequence, digest]: [String; 5] = std::env::args()
        .skip(1)
        .collect::<Vec<_>>()
        .try_into()
        .map_err(|_| anyhow::anyhow!("expected CLONE ROOT INCARNATION GENERATION DIGEST"))?;
    let dir = PathBuf::from(clone).canonicalize()?;
    ensure!(
        dir.ends_with("cal100-store-candidate-timing-r1/index/collections-1.4/cal100"),
        "isolated development snapshot required"
    );
    let active: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("active.json"))?)?;
    ensure!(
        active["active"].as_str() == Some(&incarnation),
        "unexpected active incarnation"
    );
    let store = FileStore::new(dir.join("incarnations").join(&incarnation));
    let selected = store
        .current_scope(&original_root, "cal100")?
        .context("original CAL publication missing")?;
    let read = store.current_read(&selected)?;
    let token = read.head().token()?;
    ensure!(
        token.incarnation.to_string() == incarnation
            && token.sequence == sequence.parse::<u64>()?
            && token.digest.to_string() == digest,
        "original current publication changed"
    );
    let counts = read.counts()?;
    let mut ids = Vec::new();
    for id in 0..u32::try_from(counts.allocated_count)? {
        if read.read(id)?.live {
            ids.push(id);
            if ids.len() == 400 {
                break;
            }
        }
    }
    ensure!(
        ids.len() == 400,
        "fewer than the original hybrid branch width"
    );
    for width in [64, 400] {
        let selection = &ids[..width];
        let mut expected = None;
        for (trial, fused) in [false, true, true, false].into_iter().enumerate() {
            let start = Instant::now();
            let streams = selection
                .iter()
                .map(|&id| {
                    if fused {
                        one_binding(&read, id)
                    } else {
                        original(&read, id)
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            let elapsed = start.elapsed();
            let signature = format!("{:x}", Sha256::digest(serde_json::to_vec(&streams)?));
            if let Some(previous) = expected.as_ref() {
                ensure!(
                    previous == &signature,
                    "authenticated ordered pointer stream changed"
                );
            } else {
                expected = Some(signature.clone());
            }
            println!(
                "{}",
                json!({"width":width,"trial":trial,"one_binding":fused,"elapsed_ns":elapsed.as_nanos(),
                "pointer_count":streams.iter().map(Vec::len).sum::<usize>(), "stream_sha256":signature,
                "id_selection":"first live graph ids, independent of ANN query ranking"})
            );
        }
    }
    Ok(())
}
