// SPDX-License-Identifier: Apache-2.0
//! Independent source-map/budget contract probes, without grammar dependencies.
use airk_core::location::{
    ByteSpan, HostBudget, MAX_MAP_SEGMENTS, MAX_REGIONS, MapSegment, Mapping, SOURCE_MAP_VERSION,
    SourceMap,
};

fn span(start: usize, end: usize) -> ByteSpan {
    ByteSpan { start, end }
}

#[test]
fn every_utf8_selector_preserves_verbatim_crlf_and_multibyte_bytes() {
    let local = "é\r\n雪😀";
    let host = format!("<script>{local}</script>");
    let map = SourceMap {
        version: SOURCE_MAP_VERSION,
        segments: vec![MapSegment {
            local: span(0, local.len()),
            mapping: Mapping::Verbatim {
                host: span(8, 8 + local.len()),
            },
        }],
    };
    for start in 0..=local.len() {
        for end in start..=local.len() {
            let mapped = map.original_spans(span(start, end), local, &host);
            if !local.is_char_boundary(start) || !local.is_char_boundary(end) {
                assert!(mapped.is_err());
            } else if start == end {
                assert_eq!(mapped.unwrap(), None);
            } else {
                let pieces = mapped.unwrap().unwrap();
                assert_eq!(pieces, vec![span(start + 8, end + 8)]);
                assert_eq!(&host[pieces[0].start..pieces[0].end], &local[start..end]);
            }
        }
    }
}

#[test]
fn synthetic_separator_does_not_claim_array_punctuation_as_source() {
    let map = SourceMap {
        version: SOURCE_MAP_VERSION,
        segments: vec![
            MapSegment {
                local: span(0, 1),
                mapping: Mapping::Verbatim { host: span(2, 3) },
            },
            MapSegment {
                local: span(1, 2),
                mapping: Mapping::Unmapped,
            },
            MapSegment {
                local: span(2, 3),
                mapping: Mapping::Verbatim { host: span(6, 7) },
            },
        ],
    };
    let host = "[\"a\",\"b\"]";
    assert_eq!(map.original_spans(span(0, 3), "a\nb", host).unwrap(), None);
    assert_eq!(
        map.original_spans(span(2, 3), "a\nb", host).unwrap(),
        Some(vec![span(6, 7)])
    );
}

#[test]
fn corrupt_serialized_offsets_versions_and_backtracking_fail_closed() {
    let base = SourceMap {
        version: SOURCE_MAP_VERSION,
        segments: vec![MapSegment {
            local: span(0, 1),
            mapping: Mapping::Verbatim { host: span(0, 1) },
        }],
    };
    let encoded = serde_json::to_vec(&base).unwrap();
    let mut decoded: SourceMap = serde_json::from_slice(&encoded).unwrap();
    decoded.segments[0].local.end = usize::MAX;
    assert!(decoded.original_spans(span(0, 1), "a", "a").is_err());
    decoded = base.clone();
    decoded.segments[0].mapping = Mapping::Decoded {
        host: span(usize::MAX, 0),
    };
    assert!(decoded.validate("a", "a").is_err());
    decoded = base.clone();
    decoded.version = SOURCE_MAP_VERSION + 1;
    assert!(decoded.validate("a", "a").is_err());
    decoded = base;
    decoded.segments.push(MapSegment {
        local: span(1, 2),
        mapping: Mapping::Decoded { host: span(0, 1) },
    });
    assert!(decoded.validate("ab", "a").is_err());
}

#[test]
fn segment_count_and_all_failed_work_dimensions_are_bounded() {
    let map = SourceMap {
        version: SOURCE_MAP_VERSION,
        segments: vec![
            MapSegment {
                local: span(0, 1),
                mapping: Mapping::Unmapped
            };
            MAX_MAP_SEGMENTS + 1
        ],
    };
    assert!(map.validate("a", "").is_err());
    let mut budget = HostBudget::new(1, 1, 1).unwrap();
    assert!(
        budget
            .charge(usize::MAX, usize::MAX, usize::MAX, usize::MAX)
            .is_err()
    );
    for costs in [(1, 0, 0, 0), (0, 1, 0, 0), (0, 0, 1, 0), (0, 0, 0, 1)] {
        assert!(budget.charge(costs.0, costs.1, costs.2, costs.3).is_err());
    }
    for _ in 0..MAX_REGIONS {
        assert!(budget.enter_region(0).is_err());
    }
    assert!(budget.enter_region(1).is_err());
    assert!(HostBudget::new(500_001, 0, 0).is_err());
    assert!(HostBudget::new(0, 10_001, 0).is_err());
    assert!(HostBudget::new(0, 0, 16 * 1024 * 1024).is_ok());
    assert!(HostBudget::new(0, 0, 16 * 1024 * 1024 + 1).is_err());
}
