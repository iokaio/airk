// SPDX-License-Identifier: Apache-2.0
//! Local PDF extraction in a resource-limited copy of the AIRK executable.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read, Write},
};

const WORKER: &str = "__airk_pdf_worker";
const MAX_OUTPUT: usize = 64 * 1024 * 1024;
const LIMITS: crate::provider_process::ProcessLimits = crate::provider_process::ProcessLimits {
    memory_bytes: 512 * 1024 * 1024,
    cpu_seconds: 10,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    pages: Option<Vec<String>>,
    error: Option<String>,
}

/// Call before starting the host runtime or interpreting normal command-line arguments.
/// Library hosts that extract PDFs must dispatch this private worker invocation too.
pub fn dispatch() -> Result<bool> {
    if std::env::args_os().nth(1).is_some_and(|arg| arg == WORKER) {
        worker()?;
        return Ok(true);
    }
    Ok(false)
}

pub(crate) fn extract(bytes: &[u8]) -> Result<Vec<String>> {
    extract_inner(bytes).map_err(|error| {
        if error.chain().any(|cause| {
            cause.downcast_ref::<std::io::Error>().is_some_and(|io| {
                matches!(
                    io.kind(),
                    std::io::ErrorKind::StorageFull | std::io::ErrorKind::OutOfMemory
                )
            })
        }) {
            error.context(CapacityFailure)
        } else {
            error
        }
    })
}

#[derive(Debug)]
pub(crate) struct CapacityFailure;
impl std::fmt::Display for CapacityFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .write_str("PDF worker infrastructure capacity exhausted; previous generation retained")
    }
}
impl std::error::Error for CapacityFailure {}

fn extract_inner(bytes: &[u8]) -> Result<Vec<String>> {
    ensure!(
        bytes.len() <= crate::MAX_FILE_BYTES,
        "PDF exceeds source byte limit"
    );
    ensure!(bytes.starts_with(b"%PDF-"), "PDF header missing");
    let directory = tempfile::tempdir().context("create private PDF extraction directory")?;
    std::fs::write(directory.path().join("input.pdf"), bytes)?;
    #[cfg(not(test))]
    let args = vec![WORKER.to_owned()];
    #[cfg(test)]
    let args = vec![
        "--ignored".into(),
        "--exact".into(),
        "pdf_worker::tests::worker_fixture".into(),
    ];
    let spec = crate::provider_process::ProviderCommand {
        program: std::env::current_exe()?,
        args,
        timeout_ms: 15_000,
    };
    crate::provider_process::run_limited(&spec, directory.path(), &[], 65_536, Some(LIMITS))
        .context("isolated PDF extraction failed or exceeded its resource budget")?;
    let output = crate::provider_process::read_capped(
        File::open(directory.path().join("output.json"))?,
        MAX_OUTPUT,
    )?;
    let reply: Reply = serde_json::from_slice(&output).context("invalid PDF worker reply")?;
    if let Some(error) = reply.error {
        anyhow::bail!("PDF extraction refused: {error}");
    }
    let pages = reply.pages.context("PDF worker omitted pages")?;
    validate_pages(&pages)?;
    Ok(pages)
}

fn validate_pages(pages: &[String]) -> Result<()> {
    ensure!(pages.len() <= 512, "PDF exceeds 512 pages");
    ensure!(
        pages.iter().map(String::len).sum::<usize>() <= crate::documents::MAX_EXTRACTED,
        "PDF exceeds 32 MiB extracted text"
    );
    Ok(())
}

struct BoundedText {
    bytes: Vec<u8>,
    remaining: usize,
}
impl Write for BoundedText {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.remaining {
            return Err(std::io::Error::other("PDF extracted text limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        self.remaining -= bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn parse(bytes: &[u8]) -> Result<Vec<String>> {
    let mut document = pdf_extract::Document::load_mem(bytes).context("invalid PDF document")?;
    if document.is_encrypted() {
        document.decrypt("").context("PDF requires a password")?;
    }
    preflight_page_tree(&document)?;
    let page_ids = document.get_pages();
    // Page-tree parsing is itself inside the memory/time boundary. Reject before
    // any page content stream is decompressed or rendered.
    ensure!(page_ids.len() <= 512, "PDF exceeds 512 pages");
    let mut pages = Vec::new();
    let mut remaining = crate::documents::MAX_EXTRACTED;
    for page in page_ids.keys() {
        let mut text = BoundedText {
            bytes: Vec::new(),
            remaining,
        };
        let writer: &mut dyn Write = &mut text;
        pdf_extract::output_doc_page(
            &document,
            &mut pdf_extract::PlainTextOutput::new(writer),
            *page,
        )
        .context("PDF page extraction failed")?;
        remaining = text.remaining;
        pages.push(String::from_utf8(text.bytes)?);
    }
    Ok(pages)
}

fn preflight_page_tree(document: &pdf_extract::Document) -> Result<()> {
    let root = document.catalog()?.get(b"Pages")?.as_reference()?;
    let mut pending = vec![(root, 0)];
    let mut seen = std::collections::BTreeSet::new();
    let mut pages = 0;
    while let Some((id, depth)) = pending.pop() {
        ensure!(depth <= 64, "PDF page tree exceeds 64 levels");
        ensure!(
            seen.insert(id),
            "PDF page tree contains a cycle or repeated node"
        );
        ensure!(seen.len() <= 4096, "PDF page tree exceeds 4096 nodes");
        let dictionary = document.get_dictionary(id)?;
        match dictionary.get_type()? {
            b"Pages" => {
                let kids = dictionary.get_deref(b"Kids", document)?.as_array()?;
                ensure!(
                    pending.len() + seen.len() + kids.len() <= 4096,
                    "PDF page tree exceeds 4096 nodes"
                );
                for child in kids {
                    pending.push((child.as_reference()?, depth + 1));
                }
            }
            b"Page" => {
                pages += 1;
                ensure!(pages <= 512, "PDF exceeds 512 pages");
                // Rendering follows inherited resources through Parent, independently
                // from Kids. Validate that chain before the extractor recurses into it.
                let mut parents = std::collections::BTreeSet::new();
                let mut ancestor = dictionary;
                while let Ok(parent) = ancestor.get(b"Parent") {
                    let parent = parent.as_reference()?;
                    ensure!(
                        parents.insert(parent) && parents.len() <= 64,
                        "PDF page parent chain is cyclic or exceeds 64 levels"
                    );
                    ancestor = document.get_dictionary(parent)?;
                }
            }
            _ => anyhow::bail!("PDF page tree has an unsupported node"),
        }
    }
    Ok(())
}

fn worker() -> Result<()> {
    let mut bytes = Vec::new();
    File::open("input.pdf")?
        .take((crate::MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= crate::MAX_FILE_BYTES,
        "PDF worker source byte limit exceeded"
    );
    let reply = match parse(&bytes) {
        Ok(pages) => Reply {
            pages: Some(pages),
            error: None,
        },
        Err(error) => Reply {
            pages: None,
            error: Some(format!("{error:#}").chars().take(2048).collect()),
        },
    };
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open("output.json")?;
    let mut output = CappedOutput {
        file,
        remaining: MAX_OUTPUT,
    };
    serde_json::to_writer(&mut output, &reply)?;
    output.flush()?;
    Ok(())
}

struct CappedOutput {
    file: File,
    remaining: usize,
}
impl Write for CappedOutput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        ensure_output_capacity(bytes.len(), self.remaining)?;
        let written = self.file.write(bytes)?;
        self.remaining -= written;
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
fn ensure_output_capacity(requested: usize, remaining: usize) -> std::io::Result<()> {
    if requested > remaining {
        Err(std::io::Error::other(
            "PDF worker JSON output limit exceeded",
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "invoked only by isolated PDF tests"]
    fn worker_fixture() -> Result<()> {
        worker()
    }

    #[test]
    fn malformed_pdf_is_an_extraction_failure_and_output_growth_is_bounded() -> Result<()> {
        let start = std::time::Instant::now();
        let error = extract(b"%PDF-1.4\nmalformed").unwrap_err();
        assert!(
            error.to_string().contains("PDF extraction refused"),
            "{error:#}"
        );
        assert!(start.elapsed() < std::time::Duration::from_secs(20));
        let mut text = BoundedText {
            bytes: Vec::new(),
            remaining: 3,
        };
        text.write_all(b"abc")?;
        assert!(text.write_all(b"d").is_err());
        assert_eq!(text.bytes, b"abc");
        Ok(())
    }

    #[test]
    fn excessive_pages_are_rejected_before_content_extraction() -> Result<()> {
        let mut document = pdf_extract::Document::with_version("1.4");
        let pages_id = document.new_object_id();
        let mut kids = Vec::new();
        for _ in 0..513 {
            let mut page = pdf_extract::Dictionary::new();
            page.set("Type", "Page");
            page.set("Parent", pages_id);
            let id = document.add_object(page);
            kids.push(pdf_extract::Object::Reference(id));
        }
        let mut pages = pdf_extract::Dictionary::new();
        pages.set("Type", "Pages");
        pages.set("Kids", kids);
        pages.set("Count", 513);
        document.objects.insert(pages_id, pages.into());
        let mut catalog = pdf_extract::Dictionary::new();
        catalog.set("Type", "Catalog");
        catalog.set("Pages", pages_id);
        let catalog_id = document.add_object(catalog);
        document.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        document.save_to(&mut bytes)?;
        let error = extract(&bytes).unwrap_err();
        assert!(error.to_string().contains("512 pages"), "{error:#}");
        Ok(())
    }

    #[test]
    fn deep_and_cyclic_page_trees_fail_within_the_worker_deadline() -> Result<()> {
        for cyclic in [false, true] {
            let mut document = pdf_extract::Document::with_version("1.4");
            let root = document.new_object_id();
            let mut current = root;
            for depth in 0..70 {
                let next = if cyclic && depth == 1 {
                    root
                } else {
                    document.new_object_id()
                };
                let mut node = pdf_extract::Dictionary::new();
                node.set("Type", "Pages");
                node.set("Count", 1);
                node.set("Kids", vec![pdf_extract::Object::Reference(next)]);
                document.objects.insert(current, node.into());
                if next == root {
                    break;
                }
                current = next;
            }
            let mut catalog = pdf_extract::Dictionary::new();
            catalog.set("Type", "Catalog");
            catalog.set("Pages", root);
            let catalog_id = document.add_object(catalog);
            document.trailer.set("Root", catalog_id);
            let mut bytes = Vec::new();
            document.save_to(&mut bytes)?;
            let start = std::time::Instant::now();
            let error = extract(&bytes).unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains(if cyclic { "cycle" } else { "64 levels" }),
                "{error:#}"
            );
            assert!(start.elapsed() < std::time::Duration::from_secs(20));
        }
        Ok(())
    }

    #[test]
    fn compressed_high_expansion_content_cannot_exceed_worker_limits() -> Result<()> {
        let mut document = pdf_extract::Document::with_version("1.4");
        let pages_id = document.new_object_id();
        let mut font = pdf_extract::Dictionary::new();
        font.set("Type", "Font");
        font.set("Subtype", "Type1");
        font.set("BaseFont", "Helvetica");
        let font_id = document.add_object(font);
        let mut content = b"BT /F1 12 Tf 10 100 Td (".to_vec();
        content.resize(content.len() + 33 * 1024 * 1024, b'x');
        content.extend_from_slice(b") Tj ET");
        let mut stream = pdf_extract::Stream::new(pdf_extract::Dictionary::new(), content);
        stream.compress()?;
        assert!(
            stream.content.len() < 128 * 1024,
            "fixture must exercise compressed expansion"
        );
        let stream_id = document.add_object(stream);
        let mut fonts = pdf_extract::Dictionary::new();
        fonts.set("F1", font_id);
        let mut resources = pdf_extract::Dictionary::new();
        resources.set("Font", fonts);
        let mut page = pdf_extract::Dictionary::new();
        page.set("Type", "Page");
        page.set("Parent", pages_id);
        page.set("Resources", resources);
        page.set("MediaBox", vec![0.into(), 0.into(), 200.into(), 200.into()]);
        page.set("Contents", stream_id);
        let page_id = document.add_object(page);
        let mut pages = pdf_extract::Dictionary::new();
        pages.set("Type", "Pages");
        pages.set("Count", 1);
        pages.set("Kids", vec![pdf_extract::Object::Reference(page_id)]);
        document.objects.insert(pages_id, pages.into());
        let mut catalog = pdf_extract::Dictionary::new();
        catalog.set("Type", "Catalog");
        catalog.set("Pages", pages_id);
        let catalog_id = document.add_object(catalog);
        document.trailer.set("Root", catalog_id);
        let mut bytes = Vec::new();
        document.save_to(&mut bytes)?;
        let start = std::time::Instant::now();
        let error = extract(&bytes).expect_err("33 MiB text must not be admitted");
        assert!(
            !format!("{error:#}").contains("start explicitly provisioned"),
            "fixture must reach the worker, rather than fail during startup: {error:#}"
        );
        assert!(
            start.elapsed() < std::time::Duration::from_secs(20),
            "high expansion worker did not stop"
        );
        Ok(())
    }
}
