// SPDX-License-Identifier: Apache-2.0
//! Full new-mapping classification; basename prefixes never authorize a new image.
use airk_process::directory::ObjectIdentity;
#[derive(Clone, Debug)]
pub(super) struct ModuleIdentity {
    pub address: usize,
    pub name: String,
    pub path: String,
    pub id: ObjectIdentity,
}
pub(super) fn allowed(
    current: &ModuleIdentity,
    baseline: &[ModuleIdentity],
    approved: &[ModuleIdentity],
) -> bool {
    approved
        .iter()
        .any(|v| v.name == current.name && v.path == current.path && v.id == current.id)
        || baseline.iter().any(|v| {
            v.address == current.address
                && v.name == current.name
                && v.path == current.path
                && v.id == current.id
        })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn row(address: usize, name: &str, path: &str, inode: u64) -> ModuleIdentity {
        ModuleIdentity {
            address,
            name: name.into(),
            path: path.into(),
            id: ObjectIdentity::Unix { device: 1, inode },
        }
    }
    #[test]
    fn unlisted_new_names_paths_and_known_name_wrong_identity_refuse() {
        let old = row(1, "existing.dll", "system/existing.dll", 1);
        let good = row(0, "cuda.dll", "app/cuda.dll", 2);
        assert!(allowed(
            &old,
            std::slice::from_ref(&old),
            std::slice::from_ref(&good)
        ));
        assert!(allowed(
            &row(2, "cuda.dll", "app/cuda.dll", 2),
            std::slice::from_ref(&old),
            std::slice::from_ref(&good)
        ));
        for bad in [
            row(3, "helper.dll", "app/helper.dll", 3),
            row(3, "renamed-core.dll", "outside/renamed-core.dll", 4),
            row(3, "cuda.dll", "outside/cuda.dll", 2),
            row(3, "cuda.dll", "app/cuda.dll", 9),
            row(8, "existing.dll", "system/existing.dll", 1),
        ] {
            assert!(!allowed(
                &bad,
                std::slice::from_ref(&old),
                std::slice::from_ref(&good)
            ));
        }
    }
}
