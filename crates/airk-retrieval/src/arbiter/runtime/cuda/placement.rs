// SPDX-License-Identifier: Apache-2.0
//! Final SessionState graph placement from the same-core owning C API extension.
use super::*;
#[cfg(any(
    test,
    all(
        target_arch = "x86_64",
        any(windows, all(target_os = "linux", target_env = "gnu"))
    )
))]
use std::ffi::c_void;
#[cfg(any(
    test,
    all(
        target_arch = "x86_64",
        any(windows, all(target_os = "linux", target_env = "gnu"))
    )
))]
const MAX_NODES: u64 = 65536;
#[cfg(any(
    test,
    all(
        target_arch = "x86_64",
        any(windows, all(target_os = "linux", target_env = "gnu"))
    )
))]
const MAX_BYTES: u64 = 16 << 20;
#[derive(Clone, Debug, Serialize)]
pub struct PlacedNode {
    pub subgraph_index: u64,
    pub name: String,
    pub domain: String,
    pub op: String,
    pub ep: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct PlacementObservation {
    pub scope: &'static str,
    pub complete: bool,
    pub shim_status: i32,
    pub ort_error_code: i32,
    pub reported_nodes: u64,
    pub reported_string_bytes: u64,
    pub nodes: Vec<PlacedNode>,
}
impl PlacementObservation {
    pub fn validate_strict(&self) -> Result<()> {
        ensure!(
            self.complete
                && !self.nodes.is_empty()
                && self.nodes.iter().all(|n| n.ep == "CUDAExecutionProvider"),
            "native final SessionState assignment is incomplete or not strictly CUDA"
        );
        Ok(())
    }
}
#[cfg(any(
    test,
    all(
        target_arch = "x86_64",
        any(windows, all(target_os = "linux", target_env = "gnu"))
    )
))]
struct Sink {
    nodes: Vec<PlacedNode>,
    bytes: u64,
    failed: bool,
}
#[cfg(any(
    test,
    all(
        target_arch = "x86_64",
        any(windows, all(target_os = "linux", target_env = "gnu"))
    )
))]
unsafe fn text(pointer: *const u8, len: usize) -> Result<String> {
    ensure!(
        len <= 4096 && (len == 0 || !pointer.is_null()),
        "invalid native placement string"
    );
    if len == 0 {
        return Ok(String::new());
    }
    // SAFETY: admitted native shim owns each borrowed string through callback.
    Ok(std::str::from_utf8(unsafe { std::slice::from_raw_parts(pointer, len) })?.to_owned())
}
#[cfg(all(
    target_arch = "x86_64",
    any(windows, all(target_os = "linux", target_env = "gnu"))
))]
type Callback = unsafe extern "C" fn(
    *mut c_void,
    u64,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
    *const u8,
    usize,
) -> i32;
#[cfg(all(
    target_arch = "x86_64",
    any(windows, all(target_os = "linux", target_env = "gnu"))
))]
pub(super) type Collector = unsafe extern "C" fn(
    *const c_void,
    *const c_void,
    Callback,
    *mut c_void,
    u64,
    u64,
    *mut u64,
    *mut u64,
    *mut i32,
) -> i32;
#[cfg(any(
    test,
    all(
        target_arch = "x86_64",
        any(windows, all(target_os = "linux", target_env = "gnu"))
    )
))]
unsafe extern "C" fn collect_row(
    user: *mut c_void,
    index: u64,
    name: *const u8,
    name_len: usize,
    domain: *const u8,
    domain_len: usize,
    op: *const u8,
    op_len: usize,
    ep: *const u8,
    ep_len: usize,
) -> i32 {
    if user.is_null() {
        return 1;
    }
    // SAFETY: one synchronous call lends this live unique Sink; no async callback.
    let sink = unsafe { &mut *user.cast::<Sink>() };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
        ensure!(
            !sink.failed && (sink.nodes.len() as u64) < MAX_NODES,
            "placement node capacity"
        );
        let bytes = [name_len, domain_len, op_len, ep_len]
            .iter()
            .try_fold(0u64, |a, b| {
                a.checked_add(*b as u64).context("placement overflow")
            })?;
        let total = sink
            .bytes
            .checked_add(bytes)
            .context("placement overflow")?;
        ensure!(total <= MAX_BYTES, "placement string capacity");
        let row = PlacedNode {
            subgraph_index: index,
            name: unsafe { text(name, name_len)? },
            domain: unsafe { text(domain, domain_len)? },
            op: unsafe { text(op, op_len)? },
            ep: unsafe { text(ep, ep_len)? },
        };
        ensure!(
            !row.op.is_empty() && !row.ep.is_empty(),
            "native placement op/provider missing"
        );
        sink.bytes = total;
        sink.nodes.push(row);
        Ok(())
    }));
    if matches!(result, Ok(Ok(()))) {
        0
    } else {
        sink.failed = true;
        1
    }
}
#[cfg(all(
    target_arch = "x86_64",
    any(windows, all(target_os = "linux", target_env = "gnu"))
))]
pub(super) unsafe fn observe(
    collect: Collector,
    session: &ort::session::Session,
) -> PlacementObservation {
    use ort::AsPointer;
    let mut sink = Sink {
        nodes: Vec::new(),
        bytes: 0,
        failed: false,
    };
    let (mut nodes, mut bytes, mut error) = (0, 0, 0);
    // SAFETY: same statically linked core API base and live Session under owning
    // Mutex. Extension uses exact owning InferenceSession/SessionState accessors, callback is synchronous and bounded.
    let status = unsafe {
        collect(
            ort::sys::OrtGetApiBase().cast(),
            session.ptr().cast(),
            collect_row,
            (&mut sink as *mut Sink).cast(),
            MAX_NODES,
            MAX_BYTES,
            &mut nodes,
            &mut bytes,
            &mut error,
        )
    };
    PlacementObservation {
        scope: "onnxruntime-final-session-state-ep-placement",
        complete: status == 0
            && error == 0
            && !sink.failed
            && nodes == sink.nodes.len() as u64
            && bytes == sink.bytes
            && nodes > 0,
        shim_status: status,
        ort_error_code: error,
        reported_nodes: nodes,
        reported_string_bytes: bytes,
        nodes: sink.nodes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_or_cpu_assignment_cannot_be_a_strict_gpu_proof() {
        let mut p = PlacementObservation {
            scope: "synthetic",
            complete: true,
            shim_status: 0,
            ort_error_code: 0,
            reported_nodes: 1,
            reported_string_bytes: 0,
            nodes: vec![PlacedNode {
                subgraph_index: 0,
                name: "independent".into(),
                domain: "".into(),
                op: "MatMul".into(),
                ep: "CPUExecutionProvider".into(),
            }],
        };
        assert!(p.validate_strict().is_err());
        p.nodes[0].ep = "CUDAExecutionProvider".into();
        assert!(p.validate_strict().is_ok());
        p.complete = false;
        assert!(p.validate_strict().is_err());
    }
    #[test]
    fn callback_refuses_invalid_strings_and_never_unwinds() {
        let mut sink = Sink {
            nodes: Vec::new(),
            bytes: 0,
            failed: false,
        };
        let p = (&mut sink as *mut Sink).cast();
        assert_ne!(
            unsafe {
                collect_row(
                    p,
                    0,
                    std::ptr::null(),
                    1,
                    std::ptr::null(),
                    0,
                    b"MatMul".as_ptr(),
                    6,
                    b"CUDAExecutionProvider".as_ptr(),
                    21,
                )
            },
            0
        );
        assert!(sink.failed);
        assert!(sink.nodes.is_empty());
    }
}

#[derive(Debug)]
pub struct PlacementRefusal {
    pub observation: PlacementObservation,
}
impl std::fmt::Display for PlacementRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "native final placement refused: status{} ort{} complete{} rows{}",
            self.observation.shim_status,
            self.observation.ort_error_code,
            self.observation.complete,
            self.observation.nodes.len()
        )
    }
}
impl std::error::Error for PlacementRefusal {}
impl PlacementObservation {
    pub(crate) fn require_strict(self) -> Result<()> {
        if self.validate_strict().is_err() {
            return Err(PlacementRefusal { observation: self }.into());
        }
        Ok(())
    }
}
