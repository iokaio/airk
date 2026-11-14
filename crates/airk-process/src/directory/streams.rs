// SPDX-License-Identifier: Apache-2.0
use std::{fs::File, io, path::Path};

/// Observe the exact retained object, including while reversible deletion is
/// pending. No path-based second observer can race the owning handle.
pub fn require_default_stream(_path: &Path, file: &File) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{FileStreamInfo, GetFileInformationByHandleEx};
    // An accepted file has exactly one short default-stream record. Any larger
    // stream inventory, malformed/truncated query or extra record refuses.
    let mut buffer = [0_u64; 512];
    // SAFETY: actual retained handle and aligned bounded writable buffer.
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle().cast(),
            FileStreamInfo,
            buffer.as_mut_ptr().cast(),
            std::mem::size_of_val(&buffer) as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the initialized aligned buffer is valid for its complete byte size.
    let bytes = unsafe {
        std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), std::mem::size_of_val(&buffer))
    };
    let next = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    let length = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let size = i64::from_le_bytes(bytes[8..16].try_into().unwrap());
    let default: Vec<u8> = "::$DATA"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    if next != 0
        || usize::try_from(length).ok() != Some(default.len())
        || bytes.get(24..24 + default.len()) != Some(default.as_slice())
        || u64::try_from(size).ok() != Some(file.metadata()?.len())
    {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "alternate streams require manual recovery",
        ));
    }
    Ok(())
}
