// SPDX-License-Identifier: Apache-2.0
//! Effective current-object ACL decisions; inheritance alone grants nothing here.
use super::*;
pub(super) fn trusted(sid: &str) -> bool {
    matches!(
        sid,
        "S-1-5-18"
            | "S-1-5-32-544"
            | "S-1-5-80-956008885-3418522649-1831038044-1853292631-2271478464"
    )
}
pub(super) fn effective_allow(header: [u8; 4]) -> Result<bool> {
    if header[1] & 0x08 != 0 {
        return Ok(false);
    } // INHERIT_ONLY, not INHERITED(0x10).
    if header[0] == 1 {
        return Ok(false);
    } // DENY never grants current-object access.
    ensure!(
        header[0] == 0 && u16::from_le_bytes([header[2], header[3]]) >= 20,
        "unsupported effective GPU custody ACE"
    );
    Ok(true)
}
pub(super) fn grant(private: bool, user: &str, who: &str, mask: u32) -> Result<()> {
    ensure!(
        (private && who == user) || trusted(who) || mask & 0x500d0156 == 0,
        "effective GPU custody grant permits foreign access/write"
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn creator_owner_raw_inherit_only_does_not_grant_current_object() {
        let ace = [
            0u8, 0x0b, 20, 0, 0, 0, 0, 0x10, 1, 1, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0,
        ];
        assert!(!effective_allow(ace[..4].try_into().unwrap()).unwrap());
        let mut applied = ace;
        applied[1] = 3;
        assert!(effective_allow(applied[..4].try_into().unwrap()).unwrap());
        assert!(
            grant(
                false,
                "S-1-5-21-1",
                "S-1-3-0",
                u32::from_le_bytes(applied[4..8].try_into().unwrap())
            )
            .is_err()
        );
    }
    #[test]
    fn inherited_effective_foreign_write_is_not_ignored_or_child_authorized() {
        assert!(effective_allow([0, 0x10, 20, 0]).unwrap());
        assert!(grant(false, "u", "foreign", 0x10000000).is_err());
        assert!(grant(false, "u", "foreign", 0x1200a9).is_ok());
        assert!(grant(true, "u", "foreign", 0x1200a9).is_ok());
        assert!(grant(true, "u", "foreign", 0x40000000).is_err());
        assert!(effective_allow([5, 0, 20, 0]).is_err());
        assert!(!effective_allow([5, 8, 20, 0]).unwrap());
    }
}
