// SPDX-License-Identifier: Apache-2.0
//! R Markdown fenced chunks: direct original maps only, no knitr execution.
use super::*;
use crate::{
    host_html::{HtmlDiscovery, HtmlRegion},
    host_prose::ProseDiscovery,
};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser as Markdown, Tag, TagEnd};

struct Candidate {
    whole: std::ops::Range<usize>,
    marker: char,
    width: usize,
    adapter: Option<&'static str>,
    header: String,
}
pub(crate) fn discover(
    path: &str,
    source: &str,
    budget: &mut HostBudget,
) -> Result<ProseDiscovery> {
    ensure!(
        source.len() <= crate::MAX_STRUCTURED_BYTES,
        "R Markdown source limit"
    );
    budget.charge(source.len(), 0, 0, 0)?;
    let mut found = ProseDiscovery {
        host: HtmlDiscovery {
            regions: vec![],
            facts: vec![],
            diagnostics: vec![],
            partial: false,
        },
        exclusions: vec![],
    };
    let mut facts = Facts::new(path, source, "rmd")?;
    let mut current: Option<Candidate> = None;
    for (event, range) in Markdown::new_ext(source, Options::all()).into_offset_iter() {
        budget.charge(1, 0, 0, 0)?;
        match event {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) if info.starts_with('{') => {
                budget.enter_region(1)?;
                let first = source[range.clone()]
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim_start();
                let marker = first.chars().next().unwrap_or('`');
                let width = first.chars().take_while(|c| *c == marker).count();
                let header = info.to_string();
                let engine = header
                    .strip_prefix('{')
                    .and_then(|v| v.strip_suffix('}'))
                    .and_then(|v| v.split([',', ' ', '\t']).next());
                let adapter = match engine {
                    Some("r" | "R") => Some("r"),
                    Some("python") => Some("python"),
                    Some("js" | "javascript") => Some("javascript"),
                    Some("ts" | "typescript") => Some("typescript"),
                    Some("bash" | "sh") => Some("bash"),
                    _ => None,
                };
                current = Some(Candidate {
                    whole: range,
                    marker,
                    width,
                    adapter,
                    header,
                });
            }
            Event::End(TagEnd::CodeBlock) if current.is_some() => {
                let candidate = current.take().unwrap();
                found.exclusions.push(ByteSpan {
                    start: candidate.whole.start,
                    end: candidate.whole.end,
                });
                let original = &source[candidate.whole.clone()];
                let last = original
                    .trim_end_matches(['\r', '\n'])
                    .rsplit('\n')
                    .next()
                    .unwrap_or("")
                    .trim();
                let closed = last.chars().take_while(|c| *c == candidate.marker).count()
                    >= candidate.width
                    && last.chars().all(|c| c == candidate.marker);
                // Derive raw body boundaries from the actual parsed fence's
                // original delimiters, not Markdown's newline-normalized text.
                let body_start = original
                    .find('\n')
                    .map(|offset| candidate.whole.start + offset + 1);
                let body_end = original
                    .trim_end_matches(['\r', '\n'])
                    .rfind('\n')
                    .map(|offset| candidate.whole.start + offset + 1);
                let at_line_start = candidate.whole.start == 0
                    || source.as_bytes()[candidate.whole.start - 1] == b'\n';
                let body = body_start
                    .zip(body_end)
                    .filter(|(start, end)| start <= end)
                    .map(|(start, end)| start..end);
                if !closed
                    || candidate.adapter.is_none()
                    || !at_line_start
                    || !original.starts_with(candidate.marker)
                    || body.is_none()
                {
                    found.host.partial = true;
                    if found.host.diagnostics.len() < 256 {
                        found.host.diagnostics.push(format!("R Markdown chunk at byte {} omitted: unsupported engine, missing fence or non-verbatim indentation",candidate.whole.start));
                    }
                    continue;
                }
                budget.charge(0, 1, 0, 0)?;
                facts.push(
                    candidate.whole.start,
                    candidate.whole.end,
                    "labeled_chunk",
                    "rmd_fenced_chunk",
                    &candidate.header,
                )?;
                let code = facts.facts.last_mut().unwrap().code.as_mut().unwrap();
                code.basis = EvidenceBasis::StructuralRule;
                code.producer = "airk-host-structure/rmd".into();
                code.producer_version = "general-b-facts/1".into();
                code.coverage="Written fenced chunk label/options, not evaluated; only original verbatim source is parsed".into();
                if let Some(body) = body
                    && !body.is_empty()
                {
                    budget.charge(0, 0, body.end - body.start, 1)?;
                    found.host.regions.push(HtmlRegion {
                        adapter_id: candidate.adapter.unwrap(),
                        span: ByteSpan {
                            start: body.start,
                            end: body.end,
                        },
                        kind: "fenced_chunk",
                    });
                }
            }
            _ => {}
        }
    }
    found.host.facts = facts.facts;
    Ok(found)
}
