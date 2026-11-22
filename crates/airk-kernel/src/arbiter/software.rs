// SPDX-License-Identifier: Apache-2.0
//! Private evaluation change set 1: the software-task gathering shape.
//!
//! Every layer is off unless the private evaluation fixture binds it for the
//! process; product Answer gathering is unchanged. Deterministic, no model.
use super::*;
#[cfg(feature = "test-support")]
use std::sync::OnceLock;

/// The separable layers of change set 1 (plan 5.2 step 5 leave-one-out).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Layers {
    /// Change 1: definition, references, tests, outline, governing and whole-chunk reads.
    pub followups: bool,
    /// Change 2: follow-ups merged round-robin across kinds and hits.
    pub spread: bool,
    /// Change 3: wider first search with focused snippets.
    pub pool: bool,
    /// Change 4 (kernel half): Focus pair cap, bounded windows and kept hits.
    pub focus: bool,
    /// Change 7: overlap removal, grouping, disclosure and omission rank.
    pub present: bool,
    /// Change 8: measured-cost gate before each Context call.
    pub device: bool,
    /// Change set 2, Context option B: wider roster and the best Read even when
    /// a terminal card wins (`airk_retrieval::arbiter::context_wide`). Never in
    /// `all`; it needs an option-B Context profile.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub context_b: bool,
    /// Change set 2: hits kept for whole chunks and follow-ups (default `KEPT`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus_keep: Option<u8>,
    /// Change set 2: Focus relevance threshold in thousandths (default `FOCUS_KEEP`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus_threshold_milli: Option<u16>,
    /// Replay tuning: first-search pool size (default `POOL`); needs `pool`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool_size: Option<u8>,
    /// Replay tuning: top pool hits fetched with full chunks (default: the
    /// whole pool when it is at most 8, otherwise none); needs `pool`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool_full: Option<u8>,
    /// Replay tuning: lower clamp of the per-read byte split (default `READ_FLOOR`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_floor: Option<u16>,
    /// Replay tuning: identifier lookups only for snake_case or multi-word
    /// CamelCase names that kept hit text names (`subjects=strict`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub subjects_strict: bool,
    /// Replay tuning: governing probes only inside the hit's project folder
    /// (`governing=project`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub governing_project: bool,
    /// Change set 3: follow-up searches built from the instruction's named
    /// identifiers and key nouns (`keyword_searches=<1..=6>`; `0` is off).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyword_searches: Option<u8>,
    /// Change set 3: Context calls and late searches respect the remaining
    /// deadline with worst-case estimates (`deadline=strict`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deadline_strict: bool,
}

impl Layers {
    /// Change set 1's layers; `all` turns on exactly these.
    pub const NAMES: [&'static str; 6] =
        ["followups", "spread", "pool", "focus", "present", "device"];

    pub fn all() -> Self {
        Self {
            followups: true,
            spread: true,
            pool: true,
            focus: true,
            present: true,
            device: true,
            ..Self::default()
        }
    }

    fn slot(&mut self, name: &str) -> Result<&mut bool> {
        Ok(match name {
            "followups" => &mut self.followups,
            "spread" => &mut self.spread,
            "pool" => &mut self.pool,
            "focus" => &mut self.focus,
            "present" => &mut self.present,
            "device" => &mut self.device,
            "context_b" => &mut self.context_b,
            _ => anyhow::bail!("unknown change-set layer {name:?}"),
        })
    }

    /// `all`, `none`, or a comma list of layer names; `-name` removes a layer,
    /// so `all,-focus` leaves Focus's kernel half out. Change set 2 adds
    /// `context_b`, `focus_keep=<1..=16>` and `focus_threshold=<0..=1, at most
    /// three decimals>`; `all` and `none` reset them, so name values after them.
    /// Replay tuning adds `pool=<1..=16>`, `pool_full=<0..=16>`,
    /// `read_floor=<512..=8192>`, `kept=<1..=16>` (an alias of `focus_keep`),
    /// `subjects=strict|loose` and `governing=project|any`; the second value of
    /// each pair, and every omitted value, is change set 1's behaviour.
    pub fn parse(text: &str) -> Result<Self> {
        let mut layers = Self::default();
        let mut seen = BTreeSet::new();
        // A tuned value is given at most once (`kept=` is `focus_keep=`).
        let mut valued: BTreeSet<&str> = BTreeSet::new();
        for token in text.split(',').map(str::trim) {
            ensure!(
                !token.is_empty() && seen.insert(token),
                "empty or repeated change-set layer"
            );
            let number = |prefix: &str, range: std::ops::RangeInclusive<u16>| -> Result<u16> {
                let value: u16 = token[prefix.len()..].parse()?;
                ensure!(
                    range.contains(&value),
                    "{} is {} to {}",
                    prefix.trim_end_matches('='),
                    range.start(),
                    range.end()
                );
                Ok(value)
            };
            match token {
                "all" => layers = Self::all(),
                "none" => layers = Self::default(),
                "subjects=strict" => layers.subjects_strict = true,
                "subjects=loose" => layers.subjects_strict = false,
                "governing=project" => layers.governing_project = true,
                "governing=any" => layers.governing_project = false,
                "deadline=strict" => layers.deadline_strict = true,
                "deadline=lenient" => layers.deadline_strict = false,
                _ if token.starts_with("keyword_searches=") => {
                    let count = number("keyword_searches=", 0..=6)? as u8;
                    layers.keyword_searches = (count > 0).then_some(count);
                }
                _ if token.starts_with("focus_keep=") => {
                    ensure!(valued.insert("focus_keep"), "focus_keep is given more than once");
                    layers.focus_keep = Some(number("focus_keep=", 1..=16)? as u8);
                }
                _ if token.starts_with("kept=") => {
                    ensure!(valued.insert("focus_keep"), "focus_keep is given more than once");
                    layers.focus_keep = Some(number("kept=", 1..=16)? as u8);
                }
                _ if token.starts_with("pool=") => {
                    layers.pool_size = Some(number("pool=", 1..=MAX_POOL as u16)? as u8);
                }
                _ if token.starts_with("pool_full=") => {
                    layers.pool_full = Some(number("pool_full=", 0..=MAX_POOL as u16)? as u8);
                }
                _ if token.starts_with("read_floor=") => {
                    layers.read_floor = Some(number("read_floor=", 512..=8192)?);
                }
                _ if token.starts_with("focus_threshold=") => {
                    ensure!(
                        valued.insert("focus_threshold"),
                        "focus_threshold is given more than once"
                    );
                    let value: f64 = token["focus_threshold=".len()..].parse()?;
                    let milli = (value * 1000.0).round();
                    ensure!(
                        (0.0..=1.0).contains(&value) && (value * 1000.0 - milli).abs() < 1e-6,
                        "focus_threshold is 0 to 1 with at most three decimals"
                    );
                    layers.focus_threshold_milli = Some(milli as u16);
                }
                _ => match token.strip_prefix('-') {
                    Some(name) => *layers.slot(name)? = false,
                    None => *layers.slot(token)? = true,
                },
            }
        }
        Ok(layers)
    }

    /// Bound layers and set values, change set 1's names first.
    pub fn names(self) -> Vec<String> {
        let mut copy = self;
        let mut names: Vec<String> = Self::NAMES
            .into_iter()
            .chain(["context_b"])
            .filter(|name| copy.slot(name).is_ok_and(|on| *on))
            .map(str::to_owned)
            .collect();
        if let Some(kept) = self.focus_keep {
            names.push(format!("focus_keep={kept}"));
        }
        if let Some(milli) = self.focus_threshold_milli {
            names.push(format!("focus_threshold={}.{:03}", milli / 1000, milli % 1000));
        }
        if let Some(pool) = self.pool_size {
            names.push(format!("pool={pool}"));
        }
        if let Some(full) = self.pool_full {
            names.push(format!("pool_full={full}"));
        }
        if let Some(floor) = self.read_floor {
            names.push(format!("read_floor={floor}"));
        }
        if self.subjects_strict {
            names.push("subjects=strict".into());
        }
        if self.governing_project {
            names.push("governing=project".into());
        }
        if let Some(count) = self.keyword_searches {
            names.push(format!("keyword_searches={count}"));
        }
        if self.deadline_strict {
            names.push("deadline=strict".into());
        }
        names
    }

    /// First-search pool size.
    pub(super) fn pool_size(self) -> usize {
        self.pool_size.map_or(POOL, usize::from)
    }

    /// Lower clamp of the per-read byte split.
    pub(super) fn read_floor(self) -> usize {
        self.read_floor.map_or(READ_FLOOR, usize::from)
    }

    /// Hits kept for whole chunks and follow-ups.
    pub(super) fn kept(self) -> usize {
        self.focus_keep.map_or(KEPT, usize::from)
    }

    /// The Focus relevance a scored hit needs to be kept.
    pub(super) fn keep_threshold(self) -> f32 {
        self.focus_threshold_milli
            .map_or(FOCUS_KEEP, |milli| f32::from(milli) / 1000.0)
    }

    pub fn any(self) -> bool {
        self != Self::default()
    }
}

#[cfg(feature = "test-support")]
static LAYERS: OnceLock<Layers> = OnceLock::new();

/// Bound once per process, only by the private evaluation fixture. A second
/// binding must repeat the same layers. Test-support builds only: a product
/// build has no binder, so its layers are always off.
#[cfg(feature = "test-support")]
pub fn bind_evaluation_layers(layers: Layers) -> Result<()> {
    let bound = *LAYERS.get_or_init(|| layers);
    ensure!(
        bound == layers,
        "evaluation layers are already bound differently"
    );
    Ok(())
}

pub(crate) fn evaluation_layers() -> Layers {
    #[cfg(feature = "test-support")]
    {
        LAYERS.get().copied().unwrap_or_default()
    }
    #[cfg(not(feature = "test-support"))]
    {
        Layers::default()
    }
}

/// Starting values, tuned in replay (plan 5.2 step 4). One shape on every
/// device, so CPU and GPU gather the same evidence (decision 8).
pub(super) const POOL: usize = 16;
/// Change set 3: the largest `pool=` (the search tool allows 100 hits).
pub(super) const MAX_POOL: usize = 48;
/// Change set 3: bytes per focused snippet in a pool wider than `POOL`, so the
/// whole pool's record stays near 16 KiB beside the full-chunk top hits.
pub(super) const WIDE_POOL_SNIPPET_BYTES: usize = 320;
/// Change set 3: presentation bytes for pool hits beyond the kept hits; the
/// rest are listed as a compact location menu.
pub(super) const WIDE_POOL_PRESENT_BYTES: usize = 9_000;
/// Change set 3 (`deadline=strict`): the first Context call's worst case
/// (Compact at 8,192 tokens measured 47.6 s on CPU).
pub(super) const STRICT_FIRST_CONTEXT_MS: u64 = 50_000;
/// Change set 3: no read is started with less than this left.
pub(super) const STRICT_DISPATCH_RESERVE_MS: u64 = 5_000;
/// Change set 3: no follow-up search (which may run Focus) with less than this left.
pub(super) const STRICT_SEARCH_RESERVE_MS: u64 = 25_000;
pub(super) const KEPT: usize = 4;
pub(super) const FOCUS_PAIRS: usize = 6;
pub(super) const FOCUS_KEEP: f32 = 0.10;
pub(super) const FRAME_PLAN: usize = 52_000;
pub(super) const SUBJECTS: usize = 3;
pub(super) const GOVERNING_DOCUMENTS: usize = 2;
pub(super) const GOVERNING_PROBES: usize = 6;
pub(super) const OUTLINED_FILES: usize = 2;
/// Lines a whole-chunk read covers past a snippet's first line (context `after`).
pub(super) const WHOLE_CHUNK_LINES: usize = 40;
/// Lower clamp of the per-read byte split.
pub(super) const READ_FLOOR: usize = 1536;
pub(super) const FIRST_CONTEXT_ESTIMATE_MS: u64 = 12_000;
pub(super) const SINGLE_CONTEXT_CALL_MS: u64 = 30_000;
pub(super) const READ_RESERVE_MS: u64 = 10_000;

/// The kind of each change-set read; merge priority is declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FollowUp {
    Initial,
    WholeChunk,
    Definition,
    Governing,
    References,
    Tests,
    Excerpt,
    Callers,
    Outline,
    Literal,
    /// Change set 3: an identifier or key-noun search (`keyword_searches`).
    Keyword,
}

impl FollowUp {
    pub(super) fn tag(self) -> &'static [u8] {
        match self {
            Self::Initial => b"initial",
            Self::WholeChunk => b"whole_chunk",
            Self::Definition => b"definition",
            Self::Governing => b"governing",
            Self::References => b"references",
            Self::Tests => b"tests",
            Self::Excerpt => b"excerpt",
            Self::Callers => b"callers",
            Self::Outline => b"outline",
            Self::Literal => b"literal",
            Self::Keyword => b"keyword",
        }
    }

    /// Lower ranks are omitted first when the packet must shrink.
    pub(super) fn keep_rank(self) -> u8 {
        match self {
            Self::Literal => 0,
            Self::Outline | Self::Keyword => 1,
            Self::Callers => 2,
            Self::Tests => 3,
            Self::Excerpt => 4,
            Self::References => 5,
            Self::Governing => 6,
            Self::Definition => 7,
            Self::WholeChunk => 8,
            Self::Initial => 9,
        }
    }
}

const STOP: &[&str] = &[
    "GitHub",
    "JavaScript",
    "TypeScript",
    "PowerShell",
    "MacOS",
    "OpenAI",
    "WebAssembly",
    "YouTube",
    "iPhone",
    "VSCode",
    "NuGet",
    "DevOps",
    "PostgreSQL",
    "MySQL",
    "SQLite",
    "OAuth",
    "README",
    "TODO",
    "self_",
];

fn identifier_like(word: &str) -> bool {
    let bytes = word.as_bytes();
    if !(3..=96).contains(&bytes.len())
        || !bytes[0].is_ascii_alphabetic() && bytes[0] != b'_'
        || STOP.contains(&word)
    {
        return false;
    }
    let lower = word.bytes().any(|b| b.is_ascii_lowercase());
    let inner_capital = word.bytes().skip(1).any(|b| b.is_ascii_uppercase());
    let snake = word.contains('_') && word.bytes().any(|b| b.is_ascii_alphanumeric());
    (lower && inner_capital) || snake
}

/// Code identifiers named in `text`, first-seen order, case-sensitive, at most
/// `limit`: backticked words, `a::b` paths (last segment), snake_case and
/// CamelCase. English words and dotted file names are not identifiers.
pub(crate) fn identifiers(text: &str, limit: usize) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut push = |candidate: &str, quoted: bool| {
        let segments: Vec<&str> = candidate
            .split("::")
            .map(|part| part.trim_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '_')))
            .collect();
        let path = segments.len() > 1;
        for (index, name) in segments.iter().enumerate() {
            let plain = (3..=96).contains(&name.len())
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                && name.as_bytes()[0].is_ascii_alphabetic()
                && !STOP.contains(name);
            // In `a::b` the last segment is the item; earlier segments count
            // only when they name a type, never a lowercase module path.
            let accepted = if path {
                plain
                    && (index + 1 == segments.len() || name.as_bytes()[0].is_ascii_uppercase())
            } else if quoted {
                plain
            } else {
                identifier_like(name)
            };
            if accepted && found.len() < limit && !found.iter().any(|seen| seen == name) {
                found.push((*name).to_owned());
            }
        }
    };
    let mut rest = text;
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('`') else { break };
        let quoted = &after[..end];
        if !quoted.chars().any(char::is_whitespace) {
            push(quoted.trim_end_matches("()"), true);
        }
        rest = &after[end + 1..];
    }
    for word in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':')) {
        let word = word.trim_matches(':');
        if !word.is_empty() {
            push(word, false);
        }
    }
    found
}

/// Names declared on definition lines of returned code (`fn`, `struct`, ...).
pub(crate) fn declared_names(text: &str, limit: usize) -> Vec<String> {
    const KEYWORDS: [&str; 11] = [
        "fn",
        "struct",
        "enum",
        "trait",
        "class",
        "interface",
        "def",
        "function",
        "type",
        "record",
        "mod",
    ];
    let mut found: Vec<String> = Vec::new();
    for line in text.lines() {
        let words: Vec<&str> = line
            .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .filter(|word| !word.is_empty())
            .collect();
        for pair in words.windows(2) {
            if KEYWORDS.contains(&pair[0])
                && (3..=96).contains(&pair[1].len())
                && pair[1].as_bytes()[0].is_ascii_alphabetic()
                && !STOP.contains(&pair[1])
                && !found.iter().any(|seen| seen == pair[1])
                && found.len() < limit
            {
                found.push(pair[1].to_owned());
            }
        }
    }
    found
}

/// `subjects=strict`: a snake_case name (two or more `_`-separated parts) or a
/// CamelCase name with a second word (`ChapterService`, `HTTPClient`); a plain
/// capitalised word such as a product name (`Gathering`) is not one.
pub(crate) fn strict_subject(name: &str) -> bool {
    let bytes = name.as_bytes();
    let snake = name.split('_').filter(|part| !part.is_empty()).count() >= 2;
    let camel = bytes.iter().any(u8::is_ascii_lowercase)
        && (1..bytes.len()).any(|at| {
            bytes[at].is_ascii_uppercase()
                && (bytes[at - 1].is_ascii_lowercase()
                    || bytes[at - 1].is_ascii_digit()
                    || bytes[at - 1].is_ascii_uppercase()
                        && bytes.get(at + 1).is_some_and(u8::is_ascii_lowercase))
        });
    snake || camel
}

/// `text` names `name` as a whole identifier, not inside a longer one.
pub(crate) fn names_identifier(text: &str, name: &str) -> bool {
    let word = |byte: Option<u8>| byte.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_');
    !name.is_empty()
        && text.match_indices(name).any(|(at, _)| {
            !word(text[..at].bytes().next_back()) && !word(text[at + name.len()..].bytes().next())
        })
}

/// Folders that hold projects: in `crates/<name>/...` the project is `crates/<name>`.
const PROJECT_CONTAINERS: [&str; 5] = ["crates", "packages", "apps", "libs", "src"];

/// The project folder of `path`: the folder after the first project container
/// (`crates/airk-kernel`, `pro/src/WritingForgePro.Services`), otherwise the
/// top-level folder (`docs`). A file at the root has none.
pub(crate) fn project_folder(path: &str) -> Option<String> {
    let segments: Vec<&str> = path.split('/').collect();
    let folders = &segments[..segments.len() - 1];
    if folders.is_empty() {
        return None;
    }
    let end = folders
        .iter()
        .position(|folder| PROJECT_CONTAINERS.contains(folder))
        .filter(|at| at + 1 < folders.len())
        .map_or(1, |at| at + 2);
    Some(folders[..end].join("/"))
}

/// `governing=project`: `candidate` lies in the project folder of `path`.
pub(crate) fn in_project(path: &str, candidate: &str) -> bool {
    let folder = candidate.rsplit_once('/').map_or("", |(folder, _)| folder);
    project_folder(path).is_some_and(|project| {
        folder == project || folder.strip_prefix(project.as_str()).is_some_and(|rest| rest.starts_with('/'))
    })
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

pub(crate) fn is_test_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let name = file_name(&lower);
    let directory = lower[..lower.len() - name.len()].trim_end_matches('/');
    directory.split('/').any(|part| {
        matches!(part, "tests" | "test" | "__tests__" | "spec" | "specs")
            || part.ends_with(".tests")
            || part.ends_with(".test")
    }) || name.contains(".test.")
        || name.contains(".spec.")
        || name.starts_with("test_")
        || name.ends_with("_test.rs")
        || name.ends_with("_tests.rs")
        || name == "tests.rs"
        || name.ends_with("_test.py")
        || name.ends_with("_test.go")
        || name.ends_with("tests.cs")
        || name.ends_with("test.cs")
}

pub(crate) fn is_document_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [".md", ".mdx", ".rst", ".txt", ".adoc"]
        .iter()
        .any(|extension| lower.ends_with(extension))
        || lower.starts_with("docs/")
        || lower.contains("/docs/")
}

pub(crate) fn is_governing_path(path: &str) -> bool {
    matches!(file_name(path), "CONTRACT.md" | "README.md" | "AGENTS.md")
}

/// Contract and README candidates that may govern `path`, nearest first: the
/// Rust module folder of a `name.rs` file, its own folder, then up to two
/// ancestors. The repository root is excluded.
pub(crate) fn governing_candidates(path: &str) -> Vec<String> {
    let mut folders = Vec::new();
    if let Some(stem) = path.strip_suffix(".rs")
        && !file_name(path).starts_with("mod.")
        && file_name(path) != "lib.rs"
        && file_name(path) != "main.rs"
    {
        folders.push(stem.to_owned());
    }
    let mut folder = path;
    for _ in 0..3 {
        let Some((parent, _)) = folder.rsplit_once('/') else {
            break;
        };
        folders.push(parent.to_owned());
        folder = parent;
    }
    folders
        .into_iter()
        .flat_map(|folder| {
            ["CONTRACT.md", "README.md"].map(|name| format!("{folder}/{name}"))
        })
        .filter(|candidate| candidate != path)
        .collect()
}

/// The words Context sees for each read, ending in the anchor's location so
/// the next-read parser still finds a range after the last `:`.
pub(crate) fn target(kind: FollowUp, subject: &str, path: &str, start: usize, end: usize) -> String {
    let words = match kind {
        FollowUp::Initial => format!("Search the index for: {subject}"),
        FollowUp::WholeChunk => format!("Read the whole indexed chunk of {path}"),
        FollowUp::Definition => format!("Find the definition of {subject} (symbols) named"),
        FollowUp::Governing => {
            format!("Read the rules in {subject} that govern the folder of the evidence")
        }
        FollowUp::References => {
            format!("List callers and uses of {subject} (references) named")
        }
        FollowUp::Tests => format!("Find tests that use {subject}"),
        FollowUp::Excerpt => format!("Read the definition of {subject} in {path}"),
        FollowUp::Callers => format!("Read a use of {subject} in {path}"),
        FollowUp::Outline => format!("Outline the declarations of {path}"),
        FollowUp::Literal => format!("Search for the phrase {subject}"),
        FollowUp::Keyword => format!("Search the index for the named terms {subject}"),
    };
    format!("{words} at {path}:{start}-{end}")
}

pub(super) fn choice_id(
    kind: FollowUp,
    revision: &str,
    tool: &str,
    arguments: &Map<String, Value>,
) -> Result<Sha256Digest> {
    Ok(framed_digest(
        b"airk.arbiter.software-followup/1",
        &[
            kind.tag(),
            revision.as_bytes(),
            tool.as_bytes(),
            &serde_json::to_vec(arguments)?,
        ],
    ))
}

/// Bytes for one follow-up read: half the frame plan is left for records (the
/// presentation repeats their text), shared by the kept hits and four lookups.
#[cfg(test)]
pub(super) fn per_read_bytes(initial_bytes: usize) -> usize {
    per_read_bytes_for(initial_bytes, KEPT)
}

/// As `per_read_bytes`, for `kept` hits (change set 2's `focus_keep`).
#[cfg(test)]
pub(super) fn per_read_bytes_for(initial_bytes: usize, kept: usize) -> usize {
    per_read_bytes_with(initial_bytes, kept, READ_FLOOR)
}

/// As `per_read_bytes_for`, with the `read_floor` lower clamp (at most 8192).
pub(super) fn per_read_bytes_with(initial_bytes: usize, kept: usize, floor: usize) -> usize {
    (FRAME_PLAN / 2)
        .saturating_sub(initial_bytes)
        .checked_div(kept + 4)
        .unwrap_or(0)
        .clamp(floor.min(8192), 8192)
}

/// Initial search arguments under the wider pool, with the process's bound
/// layers (the only layers `Gathering::with_layers` receives in a service).
pub(super) fn initial_search(question: &str, budget: &Budget) -> Value {
    initial_search_for(question, budget, evaluation_layers())
}

/// Focused snippets for every pool hit; whole chunks come from the kept hits'
/// own reads. The whole pool is fetched with full chunks when it is at most 8
/// or, with `pool_full`, when `pool_full` covers it; a smaller `pool_full`
/// adds one full-chunk search of the top hits (`plan::full_chunk_search`).
pub(super) fn initial_search_for(question: &str, budget: &Budget, layers: Layers) -> Value {
    let size = layers.pool_size();
    let pool = size.min(budget.candidates).min(budget.fanout.max(size));
    // A pool wider than POOL keeps its record near 16 KiB: shorter snippets.
    let max_bytes = if pool > POOL {
        (pool * WIDE_POOL_SNIPPET_BYTES).clamp(4096, 16_384)
    } else {
        (pool * 768).clamp(4096, 16_384)
    };
    let full = layers
        .pool_full
        .map_or(pool <= 8, |full| usize::from(full) >= pool);
    json!({"query":question,"limit":pool,"max_bytes":max_bytes,"full_chunks":full})
}

const KEY_NOUN_STOP: &[&str] = &[
    "about", "after", "also", "answer", "because", "before", "being", "change", "code", "does",
    "each", "every", "file", "files", "find", "from", "have", "into", "make", "more", "must",
    "need", "only", "other", "should", "show", "some", "than", "that", "their", "them", "then",
    "there", "these", "they", "this", "those", "update", "used", "uses", "using", "what", "when",
    "where", "which", "while", "with", "without", "would", "your", "work", "works", "write",
];

/// Change set 3: up to `count` follow-up search queries for an instruction:
/// its named identifiers one by one, then its key nouns two at a time
/// (lowercase words of four or more letters outside a closed stop list).
pub(crate) fn keyword_queries(question: &str, count: usize) -> Vec<String> {
    let named = identifiers(question, count);
    let lowered: Vec<String> = named.iter().map(|name| name.to_lowercase()).collect();
    let mut nouns: Vec<String> = Vec::new();
    for word in question.split(|c: char| !c.is_ascii_alphabetic()) {
        let lower = word.to_ascii_lowercase();
        if lower.len() >= 4
            && word.chars().all(|c| c.is_ascii_lowercase())
            && !KEY_NOUN_STOP.contains(&lower.as_str())
            && !lowered.contains(&lower)
            && !nouns.contains(&lower)
        {
            nouns.push(lower);
        }
    }
    let mut queries = named;
    for pair in nouns.chunks(2) {
        queries.push(pair.join(" "));
    }
    queries.truncate(count);
    queries
}

/// Change set 3: hit indices with every file's first hit (in rank order)
/// before any file's second, so a wide pool shows more distinct files.
pub(crate) fn distinct_files_first(paths: &[&str]) -> Vec<usize> {
    let mut order = Vec::with_capacity(paths.len());
    let mut round = 0usize;
    while order.len() < paths.len() {
        for (index, path) in paths.iter().enumerate() {
            if paths[..index].iter().filter(|earlier| *earlier == path).count() == round {
                order.push(index);
            }
        }
        round += 1;
    }
    order
}

/// A bounded Focus window that fits one pair: it starts at the first line that
/// names a question identifier, ends on a char boundary, and never grows the
/// original hit.
pub(super) fn focus_window(text: &str, question: &str, pair_tokens: usize) -> (usize, usize) {
    let budget = (pair_tokens * 2)
        .saturating_sub(question.len() * 2)
        .saturating_sub(320)
        .max(256);
    let names = identifiers(question, 8);
    let mut start = 0;
    if text.len() > budget
        && let Some(found) = names.iter().filter_map(|name| text.find(name.as_str())).min()
    {
        start = text[..found].rfind('\n').map_or(0, |line| line + 1);
    }
    let bounded = |start: usize| {
        let mut end = (start + budget).min(text.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        end
    };
    let end = bounded(start);
    if end <= start {
        return (0, bounded(0));
    }
    (start, end)
}

pub(crate) mod plan;
#[cfg(test)]
mod cs3_tests;
#[cfg(test)]
mod gathering_tests;
#[cfg(test)]
mod tests;
