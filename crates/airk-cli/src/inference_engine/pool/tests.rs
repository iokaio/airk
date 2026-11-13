// SPDX-License-Identifier: Apache-2.0
use super::*;
use sha2::{Digest, Sha256};

pub(crate) fn fixture(
    root: &Path,
    label: &str,
    name: &str,
) -> Result<(protocol::Grant, ModelLocation, String)> {
    let root = root.canonicalize()?;
    let source = root.join(format!("source-{label}"));
    let data = root.join(format!("data-{label}"));
    let model = data.join("models/local");
    std::fs::create_dir_all(&source)?;
    std::fs::create_dir_all(&model)?;
    let vocabulary = br#"{"cobalt":[1.0,0.0],"harbor":[0.0,1.0]}"#;
    let identity = format!("{:x}", Sha256::digest(vocabulary));
    let manifest = airk_retrieval::vector::ModelManifest {
        schema: 1,
        name: name.into(),
        license: "Apache-2.0".into(),
        sha256: identity.clone(),
        dimensions: 2,
    };
    std::fs::write(model.join("model.json"), serde_json::to_vec(&manifest)?)?;
    std::fs::write(model.join("vocabulary.json"), vocabulary)?;
    let project = airk_kernel::service_projects::registration(&source, &data, label)?;
    Ok((
        protocol::Grant {
            service: Incarnation::from_bytes([1; 16]),
            pool: Incarnation::from_bytes([2; 16]),
            project,
            worker: airk_kernel::service_broker::new_instance_id()?,
            token: Sha256Digest::from_bytes([3; 32]),
        },
        ModelLocation::Directory {
            path: model.canonicalize()?,
        },
        identity,
    ))
}

pub(crate) fn open(location: ModelLocation, identity: String) -> Request {
    Request::Open {
        protocol: PROTOCOL,
        version: env!("CARGO_PKG_VERSION").into(),
        location,
        identity,
    }
}

#[test]
fn registration_replaces_live_authority_at_capacity_without_revoking_siblings() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (template, _, _) = fixture(root.path(), "capacity", "unused fixture")?;
    let now = Instant::now();
    let mut grants = BTreeMap::new();
    for member in 1u8..=32 {
        let grant = protocol::Grant {
            worker: Incarnation::from_bytes([member; 16]),
            token: Sha256Digest::from_bytes([member; 32]),
            ..template.clone()
        };
        assert!(matches!(
            register_authorization(&mut grants, template.service, template.pool, grant, now),
            protocol::Observation::Registered
        ));
    }
    let old_token = Sha256Digest::from_bytes([1; 32]);
    let old_alive = grants[&old_token].alive.clone();
    let sibling_token = Sha256Digest::from_bytes([2; 32]);
    let sibling_alive = grants[&sibling_token].alive.clone();
    grants.get_mut(&sibling_token).unwrap().used = true;
    let replacement = protocol::Grant {
        worker: Incarnation::from_bytes([1; 16]),
        token: Sha256Digest::from_bytes([100; 32]),
        ..template.clone()
    };
    assert!(matches!(
        register_authorization(
            &mut grants,
            template.service,
            template.pool,
            replacement.clone(),
            now + Duration::from_secs(1),
        ),
        protocol::Observation::Registered
    ));
    assert!(
        !old_alive.load(Ordering::Acquire),
        "old socket retained authority after ACK"
    );
    assert!(!grants.contains_key(&old_token));
    assert_eq!(grants.len(), 32);
    assert_eq!(
        grants
            .values()
            .filter(|entry| entry.grant.worker == replacement.worker)
            .count(),
        1
    );
    assert!(!grants[&replacement.token].used);
    assert_eq!(
        grants[&replacement.token].expires,
        now + Duration::from_secs(301)
    );
    assert!(Arc::ptr_eq(&sibling_alive, &grants[&sibling_token].alive));
    assert!(grants[&sibling_token].used);
    assert_eq!(
        grants[&sibling_token].expires,
        now + Duration::from_secs(300)
    );
    let extra = protocol::Grant {
        worker: Incarnation::from_bytes([33; 16]),
        token: Sha256Digest::from_bytes([101; 32]),
        ..template
    };
    assert!(matches!(
        register_authorization(&mut grants, extra.service, extra.pool, extra, now),
        protocol::Observation::Failed
    ));
    assert_eq!(grants.len(), 32);
    assert!(grants[&replacement.token].alive.load(Ordering::Acquire));
    for member in 2u8..=32 {
        assert!(
            grants[&Sha256Digest::from_bytes([member; 32])]
                .alive
                .load(Ordering::Acquire)
        );
    }
    Ok(())
}

#[test]
fn invalid_or_colliding_registration_preserves_consumed_and_sibling_authority() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (first, _, _) = fixture(root.path(), "invalid", "unused fixture")?;
    let sibling = protocol::Grant {
        worker: Incarnation::from_bytes([41; 16]),
        token: Sha256Digest::from_bytes([42; 32]),
        ..first.clone()
    };
    let now = Instant::now();
    let mut grants = BTreeMap::new();
    for grant in [&first, &sibling] {
        assert!(matches!(
            register_authorization(&mut grants, first.service, first.pool, grant.clone(), now),
            protocol::Observation::Registered
        ));
        grants.get_mut(&grant.token).unwrap().used = true;
    }
    let first_alive = grants[&first.token].alive.clone();
    let sibling_alive = grants[&sibling.token].alive.clone();
    let first_bytes = serde_json::to_vec(&grants[&first.token].grant)?;
    let sibling_bytes = serde_json::to_vec(&grants[&sibling.token].grant)?;
    let mut invalid_project = first.project.clone();
    invalid_project.binding = Sha256Digest::from_bytes([255; 32]);
    let fresh = protocol::Grant {
        token: Sha256Digest::from_bytes([43; 32]),
        ..first.clone()
    };
    let attempts = [
        first.clone(), // A duplicate consumed token cannot become unused again.
        protocol::Grant {
            token: sibling.token,
            ..fresh.clone()
        },
        protocol::Grant {
            service: Incarnation::from_bytes([44; 16]),
            ..fresh.clone()
        },
        protocol::Grant {
            pool: Incarnation::from_bytes([45; 16]),
            ..fresh.clone()
        },
        protocol::Grant {
            project: invalid_project,
            ..fresh
        },
    ];
    for invalid in attempts {
        assert!(matches!(
            register_authorization(
                &mut grants,
                first.service,
                first.pool,
                invalid,
                now + Duration::from_secs(1)
            ),
            protocol::Observation::Failed
        ));
        assert_eq!(grants.len(), 2);
        assert_eq!(
            serde_json::to_vec(&grants[&first.token].grant)?,
            first_bytes
        );
        assert_eq!(
            serde_json::to_vec(&grants[&sibling.token].grant)?,
            sibling_bytes
        );
        for (token, alive) in [(first.token, &first_alive), (sibling.token, &sibling_alive)] {
            assert!(alive.load(Ordering::Acquire));
            assert!(Arc::ptr_eq(alive, &grants[&token].alive));
            assert!(grants[&token].used);
            assert_eq!(grants[&token].expires, now + Duration::from_secs(300));
        }
    }
    Ok(())
}

#[test]
fn repaired_owner_survives_delayed_old_release_without_losing_sibling_inference() -> Result<()> {
    // Exercise both a replaced-but-live owner and an expired owner. The
    // latter is normally retired by Sweep; the former needs the distinct
    // owner check even when the old connection has not observed revocation.
    for old_still_alive in [true, false] {
        let root = tempfile::tempdir()?;
        let (first, location, identity) = fixture(root.path(), "first", "shared fixture")?;
        let (mut sibling, other, other_identity) =
            fixture(root.path(), "sibling", "shared fixture")?;
        sibling.token = Sha256Digest::from_bytes([14; 32]);
        let old_session = Incarnation::from_bytes([21; 16]);
        let sibling_session = Incarnation::from_bytes([22; 16]);
        let repaired_session = Incarnation::from_bytes([23; 16]);
        let old_alive = Arc::new(AtomicBool::new(true));
        let old_owner = Arc::downgrade(&old_alive);
        let sibling_alive = Arc::new(AtomicBool::new(true));
        let repaired_alive = Arc::new(AtomicBool::new(true));
        let mut engine = Engine::new(root.path())?;
        engine.admit_owner(first.worker, &old_alive)?;
        engine.request(
            &first,
            old_session,
            open(location.clone(), identity.clone()),
        )?;
        engine.admit_owner(sibling.worker, &sibling_alive)?;
        engine.request(&sibling, sibling_session, open(other, other_identity))?;
        assert_eq!(
            (engine.loads, engine.reuses, engine.sessions.len()),
            (1, 1, 2)
        );

        old_alive.store(old_still_alive, Ordering::Release);
        let repaired = protocol::Grant {
            token: Sha256Digest::from_bytes([15; 32]),
            ..first.clone()
        };
        engine.admit_owner(repaired.worker, &repaired_alive)?;
        assert!(!engine.sessions.contains_key(&(first.worker, old_session)));
        assert!(
            engine
                .sessions
                .contains_key(&(sibling.worker, sibling_session))
        );
        engine.request(
            &repaired,
            repaired_session,
            open(location.clone(), identity.clone()),
        )?;
        assert_eq!(
            (engine.loads, engine.reuses, engine.sessions.len()),
            (1, 2, 2)
        );

        // The obsolete socket can close after the new grant opens a model.
        // Its Weak witness remains exact even after its last Arc is gone.
        old_alive.store(false, Ordering::Release);
        assert!(engine.admit_owner(first.worker, &old_alive).is_err());
        assert!(
            engine
                .sessions
                .contains_key(&(first.worker, repaired_session))
        );
        assert!(
            engine
                .sessions
                .contains_key(&(sibling.worker, sibling_session))
        );
        drop(old_alive);
        engine.release_owner(first.worker, Some(&old_owner));
        engine.retire_disconnected();
        engine.admit_owner(repaired.worker, &repaired_alive)?;
        for (grant, session) in [(&repaired, repaired_session), (&sibling, sibling_session)] {
            assert!(engine.sessions.contains_key(&(grant.worker, session)));
            let Response::Embedded { vectors, .. } = engine.request(
                grant,
                session,
                Request::Embed {
                    texts: vec!["cobalt".into()],
                    workers: 1,
                    memory_bytes: 8 << 20,
                },
            )?
            else {
                anyhow::bail!("delayed old-owner release lost a live inference session")
            };
            assert_eq!(vectors, vec![Some(vec![1.0, 0.0])]);
        }
        assert_eq!(engine.loads, 1, "repair reloaded the shared model");

        // Exact current-owner close retires only that worker. A subsequent
        // explicit Revoke retains its intentional whole-worker semantics.
        engine.release_owner(first.worker, Some(&Arc::downgrade(&repaired_alive)));
        assert!(
            !engine
                .sessions
                .contains_key(&(first.worker, repaired_session))
        );
        assert!(
            engine
                .sessions
                .contains_key(&(sibling.worker, sibling_session))
        );
        engine.admit_owner(repaired.worker, &repaired_alive)?;
        engine.request(&repaired, repaired_session, open(location, identity))?;
        engine.release_owner(first.worker, None);
        assert!(
            !engine
                .sessions
                .contains_key(&(first.worker, repaired_session))
        );
        assert!(
            engine
                .sessions
                .contains_key(&(sibling.worker, sibling_session))
        );
        assert!(engine.owners.contains_key(&sibling.worker));
        assert!(!engine.owners.contains_key(&first.worker));
        let Response::Embedded { vectors, .. } = engine.request(
            &sibling,
            sibling_session,
            Request::Embed {
                texts: vec!["harbor".into()],
                workers: 1,
                memory_bytes: 8 << 20,
            },
        )?
        else {
            anyhow::bail!("worker-local revocation lost sibling inference")
        };
        assert_eq!(vectors, vec![Some(vec![0.0, 1.0])]);
        assert_eq!(engine.loads, 1);
    }
    Ok(())
}

#[test]
fn compatible_projects_share_verified_inference_and_foreign_assets_are_rejected() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (first, location, identity) = fixture(root.path(), "first", "original fixture")?;
    let (second, other, other_identity) = fixture(root.path(), "second", "original fixture")?;
    let session = Incarnation::from_bytes([4; 16]);
    let mut engine = Engine::new(root.path())?;
    engine.request(&first, session, open(location.clone(), identity.clone()))?;
    engine.request(&second, session, open(other.clone(), other_identity))?;
    assert_eq!((engine.loads, engine.reuses), (1, 1));
    let embed = || Request::Embed {
        texts: vec!["cobalt harbor".into()],
        workers: 8,
        memory_bytes: 1 << 30,
    };
    let Response::Embedded { vectors: a, .. } = engine.request(&first, session, embed())? else {
        anyhow::bail!("missing first vector")
    };
    let Response::Embedded { vectors: b, .. } = engine.request(&second, session, embed())? else {
        anyhow::bail!("missing second vector")
    };
    assert_eq!(a, b);
    assert!(
        engine
            .request(
                &second,
                Incarnation::from_bytes([5; 16]),
                open(location, identity)
            )
            .is_err()
    );
    assert!(
        engine
            .request(
                &first,
                session,
                Request::Extract {
                    directory: root.path().into()
                }
            )
            .is_err()
    );
    assert_eq!(engine.loads, 1);
    Ok(())
}

#[test]
fn equal_legacy_weight_digest_does_not_alias_distinct_pipeline_descriptors() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (first, location, identity) = fixture(root.path(), "first", "original fixture")?;
    let (second, other, other_identity) = fixture(root.path(), "second", "different provenance")?;
    assert_eq!(identity, other_identity);
    let session = Incarnation::from_bytes([4; 16]);
    let mut engine = Engine::new(root.path())?;
    engine.request(&first, session, open(location, identity))?;
    engine.request(&second, session, open(other, other_identity))?;
    assert_eq!((engine.loads, engine.reuses), (2, 0));
    engine.request(&first, session, Request::Peak)?;
    assert_eq!(engine.loads, 3);
    Ok(())
}

#[test]
fn interleaved_incompatible_models_return_only_their_scoped_vectors() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (first, location, identity) = fixture(root.path(), "first", "first fixture")?;
    let (second, other, _) = fixture(root.path(), "second", "second fixture")?;
    let ModelLocation::Directory { path } = &other else {
        unreachable!()
    };
    let vocabulary = br#"{"cobalt":[0.0,1.0],"harbor":[1.0,0.0]}"#;
    let changed = format!("{:x}", Sha256::digest(vocabulary));
    let manifest = airk_retrieval::vector::ModelManifest {
        schema: 1,
        name: "second fixture".into(),
        license: "Apache-2.0".into(),
        sha256: changed.clone(),
        dimensions: 2,
    };
    std::fs::write(path.join("vocabulary.json"), vocabulary)?;
    std::fs::write(path.join("model.json"), serde_json::to_vec(&manifest)?)?;
    let session = Incarnation::from_bytes([4; 16]);
    let mut engine = Engine::new(root.path())?;
    engine.request(&first, session, open(location, identity))?;
    engine.request(&second, session, open(other, changed))?;
    let embed = || Request::Embed {
        texts: vec!["cobalt".into()],
        workers: 1,
        memory_bytes: 8 << 20,
    };
    let Response::Embedded { vectors: a, .. } = engine.request(&first, session, embed())? else {
        anyhow::bail!("missing first vector")
    };
    let Response::Embedded { vectors: b, .. } = engine.request(&second, session, embed())? else {
        anyhow::bail!("missing second vector")
    };
    assert_eq!(a, vec![Some(vec![1.0, 0.0])]);
    assert_eq!(b, vec![Some(vec![0.0, 1.0])]);
    let Response::Embedded { vectors: again, .. } = engine.request(&first, session, embed())?
    else {
        anyhow::bail!("missing repeated vector")
    };
    assert_eq!(a, again);
    Ok(())
}
#[test]
fn bundled_provision_is_global_persistent_and_does_not_unload_compatible_inference() -> Result<()> {
    let root = tempfile::tempdir()?;
    let _ownership = airk_store::service::ServiceLease::acquire(root.path())?;
    // SAFETY: this exact private model fixture retains its actual service lease
    // until the engine/session drops. No worker or external role is authorized.
    let _scope = unsafe { airk_retrieval::native_inference::ServiceComputeScope::enter()? };
    let (first, _, _) = fixture(root.path(), "bundled-first", "unused fixture")?;
    let (second, _, _) = fixture(root.path(), "bundled-second", "unused fixture")?;
    // Test projects have no per-project model input directories.
    for grant in [&first, &second] {
        std::fs::remove_dir_all(Path::new(&grant.project.data_root).join("models"))?;
    }
    let mut engine = Engine::new(root.path())?;
    let session = Incarnation::from_bytes([9; 16]);
    let Response::Provisioned { objects } =
        engine.request(&first, session, Request::ProvisionBundled)?
    else {
        anyhow::bail!("default model provisioning failed")
    };
    assert_eq!(
        objects.local_set(),
        airk_cli::bundled_assets::default_plan()?
    );
    let Response::Provisioned { objects: other } =
        engine.request(&second, session, Request::ProvisionBundled)?
    else {
        anyhow::bail!("second default model provisioning failed")
    };
    assert_eq!(objects, other);
    engine.request(
        &first,
        session,
        open(
            ModelLocation::Shared {
                objects: objects.clone(),
            },
            objects.legacy_identity.clone(),
        ),
    )?;
    engine.request(
        &second,
        session,
        open(
            ModelLocation::Shared { objects: other },
            objects.legacy_identity.clone(),
        ),
    )?;
    assert_eq!((engine.loads, engine.reuses), (1, 1));
    engine.request(
        &second,
        Incarnation::from_bytes([10; 16]),
        Request::ProvisionBundled,
    )?;
    assert!(engine.model.is_some());
    assert_eq!(engine.loads, 1);
    assert_eq!(
        std::fs::read_dir(root.path().join("model-files-1.4/sets"))?.count(),
        1
    );
    for grant in [&first, &second] {
        assert!(!Path::new(&grant.project.data_root).join("models").exists());
    }
    drop(engine);
    let mut engine = Engine::new(root.path())?;
    assert_eq!(
        airk_store::service::model_files::ServiceModelFiles::default_reference(
            root.path(),
            &objects.local_set()
        )?,
        Some(objects.clone())
    );
    engine.request(
        &first,
        session,
        open(
            ModelLocation::Shared {
                objects: objects.clone(),
            },
            objects.legacy_identity.clone(),
        ),
    )?;
    assert_eq!(engine.loads, 1);
    Ok(())
}

#[test]
fn custom_provision_captures_once_and_shared_sessions_reject_other_runtime() -> Result<()> {
    let root = tempfile::tempdir()?;
    let (first, location, identity) = fixture(root.path(), "custom-first", "custom fixture")?;
    let (second, _, _) = fixture(root.path(), "custom-second", "unused fixture")?;
    let ModelLocation::Directory { path } = location else {
        unreachable!()
    };
    let before = std::fs::read(path.join("vocabulary.json"))?;
    let mut engine = Engine::new(root.path())?;
    let session = Incarnation::from_bytes([9; 16]);
    let Response::Provisioned { objects } = engine.request(
        &first,
        session,
        Request::ProvisionModel {
            directory: path.clone(),
        },
    )?
    else {
        anyhow::bail!("custom model provisioning failed")
    };
    assert!(
        airk_store::service::model_files::ServiceModelFiles::default_reference(
            root.path(),
            &objects.local_set()
        )?
        .is_none()
    );
    // Captured installed assets no longer depend on the original selected directory.
    std::fs::rename(&path, path.with_file_name("selected-original"))?;
    engine.request(
        &first,
        session,
        open(
            ModelLocation::Shared {
                objects: objects.clone(),
            },
            identity.clone(),
        ),
    )?;
    engine.request(
        &second,
        session,
        open(
            ModelLocation::Shared {
                objects: objects.clone(),
            },
            identity.clone(),
        ),
    )?;
    assert_eq!((engine.loads, engine.reuses), (1, 1));
    let foreign = tempfile::tempdir()?;
    let mut other = Engine::new(foreign.path())?;
    assert!(
        other
            .request(
                &first,
                session,
                open(ModelLocation::Shared { objects }, identity)
            )
            .is_err()
    );
    assert_eq!(
        std::fs::read(
            path.with_file_name("selected-original")
                .join("vocabulary.json")
        )?,
        before
    );
    Ok(())
}
#[test]
fn calibration_sessions_use_global_inputs_and_preserve_exact_identity_across_projects() -> Result<()>
{
    let root = tempfile::tempdir()?;
    let (first, _, _) = fixture(root.path(), "calibration-first", "unused")?;
    let (second, _, _) = fixture(root.path(), "calibration-second", "unused")?;
    let source = root.path().canonicalize()?.join("selected-tokenizer");
    std::fs::create_dir(&source)?;
    let bytes = serde_json::to_vec(
        &serde_json::json!({"version":"1.0","truncation":null,"padding":null,
        "added_tokens":[],"normalizer":null,"pre_tokenizer":{"type":"Whitespace"},
        "post_processor":null,"decoder":null,"model":{"type":"WordLevel","vocab":{"[UNK]":0,"copper":1},"unk_token":"[UNK]"}}),
    )?;
    std::fs::write(source.join("tokenizer.json"), &bytes)?;
    let path = source.join("selected.json");
    std::fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({"schema":2,
        "tokenizer_file":"tokenizer.json","tokenizer_sha256":format!("{:x}",Sha256::digest(&bytes)),
        "model_revision_sha256":"a".repeat(64),"add_special_tokens":false,"framing":{"prefix":"copper ","suffix":" copper"}}))?,
    )?;
    let identity = airk_kernel::usage::OfflineTokenizer::open(&path)?
        .identity()
        .clone();
    let mut engine = Engine::new(root.path())?;
    let session = Incarnation::from_bytes([8; 16]);
    for grant in [&first, &second] {
        let Response::TokenizerOpened {
            identity: actual, ..
        } = engine.request(
            grant,
            session,
            Request::TokenizerOpen {
                protocol: PROTOCOL,
                version: env!("CARGO_PKG_VERSION").into(),
                path: path.clone(),
            },
        )?
        else {
            anyhow::bail!("calibration open failed")
        };
        assert_eq!(actual, identity);
        let Binding::Tokenizer { path: shared } =
            &engine.sessions[&(grant.worker, session)].binding
        else {
            unreachable!()
        };
        assert!(
            shared.starts_with(
                root.path()
                    .canonicalize()?
                    .join("model-files-1.4/calibration")
            )
        );
    }
    std::fs::rename(&source, source.with_file_name("original-inputs"))?;
    // Force re-opening from the shared session binding, without its source directory.
    engine.tokenizer = None;
    engine.tokenizer_path = None;
    for grant in [&second, &first] {
        assert!(matches!(
            engine.request(
                grant,
                session,
                Request::TokenCount {
                    text: "copper copper".into()
                }
            )?,
            Response::TokenCount { count: 4 }
        ));
    }
    assert_eq!(
        std::fs::read(
            source
                .with_file_name("original-inputs")
                .join("tokenizer.json")
        )?,
        bytes
    );
    Ok(())
}
