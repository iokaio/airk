// SPDX-License-Identifier: Apache-2.0
//! PRIVATE draft; mount as CHILD ann_self::regenerate, preserving original ProbeInput owner.
//! No new native DTO, scanner, source extraction, old graph edge copy or direct SQL.
use super::*;
use airk_core::embedding::{
    CachedEmbedding, EmbeddingCache, EmbeddingDescriptor, lookup_embedding,
};
use airk_retrieval::model::{EmbeddingCacheContext, EmbeddingCacheStats, EmbeddingModel};
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

struct StrictOriginalCache {
    entries: BTreeMap<Sha256Digest, CachedEmbedding>,
    dimensions: u32,
    lookups: AtomicU64,
    order: Mutex<Vec<Sha256Digest>>,
    live: Arc<dyn Fn() -> Result<()> + Send + Sync>,
}
impl EmbeddingCache for StrictOriginalCache {
    fn lookup(&self, key: &Sha256Digest) -> Result<Option<CachedEmbedding>> {
        (self.live)()?;
        let entry = self
            .entries
            .get(key)
            .context("ZERO-NN regeneration unknown embedding key")?;
        entry.validate(key, self.dimensions)?;
        self.lookups.fetch_add(1, Ordering::Relaxed);
        self.order
            .lock()
            .map_err(|_| anyhow::anyhow!("cache order poisoned"))?
            .push(*key);
        Ok(Some(entry.clone()))
    }
    fn store(&self, _: &CachedEmbedding) -> Result<()> {
        anyhow::bail!("ZERO-NN regeneration must never store a computed embedding")
    }
}

fn fresh_output(input: &ProbeInput, destination: &Path) -> Result<()> {
    ensure!(
        destination.is_absolute() && !destination.try_exists()?,
        "fresh absolute output required"
    );
    let parent = destination
        .parent()
        .context("output parent missing")?
        .canonicalize()?;
    ensure!(
        parent == destination.parent().unwrap(),
        "canonical output parent required"
    );
    for frozen in [&input.source_root, &input.data_root] {
        let original = frozen.canonicalize()?;
        ensure!(
            !parent.starts_with(&original) && !original.starts_with(destination),
            "fresh output overlaps frozen source/state"
        );
    }
    ensure!(
        !destination.starts_with(
            input
                .retained_capture
                .parent()
                .context("capture parent missing")?
        ),
        "fresh output overlaps retained capture inputs"
    );
    Ok(())
}

fn operation_isolation(
    anchor: &Path,
    output: &Path,
    control: &Path,
    frozen: &[PathBuf],
) -> Result<()> {
    ensure!(
        anchor.is_dir() && anchor.canonicalize()? == anchor,
        "canonical existing operation anchor required"
    );
    ensure!(
        output == anchor.join("result")
            && !output.try_exists()?
            && control == anchor.join("control")
            && control.is_dir()
            && control.canonicalize()? == control,
        "only exact distinct result/control children of operation anchor are allowed"
    );
    for protected in frozen {
        let protected = protected.canonicalize()?;
        ensure!(
            !anchor.starts_with(&protected) && !protected.starts_with(anchor),
            "operation anchor overlaps frozen input/model/source/state tree"
        );
    }
    for entry in std::fs::read_dir(anchor)? {
        ensure!(
            entry?.path() == control,
            "fresh operation anchor contains an unexpected entry"
        );
    }
    Ok(())
}

fn exact_frozen_old_spec(pipeline: Sha256Digest) -> Sha256Digest {
    // Exact original recipe from the retained specification1 producer. This
    // private import accepts ONE frozen old recipe, never all historical graphs.
    let mut digest = Sha256::new();
    digest.update(b"airk-mutable-diskann-1\0diskann-0.59.0\0le-f32-adjacency-48\0l2\0pruned-degree-32\0build-list-64\0triangle-prune\0");
    digest.update(384_u32.to_be_bytes());
    digest.update(pipeline.as_bytes());
    Sha256Digest::from_bytes(digest.finalize().into())
}

fn source_closure(input: &ProbeInput, deadline: Instant) -> Result<Sha256Digest> {
    fn walk(
        root: &Path,
        directory: &Path,
        paths: &mut BTreeSet<String>,
        depth: usize,
    ) -> Result<()> {
        ensure!(depth <= 16, "bounded source tree required");
        for entry in std::fs::read_dir(directory)? {
            let path = entry?.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            ensure!(!metadata.file_type().is_symlink(), "source aliases refused");
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    metadata.file_attributes() & 0x400 == 0,
                    "source reparse point refused"
                );
            }
            if metadata.is_dir() {
                walk(root, &path, paths, depth + 1)?;
            } else {
                ensure!(
                    metadata.is_file() && paths.len() < 366,
                    "foreign source entry or roster overflow"
                );
                paths.insert(
                    path.strip_prefix(root)?
                        .to_str()
                        .context("source path not UTF8")?
                        .replace('\\', "/"),
                );
            }
        }
        Ok(())
    }
    let mut paths = BTreeSet::new();
    walk(&input.source_root, &input.source_root, &mut paths, 0)?;
    ensure!(
        paths == input.expected_indexed_sources.keys().cloned().collect(),
        "closed source roster changed"
    );
    for (path, (bytes, hash)) in &input.expected_indexed_sources {
        ensure!(
            Instant::now() < deadline && *bytes <= 4096,
            "original source bound exceeded"
        );
        let actual = std::fs::read(input.source_root.join(path))?;
        ensure!(
            actual.len() as u64 == *bytes && format!("{:x}", Sha256::digest(actual)) == *hash,
            "original live source bytes changed"
        );
    }
    Ok(Sha256Digest::from_bytes(
        Sha256::digest(serde_json::to_vec(&input.expected_indexed_sources)?).into(),
    ))
}

#[test]
#[ignore = "PRIMARY REVIEW REQUIRED; exact old DEV vectors, fresh native state, zero NN; owned120s/2GiB"]
fn retained_dev_graph_regenerates_from_only_authenticated_exact_cached_embeddings() -> Result<()> {
    let started = Instant::now();
    let deadline = started + Duration::from_secs(120);
    let path = PathBuf::from(
        std::env::var_os("AIRK_ANN_SELF_INPUT").context("missing original native input")?,
    );
    let pin: Sha256Digest =
        "850afafe8286a626c47709449da86e12e77052e06f5a65c710ee9b4bad23af19".parse()?;
    let input: ProbeInput = serde_json::from_slice(&sealed(&path, pin)?)?;
    sealed(&input.retained_capture, input.retained_capture_sha256)?;
    ensure!(
        input.expected_indexed_sources.len() == 366
            && input.expected_live_nodes == 378
            && input.expected_head.graph_head.allocated_count == 379,
        "original DEV population required"
    );
    let output = PathBuf::from(
        std::env::var_os("AIRK_ANN_REGENERATE_OUTPUT").context("missing fresh private output")?,
    );
    fresh_output(&input, &output)?;
    let source_before = source_closure(&input, deadline)?;
    let old = crate::Kernel::open_collection_for_diagnostics(
        &input.source_root,
        Some(input.data_root.clone()),
        &input.collection,
    )?;
    let old_read = old.current_reader()?;
    ensure!(
        old_read.head() == &input.expected_head,
        "original head changed"
    );
    let original_token = old_read.token()?;
    let configuration = old_read
        .collection_state()?
        .context("original configuration missing")?
        .configuration;
    ensure!(
        configuration.model.as_ref() == Some(&input.expected_model),
        "original model/pipeline changed"
    );
    let snapshot = old.load_optional()?.context("original snapshot missing")?;
    let original_roster: BTreeMap<_, _> = snapshot
        .artifacts
        .iter()
        .map(|(path, artifact)| {
            (
                path.clone(),
                (artifact.bytes as u64, artifact.content_hash.clone()),
            )
        })
        .collect();
    ensure!(
        original_roster == input.expected_indexed_sources,
        "original source roster changed"
    );
    let descriptor = crate::shared_models::descriptor(&old.store, &input.expected_model)?;
    ensure!(
        descriptor.dimensions == 384
            && descriptor.identity()? == input.expected_model.pipeline_digest,
        "exact model descriptor required"
    );
    ensure!(
        input.expected_head.graph_head.scope.specification
            == exact_frozen_old_spec(input.expected_model.pipeline_digest)
            && input.expected_head.graph_head.scope.record_bytes
                == airk_retrieval::vector::mutable::record_bytes(384)?,
        "only frozen specification1 recipe and unchanged record codec may be imported"
    );
    let entries =
        airk_retrieval::vector::mutable::self_probe::regeneration_cache::authenticated_embeddings(
            old_read.clone(),
            384,
            379,
            378,
            deadline,
        )?;
    for (id, entry) in &entries {
        let binding = old_read
            .vector_binding(entry.key)?
            .context("original key has no authenticated map binding")?;
        ensure!(binding.graph_id == *id, "original key/id map differs");
    }
    // A new policy/specification is mandatory: never relabel old graph bytes.
    let new_spec =
        airk_retrieval::vector::mutable::specification(384, input.expected_model.pipeline_digest)?;
    ensure!(
        new_spec != input.expected_head.graph_head.scope.specification,
        "regeneration requires reviewed graph-spec bump, not unchanged failed inputs"
    );
    let fence_old = old.clone();
    let expected_head = input.expected_head.clone();
    let cache = StrictOriginalCache {
        entries: entries
            .iter()
            .map(|(_, entry)| (entry.key, entry.clone()))
            .collect(),
        dimensions: 384,
        lookups: AtomicU64::new(0),
        order: Mutex::new(Vec::new()),
        live: Arc::new(move || {
            ensure!(
                Instant::now() < deadline,
                "original regeneration deadline exceeded"
            );
            ensure!(
                fence_old.current_reader()?.head() == &expected_head,
                "original publication changed"
            );
            Ok(())
        }),
    };
    // Owned source/model assets are imported through existing owner APIs.
    // The reviewed exact model directory is independently pinned by ModelObjectSet.
    let model_directory = PathBuf::from(
        std::env::var_os("AIRK_ANN_REGENERATE_MODEL_DIRECTORY")
            .context("missing exact original candidate asset directory")?,
    )
    .canonicalize()?;
    let model_json = input
        .expected_model
        .assets
        .get(&airk_core::model_objects::ModelAssetName::ModelJson)
        .context("model manifest missing")?;
    let control = PathBuf::from(
        std::env::var_os("AIRK_ANN_REGENERATE_CONTROL")
            .context("explicit private owned control root required")?,
    )
    .canonicalize()?;
    let anchor = PathBuf::from(
        std::env::var_os("AIRK_ANN_REGENERATE_ANCHOR")
            .context("designated private operation anchor required")?,
    )
    .canonicalize()?;
    operation_isolation(
        &anchor,
        &output,
        &control,
        &[
            input.source_root.clone(),
            input.data_root.clone(),
            input
                .retained_capture
                .parent()
                .context("capture parent missing")?
                .into(),
            model_directory.clone(),
            path.parent().context("native input parent missing")?.into(),
        ],
    )?;
    let lease = Arc::new(airk_store::service::ServiceLease::acquire(&control)?);
    let owner = NativeInferenceOwner::admit(lease)?;
    let result = owner.run_encoder_evaluation(model_json.sha256, || -> Result<Value> {
        ensure!(airk_retrieval::model::inspect_object_plan(&model_directory)?.local_set()
            == input.expected_model.local_set(), "provisioned assets differ from original model");
        std::fs::create_dir(&output)?;
        let fresh = crate::Kernel::open_collection(&input.source_root,
            Some(output.join("data").join("visible-prefix-mining-r1-u8").join("index-0")), &input.collection)?;
        ensure!(fresh.current_scope()?.is_none(), "fresh state is already initialized");
        fresh.configure_vectors(true)?;
        fresh.install_default_vector_model(&model_directory)?;
        ensure!(fresh.vector_selection()?.digest()? == configuration.vector_selection.digest()?,
            "fresh desired vector selection differs from original");
        fresh.initialize()?; // Empty native bootstrap/model load only: no source scan or forwards.
        let scope = fresh.current_scope()?.context("fresh initialized scope missing")?;
        ensure!(scope.incarnation != input.expected_head.scope.incarnation, "new incarnation required");
        let quota = fresh.collection_quota()?;
        let writer = fresh.store.lock()?;
        let reservation = fresh.publication_reservation()?;
        // initialize_current already retained these exact assets through ModelObjects.
        let model = EmbeddingModel::open_objects(fresh.store.directory(), &input.expected_model)?;
        ensure!(model.cache_descriptor()? == descriptor, "planner/model descriptor changed");
        let mut changes = Vec::new();
        for path in input.expected_indexed_sources.keys() {
            (cache.live)()?;
            let original = old_read.captured_input(path)?.context("original captured input missing")?;
            let retained = old.store.load_retrieval_artifact(&original.artifact)?;
            let source = old.store.read_retained_source(&original.artifact)?;
            let artifact = fresh.store.retain_payload_with_budget_owned(&writer, &retained.artifact,
                &retained.payload, &retained.profile, Some(&source), Some(&reservation))?;
            let copied = fresh.store.load_retrieval_artifact(&artifact)?;
            ensure!(copied.payload_digest == retained.payload_digest
                && copied.profile == retained.profile
                && serde_json::to_vec(&copied.payload)? == serde_json::to_vec(&retained.payload)?,
                "owning retained payload/profile import changed bytes");
            let mut captured = original;
            captured.artifact = artifact;
            changes.push(airk_store::current::ArtifactChange::Upsert(Box::new(captured)));
        }
        let mut stats = EmbeddingCacheStats::default();
        let update = crate::current::CurrentUpdater {
            store: &fresh.store, quota: &quota, scope: &scope, writer: &writer,
            reservation: &reservation, configuration: Some(&configuration),
        }.apply(&changes, Some(crate::current::SemanticUpdate {
            model: &model, objects: input.expected_model.clone(), selection: &configuration.vector_selection,
            defer: false, cache: EmbeddingCacheContext {
                cache: Some(&cache), bypass: false, stats: &mut stats,
                max_workers: 1, working_memory_bytes: 256 << 20,
            },
        }))?;
        ensure!(stats.inferences == 0 && stats.actual_forwards == 0 && stats.misses == 0,
            "ZERO-NN reconstruction observed a miss or inference");
        ensure!(update.commit.maintenance_error.is_none(), "fresh publication maintenance incomplete");
        drop(writer);
        drop(reservation);
        let new_read = fresh.current_reader()?;
        ensure!(new_read.head().graph_head.scope.specification == new_spec
            && new_read.head().graph_head.scope.graph_instance != input.expected_head.graph_head.scope.graph_instance
            && new_read.head().graph_head.live_nodes == 378
            && new_read.head().graph_head.allocated_count == 379, "new graph identity/population mismatch");
        let new_entries = airk_retrieval::vector::mutable::self_probe::regeneration_cache::authenticated_embeddings(
            new_read.clone(), 384, 379, 378, deadline)?;
        ensure!(entries.len() == new_entries.len(), "new vector census mismatch");
        for ((old_id, original), (new_id, regenerated)) in entries.iter().zip(&new_entries) {
            ensure!(old_id == new_id && original.key == regenerated.key
                && original.vector.iter().map(|v| v.to_bits()).eq(regenerated.vector.iter().map(|v| v.to_bits())),
                "regeneration changed original vector bytes or insertion order");
            ensure!(new_read.vector_binding(original.key)?.context("new key map missing")?.graph_id == *new_id,
                "new vector map differs");
        }
        let current = fresh.load_optional()?.context("new source roster absent")?;
        let new_roster: BTreeMap<_, _> = current.artifacts.iter().map(|(path, artifact)|
            (path.clone(), (artifact.bytes as u64, artifact.content_hash.clone()))).collect();
        ensure!(new_roster == original_roster, "new source roster differs");
        (cache.live)()?;
        ensure!(old.current_reader()?.head() == &input.expected_head, "old state changed");
        let mut output_input: ProbeInput = serde_json::from_slice(&serde_json::to_vec(&input)?)?;
        output_input.data_root = fresh.data_root.canonicalize()?;
        output_input.expected_head = new_read.head().clone();
        let mut file = OpenOptions::new().create_new(true).write(true).open(output.join("regenerated-native-input.json"))?;
        file.write_all(&serde_json::to_vec_pretty(&output_input)?)?; file.sync_all()?;
        Ok(json!({"new_head":new_read.head(),"new_token":new_read.token()?,"new_scope":scope,
            "vectors":378,"sources":366,"cache_lookups":cache.lookups.load(Ordering::Relaxed),
            "embedding_actual_forwards":stats.actual_forwards,"embedding_inferences":stats.inferences,
            "exact_original_vector_bits_and_order":true,"new_data_root":output_input.data_root}))
    })??;
    old_read.require_token(&original_token)?;
    ensure!(
        old.current_reader()?.head() == &input.expected_head,
        "old publication changed before delivery"
    );
    sealed(&path, pin)?;
    sealed(&input.retained_capture, input.retained_capture_sha256)?;
    ensure!(
        source_closure(&input, deadline)? == source_before,
        "source closure changed during regeneration"
    );
    ensure!(
        Instant::now() < deadline,
        "original regeneration deadline exceeded"
    );
    let result = json!({"schema":"airk.private-ann-regeneration/1","complete":true,"evaluation":true,
        "release_qualified":false,"original_input_sha256":pin,"result":result,
        "elapsed_seconds":started.elapsed().as_secs_f64(),"query_execution":false,
        "quality_claim":false,"binary_sha256":format!("{:x}",Sha256::digest(std::fs::read(std::env::current_exe()?)?))});
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output.join("receipt.json"))?;
    file.write_all(&serde_json::to_vec_pretty(&result)?)?;
    file.sync_all()?;
    Ok(())
}

#[test]
fn strict_original_embedding_cache_refuses_unknown_tampered_and_revoked_before_forward()
-> Result<()> {
    use airk_core::embedding::embedding_key;
    use std::sync::atomic::AtomicBool;
    let one = Sha256Digest::from_bytes([1; 32]);
    let descriptor = EmbeddingDescriptor {
        version: 1,
        model: one,
        tokenizer: one,
        preprocessing: one,
        pooling: one,
        normalization: one,
        inference: one,
        dimensions: 2,
    };
    let key = embedding_key(&descriptor, b"alpha")?;
    let entry = CachedEmbedding::new(key, vec![1.0, 0.0], 2)?;
    let live = Arc::new(AtomicBool::new(true));
    let guard = live.clone();
    let mut cache = StrictOriginalCache {
        entries: BTreeMap::from([(key, entry)]),
        dimensions: 2,
        lookups: AtomicU64::new(0),
        order: Mutex::new(Vec::new()),
        live: Arc::new(move || {
            ensure!(guard.load(Ordering::SeqCst), "revoked original publication");
            Ok(())
        }),
    };
    assert_eq!(
        lookup_embedding(&cache, &descriptor, b"alpha")?,
        Some(vec![1.0, 0.0])
    );
    // Inference sentinel executes only on Ok(None). Err must propagate first.
    let forwards = AtomicU64::new(0);
    let unknown = (|| -> Result<()> {
        if lookup_embedding(&cache, &descriptor, b"beta")?.is_none() {
            forwards.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    })();
    assert!(unknown.is_err());
    assert_eq!(forwards.load(Ordering::SeqCst), 0);
    cache.entries.get_mut(&key).unwrap().vector[0] = 0.5;
    assert!(lookup_embedding(&cache, &descriptor, b"alpha").is_err());
    cache
        .entries
        .insert(key, CachedEmbedding::new(key, vec![1.0, 0.0], 2)?);
    live.store(false, Ordering::SeqCst);
    assert!(lookup_embedding(&cache, &descriptor, b"alpha").is_err());
    assert!(cache.store(cache.entries.get(&key).unwrap()).is_err());
    assert_eq!(forwards.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn regeneration_operation_anchor_rejects_every_frozen_tree_before_creating_output() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path().canonicalize()?;
    let mut protected = Vec::new();
    for name in ["source", "old-state", "capture", "model", "native-input"] {
        let path = root.join(name);
        std::fs::create_dir(&path)?;
        protected.push(path.canonicalize()?);
    }
    for path in &protected {
        let control = path.join("control");
        std::fs::create_dir(&control)?;
        assert!(operation_isolation(path, &path.join("result"), &control, &protected).is_err());
        assert!(!path.join("result").exists());
        let nested = path.join("nested");
        std::fs::create_dir(&nested)?;
        let nested_control = nested.join("control");
        std::fs::create_dir(&nested_control)?;
        assert!(
            operation_isolation(&nested, &nested.join("result"), &nested_control, &protected)
                .is_err()
        );
        assert!(!nested.join("result").exists());
    }
    let anchor = root.join("new-operation");
    std::fs::create_dir(&anchor)?;
    let control = anchor.join("control");
    std::fs::create_dir(&control)?;
    operation_isolation(&anchor, &anchor.join("result"), &control, &protected)?;
    assert!(!anchor.join("result").exists());
    assert!(
        operation_isolation(
            &anchor,
            &anchor.join("result"),
            &protected[0].join("control"),
            &protected
        )
        .is_err()
    );
    assert!(operation_isolation(&anchor, &anchor.join("other"), &control, &protected).is_err());
    assert!(
        operation_isolation(
            &anchor,
            &anchor.join("..").join("source").join("result"),
            &control,
            &protected
        )
        .is_err()
    );
    // Canonical Windows extended-path aliases resolve to the same frozen tree.
    let alias = protected[0].join("nested").join("..").canonicalize()?;
    assert!(
        operation_isolation(
            &alias,
            &alias.join("result"),
            &alias.join("control"),
            &protected
        )
        .is_err()
    );
    // A parent of frozen data also cannot be an operation anchor.
    std::fs::create_dir(root.join("control"))?;
    assert!(
        operation_isolation(
            &root,
            &root.join("result"),
            &root.join("control"),
            &protected
        )
        .is_err()
    );
    Ok(())
}
