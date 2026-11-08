// SPDX-License-Identifier: Apache-2.0
use super::*;
struct Borrowed([u8; 16]);
impl ModelAssetSource for Borrowed {
    fn read_asset(&self, _name: ModelAssetName, _maximum: u64) -> Result<Vec<u8>> {
        anyhow::bail!("graph copy is forbidden in this fixture")
    }
    fn borrowed_asset(&self, _name: ModelAssetName, _maximum: u64) -> Result<Option<&[u8]>> {
        Ok(Some(&self.0))
    }
}
#[test]
fn borrowed_graph_stays_bounded_and_digest_checked_without_fallback_copy() -> Result<()> {
    let source = Borrowed([3; 16]);
    let digest = format!("{:x}", Sha256::digest(source.0));
    let bytes = verified_source_bytes(&source, ModelAssetName::ModelOnnx, 16, &digest)?;
    assert!(matches!(bytes, std::borrow::Cow::Borrowed(_)));
    assert!(verified_source_bytes(&source, ModelAssetName::ModelOnnx, 15, &digest).is_err());
    assert!(
        verified_source_bytes(&Borrowed([4; 16]), ModelAssetName::ModelOnnx, 16, &digest).is_err()
    );
    Ok(())
}
