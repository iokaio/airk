// SPDX-License-Identifier: Apache-2.0
//! Opaque native CUDA witnesses. No serialized value grants execution.
use airk_core::{
    arbiter::{Profile, device::*},
    identity::Sha256Digest,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
#[cfg(any(test, feature = "test-support", all(windows, target_arch = "x86_64")))]
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Instant,
};
#[cfg(feature = "test-support")]
mod custody;
#[cfg(feature = "test-support")]
pub use custody::{
    HistoricalToolCustodyCopy, MetadataCustodyCopy, decode_historical_tool_custody,
    decode_metadata_custody, encode_historical_tool_custody, encode_metadata_custody,
    validate_custody_compatibility, validate_historical_tool_custody, validate_metadata_custody,
};
pub(super) mod diagnostic;
#[cfg(feature = "test-support")]
pub(super) mod graph_metadata;
#[cfg(feature = "test-support")]
pub use diagnostic::{LoadObserver, LoadSnapshot};
#[cfg(feature = "test-support")]
pub use graph_metadata::{Observation as GraphMetadataObservation, Row as GraphMetadataRow};
mod eligibility;
mod manifest;
mod numeric;
pub use numeric::compare_reference_scores;
#[cfg(any(test, all(windows, target_arch = "x86_64")))]
mod provenance;
pub use eligibility::{DeviceEligibility, EligibilityKey, is_device_eligibility_failure};
pub use manifest::product_compatibility_policy_digest;
mod run;
pub(super) use run::score as score_gpu;
#[cfg(any(test, all(windows, target_arch = "x86_64")))]
mod context;
#[cfg(any(test, all(windows, target_arch = "x86_64")))]
mod modules;
#[cfg(any(test, all(windows, target_arch = "x86_64")))]
mod pe;
mod placement;
#[cfg(any(test, all(windows, target_arch = "x86_64")))]
mod security;
pub use manifest::{compatibility_policy_digest, math_policy_digest, provider_options_digest};
pub use placement::{PlacedNode, PlacementObservation, PlacementRefusal};
#[cfg(test)]
mod tests;
#[cfg(all(windows, target_arch = "x86_64"))]
mod windows;
static NEXT_EPOCH: AtomicU64 = AtomicU64::new(1);
/// Actual native poison observation, including admission failures before a
/// CudaDevice could be returned. This never grants execution authority.
pub fn process_poisoned() -> bool {
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        windows::runtime_poisoned()
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        false
    }
}

// Keep the original typed cause and its display first when cleanup also fails.
fn preserve_primary(error: anyhow::Error, secondary: impl std::fmt::Display) -> anyhow::Error {
    let summary = format!("{error:#}; {secondary}");
    error.context(summary)
}

pub(super) fn in_time(deadline: Instant) -> Result<()> {
    ensure!(
        Instant::now() < deadline,
        "original GPU operation deadline expired"
    );
    Ok(())
}
#[derive(Clone, Debug, Serialize)]
pub struct AdapterIdentity {
    pub uuid: [u8; 16],
    pub ordinal: i32,
    pub compute_major: i32,
    pub compute_minor: i32,
    pub driver_api_version: i32,
    pub driver_image_sha256: Sha256Digest,
    pub physical_total_bytes: u64,
    pub dedicated_memory: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct AdapterObservation {
    pub uuid: [u8; 16],
    pub ordinal: i32,
    pub compute_major: i32,
    pub compute_minor: i32,
    pub driver_api_version: i32,
    pub driver_image_sha256: Sha256Digest,
    pub physical_total_bytes: u64,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub epoch: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dedicated_memory: Option<bool>,
}
impl AdapterObservation {
    fn identity(&self) -> AdapterIdentity {
        AdapterIdentity {
            uuid: self.uuid,
            ordinal: self.ordinal,
            compute_major: self.compute_major,
            compute_minor: self.compute_minor,
            driver_api_version: self.driver_api_version,
            driver_image_sha256: self.driver_image_sha256,
            physical_total_bytes: self.physical_total_bytes,
            dedicated_memory: self.dedicated_memory == Some(true),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DeviceCapacityPlan {
    cold_peak_bytes: u64,
    warm_extra_bytes: u64,
    headroom_bytes: u64,
}
impl DeviceCapacityPlan {
    fn new(cold: u64, warm: u64, headroom: u64) -> Result<Self> {
        ensure!(
            cold > 0 && headroom > 0 && warm <= cold,
            "invalid GPU phase capacity requirements"
        );
        cold.checked_add(headroom)
            .context("GPU cold capacity overflow")?;
        Ok(Self {
            cold_peak_bytes: cold,
            warm_extra_bytes: warm,
            headroom_bytes: headroom,
        })
    }
    pub fn cold_peak_bytes(&self) -> u64 {
        self.cold_peak_bytes
    }
    pub fn warm_extra_bytes(&self) -> u64 {
        self.warm_extra_bytes
    }
    pub fn headroom_bytes(&self) -> u64 {
        self.headroom_bytes
    }
    #[cfg(any(test, feature = "test-support"))]
    pub fn probe(cold: u64, warm: u64, headroom: u64) -> Result<Self> {
        Self::new(cold, warm, headroom)
    }
}
#[derive(Default)]
struct Claims {
    next: u64,
    unspent: BTreeMap<u64, (u64, u64)>,
}
impl Claims {
    fn reserve(&mut self, free: u64, bytes: u64, headroom: u64) -> Result<u64> {
        ensure!(headroom > 0, "GPU claim must have explicit bounds");
        let pending = self.unspent.values().try_fold(0u64, |a, b| {
            a.checked_add(b.0).context("GPU claims overflow")
        })?;
        let required = pending
            .checked_add(bytes)
            .and_then(|v| {
                v.checked_add(
                    self.unspent
                        .values()
                        .map(|v| v.1)
                        .max()
                        .unwrap_or(0)
                        .max(headroom),
                )
            })
            .context("GPU capacity overflow")?;
        if free < required {
            return Err(capacity_fault(format!(
                "actual free VRAM {free} cannot cover pending work+headroom {required}"
            )));
        }
        self.next = self
            .next
            .checked_add(1)
            .context("GPU claim generation overflow")?;
        self.unspent.insert(self.next, (bytes, headroom));
        Ok(self.next)
    }
}
/// No Deserialize, no public field constructor. Host retains the actual service lease.
#[cfg(all(windows, target_arch = "x86_64"))]
static RUNTIME_OWNER: Mutex<Weak<CudaRuntime>> = Mutex::new(Weak::new());

#[cfg(any(test, all(windows, target_arch = "x86_64")))]
fn same_native_lease(a: &Arc<dyn Send + Sync>, b: &Arc<dyn Send + Sync>) -> bool {
    Arc::ptr_eq(a, b)
}

pub struct CudaRuntime {
    #[cfg(all(windows, target_arch = "x86_64"))]
    loaded: &'static windows::LoadedRuntime,
    manifest: manifest::RuntimeDescriptor,
    identity: Sha256Digest,
    _native_lease: Arc<dyn Send + Sync>,
    devices: Mutex<BTreeMap<[u8; 16], Weak<CudaDevice>>>,
}
impl CudaRuntime {
    /// # Safety
    /// Host must pass its actual exclusive native ServiceLease and retain the
    /// ordinary ServiceComputeScope. No caller metadata/flags can replace it.
    pub unsafe fn from_native_lease(
        lease: Arc<dyn Send + Sync>,
        root: &Path,
        manifest_path: &Path,
        deadline: Instant,
    ) -> Result<Arc<Self>> {
        crate::native_inference::require_service_compute()?;
        in_time(deadline)?;
        #[cfg(all(windows, target_arch = "x86_64"))]
        {
            let mut owner = RUNTIME_OWNER
                .lock()
                .map_err(|_| anyhow::anyhow!("native CUDA owner cache poisoned"))?;
            let existing = owner.upgrade();
            if let Some(actual) = &existing {
                ensure!(
                    same_native_lease(&actual._native_lease, &lease),
                    "CUDA runtime already belongs to another native service lease"
                );
            }
            let (loaded, manifest, identity) = windows::admit(root, manifest_path, deadline)?;
            if let Some(actual) = existing {
                ensure!(actual.identity == identity, "CUDA owner manifest changed");
                return Ok(actual);
            }

            in_time(deadline)?;
            let actual = Arc::new(Self {
                loaded,
                manifest,
                identity,
                _native_lease: lease,
                devices: Mutex::new(BTreeMap::new()),
            });
            *owner = Arc::downgrade(&actual);
            Ok(actual)
        }
        #[cfg(not(all(windows, target_arch = "x86_64")))]
        {
            let _ = (lease, root, manifest_path);
            anyhow::bail!(RuntimeUnavailable::new("unsupported platform"))
        }
    }
    pub fn runtime_build(&self) -> Sha256Digest {
        self.manifest.build()
    }
    pub fn manifest_identity(&self) -> Sha256Digest {
        self.identity
    }
    pub fn is_product(&self) -> bool {
        self.manifest.is_product()
    }
    pub fn discover_adapters(&self, deadline: Instant) -> Result<Vec<AdapterIdentity>> {
        crate::native_inference::require_service_compute()?;
        in_time(deadline)?;
        ensure!(
            self.is_product(),
            "metadata-only discovery is the product /2 path"
        );
        #[cfg(all(windows, target_arch = "x86_64"))]
        {
            self.loaded.discover(deadline)
        }
        #[cfg(not(all(windows, target_arch = "x86_64")))]
        {
            anyhow::bail!("CUDA product discovery unavailable")
        }
    }
    /// Reports physical fit for an adapter that matches the exact product profile.
    /// Incompatible or unauthenticated pairs return None. This performs no load.
    pub fn product_adapter_physical_fit(
        &self,
        profile: &Profile,
        adapter: &AdapterIdentity,
    ) -> Option<bool> {
        let row = self.manifest.requirement(profile).ok()?;
        let policy = profile.device_execution.as_ref()?;
        (row.matches_adapter_class(adapter)
            && policy.runtime_build == self.runtime_build()
            && row.compatibility_policy_digest().ok()? == policy.compatibility_policy)
            .then(|| row.supports(adapter).is_ok())
    }
    pub fn adapters(&self, deadline: Instant) -> Result<Vec<AdapterObservation>> {
        crate::native_inference::require_service_compute()?;
        in_time(deadline)?;
        ensure!(
            !self.is_product(),
            "product discovery must not retain contexts on unselected adapters"
        );
        #[cfg(all(windows, target_arch = "x86_64"))]
        {
            self.loaded.adapters(deadline, self._native_lease.clone())
        }
        #[cfg(not(all(windows, target_arch = "x86_64")))]
        {
            anyhow::bail!("CUDA unavailable")
        }
    }
    pub fn device(self: &Arc<Self>, uuid: [u8; 16], deadline: Instant) -> Result<Arc<CudaDevice>> {
        let mut devices = self
            .devices
            .lock()
            .map_err(|_| anyhow::anyhow!("GPU adapter owner poisoned"))?;
        if let Some(device) = devices.get(&uuid).and_then(Weak::upgrade) {
            device.observation(deadline)?;
            return Ok(device);
        }
        #[cfg(all(windows, target_arch = "x86_64"))]
        let (observed, context) = if self.is_product() {
            let id = self
                .discover_adapters(deadline)?
                .into_iter()
                .find(|v| v.uuid == uuid)
                .context("native product adapter UUID unavailable")?;
            ensure!(
                self.manifest.product_has_adapter_class(&id),
                "actual adapter has no authenticated product architecture requirement"
            );
            let context = self
                .loaded
                .retain_primary(id.ordinal, self._native_lease.clone())?;
            let observed = self.loaded.observe(id.ordinal, &context, deadline)?;
            ensure!(
                observed.uuid == uuid
                    && observed.compute_major == id.compute_major
                    && observed.compute_minor == id.compute_minor
                    && self
                        .manifest
                        .product_has_adapter_class(&observed.identity()),
                "actual selected adapter changed during context creation"
            );
            (observed, context)
        } else {
            let observed = self
                .adapters(deadline)?
                .into_iter()
                .find(|v| v.uuid == uuid)
                .context("native adapter UUID unavailable")?;
            ensure!(
                observed.compute_major == 8 && observed.compute_minor == 6,
                "CUDA compute capability differs from compiled SM86 native policy"
            );
            let context = self
                .loaded
                .retain_primary(observed.ordinal, self._native_lease.clone())?;
            (observed, context)
        };
        #[cfg(not(all(windows, target_arch = "x86_64")))]
        let observed = self
            .adapters(deadline)?
            .into_iter()
            .find(|v| v.uuid == uuid)
            .context("native adapter UUID unavailable")?;
        if !self.is_product() {
            ensure!(
                observed.compute_major == 8 && observed.compute_minor == 6,
                "CUDA compute capability differs from compiled SM86 native policy"
            );
        }
        let epoch = NEXT_EPOCH
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| v.checked_add(1))
            .map_err(|_| anyhow::anyhow!("GPU capability epoch exhausted"))?;
        let device = Arc::new(CudaDevice {
            runtime: self.clone(),
            observed,
            epoch: AtomicU64::new(epoch),
            poisoned: AtomicBool::new(false),
            claims: Mutex::new(Claims::default()),
            #[cfg(all(windows, target_arch = "x86_64"))]
            context,
        });
        devices.insert(uuid, Arc::downgrade(&device));
        Ok(device)
    }
}
pub struct CudaDevice {
    runtime: Arc<CudaRuntime>,
    observed: AdapterObservation,
    epoch: AtomicU64,
    poisoned: AtomicBool,
    claims: Mutex<Claims>,
    #[cfg(all(windows, target_arch = "x86_64"))]
    context: windows::PrimaryContext,
}
impl CudaDevice {
    pub fn uuid(&self) -> [u8; 16] {
        self.observed.uuid
    }
    pub fn pending_claims(&self) -> Option<usize> {
        self.claims.lock().ok().map(|c| c.unspent.len())
    }
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }
    pub fn poisoned(&self) -> bool {
        self.poisoned.load(Ordering::Acquire) || {
            #[cfg(all(windows, target_arch = "x86_64"))]
            {
                windows::runtime_poisoned()
            }
            #[cfg(not(all(windows, target_arch = "x86_64")))]
            {
                false
            }
        }
    }
    #[cfg(any(feature = "test-support", all(windows, target_arch = "x86_64")))]
    fn poison(&self) {
        self.poisoned.store(true, Ordering::Release);
        self.epoch.fetch_add(1, Ordering::AcqRel);
    }
    pub fn observation(&self, deadline: Instant) -> Result<AdapterObservation> {
        crate::native_inference::require_service_compute()?;
        in_time(deadline)?;
        ensure!(!self.poisoned(), "CUDA adapter is poisoned");
        #[cfg(all(windows, target_arch = "x86_64"))]
        {
            let result =
                self.runtime
                    .loaded
                    .observe(self.observed.ordinal, &self.context, deadline);
            match result {
                Ok(mut value) => {
                    if value.uuid != self.observed.uuid
                        || value.driver_api_version != self.observed.driver_api_version
                        || value.driver_image_sha256 != self.observed.driver_image_sha256
                        || value.compute_major != self.observed.compute_major
                        || value.compute_minor != self.observed.compute_minor
                        || value.physical_total_bytes != self.observed.physical_total_bytes
                        || value.dedicated_memory != self.observed.dedicated_memory
                    {
                        self.poison();
                        anyhow::bail!("CUDA adapter/driver epoch changed");
                    }
                    value.epoch = self.epoch();
                    Ok(value)
                }
                Err(error) => {
                    self.poison();
                    Err(error)
                }
            }
        }
        #[cfg(not(all(windows, target_arch = "x86_64")))]
        {
            anyhow::bail!("CUDA unavailable")
        }
    }
    pub fn synchronize(&self, deadline: Instant) -> Result<()> {
        #[cfg(all(windows, target_arch = "x86_64"))]
        {
            // Cleanup is never skipped merely because the original deadline expired.
            if let Err(error) = self.runtime.loaded.synchronize(&self.context) {
                self.poison();
                return Err(error);
            }
            in_time(deadline)
        }
        #[cfg(not(all(windows, target_arch = "x86_64")))]
        {
            let _ = deadline;
            anyhow::bail!("CUDA unavailable")
        }
    }
    fn reserve(
        self: &Arc<Self>,
        bytes: u64,
        headroom: u64,
        deadline: Instant,
    ) -> Result<DeviceReservation> {
        let mut claims = self
            .claims
            .lock()
            .map_err(|_| anyhow::anyhow!("GPU capacity owner poisoned"))?;
        let observed = self.observation(deadline)?;
        let id = claims.reserve(observed.free_bytes, bytes, headroom)?;
        Ok(DeviceReservation {
            device: self.clone(),
            id: Some(id),
            deadline,
            attempted: false,
            _thread: std::marker::PhantomData,
        })
    }
    pub fn reserve_cold_load(
        self: &Arc<Self>,
        bytes: u64,
        headroom: u64,
        deadline: Instant,
    ) -> Result<DeviceReservation> {
        self.reserve(bytes, headroom, deadline)
    }
    pub fn reserve_warm_work(
        self: &Arc<Self>,
        extra_bytes: u64,
        headroom: u64,
        deadline: Instant,
    ) -> Result<DeviceReservation> {
        self.reserve(extra_bytes, headroom, deadline)
    }
    pub(super) fn validate_profile(&self, profile: &Profile) -> Result<()> {
        ensure!(!self.poisoned(), "CUDA adapter is poisoned");
        let policy = profile
            .device_execution
            .as_ref()
            .context("CPU profile cannot acquire CUDA execution")?;
        policy.validate(profile)?;
        ensure!(
            policy.backend == GpuBackend::Cuda
                && policy.precision == DevicePrecision::Float32
                && !policy.tf32
                && policy.placement == PlacementPolicy::StrictGpu
                && policy.runtime_build == self.runtime.runtime_build()
                && policy.compatibility_policy
                    == if self.runtime.manifest.is_product() {
                        self.runtime
                            .manifest
                            .requirement(profile)?
                            .compatibility_policy_digest()?
                    } else {
                        compatibility_policy_digest()
                    }
                && policy.provider_options == provider_options_digest()
                && policy.math_policy == math_policy_digest(),
            "GPU model/runtime/policy/precision is not the native FP32 strict variant"
        );
        Ok(())
    }
    pub(super) fn qualified_plan(&self, profile: &Profile) -> Result<DeviceCapacityPlan> {
        self.validate_profile(profile)?;
        if self.runtime.manifest.is_product() {
            let row = self.runtime.manifest.requirement(profile)?;
            row.supports(&self.observed.identity())?;
            return DeviceCapacityPlan::new(
                row.cold_peak_limit_bytes,
                row.warm_extra_limit_bytes,
                row.headroom_bytes,
            );
        }
        let identity = profile.device_execution_identity()?;
        let fit = self
            .runtime
            .manifest
            .fits()?
            .iter()
            .find(|v| v.manifest == profile.manifest && v.execution_identity == identity)
            .context("exact native variant has no compiled measured phase capacity")?;
        ensure!(
            matches!(&profile.device_execution.as_ref().unwrap().memory,DeviceMemory::Dedicated{required_peak_bytes,headroom_bytes} if *required_peak_bytes==fit.cold_peak_bytes&&*headroom_bytes==fit.headroom_bytes),
            "native measured phase fit differs from immutable device requirements"
        );
        ensure!(
            fit.adapter_uuid == self.observed.uuid
                && fit.driver_api_version == self.observed.driver_api_version
                && fit.driver_image_sha256 == self.observed.driver_image_sha256,
            "GPU fit is not qualified for the actual adapter/driver"
        );
        DeviceCapacityPlan::new(
            fit.cold_peak_bytes,
            fit.warm_extra_bytes,
            fit.headroom_bytes,
        )
    }
    #[cfg(feature = "test-support")]
    pub(super) fn graph_metadata(
        &self,
        session: &ort::session::Session,
        deadline: Instant,
        placement: &PlacementObservation,
    ) -> Result<GraphMetadataObservation> {
        in_time(deadline)?;
        #[cfg(all(windows, target_arch = "x86_64"))]
        {
            self.runtime
                .loaded
                .graph_metadata(session, deadline, placement)
        }
        #[cfg(not(all(windows, target_arch = "x86_64")))]
        {
            let _ = (session, placement);
            anyhow::bail!("final graph metadata unavailable")
        }
    }
    pub(super) fn placement(
        &self,
        session: &ort::session::Session,
    ) -> Result<PlacementObservation> {
        #[cfg(all(windows, target_arch = "x86_64"))]
        {
            self.runtime.loaded.placement(session)
        }
        #[cfg(not(all(windows, target_arch = "x86_64")))]
        {
            let _ = session;
            anyhow::bail!("CUDA placement unavailable")
        }
    }
    pub(super) fn ordinal(&self) -> i32 {
        self.observed.ordinal
    }
}
/// A pending future-work charge, not a resident allocation or model permit.
pub struct DeviceReservation {
    device: Arc<CudaDevice>,
    id: Option<u64>,
    deadline: Instant,
    attempted: bool,
    _thread: std::marker::PhantomData<std::rc::Rc<()>>,
}
impl DeviceReservation {
    fn finish(&mut self) -> Result<()> {
        self.attempted = true;
        let result = self.device.synchronize(self.deadline);
        if !self.device.poisoned()
            && let Some(id) = self.id.take()
        {
            self.device
                .claims
                .lock()
                .map_err(|_| anyhow::anyhow!("GPU capacity owner poisoned"))?
                .unspent
                .remove(&id);
        }
        result
    }
    pub fn settle(mut self) -> Result<()> {
        self.finish()
    }
}
impl Drop for DeviceReservation {
    fn drop(&mut self) {
        if self.id.is_some() {
            if !self.attempted {
                let _ = self.finish();
            }
            if self.device.poisoned() {
                std::mem::forget(self.device.clone());
            }
        }
    }
}

#[derive(Clone)]
pub(super) struct Binding {
    pub device: Arc<CudaDevice>,
    pub plan: DeviceCapacityPlan,
    pub load_deadline: Instant,
    pub evidence: Option<DeviceEvidence>,
    pub host_warm_extra_bytes: Option<u64>,
    log: Arc<Mutex<diagnostic::Capture>>,
    admission: Arc<Mutex<eligibility::AdmissionState>>,
    session_alive: Arc<AtomicBool>,
}
impl Binding {
    pub fn qualified(
        device: Arc<CudaDevice>,
        profile: &Profile,
        deadline: Instant,
    ) -> Result<Self> {
        in_time(deadline)?;
        let plan = device.qualified_plan(profile)?;
        let (evidence, host) = if device.runtime.manifest.is_product() {
            let row = device.runtime.manifest.requirement(profile)?;
            (
                row.artifact_evidence.clone(),
                row.host_warm_extra_limit_bytes,
            )
        } else {
            let fit = device
                .runtime
                .manifest
                .fits()?
                .iter()
                .find(|v| {
                    v.manifest == profile.manifest
                        && v.execution_identity
                            == profile.device_execution_identity().unwrap_or_default()
                })
                .context("GPU scientific variant fit missing")?;
            ensure!(
                fit.host_warm_extra_bytes <= profile.memory_reservation_bytes,
                "scientific host extra exceeds profile peak"
            );
            (fit.evidence.clone(), fit.host_warm_extra_bytes)
        };
        let admission = eligibility::initial_admission(&device.runtime.manifest);
        Ok(Self {
            device,
            plan,
            load_deadline: deadline,
            evidence: Some(evidence),
            host_warm_extra_bytes: Some(host),
            log: Arc::new(Mutex::new(diagnostic::Capture::default())),
            admission: Arc::new(Mutex::new(admission)),
            session_alive: Arc::new(AtomicBool::new(true)),
        })
    }
    #[cfg(any(test, feature = "test-support"))]
    pub fn probe(
        device: Arc<CudaDevice>,
        profile: &Profile,
        plan: DeviceCapacityPlan,
        deadline: Instant,
    ) -> Result<Self> {
        in_time(deadline)?;
        device.validate_profile(profile)?;
        ensure!(
            profile.qualification.is_none(),
            "probe cannot relabel release qualification"
        );
        Ok(Self {
            device,
            plan,
            load_deadline: deadline,
            evidence: None,
            host_warm_extra_bytes: None,
            log: Arc::new(Mutex::new(diagnostic::Capture::default())),
            admission: Arc::new(Mutex::new(eligibility::AdmissionState::Scientific)),
            session_alive: Arc::new(AtomicBool::new(true)),
        })
    }
    pub fn configure(
        &self,
        builder: ort::session::builder::SessionBuilder,
    ) -> Result<ort::session::builder::SessionBuilder> {
        in_time(self.load_deadline)?;
        let log = self.log.clone();
        let builder = builder
            .with_execution_providers([ort::execution_providers::CUDAExecutionProvider::default()
                .with_device_id(self.device.ordinal())
                .with_tf32(false)
                .with_cuda_graph(false)
                .with_memory_limit(usize::try_from(self.plan.cold_peak_bytes)?)
                .build()
                .error_on_failure()])
            .map_err(run_failure)?;
        Ok(builder
            .with_config_entry("session.disable_cpu_ep_fallback", "1")?
            .with_log_level(ort::logging::LogLevel::Verbose)?
            .with_log_verbosity(1)?
            .with_logger(Box::new(
                move |_level, _category, _id, location, message| {
                    // Native bounded observations only, never a placement permit.
                    if let Ok(mut sink) = log.lock() {
                        sink.push(location, message);
                    }
                },
            ))?)
    }
}

/// Metadata-only native factory input. Its output is unapproved until the
/// genuine receipt/PE closure is reviewed and its exact SHA is compiled.
#[cfg(feature = "test-support")]
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeAssemblyInput {
    pub directory: std::path::PathBuf,
    pub producer_name: String,
    pub producer_sha256: Sha256Digest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpointed_proof: Option<airk_core::arbiter::resource_policy::ResourceFilePin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incremental_proof: Option<airk_core::arbiter::resource_policy::ResourceFilePin>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custody_copies: Vec<MetadataCustodyCopy>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub historical_tool_copies: Vec<HistoricalToolCustodyCopy>,
    pub module_names: Vec<String>,
    pub load_order: Vec<String>,
    pub system_imports: BTreeSet<String>,
}
#[cfg(feature = "test-support")]
/// # Safety
/// The caller retains its actual ServiceLease/ServiceComputeScope while observing
/// this private build closure. This creates no runtime, device or execution permit.
pub unsafe fn observe_runtime_assembly(
    _native_lease: Arc<dyn Send + Sync>,
    input: RuntimeAssemblyInput,
    deadline: Instant,
) -> Result<Vec<u8>> {
    crate::native_inference::require_service_compute()?;
    in_time(deadline)?;
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        windows::observe_assembly(input, deadline)
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        let _ = input;
        anyhow::bail!("CUDA assembly factory requires Windows x64")
    }
}

#[derive(Debug)]
pub(crate) struct NativeDeviceFailure {
    message: String,
    capacity: bool,
    source: Option<ort::Error>,
}
impl std::fmt::Display for NativeDeviceFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for NativeDeviceFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|v| v as _)
    }
}
pub(super) fn native_fault(message: String) -> anyhow::Error {
    NativeDeviceFailure {
        message,
        capacity: false,
        source: None,
    }
    .into()
}
pub(super) fn capacity_fault(message: String) -> anyhow::Error {
    NativeDeviceFailure {
        message,
        capacity: true,
        source: None,
    }
    .into()
}
/// Genuine native free-capacity/CUDA OOM facts only; no message matching.
pub fn is_device_capacity_failure(error: &anyhow::Error) -> bool {
    error.chain().any(|v| {
        v.downcast_ref::<NativeDeviceFailure>()
            .is_some_and(|v| v.capacity)
    })
}
pub(super) fn run_failure(error: ort::Error) -> anyhow::Error {
    use ort::ErrorCode;
    if matches!(
        error.code(),
        ErrorCode::GenericFailure
            | ErrorCode::EngineError
            | ErrorCode::RuntimeException
            | ErrorCode::NotImplemented
            | ErrorCode::ExecutionProviderFailure
    ) {
        NativeDeviceFailure {
            message: "native CUDA/ORT execution failed".into(),
            capacity: false,
            source: Some(error),
        }
        .into()
    } else {
        error.into()
    }
}
impl CudaDevice {
    pub fn driver_api_version(&self) -> i32 {
        self.observed.driver_api_version
    }
    pub fn driver_image_sha256(&self) -> Sha256Digest {
        self.observed.driver_image_sha256
    }
}
impl CudaRuntime {
    pub fn poisoned(&self) -> bool {
        self.devices.is_poisoned() || {
            #[cfg(all(windows, target_arch = "x86_64"))]
            {
                windows::runtime_poisoned()
            }
            #[cfg(not(all(windows, target_arch = "x86_64")))]
            {
                false
            }
        }
    }
}

#[derive(Debug)]
pub struct RuntimeUnavailable {
    reason: &'static str,
}
impl RuntimeUnavailable {
    pub(super) fn new(reason: &'static str) -> Self {
        Self { reason }
    }
}
impl std::fmt::Display for RuntimeUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GPU runtime unavailable: {}", self.reason)
    }
}
impl std::error::Error for RuntimeUnavailable {}

/// New native metadata artifacts. The caller must retain/write these exact bytes
/// and use the updated file map before constructing the actual EvaluationCatalog.
#[cfg(feature = "test-support")]
#[derive(Debug)]
pub struct PreparedFp32ProbeProfile {
    pub profile: Profile,
    pub calibration_name: String,
    pub calibration_bytes: Vec<u8>,
}

#[cfg(any(test, feature = "test-support"))]
fn unavailable_classes() -> Vec<airk_core::arbiter::context_action::calibration::ClassDisposition> {
    use airk_core::arbiter::context_action::calibration::{
        ClassDisposition, ClassSupport, ContextActionClass,
    };
    [
        ContextActionClass::Read,
        ContextActionClass::Sufficient,
        ContextActionClass::Clarify,
        ContextActionClass::NoJustifiedRead,
    ]
    .into_iter()
    .map(|action| ClassDisposition {
        action,
        support: ClassSupport::Unsupported {
            reason: "No CUDA whole-card/class qualification; native raw-score probe only".into(),
        },
    })
    .collect()
}

#[cfg(any(test, feature = "test-support"))]
fn probe_runtime_identity(
    runtime_manifest: &[u8],
    runtime_manifest_sha256: Sha256Digest,
) -> Result<Sha256Digest> {
    // Metadata only: a parsed native manifest is not execution authority. The
    // actual lease/device/approved build must still bind in the inference lane.
    ensure!(
        runtime_manifest.len() <= 128 << 10,
        "runtime metadata byte cap"
    );
    ensure!(
        Sha256Digest::from_bytes(Sha256::digest(runtime_manifest).into())
            == runtime_manifest_sha256,
        "native runtime metadata pin mismatch"
    );
    let runtime: manifest::RuntimeManifest = serde_json::from_slice(runtime_manifest)?;
    runtime.validate()?;
    ensure!(
        runtime.fits.is_empty() && runtime.static_core_build != Sha256Digest::from_bytes([0; 32]),
        "first probe requires unqualified native runtime metadata"
    );
    Ok(runtime.static_core_build)
}

/// Data-only, unqualified first FP32 probe metadata. Does not mint evaluation,
/// device ownership, execution, fitted CAL or release authority.
#[cfg(feature = "test-support")]
pub fn prepare_fp32_probe_profile(
    cpu: &Profile,
    source: &dyn super::AssetSource,
    runtime_manifest: &[u8],
    runtime_manifest_sha256: Sha256Digest,
    plan: DeviceCapacityPlan,
) -> Result<PreparedFp32ProbeProfile> {
    use airk_core::arbiter::{AssetRole, ModelChoice};
    let runtime_build = probe_runtime_identity(runtime_manifest, runtime_manifest_sha256)?;
    cpu.validate()?;
    super::validate_selected_policies(cpu)?;
    ensure!(
        cpu.device_execution.is_none()
            && if cpu.model == ModelChoice::FocusQwenReranker06B {
                cpu.max_tokens == 512 && cpu.max_candidates == 4
            } else {
                cpu.max_tokens == 8192 && cpu.max_candidates == 8
            },
        "probe requires current native8192/N8 CPU description"
    );
    let (name, graph, threads) = match cpu.model {
        ModelChoice::FocusQwenReranker06B => (
            "private-cuda-fp32-focus-probe-v1",
            "5072c663bb8fbc587208dea2564c8ad27880177875e62633f12ea64250ec2738",
            4,
        ),
        ModelChoice::ContextQwenCompact => (
            "private-cuda-fp32-context-compact-probe-v1",
            "c79c672513b874ab773ee7df8d4518e110d1e6196ae45076ce718f5b5dc6f7b4",
            4,
        ),
        ModelChoice::ContextQwenLarge => (
            "private-cuda-fp32-context-large-probe-v1",
            "f8ba5d3ea8a4269739ec5e517e61e4ec1f2f94560ea60014a4f5a9476c32fd94",
            16,
        ),
        _ => anyhow::bail!("first FP32 probe is original unfused Context only"),
    };
    let models = cpu
        .assets
        .iter()
        .filter(|a| a.role == AssetRole::Model)
        .collect::<Vec<_>>();
    ensure!(
        models.len() == 1 && models[0].sha256 == graph.parse()?,
        "probe cannot replace original FP32 graph"
    );
    let calibrations = cpu
        .assets
        .iter()
        .filter(|a| a.role == AssetRole::Calibration)
        .collect::<Vec<_>>();
    ensure!(calibrations.len() == 1, "native raw calibration missing");
    let raw_calibration = super::bytes(source, calibrations[0], 64 << 10)?;
    let mut calibration_bytes = if cpu.model == ModelChoice::FocusQwenReranker06B {
        // Fresh native raw relevance metadata, not learned calibration or a
        // Context action policy. The authenticated old bytes are not reused.
        super::qualification::focus_raw_metadata(
            cpu,
            cpu.task_policies[&airk_core::arbiter::Task::FocusRank],
        )?
    } else {
        let mut calibration =
            airk_core::arbiter::context_action::calibration::JointCalibration::parse_and_validate(
                &raw_calibration,
                cpu,
                crate::arbiter::context_action::policy_digest()?,
            )?;
        calibration.schema =
            airk_core::arbiter::context_action::calibration::CALIBRATION_SCHEMA_V2.into();
        calibration.rules.clear();
        calibration.class_support = Some(unavailable_classes());
        serde_json::to_vec_pretty(&calibration)?
    };
    calibration_bytes.push(b'\n');
    let calibration_sha = Sha256Digest::from_bytes(Sha256::digest(&calibration_bytes).into());
    let calibration_name = calibrations[0].name.clone();
    let mut profile = cpu.clone();
    let host_fit = profile.cpu_fit.take().context("native host fit required")?;
    ensure!(
        host_fit.intra_threads == threads && host_fit.minimum_physical_cores == threads,
        "original selected host CPU tier changed"
    );
    profile.profile_id = name.into();
    profile.runtime = GpuBackend::Cuda.runtime().into();
    profile.downloads.clear();
    profile.qualified_targets.clear();
    profile.qualification = None;
    let asset = profile
        .assets
        .iter_mut()
        .find(|a| a.role == AssetRole::Calibration)
        .context("native calibration asset missing")?;
    asset.bytes = calibration_bytes.len() as u64;
    asset.sha256 = calibration_sha;
    profile.calibration = calibration_sha;
    profile.device_execution = Some(DeviceExecution {
        schema: DEVICE_EXECUTION_SCHEMA.into(),
        backend: GpuBackend::Cuda,
        runtime_build,
        compatibility_policy: compatibility_policy_digest(),
        provider_options: provider_options_digest(),
        math_policy: math_policy_digest(),
        precision: DevicePrecision::Float32,
        tf32: false,
        placement: PlacementPolicy::StrictGpu,
        graph: models[0].sha256,
        host_fit,
        memory: DeviceMemory::Dedicated {
            required_peak_bytes: plan.cold_peak_bytes,
            headroom_bytes: plan.headroom_bytes,
        },
    });
    profile.manifest = Sha256Digest::from_bytes([0; 32]);
    let canonical = serde_json::to_vec(&(
        cpu.manifest,
        runtime_manifest_sha256,
        &profile,
        plan.cold_peak_bytes,
        plan.warm_extra_bytes,
        plan.headroom_bytes,
    ))?;
    profile.manifest = airk_core::identity::framed_digest(
        b"airk.private.cuda-fp32-probe-profile/1",
        &[&canonical],
    );
    profile.validate()?;
    super::validate_selected_policies(&profile)?;
    if profile.model != ModelChoice::FocusQwenReranker06B {
        airk_core::arbiter::context_action::calibration::JointCalibration::parse_and_validate(
            &calibration_bytes,
            &profile,
            crate::arbiter::context_action::policy_digest()?,
        )?;
    }
    Ok(PreparedFp32ProbeProfile {
        profile,
        calibration_name,
        calibration_bytes,
    })
}

/// Metadata-only opaque Rust projection. Its output creates no runtime/device permit.
/// # Safety
/// Caller passes the actual retained ServiceLease and existing native compute scope;
/// every path is an approved pinned source under the original caller deadline.
#[cfg(feature = "test-support")]
pub unsafe fn observe_public_runtime_provenance(
    _lease: Arc<dyn Send + Sync>,
    raw: &[u8],
    deadline: Instant,
) -> Result<Vec<u8>> {
    crate::native_inference::require_service_compute()?;
    in_time(deadline)?;
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        windows::project_public(raw, deadline)
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        let _ = raw;
        anyhow::bail!("held public projection factory requires Windows x64")
    }
}

#[cfg(feature = "test-support")]
fn package_request(root: &Path, descriptor: &Path, digest: Sha256Digest) -> Result<()> {
    let name = descriptor
        .file_name()
        .and_then(std::ffi::OsStr::to_str)
        .context("package descriptor basename missing")?;
    ensure!(
        root.is_absolute()
            && descriptor.parent() == Some(root)
            && digest != Sha256Digest::from_bytes([0; 32])
            && name.len() <= 128
            && !name.is_empty()
            && name == name.to_ascii_lowercase()
            && !name.contains("..")
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')),
        "package descriptor must be pinned direct safe child"
    );
    Ok(())
}
/// Descriptive held native package projection only; no activation or execution permit.
/// # Safety
/// Caller retains its actual exclusive ServiceLease/compute scope throughout.
#[cfg(feature = "test-support")]
pub unsafe fn observe_runtime_package_projection(
    _native_lease: Arc<dyn Send + Sync>,
    root: &Path,
    descriptor: &Path,
    descriptor_sha256: Sha256Digest,
    deadline: Instant,
) -> Result<Vec<u8>> {
    crate::native_inference::require_service_compute()?;
    in_time(deadline)?;
    package_request(root, descriptor, descriptor_sha256)?;
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        windows::observe_package_projection(root, descriptor, descriptor_sha256, deadline)
    }
    #[cfg(not(all(windows, target_arch = "x86_64")))]
    {
        anyhow::bail!("native CUDA package projection requires Windows x64")
    }
}
