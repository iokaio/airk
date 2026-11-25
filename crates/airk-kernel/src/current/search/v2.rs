// SPDX-License-Identifier: Apache-2.0
//! Ranked terms for prose requests and the historical private Search v2 profile.
//!
//! Product hybrid search uses one ranked lexical stream with the ordinary
//! branch depth, equal rank fusion and original snippet query. Test-support
//! images can select individual branches or reproduce an older profile.
use airk_core::lexical::terms;
use airk_store::current::lexical::{MAX_RANKED_GROUPS, TermGroup, valid_alternative};
use std::collections::{BTreeMap, BTreeSet};

/// The process variable that turns the switch on in a test-support build.
#[cfg(feature = "test-support")]
pub const VARIABLE: &str = "AIRK_ARBITER_EVALUATION_SEARCH";
/// A query of at least this many words is treated as a prose instruction.
pub const PROSE_WORDS: usize = 8;
/// Candidates taken from each branch before fusion.
pub const BRANCH_CANDIDATES: usize = 300;
/// Hits of one file ranked before every other file's hits are placed.
pub const PER_FILE: usize = 3;
/// Fusion weights by branch.
pub const VECTOR_WEIGHT: f64 = 1.0;
pub const KEY_WEIGHT: f64 = 1.0;
pub const NAMED_WEIGHT: f64 = 1.0;
/// Key words kept for the lexical branches, named identifiers first.
const MAX_KEY_WORDS: usize = 20;

/// Selects the ranked lexical branches and preserves the historical replay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchProfile {
    #[cfg(any(test, feature = "test-support"))]
    Baseline,
    #[cfg(any(test, feature = "test-support"))]
    Keywords,
    SemanticSubqueries,
    Fusion {
        weighted: bool,
        diverse: bool,
    },
    #[cfg(any(test, feature = "test-support"))]
    Identifiers,
    #[cfg(any(test, feature = "test-support"))]
    RankedTerms,
    LegacyV2,
}

impl SearchProfile {
    pub(crate) fn lexical_branches(self) -> (bool, bool) {
        match self {
            #[cfg(any(test, feature = "test-support"))]
            Self::Baseline => (false, false),
            #[cfg(any(test, feature = "test-support"))]
            Self::Keywords => (true, false),
            Self::SemanticSubqueries | Self::Fusion { .. } => (true, false),
            #[cfg(any(test, feature = "test-support"))]
            Self::Identifiers => (false, true),
            #[cfg(any(test, feature = "test-support"))]
            Self::RankedTerms => (true, true),
            Self::LegacyV2 => (true, true),
        }
    }

    pub(crate) fn uses_semantic_subqueries(self) -> bool {
        self == Self::SemanticSubqueries || matches!(self, Self::Fusion { .. })
    }

    pub(crate) fn branch_weight(self, branch: BranchKind) -> f64 {
        match (self, branch) {
            (Self::Fusion { weighted: true, .. }, BranchKind::Key) => 0.9,
            _ => 1.0,
        }
    }

    pub(crate) fn file_limit(self) -> Option<usize> {
        match self {
            Self::LegacyV2 => Some(PER_FILE),
            Self::Fusion { diverse: true, .. } => Some(2),
            _ => None,
        }
    }
}

#[cfg(feature = "test-support")]
static SWITCH: std::sync::OnceLock<Option<SearchProfile>> = std::sync::OnceLock::new();

#[cfg(any(test, feature = "test-support"))]
fn parse(value: Result<String, std::env::VarError>) -> Option<SearchProfile> {
    match value {
        Err(std::env::VarError::NotPresent) => Some(SearchProfile::Baseline),
        Ok(value) => match value.as_str() {
            "none" => Some(SearchProfile::Baseline),
            "ranked_keywords" => Some(SearchProfile::Keywords),
            "semantic_subqueries" => Some(SearchProfile::SemanticSubqueries),
            "fusion_weights" => Some(SearchProfile::Fusion {
                weighted: true,
                diverse: false,
            }),
            "fusion_diversity" => Some(SearchProfile::Fusion {
                weighted: false,
                diverse: true,
            }),
            "fusion_combined" => Some(SearchProfile::Fusion {
                weighted: true,
                diverse: true,
            }),
            "ranked_identifiers" => Some(SearchProfile::Identifiers),
            "ranked_terms" => Some(SearchProfile::RankedTerms),
            "search_v2" => Some(SearchProfile::LegacyV2),
            _ => None,
        },
        Err(_) => None,
    }
}

#[cfg(feature = "test-support")]
fn from_environment() -> Option<SearchProfile> {
    let profile = parse(std::env::var(VARIABLE));
    if profile == Some(SearchProfile::LegacyV2) {
        eprintln!("AIRK_EVALUATION_SEARCH bound=search_v2");
    }
    profile
}

/// Preserve the original private switch binding for historical replays.
#[cfg(feature = "test-support")]
pub fn bind_evaluation_search(on: bool) -> anyhow::Result<()> {
    let profile = if on {
        SearchProfile::LegacyV2
    } else {
        SearchProfile::Baseline
    };
    let bound = *SWITCH.get_or_init(|| Some(profile));
    anyhow::ensure!(
        bound == Some(profile),
        "evaluation search is already bound differently"
    );
    Ok(())
}

pub(crate) fn active() -> anyhow::Result<SearchProfile> {
    #[cfg(feature = "test-support")]
    {
        SWITCH
            .get_or_init(from_environment)
            .ok_or_else(|| anyhow::anyhow!("{VARIABLE} has an unsupported Search profile"))
    }
    #[cfg(not(feature = "test-support"))]
    {
        Ok(SearchProfile::Fusion {
            weighted: false,
            diverse: true,
        })
    }
}

/// The rewritten lexical side of one prose instruction.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Plan {
    /// Every key word, as any-term groups.
    pub key: Vec<TermGroup>,
    /// Named identifiers and proper nouns only; empty when they equal `key`.
    pub named: Vec<TermGroup>,
    /// Snippet focus text: the key words, separated by spaces.
    pub focus: String,
}

/// Which fused branch a candidate came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BranchKind {
    Conjunctive,
    Vector,
    Key,
    Named,
}

impl Plan {
    /// A plan for a prose instruction, or `None` for any other query (which
    /// then searches exactly as the product does).
    pub(crate) fn for_query(text: &str) -> Option<Self> {
        let words = words(text);
        if words.len() < PROSE_WORDS {
            return None;
        }
        let mut named = Vec::<(String, TermGroup)>::new();
        let mut plain = Vec::<(String, TermGroup)>::new();
        let mut seen = BTreeSet::new();
        for word in &words {
            let lower = word.text.to_lowercase();
            if !seen.insert(lower.clone()) {
                continue;
            }
            // A word no ranked group can carry (over 128 bytes, or not an
            // identifier term once lowercased) is skipped, never an error.
            let Some(group) = group(word.text) else {
                continue;
            };
            let is_named = identifier_like(word.text) || (word.capitalized && !word.initial);
            if is_named && !STOP_WORDS.contains(&lower.as_str()) {
                named.push((lower, group));
            } else if lower.chars().count() >= 3
                && !STOP_WORDS.contains(&lower.as_str())
                && !lower.chars().all(char::is_numeric)
            {
                plain.push((lower, group));
            }
        }
        let mut key: Vec<(String, TermGroup)> = named.iter().chain(&plain).cloned().collect();
        key.truncate(MAX_KEY_WORDS.min(MAX_RANKED_GROUPS));
        if key.is_empty() {
            return None;
        }
        named.truncate(MAX_RANKED_GROUPS);
        let focus = key
            .iter()
            .map(|(word, _)| word.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let named = if named.is_empty() || named.len() == key.len() {
            Vec::new()
        } else {
            named.into_iter().map(|(_, group)| group).collect()
        };
        Some(Self {
            key: key.into_iter().map(|(_, group)| group).collect(),
            named,
            focus,
        })
    }

    pub(crate) fn weight(kind: BranchKind) -> f64 {
        match kind {
            BranchKind::Conjunctive => 1.0,
            BranchKind::Vector => VECTOR_WEIGHT,
            BranchKind::Key => KEY_WEIGHT,
            BranchKind::Named => NAMED_WEIGHT,
        }
    }
}

struct Word<'a> {
    text: &'a str,
    capitalized: bool,
    /// First word of the text or of a sentence.
    initial: bool,
}

fn words(text: &str) -> Vec<Word<'_>> {
    let mut result = Vec::new();
    let mut sentence_start = true;
    let mut start = None;
    let bytes: Vec<(usize, char)> = text.char_indices().collect();
    let mut push = |begin: usize, end: usize, sentence_start: &mut bool| {
        let word = &text[begin..end];
        if word.chars().any(char::is_alphanumeric) {
            result.push(Word {
                text: word,
                capitalized: word.chars().next().is_some_and(char::is_uppercase),
                initial: *sentence_start,
            });
            *sentence_start = false;
        }
    };
    for &(index, character) in &bytes {
        if character.is_alphanumeric() || character == '_' {
            start.get_or_insert(index);
            continue;
        }
        if let Some(begin) = start.take() {
            push(begin, index, &mut sentence_start);
        }
        if matches!(character, '.' | '!' | '?' | ':' | ';' | '\n') {
            sentence_start = true;
        }
    }
    if let Some(begin) = start {
        push(begin, text.len(), &mut sentence_start);
    }
    result
}

/// CamelCase, snake_case, mixed letters and digits, or an all-capitals
/// acronym of two to five letters.
fn identifier_like(word: &str) -> bool {
    let letters = word.chars().filter(|c| c.is_alphabetic()).count();
    let upper = word.chars().filter(|c| c.is_uppercase()).count();
    let lower = word.chars().filter(|c| c.is_lowercase()).count();
    word.contains('_')
        || (upper > 0 && lower > 0 && word.chars().skip(1).any(char::is_uppercase))
        || (letters > 0 && word.chars().any(char::is_numeric))
        || (upper == letters && (2..=5).contains(&letters))
}

/// The whole lowercase word, or all its identifier parts (split from the
/// original spelling, so CamelCase parts survive).
/// Alternatives the ranked reader would refuse are left out; `None` when
/// neither remains.
fn group(original: &str) -> Option<TermGroup> {
    let lower = original.to_lowercase();
    let word = lower.as_str();
    let parts: Vec<String> = terms(original)
        .into_iter()
        .filter(|part| part != word)
        .collect();
    let mut alternatives = Vec::new();
    for alternative in [vec![word.to_owned()], parts] {
        if valid_alternative(&alternative) {
            alternatives.push(alternative);
        }
    }
    (!alternatives.is_empty()).then_some(TermGroup { alternatives })
}

/// The ranking prior for a file kind: generated and build output last, office
/// documents and plain prose below source, governing documents with source.
pub(crate) fn prior(path: &str) -> f64 {
    let lower = path.to_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    let segments: Vec<&str> = lower.split('/').collect();
    let generated_folder = segments[..segments.len().saturating_sub(1)]
        .iter()
        .any(|s| {
            matches!(
                *s,
                "node_modules"
                    | "dist"
                    | "build"
                    | "bin"
                    | "obj"
                    | "out"
                    | "target"
                    | "generated"
                    | "vendor"
                    | ".next"
                    | "coverage"
            )
        });
    let extension = name.rsplit_once('.').map_or("", |(_, extension)| extension);
    // Machine output by extension alone (patches, diffs, captured inputs and
    // outputs, snapshots, locks, source maps, logs), never by repository path.
    let machine_extension = matches!(
        extension,
        "patch" | "diff" | "rej" | "orig" | "in" | "out" | "snap" | "lock" | "map" | "log"
    );
    let generated_name = machine_extension
        || name.ends_with(".min.js")
        || name.ends_with(".min.css")
        || name.ends_with(".g.cs")
        || name.ends_with(".designer.cs")
        || name.ends_with(".generated.ts")
        || matches!(
            name,
            "package-lock.json" | "yarn.lock" | "pnpm-lock.yaml" | "cargo.lock"
        );
    if generated_folder || generated_name {
        return 0.3;
    }
    if matches!(
        extension,
        "docx" | "doc" | "pptx" | "xlsx" | "pdf" | "odt" | "rtf"
    ) {
        return 0.5;
    }
    let governing = matches!(
        name,
        "readme.md" | "contract.md" | "claude.md" | "agents.md" | "contributing.md"
    );
    if !governing
        && matches!(
            extension,
            "md" | "markdown" | "txt" | "rst" | "adoc" | "html"
        )
    {
        return 0.75;
    }
    1.0
}

/// Stable order with every file's first entries under the threshold ahead of any
/// file's later entries; within each part the given order is kept.
pub(crate) fn diversify<T>(ranked: Vec<T>, per_file: usize, path: impl Fn(&T) -> String) -> Vec<T> {
    let mut counts = BTreeMap::<String, usize>::new();
    let mut first = Vec::with_capacity(ranked.len());
    let mut deferred = Vec::new();
    for entry in ranked {
        let count = counts.entry(path(&entry)).or_default();
        *count += 1;
        if *count <= per_file {
            first.push(entry);
        } else {
            deferred.push(entry);
        }
    }
    first.extend(deferred);
    first
}

/// English function words and instruction verbs that carry no code location.
const STOP_WORDS: &[&str] = &[
    "a", "about", "above", "across", "add", "after", "again", "all", "allow", "also", "an", "and",
    "another", "any", "are", "as", "at", "back", "be", "because", "been", "before", "being",
    "below", "between", "both", "but", "by", "can", "change", "check", "could", "describe", "did",
    "do", "does", "doing", "done", "down", "during", "each", "either", "else", "ensure", "every",
    "explain", "few", "find", "for", "from", "further", "get", "give", "go", "had", "has", "have",
    "having", "he", "her", "here", "how", "however", "i", "if", "in", "into", "is", "it", "its",
    "itself", "just", "keep", "let", "like", "make", "may", "me", "might", "more", "most", "must",
    "my", "name", "need", "needs", "new", "next", "no", "nor", "not", "now", "of", "off", "on",
    "once", "one", "only", "or", "other", "our", "out", "over", "own", "please", "same", "see",
    "should", "show", "so", "some", "such", "than", "that", "the", "their", "them", "then",
    "there", "these", "they", "this", "those", "through", "to", "too", "trace", "under", "until",
    "up", "us", "use", "used", "using", "very", "via", "want", "was", "way", "we", "were", "what",
    "when", "where", "whether", "which", "while", "who", "whom", "why", "will", "with", "within",
    "without", "would", "write", "you", "your",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn alternatives(plan: &[TermGroup]) -> Vec<Vec<Vec<String>>> {
        plan.iter()
            .map(|group| group.alternatives.clone())
            .collect()
    }

    #[test]
    fn switch_values_parse_without_reading_the_environment() {
        use std::env::VarError;
        assert_eq!(
            parse(Err(VarError::NotPresent)),
            Some(SearchProfile::Baseline)
        );
        assert_eq!(parse(Ok("none".into())), Some(SearchProfile::Baseline));
        assert_eq!(parse(Ok("search_v2".into())), Some(SearchProfile::LegacyV2));
        assert_eq!(
            parse(Ok("ranked_keywords".into())),
            Some(SearchProfile::Keywords)
        );
        assert_eq!(
            parse(Ok("ranked_identifiers".into())),
            Some(SearchProfile::Identifiers)
        );
        assert_eq!(
            parse(Ok("ranked_terms".into())),
            Some(SearchProfile::RankedTerms)
        );
        assert_eq!(parse(Ok("Search_V2".into())), None);
        assert_eq!(parse(Ok(String::new())), None);
    }

    #[test]
    fn words_no_ranked_group_can_carry_are_skipped_not_refused() {
        let long = "a".repeat(129);
        let long_camel = format!("{}Part", "b".repeat(130));
        let text = format!(
            "Add İstanbul support to {long} and {long_camel} beside the DeepSeek provider factory"
        );
        let plan = Plan::for_query(&text).expect("prose plan");
        assert!(plan.key.iter().chain(&plan.named).all(TermGroup::valid));
        let flat: Vec<&str> = plan
            .key
            .iter()
            .flat_map(|group| group.alternatives.iter().flatten())
            .map(String::as_str)
            .collect();
        assert!(!flat.contains(&long.as_str()));
        assert!(!flat.iter().any(|term| term.contains('\u{307}')));
        // An alternative is never loosened: the overlong CamelCase word has
        // no valid alternative left, so it is skipped whole.
        assert!(!flat.contains(&"part"));
        assert!(flat.contains(&"deepseek"));
        assert_eq!(plan.focus, "deepseek support beside provider factory");
        assert!(!plan.focus.contains(&long));
    }

    #[test]
    fn short_and_identifier_queries_have_no_plan() {
        assert_eq!(Plan::for_query("DeepSeekProvider"), None);
        assert_eq!(Plan::for_query("where is the provider factory"), None);
    }

    #[test]
    fn prose_plans_put_named_words_first_and_drop_stop_words() {
        let plan = Plan::for_query(
            "Add Mistral as another AI provider next to OpenAI and DeepSeek. Users should store the key_name.",
        )
        .expect("prose plan");
        assert_eq!(
            plan.focus,
            "mistral ai openai deepseek key_name provider users store"
        );
        assert_eq!(
            alternatives(&plan.named),
            vec![
                vec![vec!["mistral".to_owned()]],
                vec![vec!["ai".to_owned()]],
                vec![
                    vec!["openai".to_owned()],
                    vec!["ai".to_owned(), "open".to_owned()]
                ],
                vec![
                    vec!["deepseek".to_owned()],
                    vec!["deep".to_owned(), "seek".to_owned()]
                ],
                vec![
                    vec!["key_name".to_owned()],
                    vec!["key".to_owned(), "name".to_owned()]
                ],
            ]
        );
        assert_eq!(plan.key.len(), 8);
        // "Users" begins a sentence, so it is a plain key word, not a name.
        assert_eq!(plan.key[6].alternatives, vec![vec!["users".to_owned()]]);
    }

    #[test]
    fn named_branch_is_omitted_when_every_key_word_is_named() {
        let plan = Plan::for_query(
            "OpenAI DeepSeek OpenRouter AnthropicClient GeminiClient OllamaHost MistralKey ApiKey",
        )
        .expect("prose plan");
        assert!(plan.named.is_empty());
        assert_eq!(plan.key.len(), 8);
    }

    #[test]
    fn priors_order_generated_office_prose_and_source() {
        assert_eq!(prior("pro/src/App/Provider.cs"), 1.0);
        assert_eq!(prior("pro/src/ui/stores/pipelineStore.ts"), 1.0);
        assert_eq!(prior("CLAUDE.md"), 1.0);
        assert_eq!(prior("pro/pipeline-products/README.md"), 1.0);
        assert_eq!(prior("pro/docs/plan.md"), 0.75);
        assert_eq!(prior("pro/docs/code-review.docx"), 0.5);
        assert_eq!(prior("pro/src/ui/dist/app.js"), 0.3);
        assert_eq!(prior("pro/src/ui/package-lock.json"), 0.3);
        assert_eq!(prior("pro/src/Forms/Main.Designer.cs"), 0.3);
        for machine in [
            "scripts/history/sources/abc--kernel.diff",
            "notes/p6f-foundation.patch",
            "x/sources/0bf8--lib.rs.in",
            "tests/snapshots/render.snap",
            "logs/run.log",
            "a/fix.rej",
            "web/app.js.map",
            "src/App.out",
        ] {
            assert_eq!(prior(machine), 0.3, "{machine}");
        }
        // Source with the same stems stays source; no path rule is involved.
        for source in [
            "scripts/history/diff.py",
            "src/patch.rs",
            "src/input.ts",
            "src/inline.rs",
        ] {
            assert_eq!(prior(source), 1.0, "{source}");
        }
        // A folder named like a generated folder only counts as a folder.
        assert_eq!(prior("pro/src/build"), 1.0);
    }

    #[test]
    fn diversify_defers_a_files_fourth_hit_behind_other_files() {
        let ranked = vec!["a", "a", "a", "a", "b", "a", "c"];
        assert_eq!(
            diversify(ranked, PER_FILE, |path| (*path).to_owned()),
            vec!["a", "a", "a", "b", "c", "a", "a"]
        );
    }

    #[test]
    fn sentence_starts_are_not_names() {
        let found: Vec<(String, bool)> = words("Trace the Vue store. Then Photino: Bridge")
            .into_iter()
            .map(|word| (word.text.to_owned(), word.capitalized && !word.initial))
            .collect();
        assert_eq!(
            found,
            vec![
                ("Trace".to_owned(), false),
                ("the".to_owned(), false),
                ("Vue".to_owned(), true),
                ("store".to_owned(), false),
                ("Then".to_owned(), false),
                ("Photino".to_owned(), true),
                ("Bridge".to_owned(), false),
            ]
        );
    }
}
