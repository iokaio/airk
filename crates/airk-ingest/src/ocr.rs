// SPDX-License-Identifier: Apache-2.0
//! Deliberate local OCR of bounded selected pages, preserving original source hashes.
use crate::{
    provider_process::{remaining, run},
    providers::ProviderCommand,
    semantic,
};
use airk_core::{Artifact, DocumentEvidence, DocumentLocation};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OcrRequest {
    pub tesseract: ProviderCommand,
    pub pdf_renderer: Option<ProviderCommand>,
    pub tessdata_dir: PathBuf,
    pub language: String,
    pub first_page: usize,
    pub last_page: usize,
    pub dpi: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OcrProvenance {
    pub schema: u32,
    pub source_sha256: String,
    pub tesseract_sha256: String,
    pub renderer_sha256: Option<String>,
    pub traineddata_sha256: BTreeMap<String, String>,
    pub language: String,
    pub first_page: usize,
    pub last_page: usize,
    pub pages_with_text: usize,
    pub dpi: u32,
    pub coverage: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OcrOutput {
    pub artifact: Artifact,
    pub provenance: OcrProvenance,
}

fn program_digest(program: &ProviderCommand) -> Result<String> {
    ensure!(
        program.args.is_empty(),
        "OCR programs accept no extra arguments; AIRK controls bounded invocation"
    );
    Ok(semantic::digest(&semantic::read_bounded(
        &program.validate()?,
        268_435_456,
    )?))
}

fn models(request: &OcrRequest) -> Result<BTreeMap<String, String>> {
    ensure!(
        request.tessdata_dir.is_absolute(),
        "Tesseract model directory must be explicit and absolute"
    );
    let root = request.tessdata_dir.canonicalize()?;
    ensure!(root.is_dir(), "Tesseract model directory missing");
    let names: Vec<_> = request.language.split('+').collect();
    ensure!(
        !names.is_empty()
            && names.len() <= 8
            && names.iter().all(|name| !name.is_empty()
                && name.len() <= 32
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')),
        "OCR language must contain 1..8 explicit local model names"
    );
    let mut hashes = BTreeMap::new();
    let mut total = 0;
    for name in names {
        let path = root.join(format!("{name}.traineddata"));
        ensure!(
            !path.symlink_metadata()?.file_type().is_symlink(),
            "linked OCR model rejected"
        );
        let bytes = semantic::read_bounded(&path, 67_108_864)?;
        total += bytes.len();
        ensure!(total <= 134_217_728, "OCR language models exceed 128 MiB");
        ensure!(
            hashes
                .insert(name.into(), semantic::digest(&bytes))
                .is_none(),
            "duplicate OCR model name"
        );
    }
    Ok(hashes)
}

fn dimensions(bytes: &[u8], extension: &str) -> Result<(usize, usize)> {
    let (width, height) = match extension {
        "png" => {
            ensure!(
                bytes.starts_with(b"\x89PNG\r\n\x1a\n")
                    && bytes.get(12..16) == Some(b"IHDR")
                    && bytes.len() >= 24,
                "invalid PNG header"
            );
            (
                u32::from_be_bytes(bytes[16..20].try_into()?) as usize,
                u32::from_be_bytes(bytes[20..24].try_into()?) as usize,
            )
        }
        "bmp" => {
            ensure!(
                bytes.starts_with(b"BM")
                    && bytes.len() >= 26
                    && u32::from_le_bytes(bytes[14..18].try_into()?) >= 40,
                "unsupported BMP header"
            );
            let width = i32::from_le_bytes(bytes[18..22].try_into()?);
            let height = i32::from_le_bytes(bytes[22..26].try_into()?);
            ensure!(width > 0 && height != i32::MIN, "invalid BMP dimensions");
            (width as usize, height.unsigned_abs() as usize)
        }
        "jpg" | "jpeg" => {
            ensure!(bytes.starts_with(&[0xff, 0xd8]), "invalid JPEG header");
            let mut cursor = 2;
            let mut size = None;
            while cursor + 4 <= bytes.len() {
                ensure!(bytes[cursor] == 0xff, "invalid JPEG segment");
                cursor += 1;
                while bytes.get(cursor) == Some(&0xff) {
                    cursor += 1;
                }
                let marker = *bytes.get(cursor).context("truncated JPEG marker")?;
                cursor += 1;
                if marker == 0xda || marker == 0xd9 {
                    break;
                }
                let length = u16::from_be_bytes(
                    bytes
                        .get(cursor..cursor + 2)
                        .context("truncated JPEG segment")?
                        .try_into()?,
                ) as usize;
                ensure!(
                    length >= 2 && cursor + length <= bytes.len(),
                    "invalid JPEG segment length"
                );
                if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
                    ensure!(length >= 8, "invalid JPEG frame");
                    size = Some((
                        u16::from_be_bytes(bytes[cursor + 5..cursor + 7].try_into()?) as usize,
                        u16::from_be_bytes(bytes[cursor + 3..cursor + 5].try_into()?) as usize,
                    ));
                    break;
                }
                cursor += length;
            }
            size.context("JPEG has no supported dimension frame")?
        }
        _ => anyhow::bail!("OCR accepts PDF or bounded single PNG/JPEG/BMP images"),
    };
    ensure!(
        width > 0
            && height > 0
            && width <= 10_000
            && height <= 10_000
            && width
                .checked_mul(height)
                .is_some_and(|pixels| pixels <= 16_000_000),
        "OCR image exceeds dimension/pixel budget"
    );
    Ok((width, height))
}

fn bounded_command(command: &ProviderCommand, deadline: Instant) -> Result<ProviderCommand> {
    let mut bounded = command.clone();
    bounded.timeout_ms = u64::try_from(remaining(deadline)?.as_millis())?.min(command.timeout_ms);
    ensure!(bounded.timeout_ms >= 100, "OCR total time budget exhausted");
    Ok(bounded)
}

/// Explicit OCR, never called automatically by native discovery. PDF page selection
/// is deliberate and may cover only part of a document. Poppler must be pdftoppm.
pub fn extract(path: &str, bytes: &[u8], request: &OcrRequest) -> Result<OcrOutput> {
    semantic::validate_path(path)?;
    ensure!(
        bytes.len() <= crate::MAX_FILE_BYTES,
        "OCR source exceeds the file byte limit"
    );
    ensure!(
        request.first_page > 0
            && request.last_page >= request.first_page
            && request.last_page <= 512
            && request.last_page - request.first_page < 32,
        "OCR selects at most 32 original pages within 1..512"
    );
    ensure!((72..=300).contains(&request.dpi), "OCR DPI must be 72..300");
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let is_pdf = extension == "pdf";
    ensure!(
        is_pdf || (request.first_page == 1 && request.last_page == 1),
        "raster OCR has exactly one original page"
    );
    if !is_pdf {
        dimensions(bytes, &extension)?;
    }
    let tesseract_sha256 = program_digest(&request.tesseract)?;
    let renderer_sha256 = if is_pdf {
        Some(program_digest(request.pdf_renderer.as_ref().context(
            "PDF OCR requires an explicit pdftoppm executable",
        )?)?)
    } else {
        None
    };
    let traineddata_sha256 = models(request)?;
    let deadline = Instant::now() + Duration::from_millis(request.tesseract.timeout_ms);
    let temporary = tempfile::Builder::new().prefix("airk-ocr-").tempdir()?;
    let source = temporary.path().join(format!("source.{extension}"));
    std::fs::write(&source, bytes)?;
    let mut pages = Vec::new();
    let mut total = 0;
    for page in request.first_page..=request.last_page {
        let image = if is_pdf {
            let renderer = bounded_command(
                request
                    .pdf_renderer
                    .as_ref()
                    .context("PDF renderer missing")?,
                deadline,
            )?;
            let args = vec![
                "-f".into(),
                page.to_string(),
                "-l".into(),
                page.to_string(),
                "-singlefile".into(),
                "-r".into(),
                request.dpi.to_string(),
                "-scale-to".into(),
                "2400".into(),
                "-png".into(),
                source.to_string_lossy().into_owned(),
            ];
            let png = run(&renderer, temporary.path(), &args, 16_777_216)?;
            dimensions(&png, "png")?;
            let image = temporary.path().join("page.png");
            std::fs::write(&image, png)?;
            image
        } else {
            source.clone()
        };
        let args = vec![
            image.to_string_lossy().into_owned(),
            "stdout".into(),
            "--tessdata-dir".into(),
            request.tessdata_dir.to_string_lossy().into_owned(),
            "-l".into(),
            request.language.clone(),
            "--psm".into(),
            "3".into(),
        ];
        let output = run(
            &bounded_command(&request.tesseract, deadline)?,
            temporary.path(),
            &args,
            1_048_576,
        )?;
        let text = String::from_utf8(output).context("OCR output is not UTF-8")?;
        ensure!(!text.contains('\0'), "OCR emitted NUL text");
        total += text.len();
        ensure!(
            total <= crate::documents::MAX_EXTRACTED,
            "OCR extracted text exceeds 4 MiB"
        );
        pages.push((text, DocumentLocation::Page { number: page }));
    }
    ensure!(
        program_digest(&request.tesseract)? == tesseract_sha256
            && models(request)? == traineddata_sha256,
        "OCR executable/model changed during extraction"
    );
    if let Some(expected) = &renderer_sha256 {
        ensure!(
            Some(program_digest(
                request
                    .pdf_renderer
                    .as_ref()
                    .context("PDF renderer missing")?
            )?)
            .as_ref()
                == Some(expected),
            "PDF renderer changed during extraction"
        );
    }
    let provenance = OcrProvenance { schema: 1, source_sha256: semantic::digest(bytes), tesseract_sha256, renderer_sha256, traineddata_sha256, language: request.language.clone(), first_page: request.first_page, last_page: request.last_page, pages_with_text: pages.iter().filter(|(text, _)| !text.trim().is_empty()).count(), dpi: request.dpi, coverage: "OCR estimates from explicitly selected original pages; recognition/layout/reading order may be wrong; unselected pages are unknown".into() };
    let serialized = serde_json::to_string(&provenance)?;
    let method = format!(
        "local-tesseract-ocr/1:{}",
        semantic::digest(serialized.as_bytes())
    );
    let mut extracted = crate::documents::from_blocks(
        "ocr",
        &method,
        pages,
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeSet::new(),
    )?;
    for fact in &mut extracted.facts {
        let details = fact.document.get_or_insert_with(DocumentEvidence::default);
        details.parser_version = "document-cues/1+local-tesseract-ocr/1".into();
        details.basis = format!(
            "ocr-derived/{}",
            if details.basis.is_empty() {
                "page-text"
            } else {
                &details.basis
            }
        );
    }
    Ok(OcrOutput {
        artifact: Artifact {
            path: path.into(),
            content_hash: provenance.source_sha256.clone(),
            bytes: bytes.len(),
            chunks: extracted.chunks,
            language: "ocr".into(),
            extraction_method: method,
            parse_status: serialized,
            facts: extracted.facts,
            retained: None,
        },
        provenance,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_dimensions_and_page_models_fail_before_process_start() -> Result<()> {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(100u32.to_be_bytes());
        png.extend(40u32.to_be_bytes());
        assert_eq!(dimensions(&png, "png")?, (100, 40));
        png[16..20].copy_from_slice(&100_000u32.to_be_bytes());
        assert!(dimensions(&png, "png").is_err());
        assert!(dimensions(b"GIF89a", "gif").is_err());
        assert!(dimensions(&[0xff, 0xd8, 0xff, 0xc0, 0xff, 0xff], "jpeg").is_err());
        Ok(())
    }

    fn original_bitmap() -> Vec<u8> {
        // Original tiny block-letter fixture, rendered without image/font dependencies.
        let letters: [[u8; 7]; 11] = [
            [14, 17, 17, 31, 17, 17, 17],
            [31, 4, 4, 4, 4, 4, 31],
            [30, 17, 17, 30, 20, 18, 17],
            [17, 18, 20, 24, 20, 18, 17],
            [0; 7],
            [14, 17, 1, 2, 4, 8, 31],
            [14, 17, 17, 17, 17, 17, 14],
            [14, 17, 17, 17, 17, 17, 14],
            [0; 7],
            [17, 27, 21, 21, 17, 17, 17],
            [15, 16, 16, 14, 1, 1, 30],
        ];
        let width = 850usize;
        let height = 170usize;
        let row = (width * 3 + 3) & !3;
        let mut bmp = vec![255; 54 + row * height];
        bmp[..54].fill(0);
        bmp[..2].copy_from_slice(b"BM");
        let length = bmp.len() as u32;
        bmp[2..6].copy_from_slice(&length.to_le_bytes());
        bmp[10..14].copy_from_slice(&54u32.to_le_bytes());
        bmp[14..18].copy_from_slice(&40u32.to_le_bytes());
        bmp[18..22].copy_from_slice(&(width as i32).to_le_bytes());
        bmp[22..26].copy_from_slice(&(height as i32).to_le_bytes());
        bmp[26..28].copy_from_slice(&1u16.to_le_bytes());
        bmp[28..30].copy_from_slice(&24u16.to_le_bytes());
        for (index, glyph) in letters.iter().enumerate() {
            for (y, bits) in glyph.iter().enumerate() {
                for x in 0..5 {
                    if bits & (1 << (4 - x)) == 0 {
                        continue;
                    }
                    for dy in 0..10 {
                        for dx in 0..10 {
                            let px = 40 + index * 70 + x * 10 + dx;
                            let py = 45 + y * 10 + dy;
                            let offset = 54 + (height - 1 - py) * row + px * 3;
                            bmp[offset..offset + 3].fill(0);
                        }
                    }
                }
            }
        }
        bmp
    }

    #[test]
    fn provisioned_tesseract_preserves_original_page_and_model_provenance() -> Result<()> {
        let (Some(program), Some(tessdata)) = (
            std::env::var_os("AIRK_TEST_TESSERACT"),
            std::env::var_os("AIRK_TEST_TESSDATA"),
        ) else {
            eprintln!("AIRK_TEST_TESSERACT/TESSDATA absent: external OCR qualification not run");
            return Ok(());
        };
        let bytes = original_bitmap();
        let output = extract(
            "fixture.bmp",
            &bytes,
            &OcrRequest {
                tesseract: ProviderCommand {
                    program: program.into(),
                    args: Vec::new(),
                    timeout_ms: 30_000,
                },
                pdf_renderer: None,
                tessdata_dir: tessdata.into(),
                language: "eng".into(),
                first_page: 1,
                last_page: 1,
                dpi: 150,
            },
        )?;
        assert!(
            output
                .artifact
                .chunks
                .iter()
                .any(|chunk| chunk.text.contains("200")),
            "{:?}",
            output.artifact.chunks
        );
        assert!(
            output
                .artifact
                .chunks
                .iter()
                .all(|chunk| chunk.location == DocumentLocation::Page { number: 1 })
        );
        assert_eq!(output.artifact.content_hash, semantic::digest(&bytes));
        assert_eq!(output.provenance.traineddata_sha256["eng"].len(), 64);
        assert!(
            output
                .artifact
                .parse_status
                .contains(&output.provenance.traineddata_sha256["eng"])
        );
        Ok(())
    }
}
