// SPDX-License-Identifier: Apache-2.0
//! Original Bird/LaTeX literate spans; no generated code or synthetic separators.
use super::*;
use airk_core::location::{MapSegment, Mapping, SOURCE_MAP_VERSION, SourceMap};
pub(crate) struct LiterateRegion {
    pub text: String,
    pub source_map: SourceMap,
    pub original: ByteSpan,
    pub kind: &'static str,
}
pub(crate) struct LiterateDiscovery {
    pub regions: Vec<LiterateRegion>,
    pub exclusions: Vec<ByteSpan>,
    pub diagnostics: Vec<String>,
    pub partial: bool,
}
pub(crate) fn discover(
    _path: &str,
    source: &str,
    budget: &mut HostBudget,
) -> Result<LiterateDiscovery> {
    ensure!(
        source.len() <= crate::MAX_FILE_BYTES,
        "literate source byte limit"
    );
    let mut result = LiterateDiscovery {
        regions: vec![],
        exclusions: vec![],
        diagnostics: vec![],
        partial: false,
    };
    let mut bird: Option<LiterateRegion> = None;
    let mut latex: Option<(usize, usize)> = None;
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        budget.charge(1, 0, 0, 0)?;
        let end = offset + line.len();
        let bare = line.trim_end_matches(['\r', '\n']);
        if let Some((start, body)) = latex {
            if bare == "\\end{code}" {
                let content = &source[body..offset];
                result.exclusions.push(ByteSpan { start, end });
                if content.trim().is_empty() {
                    result.partial = true;
                } else {
                    budget.charge(0, 0, content.len(), 1)?;
                    result.regions.push(LiterateRegion {
                        text: content.into(),
                        source_map: SourceMap {
                            version: SOURCE_MAP_VERSION,
                            segments: vec![MapSegment {
                                local: ByteSpan {
                                    start: 0,
                                    end: content.len(),
                                },
                                mapping: Mapping::Verbatim {
                                    host: ByteSpan {
                                        start: body,
                                        end: offset,
                                    },
                                },
                            }],
                        },
                        original: ByteSpan { start, end },
                        kind: "latex-code",
                    });
                }
                latex = None;
            }
        } else if line.starts_with('>') {
            if bird.is_none() {
                budget.enter_region(1)?;
                bird = Some(LiterateRegion {
                    text: String::new(),
                    source_map: SourceMap {
                        version: SOURCE_MAP_VERSION,
                        segments: vec![],
                    },
                    original: ByteSpan { start: offset, end },
                    kind: "bird-code",
                });
            }
            let prefix = if line.starts_with("> ") { 2 } else { 1 };
            let content = &line[prefix..];
            let region = bird.as_mut().unwrap();
            budget.charge(0, 0, content.len(), usize::from(!content.is_empty()))?;
            let local = region.text.len();
            region.text.push_str(content);
            if !content.is_empty() {
                region.source_map.segments.push(MapSegment {
                    local: ByteSpan {
                        start: local,
                        end: local + content.len(),
                    },
                    mapping: Mapping::Verbatim {
                        host: ByteSpan {
                            start: offset + prefix,
                            end,
                        },
                    },
                });
            }
            region.original.end = end;
        } else {
            flush(&mut bird, &mut result);
            if bare == "\\begin{code}" {
                budget.enter_region(1)?;
                latex = Some((offset, end));
            }
        }
        offset = end;
    }
    flush(&mut bird, &mut result);
    if let Some((start, _)) = latex {
        result.exclusions.push(ByteSpan {
            start,
            end: source.len(),
        });
        result.partial = true;
        result
            .diagnostics
            .push("Unterminated literate code environment omitted".into());
    }
    Ok(result)
}
fn flush(bird: &mut Option<LiterateRegion>, result: &mut LiterateDiscovery) {
    if let Some(region) = bird.take() {
        result.exclusions.push(region.original);
        if region.text.trim().is_empty() {
            result.partial = true;
        } else {
            result.regions.push(region);
        }
    }
}
