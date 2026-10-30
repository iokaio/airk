// SPDX-License-Identifier: Apache-2.0
//! Explicit user-only pipe/file ACLs and native token checks on both connection ends.
use anyhow::{Context, Result, ensure};
use std::{
    ffi::c_void,
    io, mem,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    pin::Pin,
    task::{Context as TaskContext, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::windows::named_pipe::{ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions},
};
use windows_sys::Win32::{
    Foundation::{ERROR_ALREADY_EXISTS, LocalFree},
    Security::{
        ACCESS_ALLOWED_ACE,
        Authorization::{
            ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
            GetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT,
        },
        DACL_SECURITY_INFORMATION, GetAce, GetTokenInformation, OWNER_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    Storage::FileSystem::CreateDirectoryW,
    System::{
        Pipes::{GetNamedPipeClientProcessId, GetNamedPipeServerProcessId},
        SystemServices::ACCESS_ALLOWED_ACE_TYPE,
        Threading::{
            GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
        },
    },
};

struct Allocation(*mut c_void);
impl Drop for Allocation {
    fn drop(&mut self) {
        // SAFETY: only successful LocalAlloc-returning security APIs construct this owner.
        unsafe {
            LocalFree(self.0);
        }
    }
}

fn sid_string(sid: *mut c_void) -> Result<String> {
    let mut text = std::ptr::null_mut();
    // SAFETY: caller passes a SID inside a live OS-returned token/security buffer.
    ensure!(
        unsafe { ConvertSidToStringSidW(sid, &mut text) } != 0,
        "invalid native user SID"
    );
    let owner = Allocation(text.cast());
    // SAFETY: successful conversion returns a NUL-terminated LocalAlloc UTF-16 string.
    let mut length = 0;
    unsafe {
        while *text.add(length) != 0 {
            length += 1;
        }
    }
    ensure!(length <= 256, "native SID exceeds bound");
    // SAFETY: the preceding scan established the length of the live returned allocation.
    let value = String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) })?;
    drop(owner);
    Ok(value)
}

pub(crate) fn process_user(pid: u32) -> Result<String> {
    // SAFETY: PID and requested read-only rights are scalar values.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return Err(io::Error::last_os_error().into());
    }
    // SAFETY: successful OpenProcess transfers one owned real handle.
    let process = unsafe { OwnedHandle::from_raw_handle(process) };
    let mut token = std::ptr::null_mut();
    // SAFETY: process owns a valid handle; token points to writable output storage.
    if unsafe { OpenProcessToken(process.as_raw_handle(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error().into());
    }
    // SAFETY: successful OpenProcessToken transfers one owned handle.
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let mut storage = [0usize; 64];
    let mut returned = 0;
    // SAFETY: aligned writable storage exceeds TOKEN_USER plus the maximum native SID.
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            storage.as_mut_ptr().cast(),
            mem::size_of_val(&storage) as u32,
            &mut returned,
        )
    } == 0
    {
        return Err(io::Error::last_os_error().into());
    }
    ensure!(
        returned as usize >= mem::size_of::<TOKEN_USER>(),
        "truncated native token"
    );
    // SAFETY: native call succeeded with aligned TOKEN_USER storage kept alive through conversion.
    let sid = unsafe { (*(storage.as_ptr().cast::<TOKEN_USER>())).User.Sid };
    sid_string(sid)
}

pub fn current_user() -> Result<String> {
    // SAFETY: no pointer arguments or handle ownership transfer.
    process_user(unsafe { GetCurrentProcessId() })
}

fn descriptor(sddl: &str) -> Result<Allocation> {
    let wide: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
    let mut pointer: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: terminated input and writable output pointers live for the whole call.
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            wide.as_ptr(),
            SDDL_REVISION_1,
            &mut pointer,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error().into());
    }
    Ok(Allocation(pointer))
}

fn create_directory(root: &Path, user: &str, allow_existing: bool) -> Result<()> {
    let security = descriptor(&format!("O:{user}D:P(A;OICI;FA;;;{user})"))?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: security.0,
        bInheritHandle: 0,
    };
    let wide: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: descriptor, attributes and path remain live until directory creation returns.
    if unsafe { CreateDirectoryW(wide.as_ptr(), &attributes) } == 0 {
        let error = io::Error::last_os_error();
        if !allow_existing || error.raw_os_error() != Some(ERROR_ALREADY_EXISTS as i32) {
            return Err(error.into());
        }
    }
    Ok(())
}

pub(crate) fn create_root(root: &Path) -> Result<()> {
    create_directory(root, &current_user()?, false)?;
    prepare_root(root)
}

pub fn prepare_root(root: &Path) -> Result<()> {
    let user = current_user()?;
    if !root.try_exists()? {
        let parent = root.parent().context("native runtime parent missing")?;
        std::fs::create_dir_all(parent)?;
        create_directory(root, &user, true)?;
    }
    for ancestor in root.ancestors().filter(|p| !p.as_os_str().is_empty()) {
        use std::os::windows::fs::MetadataExt;
        let metadata = std::fs::symlink_metadata(ancestor)?;
        ensure!(
            metadata.is_dir() && metadata.file_attributes() & 0x400 == 0,
            "redirected native runtime directory"
        );
    }
    let wide: Vec<u16> = root.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut security = std::ptr::null_mut();
    let mut owner = std::ptr::null_mut();
    let mut dacl = std::ptr::null_mut();
    // SAFETY: writable output pointers and terminated path remain live; API allocates descriptor.
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut security,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status as i32).into());
    }
    let _security = Allocation(security);
    ensure!(
        !owner.is_null() && !dacl.is_null(),
        "unrestricted native runtime directory"
    );
    ensure!(
        sid_string(owner)? == user,
        "foreign native runtime directory owner"
    );
    // SAFETY: nonnull ACL points inside the successful OS-returned live security descriptor.
    ensure!(
        unsafe { (*dacl).AceCount } == 1,
        "native runtime directory is not owner-only"
    );
    let mut ace = std::ptr::null_mut();
    // SAFETY: the ACL has exactly one entry; writable output pointer lives for this call.
    ensure!(
        unsafe { GetAce(dacl, 0, &mut ace) } != 0 && !ace.is_null(),
        "invalid native runtime ACL"
    );
    // SAFETY: inspect native ACE header before treating it as an allowed ACE.
    let header = unsafe { &*ace.cast::<windows_sys::Win32::Security::ACE_HEADER>() };
    ensure!(
        header.AceType == ACCESS_ALLOWED_ACE_TYPE as u8
            && header.AceSize as usize >= mem::size_of::<ACCESS_ALLOWED_ACE>(),
        "unsupported native runtime ACE"
    );
    // SAFETY: type and size were checked; native descriptor owns the complete ACE/SID allocation.
    let allowed = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
    let sid = std::ptr::addr_of!(allowed.SidStart).cast_mut().cast();
    ensure!(
        sid_string(sid)? == user && allowed.Mask & 0x1f01ff == 0x1f01ff,
        "native runtime ACL must grant only its owner"
    );
    Ok(())
}

/// Owner-private descriptor check for editor-written files. Unlike AIRK's own
/// control roots, these inherit the profile ACL, so SYSTEM and Administrators
/// may also hold access; any other allowed principal, or a null DACL, is refused.
pub(crate) fn require_owner_private(path: &Path) -> Result<()> {
    const SYSTEM: &str = "S-1-5-18";
    const ADMINISTRATORS: &str = "S-1-5-32-544";
    let user = current_user()?;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let (mut security, mut owner, mut dacl) = (
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    );
    // SAFETY: writable output pointers and terminated path remain live; API allocates descriptor.
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            std::ptr::null_mut(),
            &mut dacl,
            std::ptr::null_mut(),
            &mut security,
        )
    };
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status as i32).into());
    }
    let _security = Allocation(security);
    ensure!(
        !owner.is_null() && !dacl.is_null(),
        "unrestricted descriptor ACL"
    );
    let owner = sid_string(owner)?;
    ensure!(
        owner == user || owner == ADMINISTRATORS,
        "descriptor is owned by another account"
    );
    // SAFETY: nonnull ACL points inside the successful OS-returned live security descriptor.
    let count = unsafe { (*dacl).AceCount };
    for index in 0..u32::from(count) {
        let mut ace = std::ptr::null_mut();
        // SAFETY: index is below the ACL's entry count; output pointer lives for this call.
        ensure!(
            unsafe { GetAce(dacl, index, &mut ace) } != 0 && !ace.is_null(),
            "invalid descriptor ACL"
        );
        // SAFETY: inspect the native ACE header before interpreting its body.
        let header = unsafe { &*ace.cast::<windows_sys::Win32::Security::ACE_HEADER>() };
        if header.AceType
            == windows_sys::Win32::System::SystemServices::ACCESS_DENIED_ACE_TYPE as u8
        {
            continue;
        }
        ensure!(
            header.AceType == ACCESS_ALLOWED_ACE_TYPE as u8
                && header.AceSize as usize >= mem::size_of::<ACCESS_ALLOWED_ACE>(),
            "unsupported descriptor ACE"
        );
        // SAFETY: type and size were checked; native descriptor owns the complete ACE/SID allocation.
        let allowed = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
        let sid = sid_string(std::ptr::addr_of!(allowed.SidStart).cast_mut().cast())?;
        ensure!(
            sid == user || sid == SYSTEM || sid == ADMINISTRATORS,
            "descriptor grants access to another account ({sid})"
        );
    }
    Ok(())
}

fn pipe(address: &str, first: bool) -> Result<NamedPipeServer> {
    ensure!(
        address.starts_with(r"\\.\pipe\airk-1.4-") && address.len() <= 128,
        "invalid local pipe endpoint"
    );
    let user = current_user()?;
    let security = descriptor(&format!("O:{user}D:P(A;;GA;;;{user})"))?;
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: security.0,
        bInheritHandle: 0,
    };
    // SAFETY: explicit noninheritable owner-only descriptor remains live during native creation.
    Ok(unsafe {
        ServerOptions::new()
            .first_pipe_instance(first)
            .reject_remote_clients(true)
            .max_instances(128)
            .create_with_security_attributes_raw(
                address,
                (&mut attributes as *mut SECURITY_ATTRIBUTES).cast(),
            )?
    })
}

pub struct Listener {
    address: String,
    next: NamedPipeServer,
}

impl Listener {
    pub fn bind(address: &str) -> Result<Self> {
        Ok(Self {
            address: address.into(),
            next: pipe(address, true)?,
        })
    }
    pub async fn accept(&mut self) -> Result<Stream> {
        if self.next.connect().await.is_err() {
            self.next = pipe(&self.address, false)?;
            return Err(super::PeerRejected.into());
        }
        let replacement = pipe(&self.address, false)?;
        let server = mem::replace(&mut self.next, replacement);
        let mut pid = 0;
        // SAFETY: connected pipe handle is live and pid is writable native output storage.
        if unsafe { GetNamedPipeClientProcessId(server.as_raw_handle(), &mut pid) } == 0 {
            return Err(super::PeerRejected.into());
        }
        let peer = process_user(pid).map_err(|_| super::PeerRejected)?;
        if peer != current_user()? {
            return Err(super::PeerRejected.into());
        }
        Ok(Stream::Server(server))
    }
}

pub enum Stream {
    Server(NamedPipeServer),
    Client(NamedPipeClient),
}

pub async fn connect(address: &str) -> Result<Stream> {
    ensure!(
        address.starts_with(r"\\.\pipe\airk-1.4-") && address.len() <= 128,
        "invalid local pipe endpoint"
    );
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    let client = loop {
        match ClientOptions::new().open(address) {
            Ok(client) => break client,
            Err(error)
                if error.raw_os_error() == Some(231) && tokio::time::Instant::now() < deadline =>
            {
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
            Err(error) => return Err(error.into()),
        }
    };
    let mut pid = 0;
    // SAFETY: client owns a live pipe; pid is writable native output storage.
    ensure!(
        unsafe { GetNamedPipeServerProcessId(client.as_raw_handle(), &mut pid) } != 0,
        "native server identity unavailable"
    );
    ensure!(
        process_user(pid)? == current_user()?,
        "foreign native pipe server"
    );
    Ok(Stream::Client(client))
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Server(p) => Pin::new(p).poll_read(cx, buffer),
            Self::Client(p) => Pin::new(p).poll_read(cx, buffer),
        }
    }
}
impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut TaskContext<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Server(p) => Pin::new(p).poll_write(cx, buffer),
            Self::Client(p) => Pin::new(p).poll_write(cx, buffer),
        }
    }
    fn poll_flush(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Server(p) => Pin::new(p).poll_flush(cx),
            Self::Client(p) => Pin::new(p).poll_flush(cx),
        }
    }
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Server(p) => Pin::new(p).poll_shutdown(cx),
            Self::Client(p) => Pin::new(p).poll_shutdown(cx),
        }
    }
}
