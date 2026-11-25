// SPDX-License-Identifier: Apache-2.0
//! The canonical ONNX keeps CPU arithmetic and one half-weight payload.
//! Its separately compiled CUDA view refers to ranges of that same payload.
use anyhow::{Context, Result, bail, ensure};

const CUDA_GRAPH_KEY: &[u8] = b"airk.cuda_graph.onnx.base64";
const MAX_CUDA_GRAPH: usize = 1 << 20;

pub(super) fn cuda_graph(canonical_asset: &[u8]) -> Result<Vec<u8>> {
    let mut graph = None;
    let mut model = canonical_asset;
    while let Some(WireField { field, bytes }) = next_field(&mut model)? {
        if field != 14 {
            continue;
        }
        let mut entry = bytes.context("invalid ONNX metadata entry")?;
        let mut key = None;
        let mut value = None;
        while let Some(WireField { field, bytes }) = next_field(&mut entry)? {
            match field {
                1 => ensure!(key.replace(bytes).is_none(), "duplicate ONNX metadata key"),
                2 => ensure!(
                    value.replace(bytes).is_none(),
                    "duplicate ONNX metadata value"
                ),
                _ => bail!("unexpected ONNX metadata field"),
            }
        }
        if key.flatten() == Some(CUDA_GRAPH_KEY) {
            let encoded = value.flatten().context("missing MiniLM CUDA graph")?;
            ensure!(
                graph.replace(encoded).is_none(),
                "duplicate MiniLM CUDA graph"
            );
        }
    }
    decode_graph(graph.context("missing MiniLM CUDA graph metadata")?)
}

struct WireField<'a> {
    field: u64,
    bytes: Option<&'a [u8]>,
}

fn next_field<'a>(input: &mut &'a [u8]) -> Result<Option<WireField<'a>>> {
    if input.is_empty() {
        return Ok(None);
    }
    let tag = varint(input)?;
    let field = tag >> 3;
    ensure!((1..(1 << 29)).contains(&field), "invalid ONNX field tag");
    let bytes = match tag & 7 {
        0 => {
            varint(input)?;
            None
        }
        1 => {
            take(input, 8)?;
            None
        }
        2 => {
            let length = usize::try_from(varint(input)?)?;
            Some(take(input, length)?)
        }
        5 => {
            take(input, 4)?;
            None
        }
        _ => bail!("unsupported ONNX field wire type"),
    };
    Ok(Some(WireField { field, bytes }))
}

fn take<'a>(input: &mut &'a [u8], length: usize) -> Result<&'a [u8]> {
    let (bytes, rest) = input
        .split_at_checked(length)
        .context("truncated ONNX field")?;
    *input = rest;
    Ok(bytes)
}

fn varint(input: &mut &[u8]) -> Result<u64> {
    let mut value = 0;
    for shift in (0..64).step_by(7) {
        let byte = take(input, 1)?[0];
        ensure!(shift != 63 || byte <= 1, "overflowing ONNX integer");
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    bail!("overflowing ONNX integer")
}

fn decode_graph(encoded: &[u8]) -> Result<Vec<u8>> {
    let maximum = MAX_CUDA_GRAPH.div_ceil(3) * 4;
    ensure!(
        !encoded.is_empty() && encoded.len() <= maximum && encoded.len().is_multiple_of(4),
        "invalid MiniLM CUDA graph metadata length"
    );
    let mut decoded = Vec::with_capacity(encoded.len() / 4 * 3);
    let mut groups = encoded.as_chunks::<4>().0.iter().peekable();
    while let Some(group) = groups.next() {
        let a = digit(group[0])?;
        let b = digit(group[1])?;
        decoded.push((a << 2) | (b >> 4));
        if group[2..] == *b"==" {
            ensure!(
                groups.peek().is_none() && b & 15 == 0,
                "invalid base64 padding"
            );
            break;
        }
        let c = digit(group[2])?;
        decoded.push((b << 4) | (c >> 2));
        if group[3] == b'=' {
            ensure!(
                groups.peek().is_none() && c & 3 == 0,
                "invalid base64 padding"
            );
            break;
        }
        decoded.push((c << 6) | digit(group[3])?);
    }
    ensure!(
        decoded.len() <= MAX_CUDA_GRAPH,
        "MiniLM CUDA graph exceeds bound"
    );
    Ok(decoded)
}

fn digit(value: u8) -> Result<u8> {
    Ok(match value {
        b'A'..=b'Z' => value - b'A',
        b'a'..=b'z' => value - b'a' + 26,
        b'0'..=b'9' => value - b'0' + 52,
        b'+' => 62,
        b'/' => 63,
        _ => bail!("invalid base64 graph alphabet"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_graph_metadata_must_be_unique_bounded_and_canonical() -> Result<()> {
        let entry = [
            &[0x0a, CUDA_GRAPH_KEY.len() as u8],
            CUDA_GRAPH_KEY,
            b"\x12\x04AQI=",
        ]
        .concat();
        let field = [&[0x72, entry.len() as u8], entry.as_slice()].concat();
        let canonical = [b"\x08\x09", field.as_slice()].concat();
        assert_eq!(cuda_graph(&canonical)?, [1, 2]);
        assert!(cuda_graph(canonical.split_last().unwrap().1).is_err());
        assert!(cuda_graph(&[canonical, field].concat()).is_err());
        for invalid in [b"AB==", b"A@==", b"A===", b"AA=A", b"AAB="] {
            assert!(decode_graph(invalid).is_err());
        }
        Ok(())
    }
}
