// SPDX-License-Identifier: Apache-2.0
//! Inert metadata custody only. No code/PE/runtime activation aliases.
use super::*;
use airk_core::arbiter::resource_policy::ResourceFilePin;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataCustodyCopy {
    pub original: ResourceFilePin,
    pub copy: ResourceFilePin,
    pub bytes: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema: String,
    copies: Vec<MetadataCustodyCopy>,
}
pub fn decode_metadata_custody(raw: &[u8]) -> Result<Vec<MetadataCustodyCopy>> {
    ensure!(raw.len() <= 1 << 20, "metadata custody request cap");
    let request: Envelope = serde_json::from_slice(raw)?;
    ensure!(
        request.schema == "airk.native.cuda-metadata-custody/1",
        "unknown metadata custody input"
    );
    validate_metadata_custody(&request.copies)?;
    Ok(request.copies)
}
fn key(path: &str) -> Result<String> {
    for part in Path::new(path).components() {
        match part {
            std::path::Component::CurDir | std::path::Component::ParentDir => {
                anyhow::bail!("metadata custody dot-path alias")
            }
            std::path::Component::Normal(n) => {
                let n = n.to_str().context("metadata custody path not UTF8")?;
                ensure!(
                    !n.ends_with(['.', ' ']) && !n.contains(':'),
                    "metadata custody path alias"
                );
            }
            _ => {}
        }
    }
    let normalized = Path::new(path).components().collect::<std::path::PathBuf>();
    let normalized = normalized
        .to_str()
        .context("metadata custody normalized path not UTF8")?;
    #[cfg(windows)]
    {
        Ok(normalized.replace('/', "\\").to_lowercase())
    }
    #[cfg(not(windows))]
    {
        Ok(normalized.into())
    }
}
pub fn validate_metadata_custody(rows: &[MetadataCustodyCopy]) -> Result<()> {
    validated(rows)?;
    Ok(())
}
fn validated(rows: &[MetadataCustodyCopy]) -> Result<BTreeMap<String, MetadataCustodyCopy>> {
    ensure!(rows.len() <= 512, "metadata custody mapping count cap");
    let mut copies = BTreeMap::new();
    let mut destinations = BTreeSet::new();
    for row in rows {
        row.original.validate()?;
        row.copy.validate()?;
        ensure!(
            row.bytes > 0
                && row.bytes <= 16 << 20
                && row.original.sha256 != Sha256Digest::from_bytes([0; 32])
                && row.original.sha256 == row.copy.sha256
                && row.original.path != row.copy.path,
            "metadata custody copy identity/byte bounds differ"
        );
        ensure!(
            copies
                .insert(key(&row.original.path)?, row.clone())
                .is_none()
                && destinations.insert(key(&row.copy.path)?),
            "duplicate metadata original/copy path"
        );
    }
    ensure!(
        copies.keys().all(|p| !destinations.contains(p)),
        "metadata custody cycles/chained aliases refuse"
    );
    Ok(copies)
}
#[cfg(any(test, all(windows, target_arch = "x86_64")))]
pub(super) struct Custody {
    copies: BTreeMap<String, MetadataCustodyCopy>,
    used: std::cell::RefCell<BTreeSet<String>>,
}
#[cfg(any(test, all(windows, target_arch = "x86_64")))]
impl Custody {
    pub fn new(rows: &[MetadataCustodyCopy]) -> Result<Self> {
        let copies = validated(rows)?;
        Ok(Self {
            copies,
            used: std::cell::RefCell::new(BTreeSet::new()),
        })
    }
    pub fn resolve(
        &self,
        pin: &ResourceFilePin,
        metadata: bool,
    ) -> Result<(ResourceFilePin, Option<u64>)> {
        pin.validate()?;
        if let Some(row) = self.copies.get(&key(&pin.path)?) {
            ensure!(
                metadata && &row.original == pin,
                "unknown/mismatched metadata pin or forbidden code/authenticate redirect"
            );
            Ok((row.copy.clone(), Some(row.bytes)))
        } else {
            Ok((pin.clone(), None))
        }
    }
    pub fn mapped(&self, path: &str) -> Result<bool> {
        Ok(self.copies.contains_key(&key(path)?))
    }
    pub fn consumed(&self, pin: &ResourceFilePin) -> Result<()> {
        let row = self
            .copies
            .get(&key(&pin.path)?)
            .context("unknown metadata custody consumption")?;
        ensure!(
            &row.original == pin,
            "metadata custody original pin changed"
        );
        self.used.borrow_mut().insert(key(&pin.path)?);
        Ok(())
    }
    pub fn recheck(&self) -> Result<()> {
        ensure!(
            self.used.borrow().iter().eq(self.copies.keys()),
            "unused/unknown metadata custody mapping"
        );
        Ok(())
    }
}
#[cfg(test)]
mod tests;

/// Inert historical build-tool evidence only; never an executable activation alias.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalToolCustodyCopy {
    pub tool: String,
    pub original: ResourceFilePin,
    pub copy: ResourceFilePin,
    pub bytes: u64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoricalEnvelope {
    schema: String,
    copies: Vec<HistoricalToolCustodyCopy>,
}
pub fn decode_historical_tool_custody(raw: &[u8]) -> Result<Vec<HistoricalToolCustodyCopy>> {
    ensure!(raw.len() <= 1 << 20, "historical tool evidence input cap");
    let request: HistoricalEnvelope = serde_json::from_slice(raw)?;
    ensure!(
        request.schema == "airk.native.cuda-historical-tool-custody/1",
        "unknown historical tool evidence input"
    );
    validate_historical_tool_custody(&request.copies)?;
    Ok(request.copies)
}
fn historical_rows(rows: &[HistoricalToolCustodyCopy]) -> Result<Vec<MetadataCustodyCopy>> {
    ensure!(rows.len() <= 2, "only two reviewed historical tool aliases");
    let mut names = BTreeSet::new();
    let mut result = Vec::new();
    for row in rows {
        let (path, bytes, sha) = match row.tool.as_str() {
            "cmake" => (
                r"D:\code\Github\airk\artifacts\advanced-split\ort130-x64-owned-r3\tools\cmake\bin\cmake.exe",
                12029392,
                "3fe22eb02e1c6184ec207366ed21a6f2f9c3828c1e9f3324546befadfb362be3",
            ),
            "ninja" => (
                r"D:\code\Github\airk\artifacts\advanced-split\ort130-x64-owned-r3\tools\ninja-build\ninja.exe",
                600576,
                "edccb4c55fe69db51ed8ae286594660cf8e646abead40ff3c39f28c636489653",
            ),
            _ => anyhow::bail!("historical tool alias not reviewed"),
        };
        ensure!(
            names.insert(row.tool.clone())
                && row.original.path == path
                && row.original.sha256.to_string() == sha
                && row.bytes == bytes,
            "historical tool alias differs from actual frozen R12 seal"
        );
        result.push(MetadataCustodyCopy {
            original: row.original.clone(),
            copy: row.copy.clone(),
            bytes: row.bytes,
        });
    }
    validated(&result)?;
    Ok(result)
}
pub fn validate_historical_tool_custody(rows: &[HistoricalToolCustodyCopy]) -> Result<()> {
    historical_rows(rows)?;
    Ok(())
}
pub fn validate_custody_compatibility(
    metadata: &[MetadataCustodyCopy],
    historical: &[HistoricalToolCustodyCopy],
) -> Result<()> {
    let mut all = metadata.to_vec();
    all.extend(historical_rows(historical)?);
    validated(&all)?;
    Ok(())
}
#[cfg(any(test, all(windows, target_arch = "x86_64")))]
pub(super) struct HistoricalTools {
    book: Custody,
    rows: BTreeMap<String, HistoricalToolCustodyCopy>,
}
#[cfg(any(test, all(windows, target_arch = "x86_64")))]
impl HistoricalTools {
    pub fn new(rows: &[HistoricalToolCustodyCopy]) -> Result<Self> {
        Ok(Self {
            book: Custody::new(&historical_rows(rows)?)?,
            rows: rows
                .iter()
                .map(|r| Ok((key(&r.original.path)?, r.clone())))
                .collect::<Result<_>>()?,
        })
    }
    pub fn mapped(&self, path: &str) -> Result<bool> {
        self.book.mapped(path)
    }
    pub fn resolve(
        &self,
        tool: &str,
        pin: &ResourceFilePin,
        bytes: u64,
        max: u64,
    ) -> Result<(ResourceFilePin, Option<u64>)> {
        ensure!(
            matches!(
                tool,
                "cmake"
                    | "ninja"
                    | "python"
                    | "compiler"
                    | "patch"
                    | "vcvars"
                    | "archiver"
                    | "nvcc"
            ),
            "unknown historical tool field"
        );
        if let Some(row) = self.rows.get(&key(&pin.path)?) {
            ensure!(
                row.tool == tool && row.bytes == bytes && bytes <= max,
                "historical tool use/size differs from sealed role"
            );
            self.book.resolve(pin, true)
        } else {
            Ok((pin.clone(), None))
        }
    }
    pub fn consumed(&self, pin: &ResourceFilePin) -> Result<()> {
        self.book.consumed(pin)
    }
    pub fn recheck(&self) -> Result<()> {
        self.book.recheck()
    }
}

/// Metadata data only; the actual Source owner still checks protected custody.
pub fn encode_metadata_custody(copies: &[MetadataCustodyCopy]) -> Result<Vec<u8>> {
    validate_metadata_custody(copies)?;
    let raw = serde_json::to_vec_pretty(&Envelope {
        schema: "airk.native.cuda-metadata-custody/1".into(),
        copies: copies.to_vec(),
    })?;
    ensure!(raw.len() <= 1 << 20, "metadata custody request cap");
    Ok(raw)
}
/// Historical evidence data only; never an executable or activation capability.
pub fn encode_historical_tool_custody(copies: &[HistoricalToolCustodyCopy]) -> Result<Vec<u8>> {
    validate_historical_tool_custody(copies)?;
    let raw = serde_json::to_vec_pretty(&HistoricalEnvelope {
        schema: "airk.native.cuda-historical-tool-custody/1".into(),
        copies: copies.to_vec(),
    })?;
    ensure!(raw.len() <= 1 << 20, "historical tool evidence input cap");
    Ok(raw)
}
