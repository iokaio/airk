// SPDX-License-Identifier: Apache-2.0
//! Same-user peer proof for one TLS connection (review H1).
//!
//! The decision is the pure `admin_contract::peer_proof`; this module makes
//! the platform observation and caches the result per connection. Table
//! parsing is pure and tested on every host; only the table read is
//! platform code. Windows: the connection's owning PID from
//! `GetExtendedTcpTable`, then that process's token user SID. Linux: the
//! `uid` column of `/proc/net/tcp`. Other platforms (macOS) make no
//! observation yet, so the proof is `Unavailable` and the API is refused.
use crate::admin_contract::{PeerObservation, PeerProof, peer_proof};
use std::{
    net::{SocketAddr, SocketAddrV4},
    sync::{Arc, OnceLock},
};

/// The TCP 4-tuple of one accepted connection and its cached proof.
#[derive(Clone)]
pub(crate) struct ConnectionPeer {
    client: SocketAddr,
    server: SocketAddr,
    proof: Arc<OnceLock<PeerProof>>,
}

impl ConnectionPeer {
    /// `client` is the remote (browser) end, `server` the listener's end.
    pub(crate) fn new(client: SocketAddr, server: SocketAddr) -> Self {
        Self {
            client,
            server,
            proof: Arc::new(OnceLock::new()),
        }
    }

    /// A connection whose proof is already known (handler tests).
    #[cfg(test)]
    pub(crate) fn proven(proof: PeerProof) -> Self {
        let unspecified = SocketAddr::from(([127, 0, 0, 1], 0));
        let cell = OnceLock::new();
        let _ = cell.set(proof);
        Self {
            client: unspecified,
            server: unspecified,
            proof: Arc::new(cell),
        }
    }

    /// The connection's proof, observed once and then cached.
    pub(crate) async fn proof(&self) -> PeerProof {
        if let Some(proof) = self.proof.get() {
            return *proof;
        }
        let (client, server) = (self.client, self.server);
        let observed = tokio::task::spawn_blocking(move || observe(client, server))
            .await
            .ok()
            .flatten();
        let proof = observed.as_ref().map_or(PeerProof::Unavailable, peer_proof);
        *self.proof.get_or_init(|| proof)
    }
}

fn v4(address: SocketAddr) -> Option<SocketAddrV4> {
    match address {
        SocketAddr::V4(address) => Some(address),
        SocketAddr::V6(_) => None,
    }
}

/// The owning PID of the row whose local end is `client` and remote end is
/// `server`, in a `MIB_TCPTABLE_OWNERPID` buffer: `dwNumEntries`, then rows
/// of six `u32`s (state, local address, local port, remote address, remote
/// port, PID). Addresses are network-order bytes; ports are the first two
/// bytes of their field in network order.
#[cfg(any(windows, test))]
pub(super) fn windows_owner_pid(
    table: &[u8],
    client: SocketAddrV4,
    server: SocketAddrV4,
) -> Option<u32> {
    let word =
        |offset: usize| -> Option<[u8; 4]> { table.get(offset..offset + 4)?.try_into().ok() };
    let entries = u32::from_ne_bytes(word(0)?) as usize;
    let mut found = None;
    for index in 0..entries {
        let row = 4 + index * 24;
        let (Some(local), Some(local_port), Some(remote), Some(remote_port), Some(pid)) = (
            word(row + 4),
            word(row + 8),
            word(row + 12),
            word(row + 16),
            word(row + 20),
        ) else {
            return None;
        };
        let port = |field: [u8; 4]| u16::from_be_bytes([field[0], field[1]]);
        if local == client.ip().octets()
            && port(local_port) == client.port()
            && remote == server.ip().octets()
            && port(remote_port) == server.port()
        {
            // Two rows for one 4-tuple cannot name one owner: fail closed.
            if found.is_some() {
                return None;
            }
            found = Some(u32::from_ne_bytes(pid));
        }
    }
    found
}

/// The `uid` of the `/proc/net/tcp` row whose local end is `client` and
/// remote end is `server`. Addresses are the host-order hex of the
/// network-order address; ports are plain hex.
#[cfg(any(target_os = "linux", test))]
pub(super) fn linux_peer_uid(
    table: &str,
    client: SocketAddrV4,
    server: SocketAddrV4,
) -> Option<u32> {
    let end = |text: &str| -> Option<SocketAddrV4> {
        let (address, port) = text.split_once(':')?;
        let address = u32::from_str_radix(address, 16).ok()?.to_ne_bytes();
        let port = u16::from_str_radix(port, 16).ok()?;
        Some(SocketAddrV4::new(address.into(), port))
    };
    let mut found = None;
    for line in table.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let (Some(local), Some(remote), Some(uid)) = (fields.get(1), fields.get(2), fields.get(7))
        else {
            continue;
        };
        if end(local) == Some(client) && end(remote) == Some(server) {
            if found.is_some() {
                return None;
            }
            found = Some(uid.parse::<u32>().ok()?);
        }
    }
    found
}

#[cfg(windows)]
fn observe(client: SocketAddr, server: SocketAddr) -> Option<PeerObservation> {
    let (client, server) = (v4(client)?, v4(server)?);
    let service_sid = crate::local_ipc::current_user().ok()?;
    let peer_sid = tcp_table()
        .and_then(|table| windows_owner_pid(&table, client, server))
        .filter(|pid| *pid != 0)
        .and_then(|pid| crate::local_ipc::process_user(pid).ok());
    Some(PeerObservation::Windows {
        service_sid,
        peer_sid,
    })
}

/// The IPv4 TCP table with owning PIDs, as raw bytes.
#[cfg(windows)]
fn tcp_table() -> Option<Vec<u8>> {
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, TCP_TABLE_OWNER_PID_ALL,
    };
    const AF_INET: u32 = 2;
    const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
    let mut size: u32 = 4096;
    for _ in 0..4 {
        let mut buffer = vec![0u32; (size as usize).div_ceil(4)];
        let mut length = u32::try_from(buffer.len() * 4).ok()?;
        // SAFETY: the buffer is writable, 4-byte aligned and `length` bytes
        // long; the size pointer and scalar arguments live for the call.
        let status = unsafe {
            GetExtendedTcpTable(
                buffer.as_mut_ptr().cast(),
                &mut length,
                0,
                AF_INET,
                TCP_TABLE_OWNER_PID_ALL,
                0,
            )
        };
        if status == 0 {
            return Some(buffer.iter().flat_map(|word| word.to_ne_bytes()).collect());
        }
        if status != ERROR_INSUFFICIENT_BUFFER {
            return None;
        }
        // The table can grow between calls; leave room for new rows.
        size = length.checked_add(4096)?;
    }
    None
}

#[cfg(target_os = "linux")]
fn observe(client: SocketAddr, server: SocketAddr) -> Option<PeerObservation> {
    use std::io::Read;
    let (client, server) = (v4(client)?, v4(server)?);
    // SAFETY: geteuid takes no pointers and has no failure mode.
    let service_uid = unsafe { libc::geteuid() };
    let mut table = String::new();
    std::fs::File::open("/proc/net/tcp")
        .ok()?
        .take(64 << 20)
        .read_to_string(&mut table)
        .ok()?;
    Some(PeerObservation::Linux {
        service_uid,
        peer_uid: linux_peer_uid(&table, client, server),
    })
}

/// No qualified observer on this platform (macOS: H1 names
/// `proc_listpids`/`proc_pidfdinfo`, not implemented yet).
#[cfg(not(any(windows, target_os = "linux")))]
fn observe(client: SocketAddr, server: SocketAddr) -> Option<PeerObservation> {
    let _ = (v4(client), v4(server));
    None
}
