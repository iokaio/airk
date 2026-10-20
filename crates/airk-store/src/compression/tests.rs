// SPDX-License-Identifier: Apache-2.0
use super::*;

#[test]
fn lossless_blocks_bound_scoped_decode_and_preserve_exact_bytes() -> Result<()> {
    let input = b"original bounded source\r\n".repeat(150_000);
    let (encoded, reference) =
        encode(&input, CompressionMode::Automatic)?.context("fixture did not compress")?;
    assert!(encoded.len() < input.len() / 2);
    let directory = tempfile::tempdir()?;
    let path = directory.path().join(&reference.stored_digest);
    std::fs::write(&path, &encoded)?;
    let hash = digest(&input);
    assert_eq!(
        read_all(&path, &reference, &hash, input.len() as u64)?,
        input
    );
    let range = BLOCK + 17..BLOCK + 117;
    assert_eq!(
        read_range(
            &path,
            &reference,
            &hash,
            input.len() as u64,
            range.clone(),
            BLOCK
        )?,
        input[range]
    );
    assert!(
        read_range(
            &path,
            &reference,
            &hash,
            input.len() as u64,
            BLOCK - 4..BLOCK + 4,
            BLOCK
        )
        .is_err()
    );
    assert!(read_all(&path, &reference, &hash, input.len() as u64 - 1).is_err());
    assert!(
        read_range(
            &path,
            &reference,
            &"f".repeat(64),
            input.len() as u64,
            0..1,
            BLOCK
        )
        .is_err()
    );
    assert!(encode(b"tiny", CompressionMode::Space)?.is_none());
    assert!(encode(&input, CompressionMode::Off)?.is_none());
    Ok(())
}

#[test]
fn corrupt_truncated_and_forged_frames_fail_before_evidence() -> Result<()> {
    let input = b"original corruption fixture\n".repeat(6000);
    let (encoded, reference) = encode(&input, CompressionMode::Automatic)?.unwrap();
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("frame");
    for offset in [0, 8, 13, encoded.len() - 1] {
        let mut damaged = encoded.clone();
        damaged[offset] ^= 1;
        std::fs::write(&path, damaged)?;
        assert!(
            read_all(&path, &reference, &digest(&input), input.len() as u64).is_err(),
            "offset {offset}"
        );
    }
    std::fs::write(&path, &encoded[..encoded.len() - 1])?;
    assert!(read_all(&path, &reference, &digest(&input), input.len() as u64).is_err());
    let mut forged = reference.clone();
    forged.version = 2;
    std::fs::write(&path, encoded)?;
    assert!(read_all(&path, &forged, &digest(&input), input.len() as u64).is_err());
    Ok(())
}
