// SPDX-License-Identifier: Apache-2.0
use airk_core::{DocumentLocation, evidence::EvidenceReadOptions};
use airk_kernel::Kernel;
use anyhow::Result;

#[test]
fn html_children_are_mapped_but_explicit_line_reads_return_original_source() -> Result<()> {
    let root = tempfile::tempdir()?;
    let state = tempfile::tempdir()?;
    let source = "<!doctype html>\r\n<p>Original café</p>\r\n<script type=\"module\">\r\nexport function beacon() { return 'visible'; }\r\n</script>\r\n<style>\r\n.beacon { color: blue; }\r\n</style>\r\n<p>Tail</p>\r\n";
    std::fs::write(root.path().join("original.html"), source)?;
    let kernel = Kernel::open(root.path(), Some(state.path().into()))?;
    let indexed = kernel.index()?;
    assert_eq!(indexed.indexed, 1, "{:?}", indexed.skipped);
    let outline = kernel.outline("original.html", 100, 65536)?;
    let definition = outline
        .facts
        .iter()
        .find(|fact| fact.kind == "definition" && fact.name == "beacon")
        .unwrap();
    assert!(matches!(
        definition.citation.location,
        DocumentLocation::Mapped { .. }
    ));
    assert_eq!(definition.citation.start_line, 4);
    let read = kernel.read_file("original.html", 1, 9, 65536)?;
    assert_eq!(read.hits[0].text, source);
    assert_eq!(kernel.file_info("original.html")?.indexed_lines, 9);
    let context = kernel.context("original.html", 9, 0, 0, 4096)?;
    assert_eq!(context.hits[0].text, "<p>Tail</p>\r\n");
    let conditional = kernel.context_conditional(
        "original.html",
        9,
        0,
        0,
        4096,
        &EvidenceReadOptions::default(),
    )?;
    assert_eq!(
        conditional.content.unwrap().hits[0].text,
        context.hits[0].text
    );
    assert_eq!(kernel.index()?.generation, indexed.generation);
    assert_eq!(
        std::fs::read_to_string(root.path().join("original.html"))?,
        source
    );
    Ok(())
}
