// SPDX-License-Identifier: Apache-2.0
//! Bounded nomination from cited search results. A nomination can choose a
//! canonical read, but only the resulting source excerpt can support a claim.
use super::gather::{Record, Step};
use super::retrieve::{Need, NeedKind};
use airk_core::identity::Sha256Digest;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

const LIMIT: usize = 4;
const MAX_BYTES: usize = 4096;
const MAX_LOCATIONS: usize = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Read {
    Literal {
        literal: String,
        prefix: String,
    },
    Canonical {
        tool: &'static str,
        arguments: Map<String, Value>,
    },
}

pub(super) struct Probe {
    pub(super) part: u8,
    pub(super) need: u8,
    pub(super) basis: Sha256Digest,
    pub(super) read: Read,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Location {
    pub(super) path: String,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) known_hash: Sha256Digest,
}

fn canonical(tool: &'static str, arguments: Value) -> Read {
    Read::Canonical {
        tool,
        arguments: arguments.as_object().expect("nomination arguments").clone(),
    }
}

fn nearby(path: &str) -> String {
    path.rsplit_once('/')
        .map_or_else(|| path.to_owned(), |(parent, _)| format!("{parent}/"))
}

fn checked_location(path: &str, start: usize, end: usize, hash: &str) -> Option<Location> {
    if !crate::workbench::safe_path(path)
        || airk_core::index_catalog::relative_path(path, false).is_err()
        || start == 0
        || end < start
        || end > u32::MAX as usize
    {
        return None;
    }
    Some(Location {
        path: path.to_owned(),
        start,
        end,
        known_hash: hash.parse().ok()?,
    })
}

fn source_location(citation: &Value) -> Option<Location> {
    checked_location(
        citation.get("path")?.as_str()?,
        usize::try_from(citation.get("start_line")?.as_u64()?).ok()?,
        usize::try_from(citation.get("end_line")?.as_u64()?).ok()?,
        citation.get("content_hash")?.as_str()?,
    )
}

fn value_rows(rows: Option<&Value>) -> &[Value] {
    rows.and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

fn facts(body: &Value) -> &[Value] {
    value_rows(body.get("items").or_else(|| body.get("facts")))
}

/// These are navigation coordinates only. The selected reader must still
/// return text at the same publication, source hash and original location.
pub(super) fn locations(tool: &str, body: &Value) -> Vec<Location> {
    // Suggested relationship targets are a different result, not edges.
    let rows = match tool {
        "code_facts" | "document_facts" => facts(body),
        "relationships" => value_rows(body.get("relationships")),
        "literal_scan" => value_rows(body.get("matches")),
        _ => return Vec::new(),
    };
    let mut seen = BTreeSet::new();
    rows.iter()
        .filter_map(|row| source_location(row.get("citation")?))
        .filter(|location| seen.insert(location.clone()))
        .take(MAX_LOCATIONS)
        .collect()
}

fn exact_identifier(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 128
        && text.split("::").all(|part| {
            part.as_bytes()
                .first()
                .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
}

/// The planner supplies a subject only when the task gives it an anchor. A
/// returned owner grounds the search scope; no useful hit means no probe.
pub(super) fn from_record(need: &Need, step: Step, record: &Record) -> Vec<Read> {
    if record.failure || record.tool != "search" {
        return Vec::new();
    }
    let hits = super::coverage::record_hits(record);
    let mut locations = hits.iter().filter_map(|(_, hit)| {
        let citation = &hit.citation;
        checked_location(
            &citation.path,
            citation.start_line,
            citation.end_line,
            &citation.content_hash,
        )
    });
    let subject = need.subject.as_deref();
    let next = match (step, need.kind) {
        (Step::Literal, NeedKind::Errors) => {
            let Some(literal) = subject.filter(|literal| {
                !literal.trim().is_empty()
                    && literal.len() <= 200
                    && !literal.chars().any(char::is_control)
            }) else {
                return Vec::new();
            };
            locations
                .clone()
                .find(|location| !super::software::is_document_path(&location.path))
                .or_else(|| locations.next())
                .map(|location| Read::Literal {
                    literal: literal.to_owned(),
                    prefix: nearby(&location.path),
                })
        }
        (Step::Search, NeedKind::Definition | NeedKind::Callers)
        | (Step::Imports, NeedKind::Interfaces) => {
            let Some(name) = subject.filter(|name| exact_identifier(name)) else {
                return Vec::new();
            };
            locations
                .find(|location| !super::software::is_document_path(&location.path))
                .map(|location| {
                    canonical(
                        "code_facts",
                        json!({"name":name,"path_prefix":nearby(&location.path),
                               "compact":false,"limit":LIMIT,"max_bytes":MAX_BYTES}),
                    )
                })
        }
        (Step::Search, NeedKind::Documentation | NeedKind::Contract) => locations
            .find(|location| super::software::is_document_path(&location.path))
            .map(|location| {
                let mut arguments = json!({"path":location.path,"compact":false,
                                           "limit":LIMIT,"max_bytes":MAX_BYTES});
                if let Some(name) = subject.filter(|name| exact_identifier(name)) {
                    arguments["name"] = json!(name);
                }
                canonical("document_facts", arguments)
            }),
        _ => None,
    };
    next.into_iter().collect()
}

/// One hop from an actual cited definition. Several different returned
/// definitions leave a choice for the surrounding investigation.
pub(super) fn from_facts(need: &Need, body: &Value) -> Vec<Read> {
    let direction = match need.kind {
        NeedKind::Callers => "incoming",
        NeedKind::Interfaces => "outgoing",
        _ => return Vec::new(),
    };
    let Some(name) = need
        .subject
        .as_deref()
        .filter(|name| exact_identifier(name))
    else {
        return Vec::new();
    };
    let mut identities = facts(body).iter().filter_map(|fact| {
        let code = fact.get("code")?;
        if fact.get("kind")?.as_str()? != "definition"
            || (fact.get("name").and_then(Value::as_str) != Some(name)
                && code.get("qualified_name").and_then(Value::as_str) != Some(name))
        {
            return None;
        }
        let symbol = code.get("symbol_id")?.as_str()?;
        if symbol.is_empty()
            || symbol.len() > 4096
            || symbol
                .chars()
                .any(|ch| ch.is_whitespace() || ch.is_control())
        {
            return None;
        }
        let location = source_location(fact.get("citation")?)?;
        Some((symbol, nearby(&location.path)))
    });
    let Some((symbol, prefix)) = identities.next() else {
        return Vec::new();
    };
    if identities
        .any(|(other_symbol, other_prefix)| other_symbol != symbol || other_prefix != prefix)
    {
        return Vec::new();
    }
    vec![canonical(
        "relationships",
        json!({"symbol":symbol,"direction":direction,"path_prefix":prefix,
               "hops":1,"limit":LIMIT,"max_bytes":MAX_BYTES}),
    )]
}

#[cfg(test)]
mod tests;
