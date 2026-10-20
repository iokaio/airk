// SPDX-License-Identifier: Apache-2.0
use super::*;
use super::{
    format::{self, INDEX_ENTRY_BYTES},
    update::Limits,
};
use crate::quota::{Quota, Reservation};
use airk_core::embedding::{CachedEmbedding, EmbeddingCache, EmbeddingDescriptor, embedding_key};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::PathBuf,
};

struct Fixture {
    _root: tempfile::TempDir,
    collection: PathBuf,
    quota: Quota,
    descriptor: EmbeddingDescriptor,
}
impl Fixture {
    fn new() -> Result<Self> {
        let root = tempfile::tempdir()?;
        let collection = root.path().join("collections-1.2/fixture");
        fs::create_dir_all(&collection)?;
        let quota = Quota::open(root.path())?;
        let digest = Sha256Digest::from_bytes([1; 32]);
        let descriptor = EmbeddingDescriptor {
            version: 1,
            model: digest,
            tokenizer: digest,
            preprocessing: digest,
            pooling: digest,
            normalization: digest,
            inference: digest,
            dimensions: 2,
        };
        Ok(Self {
            _root: root,
            collection,
            quota,
            descriptor,
        })
    }
    fn reserve(&self) -> Result<Reservation> {
        self.quota.reserve("fixture", 16 * 1024 * 1024)
    }
    fn entry(&self, value: u32) -> Result<CachedEmbedding> {
        CachedEmbedding::new(
            embedding_key(&self.descriptor, &value.to_be_bytes())?,
            vec![value as f32 / 1000.0, -0.0],
            2,
        )
    }
    fn build(&self, count: u32) -> Result<VectorCacheManifest> {
        let reservation = self.reserve()?;
        let cache = VectorCache::new(&self.collection, &reservation, &self.descriptor, None)?;
        for number in 0..count {
            cache.store(&self.entry(number)?)?;
        }
        cache.finalize()
    }
}

fn flip(path: &std::path::Path, offset: u64) -> Result<()> {
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut byte = [0u8];
    file.read_exact(&mut byte)?;
    byte[0] ^= 1;
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(&byte)?;
    Ok(())
}

#[test]
fn packed_round_trip_reuses_index_and_preserves_exact_float_bits() -> Result<()> {
    let fixture = Fixture::new()?;
    let reservation = fixture.reserve()?;
    let cache = VectorCache::new(&fixture.collection, &reservation, &fixture.descriptor, None)?;
    for number in 0..64 {
        cache.store(&fixture.entry(number)?)?;
    }
    let pending = cache.lookup(&fixture.entry(0)?.key)?.unwrap();
    assert_eq!(pending.vector[1].to_bits(), (-0.0f32).to_bits());
    let manifest = cache.finalize()?;
    assert_eq!(cache.finalize()?, manifest);
    assert_eq!(manifest.packs.len(), 1);
    assert_eq!(manifest.references.len(), 64);
    assert_eq!(cache.statistics()?.packs_written, 1);
    verify_pack(&fixture.collection, &manifest.packs[0])?;
    assert_eq!(
        VectorCacheManifest::from_bytes(&manifest.canonical_bytes()?)?,
        manifest
    );
    drop(cache);
    drop(reservation);

    let reservation = fixture.reserve()?;
    let warm = VectorCache::new(
        &fixture.collection,
        &reservation,
        &fixture.descriptor,
        Some(&manifest),
    )?;
    for number in (0..64).rev() {
        assert_eq!(
            warm.lookup(&fixture.entry(number)?.key)?,
            Some(fixture.entry(number)?)
        );
    }
    assert_eq!(warm.finalize()?, manifest);
    let stats = warm.statistics()?;
    assert_eq!(stats.packs_written, 0);
    assert_eq!(stats.packs_reused, 1);
    assert_eq!(stats.file_opens, 1);
    assert_eq!(stats.index_cache_hits, 63);
    assert_eq!(stats.read_bytes, manifest.packs[0].bytes);
    assert_eq!(stats.full_pack_verification_bytes, 0);

    let reservation = fixture.reserve()?;
    let bypass = VectorCache::new(
        &fixture.collection,
        &reservation,
        &fixture.descriptor,
        Some(&manifest),
    )?;
    for number in 0..64 {
        bypass.store(&fixture.entry(number)?)?;
    }
    assert_eq!(bypass.finalize()?, manifest);
    assert_eq!(bypass.statistics()?.packs_written, 0);
    assert_eq!(
        bypass.statistics()?.record_verification_bytes,
        64 * record_bytes(2)?
    );
    Ok(())
}

#[test]
fn changed_float_bits_fail_full_bypass_and_poison_update() -> Result<()> {
    let fixture = Fixture::new()?;
    let manifest = fixture.build(1)?;
    let reservation = fixture.reserve()?;
    let cache = VectorCache::new(
        &fixture.collection,
        &reservation,
        &fixture.descriptor,
        Some(&manifest),
    )?;
    let entry = fixture.entry(0)?;
    let changed = CachedEmbedding::new(entry.key, vec![0.0, 0.0], 2)?;
    assert_eq!(entry.vector, changed.vector);
    assert_ne!(entry.checksum, changed.checksum);
    assert!(cache.store(&changed).is_err());
    assert!(cache.lookup(&entry.key).is_err());
    assert!(cache.finalize().is_err());
    verify_pack(&fixture.collection, &manifest.packs[0])?;
    Ok(())
}

#[test]
fn orphan_packs_are_not_hits_and_digest_collision_requires_full_verification() -> Result<()> {
    let fixture = Fixture::new()?;
    let manifest = fixture.build(2)?;
    let reservation = fixture.reserve()?;
    let cache = VectorCache::new(&fixture.collection, &reservation, &fixture.descriptor, None)?;
    assert!(cache.lookup(&fixture.entry(0)?.key)?.is_none());
    for number in 0..2 {
        cache.store(&fixture.entry(number)?)?;
    }
    assert_eq!(cache.finalize()?, manifest);
    assert_eq!(
        cache.statistics()?.full_pack_verification_bytes,
        manifest.packs[0].bytes * 2
    );
    let path = pack_path(&fixture.collection, &manifest.packs[0].sha256)?;
    flip(&path, PACK_HEADER_BYTES + 68)?;
    let corrupt = fs::read(&path)?;
    let reservation = fixture.reserve()?;
    let cache = VectorCache::new(&fixture.collection, &reservation, &fixture.descriptor, None)?;
    for number in 0..2 {
        cache.store(&fixture.entry(number)?)?;
    }
    assert!(cache.finalize().is_err());
    assert_eq!(fs::read(path)?, corrupt);
    Ok(())
}

#[test]
fn old_pipeline_is_an_explicit_miss_without_decoding_old_packs() -> Result<()> {
    let fixture = Fixture::new()?;
    let manifest = fixture.build(1)?;
    fs::write(
        pack_path(&fixture.collection, &manifest.packs[0].sha256)?,
        b"corrupt old pipeline",
    )?;
    let mut next = fixture.descriptor.clone();
    next.dimensions = 3;
    let reservation = fixture.reserve()?;
    let cache = VectorCache::new(&fixture.collection, &reservation, &next, Some(&manifest))?;
    assert!(cache.lookup(&fixture.entry(0)?.key)?.is_none());
    let key = embedding_key(&next, b"next")?;
    cache.store(&CachedEmbedding::new(
        key,
        vec![0.0, -0.0, f32::from_bits(1)],
        3,
    )?)?;
    let current = cache.finalize()?;
    assert_eq!(current.dimensions, 3);
    assert_ne!(current.pipeline_digest, manifest.pipeline_digest);
    verify_pack(&fixture.collection, &current.packs[0])?;
    Ok(())
}

#[test]
fn header_index_record_truncation_and_hidden_bytes_fail_closed() -> Result<()> {
    for corruption in 0..5 {
        let fixture = Fixture::new()?;
        let manifest = fixture.build(2)?;
        let reference = &manifest.packs[0];
        let path = pack_path(&fixture.collection, &reference.sha256)?;
        match corruption {
            0 => flip(&path, 24)?,
            1 => flip(&path, reference.bytes - 1)?,
            2 => flip(&path, PACK_HEADER_BYTES + 68)?,
            3 => OpenOptions::new()
                .write(true)
                .open(&path)?
                .set_len(reference.bytes - 1)?,
            _ => OpenOptions::new()
                .append(true)
                .open(&path)?
                .write_all(b"hidden")?,
        }
        let reservation = fixture.reserve()?;
        let cache = VectorCache::new(
            &fixture.collection,
            &reservation,
            &fixture.descriptor,
            Some(&manifest),
        )?;
        assert!(
            cache.lookup(&fixture.entry(0)?.key).is_err(),
            "corruption {corruption}"
        );
        assert!(
            verify_pack(&fixture.collection, reference).is_err(),
            "corruption {corruption}"
        );
    }
    Ok(())
}

#[test]
fn targeted_lookup_defers_unselected_record_corruption_to_full_verifier() -> Result<()> {
    let fixture = Fixture::new()?;
    let manifest = fixture.build(2)?;
    let reference = &manifest.packs[0];
    let path = pack_path(&fixture.collection, &reference.sha256)?;
    flip(&path, PACK_HEADER_BYTES + record_bytes(2)? + 68)?;
    let reservation = fixture.reserve()?;
    let cache = VectorCache::new(
        &fixture.collection,
        &reservation,
        &fixture.descriptor,
        Some(&manifest),
    )?;
    assert_eq!(
        cache.lookup(&fixture.entry(0)?.key)?,
        Some(fixture.entry(0)?)
    );
    assert!(verify_pack(&fixture.collection, reference).is_err());
    assert!(cache.lookup(&fixture.entry(1)?.key).is_err());
    Ok(())
}

#[test]
fn ratio_compaction_retains_only_live_records_and_small_edit_keeps_pack() -> Result<()> {
    let fixture = Fixture::new()?;
    let manifest = fixture.build(10)?;
    let reservation = fixture.reserve()?;
    let sparse = VectorCache::new(
        &fixture.collection,
        &reservation,
        &fixture.descriptor,
        Some(&manifest),
    )?;
    for number in [1, 8] {
        sparse
            .lookup(&fixture.entry(number)?.key)?
            .context("fixture hit missing")?;
    }
    let compact = sparse.finalize()?;
    assert_eq!(compact.packs.len(), 1);
    assert_eq!(compact.packs[0].records, 2);
    assert_eq!(compact.references.len(), 2);
    assert_eq!(sparse.statistics()?.compacted_records, 2);
    verify_pack(&fixture.collection, &manifest.packs[0])?;
    verify_pack(&fixture.collection, &compact.packs[0])?;

    let reservation = fixture.reserve()?;
    let edit = VectorCache::new(
        &fixture.collection,
        &reservation,
        &fixture.descriptor,
        Some(&manifest),
    )?;
    for number in 1..10 {
        edit.lookup(&fixture.entry(number)?.key)?;
    }
    edit.store(&fixture.entry(100)?)?;
    let edited = edit.finalize()?;
    assert_eq!(edited.packs.len(), 2);
    assert_eq!(edit.statistics()?.compacted_records, 0);
    assert_eq!(edit.statistics()?.packs_reused, 1);
    Ok(())
}

#[test]
fn pack_pressure_compacts_across_packs_and_handle_index_caches_are_bounded() -> Result<()> {
    let fixture = Fixture::new()?;
    let reservation = fixture.reserve()?;
    let cold = VectorCache::with_limits(
        &fixture.collection,
        &reservation,
        &fixture.descriptor,
        None,
        Limits {
            records: 2,
            ..Default::default()
        },
    )?;
    for number in 0..16 {
        cold.store(&fixture.entry(number)?)?;
    }
    let manifest = cold.finalize()?;
    assert_eq!(manifest.packs.len(), 8);
    let reservation = fixture.reserve()?;
    let cache = VectorCache::with_limits(
        &fixture.collection,
        &reservation,
        &fixture.descriptor,
        Some(&manifest),
        Limits {
            packs: 1,
            index_bytes: 16 * std::mem::size_of::<format::IndexEntry>() as u64,
            ..Default::default()
        },
    )?;
    for number in 0..16 {
        cache.lookup(&fixture.entry(number)?.key)?;
    }
    let compact = cache.finalize()?;
    assert_eq!(compact.packs.len(), 1);
    assert_eq!(compact.packs[0].records, 16);
    assert_eq!(cache.statistics()?.compacted_records, 16);
    assert!(
        cache.statistics()?.index_cache_peak_bytes
            <= 16 * std::mem::size_of::<format::IndexEntry>() as u64
    );
    assert!(cache.statistics()?.file_opens > 1);
    Ok(())
}

#[test]
fn compaction_quota_denial_preserves_existing_packs_and_discards_private_work() -> Result<()> {
    let fixture = Fixture::new()?;
    let manifest = fixture.build(10)?;
    let path = pack_path(&fixture.collection, &manifest.packs[0].sha256)?;
    let before = fs::read(&path)?;
    let reservation = fixture
        .quota
        .reserve("fixture", PACK_HEADER_BYTES + record_bytes(2)? - 1)?;
    let staging = reservation.staging_directory()?;
    let cache = VectorCache::new(
        &fixture.collection,
        &reservation,
        &fixture.descriptor,
        Some(&manifest),
    )?;
    cache
        .lookup(&fixture.entry(0)?.key)?
        .context("fixture cache miss")?;
    assert!(cache.finalize().is_err());
    assert_eq!(fs::read(&path)?, before);
    assert_eq!(
        fs::read_dir(fixture.collection.join("objects/embedding-packs"))?.count(),
        1
    );
    drop(cache);
    drop(reservation);
    assert!(!staging.exists());
    verify_pack(&fixture.collection, &manifest.packs[0])?;
    Ok(())
}

#[test]
fn cumulative_compaction_read_limit_preserves_the_trusted_pack() -> Result<()> {
    let fixture = Fixture::new()?;
    let manifest = fixture.build(10)?;
    let path = pack_path(&fixture.collection, &manifest.packs[0].sha256)?;
    let before = fs::read(&path)?;
    let reservation = fixture.reserve()?;
    let cache = VectorCache::with_limits(
        &fixture.collection,
        &reservation,
        &fixture.descriptor,
        Some(&manifest),
        Limits {
            reads: 1200,
            ..Default::default()
        },
    )?;
    for number in [0, 1] {
        cache
            .lookup(&fixture.entry(number)?.key)?
            .context("fixture cache miss")?;
    }
    let error = cache.finalize().unwrap_err();
    assert!(format!("{error:#}").contains("embedding cache read budget exceeded"));
    assert!(cache.statistics()?.read_bytes <= 1200);
    assert_eq!(fs::read(&path)?, before);
    assert_eq!(
        fs::read_dir(fixture.collection.join("objects/embedding-packs"))?.count(),
        1
    );
    verify_pack(&fixture.collection, &manifest.packs[0])?;
    Ok(())
}

#[test]
fn quota_reference_read_and_index_bounds_fail_without_promoting_partial_packs() -> Result<()> {
    let fixture = Fixture::new()?;
    let small = fixture
        .quota
        .reserve("fixture", PACK_HEADER_BYTES + record_bytes(2)? - 1)?;
    let cache = VectorCache::new(&fixture.collection, &small, &fixture.descriptor, None)?;
    assert!(cache.store(&fixture.entry(0)?).is_err());
    assert!(cache.finalize().is_err());
    assert!(!fixture.collection.join("objects/embedding-packs").exists());
    let reservation = fixture.reserve()?;
    let cache = VectorCache::with_limits(
        &fixture.collection,
        &reservation,
        &fixture.descriptor,
        None,
        Limits {
            references: 1,
            ..Default::default()
        },
    )?;
    cache.store(&fixture.entry(0)?)?;
    assert!(cache.store(&fixture.entry(1)?).is_err());
    assert!(cache.finalize().is_err());
    let manifest = fixture.build(2)?;
    let reservation = fixture.reserve()?;
    let cache = VectorCache::with_limits(
        &fixture.collection,
        &reservation,
        &fixture.descriptor,
        Some(&manifest),
        Limits {
            reads: PACK_HEADER_BYTES + 2 * INDEX_ENTRY_BYTES - 1,
            ..Default::default()
        },
    )?;
    assert!(cache.lookup(&fixture.entry(0)?.key).is_err());
    assert!(cache.statistics()?.read_bytes < PACK_HEADER_BYTES + 2 * INDEX_ENTRY_BYTES);
    let reservation = fixture.reserve()?;
    let cache = VectorCache::with_limits(
        &fixture.collection,
        &reservation,
        &fixture.descriptor,
        Some(&manifest),
        Limits {
            index_bytes: std::mem::size_of::<format::IndexEntry>() as u64,
            ..Default::default()
        },
    )?;
    assert!(cache.lookup(&fixture.entry(0)?.key).is_err());
    Ok(())
}

#[test]
fn manifest_rejects_old_format_unowned_duplicate_and_noncanonical_references() -> Result<()> {
    let fixture = Fixture::new()?;
    let manifest = fixture.build(2)?;
    let error = VectorCacheManifest::from_bytes(br#"{"version":1,"references":[]}"#).unwrap_err();
    assert!(error.to_string().contains("airk index --full"));
    for kind in 0..6 {
        let mut changed = manifest.clone();
        match kind {
            0 => changed.references.push(changed.references[0].clone()),
            1 => changed.references[1].record = changed.references[0].record,
            2 => changed.references[0].record = changed.packs[0].records,
            3 => changed.references.clear(),
            4 => changed.packs[0].bytes += 1,
            _ => changed.packs[0].pipeline_digest = Sha256Digest::from_bytes([99; 32]),
        }
        assert!(changed.canonical_bytes().is_err(), "kind {kind}");
    }
    let mut bytes = manifest.canonical_bytes()?;
    bytes.push(b'\n');
    assert!(VectorCacheManifest::from_bytes(&bytes).is_err());
    assert!(VectorCacheManifest::from_bytes(&vec![0; MAX_CACHE_MANIFEST_BYTES + 1]).is_err());
    Ok(())
}

#[test]
fn collection_reservation_isolation_and_unfinished_staging_cleanup() -> Result<()> {
    let fixture = Fixture::new()?;
    let reservation = fixture.reserve()?;
    let stage = reservation.staging_directory()?;
    let other = fixture._root.path().join("collections-1.2/other");
    fs::create_dir_all(&other)?;
    assert!(VectorCache::new(&other, &reservation, &fixture.descriptor, None).is_err());
    let cache = VectorCache::new(&fixture.collection, &reservation, &fixture.descriptor, None)?;
    cache.store(&fixture.entry(0)?)?;
    drop(reservation);
    assert!(stage.exists());
    drop(cache);
    assert!(!stage.exists());
    assert!(!fixture.collection.join("objects/embedding-packs").exists());
    Ok(())
}

#[test]
fn crash_child() -> Result<()> {
    let Some(root) = std::env::var_os("AIRK_PACK_CRASH_ROOT") else {
        return Ok(());
    };
    let root = PathBuf::from(root);
    let collection = root.join("collections-1.2/fixture");
    let quota = Quota::open(&root)?;
    let reservation = quota.reserve("fixture", 1024 * 1024)?;
    let digest = Sha256Digest::from_bytes([1; 32]);
    let descriptor = EmbeddingDescriptor {
        version: 1,
        model: digest,
        tokenizer: digest,
        preprocessing: digest,
        pooling: digest,
        normalization: digest,
        inference: digest,
        dimensions: 1,
    };
    let cache = VectorCache::new(&collection, &reservation, &descriptor, None)?;
    cache.store(&CachedEmbedding::new(digest, vec![1.0], 1)?)?;
    std::process::exit(0);
}

#[test]
fn dead_owner_private_pack_is_reclaimed_without_deleting_published_files() -> Result<()> {
    let fixture = Fixture::new()?;
    let manifest = fixture.build(1)?;
    let output = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "vector_cache::tests::crash_child", "--nocapture"])
        .env("AIRK_PACK_CRASH_ROOT", fixture._root.path())
        .output()?;
    ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fs::read_dir(&fixture.collection)?.any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".quota-stage-")
    }));
    assert_eq!(fixture.quota.status()?.active_reservations, 0);
    assert!(!fs::read_dir(&fixture.collection)?.any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".quota-stage-")
    }));
    verify_pack(&fixture.collection, &manifest.packs[0])?;
    Ok(())
}
