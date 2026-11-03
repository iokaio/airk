// SPDX-License-Identifier: Apache-2.0
//! Explicit isolated-pair qualification of the actual supervisor control path.
use super::*;
use airk_retrieval::embedding_worker::{self, ModelLocation, Request, Response};
use sha2::{Digest, Sha256};

async fn search(worker: &mut ChildWorker, mode: &str) -> Result<serde_json::Value> {
    let ControlResponse::Connection { role, grant } = worker.grant()? else {
        anyhow::bail!("worker connection missing")
    };
    let mut stream = tokio::time::timeout(
        Duration::from_secs(5),
        local_ipc::connect(&local_ipc::endpoint(&worker.runtime, &role)?),
    )
    .await??;
    local_ipc::write_message(&mut stream, &grant, Duration::from_secs(5)).await?;
    let accepted: protocol::Accepted = local_ipc::read_message(&mut stream, Duration::from_secs(5))
        .await?
        .context("query handshake EOF")?;
    ensure!(
        Some(accepted.identity) == worker.identity,
        "query identity crossed"
    );
    local_ipc::write_message(
        &mut stream,
        &protocol::Query {
            frontend: None,
            tool: "airk_search".into(),
            arguments:
                serde_json::json!({"query":"cobalt harbor","mode":mode,"response_mode":"full"})
                    .as_object()
                    .unwrap()
                    .clone(),
            native: None,
            status_receipt: Default::default(),
        },
        Duration::from_secs(5),
    )
    .await?;
    let result: protocol::QueryResult =
        local_ipc::read_message(&mut stream, Duration::from_secs(20))
            .await?
            .context("query response EOF")?;
    ensure!(
        result.result["isError"] != true,
        "query failed: {}",
        result.result
    );
    let body: serde_json::Value = serde_json::from_str(
        result.result["content"][0]["text"]
            .as_str()
            .context("query text missing")?,
    )?;
    ensure!(
        body["hits"].as_array().is_some_and(|hits| !hits.is_empty()),
        "no hits: {body}"
    );
    Ok(body)
}

fn bind(worker: &mut ChildWorker, pool: &mut Pool, registry: &ServiceRegistry) -> Result<()> {
    let (access, pending) = worker
        .prepare_pool_binding(Some(pool), registry, false)?
        .context("expected a binding update")?;
    worker.configure_pool(access, pending)
}

struct ConnectorOwner;
impl Drop for ConnectorOwner {
    fn drop(&mut self) {
        crate::embedding_pool_client::close();
    }
}

#[test]
#[ignore = "requires AIRK_TEST_RUNTIME pointing to an isolated verified native pair; run alone"]
fn native_supervisor_repairs_missing_grant_without_restarting_owners() -> Result<()> {
    let staged =
        PathBuf::from(std::env::var_os("AIRK_TEST_RUNTIME").context("AIRK_TEST_RUNTIME required")?)
            .canonicalize()?;
    let main = staged
        .join(crate::runtime_install::main_name())
        .canonicalize()?;
    crate::runtime_pair::verify_helper(&main)?;
    let engine = crate::runtime_pair::engine_identity_at(&main)?;
    let owner = tempfile::Builder::new()
        .prefix("airk-grant-repair-")
        .tempdir_in(std::env::temp_dir().canonicalize()?)?;
    let root = owner.path().join("control");
    local_ipc::prepare_root(&root)?;
    let service = new_instance_id()?;
    let mut registry = ServiceRegistry::open(&root, service)?;
    let source = owner.path().join("source");
    let data = owner.path().join("data");
    let model = owner.path().join("model");
    for path in [&source, &data, &model] {
        std::fs::create_dir(path)?;
    }
    let original = "# Cobalt harbor\nCobalt harbor navigation memo.\n";
    std::fs::write(source.join("memo.md"), original)?;
    let vocabulary = br#"{"cobalt":[1.0,0.0],"harbor":[0.0,1.0]}"#;
    std::fs::write(model.join("vocabulary.json"), vocabulary)?;
    std::fs::write(
        model.join("model.json"),
        serde_json::to_vec(&serde_json::json!({
            "schema":1,"name":"supervisor grant repair fixture","license":"Apache-2.0",
            "sha256":format!("{:x}",Sha256::digest(vocabulary)),"dimensions":2
        }))?,
    )?;
    airk_store::service::data_scope::bind(&data, &root, registry.incarnation()?)?;
    let project = airk_kernel::service_projects::registration(&source, &data, "repair")?;
    registry.register(&project)?;
    let mut pool = Pool::start_fixture(&root, service, &main)?;
    let pool_id = pool.identity();
    let pool_pid = pool.process_id();
    // A separate authorized member retains a real remote model session throughout
    // worker revocation and repair. Its ownership must not be cleared by repair.
    let sibling = WorkerIdentity {
        service,
        worker: new_instance_id()?,
        project: project.binding,
        fencing_generation: 1,
    };
    let access = pool.access(sibling.worker, project.clone())?;
    crate::embedding_pool_client::install(&root, sibling, Some(access), false)?;
    let _connector = ConnectorOwner;
    let connector = embedding_worker::connector()?;
    // Capture the sibling's shared reference before indexing loads the model.
    // Explicit custom provisioning retires resident inference state; repeating it
    // after indexing would add a setup load unrelated to grant repair.
    let objects = connector
        .provision_model(&model)
        .context("provision the sibling fixture model")?;
    let kernel =
        airk_kernel::Kernel::open_service_collection(&source, Some(data.clone()), "repair")?
            .with_service_models(&root, crate::bundled_assets::default_plan()?)?;
    kernel
        .install_vector_model(&model)
        .context("install fixture model through the shared helper")?;
    kernel.index().context("publish the fixture collection")?;
    drop(kernel);
    let session = connector
        .open(
            ModelLocation::Shared {
                objects: objects.clone(),
            },
            &objects.legacy_identity,
        )
        .context("open the sibling fixture model session")?;
    let embed = || -> Result<()> {
        let Response::Embedded { vectors, .. } = session
            .request(Request::Embed {
                texts: vec!["cobalt harbor".into()],
                workers: 1,
                // Exercise the real helper's minimum admitted working-memory budget.
                memory_bytes: 8 << 20,
            })
            .context("embed through the retained sibling session")?
        else {
            anyhow::bail!("remote embedding failed")
        };
        ensure!(vectors.len() == 1 && vectors[0].as_ref().is_some_and(|v| v.len() == 2));
        Ok(())
    };
    embed().context("initial sibling inference before worker admission")?;
    let mut worker =
        ChildWorker::spawn_image(&root, project.binding, service, false, &main, engine)?;
    let identity = worker.identity.context("worker identity")?;
    let worker_pid = worker.child.id();
    worker.activate(
        identity,
        QueryAllowance {
            concurrent_queries: 1,
            response_bytes: MAX_FRAME_BYTES,
        },
        None,
        true,
    )?;
    bind(&mut worker, &mut pool, &registry)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let before = runtime.block_on(search(&mut worker, "vector"))?;
    ensure!(
        !before["publication"].is_null(),
        "vector result omitted publication identity: {before}"
    );
    let crate::embedding_pool_protocol::Observation::Status { loads, .. } = pool.status()? else {
        anyhow::bail!("pool status missing")
    };
    assert_eq!(loads, 1, "fixture must use one actual shared helper model");
    pool.revoke(identity.worker)?;
    assert_eq!(
        worker.renew_pool_binding(&mut pool)?,
        crate::embedding_pool_process::Renewal::Missing
    );
    assert!(worker.pool_binding.is_none());
    worker.renew()?;
    embed()?;
    runtime.block_on(search(&mut worker, "lexical"))?;
    bind(&mut worker, &mut pool, &registry)?;
    assert_eq!(
        worker.renew_pool_binding(&mut pool)?,
        crate::embedding_pool_process::Renewal::Renewed
    );
    let after = runtime.block_on(search(&mut worker, "vector"))?;
    assert_eq!(before["publication"], after["publication"]);
    embed()?;
    assert_eq!(worker.identity, Some(identity));
    assert_eq!(worker.child.id(), worker_pid);
    assert_eq!(pool.identity(), pool_id);
    assert_eq!(pool.process_id(), pool_pid);
    assert!(
        matches!(pool.status()?, crate::embedding_pool_protocol::Observation::Status { loads: 1, sessions, .. } if sessions >= 1)
    );
    assert_eq!(std::fs::read_to_string(source.join("memo.md"))?, original);
    Ok(())
}
