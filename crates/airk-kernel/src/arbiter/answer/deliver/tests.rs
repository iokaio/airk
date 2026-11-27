// SPDX-License-Identifier: Apache-2.0
//! Rules 5 and 6 on hand-built candidates; expectations worked by hand.
use super::*;

fn span(record: usize, path: &str, start: usize, end: usize, text: &str) -> Span {
    Span {
        record,
        pointer: "/hits/0".into(),
        hit: serde_json::from_value(super::super::tests::hit(path, start, end, text)).unwrap(),
        round: 1,
        focus: None,
        from: 0,
    }
}

fn candidate(
    record: usize,
    path: &str,
    lines: (usize, usize),
    text: &str,
    score: f32,
) -> Candidate {
    Candidate {
        span: span(record, path, lines.0, lines.1, text),
        score,
    }
}

fn set(words: &[&str]) -> BTreeSet<String> {
    words.iter().map(|word| (*word).to_owned()).collect()
}

#[test]
fn words_split_case_changes_and_drop_short_pieces() {
    assert_eq!(
        words("restartGuard HTTPServer snake_case x12 v2_run"),
        set(&["case", "guard", "http", "restart", "run", "server", "snake"])
    );
}

#[test]
fn a_rarer_shared_word_counts_more() {
    // "alpha" is in one bag of three, "beta" in all three.
    let bags = [
        set(&["alpha", "beta"]),
        set(&["beta"]),
        set(&["beta", "gamma"]),
    ];
    let rarity = Rarity::new(&bags);
    let wanted = set(&["alpha", "beta"]);
    let alpha = 1.0 / 2f32.sqrt();
    let beta = 1.0 / 4f32.sqrt();
    let total = alpha + beta;
    assert!((rarity.share(&wanted, &bags[0]) - 1.0).abs() < 1e-6);
    assert!((rarity.share(&wanted, &bags[1]) - beta / total).abs() < 1e-6);
    assert_eq!(rarity.share(&set(&[]), &bags[0]), 0.0);
}

#[test]
fn delivery_keeps_half_the_best_and_never_leaves_a_need_empty() {
    let first = vec![
        candidate(0, "a.rs", (1, 10), &"a".repeat(100), 1.0),
        candidate(1, "b.rs", (1, 10), &"b".repeat(100), 0.6),
        // Below half the best: not kept.
        candidate(2, "c.rs", (1, 10), &"c".repeat(100), 0.4),
    ];
    // Every candidate of the second need scores zero: its best still goes.
    let second = vec![candidate(3, "d.rs", (1, 10), &"d".repeat(100), 0.0)];
    let chosen = deliver(&[first, second], 10_000);
    let paths = |need: usize| -> Vec<&str> {
        chosen[need]
            .iter()
            .map(|span| span.hit.citation.path.as_str())
            .collect()
    };
    assert_eq!(paths(0), ["a.rs", "b.rs"]);
    assert_eq!(paths(1), ["d.rs"]);
}

#[test]
fn delivery_shares_the_budget_and_cites_what_another_need_already_carries() {
    // Budget 1,000: each need first gets 1000/8 = 125, plus 750 x kept/total.
    // Need 1 keeps three 300-byte passages, need 2 one: allowances 687 and 312.
    let need_one = vec![
        candidate(0, "a.rs", (1, 10), &"a".repeat(300), 1.0),
        candidate(1, "b.rs", (1, 10), &"b".repeat(300), 0.9),
        candidate(2, "c.rs", (1, 10), &"c".repeat(300), 0.8),
    ];
    // Need 2's passage is lines 2 to 9 of a.rs: held whole by need 1's.
    let need_two = vec![candidate(4, "a.rs", (2, 9), &"x".repeat(300), 1.0)];
    let chosen = deliver(&[need_one, need_two], 1_000);
    // Need 1: a.rs and b.rs fit its 687 bytes; c.rs (900) does not, and in
    // the second sweep 900 still fits the whole budget of 1,000.
    let first: Vec<&str> = chosen[0]
        .iter()
        .map(|span| span.hit.citation.path.as_str())
        .collect();
    assert_eq!(first, ["a.rs", "b.rs", "c.rs"]);
    // Need 2 cites need 1's a.rs instead of a second copy.
    assert_eq!(chosen[1].len(), 1);
    assert_eq!(chosen[1][0].record, 0);
}

#[test]
fn delivery_stops_at_the_budget_but_a_need_still_gets_its_best() {
    let need_one = vec![
        candidate(0, "a.rs", (1, 10), &"a".repeat(600), 1.0),
        candidate(1, "b.rs", (1, 10), &"b".repeat(600), 1.0),
    ];
    let need_two = vec![candidate(2, "c.rs", (1, 10), &"c".repeat(600), 1.0)];
    let chosen = deliver(&[need_one, need_two], 1_000);
    // a.rs (600) goes first; c.rs would pass 1,000 but is need 2's best.
    assert_eq!(chosen[0].len(), 1);
    assert_eq!(chosen[0][0].hit.citation.path, "a.rs");
    assert_eq!(chosen[1].len(), 1);
    assert_eq!(chosen[1][0].hit.citation.path, "c.rs");
}
