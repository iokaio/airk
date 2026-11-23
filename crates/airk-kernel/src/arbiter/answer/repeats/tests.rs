// SPDX-License-Identifier: Apache-2.0
//! Repeats and overlaps, with hand-written expectations.
use super::*;

/// A hit of `path` at `lines` in a file whose content hash is `file`.
fn hit(path: &str, file: u8, lines: (usize, usize), text: &str) -> Hit {
    serde_json::from_value(json!({
        "citation":{"path":path,"content_hash":Sha256Digest::from_bytes([file; 32]).to_string(),
            "start_line":lines.0,"end_line":lines.1,
            "location":airk_core::DocumentLocation::Lines {start:lines.0,end:lines.1}},
        "text":text,"truncated":false}))
    .expect("a hit")
}

/// The 29-byte lines `first..=last` of one file.
fn lines(first: usize, last: usize) -> String {
    (first..=last)
        .map(|number| format!("line {number:03} of the guard module\n"))
        .collect()
}

fn whole(text: &str) -> Carried {
    Carried::Text {
        start: 0,
        end: text.len(),
        overlaps: vec![],
        also_at: vec![],
    }
}

fn candidates<'a>(hits: &'a [Hit], focus: &[Option<f32>]) -> Vec<Candidate<'a>> {
    hits.iter()
        .zip(focus)
        .map(|(hit, focus)| Candidate { hit, focus: *focus })
        .collect()
}

#[test]
fn overlap_is_exact_and_needs_a_real_run_of_bytes() {
    let (a, b) = (lines(10, 20), lines(15, 25));
    // Lines 15 to 20 are six lines of 29 bytes: the tail of `a`, the head of `b`.
    assert_eq!(overlap(&a, &b), Overlap::Head(174));
    assert_eq!(overlap(&b, &a), Overlap::Tail(174));
    assert_eq!(overlap(&a, &lines(12, 14)), Overlap::Within);
    assert_eq!(overlap(&a, &a), Overlap::Within);
    assert_eq!(overlap(&lines(12, 14), &a), Overlap::Holds(58));
    assert_eq!(overlap(&a, &lines(21, 30)), Overlap::None);
    assert_eq!(overlap(&a, ""), Overlap::None);
    // One changed byte in the shared lines: no overlap at all.
    let changed = b.replacen("line 016", "line 0l6", 1);
    assert_eq!(overlap(&a, &changed), Overlap::None);
    // 31 shared bytes are not enough; 32 are.
    let (short, long) = ("x".repeat(31), "x".repeat(32));
    assert_eq!(
        overlap(&format!("left {short}"), &format!("{short} right")),
        Overlap::None
    );
    assert_eq!(
        overlap(&format!("left {long}"), &format!("{long} right")),
        Overlap::Head(32)
    );
    // Sixteen two-byte characters: the overlap is counted in whole characters.
    let accents = "é".repeat(16);
    assert_eq!(
        overlap(&format!("a{accents}"), &format!("{accents}b")),
        Overlap::Head(32)
    );
}

#[test]
fn an_exact_repeat_is_carried_by_its_highest_ranked_instance() {
    let text = lines(1, 9);
    // The same span from two reads, and a copy of the file elsewhere.
    let hits = [
        hit("src/guard.rs", 1, (1, 9), &text),
        hit("src/guard.rs", 1, (1, 9), &text),
        hit("vendor/guard.rs", 2, (1, 9), &text),
    ];
    // The second read's instance has the highest Focus relevance.
    let carried = plan(&candidates(&hits, &[Some(0.4), Some(0.9), None]));
    assert_eq!(
        carried,
        [
            Carried::Same {
                of: 1,
                within: false
            },
            Carried::Text {
                start: 0,
                end: text.len(),
                overlaps: vec![],
                // The other place is listed; the same place is not.
                also_at: vec!["vendor/guard.rs:1-9".into()],
            },
            Carried::Same {
                of: 1,
                within: false
            },
        ]
    );
    // Without scores the earliest instance carries the text.
    let carried = plan(&candidates(&hits, &[None, None, None]));
    assert_eq!(
        carried[0],
        Carried::Text {
            start: 0,
            end: text.len(),
            overlaps: vec![],
            also_at: vec!["vendor/guard.rs:1-9".into()],
        }
    );
    assert_eq!(
        carried[1..],
        [
            Carried::Same {
                of: 0,
                within: false
            },
            Carried::Same {
                of: 0,
                within: false
            }
        ]
    );
    // A scored instance ranks above an unscored one.
    let carried = plan(&candidates(&hits, &[None, None, Some(0.1)]));
    assert!(
        matches!(&carried[2], Carried::Text { also_at, .. } if also_at == &["src/guard.rs:1-9"])
    );
}

#[test]
fn two_different_texts_are_never_merged() {
    let text = lines(1, 9);
    for other in [
        // One byte differs.
        text.replacen("line 005", "line 0O5", 1),
        // Only the indentation differs.
        text.replace("line ", " line "),
        // Only the line ending differs.
        text.replace('\n', "\r\n"),
        // One more line, and a changed byte in the middle.
        lines(1, 10).replacen("line 005", "line 0O5", 1),
    ] {
        let hits = [
            hit("src/guard.rs", 1, (1, 9), &text),
            hit("src/guard.rs", 1, (1, 9), &other),
        ];
        assert_eq!(
            plan(&candidates(&hits, &[None, None])),
            [whole(&text), whole(&other)],
            "{other:?}"
        );
    }
}

#[test]
fn overlapping_spans_of_one_file_share_their_text_once() {
    let (first, second) = (lines(10, 40), lines(30, 60));
    let hits = [
        hit("src/guard.rs", 1, (10, 40), &first),
        hit("src/guard.rs", 1, (30, 60), &second),
    ];
    // Lines 30 to 40 are eleven lines of 29 bytes. The spans are equally
    // long, so the higher-ranked one keeps the overlap.
    let carried = plan(&candidates(&hits, &[Some(0.2), Some(0.8)]));
    assert_eq!(
        carried,
        [
            Carried::Text {
                start: 0,
                end: first.len() - 319,
                overlaps: vec![(1, 319, false)],
                also_at: vec![],
            },
            whole(&second),
        ]
    );
    // Nothing is lost: the two carried texts are lines 10 to 60.
    assert_eq!(
        format!("{}{}", &first[..first.len() - 319], second),
        lines(10, 60)
    );
    // A longer span keeps the overlap whatever the rank.
    let longer = lines(30, 70);
    let hits = [
        hit("src/guard.rs", 1, (10, 40), &first),
        hit("src/guard.rs", 1, (30, 70), &longer),
    ];
    let carried = plan(&candidates(&hits, &[Some(0.9), Some(0.1)]));
    assert_eq!(carried[1], whole(&longer));
    assert!(matches!(
        &carried[0],
        Carried::Text { start: 0, overlaps, .. } if overlaps == &[(1, 319, false)]
    ));
    // The same lines in another file, or lines that do not meet, are not
    // an overlap: nothing is trimmed.
    for other in [
        hit("src/other.rs", 2, (30, 60), &second),
        hit("src/guard.rs", 1, (130, 160), &second),
        hit("src/guard.rs", 3, (30, 60), &second),
    ] {
        let hits = [hit("src/guard.rs", 1, (10, 40), &first), other];
        assert_eq!(
            plan(&candidates(&hits, &[None, None])),
            [whole(&first), whole(&second)]
        );
    }
}

#[test]
fn a_span_inside_a_longer_one_names_it_and_so_do_its_repeats() {
    let (chunk, snippet) = (lines(10, 40), lines(12, 14));
    let hits = [
        // The snippet ranks highest; its repeat came from another read.
        hit("src/guard.rs", 1, (12, 14), &snippet),
        hit("src/guard.rs", 1, (10, 40), &chunk),
        hit("src/guard.rs", 1, (12, 14), &snippet),
    ];
    let carried = plan(&candidates(&hits, &[Some(0.9), Some(0.3), Some(0.5)]));
    assert_eq!(
        carried,
        [
            Carried::Same {
                of: 1,
                within: true
            },
            whole(&chunk),
            Carried::Same {
                of: 1,
                within: true
            },
        ]
    );
}
