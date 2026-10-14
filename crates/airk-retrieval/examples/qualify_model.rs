// SPDX-License-Identifier: Apache-2.0
//! Explicit real-asset qualification: cargo run --example qualify_model -- <asset-directory>.
use airk_retrieval::model::{EmbeddingModel, inspect};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{path::PathBuf, time::Instant};

#[derive(Serialize)]
struct Case {
    query: &'static str,
    expected: usize,
    retrieved: usize,
    similarity: f32,
    embedding_us: u128,
}

fn main() -> Result<()> {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("asset directory required")?,
    );
    let info = inspect(&directory)?;
    let started = Instant::now();
    let model = EmbeddingModel::open(&directory)?;
    let load_ms = started.elapsed().as_millis();
    let documents = [
        "Reset a forgotten account password by sending a time-limited recovery link to the verified email address.",
        "The parser reads comma-separated spreadsheet rows and preserves quoted cell values, headers, and row numbers.",
        "Exponential backoff retries a failed HTTP request with a growing delay; stop after the configured attempt limit.",
        "A database transaction commits all changes together or rolls them back when validation fails.",
        "def sort_events(events):\n    # Order events by their timestamp from earliest to latest.\n    return sorted(events, key=lambda event: event.timestamp)",
        "fn digest_file(bytes: &[u8]) -> String {\n    // Compute a SHA-256 checksum used to detect altered file contents.\n    hex::encode(Sha256::digest(bytes))\n}",
        "A microscope magnifies biological samples using lenses and illumination.",
        "The kitchen recipe combines flour, water, yeast and salt before baking the bread.",
    ];
    let queries = [
        (
            "How can a user recover access when they cannot remember their login secret?",
            0,
        ),
        (
            "Extract fields from CSV while retaining the original record positions",
            1,
        ),
        ("Wait longer between repeated network failures", 2),
        (
            "Ensure either every database update succeeds or none are applied",
            3,
        ),
        ("Arrange a list of events in chronological order", 4),
        (
            "Identify whether the contents of a file have changed using a cryptographic fingerprint",
            5,
        ),
    ];
    let vectors = documents
        .iter()
        .map(|text| model.embed_text(text)?.context("empty document embedding"))
        .collect::<Result<Vec<_>>>()?;
    let mut cases = Vec::new();
    let mut query_vectors = Vec::new();
    for (query, expected) in queries {
        let started = Instant::now();
        let vector = model.embed_text(query)?.context("empty query embedding")?;
        let embedding_us = started.elapsed().as_micros();
        let (retrieved, similarity) = vectors
            .iter()
            .enumerate()
            .map(|(index, item)| {
                (
                    index,
                    item.iter().zip(&vector).map(|(a, b)| a * b).sum::<f32>(),
                )
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .context("empty candidates")?;
        cases.push(Case {
            query,
            expected,
            retrieved,
            similarity,
            embedding_us,
        });
        query_vectors.push(vector);
    }
    let correct = cases
        .iter()
        .filter(|case| case.expected == case.retrieved)
        .count();
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"model":info,"load_ms":load_ms,"documents":documents,"cases":cases,"correct":correct,"total":queries.len(),"document_vectors":vectors,"query_vectors":query_vectors,"scope":"Original small English code/document fixtures; not production-corpus, compiler-resolution, cross-platform or language-general qualification"})
        )?
    );
    ensure!(
        correct == queries.len(),
        "real-model fixture retrieval did not meet the fixed top-1 gate"
    );
    Ok(())
}
