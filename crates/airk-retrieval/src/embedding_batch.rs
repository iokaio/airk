// SPDX-License-Identifier: Apache-2.0
//! Bounded inference batches. Cache writes and graph insertion stay on the caller.
use crate::model::{EmbeddingCacheStats, EmbeddingModel, onnx_input_is_blank};
use airk_core::embedding::{
    EmbeddingCache, EmbeddingDescriptor, lookup_embedding, store_embedding,
};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, time::Instant};

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

struct InputGroup<'a> {
    text: &'a str,
    positions: Vec<usize>,
}

struct PreparedBatch<'a> {
    vectors: Vec<Embedded>,
    pending: Vec<InputGroup<'a>>,
}

/// Group only exact inputs, without copying their text or changing cache identity.
/// All cache validation finishes before any inference or cache persistence starts.
fn prepare_onnx_batch<'a>(
    texts: &[&'a str],
    descriptor: &EmbeddingDescriptor,
    cache: Option<&dyn EmbeddingCache>,
    bypass: bool,
    stats: &mut EmbeddingCacheStats,
) -> Result<PreparedBatch<'a>> {
    ensure!(
        texts.len() <= MAX_BATCH_CHUNKS,
        "embedding batch exceeds chunk bound"
    );
    let started = Instant::now();
    let mut positions: BTreeMap<&str, usize> = BTreeMap::new();
    let mut groups: Vec<InputGroup<'a>> = Vec::new();
    let mut blanks = 0;
    let mut duplicates = 0;
    for (index, text) in texts.iter().copied().enumerate() {
        // Keep the public input/descriptor bounds even for skipped blank inputs.
        airk_core::embedding::embedding_key(descriptor, text.as_bytes())?;
        if onnx_input_is_blank(text) {
            blanks += 1;
        } else if let Some(group) = positions.get(text).copied() {
            groups[group].positions.push(index);
            duplicates += 1;
        } else {
            positions.insert(text, groups.len());
            groups.push(InputGroup {
                text,
                positions: vec![index],
            });
        }
    }
    let mut vectors = vec![None; texts.len()];
    let mut pending = Vec::new();
    let mut hits = 0;
    let mut misses = 0;
    for group in groups {
        if let Some(cache) = cache.filter(|_| !bypass) {
            if let Some(vector) = lookup_embedding(cache, descriptor, group.text.as_bytes())? {
                hits += group.positions.len() as u64;
                for position in group.positions {
                    vectors[position] = Some(vector.clone());
                }
                continue;
            }
            misses += group.positions.len() as u64;
        }
        pending.push(group);
    }
    stats.inputs += texts.len() as u64;
    stats.blank_inputs += blanks;
    stats.duplicate_inputs += duplicates;
    stats.inferences += blanks;
    stats.no_vector += blanks;
    stats.hits += hits;
    stats.misses += misses;
    stats.cache_lookup_us += elapsed_us(started);
    Ok(PreparedBatch { vectors, pending })
}

/// All joins complete before returning, including failure. Results retain input order.
#[cfg(any(feature = "local-inference", test))]
pub(crate) fn parallel_map<T: Sync, R: Send>(
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
    ensure!(workers > 0, "embedding worker count must be positive");
    let workers = workers.min(items.len());
    let results = std::thread::scope(|scope| {
        // Length-sorted shape groups must share their expensive tail across
        // workers. Scatter round-robin, then restore the original result order.
        let handles: Vec<_> = (0..workers)
            .map(|worker| {
                let work = &work;
                scope.spawn(move || {
                    items
                        .iter()
                        .enumerate()
                        .skip(worker)
                        .step_by(workers)
                        .map(|(index, item)| Ok((index, work(item)?)))
                        .collect::<Result<Vec<_>>>()
                })
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
    let mut results = results
        .into_iter()
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    results.sort_unstable_by_key(|(index, _)| *index);
    Ok(results.into_iter().map(|(_, result)| result).collect())
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
    stats.plan_cache_peak_bytes = stats
        .plan_cache_peak_bytes
        .max(model.plan_cache_peak_bytes()?);
    if !(matches!(model, EmbeddingModel::Onnx(_))
        || matches!(model, EmbeddingModel::Remote(remote) if remote.is_onnx()))
    {
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
    let PreparedBatch {
        mut vectors,
        pending,
    } = prepare_onnx_batch(texts, descriptor, cache, bypass, stats)?;
    if pending.is_empty() {
        return Ok(vectors);
    }
    // ONNX acquires a measured per-plan workspace permit before running. This
    // outer bound reserves tokenizer/input state while retaining worker parallelism.
    let limit = memory_limited_workers(max_workers, 0, memory_budget).min(pending.len());
    let started = Instant::now();
    let texts: Vec<_> = pending.iter().map(|group| group.text).collect();
    let (computed, execution) = match model {
        EmbeddingModel::Onnx(onnx) => onnx.embed_batch_measured(&texts, limit, memory_budget)?,
        EmbeddingModel::Remote(remote) => {
            remote.embed_batch_measured(&texts, limit, memory_budget)?
        }
        _ => unreachable!("word models handled above"),
    };
    stats.inference_us += elapsed_us(started);
    stats.add_execution(&execution);
    stats.workers = stats.workers.max(execution.workers);
    stats.inferences += pending
        .iter()
        .map(|group| group.positions.len() as u64)
        .sum::<u64>();
    let started = Instant::now();
    // Serialize cache persistence: duplicate inputs cannot race on immutable files
    // or quota reservations. Corruption still aborts the unpublished generation.
    for (group, vector) in pending.into_iter().zip(computed) {
        if let Some(cache) = cache {
            store_embedding(cache, descriptor, group.text.as_bytes(), vector.clone())?;
        }
        for index in group.positions {
            vectors[index] = Some(vector.clone());
        }
    }
    stats.cache_store_us += elapsed_us(started);
    Ok(vectors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use airk_core::{embedding::CachedEmbedding, identity::Sha256Digest};
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    #[derive(Default)]
    struct Cache {
        values: Mutex<BTreeMap<Sha256Digest, CachedEmbedding>>,
        lookups: AtomicUsize,
        stores: AtomicUsize,
    }
    impl EmbeddingCache for Cache {
        fn lookup(&self, key: &Sha256Digest) -> Result<Option<CachedEmbedding>> {
            self.lookups.fetch_add(1, Ordering::Relaxed);
            Ok(self.values.lock().unwrap().get(key).cloned())
        }
        fn store(&self, entry: &CachedEmbedding) -> Result<()> {
            self.stores.fetch_add(1, Ordering::Relaxed);
            let mut values = self.values.lock().unwrap();
            if let Some(existing) = values.get(&entry.key) {
                ensure!(existing == entry, "immutable cache mismatch");
            }
            values.insert(entry.key, entry.clone());
            Ok(())
        }
    }

    fn descriptor() -> EmbeddingDescriptor {
        let digest = Sha256Digest::from_bytes([1; 32]);
        EmbeddingDescriptor {
            version: 1,
            model: digest,
            tokenizer: digest,
            preprocessing: digest,
            pooling: digest,
            normalization: digest,
            inference: digest,
            dimensions: 2,
        }
    }

    #[test]
    fn preparation_deduplicates_exact_inputs_and_scatters_authenticated_hits() -> Result<()> {
        let descriptor = descriptor();
        let cache = Cache::default();
        let texts = [
            "alpha", " \r\n", "beta", "alpha", "\u{2003}", "gamma", "beta",
        ];
        let mut cold = EmbeddingCacheStats::default();
        let prepared = prepare_onnx_batch(&texts, &descriptor, Some(&cache), false, &mut cold)?;
        assert_eq!(
            prepared
                .pending
                .iter()
                .map(|group| (group.text, group.positions.clone()))
                .collect::<Vec<_>>(),
            vec![
                ("alpha", vec![0, 3]),
                ("beta", vec![2, 6]),
                ("gamma", vec![5])
            ]
        );
        assert!(prepared.vectors.iter().all(Option::is_none));
        assert_eq!(cache.lookups.load(Ordering::Relaxed), 3);
        assert_eq!(
            (cold.inputs, cold.blank_inputs, cold.duplicate_inputs),
            (7, 2, 2)
        );
        assert_eq!((cold.hits, cold.misses, cold.actual_forwards), (0, 5, 0));
        assert_eq!((cold.inferences, cold.no_vector), (2, 2));

        // These are cache fixtures, not a simulated neural inference adapter.
        for (text, vector) in [
            ("alpha", vec![1.0, 0.0]),
            ("beta", vec![0.0, 1.0]),
            ("gamma", vec![-1.0, 0.0]),
        ] {
            store_embedding(&cache, &descriptor, text.as_bytes(), vector)?;
        }
        let mut warm = EmbeddingCacheStats::default();
        let prepared = prepare_onnx_batch(&texts, &descriptor, Some(&cache), false, &mut warm)?;
        assert!(prepared.pending.is_empty());
        assert_eq!(
            prepared.vectors,
            vec![
                Some(vec![1.0, 0.0]),
                None,
                Some(vec![0.0, 1.0]),
                Some(vec![1.0, 0.0]),
                None,
                Some(vec![-1.0, 0.0]),
                Some(vec![0.0, 1.0]),
            ]
        );
        assert_eq!(cache.lookups.load(Ordering::Relaxed), 6);
        assert_eq!((warm.hits, warm.misses, warm.actual_forwards), (5, 0, 0));
        assert_eq!((warm.inferences, warm.no_vector), (2, 2));

        let mut bypass = EmbeddingCacheStats::default();
        let prepared = prepare_onnx_batch(&texts, &descriptor, Some(&cache), true, &mut bypass)?;
        assert_eq!(prepared.pending.len(), 3);
        assert_eq!(cache.lookups.load(Ordering::Relaxed), 6);
        assert_eq!((bypass.hits, bypass.misses), (0, 0));
        let distinct = prepare_onnx_batch(
            &["alpha", "alpha\n", "alpha\r\n"],
            &descriptor,
            None,
            false,
            &mut EmbeddingCacheStats::default(),
        )?;
        assert_eq!(
            distinct.pending.len(),
            3,
            "deduplication must preserve exact bytes"
        );
        let key = airk_core::embedding::embedding_key(&descriptor, b"alpha")?;
        cache.values.lock().unwrap().get_mut(&key).unwrap().vector[0] = 0.5;
        let mut rejected = EmbeddingCacheStats::default();
        assert!(
            prepare_onnx_batch(
                &["uncached", "alpha", "alpha"],
                &descriptor,
                Some(&cache),
                false,
                &mut rejected,
            )
            .is_err()
        );
        assert_eq!(rejected.actual_forwards, 0);
        assert_eq!(cache.stores.load(Ordering::Relaxed), 3);
        Ok(())
    }

    #[test]
    fn blank_prefixes_preserve_utf8_and_input_bounds_before_cache_access() -> Result<()> {
        use crate::model::MAX_INPUT_BYTES;
        let descriptor = descriptor();
        let cache = Cache::default();
        let prefix = format!("{}suffix", " ".repeat(MAX_INPUT_BYTES));
        let split_unicode = format!("{}é suffix", " ".repeat(MAX_INPUT_BYTES - 1));
        let complete_unicode = format!("{}é suffix", " ".repeat(MAX_INPUT_BYTES - 2));
        assert!(!prefix.trim().is_empty());
        assert!(onnx_input_is_blank(&prefix));
        assert!(onnx_input_is_blank(&split_unicode));
        assert!(!onnx_input_is_blank(&complete_unicode));
        let mut stats = EmbeddingCacheStats::default();
        let prepared = prepare_onnx_batch(
            &[&prefix, &split_unicode],
            &descriptor,
            Some(&cache),
            false,
            &mut stats,
        )?;
        assert!(prepared.pending.is_empty());
        assert_eq!(stats.blank_inputs, 2);
        assert_eq!(stats.actual_forwards, 0);
        assert_eq!(cache.lookups.load(Ordering::Relaxed), 0);
        let oversized = " ".repeat(airk_core::embedding::MAX_EMBEDDING_INPUT_BYTES + 1);
        assert!(
            prepare_onnx_batch(&[&oversized], &descriptor, Some(&cache), false, &mut stats,)
                .is_err()
        );
        assert!(
            prepare_onnx_batch(
                &[" "; MAX_BATCH_CHUNKS + 1],
                &descriptor,
                Some(&cache),
                false,
                &mut stats,
            )
            .is_err()
        );
        assert_eq!(cache.lookups.load(Ordering::Relaxed), 0);
        Ok(())
    }

    #[test]
    #[ignore = "requires AIRK_TEST_MODEL_DIR with verified local ONNX assets"]
    fn real_model_parallel_vectors_cache_and_bypass_are_exact() -> Result<()> {
        let model =
            EmbeddingModel::open(std::path::Path::new(&std::env::var("AIRK_TEST_MODEL_DIR")?))?;
        ensure!(
            matches!(model, EmbeddingModel::Onnx(_)),
            "ONNX assets required"
        );
        let descriptor = model.cache_descriptor()?;
        let batch_four = match &model {
            EmbeddingModel::Onnx(onnx) => crate::model::qualified_batch_four(&onnx.manifest),
            _ => false,
        };
        let long = "A bounded original input with Unicode café. ".repeat(600);
        let blank_prefix = format!("{}suffix", " ".repeat(crate::model::MAX_INPUT_BYTES));
        let split_unicode = format!("{}é suffix", " ".repeat(crate::model::MAX_INPUT_BYTES - 1));
        let texts = [
            "Reset an account password.",
            "Bake bread with flour and yeast.",
            "",
            "  ",
            "Search for a parser definition.",
            "Reset an account password.",
            long.as_str(),
            "An original final evidence passage.",
            blank_prefix.as_str(),
            split_unicode.as_str(),
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
        assert_eq!(cold.no_vector, 4);
        assert_eq!(
            (cold.inputs, cold.blank_inputs, cold.duplicate_inputs),
            (10, 4, 1)
        );
        assert_eq!(cold.actual_forwards, if batch_four { 2 } else { 5 });
        assert_eq!(cache.lookups.load(Ordering::Relaxed), 5);
        assert_eq!(cache.stores.load(Ordering::Relaxed), 5);
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
            warm.inferences, 4,
            "empty outcomes are not persisted as vectors"
        );
        assert_eq!(warm.actual_forwards, 0);
        assert!(
            warm.plan_cache_peak_bytes > 0,
            "warm hits still retain the parsed template/plans"
        );
        assert_eq!(warm.workers, 0);
        assert_eq!(cache.lookups.load(Ordering::Relaxed), 10);
        assert_eq!(cache.stores.load(Ordering::Relaxed), 5);
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
        assert_eq!(full.actual_forwards, if batch_four { 2 } else { 5 });
        assert_eq!(cache.lookups.load(Ordering::Relaxed), 10);
        assert_eq!(cache.stores.load(Ordering::Relaxed), 10);
        let mut direct = EmbeddingCacheStats::default();
        assert!(
            model
                .embed_cached(&blank_prefix, &descriptor, Some(&cache), false, &mut direct,)?
                .is_none()
        );
        assert_eq!(
            (direct.inputs, direct.blank_inputs, direct.actual_forwards),
            (1, 1, 0)
        );
        assert_eq!(cache.lookups.load(Ordering::Relaxed), 10);
        for positions in [vec![7, 4, 1, 0], vec![0, 1, 7]] {
            let regrouped: Vec<_> = positions.iter().map(|index| texts[*index]).collect();
            let expected: Vec<_> = positions
                .iter()
                .map(|index| serial[*index].clone())
                .collect();
            let mut changed = EmbeddingCacheStats::default();
            assert_eq!(
                embed(
                    &model,
                    &regrouped,
                    &descriptor,
                    Some(&cache),
                    true,
                    &mut changed,
                    4,
                    1 << 30
                )?,
                expected,
                "new row positions and B1 tails must match previously cached B4 vectors exactly"
            );
            assert_eq!(
                changed.actual_forwards,
                if batch_four && positions.len() == 4 {
                    1
                } else {
                    positions.len() as u64
                }
            );
        }
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
        cache.values.lock().unwrap().get_mut(&key).unwrap().vector[0] += 1.0;
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
