# SPDX-License-Identifier: Apache-2.0
"""Split plan Phase 2 group G5: inject the project owner's service hooks.

The project owner (`project_worker`) called `runtime_metadata`,
`runtime_install` and `reference_refresh_control` directly. Those calls made the
catalog module cycle and the client/runtime cycle reach each other, so
`boundary_check.py` reported one merged 20-module cycle. This step replaces the
four direct calls with a `project_worker::Hooks` value of plain function
pointers. The service composition root (`service_runtime`) builds it with
`runtime_metadata::project_worker_hooks()` and passes it through
`Supervisor::start` -> `start_worker` -> `ChildWorker::spawn` ->
`start_in_process` -> `run_in_process` -> `project_worker::serve`. The private
child-process role builds the same hooks in a crate-root function. The
functions called are unchanged, so behavior is unchanged.

Run from the repository root on a clean tree: python scripts/dev/split_g5_worker_hooks.py
"""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "crates/airk-cli/src"


def edit(relative, replacements):
    path = SRC / relative
    text = path.read_text(encoding="utf-8")
    for old, new, count in replacements:
        found = text.count(old)
        if found != count:
            raise SystemExit(f"{relative}: expected {count} of {old[:70]!r}, found {found}")
        text = text.replace(old, new)
    path.write_text(text, encoding="utf-8", newline="\n")


HOOKS = '''/// A boxed observation future returned by a [`Hooks`] observer.
pub(crate) type Observation<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = serde_json::Value> + Send + 'a>>;

/// Service functions the project owner calls but does not own. The service
/// composition root supplies them (split plan G5), so this module depends on
/// neither runtime metadata, installation nor reference-refresh control.
#[derive(Clone, Copy)]
pub(crate) struct Hooks {
    /// Runtime bundle for an unscoped `airk_diagnostics` call.
    pub(crate) runtime_bundle: for<'a> fn(
        &'a Path,
        WorkerIdentity,
        Option<airk_core::service::RuntimeProcess>,
    ) -> Observation<'a>,
    /// Work summary for an unscoped `airk_diagnostics` call.
    pub(crate) runtime_work: for<'a> fn(&'a Path, WorkerIdentity) -> Observation<'a>,
    /// Rebuild observation bound to the project's kernel.
    pub(crate) rebuild_observation: fn(
        &Path,
        airk_core::service::ProjectBinding,
    ) -> Result<Option<airk_core::rebuild::RebuildObservation>>,
    /// Executes an accepted reference-refresh configuration job.
    pub(crate) refresh_references: fn(
        &Kernel,
        &mut ServiceRegistry,
        &ProjectLease,
        airk_core::identity::Sha256Digest,
    ) -> Result<JobOutcome>,
}

struct State {
    control: std::path::PathBuf,
    identity: WorkerIdentity,
    hooks: Hooks,'''

RUNTIME_HOOKS = '''/// The service functions a project owner calls (split plan G5); the owner
/// itself does not depend on this module, installation or reference refresh.
pub(crate) fn project_worker_hooks() -> crate::project_worker::Hooks {
    crate::project_worker::Hooks {
        runtime_bundle: observe_runtime_bundle,
        runtime_work: observe_runtime_work,
        rebuild_observation: crate::runtime_install::rebuild_observation_for_control,
        refresh_references: crate::reference_refresh_control::execute,
    }
}

fn observe_runtime_bundle(
    control: &Path,
    identity: airk_core::service::WorkerIdentity,
    frontend: Option<RuntimeProcess>,
) -> crate::project_worker::Observation<'_> {
    Box::pin(bundle(control, identity, frontend))
}

fn observe_runtime_work(
    control: &Path,
    identity: airk_core::service::WorkerIdentity,
) -> crate::project_worker::Observation<'_> {
    Box::pin(work(control, identity))
}

pub(crate) async fn bundle('''

LIB_ENTRY = '''
/// The private project-owner process role, wired with the service hooks
/// (see `project_worker::Hooks`).
pub async fn run_project_worker() -> anyhow::Result<()> {
    project_worker::run(runtime_metadata::project_worker_hooks()).await
}
'''

CONTEXT = "WorkerOwnerContext { root: &root, instance, gate: catalog_gate.as_ref() }"


def main():
    if "pub(crate) struct Hooks" in (SRC / "project_worker.rs").read_text(encoding="utf-8"):
        raise SystemExit("G5 already applied")
    edit("project_worker.rs", [
        ("struct State {\n    control: std::path::PathBuf,\n    identity: WorkerIdentity,", HOOKS, 1),
        ("tools.set_runtime(crate::runtime_metadata::bundle(&state.control, state.identity, request.frontend).await);",
         "tools.set_runtime((state.hooks.runtime_bundle)(&state.control, state.identity, request.frontend).await);", 1),
        ("tools.set_work(crate::runtime_metadata::work(&state.control, state.identity).await);",
         "tools.set_work((state.hooks.runtime_work)(&state.control, state.identity).await);", 1),
        ("    let rebuild_project = init.project;\n", "    let rebuild_project = init.project;\n    let rebuild = hooks.rebuild_observation;\n", 1),
        ("            crate::runtime_install::rebuild_observation_for_control(\n                &rebuild_control,\n                rebuild_project,\n            )",
         "            rebuild(&rebuild_control, rebuild_project)", 1),
        ("return crate::reference_refresh_control::execute(&kernel,&mut registry,&job_lease,job.id);",
         "return (hooks.refresh_references)(&kernel,&mut registry,&job_lease,job.id);", 1),
        ("        control: init.runtime.clone(),\n        identity,\n", "        control: init.runtime.clone(),\n        identity,\n        hooks,\n", 1),
        ("pub async fn run() -> Result<()> {", "pub(crate) async fn run(hooks: Hooks) -> Result<()> {", 1),
        ("        Residency::Process,\n    )\n    .await\n}", "        Residency::Process,\n        hooks,\n    )\n    .await\n}", 1),
        ("    residency: Residency,\n) -> Result<()> {\n    local_ipc::prepare_root",
         "    residency: Residency,\n    hooks: Hooks,\n) -> Result<()> {\n    local_ipc::prepare_root", 1),
    ])
    edit("runtime_metadata.rs", [("pub(crate) async fn bundle(", RUNTIME_HOOKS, 1)])
    edit("project_supervisor.rs", [
        ("struct WorkerOwnerContext<'a> {\n    root: &'a Path,\n    instance: Incarnation,\n    gate: Option<&'a crate::catalog_admission::NativeCatalogAdmission>,\n}",
         "struct WorkerOwnerContext<'a> {\n    root: &'a Path,\n    instance: Incarnation,\n    gate: Option<&'a crate::catalog_admission::NativeCatalogAdmission>,\n    hooks: crate::project_worker::Hooks,\n}", 1),
        (CONTEXT, "WorkerOwnerContext { root: &root, instance, gate: catalog_gate.as_ref(), hooks }", 5),
        ("    let WorkerOwnerContext {\n        root,\n        instance,\n        gate,\n    } = owner;",
         "    let WorkerOwnerContext {\n        root,\n        instance,\n        gate,\n        hooks: _,\n    } = owner;", 1),
        ("    let WorkerOwnerContext {\n        root,\n        instance,\n        gate: catalog_gate,\n    } = owner;",
         "    let WorkerOwnerContext {\n        root,\n        instance,\n        gate: catalog_gate,\n        hooks,\n    } = owner;", 1),
        ("        ChildWorker::spawn(root, project, instance, derived)\n",
         "        ChildWorker::spawn(root, project, instance, derived, hooks)\n", 1),
        ("        service: Incarnation,\n        derived: bool,\n    ) -> Result<Self> {\n        Self::start_in_process(\n            root,\n            project,\n            service,\n            derived,\n            crate::runtime_pair::engine_identity()?,\n        )",
         "        service: Incarnation,\n        derived: bool,\n        hooks: crate::project_worker::Hooks,\n    ) -> Result<Self> {\n        Self::start_in_process(\n            root,\n            project,\n            service,\n            derived,\n            crate::runtime_pair::engine_identity()?,\n            hooks,\n        )", 1),
        ("        derived: bool,\n        engine: airk_core::identity::Sha256Digest,\n    ) -> Result<Self> {",
         "        derived: bool,\n        engine: airk_core::identity::Sha256Digest,\n        hooks: crate::project_worker::Hooks,\n    ) -> Result<Self> {", 1),
        ("                    owner_scope,\n                    derived,\n                )",
         "                    owner_scope,\n                    derived,\n                    hooks,\n                )", 1),
        ("    scope: crate::worker_scope::WorkerScope,\n    derived: bool,\n) -> Result<()> {",
         "    scope: crate::worker_scope::WorkerScope,\n    derived: bool,\n    hooks: crate::project_worker::Hooks,\n) -> Result<()> {", 1),
        ("    let result = runtime.block_on(crate::project_worker::serve(\n        init, commands, replies, residency,\n    ));",
         "    let result = runtime.block_on(crate::project_worker::serve(\n        init, commands, replies, residency, hooks,\n    ));", 1),
        ("        log: crate::service_logs::Log,\n        ownership: Arc<airk_store::service::ServiceLease>,\n    ) -> Result<Self> {",
         "        log: crate::service_logs::Log,\n        ownership: Arc<airk_store::service::ServiceLease>,\n        hooks: crate::project_worker::Hooks,\n    ) -> Result<Self> {", 1),
        ("ChildWorker::spawn(&root, project, service, false)?;",
         "ChildWorker::spawn(\n            &root,\n            project,\n            service,\n            false,\n            crate::runtime_metadata::project_worker_hooks(),\n        )?;", 1),
        ("ChildWorker::start_in_process(&root, project, service, false, engine)?;",
         "ChildWorker::start_in_process(\n            &root,\n            project,\n            service,\n            false,\n            engine,\n            crate::runtime_metadata::project_worker_hooks(),\n        )?;", 1),
    ])
    catalog_context = ("                WorkerOwnerContext {\n                    root,\n                    instance,\n"
                       "                    gate: Some(&gate),\n                },")
    edit("project_supervisor/catalog.rs", [
        ("    backoff: &mut BTreeMap<ProjectBinding, (Instant, u32)>,\n    admission: WorkerAdmission<'_>,\n) -> Result<Response> {",
         "    backoff: &mut BTreeMap<ProjectBinding, (Instant, u32)>,\n    admission: WorkerAdmission<'_>,\n    hooks: crate::project_worker::Hooks,\n) -> Result<Response> {", 1),
        (catalog_context, catalog_context.replace("gate: Some(&gate),\n", "gate: Some(&gate),\n                    hooks,\n"), 4),
    ])
    edit("project_supervisor.rs", [
        ("WorkerAdmission { log: &log, policy: &policy, pool: &mut pool, resources: &mut resources, recovery: &mut pool_recovery, drains: &mut resource_drains });\n                            catalog::send_reply(&reply, result);",
         "WorkerAdmission { log: &log, policy: &policy, pool: &mut pool, resources: &mut resources, recovery: &mut pool_recovery, drains: &mut resource_drains }, hooks);\n                            catalog::send_reply(&reply, result);", 1),
    ])
    edit("project_supervisor.rs", [
        ("impl Supervisor {\n    pub fn start(",
         "impl Supervisor {\n    #[allow(clippy::too_many_arguments)] // The owned inputs and service hooks stay explicit.\n    pub fn start(", 1),
    ])
    edit("project_supervisor/guarded_qualification_tests.rs", [
        ("                root: &self.root,\n                instance: self.instance,\n                gate: Some(&self.gate),\n            },",
         "                root: &self.root,\n                instance: self.instance,\n                gate: Some(&self.gate),\n                hooks: crate::runtime_metadata::project_worker_hooks(),\n            },", 1),
    ])
    edit("project_supervisor/guarded_qualification_tests.rs", [
        ("                drains: &mut self.drains,\n            },\n        )\n    }",
         "                drains: &mut self.drains,\n            },\n            crate::runtime_metadata::project_worker_hooks(),\n        )\n    }", 1),
    ])
    edit("service_runtime.rs", [
        ("        log.clone(),\n        _ownership.clone(),\n    )?;\n    let mut legacy_connections",
         "        log.clone(),\n        _ownership.clone(),\n        crate::runtime_metadata::project_worker_hooks(),\n    )?;\n    let mut legacy_connections", 1),
    ])
    edit("project_supervisor/catalog/startup_order_tests.rs", [
        ("        log,\n        ownership.clone(),\n    )?;", "        log,\n        ownership.clone(),\n        crate::runtime_metadata::project_worker_hooks(),\n    )?;", 1),
    ])
    edit("project_supervisor/guarded_qualification_tests/restart.rs", [
        ("        log,\n        lease,\n    )?;", "        log,\n        lease,\n        crate::runtime_metadata::project_worker_hooks(),\n    )?;", 1),
    ])
    lib = SRC / "lib.rs"
    lib.write_text(lib.read_text(encoding="utf-8").rstrip("\n") + "\n" + LIB_ENTRY, encoding="utf-8", newline="\n")
    edit("main.rs", [("return airk_cli::project_worker::run().await,", "return airk_cli::run_project_worker().await,", 1)])


if __name__ == "__main__":
    main()
