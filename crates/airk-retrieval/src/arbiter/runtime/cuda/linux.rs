// SPDX-License-Identifier: Apache-2.0
//! x86-64 glibc loader for the separately pinned Linux CUDA runtime.
use super::*;
use std::{fs::File, sync::OnceLock};
#[cfg(feature = "test-support")]
mod assembly;
#[cfg(feature = "test-support")]
pub(super) use assembly::observe_assembly;
#[cfg(feature = "test-support")]
mod package;
#[cfg(feature = "test-support")]
pub(super) use package::observe_package_projection;
mod driver;
mod held;
mod mappings;
mod product;
use self::{
    driver::Driver,
    held::Held,
    mappings::{Handle, ImageId},
};
pub(super) use driver::PrimaryContext;
pub(super) use product::check_product_cpu_bounds;

static ADMISSION: Mutex<()> = Mutex::new(());
static LOADED: OnceLock<LoadedRuntime> = OnceLock::new();
static FAILED: AtomicBool = AtomicBool::new(false);

pub(super) fn runtime_poisoned() -> bool {
    FAILED.load(Ordering::Acquire)
}

type Bootstrap = unsafe extern "C" fn(*const ort::sys::OrtApiBase, i32) -> i32;

struct Assets {
    files: BTreeMap<String, Held>,
    handles: BTreeMap<String, Handle>,
    permitted: BTreeSet<ImageId>,
    sidecars: Vec<Held>,
}

impl Assets {
    fn reconcile(&self) -> Result<()> {
        ensure!(
            mappings::executable_images()?.is_subset(&self.permitted),
            "Linux CUDA executable image outside measured dependency closure"
        );
        Ok(())
    }

    fn bind(
        &mut self,
        row: &manifest::linux::ElfFile,
        bootstrap: Option<Bootstrap>,
        deadline: Instant,
    ) -> Result<()> {
        in_time(deadline)?;
        let expected = ImageId::file(self.files[&row.name].image_file())?;
        let handle = match Handle::no_load(&row.name)? {
            Some(h) => {
                ensure!(
                    bootstrap.is_none(),
                    "CUDA provider appeared before the core installed its genuine ProviderHost"
                );
                h
            }
            None => {
                for dep in &row.needed {
                    let dependency =
                        Handle::no_load(dep)?.context("Linux CUDA dependency not preloaded")?;
                    ensure!(
                        dependency.id()? == ImageId::file(self.files[dep].image_file())?,
                        "Linux ELF DT_NEEDED resolves to different object"
                    );
                }
                let held = &self.files[&row.name];
                if let Some(bootstrap) = bootstrap {
                    use std::os::fd::AsRawFd;
                    // The linked core scopes this fd to its own ProviderLibrary call. It sets
                    // the shared ProviderHost before opening this exact sealed CUDA image.
                    let status = unsafe {
                        bootstrap(ort::sys::OrtGetApiBase(), held.image_file().as_raw_fd())
                    };
                    ensure!(
                        status == 0,
                        "protected Linux CUDA provider initialization failed ({status})"
                    );
                    Handle::no_load(&row.name)?
                        .context("core did not load the authenticated CUDA provider")?
                } else {
                    Handle::map(held)?
                }
            }
        };
        // Preserve the mapping through subsequent validation and partial-failure cleanup.
        let handle = self.handles.entry(row.name.clone()).or_insert(handle);
        ensure!(
            handle.id()? == expected,
            "glibc handle differs from authenticated Linux CUDA image"
        );
        let by_name = Handle::no_load(&row.name)?.context("ELF SONAME alias unavailable")?;
        ensure!(
            by_name.id()? == expected,
            "ELF SONAME alias points at another image"
        );
        self.reconcile()
    }

    fn load(
        &mut self,
        descriptor: &manifest::linux::Descriptor,
        bootstrap: Bootstrap,
        deadline: Instant,
    ) -> Result<()> {
        // glibc and its loader may form a resident host cycle. For other host
        // imports the resident exact dependencies determine the loading order.
        let mut pending: Vec<_> = descriptor.system_imports().iter().collect();
        while !pending.is_empty() {
            let mut next = Vec::new();
            let count = pending.len();
            for row in pending {
                if Handle::no_load(&row.name)?.is_some()
                    || row.needed.iter().all(|n| self.handles.contains_key(n))
                {
                    self.bind(row, None, deadline)?;
                } else {
                    next.push(row);
                }
            }
            ensure!(
                next.len() < count,
                "unavailable or circular Linux host dependency"
            );
            pending = next;
        }
        for name in descriptor.load_order() {
            let row = descriptor
                .files()
                .iter()
                .find(|r| &r.name == name)
                .context("Linux payload order")?;
            self.bind(
                row,
                (name == "libonnxruntime_providers_cuda.so").then_some(bootstrap),
                deadline,
            )?;
        }
        Ok(())
    }
}

pub(super) struct LoadedRuntime {
    collector: super::placement::Collector,
    assets: Assets,
    driver: Driver,
    driver_image: Sha256Digest,
    product: bool,
    identity: Sha256Digest,
}

struct Attempt(Option<Assets>);

impl Drop for Attempt {
    fn drop(&mut self) {
        if let Some(assets) = self.0.take() {
            FAILED.store(true, Ordering::Release);
            // dlopen may already have run arbitrary package initializers.
            // Keep actual descriptors and handles until process teardown.
            std::mem::forget(assets);
        }
    }
}

fn protected_environment() -> Result<()> {
    ensure!(
        std::env::vars_os().all(|(name, _)| {
            let name = name.to_string_lossy();
            !name.starts_with("LD_")
                && name != "CUDNN_FRONTEND_CUDART_LIB_NAME"
                && name != "GLIBC_TUNABLES"
        }),
        "Linux GPU loader environment can redirect or interpose dependencies"
    );
    Ok(())
}

pub(super) fn admit(
    root: &Path,
    path: &Path,
    deadline: Instant,
) -> Result<(
    &'static LoadedRuntime,
    manifest::linux::Descriptor,
    Sha256Digest,
)> {
    let _exclusive = ADMISSION
        .lock()
        .map_err(|_| anyhow::anyhow!("Linux GPU module admission poisoned"))?;
    ensure!(
        !runtime_poisoned(),
        "prior partial Linux CUDA admission requires restart"
    );
    if option_env!("AIRK_APPROVED_LINUX_CUDA_MANIFEST_SHA256").is_none()
        || option_env!("AIRK_STATIC_LINUX_CUDA_BUILD_SHA256").is_none()
    {
        return Err(RuntimeUnavailable::new(
            "Linux CUDA manifest or static-core approval not compiled",
        )
        .into());
    }
    protected_environment()?;
    in_time(deadline)?;
    ensure!(
        path.parent() == Some(root) && std::env::current_exe()?.parent() == Some(root),
        "Linux CUDA runtime and manifest must be direct siblings of native image"
    );
    let mut description = Held::open(path, false, 128 << 10)?;
    let (descriptor, identity) =
        manifest::linux::Descriptor::approved(&description.metadata_bytes(deadline)?)?;
    if let Some(existing) = LOADED.get() {
        ensure!(
            existing.identity == identity,
            "another Linux CUDA runtime is already mapped"
        );
        existing.recheck(deadline)?;
        return Ok((existing, descriptor, identity));
    }
    let provenance = descriptor.provenance();
    let mut proof = Held::open(&root.join(&provenance.name), false, 1 << 20)?;
    let proof_bytes = proof.metadata_bytes(deadline)?;
    ensure!(
        proof_bytes.len() as u64 == provenance.bytes && proof.sha == provenance.sha256,
        "Linux CUDA provenance differs"
    );
    let legal = descriptor.verify_provenance(&proof_bytes)?;
    let mut sidecars = vec![description, proof];
    for item in legal {
        let mut notice = Held::open(&root.join(&item.name), false, item.bytes.min(16 << 20))?;
        let data = notice.metadata_bytes(deadline)?;
        ensure!(
            data.len() as u64 == item.bytes && notice.sha == item.sha256,
            "Linux CUDA notice differs"
        );
        sidecars.push(notice);
    }
    let main_id = ImageId::file(&File::open("/proc/self/exe")?)?;
    let main = Handle::main_image()?;
    ensure!(
        main.id()? == main_id
            && mappings::identity_at(ort::sys::OrtGetApiBase as *const () as usize)? == main_id,
        "Linux CUDA requires the main executable's single static ORT core"
    );
    let collector = unsafe { main.symbol(c"airk_ort_collect_final_placement_v1", main_id) }
        .context(RuntimeUnavailable::new(
            "Linux final-placement capability absent",
        ))?;
    let mut assets = Assets {
        files: BTreeMap::new(),
        handles: BTreeMap::new(),
        permitted: BTreeSet::from([main_id]),
        sidecars,
    };
    for row in descriptor.system_imports() {
        let path = Held::system_path(&row.name)?;
        if row.name == "libcuda.so.1" && path.parent() == Some(Path::new("/usr/lib/wsl/lib")) {
            ensure!(
                descriptor
                    .system_imports()
                    .iter()
                    .any(|r| r.name == "libdxcore.so"),
                "WSL CUDA proxy requires an authenticated libdxcore companion"
            );
        }
        let held = Held::elf(&path, true, row, deadline)?;
        assets.permitted.insert(ImageId::file(held.image_file())?);
        assets.files.insert(row.name.clone(), held);
    }
    // The preexisting process may only contain the main executable and the
    // exact host file images declared by this descriptor.
    assets.reconcile()?;
    for row in descriptor.files() {
        let held = Held::elf(&root.join(&row.name), false, row, deadline)?;
        assets.permitted.insert(ImageId::file(held.image_file())?);
        assets.files.insert(row.name.clone(), held);
    }
    let driver_image = assets
        .files
        .get("libcuda.so.1")
        .context("Linux host driver absent")?
        .sha;
    crate::onnx::initialize_runtime()?;
    use ort::execution_providers::ExecutionProvider;
    ensure!(
        ort::execution_providers::CUDAExecutionProvider::default().is_available()?,
        "linked Linux native core was not compiled CUDA-capable"
    );
    // Resolve before any optional mapping so CPU-only cores produce a typed refusal.
    let bootstrap: Bootstrap = unsafe { main.symbol(c"airk_ort_bootstrap_linux_cuda_v1", main_id) }
        .context(RuntimeUnavailable::new(
            "Linux protected provider bootstrap absent",
        ))?;
    let mut attempt = Attempt(Some(assets));
    let assets = attempt.0.as_mut().expect("admission owns assets");
    assets.load(&descriptor, bootstrap, deadline)?;
    assets.reconcile()?;
    let handle = &assets.handles["libcuda.so.1"];
    let driver = Driver::new(handle, descriptor.is_product())?;
    driver.initialize()?;
    assets.reconcile()?;
    in_time(deadline)?;
    let loaded = LoadedRuntime {
        collector,
        assets: attempt.0.take().expect("loaded assets"),
        driver,
        driver_image,
        product: descriptor.is_product(),
        identity,
    };
    LOADED
        .set(loaded)
        .map_err(|_| anyhow::anyhow!("Linux CUDA admission ownership raced"))?;
    Ok((
        LOADED.get().expect("stored Linux CUDA runtime"),
        descriptor,
        identity,
    ))
}

impl LoadedRuntime {
    fn recheck(&self, deadline: Instant) -> Result<()> {
        self.assets.reconcile()?;
        for held in self.assets.files.values().chain(&self.assets.sidecars) {
            held.recheck(deadline)?;
        }
        Ok(())
    }
    pub(super) fn placement(
        &self,
        session: &ort::session::Session,
    ) -> Result<PlacementObservation> {
        self.assets.reconcile()?;
        Ok(unsafe { super::placement::observe(self.collector, session) })
    }
    pub(super) fn graph_metadata(
        &self,
        session: &ort::session::Session,
        deadline: Instant,
        placement: &PlacementObservation,
    ) -> Result<super::GraphMetadataObservation> {
        self.assets.reconcile()?;
        let main = Handle::main_image()?;
        let image = ImageId::file(&File::open("/proc/self/exe")?)?;
        let collector = unsafe { main.symbol(c"airk_ort_collect_final_graph_metadata_v1", image) }?;
        let result =
            unsafe { super::graph_metadata::observe(collector, session, deadline, placement) };
        self.assets.reconcile()?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests;
