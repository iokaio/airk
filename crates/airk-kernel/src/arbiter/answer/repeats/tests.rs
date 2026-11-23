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
    let carried = plan(&candidates(&hits, &[Some(0.4), Some(0.9), None]), true);
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
    let carried = plan(&candidates(&hits, &[None, None, None]), true);
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
    let carried = plan(&candidates(&hits, &[None, None, Some(0.1)]), true);
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
            plan(&candidates(&hits, &[None, None]), true),
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
    let carried = plan(&candidates(&hits, &[Some(0.2), Some(0.8)]), true);
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
    let carried = plan(&candidates(&hits, &[Some(0.9), Some(0.1)]), true);
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
            plan(&candidates(&hits, &[None, None]), true),
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
    let carried = plan(&candidates(&hits, &[Some(0.9), Some(0.3), Some(0.5)]), true);
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

/// The text a planned span delivers, rebuilt from the plan alone: a carrier
/// is its kept bytes with each trimmed part put back from the span it names
/// (head parts before, tail parts after); a repeat is its carrier's text.
fn rebuilt(hits: &[Hit], carried: &[Carried], index: usize) -> String {
    match &carried[index] {
        Carried::Text {
            start,
            end,
            overlaps,
            ..
        } => {
            let mut text = hits[index].text[*start..*end].to_owned();
            for (of, bytes, head) in overlaps {
                let other = rebuilt(hits, carried, *of);
                if *head {
                    text.insert_str(0, &other[other.len() - bytes..]);
                } else {
                    text.push_str(&other[..*bytes]);
                }
            }
            text
        }
        Carried::Same { of, .. } => rebuilt(hits, carried, *of),
    }
}

#[test]
fn a_span_trimmed_at_both_ends_by_two_carriers_rebuilds_whole() {
    // Present: every row's own location. Recoverable: b's lines 18-33 from
    // X's last three lines, b's kept lines 21-30 and Z's first three.
    let (x, z, b) = (lines(1, 20), lines(31, 50), lines(18, 33));
    let hits = [
        hit("src/guard.rs", 1, (1, 20), &x),
        hit("src/guard.rs", 1, (31, 50), &z),
        hit("src/guard.rs", 1, (18, 33), &b),
    ];
    let carried = plan(&candidates(&hits, &[None, None, None]), true);
    assert_eq!(carried[0], whole(&x));
    assert_eq!(carried[1], whole(&z));
    // Three 29-byte lines at each end: 87 bytes from X's tail, 87 from Z's head.
    assert_eq!(
        carried[2],
        Carried::Text {
            start: 87,
            end: 464 - 87,
            overlaps: vec![(0, 87, true), (1, 87, false)],
            also_at: vec![],
        }
    );
    assert_eq!(&b[87..377], lines(21, 30));
    for index in 0..3 {
        assert_eq!(rebuilt(&hits, &carried, index), hits[index].text);
    }
}

#[test]
fn a_trimmed_span_is_never_called_part_of_another() {
    // Review M1. X is lines 1-50, Y lines 48-70, b lines 45-60. b's head
    // (45-50) is X's; the rest (51-60) lies in what Y shows. b stays a
    // trimmed row naming X, never `within` Y: present are all three
    // locations; recoverable is b's whole text from X's tail and b's row.
    let (x, y, b) = (lines(1, 50), lines(48, 70), lines(45, 60));
    let hits = [
        hit("src/guard.rs", 1, (1, 50), &x),
        hit("src/guard.rs", 1, (48, 70), &y),
        hit("src/guard.rs", 1, (45, 60), &b),
    ];
    let carried = plan(&candidates(&hits, &[None, None, None]), true);
    assert_eq!(carried[0], whole(&x));
    assert_eq!(
        carried[1],
        Carried::Text {
            start: 87,
            end: y.len(),
            overlaps: vec![(0, 87, true)],
            also_at: vec![],
        }
    );
    assert_eq!(
        carried[2],
        Carried::Text {
            start: 174,
            end: b.len(),
            overlaps: vec![(0, 174, true)],
            also_at: vec![],
        }
    );
    for index in 0..3 {
        assert_eq!(rebuilt(&hits, &carried, index), hits[index].text);
    }
}

#[test]
fn a_carrier_with_other_places_keeps_them_when_it_lies_within_another() {
    // Review L2. The snippet ranks highest and carries its text for its
    // copy in vendor/; it also lies within the chunk. It stays a carrier, so
    // its `also_at` still lists vendor/guard.rs:12-14. Present: three rows,
    // and the vendor place in `also_at`. Recoverable: every text.
    let (chunk, snippet) = (lines(10, 40), lines(12, 14));
    let hits = [
        hit("src/guard.rs", 1, (12, 14), &snippet),
        hit("src/guard.rs", 1, (10, 40), &chunk),
        hit("vendor/guard.rs", 2, (12, 14), &snippet),
    ];
    let carried = plan(&candidates(&hits, &[Some(0.9), Some(0.3), Some(0.5)]), true);
    assert_eq!(
        carried,
        [
            Carried::Text {
                start: 0,
                end: snippet.len(),
                overlaps: vec![],
                also_at: vec!["vendor/guard.rs:12-14".into()],
            },
            whole(&chunk),
            Carried::Same {
                of: 0,
                within: false
            },
        ]
    );
}

#[test]
fn a_repeat_of_a_trimmed_carrier_rebuilds_the_carriers_whole_text() {
    // Present: three rows. Recoverable: the repeat's lines 10-40 are the
    // carrier's lines 10-29 plus the 319 bytes (lines 30-40) the carrier
    // names in the second span.
    let (first, second) = (lines(10, 40), lines(30, 60));
    let hits = [
        hit("src/guard.rs", 1, (10, 40), &first),
        hit("src/guard.rs", 1, (30, 60), &second),
        hit("src/guard.rs", 1, (10, 40), &first),
    ];
    let carried = plan(&candidates(&hits, &[Some(0.2), Some(0.8), None]), true);
    assert_eq!(
        carried[0],
        Carried::Text {
            start: 0,
            end: first.len() - 319,
            overlaps: vec![(1, 319, false)],
            also_at: vec![],
        }
    );
    assert_eq!(
        carried[2],
        Carried::Same {
            of: 0,
            within: false
        }
    );
    assert_eq!(rebuilt(&hits, &carried, 2), lines(10, 40));
}

#[test]
fn a_multi_byte_tail_overlap_is_cut_on_a_character_boundary() {
    // a is twenty two-byte characters then twenty bytes; b is ten bytes then
    // the same twenty characters: b's last 40 bytes are a's first. Present:
    // both rows. Recoverable: b from its first ten bytes and a's first 40.
    let accents = "é".repeat(20);
    let a = format!("{accents}{}", "Z".repeat(20));
    let b = format!("{}{accents}", "Q".repeat(10));
    let hits = [
        hit("src/text.rs", 1, (1, 2), &a),
        hit("src/text.rs", 1, (2, 3), &b),
    ];
    let carried = plan(&candidates(&hits, &[None, None]), true);
    assert_eq!(carried[0], whole(&a));
    assert_eq!(
        carried[1],
        Carried::Text {
            start: 0,
            end: 10,
            overlaps: vec![(0, 40, false)],
            also_at: vec![],
        }
    );
    assert_eq!(rebuilt(&hits, &carried, 1), b);
}

#[test]
fn two_indexes_with_the_same_path_and_hash_each_keep_their_text() {
    // An Answer reply holds one index, and Union plans each reply alone, so
    // two indexes holding the same file each carry the text in their own
    // reply. Present: one row in each reply. Recoverable: the text in both.
    let text = lines(1, 9);
    for index in [hit("src/guard.rs", 1, (1, 9), &text), hit("src/guard.rs", 1, (1, 9), &text)] {
        let hits = [index];
        assert_eq!(plan(&candidates(&hits, &[None]), true), [whole(&text)]);
    }
    // Were the two ever planned together, the second is a repeat of the
    // first at the very same place: its own row keeps that location.
    let hits = [
        hit("src/guard.rs", 1, (1, 9), &text),
        hit("src/guard.rs", 1, (1, 9), &text),
    ];
    assert_eq!(
        plan(&candidates(&hits, &[None, None]), true),
        [
            whole(&text),
            Carried::Same {
                of: 0,
                within: false
            }
        ]
    );
}

#[test]
fn without_overlaps_only_exact_repeats_merge() {
    // Where the reply cuts texts (trimming level 2 on), a span is never
    // trimmed against or named inside another: each keeps its own text.
    // Present: four rows. Recoverable: each text from its own row, and the
    // exact repeat from its carrier.
    let (chunk, snippet, tail) = (lines(10, 40), lines(12, 14), lines(30, 60));
    let hits = [
        hit("src/guard.rs", 1, (10, 40), &chunk),
        hit("src/guard.rs", 1, (12, 14), &snippet),
        hit("src/guard.rs", 1, (30, 60), &tail),
        hit("src/guard.rs", 1, (10, 40), &chunk),
    ];
    assert_eq!(
        plan(&candidates(&hits, &[None, None, None, None]), false),
        [
            whole(&chunk),
            whole(&snippet),
            whole(&tail),
            Carried::Same {
                of: 0,
                within: false
            }
        ]
    );
}