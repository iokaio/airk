// SPDX-License-Identifier: Apache-2.0
//! CUDA Driver API calls are bound to the byte-pinned system libcuda image.
use super::*;
use std::ffi::c_void;

type Ctx = *mut c_void;

pub(super) struct Driver {
    get: unsafe extern "C" fn(*mut i32, i32) -> i32,
    init: unsafe extern "C" fn(u32) -> i32,
    version: unsafe extern "C" fn(*mut i32) -> i32,
    count: unsafe extern "C" fn(*mut i32) -> i32,
    uuid: unsafe extern "C" fn(*mut [u8; 16], i32) -> i32,
    capability: unsafe extern "C" fn(*mut i32, *mut i32, i32) -> i32,
    total: unsafe extern "C" fn(*mut usize, i32) -> i32,
    retain: unsafe extern "C" fn(*mut Ctx, i32) -> i32,
    release: unsafe extern "C" fn(i32) -> i32,
    push: unsafe extern "C" fn(Ctx) -> i32,
    pop: unsafe extern "C" fn(*mut Ctx) -> i32,
    memory: unsafe extern "C" fn(*mut usize, *mut usize) -> i32,
    synchronize: unsafe extern "C" fn() -> i32,
    attribute: Option<unsafe extern "C" fn(*mut i32, i32, i32) -> i32>,
}

fn checked(status: i32) -> Result<()> {
    match status {
        0 => Ok(()),
        2 => Err(super::super::capacity_fault(
            "native CUDA driver out of memory".into(),
        )),
        other => Err(super::super::native_fault(format!(
            "Linux CUDA driver returned {other}"
        ))),
    }
}

impl Driver {
    pub(super) fn new(handle: &Handle, product: bool) -> Result<Self> {
        let image = handle.id()?;
        // Each signature is the CUDA 12.8 x86-64 Linux driver ABI.
        unsafe {
            Ok(Self {
                get: handle.symbol(c"cuDeviceGet", image)?,
                init: handle.symbol(c"cuInit", image)?,
                version: handle.symbol(c"cuDriverGetVersion", image)?,
                count: handle.symbol(c"cuDeviceGetCount", image)?,
                uuid: handle.symbol(c"cuDeviceGetUuid_v2", image)?,
                capability: handle.symbol(c"cuDeviceComputeCapability", image)?,
                total: handle.symbol(c"cuDeviceTotalMem_v2", image)?,
                retain: handle.symbol(c"cuDevicePrimaryCtxRetain", image)?,
                release: handle.symbol(c"cuDevicePrimaryCtxRelease_v2", image)?,
                push: handle.symbol(c"cuCtxPushCurrent_v2", image)?,
                pop: handle.symbol(c"cuCtxPopCurrent_v2", image)?,
                memory: handle.symbol(c"cuMemGetInfo_v2", image)?,
                synchronize: handle.symbol(c"cuCtxSynchronize", image)?,
                attribute: if product {
                    Some(handle.symbol(c"cuDeviceGetAttribute", image)?)
                } else {
                    None
                },
            })
        }
    }
    pub(super) fn initialize(&self) -> Result<()> {
        checked(unsafe { (self.init)(0) })
    }
    fn device(&self, ordinal: i32) -> Result<i32> {
        let mut device = 0;
        checked(unsafe { (self.get)(&mut device, ordinal) })?;
        Ok(device)
    }
    fn count(&self) -> Result<i32> {
        let mut count = 0;
        checked(unsafe { (self.count)(&mut count) })?;
        ensure!(
            (0..=64).contains(&count),
            "invalid Linux CUDA adapter count"
        );
        Ok(count)
    }
}

pub(in super::super) struct PrimaryContext {
    lease: Option<Arc<dyn Send + Sync>>,
    driver: &'static Driver,
    ordinal: i32,
    device: i32,
    handle: usize,
    uncertain: AtomicBool,
}
impl super::super::context::Stack for PrimaryContext {
    fn push(&self, handle: usize) -> Result<()> {
        checked(unsafe { (self.driver.push)(handle as Ctx) })
    }
    fn pop(&self) -> Result<usize> {
        let mut previous = std::ptr::null_mut();
        checked(unsafe { (self.driver.pop)(&mut previous) })?;
        Ok(previous as usize)
    }
    fn poison_and_retain(&self) {
        self.uncertain.store(true, Ordering::Release);
        FAILED.store(true, Ordering::Release);
    }
}
impl Drop for PrimaryContext {
    fn drop(&mut self) {
        if self.uncertain.load(Ordering::Acquire)
            || unsafe { (self.driver.release)(self.device) } != 0
        {
            FAILED.store(true, Ordering::Release);
            std::mem::forget(self.lease.take());
        }
    }
}

impl LoadedRuntime {
    pub(in super::super) fn retain_primary(
        &'static self,
        ordinal: i32,
        lease: Arc<dyn Send + Sync>,
    ) -> Result<PrimaryContext> {
        let device = self.driver.device(ordinal)?;
        let mut context = std::ptr::null_mut();
        checked(unsafe { (self.driver.retain)(&mut context, device) })?;
        ensure!(!context.is_null(), "CUDA primary context absent");
        Ok(PrimaryContext {
            lease: Some(lease),
            driver: &self.driver,
            ordinal,
            device,
            handle: context as usize,
            uncertain: AtomicBool::new(false),
        })
    }
    fn identity(&self, ordinal: i32, device: i32) -> Result<AdapterIdentity> {
        let (mut uuid, mut major, mut minor, mut api, mut total) = ([0; 16], 0, 0, 0, 0);
        checked(unsafe { (self.driver.uuid)(&mut uuid, device) })?;
        checked(unsafe { (self.driver.capability)(&mut major, &mut minor, device) })?;
        checked(unsafe { (self.driver.version)(&mut api) })?;
        checked(unsafe { (self.driver.total)(&mut total, device) })?;
        ensure!(
            uuid != [0; 16] && total > 0 && api >= 12080,
            "unsupported Linux CUDA device"
        );
        let dedicated_memory = if let Some(attribute) = self.driver.attribute {
            let mut integrated = 0;
            checked(unsafe { attribute(&mut integrated, 18, device) })?;
            ensure!(
                matches!(integrated, 0 | 1),
                "invalid CUDA integrated-memory flag"
            );
            integrated == 0
        } else {
            false
        };
        Ok(AdapterIdentity {
            uuid,
            ordinal,
            compute_major: major,
            compute_minor: minor,
            driver_api_version: api,
            physical_total_bytes: total as u64,
            driver_image_sha256: self.driver_image,
            dedicated_memory,
        })
    }
    pub(in super::super) fn discover(&self, deadline: Instant) -> Result<Vec<AdapterIdentity>> {
        self.recheck(deadline)?;
        ensure!(
            self.product,
            "metadata discovery requires the product descriptor"
        );
        let mut identities = Vec::new();
        for ordinal in 0..self.driver.count()? {
            in_time(deadline)?;
            identities.push(self.identity(ordinal, self.driver.device(ordinal)?)?);
        }
        in_time(deadline)?;
        Ok(identities)
    }
    pub(in super::super) fn adapters(
        &'static self,
        deadline: Instant,
        lease: Arc<dyn Send + Sync>,
    ) -> Result<Vec<AdapterObservation>> {
        self.recheck(deadline)?;
        let mut result = Vec::new();
        for ordinal in 0..self.driver.count()? {
            in_time(deadline)?;
            let context = self.retain_primary(ordinal, lease.clone())?;
            result.push(self.observe(ordinal, &context, deadline)?);
            drop(context);
            ensure!(
                !runtime_poisoned(),
                "Linux CUDA primary context release did not settle"
            );
        }
        Ok(result)
    }
    pub(in super::super) fn observe(
        &self,
        ordinal: i32,
        context: &PrimaryContext,
        deadline: Instant,
    ) -> Result<AdapterObservation> {
        in_time(deadline)?;
        self.assets.reconcile()?;
        ensure!(
            context.ordinal == ordinal,
            "Linux CUDA primary context ordinal differs"
        );
        super::super::context::run(context, context.handle, || {
            let id = self.identity(ordinal, context.device)?;
            let (mut free, mut actual) = (0, 0);
            checked(unsafe { (self.driver.memory)(&mut free, &mut actual) })?;
            ensure!(
                actual > 0 && actual as u64 <= id.physical_total_bytes && free <= actual,
                "invalid Linux CUDA memory observation"
            );
            in_time(deadline)?;
            Ok(AdapterObservation {
                uuid: id.uuid,
                ordinal,
                compute_major: id.compute_major,
                compute_minor: id.compute_minor,
                driver_api_version: id.driver_api_version,
                driver_image_sha256: self.driver_image,
                physical_total_bytes: id.physical_total_bytes,
                total_bytes: actual as u64,
                free_bytes: free as u64,
                epoch: 0,
                dedicated_memory: self.product.then_some(id.dedicated_memory),
            })
        })
    }
    pub(in super::super) fn synchronize(&self, context: &PrimaryContext) -> Result<()> {
        self.assets.reconcile()?;
        super::super::context::run(context, context.handle, || {
            checked(unsafe { (self.driver.synchronize)() })
        })
    }
}
