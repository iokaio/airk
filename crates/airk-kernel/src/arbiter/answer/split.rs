// SPDX-License-Identifier: Apache-2.0
//! Split (step 0): code proposes where a compound request breaks; a model
//! may choose among whole partitions. With no model, only explicit
//! enumerations and separate questions split. A simple request stays one
//! part, so Split adds nothing to it.
use super::*;
use airk_core::native_loop::LoopCard;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum BreakKind {
    /// A line-start `1.`, `2)`, `-`, `*`, `(a)`, or `first|second|third|finally,`.
    Enum,
    /// A `?` followed by at least three more words.
    Question,
    /// `. `, `! ` or a newline before an uppercase letter, three words each side.
    Sentence,
    /// `, and also`, `; also`, ` and then `, ` as well as `.
    Join,
    /// ` and ` between two clauses that each start with an imperative.
    Verbs,
}

/// A proposed start of a new part, as a UTF-8 offset into the request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Break {
    pub(crate) at: usize,
    pub(crate) kind: BreakKind,
}

/// An ordered partition of the whole request: contiguous ranges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Partition(pub(crate) Vec<(usize, usize)>);

/// Split's roster: card 0 keeps the request whole.
#[derive(Clone, Debug)]
pub(crate) struct SplitPlan {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) breaks: Vec<Break>,
    pub(crate) cards: Vec<Partition>,
    /// The deterministic choice: explicit enumerations and questions.
    pub(crate) deterministic: usize,
}

const IMPERATIVES: [&str; 15] = [
    "explain", "update", "add", "fix", "find", "write", "describe", "document", "refactor", "test",
    "show", "list", "change", "remove", "rename",
];
const JOINS: [&str; 4] = [", and also ", "; also ", " and then ", " as well as "];
const ABBREVIATIONS: [&str; 10] = [
    "e.g", "i.e", "etc", "vs", "mr", "mrs", "dr", "fig", "no", "cf",
];
const ENUM_WORDS: [&str; 4] = ["first,", "second,", "third,", "finally,"];

fn words(text: &str) -> usize {
    text.split_whitespace()
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .count()
}

fn skip_space(text: &str, mut at: usize) -> usize {
    while let Some(c) = text[at..].chars().next() {
        if !c.is_whitespace() {
            break;
        }
        at += c.len_utf8();
    }
    at
}

fn enumerator(line: &str) -> bool {
    let line = line.trim_start();
    let lower = line.to_ascii_lowercase();
    if ENUM_WORDS.iter().any(|word| lower.starts_with(word)) {
        return true;
    }
    let bytes = line.as_bytes();
    if matches!(bytes.first(), Some(b'-' | b'*')) && bytes.get(1) == Some(&b' ') {
        return true;
    }
    if let Some(rest) = line.strip_prefix('(') {
        let label: String = rest
            .chars()
            .take_while(char::is_ascii_alphanumeric)
            .collect();
        return (1..=3).contains(&label.len()) && rest[label.len()..].starts_with(')');
    }
    let digits = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    (1..=2).contains(&digits)
        && matches!(bytes.get(digits), Some(b'.' | b')'))
        && bytes.get(digits + 1) == Some(&b' ')
}

/// Every proposed break, in offset order, one per offset.
pub(crate) fn breaks(question: &str) -> Vec<Break> {
    let mut found = Vec::new();
    // Enumerations at line starts.
    let mut offset = 0;
    for line in question.split_inclusive('\n') {
        let indent = line.len() - line.trim_start().len();
        if enumerator(line) && offset + indent > 0 {
            found.push(Break {
                at: offset + indent,
                kind: BreakKind::Enum,
            });
        }
        offset += line.len();
    }
    // Separate questions: after a `?`, and where a sentence ending in `?` starts.
    for (at, _) in question.match_indices('?') {
        let next = skip_space(question, at + 1);
        if next < question.len() && words(&question[next..]) >= 3 {
            found.push(Break {
                at: next,
                kind: BreakKind::Question,
            });
        }
        let start = question[..at]
            .rfind(['.', '?', '!', '\n'])
            .map_or(0, |end| skip_space(question, end + 1));
        if start > 0 && start < at && words(&question[..start]) >= 3 {
            found.push(Break {
                at: start,
                kind: BreakKind::Question,
            });
        }
    }
    // Sentences.
    for (at, mark) in question.char_indices() {
        if !matches!(mark, '.' | '!' | '\n') {
            continue;
        }
        let next = skip_space(question, at + mark.len_utf8());
        let Some(first) = question[next..].chars().next() else {
            continue;
        };
        let spaced = mark == '\n' || next > at + 1;
        let before = &question[..at];
        let last = before
            .rsplit(|c: char| c.is_whitespace())
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let abbreviation = mark == '.'
            && (ABBREVIATIONS.contains(&last.as_str())
                || last.len() == 1
                || last.chars().all(|c| c.is_ascii_digit()));
        if spaced
            && first.is_uppercase()
            && !abbreviation
            && words(before) >= 3
            && words(&question[next..]) >= 3
        {
            found.push(Break {
                at: next,
                kind: BreakKind::Sentence,
            });
        }
    }
    // Joins.
    let lower = question.to_ascii_lowercase();
    for join in JOINS {
        for (at, _) in lower.match_indices(join) {
            let next = at + join.len();
            if words(&question[..at]) >= 3 && words(&question[next..]) >= 2 {
                found.push(Break {
                    at: at + usize::from(join.starts_with([',', ';'])),
                    kind: BreakKind::Join,
                });
            }
        }
    }
    // Two imperative clauses joined by ` and `.
    for (at, _) in lower.match_indices(" and ") {
        let after = lower[at + 5..].split_whitespace().next().unwrap_or("");
        let clause_start = lower[..at]
            .rfind(['.', '?', '!', '\n', ';', ','])
            .map_or(0, |end| end + 1);
        let leading = lower[clause_start..at]
            .split_whitespace()
            .next()
            .unwrap_or("");
        if IMPERATIVES.contains(&after) && IMPERATIVES.contains(&leading) {
            found.push(Break {
                at: at + 1,
                kind: BreakKind::Verbs,
            });
        }
    }
    found.sort();
    found.dedup_by_key(|item| item.at);
    found.retain(|item| {
        item.at > 0 && item.at < question.len() && question.is_char_boundary(item.at)
    });
    found
}

/// The leading join words a part does not repeat.
const LEADING: [&str; 5] = ["and also ", "also ", "and then ", "as well as ", "and "];

/// A part's text: the trimmed slice of its range without a leading join.
pub(crate) fn part_span(question: &str, (start, end): (usize, usize)) -> (usize, usize) {
    let slice = &question[start..end];
    let mut from = start + (slice.len() - slice.trim_start().len());
    let rest = question[from..end].to_ascii_lowercase();
    if let Some(join) = LEADING.iter().find(|join| rest.starts_with(**join)) {
        from = skip_space(question, from + join.len()).min(end);
    }
    let to = from + question[from..end].trim_end().len();
    (from, to.max(from))
}

fn partition(question: &str, cuts: &[usize], max_parts: usize) -> Partition {
    let mut ranges = Vec::new();
    let mut start = 0;
    for &cut in cuts {
        if cut > start {
            ranges.push((start, cut));
            start = cut;
        }
    }
    ranges.push((start, question.len()));
    // A range too short to be a request of its own joins its neighbour; an
    // introduction ending in `:` joins the part it introduces.
    loop {
        if ranges.len() == 1 {
            break;
        }
        let last = ranges.len() - 1;
        let Some((short, forward)) = ranges.iter().enumerate().find_map(|(index, range)| {
            let (from, to) = part_span(question, *range);
            let text = &question[from..to];
            if words(text) < 3 {
                Some((index, index == 0))
            } else if text.ends_with(':') && index < last {
                Some((index, true))
            } else {
                None
            }
        }) else {
            break;
        };
        let with = if forward { short + 1 } else { short - 1 };
        let (low, high) = (short.min(with), short.max(with));
        ranges[low].1 = ranges[high].1;
        ranges.remove(high);
    }
    // More parts than allowed: merge the shortest adjacent pair.
    while ranges.len() > max_parts.max(1) {
        let pair = (0..ranges.len() - 1)
            .min_by_key(|&index| ranges[index + 1].1 - ranges[index].0)
            .expect("two ranges");
        ranges[pair].1 = ranges[pair + 1].1;
        ranges.remove(pair + 1);
    }
    Partition(ranges)
}

/// Split's proposals: keep whole, then explicit breaks, then wider classes,
/// then single-class variants, de-duplicated, at most eight cards.
pub(crate) fn propose(question: &str, max_parts: usize) -> SplitPlan {
    let found = breaks(question);
    let cuts = |kinds: &[BreakKind]| -> Vec<usize> {
        found
            .iter()
            .filter(|item| kinds.contains(&item.kind))
            .map(|item| item.at)
            .collect()
    };
    use BreakKind::*;
    let sets: [&[BreakKind]; 9] = [
        &[],
        &[Enum, Question],
        &[Enum, Question, Sentence],
        &[Enum, Question, Sentence, Join, Verbs],
        &[Join],
        &[Verbs],
        &[Sentence],
        &[Enum],
        &[Question],
    ];
    let mut cards: Vec<Partition> = Vec::new();
    for set in sets {
        let candidate = partition(question, &cuts(set), max_parts);
        if !cards.contains(&candidate) && cards.len() < v3::MAX_ACTION_CARDS {
            cards.push(candidate);
        }
    }
    let explicit = partition(question, &cuts(&[Enum, Question]), max_parts);
    let deterministic = cards.iter().position(|card| *card == explicit).unwrap_or(0);
    SplitPlan {
        breaks: found,
        cards,
        deterministic,
    }
}

fn excerpt(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

/// The card a partition is shown as.
pub(crate) fn card(question: &str, partition: &Partition, symbol: &str) -> LoopCard {
    let spans: Vec<v3::QuestionSpan> = partition
        .0
        .iter()
        .map(|&(utf8_start, utf8_end)| v3::QuestionSpan {
            utf8_start,
            utf8_end,
        })
        .collect();
    if spans.len() == 1 {
        return LoopCard {
            symbol: symbol.into(),
            class: v3::CardClass::KeepWhole,
            text: "keep whole".into(),
            partition: spans,
        };
    }
    let mut text = String::from("parts:");
    for (number, range) in partition.0.iter().enumerate() {
        let (from, to) = part_span(question, *range);
        text.push_str(&format!(
            " [{}] {}-{} «{}»",
            number + 1,
            range.0,
            range.1,
            excerpt(&question[from..to], 180)
        ));
    }
    LoopCard {
        symbol: symbol.into(),
        class: v3::CardClass::SplitAt,
        text,
        partition: spans,
    }
}

#[cfg(test)]
mod tests;
