// SPDX-License-Identifier: Apache-2.0
use super::*;
use std::io::Cursor;
use std::time::Duration;

fn image(tags: &[(u64, u64)]) -> Vec<u8> {
    let mut data = vec![0; 1024];
    data[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    for (i, x) in [(16, 3_u16), (18, 62), (52, 64), (54, 56), (56, 2)] {
        data[i..i + 2].copy_from_slice(&x.to_le_bytes());
    }
    data[20..24].copy_from_slice(&1_u32.to_le_bytes());
    data[32..40].copy_from_slice(&64_u64.to_le_bytes());
    for (i, kind, at, size) in [
        (64, PT_LOAD, 0_u64, 1024_u64),
        (120, PT_DYNAMIC, 256, 16 * (tags.len() as u64 + 3)),
    ] {
        data[i..i + 4].copy_from_slice(&kind.to_le_bytes());
        for (field, value) in [(8, at), (16, 0x2000 + at), (32, size), (40, size)] {
            data[i + field..i + field + 8].copy_from_slice(&value.to_le_bytes());
        }
    }
    let strings = b"\0libx.so\0liby.so\0$ORIGIN\0/tmp\0";
    data[768..768 + strings.len()].copy_from_slice(strings);
    for (i, (tag, value)) in [(5, 0x2300), (10, strings.len() as u64)]
        .iter()
        .chain(tags)
        .enumerate()
    {
        let off = 256 + 16 * i;
        data[off..off + 8].copy_from_slice(&tag.to_le_bytes());
        data[off + 8..off + 16].copy_from_slice(&value.to_le_bytes());
    }
    data
}

fn parse(raw: Vec<u8>) -> io::Result<DynamicImage> {
    let len = raw.len() as u64;
    inspect(
        &mut Cursor::new(raw),
        len,
        Instant::now() + Duration::from_secs(2),
    )
}

#[test]
fn maps_virtual_strings_and_preserves_dependency_order() {
    let result = parse(image(&[(1, 9), (1, 1), (14, 1), (29, 17)])).unwrap();
    assert_eq!(result.soname.as_deref(), Some("libx.so"));
    assert_eq!(result.needed, ["liby.so", "libx.so"]);
    assert!(result.origin_search);
}

#[test]
fn refuses_ambient_paths_and_extra_dependency_sources() {
    for tags in [
        vec![(15, 25)],
        vec![(29, 0)],
        vec![(0x6fff_fefc, 1)],
        vec![(0x7fff_ffff, 1)],
        vec![(1, 25)],
        vec![(14, 1), (14, 9)],
    ] {
        assert!(parse(image(&tags)).is_err());
    }
}

#[test]
fn refuses_unmapped_strings_and_changed_dynamic_file_offset() {
    let mut unmapped = image(&[]);
    unmapped[264..272].copy_from_slice(&0x3000_u64.to_le_bytes());
    assert!(parse(unmapped).is_err());
    let mut mismapped = image(&[]);
    mismapped[128..136].copy_from_slice(&264_u64.to_le_bytes());
    assert!(parse(mismapped).is_err());
}

#[test]
fn refuses_expired_deadline_and_wrong_platform() {
    let mut source = image(&[]);
    assert!(
        inspect(
            &mut Cursor::new(&source),
            source.len() as u64,
            Instant::now()
        )
        .is_err()
    );
    source[18..20].copy_from_slice(&183_u16.to_le_bytes());
    assert!(parse(source).is_err());
}
