// SPDX-License-Identifier: Apache-2.0
//! Length-prefixed JSON framing for protected local IPC, not stdio MCP framing.
//!
//! The transport must set deadlines, authenticate the peer and serialize writes.
//! Any codec failure closes the connection; stream resynchronization is unsupported.
use airk_core::service::MAX_FRAME_BYTES;
use serde::{Serialize, de::DeserializeOwned};
use std::io::{self, Read, Write};

struct BoundedBuffer(Vec<u8>);

impl Write for BoundedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_FRAME_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "IPC frame exceeds byte limit",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Serialize under the limit before writing any bytes to the connection.
pub fn write_frame<W: Write, T: Serialize>(writer: &mut W, value: &T) -> io::Result<()> {
    let mut buffer = BoundedBuffer(Vec::new());
    serde_json::to_writer(&mut buffer, value).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid or oversized IPC payload",
        )
    })?;
    writer.write_all(&(buffer.0.len() as u32).to_be_bytes())?;
    writer.write_all(&buffer.0)?;
    writer.flush()
}

/// Returns None only for clean EOF between frames. Truncation fails explicitly.
pub fn read_frame<R: Read, T: DeserializeOwned>(reader: &mut R) -> io::Result<Option<T>> {
    let mut prefix = [0; 4];
    loop {
        match reader.read(&mut prefix[..1]) {
            Ok(0) => return Ok(None),
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    reader.read_exact(&mut prefix[1..])?;
    let length = u32::from_be_bytes(prefix) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid IPC frame length",
        ));
    }
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid IPC JSON"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn frames_preserve_boundaries_and_eof() {
        let mut bytes = Vec::new();
        write_frame(&mut bytes, &"hello").unwrap();
        write_frame(&mut bytes, &vec![1, 2, 3]).unwrap();
        let mut stream = Cursor::new(bytes);
        assert_eq!(
            read_frame::<_, String>(&mut stream).unwrap(),
            Some("hello".into())
        );
        assert_eq!(
            read_frame::<_, Vec<u8>>(&mut stream).unwrap(),
            Some(vec![1, 2, 3])
        );
        assert_eq!(read_frame::<_, String>(&mut stream).unwrap(), None);
    }

    #[test]
    fn rejects_header_before_allocating_or_reading_body() {
        for length in [0u32, MAX_FRAME_BYTES as u32 + 1, u32::MAX] {
            let mut stream = Cursor::new(length.to_be_bytes());
            assert_eq!(
                read_frame::<_, String>(&mut stream).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
            assert_eq!(stream.position(), 4);
        }
    }

    #[test]
    fn rejects_truncation_and_invalid_json() {
        for bytes in [vec![0], vec![0, 0, 0, 5, b'"']] {
            assert_eq!(
                read_frame::<_, String>(&mut Cursor::new(bytes))
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::UnexpectedEof
            );
        }
        assert_eq!(
            read_frame::<_, String>(&mut Cursor::new([0, 0, 0, 1, b'{']))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn serialization_escape_growth_is_bounded_before_output() {
        let mut stream = Vec::new();
        assert!(write_frame(&mut stream, &"\0".repeat(MAX_FRAME_BYTES / 2)).is_err());
        assert!(stream.is_empty());
        let exact = "a".repeat(MAX_FRAME_BYTES - 2);
        write_frame(&mut stream, &exact).unwrap();
        assert_eq!(stream.len(), MAX_FRAME_BYTES + 4);
        assert_eq!(
            read_frame::<_, String>(&mut Cursor::new(stream)).unwrap(),
            Some(exact)
        );
    }
}
