// SPDX-License-Identifier: Apache-2.0
//! Recorded pre-split `airk` outputs for the parity tests (split U95).
//!
//! Before the user CLI is deleted, each parity test runs once with
//! `AIRK_PARITY_CAPTURE=1`: every old-side invocation runs and its output is
//! stored in canonical space in `fixtures/split/parity/<test>.json`. Without
//! that variable the old side never runs and `Parity::old` returns the stored
//! record. Old and new are compared in canonical space: `canonical(new)`
//! against the canonical old record.
//!
//! Canonical space (`Parity::canonical`): masks (`mask_json_fields`,
//! `mask_text`) replace values that differ between runs (clocks, process ids,
//! secrets) by `"<MASKED>"`; substitutions (`substitute`, `substitute_path`,
//! `substitute_identifiers`, `service`) replace this run's registered values
//! (paths, bindings, connection ids) by stable placeholders, also in their
//! JSON-escaped and slash forms; then every 32- or 64-digit hex token is
//! renamed `<H{n}>` by first occurrence within that one text, so identity
//! equality inside a text is kept while fresh identifiers compare equal.
// Strict new transport comparisons live separately. The original U95 v1
// captures retain their approved, explicitly limited legacy comparison.
pub mod strict;

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Display,
    path::{Path, PathBuf},
    process::{ExitStatus, Output},
};

const SCHEMA: &str = "airk.split.parity/1";
const NOTE: &str = "U95: pre-split airk outputs captured before the user CLI was deleted; canonical form: masked, run values substituted by placeholders, 32/64-digit hex tokens renamed <H{n}> per text";
const MASK: &str = "<MASKED>";
const CAPTURE: &str = "AIRK_PARITY_CAPTURE";
/// Replay that applies the test's current masks to the stored cases and
/// rewrites the fixture: the only way to add a mask once the pre-split CLI is
/// gone. It never records new output; the old side still does not run.
const REMASK: &str = "AIRK_PARITY_REMASK";

fn remasking() -> bool {
    std::env::var(REMASK).is_ok_and(|value| value == "1")
}

/// Clock and duration fields: every run observes its own values.
pub const CLOCK_FIELDS: &[&str] = &[
    "*_unix_ms",
    "*_unix_secs",
    "uptime_ms",
    "elapsed_ms",
    "cache_age_ms",
];

/// One recorded command run. Fields are declared in key order so the
/// fixture's keys are sorted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recorded {
    pub code: Option<i32>,
    pub stderr: String,
    pub stdout: String,
    pub success: bool,
}

impl Recorded {
    fn from_output(output: &Output) -> Self {
        Self {
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            success: output.status.success(),
        }
    }
    fn map(&self, mut change: impl FnMut(&str) -> String) -> Self {
        Self {
            code: self.code,
            stderr: change(&self.stderr),
            stdout: change(&self.stdout),
            success: self.success,
        }
    }
    /// The record as a `std::process::Output`, for comparison helpers that
    /// take one.
    pub fn output(&self) -> Output {
        Output {
            status: exit_status(self.code, self.success),
            stdout: self.stdout.clone().into_bytes(),
            stderr: self.stderr.clone().into_bytes(),
        }
    }
}

fn exit_status(code: Option<i32>, success: bool) -> ExitStatus {
    let code = code.unwrap_or(if success { 0 } else { 1 });
    #[cfg(windows)]
    {
        use std::os::windows::process::ExitStatusExt;
        ExitStatus::from_raw(code as u32)
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw((code & 0xff) << 8)
    }
}

/// The checked-in fixture. Fields are declared in key order.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    cases: BTreeMap<String, Recorded>,
    masked: BTreeSet<String>,
    note: String,
    placeholders: BTreeSet<String>,
    schema: String,
    test: String,
}

pub struct Parity {
    test: &'static str,
    cases: BTreeMap<String, Recorded>,
    capture: bool,
    /// (value, placeholder) pairs used to normalize, one per distinct value.
    substitutions: Vec<(String, String)>,
    /// Base placeholders as registered, for conflict checks.
    bases: BTreeMap<String, String>,
    /// Placeholders the stored cases contain (a review aid in the fixture).
    placeholders: BTreeSet<String>,
    masked: BTreeSet<String>,
    recorded_masks: BTreeSet<String>,
    text_masks: Vec<fn(&str) -> String>,
    used: BTreeSet<String>,
}

fn fixture_path(test: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/split/parity")
        .join(format!("{test}.json"))
}

fn json_escaped(text: &str) -> String {
    let quoted = serde_json::to_string(text).expect("string serializes");
    quoted[1..quoted.len() - 1].to_owned()
}

fn is_identifier(text: &str) -> bool {
    text.len() >= 32 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Every maximal run of exactly 32 or 64 ASCII hex digits renamed `<H{n}>`,
/// numbered by first occurrence in `text`.
fn legacy_v1_rename_hex(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut names: BTreeMap<&str, usize> = BTreeMap::new();
    let mut out = String::with_capacity(text.len());
    let (mut at, mut copied) = (0, 0);
    while at < bytes.len() {
        if !bytes[at].is_ascii_hexdigit() {
            at += 1;
            continue;
        }
        let start = at;
        while at < bytes.len() && bytes[at].is_ascii_hexdigit() {
            at += 1;
        }
        let run = &text[start..at];
        if run.len() == 32 || run.len() == 64 {
            let next = names.len() + 1;
            let number = *names.entry(run).or_insert(next);
            out.push_str(&text[copied..start]);
            out.push_str(&format!("<H{number}>"));
            copied = at;
        }
    }
    out.push_str(&text[copied..]);
    out
}

/// A loopback URL carries this run's port.
fn is_loopback_url(text: &str) -> bool {
    [
        "http://127.0.0.1:",
        "https://127.0.0.1:",
        "http://localhost:",
        "https://localhost:",
    ]
    .iter()
    .any(|prefix| text.starts_with(prefix))
}

/// One left-to-right pass: at each position the longest matching key wins,
/// so a replacement is never matched again.
fn replace_all(text: &str, pairs: &[(String, String)], used: &mut BTreeSet<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    'scan: while at < text.len() {
        let rest = &text[at..];
        for (from, to) in pairs {
            if rest.starts_with(from.as_str()) {
                out.push_str(to);
                used.insert(to.clone());
                at += from.len();
                continue 'scan;
            }
        }
        let next = rest.chars().next().expect("non-empty rest");
        out.push(next);
        at += next.len_utf8();
    }
    out
}

fn mask_matches(key: &str, patterns: &BTreeSet<String>) -> bool {
    patterns
        .iter()
        .any(|pattern| match pattern.strip_prefix('*') {
            Some(suffix) => key.ends_with(suffix),
            None => key == pattern,
        })
}

/// The length of a scalar JSON value at the start of `value` (a string at
/// this escape level, a number, `true`, `false` or `null`). Objects and
/// arrays are not masked.
fn value_length(value: &str, quote: &str) -> Option<usize> {
    if let Some(inner) = value.strip_prefix(quote) {
        if quote == "\"" {
            let mut escaped = false;
            for (offset, character) in inner.char_indices() {
                match (escaped, character) {
                    (true, _) => escaped = false,
                    (false, '\\') => escaped = true,
                    (false, '"') => return Some(1 + offset + 1),
                    _ => {}
                }
            }
            return None;
        }
        return inner.find(quote).map(|end| quote.len() + end + quote.len());
    }
    let length = value
        .bytes()
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
        .count();
    (length > 0).then_some(length)
}

/// Replace the values of the matching object fields, at one JSON escape
/// level (`quote` is `"`, `\"` or `\\\"`), in any text.
fn mask_level(text: &str, quote: &str, patterns: &BTreeSet<String>) -> String {
    let replacement = format!("{quote}{MASK}{quote}");
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(offset) = text[at..].find(quote) {
        let start = at + offset;
        let key_start = start + quote.len();
        let key_length = text[key_start..]
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            .count();
        let key_end = key_start + key_length;
        if key_length == 0 || !text[key_end..].starts_with(quote) {
            out.push_str(&text[at..key_start]);
            at = key_start;
            continue;
        }
        let after_key = key_end + quote.len();
        if mask_matches(&text[key_start..key_end], patterns)
            && let Some(value) = text[after_key..]
                .trim_start_matches([' ', '\t'])
                .strip_prefix(':')
        {
            let value = value.trim_start_matches([' ', '\t']);
            let value_start = text.len() - value.len();
            if let Some(length) = value_length(value, quote) {
                out.push_str(&text[at..value_start]);
                out.push_str(&replacement);
                at = value_start + length;
                continue;
            }
        }
        out.push_str(&text[at..after_key]);
        at = after_key;
    }
    out.push_str(&text[at..]);
    out
}

impl Parity {
    /// Capture when `AIRK_PARITY_CAPTURE=1`; otherwise load the fixture.
    pub fn new(test: &'static str) -> Self {
        let capture = std::env::var(CAPTURE).is_ok_and(|value| value == "1");
        let mut parity = Self {
            test,
            cases: BTreeMap::new(),
            capture,
            substitutions: Vec::new(),
            bases: BTreeMap::new(),
            placeholders: BTreeSet::new(),
            masked: BTreeSet::new(),
            recorded_masks: BTreeSet::new(),
            text_masks: Vec::new(),
            used: BTreeSet::new(),
        };
        if !capture {
            let path = fixture_path(test);
            let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
                panic!(
                    "parity fixture {} is missing ({error}); capture it with {CAPTURE}=1 while the pre-split airk CLI exists",
                    path.display()
                )
            });
            let fixture: Fixture = serde_json::from_str(&text)
                .unwrap_or_else(|error| panic!("parity fixture {}: {error}", path.display()));
            assert_eq!(fixture.schema, SCHEMA, "{}", path.display());
            assert_eq!(fixture.test, test, "{}", path.display());
            parity.cases = fixture.cases;
            parity.placeholders = fixture.placeholders;
            parity.recorded_masks = fixture.masked;
        }
        // Machine-specific values every test may print: the user's home and
        // temporary folders and the package version.
        for (variable, placeholder) in [("USERPROFILE", "<HOME>"), ("HOME", "<HOME>")] {
            if !parity.bases.contains_key(placeholder)
                && let Some(home) = std::env::var_os(variable).filter(|home| !home.is_empty())
            {
                parity.substitute_path(Path::new(&home), placeholder);
            }
        }
        parity.substitute_path(&std::env::temp_dir(), "<TMP>");
        parity.substitute(env!("CARGO_PKG_VERSION"), "<VERSION>");
        parity
    }

    /// True while recording the old side (`AIRK_PARITY_CAPTURE=1`).
    pub fn capturing(&self) -> bool {
        self.capture
    }

    fn register(&mut self, value: String, placeholder: String) {
        // Short values would match unrelated text.
        if value.len() < 5
            || self
                .substitutions
                .iter()
                .any(|(existing, _)| *existing == value)
        {
            return;
        }
        self.substitutions.push((value, placeholder));
        self.substitutions
            .sort_by(|left, right| right.0.len().cmp(&left.0.len()).then(left.cmp(right)));
    }

    /// Register a run-specific value under a placeholder such as `<CONTROL>`,
    /// with its JSON-escaped and slash variants.
    pub fn substitute(&mut self, value: impl Display, placeholder: &str) {
        assert!(
            placeholder.len() > 2
                && placeholder.starts_with('<')
                && placeholder.ends_with('>')
                && !placeholder[1..placeholder.len() - 1].contains(['<', '>', ':'])
                && placeholder != MASK,
            "invalid parity placeholder {placeholder}"
        );
        let value = value.to_string();
        if value.is_empty() {
            return;
        }
        match self.bases.get(placeholder) {
            Some(existing) if *existing == value => return,
            Some(existing) => {
                panic!("parity placeholder {placeholder} registered for {existing:?} and {value:?}")
            }
            None => {}
        }
        self.bases.insert(placeholder.to_owned(), value.clone());
        let stem = &placeholder[..placeholder.len() - 1];
        for (form, text) in [
            ("", value.clone()),
            (":slash", value.replace('\\', "/")),
            (":backslash", value.replace('/', "\\")),
        ] {
            let once = json_escaped(&text);
            let twice = json_escaped(&once);
            for (level, text) in [("", text.clone()), (":json", once), (":json2", twice)] {
                self.register(text, format!("{stem}{form}{level}>"));
            }
        }
    }

    /// A path, and also its canonical form (`<NAME_CANONICAL>`) and that
    /// form without a Windows verbatim prefix (`<NAME_CANONICAL_PLAIN>`).
    pub fn substitute_path(&mut self, path: &Path, placeholder: &str) {
        self.substitute(path.display(), placeholder);
        let stem = &placeholder[..placeholder.len() - 1];
        let canonical = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .display()
            .to_string();
        let plain = canonical
            .strip_prefix(r"\\?\")
            .unwrap_or(&canonical)
            .to_owned();
        self.substitute(canonical, &format!("{stem}_CANONICAL>"));
        self.substitute(plain, &format!("{stem}_CANONICAL_PLAIN>"));
    }

    /// Every hex identifier or digest (32+ hex digits) and loopback URL among
    /// the string leaves of `value`, under `<PREFIX.path.to.leaf>`.
    pub fn substitute_identifiers(&mut self, value: &impl Serialize, prefix: &str) {
        fn walk(value: &serde_json::Value, path: String, found: &mut Vec<(String, String)>) {
            match value {
                serde_json::Value::String(text) if is_identifier(text) => {
                    found.push((path, text.clone()));
                }
                // Only the origin: the port is this run's, the path is not.
                serde_json::Value::String(text) if is_loopback_url(text) => {
                    let host = text.find("://").map_or(0, |at| at + 3);
                    let end = text[host..].find('/').map_or(text.len(), |at| host + at);
                    found.push((path, text[..end].to_owned()));
                }
                serde_json::Value::Object(fields) => {
                    for (key, field) in fields {
                        let key: String = key
                            .chars()
                            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                            .collect();
                        walk(field, format!("{path}.{key}"), found);
                    }
                }
                serde_json::Value::Array(items) => {
                    for (index, item) in items.iter().enumerate() {
                        walk(item, format!("{path}.{index}"), found);
                    }
                }
                _ => {}
            }
        }
        let value = serde_json::to_value(value).expect("identifier source serializes");
        let mut found = Vec::new();
        walk(&value, prefix.to_owned(), &mut found);
        for (path, text) in found {
            self.substitute(text, &format!("<{path}>"));
        }
    }

    /// A fixture service's run-specific values: its owner, control root,
    /// project paths and binding, and, when reviewed, its installation,
    /// executable and scoped selection identifiers.
    pub fn service(&mut self, service: &super::Service) {
        self.substitute_path(service.owner.path(), "<OWNER>");
        self.substitute_path(&service.control, "<CONTROL>");
        self.substitute_path(Path::new(&service.project.root), "<PROJECT_ROOT>");
        self.substitute_path(Path::new(&service.project.data_root), "<PROJECT_DATA>");
        self.substitute(service.project.binding, "<BINDING>");
        if let Some(managed) = &service.reviewed {
            self.substitute_path(&managed.install, "<INSTALL>");
            self.substitute_path(&managed.executable, "<EXECUTABLE>");
            self.substitute_identifiers(&managed.selection, "SELECTION");
        }
    }

    /// Mask these object fields' scalar values (`*suffix` matches any key
    /// ending in `suffix`), at any JSON escape level, on both sides.
    pub fn mask_json_fields(&mut self, fields: &[&str]) {
        for field in fields {
            self.mask_name(field.to_string());
        }
    }

    /// Mask with a text function (`name` is recorded in the fixture), on both
    /// sides; for values that are not JSON fields.
    pub fn mask_text(&mut self, name: &str, mask: fn(&str) -> String) {
        self.mask_name(format!("fn:{name}"));
        self.text_masks.push(mask);
    }

    fn mask_name(&mut self, name: String) {
        assert!(
            self.capture || remasking() || self.recorded_masks.contains(&name),
            "{}: mask {name} is newer than the fixture; add it with {REMASK}=1",
            self.test
        );
        self.masked.insert(name);
    }

    fn mask(&self, text: &str) -> String {
        let patterns: BTreeSet<String> = self
            .masked
            .iter()
            .filter(|name| !name.starts_with("fn:"))
            .cloned()
            .collect();
        let mut text = text.to_owned();
        if !patterns.is_empty() {
            for quote in ["\"", "\\\"", "\\\\\\\""] {
                text = mask_level(&text, quote, &patterns);
            }
        }
        for mask in &self.text_masks {
            text = mask(&text);
        }
        text
    }

    /// `text` in canonical space: masked, this run's registered values
    /// replaced by their placeholders, then every 32- or 64-digit hex token
    /// renamed `<H{n}>` by first occurrence (identical tokens share a
    /// number). Compare the new side's text through this against `old`.
    pub fn canonical(&self, text: &str) -> String {
        self.canonical_with(text, &mut BTreeSet::new())
    }

    fn canonical_with(&self, text: &str, used: &mut BTreeSet<String>) -> String {
        legacy_v1_rename_hex(&replace_all(&self.mask(text), &self.substitutions, used))
    }

    /// `text` (a case name) in canonical space.
    pub fn name(&self, text: &str) -> String {
        self.canonical(text)
    }

    /// The old side's output for `case` in canonical space: run and recorded
    /// while capturing, otherwise read from the fixture without running
    /// `run`. Both modes return the same canonical record.
    pub fn old(&mut self, case: &str, run: impl FnOnce() -> Output) -> Recorded {
        assert!(
            self.used.insert(case.to_owned()),
            "{}: parity case {case:?} used twice",
            self.test
        );
        if self.capture {
            let mut used = BTreeSet::new();
            let stored =
                Recorded::from_output(&run()).map(|text| self.canonical_with(text, &mut used));
            assert_eq!(
                stored.map(|text| self.canonical(text)),
                stored,
                "{}: case {case:?} is not stable in canonical space",
                self.test
            );
            for text in [&stored.stdout, &stored.stderr] {
                assert_eq!(
                    legacy_v1_rename_hex(text),
                    *text,
                    "{}: case {case:?} keeps a 32/64-digit hex token",
                    self.test
                );
            }
            self.placeholders.extend(used);
            self.cases.insert(case.to_owned(), stored.clone());
            stored
        } else {
            let stored = self
                .cases
                .get(case)
                .unwrap_or_else(|| {
                    panic!(
                        "{}: no recorded case {case:?}; recapture with {CAPTURE}=1",
                        self.test
                    )
                })
                .clone();
            if remasking() {
                let masked = stored.map(|text| self.mask(text));
                self.cases.insert(case.to_owned(), masked.clone());
                return masked;
            }
            stored
        }
    }

    /// A file the old side wrote, in canonical space (its text as stdout;
    /// `success` when it exists). Nothing is written back: a canonical file
    /// is not a valid input for a later step.
    pub fn old_file(&mut self, case: &str, path: &Path) -> Recorded {
        self.old(case, || match std::fs::read(path) {
            Ok(bytes) => Output {
                status: exit_status(Some(0), true),
                stdout: bytes,
                stderr: Vec::new(),
            },
            Err(_) => Output {
                status: exit_status(Some(1), false),
                stdout: Vec::new(),
                stderr: Vec::new(),
            },
        })
    }

    /// Capturing: write the fixture. Otherwise: every stored case was used
    /// and the masks match the fixture's.
    pub fn finish(self) {
        let path = fixture_path(self.test);
        if self.capture || remasking() {
            let fixture = Fixture {
                cases: self.cases,
                masked: self.masked,
                note: NOTE.to_owned(),
                placeholders: self.placeholders,
                schema: SCHEMA.to_owned(),
                test: self.test.to_owned(),
            };
            std::fs::create_dir_all(path.parent().expect("fixture folder"))
                .expect("create parity fixture folder");
            let mut text = serde_json::to_string_pretty(&fixture).expect("fixture serializes");
            text.push('\n');
            std::fs::write(&path, text)
                .unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
            return;
        }
        let stale: Vec<_> = self
            .cases
            .keys()
            .filter(|case| !self.used.contains(*case))
            .collect();
        assert!(
            stale.is_empty(),
            "{}: stale recorded cases {stale:?}; recapture with {CAPTURE}=1",
            self.test
        );
        assert_eq!(
            self.masked, self.recorded_masks,
            "{}: masks differ from the fixture's",
            self.test
        );
    }
}
