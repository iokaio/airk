// SPDX-License-Identifier: Apache-2.0
//! Bounded inference batches. Cache writes and graph insertion stay on the caller.
use crate::model::{EmbeddingCacheStats, EmbeddingModel};
use airk_core::embedding::{
    EmbeddingCache, EmbeddingDescriptor, lookup_embedding, store_embedding,
};
use anyhow::{Context, Result, ensure};
use std::time::Instant;

pub const MAX_BATCH_CHUNKS: usize = 32;

/// An execution-only override: it never changes model/cache/revision identity.
pub fn worker_limit(value: Option<&str>, available: usize) -> Result<usize> {
    let requested = value
        .map(str::parse::<usize>)
        .transpose()
        .context("AIRK_INDEX_WORKERS must be an integer from 1 through 8")?
        .unwrap_or(4);
    ensure!(
        (1..=8).contains(&requested),
        "AIRK_INDEX_WORKERS must be from 1 through 8"
    );
    Ok(requested.min(available.max(1)))
}

pub(crate) fn elapsed_us(start: Instant) -> u64 {
    start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}

fn memory_limited_workers(requested: usize, workspace: usize, budget: usize) -> usize {
    // Includes a reserve for tokenizer/input/output state. One worker retains the
    // already validated single-inference limit, even on a small machine.
    requested
        .clamp(1, 8)
        .min((budget / workspace.saturating_add(8 << 20).max(1)).max(1))
}

type Embedded = Option<Vec<f32>>;

/// All joins complete before returning, including failure. Results retain input order.
fn parallel_map<T: Sync, R: Send>(
    items: &[T],
    workers: usize,
    work: impl Fn(&T) -> Result<R> + Sync,
) -> Result<Vec<R>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }
    if workers == 1 {
        return items.iter().map(work).collect();
    }
    let results = std::thread::scope(|scope| {
        let handles: Vec<_> = items
            .chunks(items.len().div_ceil(workers))
            .map(|part| {
                let work = &work;
                scope.spawn(move || part.iter().map(work).collect::<Result<Vec<_>>>())
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("embedding worker panicked"))
                    .and_then(|result| result)
            })
            .collect::<Vec<_>>()
    });
    Ok(results
        .into_iter()
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn embed(
    model: &EmbeddingModel,
    texts: &[&str],
    descriptor: &EmbeddingDescriptor,
    cache: Option<&dyn EmbeddingCache>,
    bypass: bool,
    stats: &mut EmbeddingCacheStats,
    max_workers: usize,
    memory_budget: usize,
) -> Result<Vec<Embedded>> {
    ensure!(
        texts.len() <= MAX_BATCH_CHUNKS,
        "embedding batch exceeds chunk bound"
    );
    stats.batches += 1;
    let EmbeddingModel::Onnx(onnx) = model else {
        let inferences = stats.inferences;
        let result = texts
            .iter()
            .map(|text| model.embed_cached(text, descriptor, cache, bypass, stats))
            .collect();
        if stats.inferences > inferences {
            stats.workers = stats.workers.max(1);
        }
        return result;
    };
    let started = Instant::now();
    let mut vectors = vec![None; texts.len()];
    let mut pending = Vec::new();
    for (index, text) in texts.iter().enumerate() {
        airk_core::embedding::embedding_key(descriptor, text.as_bytes())?;
        if let Some(cache) = cache.filter(|_| !bypass) {
            if let Some(vector) = lookup_embedding(cache, descriptor, text.as_bytes())? {
                stats.hits += 1;
                vectors[index] = Some(vector);
                continue;
            }
            stats.misses += 1;
        }
        pending.push((index, *text));
    }
    stats.cache_lookup_us += elapsed_us(started);
    if pending.is_empty() {
        return Ok(vectors);
    }
    let limit =
        memory_limited_workers(max_workers, onnx.workspace_bytes, memory_budget).min(pending.len());
    let workers = pending.len().div_ceil(pending.len().div_ceil(limit));
    stats.workers = stats.workers.max(workers);
    let started = Instant::now();
    let computed = parallel_map(&pending, workers, |(_, text)| model.embed_text(text))?;
    stats.inference_us += elapsed_us(started);
    stats.inferences += pending.len() as u64;
    let started = Instant::now();
    // Serialize cache persistence: duplicate inputs cannot race on immutable files
    // or quota reservations. Corruption still aborts the unpublished generation.
    for ((index, text), vector) in pending.into_iter().zip(computed) {
        if let Some(vector) = &vector {
            if let Some(cache) = cache {
                store_embedding(cache, descriptor, text.as_bytes(), vector.clone())?;
            }
        } else {
            stats.no_vector += 1;
        }
        vectors[index] = vector;
    }
    stats.cache_store_us += elapsed_us(started);
    Ok(vectors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    #[ignore = "requires AIRK_TEST_MODEL_DIR with verified local ONNX assets"]
    fn real_model_parallel_vectors_cache_and_bypass_are_exact() -> Result<()> {
        use airk_core::{embedding::CachedEmbedding, identity::Sha256Digest};
        use std::{collections::BTreeMap, sync::Mutex};
        #[derive(Default)]
        struct Cache(Mutex<BTreeMap<Sha256Digest, CachedEmbedding>>);
        impl EmbeddingCache for Cache {
            fn lookup(&self, key: &Sha256Digest) -> Result<Option<CachedEmbedding>> {
                Ok(self.0.lock().unwrap().get(key).cloned())
            }
            fn store(&self, entry: &CachedEmbedding) -> Result<()> {
                let mut values = self.0.lock().unwrap();
                if let Some(existing) = values.get(&entry.key) {
                    ensure!(existing == entry, "immutable cache mismatch");
                }
                values.insert(entry.key, entry.clone());
                Ok(())
            }
        }
        let model =
            EmbeddingModel::open(std::path::Path::new(&std::env::var("AIRK_TEST_MODEL_DIR")?))?;
        let descriptor = model.cache_descriptor()?;
        let long = "A bounded original input with Unicode café. ".repeat(600);
        let texts = [
            "Reset an account password.",
            "Bake bread with flour and yeast.",
            "",
            "  ",
            "Search for a parser definition.",
            "Reset an account password.",
            long.as_str(),
            "An original final evidence passage.",
        ];
        let serial: Vec<_> = texts
            .iter()
            .map(|text| model.embed_text(text))
            .collect::<Result<_>>()?;
        let cache = Cache::default();
        let mut cold = EmbeddingCacheStats::default();
        let parallel = embed(
            &model,
            &texts,
            &descriptor,
            Some(&cache),
            false,
            &mut cold,
            4,
            1 << 30,
        )?;
        assert_eq!(
            parallel, serial,
            "parallel inference must preserve exact float bits and input order"
        );
        assert_eq!(cold.inferences, texts.len() as u64);
        assert_eq!(cold.no_vector, 2);
        assert!(
            cold.workers > 1,
            "real model should fit concurrent execution in the declared budget"
        );
        let mut warm = EmbeddingCacheStats::default();
        assert_eq!(
            embed(
                &model,
                &texts,
                &descriptor,
                Some(&cache),
                false,
                &mut warm,
                4,
                1 << 30
            )?,
            serial
        );
        assert_eq!(warm.hits, 6);
        assert_eq!(
            warm.inferences, 2,
            "empty outcomes are not persisted as vectors"
        );
        let mut full = EmbeddingCacheStats::default();
        assert_eq!(
            embed(
                &model,
                &texts,
                &descriptor,
                Some(&cache),
                true,
                &mut full,
                4,
                1 << 30
            )?,
            serial
        );
        assert_eq!(full.hits, 0);
        assert_eq!(full.inferences, texts.len() as u64);
        let mut rejected = EmbeddingCacheStats::default();
        assert!(
            embed(
                &model,
                &["original"; MAX_BATCH_CHUNKS + 1],
                &descriptor,
                Some(&cache),
                false,
                &mut rejected,
                4,
                1 << 30
            )
            .is_err()
        );
        let key = airk_core::embedding::embedding_key(&descriptor, texts[0].as_bytes())?;
        cache.0.lock().unwrap().get_mut(&key).unwrap().vector[0] += 1.0;
        assert!(
            embed(
                &model,
                &texts,
                &descriptor,
                Some(&cache),
                false,
                &mut rejected,
                4,
                1 << 30
            )
            .is_err()
        );
        assert_eq!(
            rejected.inferences, 0,
            "corruption must fail before inference"
        );
        eprintln!(
            "cold: {}; warm: {}; full: {}",
            serde_json::to_string(&cold)?,
            serde_json::to_string(&warm)?,
            serde_json::to_string(&full)?
        );
        Ok(())
    }

    #[test]
    fn worker_limits_respect_cpu_memory_and_invalid_overrides() -> Result<()> {
        assert_eq!(worker_limit(None, 16)?, 4);
        assert_eq!(worker_limit(Some("8"), 2)?, 2);
        assert_eq!(worker_limit(Some("1"), 16)?, 1);
        for value in ["0", "9", "-1", "many", ""] {
            assert!(worker_limit(Some(value), 16).is_err());
        }
        assert_eq!(memory_limited_workers(8, 120 << 20, 256 << 20), 2);
        assert_eq!(memory_limited_workers(8, 1 << 30, 128 << 20), 1);
        Ok(())
    }

    #[test]
    fn workers_preserve_order_and_finish_before_error_returns() -> Result<()> {
        let values: Vec<_> = (0..32).collect();
        assert_eq!(
            parallel_map(&values, 4, |value| Ok(value * 2))?,
            values.iter().map(|n| n * 2).collect::<Vec<_>>()
        );
        let completed = AtomicUsize::new(0);
        let result = parallel_map(&values, 4, |value| {
            if *value == 0 {
                anyhow::bail!("original inference failure");
            }
            completed.fetch_add(1, Ordering::SeqCst);
            Ok(*value)
        });
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("original inference failure")
        );
        assert_eq!(completed.load(Ordering::SeqCst), 24);
        assert!(
            parallel_map(&[0, 1], 2, |value| -> Result<usize> {
                assert_ne!(*value, 0, "fictional worker panic");
                Ok(*value)
            })
            .is_err()
        );
        Ok(())
    }
}
