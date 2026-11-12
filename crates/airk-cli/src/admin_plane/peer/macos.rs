// SPDX-License-Identifier: Apache-2.0
//! macOS own-socket enumeration for the same-user peer proof (review H1).
//!
//! Thin FFI only: list the PIDs whose effective uid is the service's
//! (`proc_listpids(PROC_UID_ONLY)`), their file descriptors
//! (`proc_pidinfo(PROC_PIDLISTFDS)`) and, for socket descriptors, the TCP
//! endpoints (`proc_pidfdinfo(PROC_PIDFDSOCKETINFO)`). The match decision is
//! the pure `super::macos_found_among`. Work is bounded by `MAX_PIDS`,
//! `MAX_FDS_PER_PID` and `MAX_SOCKETS`; a scan that hits a bound reports
//! only what it saw, so a missing match stays unavailable (fail closed).
use super::OwnSocket;
use libc::{c_int, c_void, proc_fdinfo};
use std::{mem::size_of, net::SocketAddrV4};

const MAX_PIDS: usize = 8192;
const MAX_FDS_PER_PID: usize = 16384;
const MAX_SOCKETS: usize = 65536;

// <sys/proc_info.h> values not exported by `libc`.
const PROC_UID_ONLY: u32 = 4;
const PROC_PIDFDSOCKETINFO: c_int = 3;
const SOCKINFO_TCP: c_int = 2;
const INI_IPV4: u8 = 0x1;
const INI_IPV6: u8 = 0x2;

// Minimal `#[repr(C)]` mirrors of <sys/proc_info.h> (xnu, LP64 layout).
// Field names follow the header; only the TCP endpoint fields are read.
// The size assertions below pin the layout to the header's.

/// `struct proc_fileinfo` (24 bytes).
#[repr(C)]
#[allow(dead_code)]
struct ProcFileinfo {
    fi_openflags: u32,
    fi_status: u32,
    fi_offset: i64,
    fi_type: i32,
    fi_guardflags: u32,
}

/// `struct vinfo_stat` (136 bytes).
#[repr(C)]
#[allow(dead_code)]
struct VinfoStat {
    vst_dev: u32,
    vst_mode: u16,
    vst_nlink: u16,
    vst_ino: u64,
    vst_uid: u32,
    vst_gid: u32,
    /// `vst_atime` .. `vst_birthtimensec`: eight `int64_t`.
    vst_times: [i64; 8],
    vst_size: i64,
    vst_blocks: i64,
    vst_blksize: i32,
    vst_flags: u32,
    vst_gen: u32,
    vst_rdev: u32,
    vst_qspare: [i64; 2],
}

/// `struct sockbuf_info` (24 bytes).
#[repr(C)]
#[allow(dead_code)]
struct SockbufInfo {
    sbi_cc: u32,
    sbi_hiwat: u32,
    sbi_mbcnt: u32,
    sbi_mbmax: u32,
    sbi_lowat: u32,
    sbi_flags: i16,
    sbi_timeo: i16,
}

/// `struct in_sockinfo` (80 bytes). `insi_faddr`/`insi_laddr` are the
/// 16-byte `in4in6_addr`/`in6_addr` union: an IPv4 address is the last four
/// bytes. Ports are `int`s holding a network-order `u_short`.
#[repr(C)]
#[allow(dead_code)]
struct InSockinfo {
    insi_fport: c_int,
    insi_lport: c_int,
    insi_gencnt: u64,
    insi_flags: u32,
    insi_flow: u32,
    insi_vflag: u8,
    insi_ip_ttl: u8,
    rfu_1: u32,
    insi_faddr: [u8; 16],
    insi_laddr: [u8; 16],
    /// `insi_v4.in4_tos`.
    insi_v4_tos: u8,
    insi_v6: InSockinfoV6,
}

/// The anonymous `insi_v6` struct of `in_sockinfo` (12 bytes).
#[repr(C)]
#[allow(dead_code)]
struct InSockinfoV6 {
    in6_hlim: u8,
    in6_cksum: c_int,
    in6_ifindex: u16,
    in6_hops: i16,
}

/// `struct tcp_sockinfo` (120 bytes).
#[repr(C)]
#[allow(dead_code)]
struct TcpSockinfo {
    tcpsi_ini: InSockinfo,
    tcpsi_state: c_int,
    tcpsi_timer: [c_int; 4],
    tcpsi_mss: c_int,
    tcpsi_flags: u32,
    rfu_1: u32,
    tcpsi_tp: u64,
}

/// `soi_proto`: the largest member is `un_sockinfo` (528 bytes).
#[repr(C)]
#[allow(dead_code)]
union SoiProto {
    pri_tcp: std::mem::ManuallyDrop<TcpSockinfo>,
    pad: [u64; 66],
}

/// `struct socket_info` (768 bytes).
#[repr(C)]
#[allow(dead_code)]
struct SocketInfo {
    soi_stat: VinfoStat,
    soi_so: u64,
    soi_pcb: u64,
    soi_type: c_int,
    soi_protocol: c_int,
    soi_family: c_int,
    soi_options: i16,
    soi_linger: i16,
    soi_state: i16,
    soi_qlen: i16,
    soi_incqlen: i16,
    soi_qlimit: i16,
    soi_timeo: i16,
    soi_error: u16,
    soi_oobmark: u32,
    soi_rcv: SockbufInfo,
    soi_snd: SockbufInfo,
    soi_kind: c_int,
    rfu_1: u32,
    soi_proto: SoiProto,
}

/// `struct socket_fdinfo` (792 bytes).
#[repr(C)]
#[allow(dead_code)]
struct SocketFdinfo {
    pfi: ProcFileinfo,
    psi: SocketInfo,
}

const _: () = assert!(size_of::<ProcFileinfo>() == 24);
const _: () = assert!(size_of::<VinfoStat>() == 136);
const _: () = assert!(size_of::<SockbufInfo>() == 24);
const _: () = assert!(size_of::<InSockinfoV6>() == 12);
const _: () = assert!(std::mem::offset_of!(InSockinfo, insi_faddr) == 32);
const _: () = assert!(std::mem::offset_of!(InSockinfo, insi_laddr) == 48);
const _: () = assert!(std::mem::offset_of!(InSockinfo, insi_v6) == 68);
const _: () = assert!(size_of::<InSockinfo>() == 80);
const _: () = assert!(std::mem::offset_of!(SocketInfo, soi_kind) == 232);
const _: () = assert!(std::mem::offset_of!(SocketInfo, soi_proto) == 240);
const _: () = assert!(size_of::<TcpSockinfo>() == 120);
const _: () = assert!(size_of::<SoiProto>() == 528);
const _: () = assert!(size_of::<SocketInfo>() == 768);
const _: () = assert!(size_of::<SocketFdinfo>() == 792);

/// TCP sockets held by processes whose effective uid is `uid`; `None` when
/// the PID list cannot be read. Unreadable processes and descriptors (for
/// example ones that exit or close during the scan) are skipped.
pub(super) fn own_tcp_sockets(uid: u32) -> Option<Vec<OwnSocket>> {
    let mut sockets = Vec::new();
    for pid in uid_pids(uid)? {
        for fd in socket_fds(pid).unwrap_or_default() {
            if sockets.len() >= MAX_SOCKETS {
                return Some(sockets);
            }
            if let Some(socket) = tcp_endpoints(pid, fd) {
                sockets.push(socket);
            }
        }
    }
    Some(sockets)
}

fn uid_pids(uid: u32) -> Option<Vec<c_int>> {
    // SAFETY: a null buffer of size 0 asks only for the required size.
    let needed = unsafe { libc::proc_listpids(PROC_UID_ONLY, uid, std::ptr::null_mut(), 0) };
    let needed = usize::try_from(needed).ok()?;
    // Processes can start between calls; leave headroom.
    let count = (needed / size_of::<c_int>() + 64).min(MAX_PIDS);
    let mut pids = vec![0 as c_int; count];
    let bytes = c_int::try_from(pids.len() * size_of::<c_int>()).ok()?;
    // SAFETY: `pids` is writable for `bytes` bytes and outlives the call.
    let written = unsafe {
        libc::proc_listpids(
            PROC_UID_ONLY,
            uid,
            pids.as_mut_ptr().cast::<c_void>(),
            bytes,
        )
    };
    let written = usize::try_from(written).ok()? / size_of::<c_int>();
    pids.truncate(written.min(count));
    pids.retain(|pid| *pid > 0);
    Some(pids)
}

fn socket_fds(pid: c_int) -> Option<Vec<c_int>> {
    let entry = size_of::<proc_fdinfo>();
    // SAFETY: a null buffer of size 0 asks only for the required size.
    let needed =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
    let needed = usize::try_from(needed).ok()?;
    let count = (needed / entry + 16).min(MAX_FDS_PER_PID);
    let mut fds: Vec<proc_fdinfo> = (0..count)
        .map(|_| proc_fdinfo {
            proc_fd: 0,
            proc_fdtype: 0,
        })
        .collect();
    let bytes = c_int::try_from(count * entry).ok()?;
    // SAFETY: `fds` is writable for `bytes` bytes and outlives the call.
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDLISTFDS,
            0,
            fds.as_mut_ptr().cast::<c_void>(),
            bytes,
        )
    };
    let written = usize::try_from(written).ok()? / entry;
    fds.truncate(written.min(count));
    let socket = u32::try_from(libc::PROX_FDTYPE_SOCKET).ok()?;
    Some(
        fds.iter()
            .filter(|fd| fd.proc_fdtype == socket)
            .map(|fd| fd.proc_fd)
            .collect(),
    )
}

fn tcp_endpoints(pid: c_int, fd: c_int) -> Option<OwnSocket> {
    let mut info = std::mem::MaybeUninit::<SocketFdinfo>::zeroed();
    let size = c_int::try_from(size_of::<SocketFdinfo>()).ok()?;
    // SAFETY: `info` is writable for `size` bytes and outlives the call.
    let written = unsafe {
        libc::proc_pidfdinfo(
            pid,
            fd,
            PROC_PIDFDSOCKETINFO,
            info.as_mut_ptr().cast::<c_void>(),
            size,
        )
    };
    if written != size {
        return None;
    }
    // SAFETY: the struct started zeroed and the kernel filled all of it;
    // every field is plain data for which any bit pattern is valid.
    let info = unsafe { info.assume_init() };
    if info.psi.soi_kind != SOCKINFO_TCP {
        return None;
    }
    // SAFETY: `soi_kind == SOCKINFO_TCP` selects `pri_tcp`; both union
    // members are plain data.
    let ini = unsafe { &info.psi.soi_proto.pri_tcp.tcpsi_ini };
    let local = endpoint(ini.insi_vflag, &ini.insi_laddr, ini.insi_lport)?;
    let remote = endpoint(ini.insi_vflag, &ini.insi_faddr, ini.insi_fport)?;
    Some(OwnSocket { local, remote })
}

/// An IPv4 (or IPv4-mapped IPv6) endpoint from `in_sockinfo` fields.
fn endpoint(vflag: u8, address: &[u8; 16], port: c_int) -> Option<SocketAddrV4> {
    let mapped = vflag & INI_IPV6 != 0
        && address[..10].iter().all(|byte| *byte == 0)
        && address[10..12] == [0xff, 0xff];
    if vflag & INI_IPV4 == 0 && !mapped {
        return None;
    }
    let ip: [u8; 4] = address[12..16].try_into().ok()?;
    // The int holds a network-order u_short (as lsof reads it:
    // `ntohs((u_short)insi_lport)`); truncation keeps those 16 bits.
    let port = u16::from_be(port as u16);
    Some(SocketAddrV4::new(ip.into(), port))
}
