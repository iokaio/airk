// SPDX-License-Identifier: Apache-2.0
//! Split authority wraps the existing descriptor-pinned physical setup engine.
use super::*;
use crate::service::registry::catalog::split::{NativeSplitPreparation, NativeSplitState};
use airk_core::{
    catalog_setup::{
        IndexSetupRequest, SetupSource, canonical_setup_sources, setup_index_id, setup_source_ids,
    },
    catalog_split::{SplitRunState, SplitSnapshot, SplitStage},
    index_catalog::CatalogRevision,
};

/// Physical initial bytes bind the immutable first intent, so a later attention
/// receipt or exact continuation can recognize its own interrupted shell.
pub(super) fn initial_seal_state(
    registry: &ServiceRegistry,
    current: &NativeSplitState,
) -> Result<NativeSplitState> {
    ensure!(
        registry
            .catalog_split_state(current.snapshot().receipt.operation_id)?
            .snapshot()
            == current.snapshot(),
        "split initial seal intent changed"
    );
    let (bytes,digest):(Vec<u8>,String)=registry.connection.query_row(
        "SELECT substr(payload,1,16385),digest FROM index_split_history WHERE operation_id=?1 AND revision=1",
        [current.snapshot().receipt.operation_id.to_string()],|row|Ok((row.get(0)?,row.get(1)?)))?;
    ensure!(
        bytes.len() <= airk_core::catalog_management::MAX_RECEIPT_BYTES,
        "initial split intent exceeds bound"
    );
    let receipt: airk_core::catalog_split::SplitReceipt = serde_json::from_slice(&bytes)?;
    receipt.validate_for(&current.snapshot().request)?;
    ensure!(
        receipt.revision == CatalogRevision::INITIAL
            && receipt.stage == SplitStage::Accepted
            && receipt.identity(&current.snapshot().request)?.to_string() == digest,
        "initial split intent identity changed"
    );
    let mut value = current.snapshot().clone();
    value.receipt_digest = receipt.identity(&value.request)?;
    value.receipt = receipt;
    value.finalization = None;
    value.validate()?;
    Ok(NativeSplitState(value))
}

/// This format-only context never becomes an ordinary saved setup operation.
fn format_context(state: &NativeSplitState) -> Result<IndexSetupReceipt> {
    let value = state.snapshot();
    value.validate()?;
    ensure!(
        value.receipt.stage == SplitStage::Accepted
            && matches!(
                value.receipt.state,
                SplitRunState::Pending | SplitRunState::Running
            ),
        "split storage requires active Accepted metadata intent"
    );
    let split = &value.request;
    let request = IndexSetupRequest {
        caller: split.caller,
        key: split.key.clone(),
        expected_catalog_revision: split.expected_catalog_revision,
        name: split.destination_name.clone(),
        registration: split.destination_registration.clone(),
        sources: canonical_setup_sources(
            &value
                .plan
                .destination
                .roots
                .iter()
                .map(|root| SetupSource {
                    canonical_path: root.canonical_path.clone(),
                    aliases: root.aliases.clone(),
                    owner: root.owner,
                })
                .collect::<Vec<_>>(),
        )?,
        setup_watch: false,
    };
    let receipt = IndexSetupReceipt {
        schema: 1,
        operation_id: split.operation_id()?,
        request_digest: request.identity()?,
        index_id: setup_index_id(&request)?,
        source_ids: setup_source_ids(&request)?,
        stage: SetupStage::Reserved,
        catalog_revision: request.expected_catalog_revision.next()?,
        source_revision: CatalogRevision::INITIAL,
        preparation: None,
        completion: None,
        request,
    };
    receipt.validate()?;
    Ok(receipt)
}

pub struct PreparedSplitStorage {
    storage: PreparedStorage,
}
impl PreparedSplitStorage {
    pub fn record(self, registry: &mut ServiceRegistry) -> Result<SplitSnapshot> {
        let storage = self.storage;
        ensure!(
            registry.root.canonicalize()? == storage.control,
            "foreign split setup registry"
        );
        storage.check_final()?;
        let state = storage
            .split
            .as_ref()
            .context("split setup lost native intent")?;
        ensure!(
            registry
                .catalog_split_state(state.snapshot().receipt.operation_id)?
                .snapshot()
                == state.snapshot(),
            "split setup intent changed before record"
        );
        crate::service::registry::catalog::split::require_destination_registration(
            &registry.connection,
            &storage.receipt.request.registration,
            state,
        )?;
        let metadata = Metadata::new_split(
            &storage.control,
            &storage.receipt,
            storage
                .split_seal
                .as_ref()
                .context("split setup lost immutable seal")?,
        )?;
        let capsule = metadata
            .split_capsule
            .as_ref()
            .context("split setup has no restrictive owner capsule")?;
        let identity = framed_digest(
            b"airk/split-pinned-prepared-storage/1",
            &[
                state.snapshot().receipt.request_digest.as_bytes(),
                &metadata.digest(&storage.receipt.request)?.as_bytes()[..],
                &storage.root.identity()?.0.to_be_bytes(),
                &storage.root.identity()?.1.to_be_bytes(),
                &storage.collection.identity()?.0.to_be_bytes(),
                &storage.collection.identity()?.1.to_be_bytes(),
                &storage.slot.identity()?.0.to_be_bytes(),
                &storage.slot.identity()?.1.to_be_bytes(),
                &capsule.1,
            ],
        );
        registry.catalog_record_split_prepared(NativeSplitPreparation::new(state, identity))
    }
}

pub fn prepare_split_storage(
    registry: &ServiceRegistry,
    control: &Path,
    state: &NativeSplitState,
    pins: NativeSetupPins,
) -> Result<PreparedSplitStorage> {
    let context = format_context(state)?;
    let storage = prepare_authorized(registry, control, &context, pins, Some(state), || {})?;
    Ok(PreparedSplitStorage { storage })
}
#[cfg(test)]
pub(in crate::service::registry::catalog) fn prepare_split_test_storage(
    registry: &ServiceRegistry,
    state: &NativeSplitState,
) -> Result<PreparedSplitStorage> {
    let context = format_context(state)?;
    let data = Path::new(&context.request.registration.data_root);
    let pins = NativeSetupPins::from_native_review(
        SetupDirectory::open_canonical(data.parent().context("test setup parent missing")?)?.file,
        if data.exists() {
            Some(SetupDirectory::open_canonical(data)?.file)
        } else {
            None
        },
        context
            .request
            .sources
            .iter()
            .map(|source| {
                Ok(SetupDirectory::open_canonical(Path::new(&source.canonical_path))?.file)
            })
            .collect::<Result<Vec<_>>>()?,
    );
    prepare_split_storage(registry, &registry.root, state, pins)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pins(context: &IndexSetupReceipt) -> Result<NativeSetupPins> {
        let data = Path::new(&context.request.registration.data_root);
        Ok(NativeSetupPins::from_native_review(
            SetupDirectory::open_canonical(data.parent().unwrap())?.file,
            if data.exists() {
                Some(SetupDirectory::open_canonical(data)?.file)
            } else {
                None
            },
            context
                .request
                .sources
                .iter()
                .map(|source| {
                    Ok(SetupDirectory::open_canonical(Path::new(&source.canonical_path))?.file)
                })
                .collect::<Result<Vec<_>>>()?,
        ))
    }
    #[test]
    fn native_split_preparation_stays_sealed_and_never_creates_ordinary_setup_or_jobs() -> Result<()>
    {
        let (_temporary, mut registry, request, baseline, plan) =
            crate::service::registry::catalog::split::tests::fixture()?;
        let original =
            std::fs::read(Path::new(&request.original_registration.root).join("original.md"))?;
        let accepted = registry.catalog_reserve_split(&request, &baseline, &plan)?;
        registry
            .catalog_reserve_split_destination(request.operation_id()?, accepted.receipt_digest)?;
        let state = registry.catalog_split_state(request.operation_id()?)?;
        let context = format_context(&state)?;
        let control = registry.root.clone();
        let prepared = prepare_split_storage(&registry, &control, &state, pins(&context)?)?;
        let recorded = prepared.record(&mut registry)?;
        assert_eq!(recorded.receipt.stage, SplitStage::Accepted);
        assert!(
            recorded.receipt.destination_storage.is_some()
                && recorded.receipt.capacity_receipt.is_none()
        );
        assert!(
            registry
                .index_setup_receipt(request.caller, &request.key)?
                .is_none()
        );
        assert!(registry.catalog_detail(&airk_core::catalog_management::CatalogEntity::Index{id:request.destination.index_id},None)?.item
            .is_some_and(|item|matches!(item,airk_core::catalog_management::CatalogItem::Index(index) if index.source_import==airk_core::index_catalog::SourceImportState::PendingMetadata)));
        let jobs: i64 =
            registry
                .connection
                .query_row("SELECT count(*) FROM service_jobs", [], |row| row.get(0))?;
        assert_eq!(jobs, 0);
        let collection = crate::service::collection::Collection::existing(
            Path::new(&request.destination_registration.data_root),
            &request.destination_registration.root,
            &request.destination_registration.collection,
        )?;
        assert!(collection.select().is_err());
        let selected = collection.select_for_diagnostics()?;
        assert!(!selected.directory().join("current.sqlite3").exists());
        assert_eq!(
            std::fs::read(Path::new(&request.original_registration.root).join("original.md"))?,
            original
        );
        Ok(())
    }
    #[test]
    fn native_split_preparation_resumes_its_immutable_seal_after_metadata_attention() -> Result<()>
    {
        let (_temporary, mut registry, request, baseline, plan) =
            crate::service::registry::catalog::split::tests::fixture()?;
        let accepted = registry.catalog_reserve_split(&request, &baseline, &plan)?;
        registry
            .catalog_reserve_split_destination(request.operation_id()?, accepted.receipt_digest)?;
        let first = registry.catalog_split_state(request.operation_id()?)?;
        let context = format_context(&first)?;
        let control = registry.root.clone();
        drop(prepare_split_storage(
            &registry,
            &control,
            &first,
            pins(&context)?,
        )?);
        let directory = Path::new(&request.destination_registration.data_root)
            .join("collections-1.4")
            .join(&request.destination_registration.collection);
        let owner_before = std::fs::read(directory.join("service-owner.json"))?;
        let failed = registry.catalog_note_split_failure(
            request.operation_id()?,
            first.snapshot().receipt_digest,
            "test_metadata_interruption",
        )?;
        let recovery = airk_core::catalog_split::SplitRecoveryRequest {
            caller: request.caller,
            key: airk_core::service::RequestKey {
                nonce: Incarnation::from_bytes([57; 16]),
                ..request.key.clone()
            },
            operation_id: request.operation_id()?,
            request_digest: request.identity()?,
            expected_receipt_digest: failed.receipt_digest,
            expected_finalization_revision: None,
            action: airk_core::catalog_split::SplitRecoveryAction::Resume,
        };
        registry.catalog_resume_split_continuation(&recovery)?;
        let next = registry.catalog_split_state(request.operation_id()?)?;
        assert!(next.snapshot().receipt.revision > first.snapshot().receipt.revision);
        let context = format_context(&next)?;
        let prepared = prepare_split_storage(&registry, &control, &next, pins(&context)?)?;
        prepared.record(&mut registry)?;
        assert_eq!(
            std::fs::read(directory.join("service-owner.json"))?,
            owner_before
        );
        assert!(
            registry
                .catalog_split_status(request.operation_id()?)?
                .unwrap()
                .receipt
                .destination_storage
                .is_some()
        );
        assert!(
            registry
                .index_setup_receipt(request.caller, &request.key)?
                .is_none()
        );
        Ok(())
    }
    #[cfg(unix)]
    #[test]
    fn native_split_preparation_redirect_after_review_cannot_record_or_touch_source_bytes()
    -> Result<()> {
        let (temporary, mut registry, request, baseline, plan) =
            crate::service::registry::catalog::split::tests::fixture()?;
        let accepted = registry.catalog_reserve_split(&request, &baseline, &plan)?;
        registry
            .catalog_reserve_split_destination(request.operation_id()?, accepted.receipt_digest)?;
        let state = registry.catalog_split_state(request.operation_id()?)?;
        let context = format_context(&state)?;
        let captured = pins(&context)?;
        let root = Path::new(&request.original_registration.root);
        let retained = temporary.path().join("retained-source");
        let control = registry.root.clone();
        let result = prepare_authorized(
            &registry,
            &control,
            &context,
            captured,
            Some(&state),
            || {
                std::fs::rename(root, &retained).unwrap();
                std::fs::create_dir(root).unwrap();
                std::fs::write(
                    root.join("original.md"),
                    b"redirected source remains unchanged",
                )
                .unwrap();
            },
        );
        assert!(result.is_err());
        assert!(
            registry
                .catalog_split_status(request.operation_id()?)?
                .unwrap()
                .receipt
                .destination_storage
                .is_none()
        );
        assert_eq!(
            std::fs::read(root.join("original.md"))?,
            b"redirected source remains unchanged"
        );
        assert_eq!(
            std::fs::read(retained.join("original.md"))?,
            b"original remainder"
        );
        assert!(
            registry
                .index_setup_receipt(request.caller, &request.key)?
                .is_none()
        );
        Ok(())
    }
    #[test]
    fn anchored_owner_replace_refuses_changed_target_then_publishes_exact_owned_file() -> Result<()>
    {
        let temporary = tempfile::tempdir()?;
        let path = temporary.path().canonicalize()?;
        let pin = SetupDirectory::open_canonical(&path)?;
        let original = pin.create_file(OsStr::new("service-owner.json"), b"old owner")?;
        let prepared = pin.create_file(OsStr::new("prepared-owner"), b"new owner")?;
        let foreign = pin.create_file(OsStr::new("foreign"), b"unrelated metadata")?;
        assert!(
            pin.replace_file(
                OsStr::new("prepared-owner"),
                &prepared,
                OsStr::new("service-owner.json"),
                &foreign
            )
            .is_err()
        );
        assert_eq!(
            pin.read_file(OsStr::new("service-owner.json"), 64)?,
            b"old owner"
        );
        assert_eq!(
            pin.read_file(OsStr::new("prepared-owner"), 64)?,
            b"new owner"
        );
        pin.replace_file(
            OsStr::new("prepared-owner"),
            &prepared,
            OsStr::new("service-owner.json"),
            &original,
        )?;
        assert_eq!(
            pin.read_file(OsStr::new("service-owner.json"), 64)?,
            b"new owner"
        );
        assert_eq!(
            pin.read_file(OsStr::new("foreign"), 64)?,
            b"unrelated metadata"
        );
        Ok(())
    }
    #[cfg(unix)]
    #[test]
    fn anchored_replace_never_reopens_a_rebound_logical_parent() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let base = temporary.path().canonicalize()?;
        let named = base.join("owned");
        std::fs::create_dir(&named)?;
        let pin = SetupDirectory::open_canonical(&named)?;
        let original = pin.create_file(OsStr::new("owner"), b"old")?;
        let prepared = pin.create_file(OsStr::new("prepared"), b"new")?;
        let retained = base.join("retained");
        std::fs::rename(&named, &retained)?;
        std::fs::create_dir(&named)?;
        std::fs::write(named.join("owner"), b"foreign source")?;
        pin.replace_file(
            OsStr::new("prepared"),
            &prepared,
            OsStr::new("owner"),
            &original,
        )?;
        assert_eq!(std::fs::read(named.join("owner"))?, b"foreign source");
        assert_eq!(std::fs::read(retained.join("owner"))?, b"new");
        Ok(())
    }
}
