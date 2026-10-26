// SPDX-License-Identifier: Apache-2.0
//! Native WAL/SHM admission, explicitly selected by service publication.
//!
//! SQLite owns each szOsFile allocation. A repr(C) header routes calls to an
//! aligned native file behind it. The registered VFS and names live until process
//! exit. A bounded registry reuses immutable limits so callbacks never dangle.
use anyhow::{Result, ensure};
use rusqlite::{Connection, ffi};
use std::{
    collections::BTreeMap,
    ffi::{CString, c_char, c_int, c_void},
    mem::size_of,
    ptr,
    sync::{Mutex, OnceLock},
};

#[repr(C)]
struct BudgetVfs {
    base: ffi::sqlite3_vfs,
    wal: u64,
    shm: u64,
}
#[repr(C)]
struct BudgetFile {
    base: ffi::sqlite3_file,
    inner: *mut ffi::sqlite3_file,
    wal: bool,
    limit: u64,
    shm: u64,
}
const OFFSET: usize = size_of::<BudgetFile>().next_multiple_of(8);

// SAFETY contract for all callbacks: SQLite supplies the live VFS/file originally
// opened by this shim, and valid ABI buffers for the duration of the call. Native
// file storage is allocated/aligned within SQLite's advertised szOsFile region.
unsafe fn parts<'a>(file: *mut ffi::sqlite3_file) -> (&'a BudgetFile, &'a ffi::sqlite3_io_methods) {
    unsafe {
        let wrapper = &*file.cast::<BudgetFile>();
        (wrapper, &*(*wrapper.inner).pMethods)
    }
}

macro_rules! file_call {
    ($name:ident,$method:ident,($($arg:ident:$ty:ty),*))=>{
        unsafe extern "C" fn $name(file:*mut ffi::sqlite3_file,$($arg:$ty),*)->c_int {
            unsafe {let (wrapper,methods)=parts(file);match methods.$method{Some(call)=>call(wrapper.inner,$($arg),*),None=>ffi::SQLITE_IOERR}}
        }
    };
}
file_call!(read,xRead,(buffer:*mut c_void,count:c_int,offset:i64));
file_call!(sync,xSync,(flags:c_int));
file_call!(file_size,xFileSize,(output:*mut i64));
file_call!(lock,xLock,(level:c_int));
file_call!(unlock,xUnlock,(level:c_int));
file_call!(reserved,xCheckReservedLock,(output:*mut c_int));
file_call!(sector, xSectorSize, ());
file_call!(characteristics, xDeviceCharacteristics, ());
file_call!(shm_lock,xShmLock,(offset:c_int,count:c_int,flags:c_int));
file_call!(shm_unmap,xShmUnmap,(delete:c_int));

unsafe extern "C" fn close(file: *mut ffi::sqlite3_file) -> c_int {
    unsafe {
        let (wrapper, methods) = parts(file);
        let result = methods
            .xClose
            .map_or(ffi::SQLITE_IOERR, |call| call(wrapper.inner));
        (*file).pMethods = ptr::null();
        result
    }
}

unsafe extern "C" fn write(
    file: *mut ffi::sqlite3_file,
    buffer: *const c_void,
    count: c_int,
    offset: i64,
) -> c_int {
    unsafe {
        let (wrapper, methods) = parts(file);
        if wrapper.wal
            && (count < 0
                || offset < 0
                || (offset as u64)
                    .checked_add(count as u64)
                    .is_none_or(|end| end > wrapper.limit))
        {
            return ffi::SQLITE_FULL;
        }
        methods.xWrite.map_or(ffi::SQLITE_IOERR, |call| {
            call(wrapper.inner, buffer, count, offset)
        })
    }
}

unsafe extern "C" fn truncate(file: *mut ffi::sqlite3_file, size: i64) -> c_int {
    unsafe {
        let (wrapper, methods) = parts(file);
        if wrapper.wal && (size < 0 || size as u64 > wrapper.limit) {
            return ffi::SQLITE_FULL;
        }
        methods
            .xTruncate
            .map_or(ffi::SQLITE_IOERR, |call| call(wrapper.inner, size))
    }
}

unsafe extern "C" fn control(
    file: *mut ffi::sqlite3_file,
    operation: c_int,
    argument: *mut c_void,
) -> c_int {
    unsafe {
        let (wrapper, methods) = parts(file);
        let Some(call) = methods.xFileControl else {
            return ffi::SQLITE_NOTFOUND;
        };
        if wrapper.wal {
            if operation == ffi::SQLITE_FCNTL_SIZE_HINT {
                if argument.is_null() {
                    return ffi::SQLITE_MISUSE;
                }
                let size = *argument.cast::<i64>();
                if size < 0 || size as u64 > wrapper.limit {
                    return ffi::SQLITE_FULL;
                }
            }
            if operation == ffi::SQLITE_FCNTL_CHUNK_SIZE {
                // Native preallocation must not round a legal hint past the cap.
                let mut zero: c_int = 0;
                return call(wrapper.inner, operation, (&mut zero as *mut c_int).cast());
            }
        }
        call(wrapper.inner, operation, argument)
    }
}

unsafe extern "C" fn shm_map(
    file: *mut ffi::sqlite3_file,
    page: c_int,
    bytes: c_int,
    extend: c_int,
    output: *mut *mut c_void,
) -> c_int {
    unsafe {
        let (wrapper, methods) = parts(file);
        if !output.is_null() {
            *output = ptr::null_mut();
        }
        if page < 0
            || bytes <= 0
            || ((page as u64) + 1)
                .checked_mul(bytes as u64)
                .is_none_or(|end| end > wrapper.shm)
        {
            return ffi::SQLITE_FULL;
        }
        methods.xShmMap.map_or(ffi::SQLITE_IOERR_SHMMAP, |call| {
            call(wrapper.inner, page, bytes, extend, output)
        })
    }
}
unsafe extern "C" fn shm_barrier(file: *mut ffi::sqlite3_file) {
    unsafe {
        let (wrapper, methods) = parts(file);
        if let Some(call) = methods.xShmBarrier {
            call(wrapper.inner);
        }
    }
}

static METHODS: ffi::sqlite3_io_methods = ffi::sqlite3_io_methods {
    iVersion: 2,
    xClose: Some(close),
    xRead: Some(read),
    xWrite: Some(write),
    xTruncate: Some(truncate),
    xSync: Some(sync),
    xFileSize: Some(file_size),
    xLock: Some(lock),
    xUnlock: Some(unlock),
    xCheckReservedLock: Some(reserved),
    xFileControl: Some(control),
    xSectorSize: Some(sector),
    xDeviceCharacteristics: Some(characteristics),
    xShmMap: Some(shm_map),
    xShmLock: Some(shm_lock),
    xShmBarrier: Some(shm_barrier),
    xShmUnmap: Some(shm_unmap),
    xFetch: None,
    xUnfetch: None,
};

unsafe extern "C" fn open(
    vfs: *mut ffi::sqlite3_vfs,
    name: *const c_char,
    file: *mut ffi::sqlite3_file,
    flags: c_int,
    out: *mut c_int,
) -> c_int {
    unsafe {
        // Copy only immutable budget fields; SQLite owns/mutates base.pNext.
        let limit = (*vfs.cast::<BudgetVfs>()).wal;
        let shm = (*vfs.cast::<BudgetVfs>()).shm;
        let native = (*vfs).pAppData.cast::<ffi::sqlite3_vfs>();
        let Some(call) = (*native).xOpen else {
            return ffi::SQLITE_CANTOPEN;
        };
        let real = file.cast::<u8>().add(OFFSET).cast::<ffi::sqlite3_file>();
        ptr::write(
            file.cast::<BudgetFile>(),
            BudgetFile {
                base: ffi::sqlite3_file {
                    pMethods: ptr::null(),
                },
                inner: real,
                wal: flags & ffi::SQLITE_OPEN_WAL != 0,
                limit,
                shm,
            },
        );
        (*real).pMethods = ptr::null();
        let result = call(native, name, real, flags, out);
        if result != ffi::SQLITE_OK {
            if !(*real).pMethods.is_null()
                && let Some(close) = (*(*real).pMethods).xClose
            {
                close(real);
            }
            return result;
        }
        if (*real).pMethods.is_null() {
            return ffi::SQLITE_CANTOPEN;
        }
        (*file).pMethods = &METHODS;
        ffi::SQLITE_OK
    }
}

macro_rules! vfs_call {
    ($name:ident,$method:ident,($($arg:ident:$ty:ty),*),$result:ty,$fallback:expr)=>{
        unsafe extern "C" fn $name(vfs:*mut ffi::sqlite3_vfs,$($arg:$ty),*)->$result {
            unsafe {let native=(*vfs).pAppData.cast::<ffi::sqlite3_vfs>();match (*native).$method{Some(call)=>call(native,$($arg),*),None=>$fallback}}
        }
    };
}
vfs_call!(delete,xDelete,(name:*const c_char,sync:c_int),c_int,ffi::SQLITE_IOERR_DELETE);
vfs_call!(access,xAccess,(name:*const c_char,flags:c_int,output:*mut c_int),c_int,ffi::SQLITE_IOERR_ACCESS);
vfs_call!(full_path,xFullPathname,(name:*const c_char,count:c_int,output:*mut c_char),c_int,ffi::SQLITE_CANTOPEN);
vfs_call!(dl_open,xDlOpen,(name:*const c_char),*mut c_void,ptr::null_mut());
vfs_call!(dl_error,xDlError,(count:c_int,output:*mut c_char),(),());
// Preserve the pinned libsqlite3-sys callback type; the symbol is opaque here.
type ExtensionSymbol = unsafe extern "C" fn(*mut ffi::sqlite3_vfs, *mut c_void, *const c_char);
vfs_call!(dl_sym,xDlSym,(handle:*mut c_void,name:*const c_char),Option<ExtensionSymbol>,None);
vfs_call!(dl_close,xDlClose,(handle:*mut c_void),(),());
vfs_call!(random,xRandomness,(count:c_int,output:*mut c_char),c_int,0);
vfs_call!(sleep,xSleep,(microseconds:c_int),c_int,0);
vfs_call!(time,xCurrentTime,(output:*mut f64),c_int,ffi::SQLITE_ERROR);
vfs_call!(last_error,xGetLastError,(count:c_int,output:*mut c_char),c_int,0);

pub(super) fn register(wal: u64, shm: u64) -> Result<String> {
    static REGISTRY: OnceLock<Mutex<BTreeMap<(u64, u64), String>>> = OnceLock::new();
    let mut registrations = REGISTRY
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|_| anyhow::anyhow!("WAL VFS registration unavailable"))?;
    ensure!(
        (4096..=1 << 46).contains(&wal) && shm <= 1 << 39,
        "invalid WAL file budgets"
    );
    if let Some(name) = registrations.get(&(wal, shm)) {
        return Ok(name.clone());
    }
    ensure!(
        registrations.len() < 64,
        "WAL VFS budget variants exhausted"
    );
    let number = registrations.len();
    // Initialize SQLite before finding its process-lifetime default VFS.
    drop(Connection::open_in_memory()?);
    let name = format!("airk-wal-budget-1.4-{number}");
    let label = CString::new(name.clone())?.into_raw();
    unsafe {
        let native = ffi::sqlite3_vfs_find(ptr::null());
        ensure!(!native.is_null(), "native SQLite VFS unavailable");
        let bytes = OFFSET
            .checked_add(usize::try_from((*native).szOsFile)?)
            .and_then(|size| c_int::try_from(size).ok())
            .ok_or_else(|| anyhow::anyhow!("VFS file size overflow"))?;
        // The name and struct intentionally remain registered. The immutable shim
        // never changes SQLite's default, reads pNext, or frees callback state early.
        let wrapped = Box::new(BudgetVfs {
            base: ffi::sqlite3_vfs {
                iVersion: 1,
                szOsFile: bytes,
                mxPathname: (*native).mxPathname,
                pNext: ptr::null_mut(),
                zName: label,
                pAppData: native.cast(),
                xOpen: Some(open),
                xDelete: Some(delete),
                xAccess: Some(access),
                xFullPathname: Some(full_path),
                xDlOpen: Some(dl_open),
                xDlError: Some(dl_error),
                xDlSym: Some(dl_sym),
                xDlClose: Some(dl_close),
                xRandomness: Some(random),
                xSleep: Some(sleep),
                xCurrentTime: Some(time),
                xGetLastError: Some(last_error),
                xCurrentTimeInt64: None,
                xSetSystemCall: None,
                xGetSystemCall: None,
                xNextSystemCall: None,
            },
            wal,
            shm,
        });
        let wrapped = Box::into_raw(wrapped);
        ensure!(
            ffi::sqlite3_vfs_register(wrapped.cast(), 0) == ffi::SQLITE_OK,
            "register WAL VFS"
        );
    }
    registrations.insert((wal, shm), name.clone());
    Ok(name)
}
