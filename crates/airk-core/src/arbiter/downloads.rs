// SPDX-License-Identifier: Apache-2.0
//! Compiled immutable acquisition locations; never source or execution authority.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DownloadOrigin {
    GitHubRelease,
    HuggingFace,
}

fn safe_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 192
        && value != "."
        && value != ".."
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}
/// Validate the exact initial source. Redirects remain a separate transport fence.
pub fn download_origin(url: &str) -> Result<DownloadOrigin> {
    ensure!(url.len() <= 2048, "Arbiter source URL byte limit");
    if let Some(path) = url.strip_prefix("https://github.com/iokaio/airk/releases/download/") {
        let pieces: Vec<_> = path.split('/').collect();
        ensure!(
            pieces.len() == 2 && pieces.iter().all(|piece| safe_component(piece)),
            "invalid Arbiter release asset URL"
        );
        return Ok(DownloadOrigin::GitHubRelease);
    }
    let path = url
        .strip_prefix("https://huggingface.co/")
        .ok_or_else(|| anyhow::anyhow!("unapproved Arbiter asset source"))?;
    let pieces: Vec<_> = path.split('/').collect();
    ensure!(
        pieces.len() == 5 && pieces[2] == "resolve",
        "invalid immutable Arbiter model URL"
    );
    ensure!(
        matches!(
            (pieces[0], pieces[1]),
            (
                "Qwen",
                "Qwen3-Reranker-0.6B" | "Qwen3-0.6B-Base" | "Qwen3-1.7B" | "Qwen3-4B"
            ) | ("onnx-community", "Qwen3-Reranker-0.6B-ONNX")
                | ("iokaio", "airk-arbiter-models")
        ),
        "unreviewed Arbiter model repository"
    );
    ensure!(
        pieces[3].len() == 40
            && pieces[3]
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            && safe_component(pieces[4]),
        "invalid immutable Arbiter model revision or file"
    );
    Ok(DownloadOrigin::HuggingFace)
}
impl AssetPart {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.bytes > 0 && self.bytes <= 1_u64 << 30 && self.backup_urls.len() <= 2,
            "invalid Arbiter asset part"
        );
        let mut seen = BTreeSet::new();
        for url in std::iter::once(&self.url).chain(self.backup_urls.iter()) {
            ensure!(seen.insert(url), "duplicate Arbiter part source");
            download_origin(url)?;
        }
        Ok(())
    }
    /// Zero is primary; later ordinals inherit this SAME part's bytes and SHA.
    pub fn source_url(&self, ordinal: usize) -> Option<&str> {
        if ordinal == 0 {
            Some(&self.url)
        } else {
            self.backup_urls.get(ordinal - 1).map(String::as_str)
        }
    }
}
impl AssetDownload {
    /// A source choice always has a complete ordered roster for the whole asset.
    pub fn source_count(&self) -> Result<usize> {
        ensure!(
            !self.parts.is_empty() && self.parts.len() <= 16,
            "Arbiter asset part limit"
        );
        let backups = self.parts[0].backup_urls.len();
        let mut seen = BTreeSet::new();
        for part in &self.parts {
            part.validate()?;
            ensure!(
                part.backup_urls.len() == backups,
                "incomplete Arbiter whole-asset backup roster"
            );
            for url in std::iter::once(&part.url).chain(part.backup_urls.iter()) {
                ensure!(seen.insert(url), "duplicate Arbiter asset part location");
            }
        }
        Ok(1 + backups)
    }
}
#[cfg(test)]
mod tests;
