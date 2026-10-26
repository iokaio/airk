// SPDX-License-Identifier: Apache-2.0
//! Native boot/scope evidence and descriptor transfer for private socket pairs.
use airk_store::service::registry::ProviderBoot;
use std::{
    fs::File,
    io,
    os::{
        fd::{AsRawFd, FromRawFd, OwnedFd},
        unix::net::UnixStream,
    },
};

pub(super) fn boot() -> io::Result<ProviderBoot> {
    let mut time = std::mem::MaybeUninit::<libc::timespec>::uninit();
    // SAFETY: correctly sized output for a read-only monotonic clock query.
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, time.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    let time = unsafe { time.assume_init() };
    let elapsed_ns = u64::try_from(time.tv_sec)
        .ok()
        .and_then(|v| v.checked_mul(1_000_000_000))
        .and_then(|v| v.checked_add(time.tv_nsec as u64))
        .ok_or_else(|| io::Error::other("invalid native uptime"))?;
    #[cfg(target_os = "linux")]
    let (boot_id, process_scope, monotonic_reset_proves_boot) = {
        use std::io::Read;
        let mut bytes = Vec::new();
        File::open("/proc/sys/kernel/random/boot_id")?
            .take(81)
            .read_to_end(&mut bytes)?;
        let id = String::from_utf8(bytes)
            .map_err(io::Error::other)?
            .trim()
            .to_owned();
        let scope = std::fs::read_link("/proc/self/ns/pid")?
            .to_str()
            .ok_or_else(|| io::Error::other("invalid PID namespace"))?
            .to_owned();
        (Some(id), scope, false)
    };
    #[cfg(target_os = "macos")]
    let (boot_id, process_scope, monotonic_reset_proves_boot) = {
        let mut bytes = [0u8; 81];
        let mut size = bytes.len();
        // SAFETY: fixed NUL-terminated sysctl name and bounded output; no write input.
        let ok = unsafe {
            libc::sysctlbyname(
                c"kern.bootsessionuuid".as_ptr(),
                bytes.as_mut_ptr().cast(),
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        } == 0;
        let id = if ok && size > 1 && size <= bytes.len() {
            std::str::from_utf8(&bytes[..size - 1])
                .ok()
                .map(str::to_owned)
        } else {
            None
        };
        (id, "darwin-global".to_owned(), true)
    };
    let boot = ProviderBoot {
        boot_id,
        process_scope,
        elapsed_ns,
        monotonic_reset_proves_boot,
    };
    boot.validate().map_err(io::Error::other)?;
    Ok(boot)
}

pub(super) fn absent(group: u32) -> anyhow::Result<bool> {
    anyhow::ensure!(
        group > 1 && group <= i32::MAX as u32,
        "invalid provider group probe"
    );
    // SAFETY: signal zero only observes existence; it never signals a reused group.
    if unsafe { libc::kill(-(group as i32), 0) } == 0 {
        return Ok(false);
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(libc::ESRCH) => Ok(true),
        Some(libc::EPERM) => Ok(false),
        _ => Err(error.into()),
    }
}

pub(super) fn members(groups: &[u32]) -> io::Result<Vec<u32>> {
    use std::{
        collections::BTreeSet,
        time::{Duration, Instant},
    };
    if groups.len() > 128 {
        return Err(io::Error::other("provider group observation bound"));
    }
    if groups.is_empty() {
        return Ok(Vec::new());
    }
    let groups: BTreeSet<_> = groups.iter().copied().collect();
    let until = Instant::now() + Duration::from_millis(250);
    let mut result = BTreeSet::new();
    #[cfg(target_os = "linux")]
    for (index, entry) in std::fs::read_dir("/proc")?.enumerate() {
        if index >= 131_072 || Instant::now() >= until {
            return Err(io::Error::other("provider process census bound"));
        }
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<i32>().ok())
            .filter(|pid| *pid > 0)
        else {
            continue;
        };
        // SAFETY: read-only process group lookup, never permission to signal a PID.
        let group = unsafe { libc::getpgid(pid) };
        if group < 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ESRCH) {
                continue;
            }
            return Err(error);
        }
        if groups.contains(&(group as u32)) {
            result.insert(pid as u32);
        }
    }
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn proc_listpgrppids(
                group: libc::pid_t,
                buffer: *mut libc::c_void,
                bytes: libc::c_int,
            ) -> libc::c_int;
        }
        for group in groups {
            if Instant::now() >= until {
                return Err(io::Error::other("provider process census deadline"));
            }
            let mut pids = [0i32; 1025];
            // SAFETY: bounded writable process-list buffer; the API returns a count.
            let count = unsafe {
                *libc::__error() = 0;
                proc_listpgrppids(
                    group as i32,
                    pids.as_mut_ptr().cast(),
                    std::mem::size_of_val(&pids) as i32,
                )
            };
            if count < 0
                || count as usize >= pids.len()
                || count == 0 && unsafe { *libc::__error() } != 0
            {
                return Err(io::Error::other("provider process census unavailable"));
            }
            result.extend(
                pids.into_iter()
                    .take(count as usize)
                    .filter(|pid| *pid > 0)
                    .map(|pid| pid as u32),
            );
        }
    }
    Ok(result.into_iter().collect())
}

pub(super) fn send_files(socket: &UnixStream, files: &[&File; 4]) -> io::Result<()> {
    let descriptors = files.map(AsRawFd::as_raw_fd);
    let mut storage = [0usize; 16];
    let mut byte = [0x41u8];
    let mut iov = libc::iovec {
        iov_base: byte.as_mut_ptr().cast(),
        iov_len: 1,
    };
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = storage.as_mut_ptr().cast();
    message.msg_controllen =
        unsafe { libc::CMSG_SPACE(std::mem::size_of_val(&descriptors) as u32) } as _;
    // SAFETY: aligned ancillary storage has space for exactly these four borrowed descriptors.
    unsafe {
        let header = libc::CMSG_FIRSTHDR(&message);
        (*header).cmsg_level = libc::SOL_SOCKET;
        (*header).cmsg_type = libc::SCM_RIGHTS;
        (*header).cmsg_len = libc::CMSG_LEN(std::mem::size_of_val(&descriptors) as u32) as _;
        std::ptr::copy_nonoverlapping(
            descriptors.as_ptr().cast::<u8>(),
            libc::CMSG_DATA(header),
            std::mem::size_of_val(&descriptors),
        );
        if libc::sendmsg(socket.as_raw_fd(), &message, 0) != 1 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

pub(super) fn receive_files(socket: &UnixStream) -> io::Result<[File; 4]> {
    let mut storage = [0usize; 16];
    let mut byte = [0u8];
    let mut iov = libc::iovec {
        iov_base: byte.as_mut_ptr().cast(),
        iov_len: 1,
    };
    let mut message: libc::msghdr = unsafe { std::mem::zeroed() };
    message.msg_iov = &mut iov;
    message.msg_iovlen = 1;
    message.msg_control = storage.as_mut_ptr().cast();
    message.msg_controllen = std::mem::size_of_val(&storage) as _;
    #[cfg(target_os = "linux")]
    let flags = libc::MSG_CMSG_CLOEXEC;
    #[cfg(not(target_os = "linux"))]
    let flags = 0;
    // SAFETY: sized writable buffers; received descriptor ownership is immediately captured below.
    let count = unsafe { libc::recvmsg(socket.as_raw_fd(), &mut message, flags) };
    if count < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut files = Vec::new();
    let mut valid = count == 1
        && byte[0] == 0x41
        && message.msg_flags & (libc::MSG_CTRUNC | libc::MSG_TRUNC) == 0;
    unsafe {
        let mut header = libc::CMSG_FIRSTHDR(&message);
        while !header.is_null() {
            if (*header).cmsg_level == libc::SOL_SOCKET && (*header).cmsg_type == libc::SCM_RIGHTS {
                let size = ((*header).cmsg_len as usize).saturating_sub(libc::CMSG_LEN(0) as usize);
                valid &= size.is_multiple_of(std::mem::size_of::<i32>());
                for index in 0..size / std::mem::size_of::<i32>() {
                    let fd =
                        std::ptr::read_unaligned(libc::CMSG_DATA(header).cast::<i32>().add(index));
                    let owned = OwnedFd::from_raw_fd(fd);
                    if libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) < 0 {
                        valid = false
                    }
                    files.push(File::from(owned));
                }
            } else {
                valid = false
            }
            header = libc::CMSG_NXTHDR(&message, header);
        }
    }
    if !valid || files.len() != 4 {
        return Err(io::Error::other("invalid private guardian descriptors"));
    }
    files
        .try_into()
        .map_err(|_| io::Error::other("invalid guardian descriptor count"))
}
