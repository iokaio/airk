// SPDX-License-Identifier: Apache-2.0
//! Windows x64 held-file CUDA loader; no ambient dependency search.
use super::*;
use airk_process::directory::{ObjectIdentity, OpenedDirectory};
use std::{
    ffi::{CStr, OsStr, c_void},
    fs::{File, OpenOptions},
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::PathBuf,
    sync::OnceLock,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HMODULE, LocalFree},
    Security::WinTrust::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::{
        LibraryLoader::*, ProcessStatus::*, SystemInformation::GetSystemDirectoryW, Threading::*,
    },
};
fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}
fn win(ok: i32) -> Result<()> {
    ensure!(
        ok != 0,
        "Windows GPU custody: {}",
        std::io::Error::last_os_error()
    );
    Ok(())
}
struct Allocation(*mut c_void);
impl Drop for Allocation {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
fn sid(value: PSID) -> Result<String> {
    ensure!(
        !value.is_null() && unsafe { IsValidSid(value) } != 0,
        "invalid GPU custody SID"
    );
    let mut text = std::ptr::null_mut();
    win(unsafe { ConvertSidToStringSidW(value, &mut text) })?;
    let _owned = Allocation(text.cast());
    let mut len = 0;
    while len < 256 && unsafe { *text.add(len) } != 0 {
        len += 1;
    }
    ensure!(len < 256, "SID text too long");
    Ok(String::from_utf16(unsafe {
        std::slice::from_raw_parts(text, len)
    })?)
}
fn current_user() -> Result<String> {
    let mut token = std::ptr::null_mut();
    win(unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) })?;
    let result = (|| {
        let mut size = 0;
        unsafe { GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut size) };
        ensure!(size > 0 && size <= 65536, "native token bound");
        let mut data = vec![0u64; (size as usize).div_ceil(8)];
        win(unsafe {
            GetTokenInformation(token, TokenUser, data.as_mut_ptr().cast(), size, &mut size)
        })?;
        sid(unsafe { (*data.as_ptr().cast::<TOKEN_USER>()).User.Sid })
    })();
    unsafe { CloseHandle(token) };
    result
}
fn security(file: &File, private: bool) -> Result<()> {
    let mut owner = std::ptr::null_mut();
    let mut acl = std::ptr::null_mut();
    let mut descriptor = std::ptr::null_mut();
    let status = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            &mut acl,
            std::ptr::null_mut(),
            &mut descriptor,
        )
    };
    ensure!(
        status == 0 && !descriptor.is_null() && !acl.is_null(),
        "unknown/null GPU custody descriptor"
    );
    let _owned = Allocation(descriptor);
    let user = current_user()?;
    let owner = sid(owner)?;
    ensure!(
        if private {
            owner == user
        } else {
            super::security::trusted(&owner)
        },
        "GPU payload/system owner mismatch"
    );
    let count = unsafe { (*acl).AceCount };
    for i in 0..u32::from(count) {
        let mut ace = std::ptr::null_mut();
        win(unsafe { GetAce(acl, i, &mut ace) })?;
        ensure!(!ace.is_null(), "missing native ACE");
        let header = unsafe { &*ace.cast::<ACE_HEADER>() };
        let size = header.AceSize.to_le_bytes();
        if !super::security::effective_allow([header.AceType, header.AceFlags, size[0], size[1]])? {
            continue;
        }
        let allow = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
        let who = sid(std::ptr::addr_of!(allow.SidStart).cast_mut().cast())?;
        super::security::grant(private, &user, &who, allow.Mask)?;
    }
    Ok(())
}
struct HeldFile {
    file: File,
    path: PathBuf,
    id: ObjectIdentity,
    bytes: u64,
    sha: Sha256Digest,
    _parent: OpenedDirectory,
    private: bool,
}
impl HeldFile {
    fn open(
        parent: &OpenedDirectory,
        path: &Path,
        private: bool,
        deadline: Instant,
    ) -> Result<Self> {
        in_time(deadline)?;
        ensure!(path.is_absolute(), "GPU payload must be a direct child");
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        let meta = file.metadata()?;
        ensure!(
            meta.is_file()
                && meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
                && (!private || single_link(&file)?),
            "redirected/nonregular/hardlinked GPU payload"
        );
        security(&file, private)?;
        let id = airk_process::directory::identity(&file)?;
        let sha = hash(&file, deadline)?;
        Ok(Self {
            file,
            path: path.to_owned(),
            id,
            bytes: meta.len(),
            sha,
            _parent: parent.clone(),
            private,
        })
    }
    fn recheck(&self, deadline: Instant) -> Result<()> {
        in_time(deadline)?;
        security(&self.file, self.private)?;
        ensure!(
            self.file.metadata()?.len() == self.bytes
                && airk_process::directory::identity(&self.file)? == self.id
                && hash(&self.file, deadline)? == self.sha,
            "held CUDA payload drift"
        );
        let current = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&self.path)?;
        ensure!(
            airk_process::directory::identity(&current)? == self.id,
            "named CUDA payload drift"
        );
        Ok(())
    }
}
fn single_link(file: &File) -> Result<bool> {
    let mut value: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    win(unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut value) })?;
    Ok(value.nNumberOfLinks == 1)
}
fn hash(file: &File, deadline: Instant) -> Result<Sha256Digest> {
    use std::os::windows::fs::FileExt;
    let mut hasher = Sha256::new();
    let mut bytes = [0u8; 1 << 20];
    let mut offset = 0u64;
    loop {
        in_time(deadline)?;
        let n = file.seek_read(&mut bytes, offset)?;
        if n == 0 {
            break;
        }
        hasher.update(&bytes[..n]);
        offset = offset
            .checked_add(n as u64)
            .context("GPU hash byte overflow")?;
    }
    Ok(Sha256Digest::from_bytes(hasher.finalize().into()))
}
fn read(file: &File, max: u64) -> Result<Vec<u8>> {
    use std::os::windows::fs::FileExt;
    let length = file.metadata()?.len();
    ensure!(length <= max, "GPU native input too large");
    let mut bytes = vec![0u8; usize::try_from(length)?];
    let mut offset = 0;
    while offset < bytes.len() {
        let n = file.seek_read(&mut bytes[offset..], offset as u64)?;
        ensure!(n > 0, "GPU native input truncated");
        offset += n;
    }
    Ok(bytes)
}
fn system_directory() -> Result<PathBuf> {
    let mut text = vec![0u16; 32768];
    let n = unsafe { GetSystemDirectoryW(text.as_mut_ptr(), text.len() as u32) };
    ensure!(
        n > 0 && (n as usize) < text.len(),
        "native System32 unavailable"
    );
    Ok(PathBuf::from(String::from_utf16(&text[..n as usize])?))
}
fn module_path(module: HMODULE) -> Result<PathBuf> {
    let mut text = vec![0u16; 32768];
    let n = unsafe { GetModuleFileNameW(module, text.as_mut_ptr(), text.len() as u32) };
    ensure!(
        n > 0 && (n as usize) < text.len(),
        "loaded module path unavailable"
    );
    Ok(PathBuf::from(String::from_utf16(&text[..n as usize])?))
}
fn gpu_name(name: &str) -> bool {
    name == "onnxruntime.dll"
        || name == "nvcuda.dll"
        || [
            "onnxruntime_providers_",
            "cudnn",
            "cublas",
            "cudart",
            "cufft",
            "nvrtc",
            "nvjitlink",
        ]
        .iter()
        .any(|p| name.starts_with(p))
}
struct Baseline {
    rows: Vec<super::modules::ModuleIdentity>,
}
fn final_path(file: &File) -> Result<String> {
    let mut bytes = vec![0u16; 32768];
    let count = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle(),
            bytes.as_mut_ptr(),
            bytes.len() as u32,
            0,
        )
    };
    ensure!(
        count > 0 && (count as usize) < bytes.len(),
        "mapped image final path unavailable"
    );
    Ok(String::from_utf16(&bytes[..count as usize])?.to_ascii_lowercase())
}
fn mapped_images() -> Result<Baseline> {
    let mut handles = vec![std::ptr::null_mut(); 4096];
    let mut needed = 0;
    win(unsafe {
        K32EnumProcessModules(
            GetCurrentProcess(),
            handles.as_mut_ptr(),
            (handles.len() * std::mem::size_of::<HMODULE>()) as u32,
            &mut needed,
        )
    })?;
    ensure!(
        (needed as usize) <= handles.len() * std::mem::size_of::<HMODULE>(),
        "loaded module inventory exceeds bound"
    );
    let mut rows = Vec::new();
    for handle in handles
        .into_iter()
        .take(needed as usize / std::mem::size_of::<HMODULE>())
    {
        let path = module_path(handle)?;
        let name = path
            .file_name()
            .and_then(OsStr::to_str)
            .context("module basename unavailable")?
            .to_ascii_lowercase();
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)?;
        ensure!(
            file.metadata()?.is_file()
                && file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
            "redirected mapped image refused"
        );
        rows.push(super::modules::ModuleIdentity {
            address: handle as usize,
            name,
            path: final_path(&file)?,
            id: airk_process::directory::identity(&file)?,
        });
    }
    Ok(Baseline { rows })
}
fn approved_images(files: &[HeldFile]) -> Result<Vec<super::modules::ModuleIdentity>> {
    files
        .iter()
        .filter(|f| {
            f.path
                .extension()
                .and_then(OsStr::to_str)
                .is_some_and(|v| v.eq_ignore_ascii_case("dll"))
        })
        .map(|f| {
            Ok(super::modules::ModuleIdentity {
                address: 0,
                name: f
                    .path
                    .file_name()
                    .and_then(OsStr::to_str)
                    .context("payload basename missing")?
                    .to_ascii_lowercase(),
                path: final_path(&f.file)?,
                id: f.id,
            })
        })
        .collect()
}
fn initial_baseline(files: &[HeldFile]) -> Result<Baseline> {
    let baseline = mapped_images()?;
    let approved = approved_images(files)?;
    for row in &baseline.rows {
        ensure!(
            !gpu_name(&row.name)
                || approved
                    .iter()
                    .any(|v| v.name == row.name && v.path == row.path && v.id == row.id),
            "unapproved existing GPU/core module"
        );
    }
    Ok(baseline)
}
fn reconcile(files: &[HeldFile], baseline: &Baseline) -> Result<()> {
    let current = mapped_images()?;
    let approved = approved_images(files)?;
    for row in &current.rows {
        ensure!(
            super::modules::allowed(row, &baseline.rows, &approved),
            "new/unapproved mapped image {} at {}",
            row.name,
            row.path
        );
    }
    Ok(())
}
fn trusted_driver(file: &HeldFile) -> Result<()> {
    let name = wide(&file.path);
    let mut info = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: name.as_ptr(),
        hFile: file.file.as_raw_handle(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut state = WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 { pFile: &mut info },
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL | WTD_REVOCATION_CHECK_NONE,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let result = unsafe {
        WinVerifyTrust(
            std::ptr::null_mut(),
            &mut action,
            (&mut state as *mut WINTRUST_DATA).cast(),
        )
    };
    state.dwStateAction = WTD_STATEACTION_CLOSE;
    unsafe {
        WinVerifyTrust(
            std::ptr::null_mut(),
            &mut action,
            (&mut state as *mut WINTRUST_DATA).cast(),
        )
    };
    ensure!(
        result == 0,
        "native System32 CUDA driver signature is not trusted"
    );
    Ok(())
}
fn load(path: &Path) -> Result<usize> {
    let value = unsafe {
        LoadLibraryExW(
            wide(path).as_ptr(),
            std::ptr::null_mut(),
            LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    };
    ensure!(
        !value.is_null(),
        "authenticated GPU module load failed: {}",
        std::io::Error::last_os_error()
    );
    Ok(value as usize)
}
unsafe fn symbol<T: Copy>(module: usize, name: &CStr) -> Result<T> {
    let value = unsafe { GetProcAddress(module as HMODULE, name.as_ptr().cast()) }
        .context("native CUDA driver API missing")?;
    ensure!(
        std::mem::size_of::<T>() == std::mem::size_of_val(&value),
        "native API pointer size mismatch"
    );
    Ok(unsafe { std::mem::transmute_copy(&value) })
}
type ContextHandle = *mut c_void;
struct Driver {
    get: unsafe extern "system" fn(*mut i32, i32) -> i32,
    init: unsafe extern "system" fn(u32) -> i32,
    version: unsafe extern "system" fn(*mut i32) -> i32,
    count: unsafe extern "system" fn(*mut i32) -> i32,
    uuid: unsafe extern "system" fn(*mut [u8; 16], i32) -> i32,
    cc: unsafe extern "system" fn(*mut i32, *mut i32, i32) -> i32,
    total: unsafe extern "system" fn(*mut usize, i32) -> i32,
    retain: unsafe extern "system" fn(*mut ContextHandle, i32) -> i32,
    release: unsafe extern "system" fn(i32) -> i32,
    push: unsafe extern "system" fn(ContextHandle) -> i32,
    pop: unsafe extern "system" fn(*mut ContextHandle) -> i32,
    memory: unsafe extern "system" fn(*mut usize, *mut usize) -> i32,
    sync: unsafe extern "system" fn() -> i32,
    attribute: Option<unsafe extern "system" fn(*mut i32, i32, i32) -> i32>,
}
fn cuda(result: i32) -> Result<()> {
    // Pinned CUDA12.8 cuda.h: CUDA_ERROR_OUT_OF_MEMORY=2.
    if result == 2 {
        return Err(super::capacity_fault(
            "actual native CUDA driver out-of-memory".into(),
        ));
    }
    if result != 0 {
        return Err(super::native_fault(format!(
            "native CUDA driver call failed ({result})"
        )));
    }
    Ok(())
}
impl Driver {
    unsafe fn from(module: usize, product: bool) -> Result<Self> {
        Ok(Self {
            get: unsafe { symbol(module, c"cuDeviceGet")? },
            init: unsafe { symbol(module, c"cuInit")? },
            version: unsafe { symbol(module, c"cuDriverGetVersion")? },
            count: unsafe { symbol(module, c"cuDeviceGetCount")? },
            uuid: unsafe { symbol(module, c"cuDeviceGetUuid_v2")? },
            cc: unsafe { symbol(module, c"cuDeviceComputeCapability")? },
            total: unsafe { symbol(module, c"cuDeviceTotalMem_v2")? },
            retain: unsafe { symbol(module, c"cuDevicePrimaryCtxRetain")? },
            release: unsafe { symbol(module, c"cuDevicePrimaryCtxRelease_v2")? },
            push: unsafe { symbol(module, c"cuCtxPushCurrent_v2")? },
            pop: unsafe { symbol(module, c"cuCtxPopCurrent_v2")? },
            memory: unsafe { symbol(module, c"cuMemGetInfo_v2")? },
            sync: unsafe { symbol(module, c"cuCtxSynchronize")? },
            attribute: if product {
                Some(unsafe { symbol(module, c"cuDeviceGetAttribute")? })
            } else {
                None
            },
        })
    }
}
pub(super) struct PrimaryContext {
    _native_lease: Option<Arc<dyn Send + Sync>>,
    uncertain: AtomicBool,
    handle: usize,
    ordinal: i32,
    device: i32,
    driver: &'static Driver,
}
impl Drop for PrimaryContext {
    fn drop(&mut self) {
        if self.uncertain.load(Ordering::Acquire)
            || unsafe { (self.driver.release)(self.device) } != 0
        {
            FAILED.store(true, Ordering::Release);
            if let Some(lease) = self._native_lease.take() {
                std::mem::forget(lease);
            }
        }
    }
}
impl super::context::Stack for PrimaryContext {
    fn push(&self, handle: usize) -> Result<()> {
        cuda(unsafe { (self.driver.push)(handle as ContextHandle) })
    }
    fn pop(&self) -> Result<usize> {
        let mut previous = std::ptr::null_mut();
        cuda(unsafe { (self.driver.pop)(&mut previous) })?;
        Ok(previous as usize)
    }
    fn poison_and_retain(&self) {
        self.uncertain.store(true, Ordering::Release);
        FAILED.store(true, Ordering::Release);
    }
}
impl PrimaryContext {
    fn with_current<T>(&self, call: impl FnOnce() -> Result<T>) -> Result<T> {
        super::context::run(self, self.handle, call)
    }
}
pub(super) struct LoadedRuntime {
    collector: super::placement::Collector,
    files: Vec<HeldFile>,
    baseline: Baseline,
    _modules: Vec<usize>,
    driver: Driver,
    product: bool,
    identity: Sha256Digest,
}
static LOADED: OnceLock<LoadedRuntime> = OnceLock::new();
static ADMISSION: Mutex<()> = Mutex::new(());
static FAILED: AtomicBool = AtomicBool::new(false);
struct Attempt {
    files: Option<Vec<HeldFile>>,
    modules: Vec<usize>,
    started: bool,
}
impl Drop for Attempt {
    fn drop(&mut self) {
        if let Some(files) = self.files.take()
            && self.started
        {
            FAILED.store(true, Ordering::Release);
            std::mem::forget(files);
            std::mem::forget(std::mem::take(&mut self.modules));
        }
    }
}
pub(super) fn admit(
    root: &Path,
    manifest_path: &Path,
    deadline: Instant,
) -> Result<(
    &'static LoadedRuntime,
    manifest::RuntimeDescriptor,
    Sha256Digest,
)> {
    let _exclusive = ADMISSION
        .lock()
        .map_err(|_| anyhow::anyhow!("GPU module admission poisoned"))?;
    ensure!(
        !FAILED.load(Ordering::Acquire),
        "prior GPU DLL admission failed; process restart required"
    );
    ensure!(
        manifest_path.parent() == Some(root),
        "CUDA manifest must be a direct runtime child"
    );
    let directory = OpenedDirectory::open(root)?;
    let image = std::env::current_exe()?;
    let image_parent =
        OpenedDirectory::open(image.parent().context("native image has no parent")?)?;
    ensure!(
        directory.identity()? == image_parent.identity()?,
        "CUDA payload root differs from linked native image directory"
    );
    security(directory.as_file(), true)?;
    let description = HeldFile::open(&directory, manifest_path, true, deadline)?;
    let bytes = read(&description.file, 128 << 10)?;
    let (manifest, identity) = manifest::RuntimeDescriptor::approved(&bytes)?;
    crate::onnx::initialize_runtime()?;
    use ort::execution_providers::ExecutionProvider;
    ensure!(
        ort::execution_providers::CUDAExecutionProvider::default().is_available()?,
        "linked native core was not compiled CUDA-capable"
    );
    let collector = final_collector()?;
    if let Some(existing) = LOADED.get() {
        ensure!(
            existing.identity == identity,
            "another CUDA runtime is already mapped"
        );
        existing.recheck(deadline)?;
        return Ok((existing, manifest, identity));
    }
    let mut sidecars = Vec::new();
    match &manifest {
        manifest::RuntimeDescriptor::Scientific(value) => {
            let producer =
                HeldFile::open(&directory, &root.join(&value.producer_name), true, deadline)?;
            ensure!(
                producer.bytes == value.producer_bytes && producer.sha == value.producer_sha256,
                "actual scientific producer receipt differs"
            );
            sidecars.push(producer);
        }
        manifest::RuntimeDescriptor::Product(value) => {
            let artifact = &value.provenance;
            let proof = HeldFile::open(&directory, &root.join(&artifact.name), true, deadline)?;
            let projected = super::provenance::validate(
                &read(&proof.file, 1 << 20)?,
                artifact,
                value.static_core_build,
                &value.files,
            )?;
            sidecars.push(proof);
            let mut names = BTreeSet::new();
            for legal in projected.legal_assets() {
                ensure!(
                    legal.role == airk_core::arbiter::AssetRole::Notice
                        && legal.name.starts_with("airk-cuda-notice-")
                        && names.insert(legal.name.clone())
                        && !value.files.iter().any(|v| v.name == legal.name),
                    "public legal closure name mismatch"
                );
                let file = HeldFile::open(&directory, &root.join(&legal.name), true, deadline)?;
                ensure!(
                    file.bytes == legal.bytes && file.sha == legal.sha256,
                    "actual legal notice differs from native public projection"
                );
                sidecars.push(file);
            }
        }
    }
    let system_path = system_directory()?;
    let system = OpenedDirectory::open(&system_path)?;
    security(system.as_file(), false)?;
    let driver_file = HeldFile::open(
        &system,
        &system_path.as_path().join("nvcuda.dll"),
        false,
        deadline,
    )?;
    trusted_driver(&driver_file)?;
    let mut files = vec![description];
    files.extend(sidecars);
    files.push(driver_file);
    for item in manifest.files() {
        let held = HeldFile::open(&directory, &root.join(&item.name), true, deadline)?;
        ensure!(
            held.sha == item.sha256 && held.bytes == item.bytes,
            "GPU payload hash/size mismatch"
        );
        let imports = super::pe::imports_file(&held.file, deadline)?;
        ensure!(
            imports == (item.imports.clone(), item.delay_imports.clone()),
            "actual GPU PE imports differ from approved closure"
        );
        files.push(held);
    }
    for name in manifest.system_imports() {
        if name == "nvcuda.dll"
            || name.starts_with("api-ms-win-")
            || name.starts_with("ext-ms-win-")
        {
            continue;
        }
        files.push(HeldFile::open(
            &system,
            &system_path.as_path().join(name),
            false,
            deadline,
        )?);
    }
    let baseline = initial_baseline(&files)?;
    win(unsafe {
        SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_APPLICATION_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32)
    })?;
    let mut attempt = Attempt {
        files: Some(files),
        modules: Vec::new(),
        started: true,
    };
    attempt.modules.push(load(&system_path.join("nvcuda.dll"))?);
    reconcile(attempt.files.as_ref().unwrap(), &baseline)?;
    for name in manifest.load_order() {
        in_time(deadline)?;
        attempt.modules.push(load(&root.join(name))?);
        reconcile(attempt.files.as_ref().unwrap(), &baseline)?;
    }
    let driver = unsafe { Driver::from(attempt.modules[0], manifest.is_product())? };
    cuda(unsafe { (driver.init)(0) })?;
    for file in attempt.files.as_ref().unwrap() {
        file.recheck(deadline)?;
    }
    in_time(deadline)?;
    let files = attempt.files.take().unwrap();
    let modules = std::mem::take(&mut attempt.modules);
    LOADED
        .set(LoadedRuntime {
            collector,
            files,
            baseline,
            _modules: modules,
            driver,
            product: manifest.is_product(),
            identity,
        })
        .map_err(|_| anyhow::anyhow!("GPU module owner raced"))?;
    let loaded = LOADED.get().unwrap();
    Ok((loaded, manifest, identity))
}
impl LoadedRuntime {
    fn recheck(&self, deadline: Instant) -> Result<()> {
        reconcile(&self.files, &self.baseline)?;
        for file in &self.files {
            file.recheck(deadline)?;
        }
        Ok(())
    }
    pub fn retain_primary(
        &'static self,
        ordinal: i32,
        lease: Arc<dyn Send + Sync>,
    ) -> Result<PrimaryContext> {
        let mut device = 0;
        cuda(unsafe { (self.driver.get)(&mut device, ordinal) })?;
        let mut context = std::ptr::null_mut();
        cuda(unsafe { (self.driver.retain)(&mut context, device) })?;
        ensure!(!context.is_null(), "native CUDA primary context missing");
        Ok(PrimaryContext {
            _native_lease: Some(lease),
            uncertain: AtomicBool::new(false),
            handle: context as usize,
            ordinal,
            device,
            driver: &self.driver,
        })
    }
    pub fn discover(&self, deadline: Instant) -> Result<Vec<AdapterIdentity>> {
        self.recheck(deadline)?;
        ensure!(self.product, "product-only metadata discovery");
        let mut count = 0;
        cuda(unsafe { (self.driver.count)(&mut count) })?;
        ensure!((0..=64).contains(&count), "invalid adapter count");
        let driver_image = self
            .files
            .iter()
            .find(|f| {
                f.path
                    .file_name()
                    .and_then(OsStr::to_str)
                    .is_some_and(|n| n.eq_ignore_ascii_case("nvcuda.dll"))
            })
            .context("held driver identity missing")?
            .sha;
        let mut values = Vec::new();
        for ordinal in 0..count {
            in_time(deadline)?;
            let (mut device, mut major, mut minor, mut api, mut pool, mut total, mut uuid) =
                (0, 0, 0, 0, 0, 0usize, [0; 16]);
            cuda(unsafe { (self.driver.get)(&mut device, ordinal) })?;
            cuda(unsafe { (self.driver.uuid)(&mut uuid, device) })?;
            cuda(unsafe { (self.driver.cc)(&mut major, &mut minor, device) })?;
            cuda(unsafe { (self.driver.version)(&mut api) })?;
            cuda(unsafe { (self.driver.total)(&mut total, device) })?;
            cuda(unsafe {
                self.driver
                    .attribute
                    .context("native memory-pool observer absent")?(
                    &mut pool, 18, device
                )
            })?;
            ensure!(
                uuid != [0; 16] && total > 0 && matches!(pool, 0 | 1),
                "native adapter metadata invalid"
            );
            values.push(AdapterIdentity {
                uuid,
                ordinal,
                compute_major: major,
                compute_minor: minor,
                driver_api_version: api,
                driver_image_sha256: driver_image,
                physical_total_bytes: total as u64,
                dedicated_memory: pool == 0,
            });
        }
        in_time(deadline)?;
        Ok(values)
    }
    pub fn adapters(
        &'static self,
        deadline: Instant,
        lease: Arc<dyn Send + Sync>,
    ) -> Result<Vec<AdapterObservation>> {
        self.recheck(deadline)?;
        let mut count = 0;
        cuda(unsafe { (self.driver.count)(&mut count) })?;
        ensure!(
            (0..=64).contains(&count),
            "invalid native CUDA adapter count"
        );
        let mut values = Vec::new();
        for ordinal in 0..count {
            let context = self.retain_primary(ordinal, lease.clone())?;
            values.push(self.observe(ordinal, &context, deadline)?);
            drop(context);
            ensure!(
                !runtime_poisoned(),
                "CUDA primary context release did not settle"
            );
        }
        Ok(values)
    }
    pub fn observe(
        &self,
        ordinal: i32,
        context: &PrimaryContext,
        deadline: Instant,
    ) -> Result<AdapterObservation> {
        in_time(deadline)?;
        reconcile(&self.files, &self.baseline)?;
        ensure!(ordinal == context.ordinal, "CUDA context/ordinal mismatch");
        context.with_current(||{let(mut uuid,mut major,mut minor,mut driver,mut total,mut free,mut actual_total)=([0;16],0,0,0,0usize,0usize,0usize);cuda(unsafe{(self.driver.uuid)(&mut uuid,context.device)})?;cuda(unsafe{(self.driver.cc)(&mut major,&mut minor,context.device)})?;cuda(unsafe{(self.driver.version)(&mut driver)})?;cuda(unsafe{(self.driver.total)(&mut total,context.device)})?;cuda(unsafe{(self.driver.memory)(&mut free,&mut actual_total)})?;ensure!(uuid!=[0;16]&&total>0&&actual_total>0&&actual_total<=total&&free<=actual_total&&driver>=12080,"invalid/unsupported actual CUDA device observation");in_time(deadline)?;Ok(AdapterObservation{uuid,ordinal,compute_major:major,compute_minor:minor,driver_api_version:driver,driver_image_sha256:self.files.iter().find(|f|f.path.file_name().and_then(OsStr::to_str).is_some_and(|n|n.eq_ignore_ascii_case("nvcuda.dll"))).context("held driver identity missing")?.sha,physical_total_bytes:total as u64,total_bytes:actual_total as u64,free_bytes:free as u64,epoch:0,dedicated_memory:if self.product {let mut integrated=0;let get=self.driver.attribute.context("native memory-pool observer missing")?;cuda(unsafe{get(&mut integrated,18,context.device)})?;ensure!(matches!(integrated,0|1),"native CUDA memory pool type unknown");Some(integrated==0)}else{None}})})
    }
    pub fn synchronize(&self, context: &PrimaryContext) -> Result<()> {
        reconcile(&self.files, &self.baseline)?;
        context.with_current(|| cuda(unsafe { (self.driver.sync)() }))
    }
}

#[cfg(feature = "test-support")]
pub(super) fn observe_assembly(input: RuntimeAssemblyInput, deadline: Instant) -> Result<Vec<u8>> {
    let directory = OpenedDirectory::open(&input.directory)?;
    security(directory.as_file(), true)?;
    ensure!(
        !input.producer_name.contains(['\\', '/']) && input.module_names.len() <= 64,
        "invalid assembly names"
    );
    let producer = HeldFile::open(
        &directory,
        &input.directory.join(&input.producer_name),
        true,
        deadline,
    )?;
    ensure!(
        producer.sha == input.producer_sha256,
        "actual reviewed producer receipt pin differs"
    );
    let receipt: serde_json::Value = serde_json::from_slice(&read(&producer.file, 16 << 20)?)?;
    ensure!(
        receipt["status"] == "PASS_PRODUCER_ONLY_UNQUALIFIED",
        "producer is not naturally completed successful unqualified build"
    );
    let provider_rows = receipt["provider_dlls"]
        .as_array()
        .context("actual provider outputs missing")?;
    ensure!(
        provider_rows.len() == 2,
        "provider pair output must be exact"
    );
    let mut files = Vec::new();
    let mut held = vec![producer];
    for name in &input.module_names {
        ensure!(
            !name.contains(['\\', '/']) && name.ends_with(".dll"),
            "invalid assembly DLL name"
        );
        let value = HeldFile::open(&directory, &input.directory.join(name), true, deadline)?;
        let (imports, delay_imports) = super::pe::imports_file(&value.file, deadline)?;
        if name.starts_with("onnxruntime_providers_") {
            ensure!(
                provider_rows
                    .iter()
                    .filter(|v| v["path"]
                        .as_str()
                        .is_some_and(|p| p.rsplit('/').next() == Some(name.as_str()))
                        && v["sha256"].as_str() == Some(value.sha.to_string().as_str())
                        && v["bytes"].as_u64() == Some(value.bytes))
                    .count()
                    == 1,
                "provider DLL does not match actual same-build producer output"
            );
        }
        files.push(manifest::RuntimeFile {
            name: name.clone(),
            bytes: value.bytes,
            sha256: value.sha,
            imports,
            delay_imports,
        });
        held.push(value);
    }
    let manifest = manifest::RuntimeManifest {
        schema: "airk.native.cuda-runtime-manifest/1".into(),
        ort_version: "1.30.0".into(),
        static_core_build: input.producer_sha256,
        producer_name: input.producer_name,
        producer_bytes: held[0].bytes,
        producer_sha256: input.producer_sha256,
        files,
        load_order: input.load_order,
        system_imports: input.system_imports,
        fits: Vec::new(),
    };
    manifest.validate()?;
    for file in &held {
        file.recheck(deadline)?;
    }
    in_time(deadline)?;
    Ok(serde_json::to_vec(&manifest)?)
}

pub(super) fn runtime_poisoned() -> bool {
    FAILED.load(Ordering::Acquire)
}

impl LoadedRuntime {
    pub fn placement(&self, session: &ort::session::Session) -> Result<PlacementObservation> {
        reconcile(&self.files, &self.baseline)?;
        // SAFETY: exact same statically linked ORT owner; live Session is held
        // under its native Mutex for this bounded synchronous observation.
        Ok(unsafe { super::placement::observe(self.collector, session) })
    }
}

fn final_collector() -> Result<super::placement::Collector> {
    let mut module = std::ptr::null_mut();
    // FROM_ADDRESS interprets this parameter as a code address, not UTF16 text.
    // No reference count is changed; the native main image has process lifetime.
    win(unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            ort::sys::OrtGetApiBase as *const () as *const u16,
            &mut module,
        )
    })?;
    ensure!(
        module == unsafe { GetModuleHandleW(std::ptr::null()) },
        "ORT API base is not in the authenticated single static-core native image"
    );
    // Optional export: CPU --all-features builds still link without this symbol.
    // Missing/old core refuses GPU; no second core or API offset is attempted.
    unsafe { symbol(module as usize, c"airk_ort_collect_final_placement_v1") }
        .context(RuntimeUnavailable::new("final-placement capability absent"))
}

#[cfg(feature = "test-support")]
struct ProjectionSource {
    deadline: Instant,
    files: std::cell::RefCell<BTreeMap<PathBuf, HeldFile>>,
}
#[cfg(feature = "test-support")]
impl ProjectionSource {
    fn hold(
        &self,
        pin: &airk_core::arbiter::resource_policy::ResourceFilePin,
        maximum: u64,
    ) -> Result<()> {
        pin.validate()?;
        in_time(self.deadline)?;
        let path = Path::new(&pin.path);
        ensure!(
            path.is_absolute(),
            "projection source path must be absolute"
        );
        if !self.files.borrow().contains_key(path) {
            let parent =
                OpenedDirectory::open(path.parent().context("projection source parent missing")?)?;
            // Immutable metadata/SDK inputs may be human-owned or OS trusted. Neither
            // branch accepts foreign write/delete/DAC/owner grants or modifies any ACL.
            let private = if security(parent.as_file(), true).is_ok() {
                true
            } else {
                security(parent.as_file(), false)?;
                false
            };
            let held = HeldFile::open(&parent, path, private, self.deadline)?;
            ensure!(
                held.sha == pin.sha256 && held.bytes <= maximum,
                "actual pinned projection source differs"
            );
            self.files.borrow_mut().insert(path.to_owned(), held);
        }
        let files = self.files.borrow();
        let held = files.get(path).context("projection custody missing")?;
        ensure!(
            held.sha == pin.sha256 && held.bytes <= maximum,
            "projection pin changed"
        );
        held.recheck(self.deadline)
    }
}
#[cfg(feature = "test-support")]
impl super::provenance::projection::Source for ProjectionSource {
    fn read(
        &self,
        pin: &airk_core::arbiter::resource_policy::ResourceFilePin,
        maximum: u64,
    ) -> Result<Vec<u8>> {
        self.hold(pin, maximum)?;
        let files = self.files.borrow();
        let held = files
            .get(Path::new(&pin.path))
            .context("projection held input missing")?;
        let raw = read(&held.file, maximum)?;
        in_time(self.deadline)?;
        Ok(raw)
    }
    fn authenticate(
        &self,
        pin: &airk_core::arbiter::resource_policy::ResourceFilePin,
        bytes: u64,
        maximum: u64,
    ) -> Result<()> {
        self.hold(pin, maximum)?;
        ensure!(
            self.files
                .borrow()
                .get(Path::new(&pin.path))
                .context("projection input missing")?
                .bytes
                == bytes,
            "projection declared source size differs"
        );
        Ok(())
    }
    fn recheck(&self) -> Result<()> {
        for held in self.files.borrow().values() {
            held.recheck(self.deadline)?;
        }
        in_time(self.deadline)
    }
}
#[cfg(feature = "test-support")]
pub(super) fn project_public(raw: &[u8], deadline: Instant) -> Result<Vec<u8>> {
    let input = super::provenance::projection::decode_input(raw)?;
    let source = ProjectionSource {
        deadline,
        files: std::cell::RefCell::new(BTreeMap::new()),
    };
    let prepared = super::provenance::projection::project(&source, input)?;
    let bytes = serde_json::to_vec(&prepared)?;
    ensure!(bytes.len() <= 16 << 20, "native projection output cap");
    in_time(deadline)?;
    Ok(bytes)
}
